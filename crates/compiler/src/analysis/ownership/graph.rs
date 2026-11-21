//! Ordinary and bounded delivered-exit judgments, not an execution plan.
use super::*;
use crate::analysis::typed::CollectionOperation;
use crate::analysis::{ordinary::ImplementationFacts, scalars::BodyFacts};
use crate::loader::{GraphId, ModuleKey, Origin};
use std::collections::BTreeSet;

mod closed_release;

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

/// The frame or terminal contract required at a reached site. Closed
/// specializations check their own bodies without closing unbound source roots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyContract<'g> {
    OrdinaryCall,
    /// Closed at the call by ordinary flow; the open source root is separate.
    CallableTransfer(CallableLayout),
    /// Closed ordinary specialization. `body` belongs to the implementation's
    /// module; `arrow_bodies` are the two checked arrow bodies in the caller.
    SynchronousListArrows {
        body: NodeId,
        arrow_bodies: [NodeId; 2],
    },
    /// Closed at the operation's reached commit, wait and terminal successors.
    ScopedJoin {
        bridge: &'g DeclarationId,
        arrow_bodies: [NodeId; 2],
    },
    ActiveDisposer,
    BuiltinDispose,
    ProtectedRelease {
        entry: &'g DeclarationId,
    },
    /// Closed ordinary effect of the installed List<i32> disposal bridge.
    ListI32Disposal {
        bridge: &'g DeclarationId,
    },
    /// Closed ordinary cursor effect, not a checked generic wrapper frame.
    ListI32Iteration {
        operation: CollectionOperation,
        bridge: &'g DeclarationId,
    },
    /// Closed List<string> transfer/advance/disposal through the installed bridge.
    ListTextCollection {
        operation: CollectionOperation,
        bridge: &'g DeclarationId,
    },
    /// The owning string cleanup supplied to the whole-list or suffix disposer.
    ListTextElementDisposal {
        bridge: &'g DeclarationId,
    },
    /// A constructor outside the closed storage contract.
    ResultConstructor,
    /// Exact cause-free `Result<bool, i32>` construction in pinned storage.
    FreshResultConstructor {
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        storage_source: &'g std::path::Path,
    },
    /// Drop the actual sealed wrapper, not a call to an ordinary disposer.
    FreshResultErrorDisposal {
        /// The constructor node in the dependency site's module.
        origin: NodeId,
        variant: &'g DeclarationId,
        storage_source: &'g std::path::Path,
    },
    RetainedErrorDisplay,
}

#[derive(Clone, Copy, Debug)]
pub struct Dependency<'g> {
    pub site: Site<'g>,
    pub declaration: &'g DeclarationId,
    pub contract: DependencyContract<'g>,
}

/// A bounded delivery judgment. Unresolved is not evidence of no delivery.
#[derive(Debug)]
pub enum Delivery<'g, Proof> {
    Unresolved,
    Proved(Proof),
    Rejected {
        /// Index in this report's findings.
        finding_id: usize,
        /// Absent for an assumed-enabled frame judgment.
        via_call: Option<RetainingCallSite<'g>>,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct RetainingCallSite<'g> {
    /// Block in the caller's ordinary MIR; not a delivery checkpoint.
    pub block: usize,
    pub site: Site<'g>,
    /// Index in this graph report's source roots, not an alias or local node ID.
    pub callee_root: usize,
}

/// Bounded proof for one selected unmasked call. For delivery after transfer,
/// use `call.callee_root` to read the callee's enabled-frame proof and ordinary
/// MIR, then run `suspended.caller_cleanup`. The ordinary suffix does not run.
#[derive(Debug)]
pub struct GraphRetainingCall<'g> {
    pub call: RetainingCallSite<'g>,
    pub caller_entry_site: Site<'g>,
    pub on_caller_entry_delivery: CancellationExit<'g>,
    pub suspended: SuspendedRetainingFrame<'g>,
}

