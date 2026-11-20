//! Ordinary ownership coverage over the loaded graph, not an execution plan.
use super::*;
use crate::analysis::{ordinary::ImplementationFacts, scalars::BodyFacts};
use crate::loader::{GraphId, ModuleKey, Origin};
use std::collections::BTreeSet;

/// Whole-capture disposition. This increment has no complete state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphCompletion {
    Incomplete,
    SourceRejected,
    ResourceLimit,
}

/// Obligations separate from the ordinary root/dependency judgment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphObligation {
    IntrinsicCancellation,
    MaskingAndJoin,
    EmbeddedAndUnreachedSource,
}

/// The frame or terminal contract required at a reached site. Only direct
/// calls and active disposers have body checks here; other contracts stay pending.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyContract<'g> {
    OrdinaryCall,
    ActiveDisposer,
    BuiltinDispose,
    ProtectedRelease { entry: &'g DeclarationId },
    ResultConstructor,
    RetainedErrorDisplay,
}

#[derive(Clone, Copy, Debug)]
pub struct Dependency<'g> {
    pub site: Site<'g>,
    pub declaration: &'g DeclarationId,
    pub contract: DependencyContract<'g>,
}

#[derive(Debug)]
pub struct RootReport<'g> {
    site: Site<'g>,
    owner: BodyOwner<'g>,
    completion: Completion,
    blocked_at: Option<Site<'g>>,
    mir: Option<Mir<'g>>,
    dependencies: Vec<Dependency<'g>>,
}

impl<'g> RootReport<'g> {
    pub fn site(&self) -> Site<'g> {
        self.site
    }
    pub fn owner(&self) -> BodyOwner<'g> {
        self.owner
    }
    /// Includes the reached ordinary dependency closure, not the separate
    /// obligations listed by `GraphReport::open_obligations`. `ResourceLimit`
    /// also covers roots whose ordinary result could not be published.
    pub fn completion(&self) -> Completion {
        self.completion
    }
    /// A known blocker, even if exhaustion prevented a closure verdict.
    pub fn blocked_at(&self) -> Option<Site<'g>> {
        self.blocked_at
    }
    /// Scoped ordinary MIR. Resolve called bodies and active disposers through
    /// the graph report; they are stored once, not copied into each root MIR.
    pub fn mir(&self) -> Option<&Mir<'g>> {
        self.mir.as_ref()
    }
    /// Reached edges from successful local lowering. A rejection or exhaustion
    /// may prevent edge discovery; an empty list isn't a coverage claim.
    pub fn dependencies(&self) -> &[Dependency<'g>] {
        &self.dependencies
    }
}

#[derive(Debug)]
pub struct GraphReport<'g> {
    context: GraphId,
    test_mode: bool,
    completion: GraphCompletion,
    inventory_complete: bool,
    source_roots: usize,
    bodies: Vec<RootReport<'g>>,
    findings: Vec<Finding<'g>>,
    statistics: Statistics,
}

impl<'g> GraphReport<'g> {
    pub fn context(&self) -> GraphId {
        self.context.clone()
    }
    pub fn test_mode(&self) -> bool {
        self.test_mode
    }
    /// Resource exhaustion takes precedence; already recorded findings survive.
    pub fn completion(&self) -> GraphCompletion {
        self.completion
    }
    /// Whether all required *source roots* were identified, not whether they
    /// were checked or their reached edges and embedded work were inventoried.
    pub fn inventory_complete(&self) -> bool {
        self.inventory_complete
    }
    /// Application then standard modules, by root-relative path and, after
    /// each startup, declaration offset. On inventory exhaustion this is a prefix.
    pub fn roots(&self) -> &[RootReport<'g>] {
        &self.bodies[..self.source_roots]
    }
    /// Reached active frames in discovery order. These do not discharge their
    /// implementation's source root or any protected release contract.
    pub fn active_disposers(&self) -> &[RootReport<'g>] {
        &self.bodies[self.source_roots..]
    }
    /// Findings follow local checking order, with reached active frames after
    /// the source roots. Each finding is recorded once, not once per caller.
    pub fn findings(&self) -> &[Finding<'g>] {
        &self.findings
    }
    pub fn open_obligations(&self) -> &'static [GraphObligation] {
        &[
            GraphObligation::IntrinsicCancellation,
            GraphObligation::MaskingAndJoin,
            GraphObligation::EmbeddedAndUnreachedSource,
        ]
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
}

