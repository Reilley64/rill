//! Ownership checking and cleanup MIR for a checked function body.
use super::DeclarationFacts;
use super::declarations::Budget;
use super::scalars::{NodeFactKind, NodeFacts, Site};
use super::typed::{TypedReceipt, TypedReport};
use super::{
    AnalysisLimits, DeclarationShape, InvocationMode, Module, NodeId, QueryError, Statistics,
    TypeTerm,
};
use crate::loader::{Declaration, DeclarationId, DeclarationKind};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;

mod build;
mod flow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Checked,
    SourceRejected,
    Incomplete,
    ResourceLimit,
}

#[derive(Debug)]
pub struct Finding<'g> {
    pub site: Site<'g>,
    pub related: Site<'g>,
    kind: Conflict,
}

#[derive(Clone, Copy, Debug)]
enum Conflict {
    Moved,
    Access,
    ForbiddenMove,
    Escape,
    Release,
}

impl Finding<'_> {
    pub fn code(&self) -> &'static str {
        match self.kind {
            Conflict::Moved => "R0200",
            Conflict::Access => "R0201",
            Conflict::ForbiddenMove => "R0501",
            Conflict::Escape => "R0202",
            Conflict::Release => "R0203",
        }
    }
}

#[derive(Debug)]
pub struct Report<'g> {
    completion: Completion,
    findings: Vec<Finding<'g>>,
    blocked_at: Option<Site<'g>>,
    mir: Option<Mir<'g>>,
    statistics: Statistics,
}

impl<'g> Report<'g> {
    pub fn completion(&self) -> Completion {
        self.completion
    }
    pub fn findings(&self) -> &[Finding<'g>] {
        &self.findings
    }
    pub fn blocked_at(&self) -> Option<Site<'g>> {
        self.blocked_at
    }
    pub fn mir(&self) -> Option<&Mir<'g>> {
        self.mir.as_ref()
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
}

#[derive(Debug)]
pub struct Mir<'g> {
    pub entry: usize,
    pub blocks: Vec<Block<'g>>,
}

#[derive(Debug)]
pub struct Block<'g> {
    pub site: Site<'g>,
    pub operation: Operation<'g>,
    pub terminator: Terminator,
}

/// A named or temporary storage root, optionally projected to one owned field.
#[derive(Clone, Copy, Debug)]
pub struct Place<'g> {
    pub root: Site<'g>,
    pub field: Option<&'g Declaration>,
}

