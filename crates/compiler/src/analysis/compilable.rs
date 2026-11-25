//! The compilable-program rule: whether a loaded program may be compiled yet.
//!
//! A program is compilable when its entry startup and every body reachable
//! from it are `Checked`, although the whole graph has no complete state, and
//! every construct in those bodies is one the backend lowers
//! ([`capability::Shape`]). This module is the one place the backend and the
//! CLI ask; neither decides compilability itself. [`ProgramAnalysis`] is the
//! entry point for a typed report with or without a receipt.
//!
//! The rule reads finished reports, but it allocates in proportion to them,
//! so it is metered under `docs/agents/compiler-metering.md`: it takes its own
//! budget from the analysis limits, reserves before it allocates and charges
//! each probe, and a limit it reaches is [`Blocker::ResourceLimit`] with its
//! own statistics. A report whose completion is `ResourceLimit` is never read
//! past that completion (ADR 0029).
//!
//! `Checked` certifies ordinary MIR, not that delivery has been resolved
//! (`RootReport::completion`), and the graph's open obligations are not
//! consulted. The rule is for ordinary execution: it reads the entry's
//! startup, which test mode does not run.
pub mod capability;
pub mod messages;

use super::declarations::Budget;
use super::{
    AnalysisLimits, Statistics,
    ownership::{
        Completion, Dependency, DependencyContract, Finding, GraphCompletion, GraphReport, Mir, RootReport,
        analyze_ownership,
    },
    scalars::{BodyOwner, Site},
    typed::{CoreBlocker, TypedCompletion, TypedReport},
};
use crate::loader::{DeclarationId, Graph, Origin, keyed::PathMap};
use capability::{Facts, Shape, Shapes, first_unadmitted};
use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsStr;

type Work<T> = Result<T, ()>;

/// The diagnostic code for valid source whose shape the compiler cannot compile
/// yet. It is not a source error: report it for [`Blocker::Incomplete`] under
/// driver status 1, with the blocking span.
pub const UNSUPPORTED_SHAPE: &str = "R0605";

/// The answer to "may this program be compiled?".
#[derive(Debug)]
pub enum Compilability<'r, 'g> {
    Compilable(CompilableProgram<'r, 'g>),
    Blocked(Blocker<'r, 'g>),
}

/// Why a program is not compilable. Each variant is reported as itself: a
/// blocker never stands in for another.
#[derive(Debug)]
pub enum Blocker<'r, 'g> {
    /// Valid source whose reachable shape the compiler cannot compile yet, at
    /// the first blocking span: a body that is not `Checked`, an edge the walk
    /// does not follow, or a construct the backend does not lower
    /// ([`capability::Shape`]). Report it under [`UNSUPPORTED_SHAPE`].
    Incomplete { site: Site<'g> },
    /// The source carries findings, in a reachable body or not: unexecuted
    /// code is still checked. Every finding, in source order.
    SourceRejected { findings: Vec<&'r Finding<'g>> },
    /// A cap ran out, in the analysis or in this rule. Nothing else is read
    /// from the report.
    ResourceLimit { statistics: Statistics },
    /// Typed analysis rejected the source, so ownership analysis did not run.
    /// `findings` are the located typed findings in source order; it is empty
    /// when the rejection comes from a finding the typed report does not
    /// locate.
    TypedRejected { findings: Vec<TypedFinding<'g>> },
    /// Typed analysis could not finish and published no site to name. Report
    /// it as an incomplete analysis, not as [`UNSUPPORTED_SHAPE`].
    TypedUnlocated { core_blockers: Vec<CoreBlocker> },
}

/// A finding of typed analysis: its code, where it is and what it is.
#[derive(Debug)]
pub struct TypedFinding<'g> {
    pub code: Option<&'static str>,
    pub site: Site<'g>,
    pub description: &'static str,
}

/// A body that is `Checked`, with its ordinary MIR.
#[derive(Debug)]
pub struct CheckedBody<'r, 'g> {
    root: &'r RootReport<'g>,
    mir: &'r Mir<'g>,
}

impl<'r, 'g> CheckedBody<'r, 'g> {
    pub fn owner(&self) -> BodyOwner<'g> {
        self.root.owner()
    }
    pub fn site(&self) -> Site<'g> {
        self.root.site()
    }
    pub fn mir(&self) -> &'r Mir<'g> {
        self.mir
    }
}