#[derive(Debug)]
pub struct RootReport<'g> {
    site: Site<'g>,
    owner: BodyOwner<'g>,
    ordinary_completion: Completion,
    blocked_at: Option<Site<'g>>,
    ordinary_mir: Option<Mir<'g>>,
    enabled_frame: Delivery<'g, Cancellation<'g>>,
    selected_call: Delivery<'g, Box<GraphRetainingCall<'g>>>,
    replacement: Delivery<'g, Box<ReplacementCancellation<'g>>>,
    dependencies: Vec<Dependency<'g>>,
}

impl<'g> RootReport<'g> {
    pub fn site(&self) -> Site<'g> {
        self.site
    }
    pub fn owner(&self) -> BodyOwner<'g> {
        self.owner
    }
    /// Includes the ordinary closure and any recorded delivery rejection.
    /// `Checked` certifies ordinary MIR, not that delivery has been resolved.
    pub fn completion(&self) -> Completion {
        if matches!(self.enabled_frame, Delivery::Rejected { .. })
            || matches!(self.selected_call, Delivery::Rejected { .. })
        {
            return Completion::SourceRejected;
        }
        self.ordinary_completion()
    }
    /// The ordinary closure alone. `ResourceLimit` can mean its result could
    /// not be published, even if local work finished.
    pub fn ordinary_completion(&self) -> Completion {
        self.ordinary_completion
    }
    /// Checks this named source frame assuming delivery is enabled, without
    /// asserting that any invocation reaches it in that context.
    pub fn enabled_frame(&self) -> &Delivery<'g, Cancellation<'g>> {
        &self.enabled_frame
    }
    /// The exact #69 retaining call in this root, with its unmasked caller
    /// and leaf-callee premises checked. Ordinary parents do not inherit it.
    pub fn selected_call(&self) -> &Delivery<'g, Box<GraphRetainingCall<'g>>> {
        &self.selected_call
    }
    /// The checked disposal-through-install interval, separate from an
    /// enabled-frame judgment. Resolve its body in `GraphReport::active_disposers`.
    pub fn replacement_cancellation(&self) -> &Delivery<'g, Box<ReplacementCancellation<'g>>> {
        &self.replacement
    }
    /// A known blocker, even if exhaustion prevented a closure verdict.
    pub fn blocked_at(&self) -> Option<Site<'g>> {
        self.blocked_at
    }
    /// Ordinary MIR if no recorded judgment rejects this root.
    pub fn mir(&self) -> Option<&Mir<'g>> {
        if self.completion() != Completion::Checked {
            return None;
        }
        self.ordinary_mir()
    }
    /// The reusable ordinary contract, even if the delivery judgment rejects.
    /// Callees and active disposers are stored once, not in each caller's MIR.
    pub fn ordinary_mir(&self) -> Option<&Mir<'g>> {
        self.ordinary_mir.as_ref()
    }
    /// Reached ordinary edges from successful local lowering, not a census of
    /// delivery exits. A rejection or exhaustion may prevent edge discovery;
    /// an empty list isn't a coverage claim.
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
    /// Ordinary findings in local checking order, then bounded delivery
    /// findings. Each canonical witness is recorded once, not once per caller.
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