/// Check loaded source roots and reached ordinary dependencies in one metered
/// request. A typed receipt binds this report to exactly one capture and mode.
/// This increment does not issue a full ownership receipt, even if every
/// ordinary judgment checks: cancellation, masking/join and embedded or
/// unreached source work still require separate proofs.
pub fn analyze_ownership<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    limits: AnalysisLimits,
) -> GraphReport<'g> {
    let typed = receipt.report;
    let graph = typed.inventory().graph();
    let mut report = GraphReport {
        context: graph.id(),
        test_mode: graph.request().test_mode,
        completion: GraphCompletion::ResourceLimit,
        inventory_complete: false,
        source_roots: 0,
        bodies: Vec::new(),
        findings: Vec::new(),
        statistics: Statistics::default(),
    };
    let mut budget = Budget {
        limits,
        statistics: Statistics::default(),
    };
    if analyze(typed, &mut report, &mut budget).is_ok() {
        report.completion = if report.findings.is_empty() {
            GraphCompletion::Incomplete
        } else {
            GraphCompletion::SourceRejected
        };
    }
    report.statistics = budget.statistics;
    report
}

#[derive(Default)]
struct ModuleFacts<'a, 'g> {
    nodes: NodeIndex<'a, 'g>,
    startup: usize,
}

#[derive(Clone, Copy, Default)]
struct RootFacts<'a, 'g> {
    body: Option<&'a BodyFacts<'g>>,
    header: Option<&'a DeclarationFacts<'g>>,
    implementation: Option<(&'a ImplementationFacts<'g>, bool)>,
}

struct WorkItem<'a, 'g> {
    module: &'g Module,
    module_index: usize,
    declaration: Option<&'g Declaration>,
    facts: RootFacts<'a, 'g>,
    active: Option<usize>,
    parents: Vec<usize>,
    // Held privately until closure checking is finished, even on exhaustion.
    ordinary: Option<Mir<'g>>,
}

// Modules are already sorted by the loader. Prepay the whole binary search
// (one comparison per halving, plus the final equality check). Each comparison
// can inspect at most the query path's byte length, even for a shared prefix.
fn module_index(
    modules: &[Module],
    key: &ModuleKey,
    budget: &mut Budget,
) -> Result<Option<usize>, ()> {
    let probes = usize::BITS - modules.len().leading_zeros() + 1;
    charge_key(key, probes as usize, budget)?;
    Ok(modules.binary_search_by(|module| module.key.cmp(key)).ok())
}

fn charge_key(key: &ModuleKey, probes: usize, budget: &mut Budget) -> Result<(), ()> {
    let width = key.path.len().checked_add(1).ok_or(())?;
    budget.step(width.checked_mul(probes).ok_or(())?)
}

// All keys below come from this receipt. After resolving a module by its
// canonical value, graph indexes need only fixed-width capture-local keys.
type RootIndex = BTreeMap<(usize, usize, DeclarationKind), usize>;

fn local_key(module: usize, id: &DeclarationId) -> (usize, usize, DeclarationKind) {
    (module, id.start, id.kind)
}

fn find_root(
    modules: &[Module],
    roots: &RootIndex,
    id: &DeclarationId,
    budget: &mut Budget,
) -> Result<Option<usize>, ()> {
    let Some(module) = module_index(modules, &id.module, budget)? else {
        return Ok(None);
    };
    budget.step(1)?;
    Ok(roots.get(&local_key(module, id)).copied())
}

fn push_body<'a, 'g>(
    module: &'g Module,
    module_index: usize,
    declaration: Option<&'g Declaration>,
    report: &mut GraphReport<'g>,
    work: &mut Vec<WorkItem<'a, 'g>>,
    budget: &mut Budget,
) -> Result<usize, ()> {
    budget.reserve(2)?;
    let index = work.len();
    report.bodies.push(RootReport {
        site: site_at(module, declaration.map_or(module.syntax.root, |d| d.node)),
        owner: declaration.map_or(BodyOwner::Startup, |d| BodyOwner::Declaration(&d.id)),
        completion: Completion::ResourceLimit,
        blocked_at: None,
        mir: None,
        dependencies: Vec::new(),
    });
    work.push(WorkItem {
        module,
        module_index,
        declaration,
        facts: RootFacts::default(),
        active: None,
        parents: Vec::new(),
        ordinary: None,
    });
    Ok(index)
}