/// The entry startup and every body reachable from it, all `Checked` and all
/// built from shapes the backend lowers. Reachable bodies appear once, in
/// breadth-first discovery order from the startup, following each body's
/// dependencies in report order.
#[derive(Debug)]
pub struct CompilableProgram<'r, 'g> {
    startup: CheckedBody<'r, 'g>,
    functions: Vec<CheckedBody<'r, 'g>>,
    shapes: BTreeSet<Shape>,
    statistics: Statistics,
}

impl<'r, 'g> CompilableProgram<'r, 'g> {
    pub fn startup(&self) -> &CheckedBody<'r, 'g> {
        &self.startup
    }
    /// Every function or implementation reachable from the startup, excluding
    /// the startup itself.
    pub fn functions(&self) -> &[CheckedBody<'r, 'g>] {
        &self.functions
    }
    /// Every shape the program uses, each once.
    pub fn shapes(&self) -> &BTreeSet<Shape> {
        &self.shapes
    }
    /// What the rule itself spent deciding, apart from the analysis it read.
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
}

/// The front end of one loaded program, read by the compilable-program rule:
/// the typed report and, when typed analysis granted a receipt, the ownership
/// graph built from it.
pub struct ProgramAnalysis<'t, 'g> {
    graph: &'g Graph,
    typed: &'t TypedReport<'g>,
    ownership: Option<GraphReport<'g>>,
    limits: AnalysisLimits,
}

impl<'t, 'g> ProgramAnalysis<'t, 'g> {
    /// Run ownership analysis on a typed report that has a receipt. `graph` is
    /// the loaded graph `typed` was built from. The rule that reads the
    /// reports takes the same `limits` as its own budget.
    pub fn new(graph: &'g Graph, typed: &'t TypedReport<'g>, limits: AnalysisLimits) -> Self {
        let ownership = typed
            .receipt()
            .map(|receipt| analyze_ownership(&receipt, limits));
        Self {
            graph,
            typed,
            ownership,
            limits,
        }
    }

    /// Decide compilability once for the whole front end. A typed report with
    /// no receipt blocks without reading ownership: `ResourceLimit` as itself,
    /// `SourceRejected` with its located findings, and `Incomplete` at the
    /// earliest site the typed report publishes (an unsupported body
    /// obligation, a pending Result use or a conditional finding). With a
    /// receipt, the result is [`compilable_program`] on the ownership report.
    pub fn compilability(&self) -> Compilability<'_, 'g> {
        match &self.ownership {
            Some(report) => compilable_program(self.graph, self.typed, report, self.limits),
            None => {
                let mut budget = budget(self.limits);
                match typed_blocker(self.typed, &mut budget) {
                    Ok(blocker) => Compilability::Blocked(blocker),
                    Err(()) => spent(&budget),
                }
            }
        }
    }
}

fn budget(limits: AnalysisLimits) -> Budget {
    Budget {
        limits,
        statistics: Statistics::default(),
    }
}

fn spent<'r, 'g>(budget: &Budget) -> Compilability<'r, 'g> {
    Compilability::Blocked(Blocker::ResourceLimit {
        statistics: budget.statistics,
    })
}