/// Check loaded roots, reached ordinary dependencies and bounded delivered
/// exits in one metered request, for exactly the receipt's capture and mode.
/// This does not issue a full ownership receipt. General cancellation,
/// masking/join, and embedded or unreached source work remain open.
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
    // Closure and positive proofs stay private until publication is funded.
    ordinary_completion: Completion,
    blocked_at: Option<Site<'g>>,
    ordinary: Option<Mir<'g>>,
    empty_entry: Option<Site<'g>>,
    trace: Option<flow::CancellationTrace<'a, 'g>>,
    replacement: Option<(flow::ReplacementCandidate<'g>, usize, usize)>,
    replacement_proof: Option<Box<ReplacementCancellation<'g>>>,
    call_targets: BTreeMap<usize, usize>,
    enabled_proof: Option<Cancellation<'g>>,
    retaining_parameters: Option<[Place<'g>; 3]>,
    selected_proof: Option<Box<GraphRetainingCall<'g>>>,
}

type FindingIndex = BTreeMap<(usize, NodeId, NodeId, Conflict), usize>;

fn record_finding<'g>(
    report: &mut GraphReport<'g>,
    findings: &mut FindingIndex,
    item: &WorkItem<'_, 'g>,
    site: NodeId,
    related: NodeId,
    kind: Conflict,
    budget: &mut Budget,
) -> Result<usize, ()> {
    let key = (item.module_index, site, related, kind);
    budget.step(1)?;
    if let Some(&index) = findings.get(&key) {
        return Ok(index);
    }
    budget.reserve(2)?;
    let index = report.findings.len();
    findings.insert(key, index);
    report
        .findings
        .push(finding(item.module, site, related, kind));
    Ok(index)
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

pub(super) fn charge_key(key: &ModuleKey, probes: usize, budget: &mut Budget) -> Result<(), ()> {
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
        ordinary_completion: Completion::ResourceLimit,
        blocked_at: None,
        ordinary_mir: None,
        enabled_frame: Delivery::Unresolved,
        selected_call: Delivery::Unresolved,
        replacement: Delivery::Unresolved,
        dependencies: Vec::new(),
    });
    work.push(WorkItem {
        module,
        module_index,
        declaration,
        facts: RootFacts::default(),
        active: None,
        parents: Vec::new(),
        ordinary_completion: Completion::ResourceLimit,
        blocked_at: None,
        ordinary: None,
        empty_entry: None,
        trace: None,
        replacement: None,
        replacement_proof: None,
        call_targets: BTreeMap::new(),
        enabled_proof: None,
        retaining_parameters: None,
        selected_proof: None,
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
    let mut findings = FindingIndex::new();
    let mut cache = QueryCache::default();
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
            Lowering::new(budget, &mut cache),
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
                record_finding(report, &mut findings, item, site, related, kind, budget)?;
                report.bodies[index].ordinary_completion = Completion::SourceRejected;
            }
            Ok(Lowered {
                mir,
                trace,
                replacement,
                empty_entry,
            }) => {
                let mut displaced_body = None;
                let mut post_install_leaf = None;
                visit_dependencies(&mir, budget, |block, edge, budget| {
                    budget.reserve(1)?;
                    if matches!(
                        edge.contract,
                        DependencyContract::CallableTransfer(_)
                            | DependencyContract::SynchronousListArrows { .. }
                            | DependencyContract::ScopedJoin { .. }
                            | DependencyContract::ListI32Disposal { .. }
                            | DependencyContract::ListI32Iteration { .. }
                            | DependencyContract::ListTextCollection { .. }
                            | DependencyContract::ListTextElementDisposal { .. }
                            | DependencyContract::FreshResultConstructor { .. }
                            | DependencyContract::FreshResultErrorDisposal { .. }
                    ) || closed_release::proved(
                        typed,
                        module,
                        &facts[work[index].module_index].nodes,
                        &mir.blocks[block],
                        edge,
                        family_count,
                        budget,
                    )? {
                        // Specialized evidence closes this ordinary edge,
                        // not the callee's unbound source root.
                        report.bodies[index].dependencies.push(edge);
                        return Ok(());
                    }
                    let id = edge.declaration;
                    let target = match edge.contract {
                        DependencyContract::OrdinaryCall
                            if matches!(
                                id.kind,
                                DeclarationKind::Function | DeclarationKind::Implementation
                            ) =>
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
                        if let Some(candidate) = &replacement {
                            match edge.contract {
                                DependencyContract::ActiveDisposer
                                    if block == candidate.site.displacement =>
                                {
                                    displaced_body = Some(target);
                                }
                                DependencyContract::OrdinaryCall
                                    if block == candidate.site.post_install_call =>
                                {
                                    post_install_leaf = Some(target);
                                }
                                _ => {}
                            }
                        }
                        if trace.is_some() && edge.contract == DependencyContract::OrdinaryCall {
                            budget.reserve(1)?;
                            work[index].call_targets.insert(block, target);
                        }
                    } else {
                        let root = &mut report.bodies[index];
                        root.blocked_at.get_or_insert(edge.site);
                    }
                    Ok(())
                })
                .map_err(|_| ())?;
                let item = &mut work[index];
                item.ordinary = Some(mir);
                item.trace = trace;
                item.replacement = replacement
                    .zip(displaced_body)
                    .zip(post_install_leaf)
                    .map(|((candidate, body), leaf)| (candidate, body, leaf));
                item.empty_entry = empty_entry;
            }
        }
        index += 1;
    }

    // Close only ordinary dependencies. Keep provisional blockers public but
    // don't publish the closure until the final pass is funded.
    budget.step(work.len().checked_mul(5).ok_or(())?)?;
    budget.step(reverse_edges.checked_mul(2).ok_or(())?)?;
    budget.reserve(work.len())?;
    let mut queue = VecDeque::with_capacity(work.len());
    for (root, item) in report.bodies.iter().zip(&mut work) {
        item.blocked_at = root.blocked_at;
        item.ordinary_completion = if root.ordinary_completion == Completion::SourceRejected {
            Completion::SourceRejected
        } else if root.blocked_at.is_some() {
            Completion::Incomplete
        } else if item.ordinary.is_some() {
            Completion::Checked
        } else {
            Completion::ResourceLimit
        };
    }
    propagate(&mut work, &mut queue, Completion::SourceRejected);
    propagate(&mut work, &mut queue, Completion::Incomplete);
    check_enabled_frames(&mut work, report, &mut findings, budget)?;
    check_selected_calls(&mut work, report, budget)?;

    budget.step(work.len())?;
    budget.reserve(work.len())?;
    for (root, item) in report.bodies.iter_mut().zip(work) {
        root.ordinary_completion = item.ordinary_completion;
        root.blocked_at = item.blocked_at;
        if item.ordinary_completion == Completion::Checked {
            root.ordinary_mir = item.ordinary;
        }
        if let Some(proof) = item.enabled_proof {
            root.enabled_frame = Delivery::Proved(proof);
        }
        if let Some(proof) = item.selected_proof {
            root.selected_call = Delivery::Proved(proof);
        }
        if let Some(proof) = item.replacement_proof {
            root.replacement = Delivery::Proved(proof);
        }
    }
    Ok(())
}

