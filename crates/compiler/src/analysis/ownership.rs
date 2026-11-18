//! Ownership checking and cleanup MIR for a checked function body.
use super::declarations::Budget;
use super::scalars::{NodeFactKind, NodeFacts, Site};
use super::typed::TypedReceipt;
use super::{
    AnalysisLimits, DeclarationShape, InvocationMode, Module, NodeId, QueryError, Statistics,
    TypeTerm,
};
use crate::loader::{DeclarationId, DeclarationKind};
use std::collections::BTreeMap;

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
    Escape,
    Release,
}

impl Finding<'_> {
    pub fn code(&self) -> &'static str {
        match self.kind {
            Conflict::Moved => "R0200",
            Conflict::Access => "R0201",
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

#[derive(Debug)]
pub enum Operation<'g> {
    None,
    Initialize(Site<'g>),
    Copy {
        from: Site<'g>,
        into: Option<Site<'g>>,
    },
    Move {
        from: Site<'g>,
        into: Option<Site<'g>>,
    },
    Borrow {
        source: Site<'g>,
        holder: Site<'g>,
        exclusive: bool,
    },
    Reborrow {
        parent: Site<'g>,
        holder: Site<'g>,
        exclusive: bool,
    },
    Dereference(Site<'g>),
    FormClosure {
        closure: Site<'g>,
        capture: Site<'g>,
        mode: InvocationMode,
    },
    Call(&'g DeclarationId),
    Dispose {
        callee: &'g DeclarationId,
        operand: Site<'g>,
    },
    Install {
        target: Site<'g>,
        incoming: Site<'g>,
    },
    Export(Site<'g>),
    Drop(Site<'g>),
    DropCapture {
        closure: Site<'g>,
        field: Site<'g>,
    },
    ScopeExit(Vec<Cleanup<'g>>),
}

#[derive(Debug)]
pub enum Cleanup<'g> {
    EndStorage(Site<'g>),
    Drop(Site<'g>),
    DropCapture { closure: Site<'g>, field: Site<'g> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terminator {
    Goto(usize),
    Branch { yes: usize, no: usize },
    Return,
}

/// Lower only the selected body, not the entire capture. A body MIR is not an
/// ownership receipt for the program or evidence of execution.
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
    let entry_site = header.declaration.node;
    let result = (|| {
        let DeclarationShape::Function { parameters, .. } = &header.shape else {
            return Err(Stop::Pending(entry_site));
        };
        let Some(&body) = module.syntax.nodes[entry_site].children.get(1) else {
            return Err(Stop::Pending(entry_site));
        };
        let mut build = Build {
            module,
            facts: BTreeMap::new(),
            locals: BTreeMap::new(),
            slots: Vec::new(),
            code: Vec::new(),
            calls: Vec::new(),
            cleanups: Vec::new(),
            dispose: typed.dispose_role().map(|d| &d.id),
            budget: &mut budget,
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
    })();
    let (completion, findings, blocked_at, mir) = match result {
        Ok(mir) => (Completion::Checked, Vec::new(), None, Some(mir)),
        Err(Stop::Rejected {
            site,
            related,
            kind,
        }) => (
            Completion::SourceRejected,
            vec![Finding {
                site: site_at(module, site),
                related: site_at(module, related),
                kind,
            }],
            None,
            None,
        ),
        Err(Stop::Pending(node)) => (
            Completion::Incomplete,
            Vec::new(),
            Some(site_at(module, node)),
            None,
        ),
        Err(Stop::Limit) => (Completion::ResourceLimit, Vec::new(), None, None),
    };
    Ok(Report {
        completion,
        findings,
        blocked_at,
        mir,
        statistics: budget.statistics,
    })
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
struct Local {
    node: NodeId,
    moves: bool,
    drops: bool,
}

#[derive(Clone, Copy)]
enum Action {
    None,
    Init(Slot),
    Read {
        from: Slot,
        into: Option<Slot>,
    },
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
    module: &'g Module,
    facts: BTreeMap<NodeId, &'a NodeFacts<'g>>,
    locals: BTreeMap<&'g DeclarationId, Slot>,
    slots: Vec<Local>,
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