fn typed_blocker<'r, 'g>(typed: &TypedReport<'g>, budget: &mut Budget) -> Work<Blocker<'r, 'g>> {
    Ok(match typed.completion() {
        TypedCompletion::ResourceLimit => Blocker::ResourceLimit {
            statistics: typed.statistics(),
        },
        TypedCompletion::SourceRejected => Blocker::TypedRejected {
            findings: typed_findings(typed, budget)?,
        },
        TypedCompletion::Incomplete => match first_blocking_site(typed, budget)? {
            Some(site) => Blocker::Incomplete { site },
            None => {
                budget.reserve(typed.core_blockers().len())?;
                Blocker::TypedUnlocated {
                    core_blockers: typed.core_blockers().to_vec(),
                }
            }
        },
        TypedCompletion::Checked => unreachable!("a checked typed report grants a receipt"),
    })
}

type SiteOrder<'a> = (Origin, &'a String, usize, usize);

fn site_order<'a>(site: &Site<'a>) -> SiteOrder<'a> {
    (
        site.module.origin,
        &site.module.path,
        site.span.start(),
        site.span.end(),
    )
}

/// Charge for sorting `len` items by a key that is compared in place.
fn charge_sort(len: usize, budget: &mut Budget) -> Work<()> {
    budget.step(len.saturating_mul(len.max(1).ilog2() as usize + 1))
}

fn first_blocking_site<'g>(
    typed: &TypedReport<'g>,
    budget: &mut Budget,
) -> Work<Option<Site<'g>>> {
    let bodies = typed.bodies();
    let obligations = bodies.obligations().iter().map(|o| o.site);
    let conditional = bodies.conditional_findings().iter().map(|c| c.finding.site);
    let pending = typed
        .result_use()
        .into_iter()
        .flat_map(|uses| uses.pending())
        .map(|pending| pending.site);
    let mut first: Option<Site<'g>> = None;
    for site in obligations.chain(conditional).chain(pending) {
        budget.step(1)?;
        if first.is_none_or(|known| site_order(&site) < site_order(&known)) {
            first = Some(site);
        }
    }
    Ok(first)
}

fn typed_findings<'g>(
    typed: &TypedReport<'g>,
    budget: &mut Budget,
) -> Work<Vec<TypedFinding<'g>>> {
    let bodies = typed.bodies().findings().iter().map(|finding| TypedFinding {
        code: finding.kind.code(),
        site: finding.site,
        description: messages::scalar(finding.kind),
    });
    let interfaces = typed
        .public_interface_findings()
        .iter()
        .map(|finding| TypedFinding {
            code: Some(finding.code()),
            site: finding.site,
            description: messages::interface(finding.kind),
        });
    let discards = typed
        .result_use()
        .into_iter()
        .flat_map(|uses| uses.discards())
        .map(|discard| TypedFinding {
            code: Some(discard.code()),
            site: discard.discard,
            description: messages::discard(discard.kind),
        });
    let mut findings = Vec::new();
    for finding in bodies.chain(interfaces).chain(discards) {
        budget.reserve(1)?;
        findings.push(finding);
    }
    charge_sort(findings.len(), budget)?;
    findings.sort_by(|a, b| site_order(&a.site).cmp(&site_order(&b.site)));
    Ok(findings)
}

#[derive(Clone, Copy)]
enum Node {
    Source(usize),
    Disposer(usize),
}

/// The body an edge reaches, when the walk follows its kind.
enum Follow {
    /// The edge reaches an active disposer's frame.
    Disposer,
    /// The edge reaches a source root, which the walk checks.
    Body,
}