fn propagate(work: &mut [WorkItem<'_, '_>], queue: &mut VecDeque<usize>, completion: Completion) {
    queue.extend(
        work.iter()
            .enumerate()
            .filter_map(|(i, item)| (item.ordinary_completion == completion).then_some(i)),
    );
    while let Some(index) = queue.pop_front() {
        let blocked_at = work[index].blocked_at;
        for i in 0..work[index].parents.len() {
            let parent = work[index].parents[i];
            let item = &mut work[parent];
            if item.ordinary_completion != completion
                && item.ordinary_completion != Completion::SourceRejected
            {
                item.ordinary_completion = completion;
                item.blocked_at = blocked_at;
                queue.push_back(parent);
            }
        }
    }
}

fn check_enabled_frames<'g>(
    work: &mut [WorkItem<'_, 'g>],
    report: &mut GraphReport<'g>,
    findings: &mut FindingIndex,
    budget: &mut Budget,
) -> Result<(), ()> {
    for index in 0..report.source_roots {
        budget.step(1)?;
        // Close the local interval using the active ordinary body and the
        // qualified leaf entry, not the leaf's enabled-frame disposition.
        if let Some((candidate, body, leaf)) = work[index].replacement.take()
            && work[index].ordinary_completion == Completion::Checked
            && let Some(entry) = work[leaf].empty_entry
        {
            work[index].replacement_proof = candidate
                .finish(
                    work[index].ordinary.as_ref().unwrap(),
                    work[body].ordinary.as_ref().unwrap(),
                    entry,
                    budget,
                )
                .map_err(|_| ())?;
        }
        let Some(trace) = work[index].trace.take() else {
            continue;
        };
        let item = &work[index];
        // A missing ordinary premise prevents proof publication, but a known
        // earlier checkpoint can still expose an invalid outward store.
        let ordinary_checked = item.ordinary_completion == Completion::Checked;
        let mut known_entry = false;
        let mut unknown_entry = !ordinary_checked;
        for &target in item.call_targets.values() {
            budget.step(1)?;
            known_entry |= work[target].empty_entry.is_some();
            unknown_entry |= work[target].empty_entry.is_none();
        }
        // Do not use delivery to discharge an ordinary dependency. Even in
        // a checked ordinary closure, nonempty calls have unknown delivery.
        match trace.finish(
            item.ordinary.as_ref().unwrap(),
            |block, _, budget| {
                budget.step(1)?;
                Ok(item
                    .call_targets
                    .get(&block)
                    .and_then(|&target| work[target].empty_entry))
            },
            known_entry,
            unknown_entry,
            budget,
        ) {
            Ok(Some(proof)) => {
                work[index].retaining_parameters = trace.retaining_parameters();
                work[index].enabled_proof = Some(proof);
            }
            Err(Stop::Limit) => return Err(()),
            Err(Stop::Rejected {
                site,
                related,
                kind: kind @ Conflict::CancellationEscape { .. },
            }) => {
                budget.reserve(1)?;
                let finding_id =
                    record_finding(report, findings, item, site, related, kind, budget)?;
                report.bodies[index].enabled_frame = Delivery::Rejected {
                    finding_id,
                    via_call: None,
                };
                if ordinary_checked {
                    work[index].retaining_parameters = trace.retaining_parameters();
                }
            }
            Ok(None) | Err(Stop::Pending(_) | Stop::Rejected { .. }) => {}
        }
        if ordinary_checked && trace.retaining_call_block().is_some() {
            work[index].trace = Some(trace);
        }
    }
    Ok(())
}