#[derive(Debug)]
pub enum Operation<'g> {
    None,
    Initialize(Place<'g>),
    Copy {
        from: Place<'g>,
        into: Option<Place<'g>>,
    },
    Move {
        from: Place<'g>,
        into: Option<Place<'g>>,
    },
    Borrow {
        source: Place<'g>,
        holder: Place<'g>,
        exclusive: bool,
    },
    Reborrow {
        parent: Place<'g>,
        holder: Place<'g>,
        exclusive: bool,
    },
    Dereference(Place<'g>),
    FormClosure {
        closure: Site<'g>,
        capture: Site<'g>,
        mode: InvocationMode,
    },
    Call(&'g DeclarationId),
    Dispose {
        callee: &'g DeclarationId,
        operand: Place<'g>,
    },
    Install {
        target: Place<'g>,
        incoming: Place<'g>,
    },
    Export(Place<'g>),
    Drop(Place<'g>),
    DropCapture {
        closure: Site<'g>,
        field: Site<'g>,
    },
    Cleanup(Vec<Cleanup<'g>>),
}

#[derive(Clone, Copy, Debug)]
pub enum Cleanup<'g> {
    EndStorage(Place<'g>),
    Drop(Place<'g>),
    DropCapture { closure: Site<'g>, field: Site<'g> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terminator {
    Goto(usize),
    Branch { yes: usize, no: usize },
    Return,
}

/// Lower the selected body after validating its supported direct call graph.
/// A body MIR is not a whole-capture ownership receipt or evidence of execution.
pub fn lower_body<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g Module,
    declaration: &DeclarationId,
    limits: AnalysisLimits,
) -> Result<Report<'g>, QueryError> {
    let typed = receipt.report;
    if declaration.kind != DeclarationKind::Function || declaration.module != module.key {
        return Err(QueryError::InvalidRequest);
    }
    if !typed
        .inventory()
        .graph()
        .modules()
        .iter()
        .any(|candidate| std::ptr::eq(candidate, module))
    {
        return Err(QueryError::InvalidRequest);
    }
    let header = typed
        .inventory()
        .type_facts()
        .declarations
        .get(declaration)
        .ok_or(QueryError::InvalidRequest)?;
    let mut budget = Budget {
        limits,
        statistics: Statistics::default(),
    };
    let result = checked_graph(typed, module, header, &mut budget);
    let (completion, findings, blocked_at, mir) = match result {
        Ok(mir) => (Completion::Checked, Vec::new(), None, Some(mir)),
        Err((
            module,
            Stop::Rejected {
                site,
                related,
                kind,
            },
        )) => (
            Completion::SourceRejected,
            vec![Finding {
                site: site_at(module, site),
                related: site_at(module, related),
                kind,
            }],
            None,
            None,
        ),
        Err((module, Stop::Pending(node))) => (
            Completion::Incomplete,
            Vec::new(),
            Some(site_at(module, node)),
            None,
        ),
        Err((_, Stop::Limit)) => (Completion::ResourceLimit, Vec::new(), None, None),
    };
    Ok(Report {
        completion,
        findings,
        blocked_at,
        mir,
        statistics: budget.statistics,
    })
}

// Prove the declared dependency contracts of every direct callee reached by
// the selected body. Recursion uses those declarations provisionally; no MIR
// escapes until the worklist has validated each reached body.
fn checked_graph<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    header: &DeclarationFacts<'g>,
    budget: &mut Budget,
) -> Result<Mir<'g>, (&'g Module, Stop)> {
    let reached = |mir: &Mir<'g>,
                   pending: &mut VecDeque<(&'g Module, &'g DeclarationId)>,
                   seen: &mut BTreeSet<&'g DeclarationId>,
                   budget: &mut Budget|
     -> ResultOf<()> {
        for block in &mir.blocks {
            budget.step(1)?;
            let Operation::Call(callee) = block.operation else {
                continue;
            };
            if seen.contains(callee) {
                continue;
            }
            let mut target = None;
            for candidate in typed.inventory().graph().modules() {
                budget.step(1)?;
                if candidate.key == callee.module {
                    target = Some(candidate);
                    break;
                }
            }
            let target = target.ok_or(Stop::Pending(block.site.node))?;
            budget.reserve(2)?;
            seen.insert(callee);
            pending.push_back((target, callee));
        }
        Ok(())
    };

    budget.reserve(1).map_err(|()| (module, Stop::Limit))?;
    let mut seen = BTreeSet::from([&header.declaration.id]);
    let mut pending = VecDeque::new();
    let root = lower(typed, module, header, budget).map_err(|stop| (module, stop))?;
    reached(&root, &mut pending, &mut seen, budget).map_err(|stop| (module, stop))?;
    while let Some((target, id)) = pending.pop_front() {
        budget.step(1).map_err(|()| (target, Stop::Limit))?;
        let header = typed
            .inventory()
            .type_facts()
            .declarations
            .get(id)
            .ok_or((module, Stop::Pending(header.declaration.node)))?;
        let mir = lower(typed, target, header, budget).map_err(|stop| (target, stop))?;
        reached(&mir, &mut pending, &mut seen, budget).map_err(|stop| (target, stop))?;
    }
    Ok(root)
}

fn lower<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    header: &DeclarationFacts<'g>,
    budget: &mut Budget,
) -> ResultOf<Mir<'g>> {
    let entry_site = header.declaration.node;
    let DeclarationShape::Function { parameters, .. } = &header.shape else {
        return Err(Stop::Pending(entry_site));
    };
    let Some(&body) = module.syntax.nodes[entry_site].children.get(1) else {
        return Err(Stop::Pending(entry_site));
    };
    let mut build = Build {
        typed,
        module,
        facts: BTreeMap::new(),
        locals: BTreeMap::new(),
        slots: Vec::new(),
        code: Vec::new(),
        calls: Vec::new(),
        cleanups: Vec::new(),
        dispose: typed.dispose_role().map(|d| &d.id),
        budget,
    };
    for fact in typed.bodies().typed_nodes() {
        build.budget.step(1)?;
        if fact.site.module == &module.key {
            build.budget.reserve(1)?;
            build.facts.insert(fact.site.node, fact);
        }
    }
    build.budget.reserve(parameters.len())?;
    let mut params = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        if matches!(parameter.ty.as_ref(), TypeTerm::Callable { .. }) {
            return Err(Stop::Pending(parameter.declaration.node));
        }
        let slot = build.local(
            &parameter.declaration.id,
            parameter.declaration.node,
            &parameter.ty,
        )?;
        params.push(slot);
    }
    let end = build.emit(body, Action::None, Edges::End)?;
    let finish = build.cleanup(body, &params, end)?;
    let entry = build.statement(
        body,
        finish,
        Exits {
            returning: finish,
            looping: None,
        },
        1,
    )?;
    flow::run(build, entry, &params)
}

fn site_at(module: &Module, node: NodeId) -> Site<'_> {
    Site {
        module: &module.key,
        node,
        span: &module.syntax.nodes[node].span,
    }
}