/// How the walk treats a dependency edge: the kind of body it follows, or the
/// site at which it blocks. Every kind is decided here, so a new kind fails to
/// compile until it is. A kind the walk does not follow blocks at the edge's
/// site: nothing checks what it reaches, and a program is never compilable
/// over a body nobody checked.
fn follow<'g>(dependency: &Dependency<'g>) -> Result<Follow, Site<'g>> {
    match dependency.contract {
        DependencyContract::ActiveDisposer => Ok(Follow::Disposer),
        DependencyContract::OrdinaryCall
        | DependencyContract::CallableTransfer(_)
        | DependencyContract::DeclaredSignature
        | DependencyContract::DeclaredOnceInvocation { .. }
        | DependencyContract::DeclaredSharedInvocation { .. } => Ok(Follow::Body),
        DependencyContract::SynchronousListArrows { .. }
        | DependencyContract::ScopedJoin { .. }
        | DependencyContract::BuiltinDispose
        | DependencyContract::ProtectedRelease { .. }
        | DependencyContract::ListI32Disposal { .. }
        | DependencyContract::ListI32Iteration { .. }
        | DependencyContract::ListTextCollection { .. }
        | DependencyContract::ListTextElementDisposal { .. }
        | DependencyContract::ListOptionTextCollection { .. }
        | DependencyContract::OptionTextElementDisposal { .. }
        | DependencyContract::ListRecordTextCollection { .. }
        | DependencyContract::RecordTextElementDisposal { .. }
        | DependencyContract::ResultConstructor
        | DependencyContract::FreshResultConstructor { .. }
        | DependencyContract::FreshResultErrorDisposal { .. }
        | DependencyContract::RetainedErrorDisplay => Err(dependency.site),
    }
}

/// The site at which `dependency` blocks the walk without a body to follow,
/// or `None` for a kind the walk follows.
pub fn unfollowed_edge<'g>(dependency: &Dependency<'g>) -> Option<Site<'g>> {
    follow(dependency).err()
}

fn index<'r, 'g>(
    roots: &'r [RootReport<'g>],
    budget: &mut Budget,
) -> Work<PathMap<&'g DeclarationId, usize>> {
    let mut index = PathMap::new();
    for (at, root) in roots.iter().enumerate() {
        budget.step(1)?;
        if let BodyOwner::Declaration(id) = root.owner() {
            budget.reserve(1)?;
            index.insert(id, at, budget)?;
        }
    }
    Ok(index)
}

/// Decide whether the program that `report` describes may be compiled.
/// `graph` is the loaded graph the report was built from; it names the entry
/// whose startup is the program's start. `typed` is the typed report the
/// ownership report was built from, whose facts say what the backend would be
/// asked to lower. `limits` is the rule's own budget.
///
/// Order of decision: a `ResourceLimit` report is returned as itself and
/// nothing else is read; a `SourceRejected` report is returned with all its
/// findings; otherwise the bodies reachable from the entry startup are
/// walked and the first one that is not `Checked` blocks, at its blocked site.
/// Reached edges that need a body (ordinary calls, callable transfers, declared
/// signatures, once and shared invocations, active disposers) block at their
/// site when no body answers them, and every other edge kind blocks at its
/// site: the walk does not follow it, and a body nobody checked is never
/// compilable. Last, every walked body is read against the shapes the backend
/// lowers ([`capability::Shape`]); the first construct outside them blocks at
/// its span.
pub fn compilable_program<'r, 'g>(
    graph: &Graph,
    typed: &TypedReport<'g>,
    report: &'r GraphReport<'g>,
    limits: AnalysisLimits,
) -> Compilability<'r, 'g> {
    let mut budget = budget(limits);
    decide(graph, typed, report, &mut budget).unwrap_or_else(|()| spent(&budget))
}