fn check_selected_calls<'g>(
    work: &mut [WorkItem<'_, 'g>],
    report: &mut GraphReport<'g>,
    budget: &mut Budget,
) -> Result<(), ()> {
    for index in 0..report.source_roots {
        budget.step(1)?;
        let Some(trace) = work[index].trace.take() else {
            continue;
        };
        let block = trace.retaining_call_block().unwrap();
        let item = &work[index];
        budget.step(1)?;
        let Some(&callee_root) = item.call_targets.get(&block) else {
            continue;
        };
        let Some(parameters) = work[callee_root].retaining_parameters else {
            continue;
        };
        let mir = item.ordinary.as_ref().unwrap();
        let call = match trace.prepare_retaining_call(mir, budget) {
            Ok(Some(call)) => call,
            Err(Stop::Limit) => return Err(()),
            Ok(None) | Err(Stop::Pending(_) | Stop::Rejected { .. }) => continue,
        };
        let site = RetainingCallSite {
            block,
            site: mir.blocks[block].site,
            callee_root,
        };
        // Only this exact context can reuse the enabled result. In particular,
        // neither an ordinary caller nor a second nested call inherits it.
        if let Delivery::Rejected { finding_id, .. } = report.bodies[callee_root].enabled_frame {
            budget.reserve(1)?;
            report.bodies[index].selected_call = Delivery::Rejected {
                finding_id,
                via_call: Some(site),
            };
            continue;
        }
        match call.finish_caller(parameters, budget) {
            Ok(Some((entry, suspended))) => {
                work[index].selected_proof = Some(Box::new(GraphRetainingCall {
                    call: site,
                    caller_entry_site: entry.entry_site,
                    on_caller_entry_delivery: entry.on_entry_delivery,
                    suspended,
                }));
            }
            Err(Stop::Limit) => return Err(()),
            Ok(None) | Err(Stop::Pending(_) | Stop::Rejected { .. }) => {}
        }
    }
    Ok(())
}