enum Stop {
    Rejected {
        site: NodeId,
        related: NodeId,
        kind: Conflict,
    },
    Pending(NodeId),
    Limit,
}
impl From<()> for Stop {
    fn from((): ()) -> Self {
        Self::Limit
    }
}
type ResultOf<T> = Result<T, Stop>;
type Slot = usize;
type Pc = usize;

#[derive(Clone, Copy)]
struct Local<'g> {
    node: NodeId,
    owner: Slot,
    field: Option<&'g Declaration>,
    fields: usize,
    reference_field: bool,
    has_custom_disposer: bool,
    moves: bool,
    drops: bool,
}

impl<'g> Local<'g> {
    fn storage(self, slot: Slot) -> Range<Slot> {
        slot..slot + 1 + self.fields
    }

    fn extraction_sources(self, slot: Slot) -> Slot {
        if self.reference_field {
            self.owner
        } else {
            slot
        }
    }

    fn place(self, module: &'g Module) -> Place<'g> {
        Place {
            root: site_at(module, self.node),
            field: self.field,
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    None,
    Init(Slot),
    Read {
        from: Slot,
        into: Option<Slot>,
    },
    ForbiddenBorrowedMove(Slot),
    Borrow {
        from: Slot,
        into: Slot,
        exclusive: bool,
    },
    Reborrow {
        parent: Slot,
        into: Slot,
        exclusive: bool,
        suspends: bool,
    },
    Dereference {
        from: Slot,
        into: Option<Slot>,
    },
    Call(usize),
    FormClosure {
        capture: Slot,
        into: Slot,
        mode: InvocationMode,
    },
    Dispose(Slot),
    Displace {
        target: Slot,
        incoming: Slot,
    },
    Install {
        target: Slot,
        incoming: Slot,
    },
    End(Slot),
    Cleanup(usize),
    Export(Slot),
}

#[derive(Clone, Copy)]
enum Edges {
    Next(Pc),
    Branch { yes: Pc, no: Pc },
    End,
}

#[derive(Clone, Copy)]
struct Instruction {
    node: NodeId,
    action: Action,
    edges: Edges,
}

#[derive(Clone, Copy)]
struct Exits {
    returning: Pc,
    looping: Option<Loop>,
}

#[derive(Clone, Copy)]
struct Loop {
    breaks: Pc,
    continues: Pc,
}

struct Build<'a, 'g> {
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    facts: BTreeMap<NodeId, &'a NodeFacts<'g>>,
    locals: BTreeMap<&'g DeclarationId, Slot>,
    slots: Vec<Local<'g>>,
    code: Vec<Instruction>,
    calls: Vec<Call<'g>>,
    cleanups: Vec<Vec<Slot>>,
    dispose: Option<&'g DeclarationId>,
    budget: &'a mut Budget,
}

struct Call<'g> {
    callee: &'g DeclarationId,
    args: Vec<Slot>,
    into: Option<Slot>,
    retains: bool,
}