fn decide<'r, 'g>(
    graph: &Graph,
    typed: &TypedReport<'g>,
    report: &'r GraphReport<'g>,
    budget: &mut Budget,
) -> Work<Compilability<'r, 'g>> {
    let limit = || {
        Compilability::Blocked(Blocker::ResourceLimit {
            statistics: report.statistics(),
        })
    };
    match report.completion() {
        GraphCompletion::ResourceLimit => return Ok(limit()),
        GraphCompletion::SourceRejected if !report.findings().is_empty() => {
            let found = report.findings();
            budget.reserve(found.len())?;
            let mut findings: Vec<&Finding<'g>> = found.iter().collect();
            charge_sort(findings.len(), budget)?;
            findings.sort_by(|a, b| {
                (site_order(&a.site), a.code()).cmp(&(site_order(&b.site), b.code()))
            });
            return Ok(Compilability::Blocked(Blocker::SourceRejected { findings }));
        }
        GraphCompletion::SourceRejected | GraphCompletion::Incomplete => {}
    }
    let entry = graph.request().entry.file_name();
    let mut start = None;
    for (at, root) in report.roots().iter().enumerate() {
        budget.step(1)?;
        let module = root.site().module;
        if root.owner() == BodyOwner::Startup
            && module.origin == Origin::Application
            && Some(OsStr::new(&module.path)) == entry
        {
            start = Some(at);
            break;
        }
    }
    let start = start.expect("a finished graph report has a startup root for the entry module");

    let sources = index(report.roots(), budget)?;
    let disposers = index(report.active_disposers(), budget)?;

    budget.reserve(report.roots().len())?;
    let mut seen_sources = vec![false; report.roots().len()];
    budget.reserve(report.active_disposers().len())?;
    let mut seen_disposers = vec![false; report.active_disposers().len()];
    seen_sources[start] = true;
    budget.reserve(1)?;
    let mut queue = VecDeque::from([Node::Source(start)]);
    let mut bodies: Vec<(Node, CheckedBody<'r, 'g>)> = Vec::new();
    while let Some(node) = queue.pop_front() {
        budget.step(1)?;
        let root = match node {
            Node::Source(at) => &report.roots()[at],
            Node::Disposer(at) => &report.active_disposers()[at],
        };
        let mir = match (root.completion(), root.mir()) {
            (Completion::Checked, Some(mir)) => mir,
            (Completion::ResourceLimit, _) => return Ok(limit()),
            _ => {
                return Ok(Compilability::Blocked(Blocker::Incomplete {
                    site: root.blocked_at().unwrap_or(root.site()),
                }));
            }
        };
        budget.reserve(1)?;
        bodies.push((node, CheckedBody { root, mir }));
        for dependency in root.dependencies() {
            budget.step(1)?;
            let next = match follow(dependency) {
                Err(site) => {
                    return Ok(Compilability::Blocked(Blocker::Incomplete { site }));
                }
                Ok(Follow::Disposer) => disposers
                    .get(dependency.declaration, budget)?
                    .map(|&at| Node::Disposer(at)),
                Ok(Follow::Body) => sources
                    .get(dependency.declaration, budget)?
                    .map(|&at| Node::Source(at)),
            };
            let Some(next) = next else {
                return Ok(Compilability::Blocked(Blocker::Incomplete {
                    site: dependency.site,
                }));
            };
            let seen = match next {
                Node::Source(at) => &mut seen_sources[at],
                Node::Disposer(at) => &mut seen_disposers[at],
            };
            if !std::mem::replace(seen, true) {
                budget.reserve(1)?;
                queue.push_back(next);
            }
        }
    }

    let facts = Facts::new(graph, typed, bodies.iter().map(|(_, body)| body.site().module), budget)?;
    let mut shapes = Shapes::default();
    for (node, body) in &bodies {
        budget.step(1)?;
        let unadmitted = match node {
            // The backend lowers no custom disposer.
            Node::Disposer(_) => Some(body.site()),
            Node::Source(_) => first_unadmitted(body, &facts, &mut shapes, budget)?,
        };
        if let Some(site) = unadmitted {
            return Ok(Compilability::Blocked(Blocker::Incomplete { site }));
        }
    }

    budget.reserve(bodies.len())?;
    let mut startup = None;
    let mut functions = Vec::with_capacity(bodies.len());
    for (node, body) in bodies {
        match node {
            Node::Source(at) if at == start => startup = Some(body),
            Node::Source(_) => functions.push(body),
            Node::Disposer(_) => unreachable!("a disposer is not admitted"),
        }
    }
    Ok(Compilability::Compilable(CompilableProgram {
        startup: startup.expect("the walk starts at the startup"),
        functions,
        shapes: shapes.into_set(),
        statistics: budget.statistics,
    }))
}