fn lower_indexed<'a, 'g>(
    typed: &'a TypedReport<'g>,
    item: &WorkItem<'a, 'g>,
    nodes: &'a NodeIndex<'a, 'g>,
    active: bool,
    lowering: Lowering<'_, 'a, 'g>,
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
        return lower_startup_body(typed, module, nodes, lowering);
    };
    if !matches!(body.owner, BodyOwner::Declaration(owner) if std::ptr::eq(owner, &declaration.id))
    {
        return Err(pending());
    }
    let (header, frame, collect_trace) = match declaration.id.kind {
        DeclarationKind::Expectation => {
            if body.site.node != site {
                return Err(pending());
            }
            let result = body.result.as_ref().ok_or_else(pending)?;
            return lower_expectation_body(typed, module, site, result, nodes, lowering);
        }
        DeclarationKind::Function => {
            let header = facts.header.ok_or_else(pending)?;
            if !std::ptr::eq(header.declaration, declaration)
                || !header.binders.is_empty()
                || module.syntax.nodes[site].children.get(1) != Some(&body.site.node)
            {
                return Err(pending());
            }
            (header, Frame::OrdinaryFunction, true)
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
            (&implementation.header, frame, false)
        }
        DeclarationKind::Implementation
            if module.key.origin == Origin::Standard && module.key.path == "core.rill" =>
        {
            let (implementation, _) = facts.implementation.ok_or_else(pending)?;
            let frame = scalar::implementation_frame(
                typed,
                module,
                declaration,
                implementation,
                body,
                lowering.budget,
            )?;
            (&implementation.header, frame, false)
        }
        _ => return Err(pending()),
    };
    lower(typed, module, header, frame, collect_trace, nodes, lowering)
}