fn analyze<'g>(
    typed: &TypedReport<'g>,
    report: &mut GraphReport<'g>,
    budget: &mut Budget,
) -> Result<(), ()> {
    let inventory = typed.inventory();
    let graph = inventory.graph();
    let modules = graph.modules();
    let mut work = Vec::new();
    let mut by_declaration = BTreeMap::new();
    let mut facts = Vec::new();
    let mut family_count = 0usize;
    // The loader stores modules in key order and declarations in id order
    // (within a module: source offset, then kind). No typed facts define membership.
    for (module_index, module) in modules.iter().enumerate() {
        budget.reserve(1)?;
        let startup = push_body(module, module_index, None, report, &mut work, budget)?;
        facts.push(ModuleFacts {
            startup,
            ..ModuleFacts::default()
        });
        report.source_roots += 1;
        for declaration in &module.declarations {
            budget.step(1)?;
            match declaration.id.kind {
                DeclarationKind::Family if declaration.scope.is_none() => {
                    family_count = family_count.checked_add(1).ok_or(())?;
                    continue;
                }
                DeclarationKind::Function
                | DeclarationKind::Implementation
                | DeclarationKind::Expectation => {}
                _ => continue,
            }
            budget.reserve(1)?;
            let index = push_body(
                module,
                module_index,
                Some(declaration),
                report,
                &mut work,
                budget,
            )?;
            by_declaration.insert(local_key(module_index, &declaration.id), index);
            report.source_roots += 1;
        }
    }
    report.inventory_complete = true;
    for node in typed.bodies().typed_nodes() {
        budget.step(1)?;
        if let Some(module) = module_index(modules, node.site.module, budget)? {
            budget.reserve(1)?;
            facts[module].nodes.insert(node.site.node, node);
        }
    }
    for body in typed.bodies().checked_bodies() {
        budget.step(1)?;
        // BodyFacts pairs the canonical owner with a site in its own module.
        if let Some(module) = module_index(modules, body.site.module, budget)? {
            let index = match body.owner {
                BodyOwner::Startup => Some(facts[module].startup),
                BodyOwner::Declaration(id) => {
                    budget.step(1)?;
                    by_declaration.get(&local_key(module, id)).copied()
                }
            };
            if let Some(index) = index {
                work[index].facts.body = Some(body);
            }
        }
    }
    for header in inventory.type_facts().declarations() {
        budget.step(1)?;
        let id = &header.declaration.id;
        if id.kind == DeclarationKind::Function
            && let Some(index) = find_root(modules, &by_declaration, id, budget)?
        {
            work[index].facts.header = Some(header);
        }
    }
    let mut families = BTreeSet::new();
    for source in graph.implementations() {
        budget.step(1)?;
        let id = &source.family;
        let Some(module) = module_index(modules, &id.module, budget)? else {
            continue;
        };
        let key = local_key(module, id);
        budget.step(1)?;
        if families.contains(&key) {
            continue;
        }
        budget.reserve(1)?;
        families.insert(key);
        // The borrowed M4 map has at most one entry per module-scope family.
        // Bound all its candidate comparisons without relying on tree fanout.
        charge_key(&id.module, family_count, budget)?;
        if let Some(family) = inventory.captured_family(id) {
            for body in &family.implementations {
                budget.step(1)?;
                if let Some(index) =
                    find_root(modules, &by_declaration, &body.source.declaration, budget)?
                {
                    work[index].facts.implementation =
                        Some((body, family.header.binders.is_empty()));
                }
            }
        }
    }

    // An ordinary direct call reuses its symbolic named-function check. Active
    // cleanup has a different operand/epilogue, so it has its own work item.
    let dispose = typed.dispose_role().map(|role| &role.id);
    let mut reverse_edges = 0usize;
    let mut index = 0;
    while index < work.len() {
        budget.step(1)?;
        let item = &work[index];
        let module = item.module;
        let lowered = lower_indexed(
            typed,
            item,
            &facts[item.module_index].nodes,
            index >= report.source_roots,
            budget,
        );
        match lowered {
            Err(Stop::Limit) => return Err(()),
            Err(Stop::Pending(node)) => {
                let root = &mut report.bodies[index];
                root.blocked_at = Some(site_at(module, node));
            }
            Err(Stop::Rejected {
                site,
                related,
                kind,
            }) => {
                budget.reserve(1)?;
                report.findings.push(finding(module, site, related, kind));
                report.bodies[index].completion = Completion::SourceRejected;
            }
            Ok(Lowered { mir, .. }) => {
                visit_dependencies(&mir, budget, |edge, budget| {
                    budget.reserve(1)?;
                    let id = edge.declaration;
                    let target = match edge.contract {
                        DependencyContract::OrdinaryCall
                            if id.kind == DeclarationKind::Function =>
                        {
                            find_root(modules, &by_declaration, id, budget)?
                        }
                        DependencyContract::ActiveDisposer
                            if id.kind == DeclarationKind::Implementation
                                && id.module.origin == Origin::Application =>
                        'active: {
                            let Some(source) = find_root(modules, &by_declaration, id, budget)?
                            else {
                                break 'active None;
                            };
                            let item = &work[source];
                            if let Some(target) = item.active {
                                break 'active Some(target);
                            }
                            let (Some((body, _)), Some(dispose)) =
                                (item.facts.implementation, dispose)
                            else {
                                break 'active None;
                            };
                            charge_key(&body.source.family.module, 1, budget)?;
                            if &body.source.family != dispose {
                                break 'active None;
                            }
                            let source_facts = item.facts;
                            let target = push_body(
                                item.module,
                                item.module_index,
                                item.declaration,
                                report,
                                &mut work,
                                budget,
                            )?;
                            work[target].facts = source_facts;
                            work[source].active = Some(target);
                            Some(target)
                        }
                        _ => None,
                    };
                    report.bodies[index].dependencies.push(edge);
                    if let Some(target) = target {
                        budget.reserve(1)?;
                        work[target].parents.push(index);
                        reverse_edges += 1;
                    } else {
                        let root = &mut report.bodies[index];
                        root.blocked_at.get_or_insert(edge.site);
                    }
                    Ok(())
                })
                .map_err(|_| ())?;
                work[index].ordinary = Some(mir);
            }
        }
        index += 1;
    }

    // Charge the complete bounded close before releasing any MIR. Each pass
    // visits a frame once and traverses each incoming edge at most once.
    budget.step(work.len().checked_mul(6).ok_or(())?)?;
    budget.step(reverse_edges.checked_mul(2).ok_or(())?)?;
    budget.reserve(work.len())?;
    let mut queue = VecDeque::with_capacity(work.len());
    // Until closure is funded, a pending local edge is not the final result:
    // a rejected dependency may take precedence over it.
    for root in &mut report.bodies {
        if root.blocked_at.is_some() {
            root.completion = Completion::Incomplete;
        }
    }
    propagate(
        &mut report.bodies,
        &work,
        &mut queue,
        Completion::SourceRejected,
    );
    propagate(
        &mut report.bodies,
        &work,
        &mut queue,
        Completion::Incomplete,
    );
    for (root, item) in report.bodies.iter_mut().zip(work) {
        if root.completion == Completion::ResourceLimit
            && let Some(mir) = item.ordinary
        {
            root.completion = Completion::Checked;
            root.mir = Some(mir);
        }
    }
    Ok(())
}

