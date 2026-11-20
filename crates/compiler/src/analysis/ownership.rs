//! Ownership checking and cleanup MIR for a checked function body.
use super::DeclarationFacts;
use super::declarations::Budget;
use super::scalars::{NodeFactKind, NodeFacts, Site};
use super::typed::{TypedReceipt, TypedReport};
use super::{
    AnalysisLimits, DeclarationShape, InvocationMode, Module, NodeId, QueryError, Statistics, Type,
    TypeTerm, retained_bool_contents_type,
};
use crate::loader::{Declaration, DeclarationId, DeclarationKind};
use crate::source::Span;
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
    disposer_bodies: BTreeMap<&'g DeclarationId, Mir<'g>>,
}

impl<'g> Mir<'g> {
    /// On the root MIR, look up an active disposer reached by this query.
    pub fn disposer(&self, implementation: &DeclarationId) -> Option<&Mir<'g>> {
        self.disposer_bodies.get(implementation)
    }
}

#[derive(Debug)]
pub struct Block<'g> {
    pub site: Site<'g>,
    pub operation: Operation<'g>,
    pub terminator: Terminator,
}

/// A named or temporary storage root, optionally projected to member storage.
#[derive(Clone, Copy, Debug)]
pub struct Place<'g> {
    pub root: Site<'g>,
    pub projection: Option<Projection<'g>>,
}

#[derive(Clone, Copy, Debug)]
pub enum Projection<'g> {
    Field(&'g Declaration),
    /// A leaf of a two-level owned record, relative to the owned root.
    NestedField {
        outer: &'g Declaration,
        inner: &'g Declaration,
    },
    TupleElement(usize),
    /// Two tuple positions relative to an owned root or a descriptor's referent.
    NestedTupleElement {
        outer: usize,
        inner: usize,
    },
}