pub(super) fn visit_dependencies<'g>(
    mir: &Mir<'g>,
    budget: &mut Budget,
    mut visit: impl FnMut(usize, Dependency<'g>, &mut Budget) -> ResultOf<()>,
) -> ResultOf<()> {
    use crate::analysis::typed::ListTraversal::Owned;
    use DependencyContract::*;
    for (index, block) in mir.blocks.iter().enumerate() {
        budget.step(1)?;
        let mut emit = |declaration, contract, budget: &mut Budget| {
            visit(
                index,
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
            Operation::OpenOwnedListI32 { provider, .. }
            | Operation::AdvanceOwnedListI32 { provider, .. } => emit(
                &provider.implementation.id,
                ListI32Iteration {
                    operation: if matches!(block.operation, Operation::OpenOwnedListI32 { .. }) {
                        CollectionOperation::Iterate(Owned)
                    } else {
                        CollectionOperation::Next(Owned)
                    },
                    bridge: &provider.bridge.id,
                },
                budget,
            )?,
            Operation::OpenOwnedListText { provider, .. }
            | Operation::AdvanceOwnedListText { provider, .. } => emit(
                &provider.implementation.id,
                ListTextCollection {
                    operation: if matches!(block.operation, Operation::OpenOwnedListText { .. }) {
                        CollectionOperation::Iterate(Owned)
                    } else {
                        CollectionOperation::Next(Owned)
                    },
                    bridge: &provider.bridge.id,
                },
                budget,
            )?,
            Operation::CallCallableTransfer { callee, layout, .. } => {
                emit(callee, CallableTransfer(layout), budget)?
            }
            Operation::CallListArrows {
                implementation,
                body,
                ref slots,
            } => {
                emit(
                    implementation,
                    SynchronousListArrows {
                        body: body.node,
                        arrow_bodies: [slots[0].body_site.node, slots[1].body_site.node],
                    },
                    budget,
                )?;
            }
            Operation::ScopedJoin(ref join) => {
                emit(
                    join.implementation,
                    ScopedJoin {
                        bridge: join.bridge,
                        arrow_bodies: join.frames.each_ref().map(|frame| frame.body_site.node),
                    },
                    budget,
                )?;
                for cleanup in &join.parent_exit.cleanup {
                    budget.step(1)?;
                    if let Cleanup::DisposeListI32 {
                        implementation,
                        bridge,
                        ..
                    } = cleanup
                    {
                        emit(implementation, ListI32Disposal { bridge }, budget)?;
                    }
                }
            }
            Operation::CopyClosure { ref captures, .. } => {
                budget.step(captures.len())?;
                for capture in captures {
                    if let CaptureCopyEffect::Scalar { implementation, .. } = capture {
                        emit(implementation, OrdinaryCall, budget)?;
                    }
                }
            }
            Operation::CallScalarCopy { implementation, .. }
            | Operation::CallScalarDispose { implementation, .. } => {
                emit(implementation, OrdinaryCall, budget)?
            }
            Operation::DisposeText { element, .. } => emit(
                element.implementation,
                ListTextElementDisposal {
                    bridge: element.bridge,
                },
                budget,
            )?,
            Operation::Dispose { callee, .. } => emit(callee, BuiltinDispose, budget)?,
            Operation::ConstructResultError {
                constructor,
                variant,
                wrapper,
                ..
            } => emit(
                constructor,
                block
                    .fresh_result_storage
                    .map_or(ResultConstructor, |storage_source| FreshResultConstructor {
                        variant,
                        wrapper,
                        storage_source,
                    }),
                budget,
            )?,
            Operation::WrapResultFailure { cause_display, .. } => {
                emit(cause_display, RetainedErrorDisplay, budget)?
            }
            Operation::Cleanup(ref actions) => {
                budget.step(actions.len())?;
                for action in actions {
                    match *action {
                        Cleanup::DropFreshResultError {
                            origin,
                            variant,
                            wrapper,
                            storage_source,
                            ..
                        } => emit(
                            wrapper,
                            FreshResultErrorDisposal {
                                origin: origin.node,
                                variant,
                                storage_source,
                            },
                            budget,
                        )?,
                        Cleanup::InvokeDisposer { implementation, .. } => {
                            emit(implementation, ActiveDisposer, budget)?
                        }
                        Cleanup::ReleaseGuard {
                            implementation,
                            entry,
                            ..
                        } => emit(implementation, ProtectedRelease { entry }, budget)?,
                        Cleanup::DisposeListI32 {
                            implementation,
                            bridge,
                            ..
                        } => emit(implementation, ListI32Disposal { bridge }, budget)?,
                        Cleanup::DisposeOwnedListIterI32 {
                            implementation,
                            bridge,
                            ..
                        } => emit(
                            implementation,
                            ListI32Iteration {
                                operation: CollectionOperation::DisposeIterator(Owned),
                                bridge,
                            },
                            budget,
                        )?,
                        Cleanup::DisposeListText {
                            implementation,
                            bridge,
                            element,
                            ..
                        }
                        | Cleanup::DisposeOwnedListIterText {
                            implementation,
                            bridge,
                            element,
                            ..
                        } => {
                            emit(
                                implementation,
                                ListTextCollection {
                                    operation: if matches!(action, Cleanup::DisposeListText { .. })
                                    {
                                        CollectionOperation::DisposeList
                                    } else {
                                        CollectionOperation::DisposeIterator(Owned)
                                    },
                                    bridge,
                                },
                                budget,
                            )?;
                            emit(
                                element.implementation,
                                ListTextElementDisposal {
                                    bridge: element.bridge,
                                },
                                budget,
                            )?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