fn propagate(
    bodies: &mut [RootReport<'_>],
    work: &[WorkItem<'_, '_>],
    queue: &mut VecDeque<usize>,
    completion: Completion,
) {
    queue.extend(
        bodies
            .iter()
            .enumerate()
            .filter_map(|(i, root)| (root.completion == completion).then_some(i)),
    );
    while let Some(index) = queue.pop_front() {
        let blocked_at = bodies[index].blocked_at;
        for &parent in &work[index].parents {
            let root = &mut bodies[parent];
            if root.completion != completion && root.completion != Completion::SourceRejected {
                root.completion = completion;
                root.blocked_at = blocked_at;
                queue.push_back(parent);
            }
        }
    }
}

fn lower_indexed<'a, 'g>(
    typed: &'a TypedReport<'g>,
    item: &WorkItem<'a, 'g>,
    nodes: &'a NodeIndex<'a, 'g>,
    active: bool,
    budget: &mut Budget,
) -> ResultOf<Lowered<'a, 'g>> {
    let WorkItem {
        module,
        declaration,
        facts,
        ..
    } = *item;
    let site = declaration.map_or(module.syntax.root, |d| d.node);
    let pending = || Stop::Pending(site);
    let body = facts
        .body
        .filter(|body| !body.flow.pending)
        .ok_or_else(pending)?;
    let nodes = Some(nodes);
    let Some(declaration) = declaration else {
        if body.site.node != site {
            return Err(pending());
        }
        return lower_startup_body(typed, module, nodes, budget);
    };
    let (header, frame) = match declaration.id.kind {
        DeclarationKind::Expectation => {
            if body.site.node != site {
                return Err(pending());
            }
            let result = body.result.as_ref().ok_or_else(pending)?;
            return lower_expectation_body(typed, module, site, result, nodes, budget);
        }
        DeclarationKind::Function => {
            let header = facts.header.ok_or_else(pending)?;
            if !std::ptr::eq(header.declaration, declaration)
                || !header.binders.is_empty()
                || module.syntax.nodes[site].children.get(1) != Some(&body.site.node)
            {
                return Err(pending());
            }
            (header, Frame::Ordinary)
        }
        DeclarationKind::Implementation if module.key.origin == Origin::Application => {
            let (implementation, closed_family) = facts.implementation.ok_or_else(pending)?;
            let source = implementation.source;
            if !std::ptr::eq(implementation.header.declaration, declaration)
                || source.body_node != body.site.node
            {
                return Err(pending());
            }
            let body = concrete_implementation_body(implementation)?;
            let frame = if active {
                // Only created by a checked, substitution-free InvokeDisposer
                // edge. The implementation is concrete even if dispose<T> isn't.
                if module.syntax.nodes[site].children.get(1) != Some(&body) {
                    return Err(pending());
                }
                Frame::ActiveDisposer
            } else if closed_family
                && source.family.module.origin == Origin::Application
                && source.family.kind == DeclarationKind::Family
            {
                Frame::PlainImplementation { body }
            } else {
                return Err(pending());
            };
            (&implementation.header, frame)
        }
        _ => return Err(pending()),
    };
    lower(typed, module, header, frame, nodes, budget)
}

pub(super) fn visit_dependencies<'g>(
    mir: &Mir<'g>,
    budget: &mut Budget,
    mut visit: impl FnMut(Dependency<'g>, &mut Budget) -> ResultOf<()>,
) -> ResultOf<()> {
    use DependencyContract::*;
    for block in &mir.blocks {
        budget.step(1)?;
        let mut emit = |declaration, contract, budget: &mut Budget| {
            visit(
                Dependency {
                    site: block.site,
                    declaration,
                    contract,
                },
                budget,
            )
        };
        match block.operation {
            Operation::Call(callee)
            | Operation::CallBorrowingRecord { callee, .. }
            | Operation::CallIntoTemporary { callee, .. }
            | Operation::CallRetaining { callee, .. }
            | Operation::CallRetainingPair { callee, .. }
            | Operation::CallRetainingWithBool { callee, .. }
            | Operation::CallRetainingWithOptionBool { callee, .. } => {
                emit(callee, OrdinaryCall, budget)?
            }
            Operation::Dispose { callee, .. } => emit(callee, BuiltinDispose, budget)?,
            Operation::ConstructResultError { constructor, .. } => {
                emit(constructor, ResultConstructor, budget)?
            }
            Operation::WrapResultFailure { cause_display, .. } => {
                emit(cause_display, RetainedErrorDisplay, budget)?
            }
            Operation::Cleanup(ref actions) => {
                budget.step(actions.len())?;
                for action in actions {
                    match *action {
                        Cleanup::InvokeDisposer { implementation, .. } => {
                            emit(implementation, ActiveDisposer, budget)?
                        }
                        Cleanup::ReleaseGuard {
                            implementation,
                            entry,
                            ..
                        } => emit(implementation, ProtectedRelease { entry }, budget)?,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
