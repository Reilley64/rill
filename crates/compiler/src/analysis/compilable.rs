//! The compilable-program rule: whether a loaded program may be compiled yet.
//!
//! A program is compilable when its entry startup and every body reachable
//! from it are `Checked`, although the whole graph has no complete state. This
//! module is the one place the backend and the CLI ask; neither decides
//! compilability itself.
//!
//! The rule only reads a finished [`GraphReport`]. It takes no limits and
//! spends no budget: no `step`, `reserve` or `charge_key`, so no cap moves and
//! nothing here is metered under `docs/agents/compiler-metering.md`. Its cost
//! is linear in the roots and dependencies the analysis already paid for.
//! A report whose completion is `ResourceLimit` is never read past that
//! completion (ADR 0029).
//!
//! `Checked` certifies ordinary MIR, not that delivery has been resolved
//! (`RootReport::completion`), and the graph's open obligations are not
//! consulted. The rule is for ordinary execution: it reads the entry's
//! startup, which test mode does not run.
use super::{
    Statistics,
    ownership::{
        Completion, Dependency, DependencyContract, Finding, GraphCompletion, GraphReport, Mir,
        RootReport,
    },
    scalars::{BodyOwner, Site},
};
use crate::loader::{DeclarationId, Graph, Origin};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsStr;

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
    /// Valid source whose reachable shape the analysis does not support yet,
    /// at the first blocking span. Report it under [`UNSUPPORTED_SHAPE`].
    Incomplete { site: Site<'g> },
    /// The source carries a finding, in a reachable body or not: unexecuted
    /// code is still checked.
    SourceRejected { finding: &'r Finding<'g> },
    /// A cap ran out. Nothing else is read from the report.
    ResourceLimit { statistics: Statistics },
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

/// The entry startup and every body reachable from it, all `Checked`.
/// Reachable bodies appear once, in breadth-first discovery order from the
/// startup, following each body's dependencies in report order.
#[derive(Debug)]
pub struct CompilableProgram<'r, 'g> {
    startup: CheckedBody<'r, 'g>,
    functions: Vec<CheckedBody<'r, 'g>>,
    active_disposers: Vec<CheckedBody<'r, 'g>>,
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
    /// The active frames of custom disposers that a reachable body reaches. An
    /// active frame is a separate body from its implementation's source root.
    pub fn active_disposers(&self) -> &[CheckedBody<'r, 'g>] {
        &self.active_disposers
    }
}

#[derive(Clone, Copy)]
enum Node {
    Source(usize),
    Disposer(usize),
}

/// Decide whether the program that `report` describes may be compiled.
/// `graph` is the loaded graph the report was built from; it names the entry
/// whose startup is the program's start.
///
/// Order of decision: a `ResourceLimit` report is returned as itself and
/// nothing else is read; a `SourceRejected` report is returned with its first
/// finding; otherwise the bodies reachable from the entry startup are
/// walked and the first one that is not `Checked` blocks, at its blocked site.
/// Reached edges that need a body (ordinary calls, callable transfers, declared
/// signatures, once and shared invocations, active disposers) block at their
/// site when no body answers them. Edges closed by an effect in the caller's
/// MIR need no body and are not followed.
pub fn compilable_program<'r, 'g>(
    graph: &Graph,
    report: &'r GraphReport<'g>,
) -> Compilability<'r, 'g> {
    let limit = || {
        Compilability::Blocked(Blocker::ResourceLimit {
            statistics: report.statistics(),
        })
    };
    match report.completion() {
        GraphCompletion::ResourceLimit => return limit(),
        GraphCompletion::SourceRejected => {
            if let Some(finding) = report.findings().first() {
                return Compilability::Blocked(Blocker::SourceRejected { finding });
            }
        }
        GraphCompletion::Incomplete => {}
    }
    let entry = graph.request().entry.file_name();
    let start = report
        .roots()
        .iter()
        .position(|root| {
            let module = root.site().module;
            root.owner() == BodyOwner::Startup
                && module.origin == Origin::Application
                && Some(OsStr::new(&module.path)) == entry
        })
        .expect("a finished graph report has a startup root for the entry module");

    let index = |roots: &'r [RootReport<'g>]| -> BTreeMap<&'g DeclarationId, usize> {
        roots
            .iter()
            .enumerate()
            .filter_map(|(at, root)| match root.owner() {
                BodyOwner::Declaration(id) => Some((id, at)),
                BodyOwner::Startup => None,
            })
            .collect()
    };
    let sources = index(report.roots());
    let disposers = index(report.active_disposers());

    let mut seen_sources = vec![false; report.roots().len()];
    let mut seen_disposers = vec![false; report.active_disposers().len()];
    seen_sources[start] = true;
    let mut queue = VecDeque::from([Node::Source(start)]);
    let mut startup = None;
    let mut functions = Vec::new();
    let mut active_disposers = Vec::new();
    while let Some(node) = queue.pop_front() {
        let root = match node {
            Node::Source(at) => &report.roots()[at],
            Node::Disposer(at) => &report.active_disposers()[at],
        };
        let mir = match (root.completion(), root.mir()) {
            (Completion::Checked, Some(mir)) => mir,
            (Completion::ResourceLimit, _) => return limit(),
            _ => {
                return Compilability::Blocked(Blocker::Incomplete {
                    site: root.blocked_at().unwrap_or(root.site()),
                });
            }
        };
        let body = CheckedBody { root, mir };
        match node {
            Node::Source(at) if at == start => startup = Some(body),
            Node::Source(_) => functions.push(body),
            Node::Disposer(_) => active_disposers.push(body),
        }
        for edge in root.dependencies() {
            let Some(next) = edge_target(edge, &sources, &disposers) else {
                if needs_body(edge) {
                    return Compilability::Blocked(Blocker::Incomplete { site: edge.site });
                }
                continue;
            };
            let seen = match next {
                Node::Source(at) => &mut seen_sources[at],
                Node::Disposer(at) => &mut seen_disposers[at],
            };
            if !std::mem::replace(seen, true) {
                queue.push_back(next);
            }
        }
    }
    Compilability::Compilable(CompilableProgram {
        startup: startup.expect("the walk starts at the startup"),
        functions,
        active_disposers,
    })
}

fn needs_body(edge: &Dependency<'_>) -> bool {
    matches!(
        edge.contract,
        DependencyContract::OrdinaryCall
            | DependencyContract::CallableTransfer(_)
            | DependencyContract::DeclaredSignature
            | DependencyContract::DeclaredOnceInvocation { .. }
            | DependencyContract::DeclaredSharedInvocation { .. }
            | DependencyContract::ActiveDisposer
    )
}

fn edge_target(
    edge: &Dependency<'_>,
    sources: &BTreeMap<&DeclarationId, usize>,
    disposers: &BTreeMap<&DeclarationId, usize>,
) -> Option<Node> {
    match edge.contract {
        DependencyContract::ActiveDisposer => disposers
            .get(edge.declaration)
            .map(|&at| Node::Disposer(at)),
        _ if needs_body(edge) => sources.get(edge.declaration).map(|&at| Node::Source(at)),
        _ => None,
    }
}