impl<'g> Place<'g> {
    pub fn field(self) -> Option<&'g Declaration> {
        match self.projection {
            Some(Projection::Field(field)) => Some(field),
            _ => None,
        }
    }

    pub fn tuple_element(self) -> Option<usize> {
        match self.projection {
            Some(Projection::TupleElement(position)) => Some(position),
            _ => None,
        }
    }
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
    /// Copy a scalar through a statement-local borrow into its destination.
    CopyReferent {
        from: Place<'g>,
        into: Place<'g>,
    },
    /// Bind a fixed projection relative to a borrow's referent.
    /// No mode moves a member out of the aggregate.
    BindBorrowedMember {
        descriptor: Place<'g>,
        projection: Projection<'g>,
        into: Place<'g>,
        binding: BorrowedMemberBinding,
    },
    FormClosure {
        closure: Site<'g>,
        capture: Site<'g>,
        mode: InvocationMode,
    },
    /// Form a pair environment from the two already-staged captures, in order.
    FormClosurePair {
        closure: Site<'g>,
        captures: [Site<'g>; 2],
        mode: InvocationMode,
    },
    Call(&'g DeclarationId),
    /// Call with a staged shared record descriptor. The result may retain the
    /// whole record access and the sources carried by its contents.
    CallBorrowingRecord {
        callee: &'g DeclarationId,
        argument: Place<'g>,
        into: Place<'g>,
    },
    /// Evaluate a direct producer into the statement's owned backing storage.
    CallIntoTemporary {
        callee: &'g DeclarationId,
        arguments: Vec<Place<'g>>,
        into: Place<'g>,
    },
    /// Consume a known once environment and transfer its two captured values
    /// into the corresponding result tuple positions.
    InvokeClosurePair {
        receiver: Place<'g>,
        origin: Site<'g>,
        into: Place<'g>,
    },
    /// A direct call may retain an input's backing in the destination's
    /// contents, including the exclusive restriction on a shared view.
    CallRetaining {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        input: Place<'g>,
    },
    /// A direct call can retain either of two compatible Bool inputs.
    /// Input stages are listed in parameter order, omitting the destination.
    CallRetainingPair {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        inputs: [Place<'g>; 2],
    },
    /// `arguments` lists already-staged operands in formal parameter order.
    /// Only compatible reference inputs contribute backing; the Bool stage carries none.
    CallRetainingWithBool {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        arguments: Vec<Place<'g>>,
        input: Place<'g>,
        second_input: Option<Place<'g>>,
    },
    /// A direct call with staged `(destination, seed, gate)` in formal order.
    /// Only `seed` can add backing to the destination. Neither the canonical
    /// `Option<bool>` gate nor the result carries loans.
    CallRetainingWithOptionBool {
        callee: &'g DeclarationId,
        destination: RetainedPlace<'g>,
        arguments: [Place<'g>; 3],
        into: Place<'g>,
    },
    /// Select the success variant of a staged propagation or wrapping operand.
    TestVariant {
        operand: Place<'g>,
        success: &'g DeclarationId,
    },
    CopyOptionPayload {
        operand: Place<'g>,
        into: Place<'g>,
    },
    /// Consume a proved Some carrier and move its owning payload into a slot.
    TakeOptionPayload {
        operand: Place<'g>,
        into: Place<'g>,
    },
    /// Consume a proved None carrier without dropping a payload.
    EndEmptyOption(Place<'g>),
    ConstructVariant {
        variant: &'g DeclarationId,
        payload: Option<Place<'g>>,
        into: Place<'g>,
    },
    /// Consume the payload into a fresh, cause-free `Result.Err` in `into`.
    /// Its origin is this block's source snapshot and `site.span.start()`.
    ConstructResultError {
        constructor: &'g DeclarationId,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        payload: Place<'g>,
        into: Place<'g>,
    },
    /// Consume a proved Ok operand after transferring or discarding its payload.
    TakeResultSuccess {
        operand: Place<'g>,
        into: Option<Place<'g>>,
    },
    /// Move the existing Err wrapper, including its origin and retained cause.
    ForwardResultFailure {
        operand: Place<'g>,
        into: Place<'g>,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
    },
    /// Consume the staged failed Result and outer payload into a new
    /// `Result.Err` in `into`. The cause keeps the inner origin and chain;
    /// only the outer wrapper gets `origin`. `cause_display` names the
    /// retained renderer, not a call during wrapping. The inner i32 uses
    /// scalar cleanup.
    WrapResultFailure {
        operand: Place<'g>,
        payload: Place<'g>,
        into: Place<'g>,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        origin: &'g Span,
        cause_display: &'g DeclarationId,
    },
    Dispose {
        callee: &'g DeclarationId,
        operand: Place<'g>,
    },
    Install {
        target: Place<'g>,
        incoming: Place<'g>,
    },
    /// Copy the shared reference stored through the exclusive descriptor.
    CopyThrough {
        descriptor: Place<'g>,
        into: Option<Place<'g>>,
    },
    /// Replace the contents of an exact local or caller destination through
    /// its initialized exclusive access descriptor.
    InstallThrough {
        descriptor: Place<'g>,
        incoming: Place<'g>,
    },
    /// Install at `**parameter`, leaving the exclusive descriptor at
    /// `*parameter` in place.
    InstallThroughTwoHop {
        parameter: Place<'g>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowedMemberBinding {
    /// Copy a primitive value with no carried backing.
    CopyScalar,
    /// Copy a stored shared reference with carried dependencies, not the
    /// loan used to access the enclosing aggregate.
    CopyReference,
    /// Borrow the stored reference's caller-owned slot without copying it.
    SharedSlot,
    /// Retain the original backing for a shared view of the member.
    SharedView,
    /// Consume the fresh staged descriptor and transfer its original
    /// exclusive restriction to the sole writable member view.
    ExclusiveView,
}

#[derive(Clone, Copy, Debug)]
pub enum RetainedPlace<'g> {
    Local(Place<'g>),
    ThroughParameter(Place<'g>),
    /// The final contents at `**parameter`, passed via the staged exclusive
    /// reborrow of `parameter`. The descriptor at `*parameter` stays in place.
    ThroughTwoHopParameter {
        parameter: Place<'g>,
        argument: Place<'g>,
    },
    ThroughAlias {
        descriptor: Place<'g>,
        target: Place<'g>,
    },
    /// A local binding holds the exclusive access moved from this parameter.
    ThroughParameterAlias {
        descriptor: Place<'g>,
        parameter: Place<'g>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Cleanup<'g> {
    EndStorage(Place<'g>),
    Drop(Place<'g>),
    /// Consume an owned `Option<string>`, dropping its string only in `some`.
    DropOptionPayload {
        operand: Place<'g>,
        some: &'g DeclarationId,
    },
    DropCapture {
        closure: Site<'g>,
        field: Site<'g>,
    },
    InvokeDisposer {
        operand: Place<'g>,
        implementation: &'g DeclarationId,
    },
    /// Consume the checked LockGuard<i32> through its protected release.
    ReleaseGuard {
        operand: Place<'g>,
        implementation: &'g DeclarationId,
        entry: &'g DeclarationId,
    },
    /// Transfer the initialized remainder to the active disposer's epilogue.
    DeferDisposerFields {
        from: Place<'g>,
        into: Place<'g>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Terminator {
    Goto(usize),
    Branch { yes: usize, no: usize },
    Return,
}

/// Lower the selected body after validating reached direct calls and disposers.
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
                   pending: &mut VecDeque<(&'g Module, &'g DeclarationId, bool)>,
                   seen: &mut BTreeSet<&'g DeclarationId>,
                   budget: &mut Budget|
     -> ResultOf<()> {
        for block in &mir.blocks {
            budget.step(1)?;
            let call = match block.operation {
                Operation::Call(callee)
                | Operation::CallBorrowingRecord { callee, .. }
                | Operation::CallIntoTemporary { callee, .. }
                | Operation::CallRetaining { callee, .. }
                | Operation::CallRetainingPair { callee, .. }
                | Operation::CallRetainingWithBool { callee, .. }
                | Operation::CallRetainingWithOptionBool { callee, .. } => Some((callee, false)),
                _ => None,
            };
            let cleanup = match &block.operation {
                Operation::Cleanup(actions) => actions.as_slice(),
                _ => &[],
            };
            budget.step(cleanup.len())?;
            for (callee, is_disposer) in
                call.into_iter()
                    .chain(cleanup.iter().filter_map(|action| match action {
                        Cleanup::InvokeDisposer { implementation, .. } => {
                            Some((*implementation, true))
                        }
                        _ => None,
                    }))
            {
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
                pending.push_back((target, callee, is_disposer));
            }
        }
        Ok(())
    };

    budget.reserve(1).map_err(|()| (module, Stop::Limit))?;
    let mut seen = BTreeSet::from([&header.declaration.id]);
    let mut pending = VecDeque::new();
    let mut root = lower(typed, module, header, false, budget).map_err(|stop| (module, stop))?;
    reached(&root, &mut pending, &mut seen, budget).map_err(|stop| (module, stop))?;
    while let Some((target, id, is_disposer)) = pending.pop_front() {
        budget.step(1).map_err(|()| (target, Stop::Limit))?;
        let missing = (module, Stop::Pending(header.declaration.node));
        let selected = if is_disposer {
            let family = typed
                .dispose_role()
                .and_then(|role| typed.inventory().captured_family(&role.id))
                .ok_or((module, Stop::Pending(header.declaration.node)))?;
            budget
                .step(family.implementations.len())
                .map_err(|()| (target, Stop::Limit))?;
            family
                .implementations
                .iter()
                .find(|body| body.source.declaration == *id)
                .map(|body| &body.header)
        } else {
            typed.inventory().type_facts().declarations.get(id)
        }
        .ok_or(missing)?;
        let mir =
            lower(typed, target, selected, is_disposer, budget).map_err(|stop| (target, stop))?;
        reached(&mir, &mut pending, &mut seen, budget).map_err(|stop| (target, stop))?;
        if is_disposer {
            budget.reserve(1).map_err(|()| (target, Stop::Limit))?;
            root.disposer_bodies.insert(id, mir);
        }
    }
    Ok(root)
}

fn lower<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    header: &DeclarationFacts<'g>,
    start_disposer: bool,
    budget: &mut Budget,
) -> ResultOf<Mir<'g>> {
    let entry_site = header.declaration.node;
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return Err(Stop::Pending(entry_site));
    };
    let Some(&body) = module.syntax.nodes[entry_site].children.get(1) else {
        return Err(Stop::Pending(entry_site));
    };
    let mut retains_exclusive_bool = false;
    if matches!(parameters.len(), 2..=4) {
        budget.step(parameters.len())?;
        if let Some(destination) = parameters
            .iter()
            .position(|parameter| retained_bool_contents_type(&parameter.ty).is_some())
        {
            retains_exclusive_bool =
                retained_bool_contract(parameters, result, destination, budget)?
                    .is_some_and(|contract| contract.exclusive_source);
        }
    }
    let two_hop = retained_two_hop_bool_contract(header);
    let mut build = Build {
        typed,
        module,
        body,
        facts: BTreeMap::new(),
        locals: BTreeMap::new(),
        nested_record_values: BTreeMap::new(),
        nested_tuple_parameters: BTreeMap::new(),
        nested_tuple_bindings: BTreeMap::new(),
        retained: BTreeMap::new(),
        retains_exclusive_bool,
        incoming_exclusive_bools: Vec::new(),
        slots: Vec::new(),
        code: Vec::new(),
        calls: Vec::new(),
        cleanups: Vec::new(),
        dispose: typed.dispose_role().map(|d| &d.id),
        return_contract: (&header.declaration.id, result),
        proved_variant_cleanup: Vec::new(),
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
    let mut owns_guard = false;
    for parameter in parameters {
        if matches!(parameter.ty.as_ref(), TypeTerm::Callable { .. }) {
            return Err(Stop::Pending(parameter.declaration.node));
        }
        let slot = build.parameter(parameter)?;
        params.push(slot);
        owns_guard |= build.slots[slot].contains_guard();
        if let Some(ty) = retained_bool_contents_type(&parameter.ty) {
            let contents = build.slot(parameter.declaration.node, ty)?;
            build.budget.reserve(1)?;
            build
                .retained
                .insert(slot, RetainedContents::OneHop(contents));
        } else if let Some((destination, ty)) = two_hop
            && destination.declaration.id == parameter.declaration.id
        {
            let contents = build.slot(parameter.declaration.node, ty)?;
            build.budget.reserve(1)?;
            build
                .retained
                .insert(slot, RetainedContents::TwoHop(contents));
        } else if matches!(parameter.ty.as_ref(),
            TypeTerm::Reference { mutable: true, referent }
                if matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
        {
            build.budget.reserve(1)?;
            build.incoming_exclusive_bools.push(slot);
        }
    }
    if owns_guard {
        // The scoped release contract does not establish mixed borrowed
        // guard routes. Leave the existing reference-only bodies unchanged.
        build.budget.step(parameters.len())?;
        for parameter in parameters {
            if let TypeTerm::Reference { referent, .. } = parameter.ty.as_ref()
                && build.guard_bearing_type(referent)?
            {
                return Err(Stop::Pending(parameter.declaration.node));
            }
        }
    }
    let active = if start_disposer {
        let ([slot], [parameter]) = (params.as_slice(), parameters.as_slice()) else {
            return Err(Stop::Pending(entry_site));
        };
        if build.slots[*slot].custom_disposer != Some(&header.declaration.id) {
            return Err(Stop::Pending(entry_site));
        }
        let remainder = build.slot(body, &parameter.ty)?;
        Some(DisposerFrame {
            operand: *slot,
            remainder,
        })
    } else {
        None
    };
    let end = build.emit(body, Action::None, Edges::End)?;
    let end = match active {
        Some(frame) => build.cleanup(body, &[frame.remainder], end)?,
        None => end,
    };
    let finish = build.cleanup_scope(body, &params, end)?;
    let entry = build.statement(
        body,
        finish,
        Exits {
            returning: finish,
            looping: None,
        },
        1,
    )?;
    flow::run(build, entry, &params, active)
}

fn retained_two_hop_bool_type<'a, 'g>(ty: &'a TypeTerm<'g>) -> Option<&'a Type<'g>> {
    let TypeTerm::Reference {
        mutable: true,
        referent,
    } = ty
    else {
        return None;
    };
    retained_bool_contents_type(referent)
}

fn retained_two_hop_bool_contract<'a, 'g>(
    header: &'a DeclarationFacts<'g>,
) -> Option<(&'a super::MemberType<'g>, &'a Type<'g>)> {
    if header.declaration.id.kind != DeclarationKind::Function || !header.binders.is_empty() {
        return None;
    }
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return None;
    };
    let [destination, source] = parameters.as_slice() else {
        return None;
    };
    let contents = retained_two_hop_bool_type(&destination.ty)?;
    (source.ty == *contents
        && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void"))
    .then_some((destination, contents))
}

struct RetainedBoolContract<'a, 'g> {
    contents: &'a Type<'g>,
    extra: Option<RetainedCallExtra>,
    exclusive_source: bool,
}

#[derive(Clone, Copy)]
enum RetainedCallExtra {
    Bool { position: usize },
    OptionBool,
}

// Void calls: one destination, one or two compatible sources and at most one
// independent Bool. Check the same declared contract at calls and exits.
fn retained_bool_contract<'a, 'g>(
    parameters: &'a [super::MemberType<'g>],
    result: &Type<'g>,
    destination: usize,
    budget: &mut Budget,
) -> ResultOf<Option<RetainedBoolContract<'a, 'g>>> {
    if !matches!(parameters.len(), 2..=4)
        || !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
    {
        return Ok(None);
    }
    budget.step(1)?;
    let Some(contents) = parameters
        .get(destination)
        .and_then(|formal| retained_bool_contents_type(&formal.ty))
    else {
        return Ok(None);
    };
    let mut scalar = None;
    let mut sources = 0;
    let mut exclusive_source = false;
    for (index, formal) in parameters.iter().enumerate() {
        budget.step(1)?;
        if index == destination {
            continue;
        }
        if formal.ty == *contents {
            sources += 1;
        } else if matches!((contents.as_ref(), formal.ty.as_ref()), (
            TypeTerm::Reference { referent: stored, .. },
            TypeTerm::Reference { mutable: true, referent: source },
        ) if stored == source)
        {
            sources += 1;
            exclusive_source = true;
        } else if matches!(formal.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            && scalar.is_none()
        {
            scalar = Some(index);
        } else {
            return Ok(None);
        }
    }
    Ok(matches!(sources, 1 | 2).then_some(RetainedBoolContract {
        contents,
        extra: scalar.map(|position| RetainedCallExtra::Bool { position }),
        exclusive_source,
    }))
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
struct DisposerFrame {
    operand: Slot,
    remainder: Slot,
}

#[derive(Clone, Copy)]
struct Local<'g> {
    node: NodeId,
    /// Lexical cleanup root, including for leaves inside a nested child.
    owner: Slot,
    projection: Option<Projection<'g>>,
    /// Number of trailing slots in this region, not just direct fields.
    members: usize,
    reference_member: bool,
    has_guard_field: bool,
    custom_disposer: Option<&'g DeclarationId>,
    /// Materialized backing or descriptor of a computed scalar borrow.
    scalar_borrow_temporary: bool,
    moves: bool,
    cleanup: StorageCleanup<'g>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct GuardRelease<'g> {
    implementation: &'g DeclarationId,
    entry: &'g DeclarationId,
}

impl<'g> GuardRelease<'g> {
    fn cleanup(self, operand: Place<'g>) -> Cleanup<'g> {
        Cleanup::ReleaseGuard {
            operand,
            implementation: self.implementation,
            entry: self.entry,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageCleanup<'g> {
    End,
    Drop,
    DropOptionPayload(&'g DeclarationId),
    Unlowered,
    ReleaseGuard(GuardRelease<'g>),
}

impl<'g> Local<'g> {
    fn storage(self, slot: Slot) -> Range<Slot> {
        slot..slot + 1 + self.members
    }

    fn needs_boundary_cleanup(self) -> bool {
        self.members > 0 || self.custom_disposer.is_some() || self.guard_release().is_some()
    }

    fn guard_release(self) -> Option<GuardRelease<'g>> {
        match self.cleanup {
            StorageCleanup::ReleaseGuard(release) => Some(release),
            _ => None,
        }
    }

    fn contains_guard(self) -> bool {
        self.has_guard_field || self.guard_release().is_some()
    }

    fn carried_loan_root(self, slot: Slot) -> Slot {
        if self.reference_member {
            self.owner
        } else {
            slot
        }
    }

    fn place(self, module: &'g Module) -> Place<'g> {
        Place {
            root: site_at(module, self.node),
            projection: self.projection,
        }
    }
}

#[derive(Clone, Copy)]
enum Action<'g> {
    None,
    Init(Slot),
    Read {
        from: Slot,
        into: Option<Slot>,
    },
    Move {
        from: Slot,
        into: Option<Slot>,
    },
    CheckRebuild {
        base: Slot,
        // None selects the base; Some is the contiguous stage range at commit.
        staged: Option<(Slot, usize)>,
    },
    CheckNestedFieldBase(Slot),
    UnloweredRebuildRead(Slot),
    ForbiddenBorrowedMove(Slot),
    Borrow {
        from: Slot,
        into: Slot,
        exclusive: bool,
        retained_target: bool,
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
    BindBorrowedMember {
        descriptor: Slot,
        projection: Projection<'g>,
        into: Slot,
        binding: BorrowedMemberBinding,
    },
    Call(usize),
    InvokeClosurePair {
        receiver: Slot,
        into: Slot,
    },
    TestVariant {
        operand: Slot,
        success: &'g DeclarationId,
    },
    CopyOptionPayload {
        operand: Slot,
        into: Slot,
    },
    TakeOptionPayload {
        operand: Slot,
        into: Slot,
    },
    EndEmptyOption(Slot),
    ConstructVariant {
        variant: &'g DeclarationId,
        payload: Option<Slot>,
        into: Slot,
    },
    ConstructResultError {
        constructor: &'g DeclarationId,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        payload: Slot,
        into: Slot,
    },
    TakeResultSuccess {
        operand: Slot,
        into: Option<Slot>,
    },
    ForwardResultFailure {
        operand: Slot,
        into: Slot,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
    },
    WrapResultFailure {
        operand: Slot,
        payload: Slot,
        into: Slot,
        variant: &'g DeclarationId,
        wrapper: &'g DeclarationId,
        origin: &'g Span,
        cause_display: &'g DeclarationId,
    },
    FormClosure {
        capture: Slot,
        second: Option<Slot>,
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
    CopyThrough {
        descriptor: Slot,
        into: Option<Slot>,
    },
    InstallThrough {
        descriptor: Slot,
        incoming: Slot,
    },
    InstallThroughTwoHop {
        route: OriginalTwoHopBool,
        incoming: Slot,
    },
    CheckRetained,
    End(Slot),
    Cleanup(usize),
    EndScope(usize),
    Export(Slot),
}

#[derive(Clone, Copy)]
enum Edges {
    Next(Pc),
    Branch { yes: Pc, no: Pc },
    End,
}

#[derive(Clone, Copy)]
struct Instruction<'g> {
    node: NodeId,
    action: Action<'g>,
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
    body: NodeId,
    facts: BTreeMap<NodeId, &'a NodeFacts<'g>>,
    locals: BTreeMap<&'g DeclarationId, Slot>,
    /// Checked nested regions expose value transfers, not general places.
    nested_record_values: BTreeMap<&'g DeclarationId, Slot>,
    /// Two-level owned tuple inputs kept out of general place resolution.
    nested_tuple_parameters: BTreeMap<&'g DeclarationId, Slot>,
    /// Exact named tuple bindings with their proved initializer slot.
    nested_tuple_bindings: BTreeMap<&'g DeclarationId, (Slot, Slot)>,
    /// Separate contents of checked caller destinations from the access descriptors.
    retained: BTreeMap<Slot, RetainedContents>,
    retains_exclusive_bool: bool,
    incoming_exclusive_bools: Vec<Slot>,
    slots: Vec<Local<'g>>,
    code: Vec<Instruction<'g>>,
    calls: Vec<Call<'g>>,
    cleanups: Vec<Vec<Slot>>,
    dispose: Option<&'g DeclarationId>,
    return_contract: (&'g DeclarationId, &'a Type<'g>),
    proved_variant_cleanup: Vec<Type<'g>>,
    budget: &'a mut Budget,
}

#[derive(Clone, Copy)]
enum RetainedContents {
    OneHop(Slot),
    TwoHop(Slot),
}

impl RetainedContents {
    fn slot(self) -> Slot {
        match self {
            Self::OneHop(slot) | Self::TwoHop(slot) => slot,
        }
    }
}

/// The original exclusive route authorizes the store. Its inner descriptor
/// is never resolved as a separate value or replaced by the final contents.
#[derive(Clone, Copy)]
struct OriginalTwoHopBool {
    descriptor: Slot,
    contents: Slot,
}

struct Call<'g> {
    callee: &'g DeclarationId,
    args: Vec<Slot>,
    into: Option<Slot>,
    result: CallResult,
    destination: Option<CallDestination>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallResult {
    Plain,
    RetainsInputs,
    RetainsRecordInput,
}

#[derive(Clone, Copy)]
struct CallDestination {
    storage: CallStorage,
    argument: Slot,
    input: Slot,
    second_input: Option<Slot>,
    extra: Option<RetainedCallExtra>,
}

#[derive(Clone, Copy)]
enum CallStorage {
    Local(Slot),
    ThroughParameter { descriptor: Slot, contents: Slot },
    ThroughTwoHopParameter(OriginalTwoHopBool),
    ThroughAlias { descriptor: Slot },
}
