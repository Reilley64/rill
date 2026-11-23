//! Compute loan liveness, initialized states and path-specific cleanup.
use super::*;
use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

pub(super) mod forwarding;
pub(super) mod scoped_join;

// Retain distinct continuing states at a join. Every feasible incoming state
// must authorize a read, and each MIR successor cleans only what it still owns.
#[derive(Clone, Debug, Eq, PartialEq)]
enum State {
    Vacant,
    Initialized(Held),
    Moved(NodeId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Held {
    loans: Arc<[Loan]>,
    capture_drops: [Option<NodeId>; 2],
    callable_owner: Option<CallableOwner>,
    /// A checked pair body acquired at this local arrow, not just a callable type.
    pair_origin: Option<(NodeId, InvocationMode)>,
    /// The checked one-capture body of the environment currently in this slot.
    single_body: Option<SingleBody>,
    /// The original parameter of an already-running outer disposer.
    disposal_started_at: Option<NodeId>,
    /// Provenance accompanies the value, not the slot. Retained descriptors
    /// and fresh Results are disjoint admitted storage shapes.
    origin: ValueOrigin,
    /// Identity of the stored descriptor, independent of the access loans.
    exact_target: Option<ExactTarget>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ValueOrigin {
    #[default]
    Untracked,
    RetainedStore(NodeId),
    FreshResultOk,
    FreshResultErr(NodeId),
}

impl ValueOrigin {
    fn stored_at(self) -> Option<NodeId> {
        match self {
            Self::RetainedStore(node) => Some(node),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallableOwner {
    LocalCapture(NodeId),
    ListArrow(NodeId),
    ScopedValue(NodeId),
    LocalExclusive(Option<CallableLayout>),
    Pair(CallableLayout),
    Formal(CallableLayout),
    Returned(CallableLayout),
}

impl CallableOwner {
    fn transport_cleanup(self) -> Option<CallableLayout> {
        match self {
            Self::LocalCapture(_)
            | Self::ListArrow(_)
            | Self::ScopedValue(_)
            | Self::LocalExclusive(_)
            | Self::Pair(_) => None,
            Self::Formal(layout) | Self::Returned(layout) => Some(layout),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingleBody {
    MutableCell(MutableCell),
    /// A read-only body that is its sole capture, acquired at this local
    /// arrow, so an invocation copies that value.
    Scalar(NodeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MutableCell {
    origin: NodeId,
    field: NodeId,
    addend: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactTarget {
    Local(Slot),
    Caller { parameter: Slot, contents: Slot },
}

impl ExactTarget {
    fn contents_slot(self) -> Slot {
        match self {
            Self::Local(slot) | Self::Caller { contents: slot, .. } => slot,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Loan {
    source: Slot,
    site: NodeId,
    exclusive: bool,
    kind: LoanKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum LoanKind {
    Backing,
    Contents,
    Suspension,
    // External backing carried by an incoming exclusive Bool descriptor,
    // not a borrow of the parameter's local storage.
    IncomingExclusive,
    // Carried by an incoming shared i32 descriptor, not a loan of its slot.
    IncomingSharedI32,
    // Shared access to a caller-owned field slot, keyed conservatively by
    // the incoming parameter, not by the parameter's local storage.
    IncomingSharedSlot,
    // Backing carried by an incoming owned Option<&string>, not its slot.
    IncomingSharedOptionBacking,
    // Opaque caller backing, not a loan of the formal's storage.
    IncomingCallableBacking,
}

impl LoanKind {
    fn is_incoming(self) -> bool {
        matches!(
            self,
            Self::IncomingExclusive
                | Self::IncomingSharedI32
                | Self::IncomingSharedSlot
                | Self::IncomingSharedOptionBacking
                | Self::IncomingCallableBacking
        )
    }

    fn borrows_local_storage(self) -> bool {
        matches!(self, Self::Backing | Self::Contents)
    }
}

#[derive(Clone, Copy)]
enum Access {
    Observe,
    Exclusive,
    // Check access through this holder's single exclusive backing loan.
    ThroughExclusive(Slot),
    EndScope,
}

impl Edges {
    fn successors(self) -> impl Iterator<Item = Pc> {
        let (first, second) = match self {
            Self::Next(pc) => (Some(pc), None),
            Self::Branch { yes, no } => (Some(yes), Some(no)),
            Self::End | Self::FinishStartup | Self::FinishExpectation => (None, None),
        };
        first.into_iter().chain(second)
    }
}

// This backward pass tracks possible future reads. A transfer to a slot or
// leaving its scope kills liveness of the old version. The only cleanup that
// can observe a borrowed value is the custom disposer of a record with a
// reference member (`observing`). Its displacement reads the old value. Its
// scope cleanup reads it on entry: the record stays live up to that boundary,
// which itself retires dependents before their sources.
fn liveness(
    slots: &[Local<'_>],
    code: &[Instruction<'_>],
    calls: &[Call<'_, '_>],
    cleanups: &[Vec<Slot>],
    retained: &BTreeMap<Slot, RetainedContents>,
    observing: &[Slot],
    budget: &mut Budget,
) -> ResultOf<Vec<BTreeSet<Slot>>> {
    budget.reserve(3 * code.len())?;
    let mut live = vec![BTreeSet::new(); code.len()];
    let mut users = vec![Vec::new(); code.len()];
    for (pc, instruction) in code.iter().enumerate() {
        budget.step(1)?;
        for next in instruction.edges.successors() {
            budget.reserve(1)?;
            users[next].push(pc);
        }
    }
    // Screened by the build: no observing root, no added work.
    let mut cleanup_reads = Vec::new();
    if !observing.is_empty() {
        budget.reserve(code.len())?;
        cleanup_reads.resize(code.len(), Vec::new());
        for (pc, instruction) in code.iter().enumerate() {
            let (Action::Cleanup(index) | Action::EndScope(index)) = instruction.action else {
                continue;
            };
            for &root in observing {
                budget.step(1)?;
                if cleanups[index].binary_search(&root).is_ok() {
                    budget.reserve(1)?;
                    cleanup_reads[pc].push(root);
                }
            }
        }
    }
    let mut pending: VecDeque<_> = (0..code.len()).collect();
    while let Some(pc) = pending.pop_front() {
        budget.step(1)?;
        let instruction = code[pc];
        let use_one = |used, killed| ([used, None], killed);
        let (used, killed) = match instruction.action {
            Action::None
            | Action::BudgetBackedge { .. }
            | Action::Cleanup(_)
            | Action::EndScope(_)
            | Action::CheckRetained => use_one(None, None),
            Action::CheckScopedReferenceResults { entries, .. } => (entries.map(Some), None),
            Action::Init(slot) | Action::End(slot) => use_one(None, Some(slot)),
            Action::CommitList { into, .. } => use_one(None, Some(into)),
            Action::OpenOwnedList { source, cursor, .. } => use_one(Some(source), Some(cursor)),
            Action::AdvanceOwnedList { cursor, into, .. } => use_one(Some(cursor), into),
            Action::Read { from, into }
            | Action::Move { from, into }
            | Action::Dereference { from, into, .. } => {
                use_one(Some(slots[from].carried_loan_root(from)), into)
            }
            Action::CheckRebuild { base, .. }
            | Action::UnloweredRebuildRead(base)
            | Action::ForbiddenBorrowedMove(base) => use_one(Some(base), None),
            Action::CheckNestedFieldBase(child) => ([Some(slots[child].owner), Some(child)], None),
            Action::Borrow { from, into, .. } => use_one(Some(from), Some(into)),
            Action::BindBorrowedMember {
                descriptor, into, ..
            } => use_one(Some(descriptor), Some(into)),
            Action::ReadListI32 { view, into, .. } => use_one(Some(view), into),
            Action::EqualI32Literal { from, into, .. } => use_one(Some(from), Some(into)),
            Action::CheckedI32Multiply { left, into, .. } => use_one(Some(left), Some(into)),
            Action::CheckedI32Add { left, right, into } => ([Some(left), Some(right)], Some(into)),
            Action::EqualI32 { left, right, into } => ([Some(left), Some(right)], Some(into)),
            Action::FormatI32Template { into, .. } => use_one(None, Some(into)),
            Action::Reborrow { parent, into, .. } => {
                use_one(Some(slots[parent].carried_loan_root(parent)), Some(into))
            }
            Action::CheckCallableReturn(slot) => use_one(Some(slot), None),
            Action::Dispose { operand: slot, .. }
            | Action::Export(slot)
            | Action::StageScopedValue(slot)
            | Action::EndEmptyOption(slot)
            | Action::DropResultFailure { operand: slot, .. } => use_one(Some(slot), Some(slot)),
            Action::Displace { target, incoming } | Action::Install { target, incoming } => {
                use_one(Some(incoming), Some(target))
            }
            Action::FormClosure {
                capture,
                second,
                into,
                ..
            } => ([Some(capture), second], Some(into)),
            Action::FormListArrow { capture, into, .. } => use_one(Some(capture), Some(into)),
            Action::FormScopedValueArrow { capture, into } => use_one(capture, Some(into)),
            Action::CallListArrows {
                receivers, into, ..
            }
            | Action::ScopedJoin {
                receivers, into, ..
            }
            | Action::OverlappingScopedJoin { receivers, into } => {
                (receivers.map(Some), Some(into))
            }
            Action::Call(index) => use_one(None, calls[index].into),
            Action::CopyClosure { from, into } => use_one(Some(from), Some(into)),
            Action::InvokeClosurePair { receiver, into, .. }
            | Action::InvokeMutableI32Cell { receiver, into }
            | Action::InvokeDeclaredShared {
                argument: receiver,
                into,
                ..
            } => use_one(Some(receiver), Some(into)),
            // External contents are already live at every instruction.
            Action::CopyThrough {
                descriptor, into, ..
            } => use_one(Some(descriptor), into),
            Action::InstallThrough {
                descriptor,
                incoming,
            }
            | Action::InstallThroughTwoHop {
                route: OriginalTwoHopBool { descriptor, .. },
                incoming,
            } => ([Some(descriptor), Some(incoming)], None),
            Action::TestVariant { operand, .. } => use_one(Some(operand), None),
            Action::CopyOptionPayload { operand, into }
            | Action::TakeOptionPayload { operand, into } => use_one(Some(operand), Some(into)),
            Action::ConstructVariant { payload, into, .. } => use_one(payload, Some(into)),
            Action::ConstructResultError { payload, into, .. } => {
                use_one(Some(payload), Some(into))
            }
            Action::TakeResultSuccess { operand, into } => use_one(Some(operand), into),
            Action::ForwardResultFailure { operand, into, .. } => {
                use_one(Some(operand), Some(into))
            }
            Action::WrapResultFailure {
                operand,
                payload,
                into,
                ..
            } => ([Some(operand), Some(payload)], Some(into)),
        };
        let cleaned = match instruction.action {
            Action::Cleanup(index) | Action::EndScope(index) => cleanups[index].as_slice(),
            _ => &[],
        };
        let displaced = match instruction.action {
            Action::Displace { target, .. } if !observing.is_empty() => {
                budget.step(1)?;
                observing.binary_search(&target).ok().map(|_| target)
            }
            _ => None,
        };
        let mut changed = false;
        for next in instruction.edges.successors() {
            // A self-edge adds no new facts to this monotone solution.
            if next == pc {
                continue;
            }
            let (low, high) = live.split_at_mut(pc.max(next));
            let (here, there) = if pc < next {
                (&mut low[pc], &high[0])
            } else {
                (&mut high[0], &low[next])
            };
            let read = cleanup_reads.get(next).into_iter().flatten();
            let read = read.flat_map(|&root| slots[root].storage(root));
            for slot in there.iter().copied().chain(read) {
                budget.step(1)?;
                if !killed.is_some_and(|target| {
                    target == slot
                        || !matches!(instruction.action, Action::Init(_))
                            && slots[target].storage(target).contains(&slot)
                }) && cleaned.binary_search(&slots[slot].owner).is_err()
                    && !here.contains(&slot)
                {
                    budget.reserve(1)?;
                    here.insert(slot);
                    changed = true;
                }
            }
        }
        let args = match instruction.action {
            Action::Call(index) => calls[index].args.as_slice(),
            _ => &[],
        };
        let stages = match instruction.action {
            Action::CommitList { first, count, .. } => {
                first..first + count * slots[first].storage(first).len()
            }
            Action::FormatI32Template { first, count, .. } => first..first + count,
            _ => 0..0,
        };
        // Caller storage can still expose its installed value until a write
        // commits, even if normal local liveness would kill that version.
        for slot in used
            .into_iter()
            .flatten()
            .chain(args.iter().copied())
            .chain(stages)
            .chain(displaced)
            .chain(retained.values().map(|contents| contents.slot()))
        {
            for member in slots[slot].storage(slot) {
                budget.step(1)?;
                if !live[pc].contains(&member) {
                    budget.reserve(1)?;
                    live[pc].insert(member);
                    changed = true;
                }
            }
        }
        if changed {
            budget.reserve(users[pc].len())?;
            pending.extend(&users[pc]);
        }
    }
    Ok(live)
}

fn overlap(slots: &[Local<'_>], a: Slot, b: Slot) -> bool {
    a < slots[b].storage(b).end && b < slots[a].storage(a).end
}

fn conflict(
    slots: &[Local<'_>],
    state: &[State],
    live: &BTreeSet<Slot>,
    source: Slot,
    (site, kind): (NodeId, Conflict),
    access: Access,
    budget: &mut Budget,
) -> ResultOf<()> {
    if let Access::ThroughExclusive(view) = access {
        budget.step(1)?;
        if !matches!(initialized(&state[view], site)?.loans.as_ref(),
            [Loan { source: backing, exclusive: true, kind: LoanKind::Backing, .. }] if *backing == source)
        {
            return Err(Stop::Pending(site));
        }
    }
    let check = |loan: Loan, _: &mut Budget| -> ResultOf<()> {
        // The parameter key for incoming backing is not a local place.
        if loan.kind.is_incoming() {
            return Ok(());
        }
        let blocks = match access {
            Access::Observe => loan.exclusive,
            Access::Exclusive | Access::ThroughExclusive(_) => true,
            Access::EndScope => loan.kind.borrows_local_storage(),
        };
        // Keep the exact member for initialization, backing and MIR, but
        // do not use distinct tuple positions as proof of disjoint access.
        let restriction = if loan.kind.borrows_local_storage()
            && matches!(
                slots[loan.source].projection,
                Some(Projection::TupleElement(_))
            ) {
            slots[loan.source].owner
        } else {
            loan.source
        };
        if blocks && overlap(slots, restriction, source) {
            return Err(Stop::Rejected {
                site,
                related: loan.site,
                kind,
            });
        }
        Ok(())
    };
    for &holder in live {
        budget.step(1)?;
        if matches!(access, Access::ThroughExclusive(view) if view == holder) {
            continue;
        }
        if let State::Initialized(held) = &state[holder] {
            for &loan in held.loans.iter() {
                budget.step(1)?;
                check(loan, budget)?;
            }
            // The alias may later extract a carried reference. Keep the
            // target's current aggregate backing live until that use.
            if let Some(ExactTarget::Local(target)) = held.exact_target {
                budget.step(1)?;
                let root = slots[target].carried_loan_root(target);
                visit_loans(state, slots[root].storage(root), budget, check)?;
            }
        }
    }
    Ok(())
}

fn visit_loans(
    state: &[State],
    storage: Range<Slot>,
    budget: &mut Budget,
    mut visit: impl FnMut(Loan, &mut Budget) -> ResultOf<()>,
) -> ResultOf<()> {
    budget.step(storage.len())?;
    for member in &state[storage] {
        if let State::Initialized(held) = member {
            for &loan in held.loans.iter() {
                budget.step(1)?;
                visit(loan, budget)?;
            }
        }
    }
    Ok(())
}

fn insert_loan(loans: &mut BTreeSet<Loan>, loan: Loan, budget: &mut Budget) -> ResultOf<()> {
    if !loans.contains(&loan) {
        budget.reserve(1)?;
        loans.insert(loan);
    }
    Ok(())
}

fn include_loans(
    loans: &mut BTreeSet<Loan>,
    state: &[State],
    storage: Range<Slot>,
    budget: &mut Budget,
) -> ResultOf<()> {
    visit_loans(state, storage, budget, |loan, budget| {
        insert_loan(loans, loan, budget)
    })
}

fn borrowed_member_value(
    binding: BorrowedMemberBinding,
    held: &Held,
    descriptor: Slot,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<Held> {
    Ok(match binding {
        BorrowedMemberBinding::CopyScalar => Held::default(),
        BorrowedMemberBinding::CopyReference => {
            // Borrow marks every local-storage loan already in
            // the aggregate as Contents. Only Backing describes
            // the new access to the enclosing storage.
            budget.step(held.loans.len())?;
            budget.reserve(held.loans.len())?;
            Held {
                loans: Arc::from_iter(
                    held.loans
                        .iter()
                        .copied()
                        .filter(|loan| loan.kind != LoanKind::Backing),
                ),
                ..Held::default()
            }
        }
        BorrowedMemberBinding::SharedSlot => {
            budget.step(held.loans.len())?;
            budget.reserve(held.loans.len() + 1)?;
            let access = Loan {
                source: descriptor,
                site: node,
                exclusive: false,
                kind: LoanKind::IncomingSharedSlot,
            };
            Held {
                loans: Arc::from_iter(held.loans.iter().copied().chain([access])),
                ..Held::default()
            }
        }
        BorrowedMemberBinding::SharedView | BorrowedMemberBinding::ExclusiveView => Held {
            loans: held.loans.clone(),
            ..Held::default()
        },
    })
}

fn loan_set(loans: BTreeSet<Loan>, budget: &mut Budget) -> ResultOf<Arc<[Loan]>> {
    budget.reserve(loans.len())?;
    Ok(Arc::from_iter(loans))
}

fn aggregate_carried_loans(
    state: &[State],
    slots: &[Local<'_>],
    member: Slot,
    budget: &mut Budget,
) -> ResultOf<Option<Arc<[Loan]>>> {
    let root = slots[member].carried_loan_root(member);
    if root == member {
        return Ok(None);
    }
    let mut loans = BTreeSet::new();
    include_loans(&mut loans, state, slots[root].storage(root), budget)?;
    Ok(Some(loan_set(loans, budget)?))
}

fn initialized(state: &State, site: NodeId) -> ResultOf<&Held> {
    match state {
        State::Initialized(held) => Ok(held),
        State::Moved(related) => Err(Stop::Rejected {
            site,
            related: *related,
            kind: Conflict::Moved,
        }),
        State::Vacant => Err(Stop::Pending(site)),
    }
}

fn shared_pair_owner(held: &Held, node: NodeId, budget: &mut Budget) -> ResultOf<Slot> {
    budget.step(held.loans.len())?;
    // Contents loans describe the closure's stored references, not the
    // descriptor's access to the closure itself. Only one shared backing
    // loan can identify the actual receiver on this path.
    let mut backing = held
        .loans
        .iter()
        .filter(|loan| loan.kind == LoanKind::Backing);
    match (backing.next(), backing.next()) {
        (Some(loan), None) if !loan.exclusive => Ok(loan.source),
        _ => Err(Stop::Pending(node)),
    }
}

fn exact_target(
    state: &[State],
    descriptor: Slot,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<ExactTarget> {
    budget.step(1)?;
    let target = initialized(&state[descriptor], node)?
        .exact_target
        .ok_or(Stop::Pending(node))?;
    if let ExactTarget::Local(slot) = target {
        budget.step(1)?;
        initialized(&state[slot], node)?;
    }
    Ok(target)
}

fn require_initialized(
    slots: &[Local<'_>],
    state: &[State],
    slot: Slot,
    site: NodeId,
    budget: &mut Budget,
) -> ResultOf<()> {
    for member in slots[slot].storage(slot) {
        budget.step(1)?;
        initialized(&state[member], site)?;
    }
    Ok(())
}

fn disposal_origin(state: &State) -> Option<NodeId> {
    match state {
        State::Initialized(held) => held.disposal_started_at,
        _ => None,
    }
}

fn forbid_active_escape(state: &State, site: NodeId) -> ResultOf<()> {
    if let Some(related) = disposal_origin(state) {
        return Err(Stop::Rejected {
            site,
            related,
            kind: Conflict::ForbiddenMove,
        });
    }
    Ok(())
}

fn owed_disposer<'g>(local: Local<'g>, state: &State) -> Option<&'g DeclarationId> {
    local
        .custom_disposer
        .filter(|_| disposal_origin(state).is_none())
}

fn forbid_custom_partial(
    slots: &[Local<'_>],
    state: &[State],
    slot: Slot,
    node: NodeId,
) -> ResultOf<()> {
    let owner = slots[slot].owner;
    if owner != slot && owed_disposer(slots[owner], &state[owner]).is_some() {
        return Err(Stop::Rejected {
            site: node,
            related: slots[owner].node,
            kind: Conflict::ForbiddenMove,
        });
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct CleanupContext<'a, 'g> {
    module: &'g Module,
    disposer: Option<build::DisposerLookup<'a, 'g>>,
}

impl<'g> From<&'g Module> for CleanupContext<'_, 'g> {
    // A cleanup path without the typed lookup cannot certify a fresh error drop.
    fn from(module: &'g Module) -> Self {
        Self {
            module,
            disposer: None,
        }
    }
}

fn cleanup_of<'g>(
    context: CleanupContext<'_, 'g>,
    local: Local<'g>,
    held: &Held,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<Cleanup<'g>> {
    let place = local.place(context.module);
    match local.cleanup {
        StorageCleanup::Unlowered => Err(Stop::Pending(node)),
        StorageCleanup::FreshBoolResult(storage) => match held.origin {
            ValueOrigin::FreshResultOk => Ok(Cleanup::EndStorage(place)),
            ValueOrigin::FreshResultErr(origin) => {
                // Provenance alone permits transfer, not destruction. Both
                // ordinary and delivery exits must select on the actual wrapper.
                context
                    .disposer
                    .ok_or(Stop::Pending(node))?
                    .prove_fresh_error_drop(storage, node, budget)?;
                Ok(Cleanup::DropFreshResultError {
                    operand: place,
                    origin: site_at(context.module, origin),
                    variant: storage.err,
                    wrapper: storage.wrapper,
                    storage_source: storage.storage_source,
                })
            }
            _ => Err(Stop::Pending(node)),
        },
        StorageCleanup::Drop => Ok(Cleanup::Drop(place)),
        StorageCleanup::DropOptionPayload(some) => Ok(Cleanup::DropOptionPayload {
            operand: place,
            some,
        }),
        StorageCleanup::ReleaseGuard(release) => Ok(release.cleanup(place)),
        StorageCleanup::DisposeListI32(disposal) => Ok(disposal.cleanup(place)),
        StorageCleanup::DisposeOwnedListIterI32(disposal) => Ok(Cleanup::DisposeOwnedListIterI32 {
            cursor: place,
            implementation: disposal.implementation,
            bridge: disposal.bridge,
        }),
        StorageCleanup::DisposeListText(disposal) => Ok(Cleanup::DisposeListText {
            operand: place,
            implementation: disposal.list.implementation,
            bridge: disposal.list.bridge,
            element: disposal.element,
        }),
        StorageCleanup::DisposeOwnedListIterText(disposal) => {
            Ok(Cleanup::DisposeOwnedListIterText {
                cursor: place,
                implementation: disposal.list.implementation,
                bridge: disposal.list.bridge,
                element: disposal.element,
            })
        }
        StorageCleanup::DropYieldedOptionText(element) => Ok(Cleanup::DropYieldedOptionText {
            operand: place,
            element,
        }),
        StorageCleanup::DisposeListOptionText { disposal, .. } => {
            Ok(Cleanup::DisposeListOptionText {
                operand: place,
                implementation: disposal.list.implementation,
                bridge: disposal.list.bridge,
                element: disposal.element,
            })
        }
        StorageCleanup::DisposeOwnedListIterOptionText(disposal) => {
            Ok(Cleanup::DisposeOwnedListIterOptionText {
                cursor: place,
                implementation: disposal.list.implementation,
                bridge: disposal.list.bridge,
                element: disposal.element,
            })
        }
        StorageCleanup::DisposeOwnedListIterRecordText(disposal) => {
            Ok(Cleanup::DisposeOwnedListIterRecordText {
                cursor: place,
                implementation: disposal.list.implementation,
                bridge: disposal.list.bridge,
                element: disposal.element,
            })
        }
        StorageCleanup::End => Ok(Cleanup::EndStorage(place)),
    }
}

fn cleanup_operation<'g>(
    context: CleanupContext<'_, 'g>,
    local: Local<'g>,
    state: State,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<Operation<'g>> {
    let module = context.module;
    let State::Initialized(held) = state else {
        return Ok(Operation::None);
    };
    if let Some(layout) = held
        .callable_owner
        .and_then(CallableOwner::transport_cleanup)
    {
        budget.reserve(1)?;
        return Ok(Operation::Cleanup(vec![Cleanup::ConsumeCallableTransfer {
            operand: local.place(module),
            layout,
        }]));
    }
    let closure = local.place(module).root;
    match held.capture_drops {
        [Some(first), Some(second)] => {
            budget.reserve(2)?;
            return Ok(Operation::Cleanup(
                [second, first]
                    .into_iter()
                    .map(|field| Cleanup::DropCapture {
                        closure,
                        field: site_at(module, field),
                    })
                    .collect(),
            ));
        }
        [Some(field), None] | [None, Some(field)] => {
            return Ok(Operation::DropCapture {
                closure,
                field: site_at(module, field),
            });
        }
        [None, None] => {}
    }
    Ok(match cleanup_of(context, local, &held, node, budget)? {
        Cleanup::Drop(place) => Operation::Drop(place),
        Cleanup::DropCapture { closure, field } => Operation::DropCapture { closure, field },
        cleanup @ (Cleanup::DropFreshResultError { .. }
        | Cleanup::DropOptionPayload { .. }
        | Cleanup::DisposeListI32 { .. }
        | Cleanup::DisposeOwnedListIterI32 { .. }
        | Cleanup::DisposeListText { .. }
        | Cleanup::DisposeOwnedListIterText { .. }
        | Cleanup::DisposeListOptionText { .. }
        | Cleanup::DisposeOwnedListIterOptionText { .. }
        | Cleanup::DisposeOwnedListIterRecordText { .. }
        | Cleanup::DropYieldedOptionText { .. }) => {
            budget.reserve(1)?;
            Operation::Cleanup(vec![cleanup])
        }
        cleanup @ Cleanup::EndStorage(_)
            if matches!(held.callable_owner, Some(CallableOwner::LocalExclusive(_)))
                || matches!(local.cleanup, StorageCleanup::FreshBoolResult(_)) =>
        {
            budget.reserve(1)?;
            Operation::Cleanup(vec![cleanup])
        }
        Cleanup::EndStorage(_) => Operation::None,
        Cleanup::InvokeDisposer { .. }
        | Cleanup::ConsumeCallableTransfer { .. }
        | Cleanup::DeferDisposerFields { .. }
        | Cleanup::ReleaseGuard { .. } => {
            unreachable!("these obligations use a cleanup boundary")
        }
    })
}

// Refine a stuck owner schedule only for flat tuples whose remaining
// storage has no observable cleanup. Keep the reverse-position edges and
// add dependencies on exact backing members, not their entire owners.
fn finish_tuple_cleanup<'g>(
    module: &'g Module,
    slots: &[Local<'g>],
    state: &mut [State],
    scope: &[Slot],
    node: NodeId,
    actions: &mut Vec<Cleanup<'g>>,
    budget: &mut Budget,
) -> ResultOf<()> {
    let mut nodes = Vec::new();
    for &owner in scope {
        budget.step(1)?;
        let State::Initialized(root) = &state[owner] else {
            continue;
        };
        if slots[owner].members == 0
            || !matches!(
                slots[owner + 1].projection,
                Some(Projection::TupleElement(_))
            )
            || !root.loans.is_empty()
        {
            return Err(Stop::Pending(node));
        }
        let mut lower = None;
        for slot in slots[owner].storage(owner) {
            budget.step(1)?;
            if !matches!(state[slot], State::Initialized(_)) {
                continue;
            }
            if slots[slot].cleanup != StorageCleanup::End {
                return Err(Stop::Pending(node));
            }
            budget.reserve(1)?;
            let index = nodes.len();
            nodes.push((slot, lower));
            lower = Some(index);
        }
    }
    // The slot order is declaration order, then ascending position. Pop the
    // largest ready slot so an independent tie keeps the normal cleanup order.
    budget.reserve(3 * nodes.len())?;
    let mut indegrees = vec![0usize; nodes.len()];
    let mut edges = vec![Vec::new(); nodes.len()];
    let index_of = |slot| nodes.binary_search_by_key(&slot, |&(member, _)| member);
    for (index, &(slot, lower)) in nodes.iter().enumerate() {
        budget.step(1)?;
        let mut add = |target: usize, budget: &mut Budget| -> ResultOf<()> {
            budget.reserve(1)?;
            edges[index].push(target);
            indegrees[target] += 1;
            Ok(())
        };
        if let Some(lower) = lower {
            add(lower, budget)?;
        }
        visit_loans(state, slot..slot + 1, budget, |loan, budget| {
            if loan.kind.borrows_local_storage() {
                let owner = slots[loan.source].owner;
                budget.step(1)?;
                if index_of(owner).is_ok() {
                    if owner == loan.source {
                        return Err(Stop::Pending(node));
                    }
                    budget.step(1)?;
                    add(
                        index_of(loan.source).map_err(|_| Stop::Pending(node))?,
                        budget,
                    )?;
                }
            }
            Ok(())
        })?;
    }
    let mut ready = BTreeSet::new();
    for (index, &indegree) in indegrees.iter().enumerate() {
        budget.step(1)?;
        if indegree == 0 {
            ready.insert(index);
        }
    }
    let mut ended = 0;
    while let Some(index) = ready.pop_last() {
        let slot = nodes[index].0;
        budget.step(1 + edges[index].len())?;
        budget.reserve(1)?;
        actions.push(Cleanup::EndStorage(slots[slot].place(module)));
        state[slot] = State::Vacant;
        ended += 1;
        for &target in &edges[index] {
            indegrees[target] -= 1;
            if indegrees[target] == 0 {
                ready.insert(target);
            }
        }
    }
    if ended != nodes.len() {
        return Err(Stop::Pending(node));
    }
    Ok(())
}

// A flat record can keep its ordinary reverse field order when every
// intra-record edge already points from a later view to an earlier leaf.
fn can_end_record_in_reverse_order(
    slots: &[Local<'_>],
    state: &[State],
    owner: Slot,
    budget: &mut Budget,
) -> ResultOf<bool> {
    let root = slots[owner];
    if root.custom_disposer.is_some() || root.contains_guard() {
        return Ok(false);
    }
    for slot in root.storage(owner) {
        budget.step(1)?;
        let local = slots[slot];
        if local.owner != owner
            || local.cleanup != StorageCleanup::End
            || (slot != owner
                && (local.members != 0 || !matches!(local.projection, Some(Projection::Field(_)))))
        {
            return Ok(false);
        }
        let State::Initialized(held) = &state[slot] else {
            continue;
        };
        if held.capture_drops != [None, None] || held.disposal_started_at.is_some() {
            return Ok(false);
        }
        for loan in held.loans.iter() {
            budget.step(1)?;
            if loan.kind.borrows_local_storage()
                && slots[loan.source].owner == owner
                && (!local.reference_member
                    || loan.kind != LoanKind::Backing
                    || loan.exclusive
                    || !(owner < loan.source && loan.source < slot)
                    || slots[loan.source].reference_member
                    || !matches!(state[loan.source], State::Initialized(_)))
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[derive(Clone, Copy)]
enum Boundary {
    End {
        lexical: bool,
        remainder: Option<Slot>,
    },
    Replacement,
}

fn cleanup_boundary<'a, 'g: 'a>(
    context: impl Into<CleanupContext<'a, 'g>>,
    slots: &[Local<'g>],
    state: &mut [State],
    scope: &[Slot],
    live: &BTreeSet<Slot>,
    (node, boundary): (NodeId, Boundary),
    budget: &mut Budget,
) -> ResultOf<Vec<Cleanup<'g>>> {
    let context = context.into();
    let module = context.module;
    budget.reserve(3 * scope.len())?;
    let mut indegrees = vec![0usize; scope.len()];
    let mut remaining = 0usize;
    let mut grouped_dependencies = false;
    let source_rank = |loan: &Loan, state: &[State]| {
        if !loan.kind.borrows_local_storage() {
            return None;
        }
        let owner = slots[loan.source].owner;
        let rank = scope.binary_search(&owner).ok()?;
        matches!(state[owner], State::Initialized(_)).then_some(rank)
    };
    // Count an incoming edge at each source for every dependent that must
    // retire before it. Only initialized storage participates.
    for (i, &slot) in scope.iter().enumerate() {
        budget.step(1)?;
        if !matches!(state[slot], State::Initialized(_)) {
            continue;
        }
        remaining += 1;
        conflict(
            slots,
            state,
            live,
            slot,
            (node, Conflict::Release),
            Access::EndScope,
            budget,
        )?;
        let mut internal = 0;
        visit_loans(state, slots[slot].storage(slot), budget, |loan, _| {
            if let Some(source) = source_rank(&loan, state) {
                indegrees[source] += 1;
                internal += usize::from(source == i);
                grouped_dependencies |=
                    slots[slot].members != 0 || slots[scope[source]].members != 0;
            }
            Ok(())
        })?;
        if internal != 0
            && slots[slot].members != 0
            && matches!(boundary, Boundary::End { lexical: true, .. })
            && can_end_record_in_reverse_order(slots, state, slot, budget)?
        {
            indegrees[i] -= internal;
        }
    }
    let mut ready = BTreeSet::new();
    for (i, &slot) in scope.iter().enumerate() {
        budget.step(1)?;
        if matches!(state[slot], State::Initialized(_)) && indegrees[i] == 0 {
            ready.insert(i);
        }
    }
    let mut actions = Vec::with_capacity(remaining);
    let mut scheduled = 0;
    while let Some(i) = ready.pop_last() {
        budget.step(1)?;
        let slot = scope[i];
        visit_loans(state, slots[slot].storage(slot), budget, |loan, _| {
            // A ready owner had no counted self-edges; its field cleanup
            // satisfies any admitted internal dependencies.
            if let Some(source) = source_rank(&loan, state).filter(|&source| source != i) {
                indegrees[source] -= 1;
                if indegrees[source] == 0 {
                    ready.insert(source);
                }
            }
            Ok(())
        })?;
        scheduled += 1;
        if disposal_origin(&state[slot]).is_some()
            && let Boundary::End {
                remainder: Some(remainder),
                ..
            } = boundary
            && remainder != slot
        {
            // This lexical alias ends before the outer disposer has returned.
            // Its source-level storage still goes out of scope. Keep only the
            // compiler's remaining cleanup responsibility for the epilogue.
            if !matches!(state[remainder], State::Vacant) {
                return Err(Stop::Pending(node));
            }
            let storage = slots[slot].storage(slot);
            budget.step(storage.len())?;
            for member in storage {
                state[remainder + (member - slot)] =
                    std::mem::replace(&mut state[member], State::Vacant);
            }
            actions.push(Cleanup::DeferDisposerFields {
                from: slots[slot].place(module),
                into: slots[remainder].place(module),
            });
            continue;
        }
        if let Some(implementation) = owed_disposer(slots[slot], &state[slot]) {
            require_initialized(slots, state, slot, node, budget)?;
            // The selected body owns the record and runs its remaining-field
            // cleanup. This boundary must not repeat it.
            actions.push(Cleanup::InvokeDisposer {
                operand: slots[slot].place(module),
                implementation,
            });
            let storage = slots[slot].storage(slot);
            budget.step(storage.len())?;
            state[storage].fill(State::Vacant);
            continue;
        }
        for member in slots[slot].storage(slot).rev() {
            budget.step(1)?;
            if let State::Initialized(held) = std::mem::replace(&mut state[member], State::Vacant) {
                if member != slot {
                    budget.reserve(1)?;
                }
                if let Some(layout) = held
                    .callable_owner
                    .and_then(CallableOwner::transport_cleanup)
                {
                    actions.push(Cleanup::ConsumeCallableTransfer {
                        operand: slots[member].place(module),
                        layout,
                    });
                    continue;
                }
                let drops = held.capture_drops.into_iter().flatten();
                let count = drops.clone().count();
                if count == 0 {
                    actions.push(cleanup_of(context, slots[member], &held, node, budget)?);
                } else {
                    budget.reserve(count - 1)?;
                    let closure = slots[member].place(module).root;
                    actions.extend(drops.rev().map(|field| Cleanup::DropCapture {
                        closure,
                        field: site_at(module, field),
                    }));
                }
            }
        }
    }
    if scheduled != remaining {
        if grouped_dependencies {
            finish_tuple_cleanup(module, slots, state, scope, node, &mut actions, budget)?;
        } else {
            let slot = scope
                .iter()
                .copied()
                .find(|&slot| matches!(state[slot], State::Initialized(_)))
                .ok_or(Stop::Pending(node))?;
            return Err(Stop::Rejected {
                site: node,
                related: slots[slot].node,
                kind: Conflict::Release,
            });
        }
    }
    for &slot in scope {
        let region = slots[slot].storage(slot);
        budget.step(region.len())?;
        state[region].fill(State::Vacant);
    }
    retire_loans(slots, state, scope, budget)?;
    Ok(actions)
}

// No surviving future use may depend on a retired storage version. Forget the
// obsolete edges of holders that are dead, and drop parent-only restrictions
// when the parent descriptor's storage ends without retiring its referent.
fn retire_loans(
    slots: &[Local<'_>],
    state: &mut [State],
    retired: &[Slot],
    budget: &mut Budget,
) -> ResultOf<()> {
    budget.step(state.len())?;
    for slot in state {
        let State::Initialized(held) = slot else {
            continue;
        };
        budget.step(held.loans.len().saturating_mul(retired.len()))?;
        let current = |loan: &Loan| {
            loan.kind.is_incoming() || !retired.iter().any(|&old| overlap(slots, old, loan.source))
        };
        if held.loans.iter().any(|loan| !current(loan)) {
            budget.reserve(held.loans.len())?;
            held.loans = Arc::from_iter(held.loans.iter().copied().filter(current));
        }
    }
    Ok(())
}

fn check_retained(
    retained: &BTreeMap<Slot, RetainedContents>,
    state: &[State],
    (node, escape): (NodeId, Conflict),
    retains_exclusive_bool: bool,
    budget: &mut Budget,
) -> ResultOf<()> {
    let mut unhandled_restriction = None;
    for contents in retained.values() {
        budget.step(1)?;
        let State::Initialized(value) = &state[contents.slot()] else {
            continue;
        };
        let site = value.origin.stored_at().unwrap_or(node);
        for loan in value.loans.iter() {
            budget.step(1)?;
            if loan.kind.borrows_local_storage() {
                return Err(Stop::Rejected {
                    site,
                    related: loan.site,
                    kind: escape,
                });
            }
            if !retains_exclusive_bool {
                unhandled_restriction.get_or_insert(site);
            }
        }
    }
    // Local access restrictions in outward storage need a supported contract.
    if let Some(site) = unhandled_restriction {
        return Err(Stop::Pending(site));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct CalleeEntry<'g> {
    callee: &'g DeclarationId,
    site: Site<'g>,
}

fn flat_action(action: Action<'_>, slots: &[Local<'_>]) -> bool {
    match action {
        Action::Displace { target, .. } => slots[target].cleanup == StorageCleanup::End,
        // A checked List<i32> index, its backing release and a protected
        // guard release are leaf actions in this static frame. None
        // introduces a call-entry checkpoint.
        Action::Dispose {
            target: WrittenDisposal::Descriptor,
            ..
        } => true,
        Action::Dispose {
            operand,
            target: WrittenDisposal::Ordinary,
        } => matches!(
            slots[operand].cleanup,
            StorageCleanup::DisposeListI32(_) | StorageCleanup::ReleaseGuard(_)
        ),
        Action::ConstructVariant {
            fresh_result_ok: true,
            ..
        }
        | Action::ConstructResultError {
            fresh_storage: Some(_),
            ..
        }
        | Action::ReadListI32 { .. }
        | Action::EqualI32Literal { .. }
        | Action::None
        | Action::BudgetBackedge { .. }
        | Action::Init(_)
        | Action::Read { .. }
        | Action::Move { .. }
        | Action::Borrow { .. }
        | Action::Reborrow { .. }
        | Action::Dereference { .. }
        | Action::Install { .. }
        | Action::CopyThrough { .. }
        | Action::InstallThrough { .. }
        | Action::CheckRetained
        | Action::End(_)
        | Action::Cleanup(_)
        | Action::EndScope(_)
        | Action::Export(_)
        | Action::Call(_) => true,
        _ => false,
    }
}

struct BlockInput {
    pc: Pc,
    state: Vec<State>,
    previous: Option<usize>,
}

// Index the most recent input per PC. Alternate inputs use links in the
// worklist itself; they do not grow a second map of wide keys.
#[derive(Default)]
struct KnownStates {
    heads: BTreeMap<Pc, usize>,
    blocks: Vec<BlockInput>,
}

impl KnownStates {
    fn add(&mut self, pc: Pc, state: Vec<State>, budget: &mut Budget) -> ResultOf<usize> {
        // Preserve the indexed PC probe and one input-width charge. Reading
        // loan lengths is covered by the slot walk, before comparing contents.
        let mut width = state.len().checked_add(1).ok_or(Stop::Limit)?;
        budget.step(width)?;
        for slot in &state {
            if let State::Initialized(held) = slot {
                let loans = held.loans.len();
                width = width.checked_add(loans).ok_or(Stop::Limit)?;
                budget.step(loans)?;
            }
        }
        let head = self.heads.get_mut(&pc);
        let previous = head.as_deref().copied();
        let mut current = previous;
        while let Some(block) = current {
            let input = &self.blocks[block];
            if input.state == state {
                return Ok(block);
            }
            current = input.previous;
            if current.is_some() {
                budget.step(width)?;
            }
        }
        // One worklist record, a head only for a new PC, and both the saved
        // input and the mutable execution copy. No allocation before reserve.
        let entries = state
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(1 + usize::from(previous.is_none())))
            .ok_or(Stop::Limit)?;
        budget.reserve(entries)?;
        let block = self.blocks.len();
        if let Some(head) = head {
            *head = block;
        } else {
            self.heads.insert(pc, block);
        }
        self.blocks.push(BlockInput {
            pc,
            state,
            previous,
        });
        Ok(block)
    }

    fn input(&self, block: usize) -> &[State] {
        &self.blocks[block].state
    }
}

// Reuse the ordinary pre-instruction states and the checked graph's callee
// identities. Taking ownership of these already-accounted collections does
// not run a speculative walk or charge ordinary-only callers for admission.
pub(super) struct CancellationTrace<'a, 'g> {
    module: &'g Module,
    disposer: build::DisposerLookup<'a, 'g>,
    body: NodeId,
    slots: Vec<Local<'g>>,
    locals: Vec<Slot>,
    params: Vec<Slot>,
    retained: BTreeMap<Slot, RetainedContents>,
    facts: BuildFacts<'a, 'g>,
    bindings: BTreeMap<&'g DeclarationId, Slot>,
    code: Vec<Instruction<'g>>,
    calls: Vec<Call<'a, 'g>>,
    known: KnownStates,
    live: Vec<BTreeSet<Slot>>,
    flat: bool,
    loop_delivery: bool,
    forwarding_backedge: bool,
    loop_owner: bool,
    branching: bool,
    calls_reached: usize,
    call_block: Option<usize>,
}

pub(super) struct EnabledRetainingCall<'a, 'g> {
    pub(super) callee: &'g DeclarationId,
    block: usize,
    frame: CancellationFrame<'a, 'g>,
    body: NodeId,
    entry: &'a [State],
    node: NodeId,
    stages: [Place<'g>; 3],
    gate_parameter: Slot,
    destination: Slot,
    suspended: Vec<State>,
}

impl<'g> EnabledRetainingCall<'_, 'g> {
    pub(super) fn forwarded_parameter(&self) -> Option<Place<'g>> {
        (self.frame.params == [self.gate_parameter])
            .then(|| self.frame.slots[self.gate_parameter].place(self.frame.module))
    }

    pub(super) fn finish(
        self,
        trace: CancellationTrace<'_, 'g>,
        mut body: Mir<'g>,
        budget: &mut Budget,
    ) -> Result<Option<Cancellation<'g>>, (&'g Module, Stop)> {
        let Some(parameters) = trace.retaining_parameters() else {
            return Ok(None);
        };
        let callee = trace.module;
        // Callee ownership states use formal slots. Its incoming shared
        // backing is opaque, not storage owned by that frame. Check its actual
        // feasible exits before retiring any caller storage. Caller loans keep
        // the concrete old and compatible new sources alive across this check.
        let Some(callee_proof) = trace
            .finish(&body, |_, _, _| Ok(None), false, false, budget)
            .map_err(|stop| (callee, stop))?
        else {
            return Ok(None);
        };
        body.cancellation = Some(callee_proof);

        let (module, block, callee) = (self.frame.module, self.block, self.callee);
        let Some((mut proof, suspended)) = self
            .finish_caller(parameters, budget)
            .map_err(|stop| (module, stop))?
        else {
            return Ok(None);
        };
        proof.successors.insert(
            block,
            CancellationSuccessor::RetainingCall(Box::new(RetainingCallCancellation {
                callee,
                body,
                arguments: suspended.arguments,
                destination: suspended.destination,
                possible_backing: suspended.possible_backing,
                caller_cleanup: suspended.caller_cleanup,
            })),
        );
        Ok(Some(proof))
    }

    pub(super) fn finish_caller(
        self,
        parameters: [Place<'g>; 3],
        budget: &mut Budget,
    ) -> ResultOf<Option<(Cancellation<'g>, SuspendedRetainingFrame<'g>)>> {
        let Some(proof) = self.frame.entry(self.entry, self.body, budget)? else {
            return Ok(None);
        };
        let loans = &initialized(&self.suspended[self.destination], self.node)?.loans;
        budget.step(loans.len())?;
        budget.reserve(loans.len() + self.stages.len() + 2)?;
        let possible_backing = loans
            .iter()
            .map(|loan| self.frame.slots[loan.source].place(self.frame.module))
            .collect();
        let exit = self
            .frame
            .exit(Cow::Owned(self.suspended), self.node, budget)?;
        let arguments = std::array::from_fn(|i| CancellationArgument {
            staged: self.stages[i],
            parameter: parameters[i],
        });
        Ok(Some((
            proof,
            SuspendedRetainingFrame {
                arguments,
                destination: self.frame.slots[self.destination].place(self.frame.module),
                possible_backing,
                caller_cleanup: exit.cleanup,
            },
        )))
    }
}

impl<'g> CancellationTrace<'_, 'g> {
    fn fact_kind(&self, node: NodeId) -> Option<&NodeFactKind<'g>> {
        self.facts.nodes().get(&node).map(|fact| &fact.kind)
    }

    fn frame(&self) -> CancellationFrame<'_, 'g> {
        CancellationFrame {
            module: self.module,
            disposer: Some(self.disposer),
            slots: &self.slots,
            locals: &self.locals,
            params: &self.params,
            retained: &self.retained,
            indexed: self.facts.is_indexed(),
        }
    }

    pub(super) fn retaining_parameters(&self) -> Option<[Place<'g>; 3]> {
        // Compose only a flat leaf with a possible body checkpoint.
        if !self.flat || !self.loop_delivery || self.calls_reached != 0 {
            return None;
        }
        let &[slot, input, gate] = self.params.as_slice() else {
            return None;
        };
        matches!(self.retained.get(&slot), Some(RetainedContents::OneHop(_)))
            .then(|| [slot, input, gate].map(|slot| self.slots[slot].place(self.module)))
    }

    pub(super) fn retaining_call_block(&self) -> Option<usize> {
        if self.flat
            && !self.branching
            && !self.loop_delivery
            && self.calls_reached == 1
            && self.retained.is_empty()
        {
            self.call_block
        } else {
            None
        }
    }

    pub(super) fn prepare_retaining_call(
        &self,
        mir: &Mir<'g>,
        budget: &mut Budget,
    ) -> ResultOf<Option<EnabledRetainingCall<'_, 'g>>> {
        let Some(block) = self.retaining_call_block() else {
            return Ok(None);
        };
        let pc = self.known.blocks[block].pc;
        let Instruction {
            node,
            action: Action::Call(index),
            ..
        } = self.code[pc]
        else {
            unreachable!("call block records a reached call");
        };
        let call = &self.calls[index];
        let (
            &[access, input, gate],
            Some(CallDestination {
                storage: CallStorage::Local(target),
                argument,
                input: source,
                second_input: None,
                extra: Some(RetainedCallExtra::Bool { position: 2 }),
            }),
        ) = (call.args.as_slice(), call.destination)
        else {
            return Ok(None);
        };
        if access != argument || input != source {
            return Ok(None);
        }
        // The sole reached call in this straight frame is unmasked. Its
        // direct immutable Bool input admits both values, independently of
        // reaching the call. No caller-side correlation is discarded.
        budget.step(1)?;
        let Some(NodeFactKind::Expression(facts)) = self.fact_kind(node) else {
            return Ok(None);
        };
        let Some(facts) = &facts.call else {
            return Ok(None);
        };
        if !std::ptr::eq(facts.callee, call.callee) {
            if self.facts.is_indexed() {
                budget.step(
                    facts
                        .callee
                        .module
                        .path
                        .len()
                        .checked_add(1)
                        .ok_or(Stop::Limit)?,
                )?;
            }
            if facts.callee != call.callee {
                return Ok(None);
            }
        }
        if facts.pipe_input.is_some() {
            return Ok(None);
        }
        let Some(gate_parameter) = self.branch_parameter(self.slots[gate].node, budget)? else {
            return Ok(None);
        };
        let state = self.known.input(block);
        let old = initialized(&state[target], node)?;
        let source = initialized(&state[input], node)?;
        if old.loans.is_empty()
            || source.loans.is_empty()
            || initialized(&state[access], node)?.exact_target != Some(ExactTarget::Local(target))
        {
            return Ok(None);
        }
        let mut backing = BTreeSet::new();
        for &loan in old.loans.iter().chain(source.loans.iter()) {
            budget.step(1)?;
            if loan.kind != LoanKind::Backing
                || loan.exclusive
                || !matches!(state[loan.source], State::Initialized(_))
            {
                return Ok(None);
            }
            insert_loan(&mut backing, loan, budget)?;
        }
        // These stages become callee parameters, not caller cleanup operands.
        // Bound the slot by its old and sole compatible input loans, never by
        // the transient access loan. Entry itself still has the old contents.
        let loans = loan_set(backing, budget)?;
        if self.facts.is_indexed() {
            budget.step(state.len())?;
        }
        budget.reserve(state.len())?;
        let mut suspended = state.to_vec();
        for arg in [access, input, gate] {
            suspended[arg] = State::Vacant;
        }
        suspended[target] = State::Initialized(Held {
            loans,
            ..Held::default()
        });
        let frame = self.frame();
        if !frame.check(&suspended, node, budget)? {
            return Ok(None);
        }
        Ok(Some(EnabledRetainingCall {
            callee: call.callee,
            frame,
            body: self.body,
            entry: self.known.input(mir.entry),
            block,
            node,
            stages: [access, input, gate].map(|slot| self.slots[slot].place(self.module)),
            gate_parameter,
            destination: target,
            suspended,
        }))
    }

    // Dynamic branches only admit a direct read of an immutable Bool
    // parameter. Values of locals, arbitrary expressions and call results
    // are not independent choices. The ordinary CFG's one-way edges already
    // exclude branches ruled out by the typed outcomes.
    fn branch_parameter(&self, mut node: NodeId, budget: &mut Budget) -> ResultOf<Option<Slot>> {
        use crate::parser::NodeKind;

        loop {
            budget.step(1)?;
            let syntax = &self.module.syntax.nodes[node];
            if syntax.kind != NodeKind::Group {
                if syntax.kind != NodeKind::ValueReference {
                    return Ok(None);
                }
                break;
            }
            node = syntax.children[0];
        }
        let Some(NodeFactKind::Expression(facts)) = self.fact_kind(node) else {
            return Ok(None);
        };
        let Some(path) = &facts.path else {
            return Ok(None);
        };
        if path.root.kind != DeclarationKind::Parameter
            || !path.projections.is_empty()
            || facts.refinement.is_some()
            || facts.access != crate::analysis::scalars::data::Access::Owned
            || facts.place != crate::analysis::scalars::Place::ReadOnly
            || !matches!(facts.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
        {
            return Ok(None);
        }
        if self.facts.is_indexed() {
            // Include the binding equality test and all possible map probes.
            let width = path
                .root
                .module
                .path
                .len()
                .checked_add(1)
                .ok_or(Stop::Limit)?;
            budget.step(
                width
                    .checked_mul(self.bindings.len() + 1)
                    .ok_or(Stop::Limit)?,
            )?;
        }
        if facts.binding != Some(path.root) {
            return Ok(None);
        }
        let slot = self.bindings[path.root];
        Ok((slot < u64::BITS as usize).then_some(slot))
    }

    pub(super) fn finish(
        &self,
        mir: &Mir<'g>,
        entry_for: impl Fn(usize, &'g DeclarationId, &mut Budget) -> ResultOf<Option<Site<'g>>>,
        known_entry: bool,
        unknown_entry: bool,
        budget: &mut Budget,
    ) -> ResultOf<Option<Cancellation<'g>>> {
        let mut complete = self.flat && !unknown_entry;
        let checkpoints = self.loop_delivery || known_entry;
        if !complete && (!checkpoints || self.retained.is_empty()) {
            return Ok(None);
        }
        let frame = self.frame();
        if !checkpoints {
            return frame.entry(self.known.input(mir.entry), self.body, budget);
        }

        // A path condition is the pair of true and false parameter masks.
        // Check before pushing, including a repeated loop path. Every key
        // and worklist item is charged, so state splitting cannot grow past
        // the caller's work and entry limits.
        let mut visited = BTreeSet::new();
        let mut pending = Vec::new();
        let enqueue = |path,
                       visited: &mut BTreeSet<_>,
                       pending: &mut Vec<_>,
                       budget: &mut Budget|
         -> ResultOf<()> {
            budget.step(1)?;
            if !visited.contains(&path) {
                budget.reserve(2)?;
                visited.insert(path);
                pending.push(path);
            }
            Ok(())
        };
        enqueue((mir.entry, 0u64, 0u64), &mut visited, &mut pending, budget)?;
        let mut deliveries = BTreeMap::new();
        let mut feasible_backedge = false;
        while let Some((block, yes_bits, no_bits)) = pending.pop() {
            if !complete && self.retained.is_empty() {
                return Ok(None);
            }
            budget.step(1)?;
            let pc = self.known.blocks[block].pc;
            let Instruction { node, action, .. } = self.code[pc];
            if !flat_action(action, &self.slots) {
                complete = false;
                continue;
            }
            let callee = if let Action::Call(index) = action {
                let callee = self.calls[index].callee;
                let Some(site) = entry_for(block, callee, budget)? else {
                    complete = false;
                    continue;
                };
                Some(CalleeEntry { callee, site })
            } else {
                None
            };
            if (callee.is_some() || matches!(action, Action::BudgetBackedge { .. }))
                && !deliveries.contains_key(&block)
            {
                // Even after losing the receipt, a feasible independent path
                // can expose a forbidden outward loan. Don't build cleanup
                // or accumulate successors for a proof we cannot publish.
                let admitted = frame.check(self.known.input(block), node, budget)?;
                complete &= admitted;
                if complete {
                    budget.reserve(1)?;
                    feasible_backedge |= matches!(action, Action::BudgetBackedge { .. });
                    deliveries.insert(block, callee);
                }
            }
            match mir.blocks[block].terminator {
                Terminator::Goto(next) => {
                    enqueue(
                        (next, yes_bits, no_bits),
                        &mut visited,
                        &mut pending,
                        budget,
                    )?;
                }
                Terminator::Branch { yes, no } => {
                    let Some(slot) = self.branch_parameter(node, budget)? else {
                        complete = false;
                        continue;
                    };
                    let bit = 1u64 << slot;
                    // The condition has already read its parameter. Preserve that
                    // decision only if another read is possible, including on a backedge.
                    budget.step(1)?;
                    let keep = if self.live[pc].contains(&slot) {
                        bit
                    } else {
                        0
                    };
                    if yes_bits & bit == 0 {
                        enqueue(
                            (no, yes_bits, (no_bits & !bit) | keep),
                            &mut visited,
                            &mut pending,
                            budget,
                        )?;
                    }
                    if no_bits & bit == 0 {
                        enqueue(
                            (yes, (yes_bits & !bit) | keep, no_bits),
                            &mut visited,
                            &mut pending,
                            budget,
                        )?;
                    }
                }
                Terminator::Return => {}
                Terminator::FinishStartup | Terminator::FinishExpectation => return Ok(None),
            }
        }
        if !complete || (self.loop_owner && !feasible_backedge) {
            return Ok(None);
        }

        let Some(mut proof) = frame.entry(self.known.input(mir.entry), self.body, budget)? else {
            return Ok(None);
        };
        for (block, callee) in deliveries {
            budget.step(1)?;
            let on_delivery = frame.exit(
                self.known.input(block).into(),
                mir.blocks[block].site.node,
                budget,
            )?;
            let successor = match callee {
                Some(CalleeEntry { callee, site }) => CancellationSuccessor::CalleeEntry {
                    callee,
                    entry: site,
                    on_delivery,
                },
                None => CancellationSuccessor::LoopBackedge { on_delivery },
            };
            budget.reserve(1)?;
            proof.successors.insert(block, successor);
        }
        Ok(Some(proof))
    }
}

struct CancellationFrame<'a, 'g> {
    module: &'g Module,
    disposer: Option<build::DisposerLookup<'a, 'g>>,
    slots: &'a [Local<'g>],
    locals: &'a [Slot],
    params: &'a [Slot],
    retained: &'a BTreeMap<Slot, RetainedContents>,
    indexed: bool,
}

impl<'g> CancellationFrame<'_, 'g> {
    fn entry(
        &self,
        state: &[State],
        body: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Option<Cancellation<'g>>> {
        if !self.check(state, body, budget)? {
            return Ok(None);
        }
        budget.reserve(1)?;
        Ok(Some(Cancellation {
            entry_site: site_at(self.module, body),
            on_entry_delivery: self.exit(state.into(), body, budget)?,
            successors: BTreeMap::new(),
        }))
    }

    fn check(&self, state: &[State], node: NodeId, budget: &mut Budget) -> ResultOf<bool> {
        self.check_initialized(state, node, false, budget)
    }

    fn check_initialized(
        &self,
        state: &[State],
        node: NodeId,
        scoped_join: bool,
        budget: &mut Budget,
    ) -> ResultOf<bool> {
        let contents = self
            .retained
            .values()
            .next()
            .map(|contents| contents.slot());
        for (slot, state) in state.iter().enumerate() {
            budget.step(1)?;
            if let State::Initialized(held) = state {
                if matches!(self.slots[slot].cleanup, StorageCleanup::FreshBoolResult(_)) {
                    match held.origin {
                        ValueOrigin::FreshResultOk => {}
                        ValueOrigin::FreshResultErr(_) if !scoped_join => {}
                        // A live parent error is outside the scoped parent
                        // exit, separately from checked child-stage retirement.
                        _ => return Ok(false),
                    }
                }
                if Some(slot) != contents
                    && self.locals.binary_search(&slot).is_err()
                    && self.params.binary_search(&slot).is_err()
                {
                    // Nested scopes and in-flight staging are not flat boundaries.
                    return Ok(false);
                }
            }
        }
        // Check outward storage before ending even the first local owner.
        match check_retained(
            self.retained,
            state,
            (node, Conflict::CancellationEscape { delivery: node }),
            false,
            budget,
        ) {
            Err(Stop::Pending(_)) => Ok(false),
            checked => checked.map(|()| true),
        }
    }

    fn exit(
        &self,
        state: Cow<'_, [State]>,
        node: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<CancellationExit<'g>> {
        let outward = self.retained.iter().next();
        let contents = outward.map(|(_, contents)| contents.slot());
        // The outward evidence and the live-set member are distinct entries.
        budget.reserve(usize::from(outward.is_some()))?;
        let retained = outward.map(|(&parameter, contents)| RetainedAtDelivery {
            parameter: self.slots[parameter].place(self.module),
            last_store: match &state[contents.slot()] {
                State::Initialized(held) => held
                    .origin
                    .stored_at()
                    .map(|store| site_at(self.module, store)),
                _ => None,
            },
        });
        if self.indexed && matches!(state, Cow::Borrowed(_)) {
            budget.step(state.len())?;
        }
        // Preserve the exit scratch charge even when reusing the suspended
        // caller's buffer rather than making a second copy.
        budget.reserve(state.len() + usize::from(contents.is_some()))?;
        let mut abandoned = state.into_owned();
        let mut live = BTreeSet::from_iter(contents);
        // The lexical body ends before parameter storage. Within a boundary
        // all initialized dependents are scheduled before their sources.
        budget.reserve(self.params.len())?;
        live.extend(self.params.iter().copied());
        let boundary = (
            node,
            Boundary::End {
                lexical: true,
                remainder: None,
            },
        );
        let context = CleanupContext {
            module: self.module,
            disposer: self.disposer,
        };
        let mut cleanup = cleanup_boundary(
            context,
            self.slots,
            &mut abandoned,
            self.locals,
            &live,
            boundary,
            budget,
        )?;
        for parameter in self.params {
            budget.step(1)?;
            live.remove(parameter);
        }
        let parameters = cleanup_boundary(
            context,
            self.slots,
            &mut abandoned,
            self.params,
            &live,
            boundary,
            budget,
        )?;
        budget.reserve(parameters.len())?;
        cleanup.extend(parameters);
        Ok(CancellationExit { retained, cleanup })
    }
}

pub(super) struct ReplacementSite<'g> {
    pub displacement: usize,
    pub post_install_call: usize,
    pub implementation: &'g DeclarationId,
    pub callee: &'g DeclarationId,
    installation: usize,
    target: Slot,
    incoming: Slot,
}

pub(super) struct ReplacementCandidate<'g> {
    pub site: ReplacementSite<'g>,
    module: &'g Module,
    slots: Vec<Local<'g>>,
    locals: Vec<Slot>,
    params: Vec<Slot>,
    known: KnownStates,
    indexed: bool,
}

impl<'g> ReplacementCandidate<'g> {
    // `body` is the checked active contract for `implementation`, and `entry`
    // belongs to the checked, empty, zero-argument `callee`. Prove that the
    // active frame actually reaches that callee and has a single continuation
    // through primitive cleanup and the deferred-field epilogue. Never ask
    // the leaf's enabled-frame judgment to exit this active frame.
    pub(super) fn finish(
        self,
        caller: &Mir<'g>,
        body: &Mir<'g>,
        entry: Site<'g>,
        budget: &mut Budget,
    ) -> ResultOf<Option<Box<ReplacementCancellation<'g>>>> {
        if !body
            .active_leaf
            .is_some_and(|leaf| std::ptr::eq(leaf, self.site.callee))
        {
            return Ok(None);
        }
        let mut index = body.entry;
        let mut checkpoint = None;
        let mut transfer: Option<(usize, Place<'g>, Place<'g>)> = None;
        let mut remainder_ended = false;
        loop {
            budget.step(1)?;
            let block = &body.blocks[index];
            match &block.operation {
                Operation::None => {}
                Operation::Move {
                    from,
                    into: Some(into),
                } if transfer.is_none() && from.field().is_some() && into.projection.is_none() => {}
                Operation::Call(callee) if checkpoint.is_none() && transfer.is_none() => {
                    if !std::ptr::eq(*callee, self.site.callee) {
                        return Ok(None);
                    }
                    let Terminator::Goto(resume) = block.terminator else {
                        return Ok(None);
                    };
                    checkpoint = Some((index, resume));
                }
                Operation::Cleanup(actions) => {
                    for action in actions {
                        budget.step(1)?;
                        match *action {
                            Cleanup::Drop(_) | Cleanup::EndStorage(_) if transfer.is_none() => {}
                            Cleanup::Drop(field)
                                if !remainder_ended
                                    && field.field().is_some()
                                    && transfer.is_some_and(|(_, _, place)| {
                                        field.root.node == place.root.node
                                    }) => {}
                            Cleanup::EndStorage(root)
                                if !remainder_ended
                                    && root.projection.is_none()
                                    && transfer.is_some_and(|(_, _, place)| {
                                        root.root.node == place.root.node
                                    }) =>
                            {
                                remainder_ended = true;
                            }
                            Cleanup::DeferDisposerFields { from, into }
                                if checkpoint.is_some() && transfer.is_none() =>
                            {
                                transfer = Some((index, from, into));
                            }
                            _ => return Ok(None),
                        }
                    }
                }
                _ => return Ok(None),
            }
            match block.terminator {
                Terminator::Goto(next) if next > index => index = next,
                Terminator::Return => break,
                _ => return Ok(None),
            }
        }
        let (Some((call, resume)), Some((defer_fields, operand, remainder)), true) =
            (checkpoint, transfer, remainder_ended)
        else {
            return Ok(None);
        };
        let Some(on_delivery) = self.installed_exit(caller, budget)? else {
            return Ok(None);
        };
        let Terminator::Goto(ordinary_suffix) =
            caller.blocks[self.site.post_install_call].terminator
        else {
            return Ok(None);
        };
        budget.reserve(1)?;
        Ok(Some(Box::new(ReplacementCancellation {
            displacement: self.site.displacement,
            installation: self.site.installation,
            staged: self.slots[self.site.incoming].place(self.module),
            installed: self.slots[self.site.target].place(self.module),
            implementation: self.site.implementation,
            deferred: DeferredDisposerCheckpoint {
                call,
                resume,
                defer_fields,
                operand,
                remainder,
            },
            callee: self.site.callee,
            entry,
            post_install_call: self.site.post_install_call,
            ordinary_suffix,
            installed_cleanup: on_delivery.cleanup,
        })))
    }
}

/// A bounded replacement state: exactly the designated regions are live,
/// with no carried loans or other ownership contracts in those regions.
fn replacement_state(
    state: &[State],
    first: Range<Slot>,
    second: Range<Slot>,
    budget: &mut Budget,
) -> ResultOf<bool> {
    let plain = Held::default();
    for (slot, state) in state.iter().enumerate() {
        budget.step(1)?;
        if first.contains(&slot) || second.contains(&slot) {
            if !matches!(state, State::Initialized(held) if *held == plain) {
                return Ok(false);
            }
        } else if matches!(state, State::Initialized(_)) {
            return Ok(false);
        }
    }
    Ok(true)
}

// Read the adjacent ordinary successors without another CFG walk or state copy.
fn replacement_site<'g>(
    blocks: &[Block<'g>],
    code: &[Instruction<'g>],
    known: &KnownStates,
    displacement: usize,
) -> Option<ReplacementSite<'g>> {
    let Action::Displace { target, incoming } = code[known.blocks[displacement].pc].action else {
        return None;
    };
    let [Cleanup::InvokeDisposer { implementation, .. }] = (match &blocks[displacement].operation {
        Operation::Cleanup(actions) => actions.as_slice(),
        _ => return None,
    }) else {
        return None;
    };
    let Terminator::Goto(installation) = blocks[displacement].terminator else {
        return None;
    };
    let Action::Install {
        target: dest,
        incoming: stage,
    } = code[known.blocks[installation].pc].action
    else {
        return None;
    };
    if dest != target || stage != incoming {
        return None;
    }
    let Terminator::Goto(post_install_call) = blocks[installation].terminator else {
        return None;
    };
    let Operation::Call(callee) = blocks[post_install_call].operation else {
        return None;
    };
    Some(ReplacementSite {
        displacement,
        installation,
        post_install_call,
        target,
        incoming,
        implementation,
        callee,
    })
}

impl<'g> ReplacementCandidate<'g> {
    // Only after the two canonical entry contexts are established is there
    // a delivery exit to construct. All inputs were moved from ordinary flow.
    fn installed_exit(
        &self,
        caller: &Mir<'g>,
        budget: &mut Budget,
    ) -> ResultOf<Option<CancellationExit<'g>>> {
        let ReplacementSite {
            displacement,
            installation,
            post_install_call,
            target,
            incoming,
            implementation,
            ..
        } = self.site;
        let blocks = &caller.blocks;
        let known = &self.known;
        let frame = CancellationFrame {
            module: self.module,
            disposer: None,
            slots: &self.slots,
            locals: &self.locals,
            params: &self.params,
            retained: &BTreeMap::new(),
            indexed: self.indexed,
        };
        // Admit a reached, move-only prefix. An ordinary call contract would
        // not establish that an arbitrary operation before the region returns.
        for (index, block) in blocks[..displacement].iter().enumerate() {
            budget.step(1)?;
            if !matches!(block.operation, Operation::Move { from, into: Some(into) }
            if from.projection.is_none() && into.projection.is_none())
                || block.terminator != Terminator::Goto(index + 1)
            {
                return Ok(None);
            }
        }
        let slots = frame.slots;
        let old = slots[target].storage(target);
        let staged = slots[incoming].storage(incoming);
        budget.step(frame.locals.len())?;
        if old.contains(&incoming)
            || staged.contains(&target)
            || frame.locals.binary_search(&target).is_err()
            || slots[target].members == 0
            || slots[target].owner != target
            || slots[incoming].owner != incoming
            || slots[target].cleanup != StorageCleanup::End
        {
            return Ok(None);
        }
        for field in old.clone().skip(1) {
            budget.step(1)?;
            let local = slots[field];
            // Field + primitive Drop is an owned string in this storage model.
            if !matches!(local.projection, Some(Projection::Field(_)))
                || local.cleanup != StorageCleanup::Drop
                || local.custom_disposer.is_some()
            {
                return Ok(None);
            }
        }
        let installed = known.input(post_install_call);
        if !replacement_state(
            known.input(displacement),
            old.clone(),
            staged.clone(),
            budget,
        )? || !replacement_state(known.input(installation), staged, 0..0, budget)?
            || !replacement_state(installed, old, 0..0, budget)?
        {
            return Ok(None);
        }
        // Build the exit from the post-install OWNER STATE, not from a normal
        // suffix or from the old displacement action. No other storage is live.
        let on_delivery = match frame.exit(
            installed.into(),
            blocks[post_install_call].site.node,
            budget,
        ) {
            Ok(exit) => exit,
            Err(Stop::Limit) => return Err(Stop::Limit),
            Err(Stop::Pending(_) | Stop::Rejected { .. }) => return Ok(None),
        };
        let [
            Cleanup::InvokeDisposer {
                implementation: installed_impl,
                ..
            },
        ] = on_delivery.cleanup.as_slice()
        else {
            return Ok(None);
        };
        if !std::ptr::eq(implementation, *installed_impl) {
            return Ok(None);
        }
        Ok(Some(on_delivery))
    }
}

pub(super) fn run<'a, 'g>(
    build: Build<'a, 'g, '_>,
    entry: Pc,
    params: Vec<Slot>,
    active: Option<DisposerFrame>,
    collect_trace: bool,
    mut cache: Option<&mut QueryCache<'a, 'g>>,
) -> ResultOf<Lowered<'a, 'g>> {
    let application_function =
        collect_trace && build.module.key.origin == crate::loader::Origin::Application;
    let mut candidate = application_function
        && !build.retains_exclusive_bool
        && build.retained.len() <= 1
        && !matches!(
            build.retained.values().next(),
            Some(RetainedContents::TwoHop(_))
        );
    let mut flat = true;
    let mut loop_owner = false;
    let mut fresh_result = false;
    let mut loop_delivery = false;
    let mut forwarding_backedge = false;
    let mut branching = false;
    let mut calls_reached = 0;
    let mut call_block = None;
    let mut displacement = None;
    let mut active_leaf = None;
    let mut single_active_call = true;
    let mut replacement_candidate = application_function && build.retained.is_empty();
    let Build {
        typed,
        copied_origins,
        declared_shared_frame,
        module,
        body,
        mut facts,
        callable_frame,
        locals: bindings,
        return_contract,
        slots,
        code,
        calls,
        list_arrow_pairs,
        incoming_list_arrow,
        mut cleanups,
        body_scope,
        list_delivery_loop,
        dispose,
        retained,
        retains_exclusive_bool,
        incoming_exclusive_bools,
        incoming_shared_i32,
        observing_disposers,
        budget,
        ..
    } = build;
    let disposer = build::DisposerLookup { typed, dispose };
    let cleanup_context = CleanupContext {
        module,
        disposer: Some(disposer),
    };
    let live = liveness(
        &slots,
        &code,
        &calls,
        &cleanups,
        &retained,
        &observing_disposers,
        budget,
    )?;
    let empty = Held::default();
    let mut known = KnownStates::default();
    let mut blocks = Vec::new();
    // A worklist index is also the eventual MIR block index.
    budget.reserve(slots.len())?;
    let mut state: Vec<_> = slots
        .iter()
        .map(|slot| {
            // These transfers have only an ordinary-exit proof.
            if slot.reference_option.is_some()
                || matches!(
                    slot.cleanup,
                    StorageCleanup::DisposeOwnedListIterI32(_)
                        | StorageCleanup::DisposeListText(_)
                        | StorageCleanup::DisposeOwnedListIterText(_)
                        | StorageCleanup::DisposeListOptionText { .. }
                        | StorageCleanup::DisposeOwnedListIterOptionText(_)
                        | StorageCleanup::DisposeOwnedListIterRecordText(_)
                )
            {
                candidate = false;
                replacement_candidate = false;
            }
            // An owned List<i32> backing or LockGuard<i32> is released on
            // delivery only from the leaf frame of the simple loop below.
            loop_owner |= matches!(
                slot.cleanup,
                StorageCleanup::DisposeListI32(_) | StorageCleanup::ReleaseGuard(_)
            );
            fresh_result |= matches!(slot.cleanup, StorageCleanup::FreshBoolResult(_));
            candidate = candidate
                && slot.projection.is_none()
                && slot.members == 0
                && slot.custom_disposer.is_none()
                && matches!(
                    slot.cleanup,
                    StorageCleanup::End
                        | StorageCleanup::Drop
                        | StorageCleanup::DisposeListI32(_)
                        | StorageCleanup::ReleaseGuard(_)
                        | StorageCleanup::FreshBoolResult(_)
                );
            State::Vacant
        })
        .collect();
    // This admits the installed owner in a leaf frame only; in particular
    // it adds no contract for outward storage or for transferred call frames.
    candidate &= !loop_owner || (list_delivery_loop.is_some() && retained.is_empty());
    for &slot in &params {
        let region = slots[slot].storage(slot);
        budget.step(region.len())?;
        // Parameters do not supply a checked fresh residence at frame entry.
        candidate &= !matches!(slots[slot].cleanup, StorageCleanup::FreshBoolResult(_));
        state[region].fill(State::Initialized(empty.clone()));
        if let Some(option @ (ReferenceOption::Shared | ReferenceOption::Exclusive)) =
            slots[slot].reference_option
        {
            let exclusive = option == ReferenceOption::Exclusive;
            budget.reserve(1)?;
            state[slot] = State::Initialized(Held {
                loans: Arc::from([Loan {
                    source: slot,
                    site: slots[slot].node,
                    exclusive,
                    kind: if exclusive {
                        LoanKind::IncomingExclusive
                    } else {
                        LoanKind::IncomingSharedOptionBacking
                    },
                }]),
                ..Held::default()
            });
        }
    }
    for source in incoming_shared_i32 {
        budget.step(1 + params.len().max(1).ilog2() as usize)?;
        if params.binary_search(&source).is_err() {
            return Err(Stop::Pending(body));
        }
        budget.reserve(1)?;
        state[source] = State::Initialized(Held {
            loans: Arc::from([Loan {
                source,
                site: slots[source].node,
                exclusive: false,
                kind: LoanKind::IncomingSharedI32,
            }]),
            ..Held::default()
        });
    }
    if let Some((view, backing, exclusive)) = incoming_list_arrow {
        budget.reserve(1)?;
        state[view] = State::Initialized(Held {
            loans: Arc::from([Loan {
                source: backing,
                site: slots[view].node,
                exclusive,
                kind: LoanKind::Backing,
            }]),
            ..Held::default()
        });
    }
    let mut callable_exported = false;
    let mut unsupported_cleanup = Vec::new();
    if let Some((layout, _)) = callable_frame {
        budget.step(slots.len())?;
        for (slot, local) in slots.iter().enumerate() {
            if local.custom_disposer.is_some()
                || !matches!(local.cleanup, StorageCleanup::End | StorageCleanup::Drop)
            {
                budget.reserve(1)?;
                unsupported_cleanup.push(slot);
            }
        }
        let [formal] = params.as_slice() else {
            return Err(Stop::Pending(body));
        };
        budget.reserve(1)?;
        state[*formal] = State::Initialized(Held {
            callable_owner: Some(CallableOwner::Formal(layout)),
            loans: Arc::from([Loan {
                source: *formal,
                site: slots[*formal].node,
                exclusive: layout.0[0] == CallableField::ExclusiveBool,
                kind: LoanKind::IncomingCallableBacking,
            }]),
            ..Held::default()
        });
    }
    for source in incoming_exclusive_bools {
        budget.step(1)?;
        budget.reserve(1)?;
        state[source] = State::Initialized(Held {
            loans: Arc::from([Loan {
                source,
                site: slots[source].node,
                exclusive: true,
                kind: LoanKind::IncomingExclusive,
            }]),
            ..Held::default()
        });
    }
    for (&parameter, &contents) in &retained {
        budget.step(1)?;
        match contents {
            RetainedContents::OneHop(contents) => {
                state[parameter] = State::Initialized(Held {
                    exact_target: Some(ExactTarget::Caller {
                        parameter,
                        contents,
                    }),
                    ..Held::default()
                });
            }
            RetainedContents::TwoHop(contents) => {
                // Incoming final backing is opaque. There are no tracked
                // body-local loans yet, and no one-hop target for the route.
                state[contents] = State::Initialized(empty.clone());
            }
        }
    }
    if let Some(frame) = active {
        state[frame.operand] = State::Initialized(Held {
            disposal_started_at: Some(slots[frame.operand].node),
            ..Held::default()
        });
    }
    let remainder = active.map(|frame| frame.remainder);
    let entry = known.add(entry, state, budget)?;
    let mut blocked = None;
    while blocks.len() < known.blocks.len() {
        let input = &known.blocks[blocks.len()];
        let pc = input.pc;
        // The insertion reserved this copy alongside the saved input.
        let mut state = input.state.clone();
        let Instruction {
            node,
            action,
            edges,
        } = code[pc];
        let site = site_at(module, node);
        let local = |slot: Slot| slots[slot].place(module);
        // Budget backedges have no ordinary MIR operation. Don't mistake
        // one with a fresh successor state for a straight-line disposer.
        if active.is_some() && matches!(action, Action::BudgetBackedge { .. }) {
            single_active_call = false;
        }
        if loop_owner
            && candidate
            && let Some((loop_node, test)) = list_delivery_loop
        {
            // Any other reached branch, call, or backedge is outside this
            // leaf proof. In particular, do not pay for a speculative path
            // walk that will veto a local condition or a live nested scope.
            candidate = !matches!(action, Action::Call(_))
                && (!matches!(action, Action::BudgetBackedge { .. }) || node == loop_node)
                && (!matches!(edges, Edges::Branch { .. }) || node == test);
        }
        if matches!(
            action,
            Action::CallListArrows { .. } | Action::ScopedJoin { .. }
        ) {
            candidate = false;
            replacement_candidate = false;
            single_active_call = false;
        }
        // Tag reached sites during the ordinary visit without a second walk.
        if candidate {
            flat &= flat_action(action, &slots);
            fresh_result |= matches!(
                action,
                Action::ConstructVariant {
                    fresh_result_ok: true,
                    ..
                } | Action::ConstructResultError {
                    fresh_storage: Some(_),
                    ..
                }
            );
            if let Action::BudgetBackedge {
                forwarding_possible,
            } = action
            {
                loop_delivery = true;
                forwarding_backedge |= forwarding_possible;
            }
            branching |= matches!(edges, Edges::Branch { .. });
            if let Action::Call(index) = action {
                calls_reached += 1;
                call_block = Some(blocks.len());
                if let Some((id, _)) = return_contract {
                    let callee = calls[index].callee;
                    if callee.start == id.start && callee.kind == id.kind {
                        if facts.is_indexed() {
                            budget.step(id.module.path.len().checked_add(1).ok_or(Stop::Limit)?)?;
                        }
                        flat &= callee != id;
                    }
                }
            }
        }
        let mut join_resume = None;
        let mut fresh_result_storage = None;
        let operation = (|| -> ResultOf<_> {
            if callable_frame.is_some() {
                // A bound transfer has no proof for other calls or protected
                // cleanup. Stop this path, not independent paths in the body.
                budget.step(1 + unsupported_cleanup.len())?;
                if matches!(
                    action,
                    Action::Call(_)
                        | Action::Dispose { .. }
                        | Action::ConstructResultError { .. }
                        | Action::WrapResultFailure { .. }
                        | Action::DropResultFailure { .. }
                        | Action::CopyClosure { .. }
                        | Action::CallListArrows { .. }
                        | Action::ScopedJoin { .. }
                        | Action::InvokeClosurePair { .. }
                        | Action::InvokeDeclaredShared { .. }
                        | Action::InvokeMutableI32Cell { .. }
                ) || unsupported_cleanup
                    .iter()
                    .any(|&slot| matches!(state[slot], State::Initialized(_)))
                {
                    return Err(Stop::Pending(node));
                }
            }
            Ok(match action {
                Action::None | Action::BudgetBackedge { .. } => Operation::None,
                Action::Init(slot) => {
                    for field in slots[slot].storage(slot).skip(1) {
                        budget.step(1)?;
                        initialized(&state[field], node)?;
                    }
                    state[slot] = State::Initialized(empty.clone());
                    Operation::Initialize(local(slot))
                }
                Action::CommitList {
                    first,
                    count,
                    into,
                    backing,
                    provider,
                } => {
                    // Only this builder's private, borrow-free string or
                    // string-field record stages can reach commitment. The
                    // destination has no live owner.
                    if matches!(state[into], State::Initialized(_)) {
                        return Err(Stop::Pending(node));
                    }
                    let width = slots[first].storage(first).len();
                    let scalars = matches!(slots[into].cleanup, StorageCleanup::DisposeListI32(_));
                    budget.step(count * width)?;
                    budget.reserve(count)?;
                    let mut elements = Vec::with_capacity(count);
                    for (index, stage) in state[first..first + count * width]
                        .chunks_mut(width)
                        .enumerate()
                    {
                        for slot in stage.iter_mut() {
                            initialized(slot, node)?;
                            // A scalar stage is ended, as an equality ends its scratch.
                            *slot = if scalars {
                                State::Vacant
                            } else {
                                State::Moved(node)
                            };
                        }
                        elements.push(local(first + index * width));
                    }
                    state[into] = State::Initialized(empty.clone());
                    if scalars {
                        Operation::CommitListI32 {
                            elements,
                            into: local(into),
                            backing,
                            provider,
                        }
                    } else if width == 1 {
                        Operation::CommitListText {
                            elements,
                            into: local(into),
                            backing,
                            provider,
                        }
                    } else {
                        Operation::CommitListRecordText {
                            elements,
                            into: local(into),
                            backing,
                            provider,
                        }
                    }
                }
                Action::OpenOwnedList {
                    source,
                    cursor,
                    backing,
                    source_backing,
                    provider,
                } => {
                    require_initialized(&slots, &state, source, node, budget)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        source,
                        (node, Conflict::Release),
                        Access::Exclusive,
                        budget,
                    )?;
                    state[source] = State::Moved(node);
                    retire_loans(&slots, &mut state, &[source], budget)?;
                    state[cursor] = State::Initialized(empty.clone());
                    if matches!(
                        slots[cursor].cleanup,
                        StorageCleanup::DisposeOwnedListIterRecordText(_)
                    ) {
                        Operation::OpenOwnedListRecordText {
                            source: local(source),
                            cursor: local(cursor),
                            source_backing: source_backing.ok_or(Stop::Pending(node))?,
                            cursor_backing: backing,
                            provider,
                        }
                    } else if matches!(
                        slots[cursor].cleanup,
                        StorageCleanup::DisposeOwnedListIterOptionText(_)
                    ) {
                        Operation::OpenOwnedListOptionText {
                            source: local(source),
                            cursor: local(cursor),
                            source_backing: source_backing.ok_or(Stop::Pending(node))?,
                            cursor_backing: backing,
                            provider,
                        }
                    } else if let Some(source_backing) = source_backing {
                        Operation::OpenOwnedListText {
                            source: local(source),
                            cursor: local(cursor),
                            source_backing,
                            cursor_backing: backing,
                            provider,
                        }
                    } else {
                        Operation::OpenOwnedListI32 {
                            source: local(source),
                            cursor: local(cursor),
                            backing,
                            provider,
                        }
                    }
                }
                Action::AdvanceOwnedList {
                    cursor,
                    into,
                    some,
                    none,
                    provider,
                } => {
                    require_initialized(&slots, &state, cursor, node, budget)?;
                    if let Some(slot) = into {
                        // A scalar or string binder is its root alone; only
                        // record fields are additional work.
                        let region = slots[slot].storage(slot);
                        budget.step(region.len() - 1)?;
                        if !state[region].iter().all(|s| matches!(s, State::Vacant)) {
                            return Err(Stop::Pending(node));
                        }
                    }
                    if matches!(
                        slots[cursor].cleanup,
                        StorageCleanup::DisposeOwnedListIterRecordText(_)
                    ) {
                        Operation::AdvanceOwnedListRecordText {
                            cursor: local(cursor),
                            into: local(into.ok_or(Stop::Pending(node))?),
                            some,
                            none,
                            provider,
                        }
                    } else if matches!(
                        slots[cursor].cleanup,
                        StorageCleanup::DisposeOwnedListIterOptionText(_)
                    ) {
                        Operation::AdvanceOwnedListOptionText {
                            cursor: local(cursor),
                            into: local(into.ok_or(Stop::Pending(node))?),
                            some,
                            none,
                            provider,
                        }
                    } else if matches!(
                        slots[cursor].cleanup,
                        StorageCleanup::DisposeOwnedListIterText(_)
                    ) {
                        Operation::AdvanceOwnedListText {
                            cursor: local(cursor),
                            into: local(into.ok_or(Stop::Pending(node))?),
                            some,
                            none,
                            provider,
                        }
                    } else {
                        Operation::AdvanceOwnedListI32 {
                            cursor: local(cursor),
                            into: into.map(local),
                            some,
                            none,
                            provider,
                        }
                    }
                }
                Action::Read { from, into } | Action::Move { from, into } => {
                    require_initialized(&slots, &state, from, node, budget)?;
                    let moves = matches!(action, Action::Move { .. }) || slots[from].moves;
                    if moves {
                        forbid_custom_partial(&slots, &state, from, node)?;
                    }
                    let kind = if moves {
                        Conflict::Release
                    } else {
                        Conflict::Access
                    };
                    let access = if moves {
                        Access::Exclusive
                    } else {
                        Access::Observe
                    };
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        from,
                        (node, kind),
                        access,
                        budget,
                    )?;
                    if let Some(into) = into
                        && slots[from].members != slots[into].members
                    {
                        return Err(Stop::Pending(node));
                    }
                    let carried = if into.is_some() {
                        aggregate_carried_loans(&state, &slots, from, budget)?
                    } else {
                        None
                    };
                    let region = slots[from].storage(from);
                    budget.step(region.len())?;
                    for member in region {
                        let mut held = if moves {
                            std::mem::replace(&mut state[member], State::Moved(node))
                        } else {
                            state[member].clone()
                        };
                        if let Some(into) = into {
                            if let (Some(loans), State::Initialized(value)) = (&carried, &mut held)
                            {
                                value.loans = loans.clone();
                            }
                            state[into + (member - from)] = held;
                        }
                    }
                    if moves {
                        retire_loans(&slots, &mut state, &[from], budget)?;
                    }
                    if moves {
                        Operation::Move {
                            from: local(from),
                            into: into.map(local),
                        }
                    } else {
                        Operation::Copy {
                            from: local(from),
                            into: into.map(local),
                        }
                    }
                }
                Action::CheckRebuild { base, staged } => {
                    budget.step(1)?;
                    initialized(&state[base], node)?;
                    if let Some((first, count)) = staged {
                        // A live root with a hole is outside this slice, not a
                        // rejected use of an omitted field or permission to repair it.
                        let region = slots[base].storage(base);
                        budget.step(region.len() + count)?;
                        for member in region.chain(first..first + count) {
                            if !matches!(&state[member], State::Initialized(held)
                                if held.loans.is_empty() && held.exact_target.is_none())
                            {
                                return Err(Stop::Pending(node));
                            }
                        }
                        forbid_active_escape(&state[base], node)?;
                        // Check the entire owner before any original field is retired.
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            base,
                            (node, Conflict::Release),
                            Access::Exclusive,
                            budget,
                        )?;
                    }
                    Operation::None
                }
                Action::CheckNestedFieldBase(child) => {
                    // Form the destination using the two headers, not a read
                    // of the whole record or its remaining leaves.
                    budget.step(2)?;
                    initialized(&state[slots[child].owner], node)?;
                    initialized(&state[child], node)?;
                    Operation::None
                }
                Action::UnloweredRebuildRead(source) => {
                    budget.step(1)?;
                    initialized(&state[source], node)?;
                    return Err(Stop::Pending(node));
                }
                Action::TestVariant { operand, success } => {
                    initialized(&state[operand], node)?;
                    Operation::TestVariant {
                        operand: local(operand),
                        success,
                    }
                }
                Action::CopyOptionPayload { operand, into } => {
                    initialized(&state[operand], node)?;
                    state[into] = State::Initialized(empty.clone());
                    Operation::CopyOptionPayload {
                        operand: local(operand),
                        into: local(into),
                    }
                }
                Action::TakeOptionPayload { operand, into } => {
                    initialized(&state[operand], node)?;
                    let value = std::mem::replace(&mut state[operand], State::Vacant);
                    // On the Some edge, take the descriptor and its backing,
                    // not a reference to the staged carrier slot.
                    state[into] = if matches!(
                        slots[operand].reference_option,
                        Some(ReferenceOption::Shared | ReferenceOption::Exclusive)
                    ) {
                        value
                    } else {
                        State::Initialized(empty.clone())
                    };
                    Operation::TakeOptionPayload {
                        operand: local(operand),
                        into: local(into),
                    }
                }
                Action::EndEmptyOption(operand) => {
                    initialized(&state[operand], node)?;
                    state[operand] = State::Vacant;
                    Operation::EndEmptyOption(local(operand))
                }
                Action::ConstructVariant {
                    variant,
                    payload,
                    into,
                    fresh_result_ok,
                } => {
                    state[into] = if let Some(payload) = payload {
                        initialized(&state[payload], node)?;
                        let value = std::mem::replace(&mut state[payload], State::Vacant);
                        if matches!(
                            slots[into].reference_option,
                            Some(ReferenceOption::Shared | ReferenceOption::Exclusive)
                        ) {
                            value
                        } else {
                            State::Initialized(empty.clone())
                        }
                    } else {
                        State::Initialized(empty.clone())
                    };
                    if fresh_result_ok && let State::Initialized(held) = &mut state[into] {
                        held.origin = ValueOrigin::FreshResultOk;
                    }
                    Operation::ConstructVariant {
                        variant,
                        payload: payload.map(local),
                        into: local(into),
                    }
                }
                Action::ConstructResultError {
                    constructor,
                    variant,
                    wrapper,
                    payload,
                    into,
                    fresh_storage,
                } => {
                    initialized(&state[payload], node)?;
                    state[into] = std::mem::replace(&mut state[payload], State::Vacant);
                    if let Some(storage) = fresh_storage
                        && let State::Initialized(held) = &mut state[into]
                    {
                        held.origin = ValueOrigin::FreshResultErr(node);
                        fresh_result_storage = Some(storage);
                    }
                    Operation::ConstructResultError {
                        constructor,
                        variant,
                        wrapper,
                        payload: local(payload),
                        into: local(into),
                    }
                }
                Action::TakeResultSuccess { operand, into } => {
                    initialized(&state[operand], node)?;
                    state[operand] = State::Vacant;
                    if let Some(into) = into {
                        state[into] = State::Initialized(empty.clone());
                    }
                    Operation::TakeResultSuccess {
                        operand: local(operand),
                        into: into.map(local),
                    }
                }
                Action::ForwardResultFailure {
                    operand,
                    into,
                    variant,
                    wrapper,
                } => {
                    initialized(&state[operand], node)?;
                    state[into] = std::mem::replace(&mut state[operand], State::Vacant);
                    if let State::Initialized(held) = &mut state[into] {
                        // Propagation can change the Result's success type.
                        held.origin = ValueOrigin::Untracked;
                    }
                    Operation::ForwardResultFailure {
                        operand: local(operand),
                        into: local(into),
                        variant,
                        wrapper,
                    }
                }
                Action::DropResultFailure {
                    operand,
                    variant,
                    wrapper,
                    storage_source,
                } => {
                    initialized(&state[operand], node)?;
                    state[operand] = State::Vacant;
                    Operation::DropResultFailure {
                        operand: local(operand),
                        variant,
                        wrapper,
                        storage_source,
                    }
                }
                Action::WrapResultFailure {
                    operand,
                    payload,
                    into,
                    variant,
                    wrapper,
                    origin,
                    cause_display,
                } => {
                    initialized(&state[operand], node)?;
                    initialized(&state[payload], node)?;
                    state[into] = std::mem::replace(&mut state[operand], State::Vacant);
                    if let State::Initialized(held) = &mut state[into] {
                        // The new wrapper retains a cause, even when its operand did not.
                        held.origin = ValueOrigin::Untracked;
                    }
                    state[payload] = State::Vacant;
                    Operation::WrapResultFailure {
                        operand: local(operand),
                        payload: local(payload),
                        into: local(into),
                        variant,
                        wrapper,
                        origin,
                        cause_display,
                    }
                }
                Action::ForbiddenBorrowedMove(slot) => {
                    initialized(&state[slot], node)?;
                    return Err(Stop::Rejected {
                        site: node,
                        related: slots[slot].node,
                        kind: Conflict::ForbiddenMove,
                    });
                }
                Action::Borrow {
                    from,
                    into,
                    exclusive,
                    retained_target,
                } => {
                    require_initialized(&slots, &state, from, node, budget)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        from,
                        (node, Conflict::Access),
                        if exclusive {
                            Access::Exclusive
                        } else {
                            Access::Observe
                        },
                        budget,
                    )?;
                    let mut loans = BTreeSet::new();
                    insert_loan(
                        &mut loans,
                        Loan {
                            source: from,
                            site: node,
                            exclusive,
                            kind: LoanKind::Backing,
                        },
                        budget,
                    )?;
                    if !retained_target {
                        // An exact access value sees the current target. Only
                        // views without that route need a snapshot of backing.
                        let mut include = |loan: Loan, budget: &mut Budget| {
                            insert_loan(
                                &mut loans,
                                Loan {
                                    kind: if loan.kind.borrows_local_storage() {
                                        LoanKind::Contents
                                    } else {
                                        loan.kind
                                    },
                                    ..loan
                                },
                                budget,
                            )
                        };
                        visit_loans(&state, slots[from].storage(from), budget, &mut include)?;
                        budget.step(1)?;
                        if let Some(target) = initialized(&state[from], node)?.exact_target {
                            let contents = target.contents_slot();
                            let root = slots[contents].carried_loan_root(contents);
                            visit_loans(&state, slots[root].storage(root), budget, &mut include)?;
                        }
                    }
                    state[into] = State::Initialized(Held {
                        loans: loan_set(loans, budget)?,
                        exact_target: retained_target.then_some(ExactTarget::Local(from)),
                        ..Held::default()
                    });
                    Operation::Borrow {
                        source: local(from),
                        holder: local(into),
                        exclusive,
                    }
                }
                Action::Reborrow {
                    parent,
                    into,
                    exclusive,
                    suspends,
                } => {
                    let held = initialized(&state[parent], node)?;
                    let access = if exclusive || suspends {
                        Access::Exclusive
                    } else {
                        Access::Observe
                    };
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        parent,
                        (node, Conflict::Access),
                        access,
                        budget,
                    )?;
                    let owner = slots[parent].carried_loan_root(parent);
                    let loans = if suspends || owner != parent {
                        let mut loans = BTreeSet::new();
                        include_loans(&mut loans, &state, slots[owner].storage(owner), budget)?;
                        if suspends {
                            insert_loan(
                                &mut loans,
                                Loan {
                                    source: parent,
                                    site: node,
                                    exclusive: true,
                                    kind: LoanKind::Suspension,
                                },
                                budget,
                            )?;
                        }
                        loan_set(loans, budget)?
                    } else {
                        held.loans.clone()
                    };
                    state[into] = State::Initialized(Held {
                        loans,
                        exact_target: if exclusive { held.exact_target } else { None },
                        ..Held::default()
                    });
                    Operation::Reborrow {
                        parent: local(parent),
                        holder: local(into),
                        exclusive,
                    }
                }
                Action::Dereference {
                    from,
                    into,
                    explicit_copy,
                } => {
                    initialized(&state[from], node)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        from,
                        (node, Conflict::Access),
                        Access::Observe,
                        budget,
                    )?;
                    if let Some(into) = into {
                        state[into] = State::Initialized(empty.clone());
                    }
                    if let Some(into) = into.filter(|_| explicit_copy) {
                        Operation::CopyReferent {
                            from: local(from),
                            into: local(into),
                        }
                    } else {
                        Operation::Dereference(local(from))
                    }
                }
                Action::BindBorrowedMember {
                    descriptor,
                    projection,
                    into,
                    binding,
                } => {
                    let held = initialized(&state[descriptor], node)?;
                    let transfers = binding == BorrowedMemberBinding::ExclusiveView;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        descriptor,
                        (node, Conflict::Access),
                        if transfers {
                            Access::Exclusive
                        } else {
                            Access::Observe
                        },
                        budget,
                    )?;
                    state[into] = State::Initialized(borrowed_member_value(
                        binding, held, descriptor, node, budget,
                    )?);
                    if transfers {
                        state[descriptor] = State::Vacant;
                    }
                    Operation::BindBorrowedMember {
                        descriptor: local(descriptor),
                        projection,
                        into: local(into),
                        binding,
                    }
                }
                Action::Call(index)
                    if let CallResult::RetainsOptionCarrier(header) = calls[index].result =>
                {
                    let call = &calls[index];
                    let ([argument], Some(into)) = (call.args.as_slice(), call.into) else {
                        return Err(Stop::Pending(node));
                    };
                    let argument = *argument;
                    let actual = initialized(&state[argument], node)?;
                    forbid_active_escape(&state[argument], node)?;
                    if cache.is_none() {
                        // The selected frame must check without another such handoff.
                        return Err(Stop::Pending(node));
                    }
                    let body = lower(
                        typed,
                        module,
                        header,
                        Frame::OrdinaryFunction,
                        false,
                        Some(facts.nodes()),
                        Lowering {
                            budget,
                            cache: None,
                        },
                    )?;
                    graph::visit_dependencies(&body.mir, budget, |_, edge, _| {
                        Err(Stop::Pending(edge.site.node))
                    })?;
                    // Some may carry every actual input loan, not the stage's
                    // storage or a narrower set inferred from the selected body.
                    budget.reserve(1)?;
                    state[into] = State::Initialized(Held {
                        loans: actual.loans.clone(),
                        ..Held::default()
                    });
                    state[argument] = State::Vacant;
                    Operation::CallIntoTemporary {
                        callee: call.callee,
                        arguments: vec![local(argument)],
                        into: local(into),
                    }
                }
                Action::Call(index)
                    if let CallResult::TransfersCallable(header) = calls[index].result =>
                {
                    let call = &calls[index];
                    let ([argument], Some(into)) = (call.args.as_slice(), call.into) else {
                        return Err(Stop::Pending(node));
                    };
                    let actual = initialized(&state[*argument], node)?;
                    let (layout, exclusive) = match actual.callable_owner {
                        Some(CallableOwner::Pair(layout)) => (layout, false),
                        Some(CallableOwner::LocalExclusive(Some(layout))) => (layout, true),
                        _ => return Err(Stop::Pending(node)),
                    };
                    budget.step(actual.loans.len())?;
                    let backed = if exclusive {
                        // Carry the one actual loan, not a descriptor inferred
                        // from the parameter's or result's callable type.
                        matches!(actual.loans.as_ref(),
                            [Loan { source, exclusive: true, kind: LoanKind::Backing, .. }]
                                if *source != *argument && matches!(state[*source], State::Initialized(_)))
                    } else {
                        !actual
                            .loans
                            .iter()
                            .any(|loan| loan.exclusive || loan.source == *argument)
                    };
                    if !backed || !matches!(state[into], State::Vacant) {
                        return Err(Stop::Pending(node));
                    }
                    let cache = cache.as_deref_mut().ok_or(Stop::Pending(node))?;
                    cache.share_target_nodes(module, &mut facts, budget)?;
                    let body = cache
                        .callable_transfer(typed, module, header, layout, facts.nodes(), budget)
                        .map_err(|stop| match stop {
                            Stop::Pending(_) => Stop::Pending(node),
                            _ => stop,
                        })?;
                    // Declared conservative may-carry behavior: retain all
                    // actual input loans without the origin or exact targets.
                    let loans = actual.loans.clone();
                    state[*argument] = State::Vacant;
                    state[into] = State::Initialized(Held {
                        loans,
                        callable_owner: Some(CallableOwner::Returned(layout)),
                        ..Held::default()
                    });
                    candidate = false;
                    Operation::CallCallableTransfer {
                        callee: call.callee,
                        argument: local(*argument),
                        into: local(into),
                        layout,
                        body,
                    }
                }
                Action::Call(index) => {
                    let call = &calls[index];
                    let mut retained = BTreeSet::new();
                    let contents = if let Some(destination) = call.destination {
                        let (access, contents, target) = match destination.storage {
                            CallStorage::Local(slot) => {
                                (slot, slot, RetainedPlace::Local(local(slot)))
                            }
                            CallStorage::ThroughParameter {
                                descriptor,
                                contents,
                            } => (
                                descriptor,
                                contents,
                                RetainedPlace::ThroughParameter(local(descriptor)),
                            ),
                            CallStorage::ThroughTwoHopParameter(route) => {
                                initialized(&state[route.contents], node)?;
                                (
                                    route.descriptor,
                                    route.contents,
                                    RetainedPlace::ThroughTwoHopParameter {
                                        parameter: local(route.descriptor),
                                        argument: local(destination.argument),
                                    },
                                )
                            }
                            CallStorage::ThroughAlias { descriptor } => {
                                match exact_target(&state, destination.argument, node, budget)? {
                                    ExactTarget::Local(target) => (
                                        descriptor,
                                        target,
                                        RetainedPlace::ThroughAlias {
                                            descriptor: local(descriptor),
                                            target: local(target),
                                        },
                                    ),
                                    ExactTarget::Caller {
                                        parameter,
                                        contents,
                                    } => (
                                        descriptor,
                                        contents,
                                        RetainedPlace::ThroughParameterAlias {
                                            descriptor: local(descriptor),
                                            parameter: local(parameter),
                                        },
                                    ),
                                }
                            }
                        };
                        initialized(&state[access], node)?;
                        let mut loans = BTreeSet::new();
                        include_loans(
                            &mut loans,
                            &state,
                            slots[contents].storage(contents),
                            budget,
                        )?;
                        for slot in
                            std::iter::once(destination.input).chain(destination.second_input)
                        {
                            initialized(&state[slot], node)?;
                            include_loans(&mut loans, &state, slots[slot].storage(slot), budget)?;
                        }
                        // The access argument is temporary. It cannot become a
                        // backing source of the very descriptor it gives access to.
                        let held = Held {
                            loans: loan_set(loans, budget)?,
                            origin: if matches!(
                                target,
                                RetainedPlace::ThroughParameter(_)
                                    | RetainedPlace::ThroughTwoHopParameter { .. }
                                    | RetainedPlace::ThroughParameterAlias { .. }
                            ) {
                                ValueOrigin::RetainedStore(node)
                            } else {
                                ValueOrigin::Untracked
                            },
                            ..Held::default()
                        };
                        Some((target, contents, destination, held))
                    } else {
                        None
                    };
                    for &arg in &call.args {
                        require_initialized(&slots, &state, arg, node, budget)?;
                        forbid_active_escape(&state[arg], node)?;
                        let storage = slots[arg].storage(arg);
                        if matches!(
                            call.result,
                            CallResult::RetainsInputs
                                | CallResult::RetainsRecordInput
                                | CallResult::RetainsExclusiveInput
                        ) && call.into.is_some()
                        {
                            // A staged record borrow holds both whole access and its
                            // contained sources; neither is narrowed at the call.
                            include_loans(&mut retained, &state, storage.clone(), budget)?;
                        }
                        budget.step(storage.len())?;
                        state[storage].fill(State::Vacant);
                    }
                    if let Some(into) = call.into {
                        let storage = slots[into].storage(into);
                        budget.step(storage.len())?;
                        state[storage].fill(State::Initialized(empty.clone()));
                        state[into] = State::Initialized(Held {
                            loans: loan_set(retained, budget)?,
                            ..Held::default()
                        });
                    }
                    if let Some((destination, contents, retention, held)) = contents {
                        state[contents] = State::Initialized(held);
                        if let Some(RetainedCallExtra::OptionBool) = retention.extra {
                            budget.step(call.args.len())?;
                            Operation::CallRetainingWithOptionBool {
                                callee: call.callee,
                                destination,
                                arguments: std::array::from_fn(|i| local(call.args[i])),
                                into: local(call.into.ok_or(Stop::Pending(node))?),
                            }
                        } else if retention.extra.is_some() {
                            budget.step(call.args.len())?;
                            budget.reserve(call.args.len())?;
                            Operation::CallRetainingWithBool {
                                callee: call.callee,
                                destination,
                                arguments: call.args.iter().copied().map(local).collect(),
                                input: local(retention.input),
                                second_input: retention.second_input.map(local),
                            }
                        } else if let Some(second) = retention.second_input {
                            Operation::CallRetainingPair {
                                callee: call.callee,
                                destination,
                                inputs: [local(retention.input), local(second)],
                            }
                        } else {
                            Operation::CallRetaining {
                                callee: call.callee,
                                destination,
                                input: local(retention.input),
                            }
                        }
                    } else if matches!(call.result, CallResult::ScalarCopy) {
                        let [argument] = call.args.as_slice() else {
                            return Err(Stop::Pending(node));
                        };
                        Operation::CallScalarCopy {
                            implementation: call.callee,
                            argument: local(*argument),
                            into: call.into.map(local),
                        }
                    } else if matches!(call.result, CallResult::RetainsExclusiveInput) {
                        let [argument] = call.args.as_slice() else {
                            return Err(Stop::Pending(node));
                        };
                        Operation::CallExclusiveView {
                            callee: call.callee,
                            argument: local(*argument),
                            into: local(call.into.ok_or(Stop::Pending(node))?),
                        }
                    } else if matches!(call.result, CallResult::RetainsRecordInput) {
                        let [argument] = call.args.as_slice() else {
                            return Err(Stop::Pending(node));
                        };
                        Operation::CallBorrowingRecord {
                            callee: call.callee,
                            argument: local(*argument),
                            into: local(call.into.ok_or(Stop::Pending(node))?),
                        }
                    } else if let Some(into) = call
                        .into
                        .filter(|&slot| slots[slot].scalar_borrow_temporary)
                    {
                        budget.step(call.args.len())?;
                        budget.reserve(call.args.len())?;
                        Operation::CallIntoTemporary {
                            callee: call.callee,
                            arguments: call.args.iter().copied().map(local).collect(),
                            into: local(into),
                        }
                    } else {
                        Operation::Call(call.callee)
                    }
                }
                Action::FormListArrow {
                    capture,
                    into,
                    mode,
                } => {
                    let staged = initialized(&state[capture], node)?;
                    state[into] = State::Initialized(Held {
                        loans: staged.loans.clone(),
                        callable_owner: Some(CallableOwner::ListArrow(node)),
                        ..Held::default()
                    });
                    state[capture] = State::Vacant;
                    Operation::FormClosure {
                        closure: local(into).root,
                        capture: local(capture).root,
                        mode,
                    }
                }
                Action::FormScopedValueArrow { capture, into } => {
                    if !matches!(state[into], State::Vacant) {
                        return Err(Stop::Pending(node));
                    }
                    if let Some(from) = capture {
                        require_initialized(&slots, &state, from, node, budget)?;
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            from,
                            (node, Conflict::Access),
                            Access::Observe,
                            budget,
                        )?;
                        if initialized(&state[from], node)? != &Held::default() {
                            return Err(Stop::Pending(node));
                        }
                    }
                    state[into] = State::Initialized(Held {
                        callable_owner: Some(CallableOwner::ScopedValue(slots[into].node)),
                        ..Held::default()
                    });
                    Operation::FormScopedValueArrow {
                        environment: local(into),
                        capture: capture.map(local),
                    }
                }
                Action::CheckScopedReferenceResults {
                    entries,
                    body_depths,
                } => {
                    // Both entry inputs must have formed, in written order, before
                    // even an independently known invalid child can export.
                    for stage in entries {
                        budget.step(1)?;
                        initialized(&state[stage], node)?;
                    }
                    build::check_scoped_reference_results(
                        typed,
                        module,
                        facts.nodes(),
                        node,
                        body_depths,
                        budget,
                    )?;
                    return Err(Stop::Pending(node));
                }
                Action::OverlappingScopedJoin { .. } => return Err(Stop::Pending(node)),
                Action::ScopedJoin {
                    pair,
                    receivers,
                    into,
                    role,
                } => {
                    let Edges::Next(resume) = edges else {
                        return Err(Stop::Pending(node));
                    };
                    if active.is_some() {
                        return Err(Stop::Pending(node));
                    }
                    let mut ordinary_frames = || -> ResultOf<_> {
                        let deferred;
                        let frames = match pair {
                            ScopedFrames::FreshResult { .. } => return Err(Stop::Pending(node)),
                            ScopedFrames::Prepared(index) => &list_arrow_pairs[index],
                            ScopedFrames::DisjointExclusive {
                                sources,
                                tuple_depth,
                            } => {
                                deferred = build::reached_exclusive_pair(
                                    typed,
                                    module,
                                    facts.nodes(),
                                    node,
                                    (sources, tuple_depth),
                                    &slots,
                                    budget,
                                )?;
                                &deferred
                            }
                        };
                        budget.reserve(2)?;
                        for (receiver, child) in receivers.into_iter().zip(&frames.children) {
                            if matches!(child.body, build::ArrowBody::Reference(_)) {
                                budget.step(1)?;
                                initialized(&state[receiver], node)?;
                            }
                            conflict(
                                &slots,
                                &state,
                                &live[pc],
                                receiver,
                                (node, Conflict::Access),
                                Access::Exclusive,
                                budget,
                            )?;
                            let exclusive = matches!(
                                child.body,
                                build::ArrowBody::List {
                                    exclusive: true,
                                    ..
                                }
                            );
                            // Check the transferred access, not an observation
                            // by the parent. Staged::check binds this loan to
                            // the child's actual capture site and backing proxy.
                            conflict(
                                &slots,
                                &state,
                                &live[pc],
                                child.source,
                                (node, Conflict::Access),
                                if exclusive {
                                    Access::ThroughExclusive(receiver)
                                } else {
                                    Access::Observe
                                },
                                budget,
                            )?;
                        }
                        // Both captures have formed before probing either
                        // reference child. The scalar frames are prebuilt.
                        let mut frame = |i: usize| -> ResultOf<_> {
                            let child = &frames.children[i];
                            let environment = local(receivers[i]);
                            let (incoming_list_backing, exclusive, body) = match &child.body {
                                build::ArrowBody::List {
                                    incoming_backing,
                                    exclusive,
                                    mir,
                                } => (Some(*incoming_backing), *exclusive, Arc::clone(mir)),
                                build::ArrowBody::Reference(ty) => (
                                    None,
                                    false,
                                    build::lower_reference_child(
                                        typed,
                                        module,
                                        child,
                                        environment.root.node,
                                        ty,
                                        budget,
                                    )?,
                                ),
                            };
                            Ok(ScopedJoinFrame {
                                origin: environment.root,
                                invocation: child.invocation,
                                body_site: child.body_site,
                                environment,
                                capture: Some(child.capture),
                                backing: Some(local(child.source)),
                                incoming_list_backing,
                                exclusive,
                                result: local(into + 1 + i),
                                body,
                                value: false,
                                result_origin: None,
                            })
                        };
                        Ok([frame(0)?, frame(1)?])
                    };
                    let parent = CancellationFrame {
                        module,
                        disposer: Some(disposer),
                        slots: &slots,
                        locals: body_scope.map_or(&[], |scope| &cleanups[scope]),
                        params: &params,
                        retained: &retained,
                        indexed: facts.is_indexed(),
                    };
                    let frames_and_exit = if let ScopedFrames::FreshResult { tuple_depth } = pair {
                        // Do not lower child frames unless the actual
                        // captures and the parent's entry-delivery exit check.
                        let before_commit = scoped_join::before_fresh_commit(
                            &parent, &state, receivers, &live[pc], site, budget,
                        )?;
                        let frames = build::lower_fresh_scoped_children(
                            typed,
                            module,
                            facts.nodes(),
                            node,
                            tuple_depth,
                            std::array::from_fn(|i| (local(receivers[i]), local(into + 1 + i))),
                            budget,
                        )?;
                        (frames, Some(before_commit))
                    } else {
                        (ordinary_frames()?, None)
                    };
                    let (proof, resume, parent_state) = scoped_join::installed::check(
                        &parent,
                        &state,
                        (receivers, into),
                        frames_and_exit,
                        (site, resume),
                        role,
                        budget,
                    )?;
                    state = parent_state;
                    join_resume = Some(resume);
                    Operation::ScopedJoin(proof)
                }
                Action::CallListArrows {
                    pair,
                    receivers,
                    into,
                } => {
                    let pair = &list_arrow_pairs[pair];
                    budget.reserve(2)?;
                    let mut invoke = |position: usize| -> ResultOf<ListArrowFrame<'g>> {
                        let receiver = receivers[position];
                        let body = &pair.children[position];
                        let build::ArrowBody::List {
                            incoming_backing,
                            exclusive: false,
                            mir,
                        } = &body.body
                        else {
                            return Err(Stop::Pending(node));
                        };
                        let environment = local(receiver);
                        let origin = environment.root;
                        let held = initialized(&state[receiver], node)?;
                        budget.step(1)?;
                        if held.callable_owner != Some(CallableOwner::ListArrow(origin.node))
                            || !matches!(held.loans.as_ref(), [Loan {
                                source, site, exclusive: false, kind: LoanKind::Backing,
                            }] if *source == body.source && *site == body.capture.root.node)
                        {
                            return Err(Stop::Pending(node));
                        }
                        require_initialized(&slots, &state, body.source, node, budget)?;
                        for (slot, access) in [
                            (receiver, Access::Exclusive),
                            (body.source, Access::Observe),
                        ] {
                            conflict(
                                &slots,
                                &state,
                                &live[pc],
                                slot,
                                (node, Conflict::Access),
                                access,
                                budget,
                            )?;
                        }
                        // The single loan moves into the child descriptor and
                        // ends at its checked cleanup, not again in the caller.
                        state[receiver] = State::Moved(node);
                        retire_loans(&slots, &mut state, &[receiver], budget)?;
                        state[into + 1 + position] = State::Initialized(empty.clone());
                        Ok(ListArrowFrame {
                            origin,
                            invocation: body.invocation,
                            body_site: body.body_site,
                            environment,
                            capture: body.capture,
                            backing: local(body.source),
                            incoming_backing: *incoming_backing,
                            result: local(into + 1 + position),
                            body: Arc::clone(mir),
                        })
                    };
                    let slots = Box::new([invoke(0)?, invoke(1)?]);
                    state[into] = State::Initialized(empty.clone());
                    Operation::CallListArrows {
                        implementation: pair.implementation,
                        body: pair.body,
                        slots,
                    }
                }
                Action::FormClosure {
                    capture,
                    second,
                    into,
                    mode,
                    cell_addend,
                    layout,
                    local_exclusive,
                } => {
                    let first = initialized(&state[capture], node)?;
                    let loans = if let Some(second) = second {
                        initialized(&state[second], node)?;
                        let mut loans = BTreeSet::new();
                        for slot in [capture, second] {
                            include_loans(&mut loans, &state, slots[slot].storage(slot), budget)?;
                        }
                        loan_set(loans, budget)?
                    } else {
                        first.loans.clone()
                    };
                    let drop_of = |slot: Slot| {
                        (slots[slot].cleanup == StorageCleanup::Drop).then_some(slots[slot].node)
                    };
                    let track_single = if second.is_none()
                        && cell_addend.is_none()
                        && !copied_origins.is_empty()
                    {
                        budget.step(1)?;
                        copied_origins.contains(&node)
                    } else {
                        false
                    };
                    let callable_owner = if local_exclusive {
                        let transport = if let (Some(layout), Some(second)) = (layout, second) {
                            budget.step(1)?;
                            // The exclusive backing must be in the first
                            // descriptor, not merely somewhere in the union.
                            (matches!(
                                first.loans.as_ref(),
                                [Loan {
                                    exclusive: true,
                                    kind: LoanKind::Backing,
                                    ..
                                }]
                            ) && initialized(&state[second], node)?.loans.is_empty())
                            .then_some(layout)
                        } else {
                            None
                        };
                        Some(CallableOwner::LocalExclusive(transport))
                    } else if track_single {
                        Some(CallableOwner::LocalCapture(node))
                    } else {
                        layout.map(CallableOwner::Pair)
                    };
                    state[into] = State::Initialized(Held {
                        loans,
                        capture_drops: [drop_of(capture), second.and_then(drop_of)],
                        callable_owner,
                        // An exclusive capture transfers locally but grants no
                        // invocation proof, even through a weakened interface.
                        pair_origin: (second.is_some() && !local_exclusive).then_some((node, mode)),
                        single_body: match cell_addend {
                            Some(addend) => Some(SingleBody::MutableCell(MutableCell {
                                origin: node,
                                field: slots[capture].node,
                                addend,
                            })),
                            // Only where a frame reads it: other bodies keep
                            // joining distinct one-capture formations.
                            None => (declared_shared_frame
                                && second.is_none()
                                && !local_exclusive
                                && mode == InvocationMode::ReadOnly)
                                .then_some(SingleBody::Scalar(node)),
                        },
                        ..Held::default()
                    });
                    state[capture] = State::Vacant;
                    let closure = local(into).root;
                    if let Some(second) = second {
                        state[second] = State::Vacant;
                        Operation::FormClosurePair {
                            closure,
                            captures: [local(capture).root, local(second).root],
                            mode,
                        }
                    } else {
                        Operation::FormClosure {
                            closure,
                            capture: local(capture).root,
                            mode,
                        }
                    }
                }
                Action::CopyClosure { from, into } => {
                    use crate::analysis::{
                        scalars::{data::DataExpression, generics::CaptureCopyAction},
                        typed::PrimitiveOperation,
                    };
                    budget.step(facts.nodes().len().checked_ilog2().unwrap_or(0) as usize + 1)?;
                    let Some(NodeFactKind::Expression(value)) =
                        facts.nodes().get(&node).map(|f| &f.kind)
                    else {
                        return Err(Stop::Pending(node));
                    };
                    let Some(DataExpression::CallableCopy(selection)) = &value.data else {
                        return Err(Stop::Pending(node));
                    };
                    let source = initialized(&state[from], node)?;
                    let origin = match source.callable_owner {
                        Some(CallableOwner::LocalCapture(origin)) => Some(origin),
                        Some(CallableOwner::Pair(_)) => {
                            source.pair_origin.map(|(origin, _)| origin)
                        }
                        _ => None,
                    };
                    if origin != Some(selection.origin.node)
                        || !source.loans.is_empty()
                        || matches!(&state[into], State::Initialized(_))
                    {
                        return Err(Stop::Pending(node));
                    }
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        from,
                        (node, Conflict::Access),
                        Access::Observe,
                        budget,
                    )?;
                    let mut drops = [None; 2];
                    let fields = selection.captures.len();
                    if fields == 0 || fields > drops.len() {
                        return Err(Stop::Pending(node));
                    }
                    budget.step(fields)?;
                    budget.reserve(fields)?;
                    let mut captures = Vec::with_capacity(fields);
                    for (drop, capture) in drops.iter_mut().zip(&selection.captures) {
                        let field = capture.site;
                        let effect = match capture.action {
                            CaptureCopyAction::Scalar(p) => CaptureCopyEffect::Scalar {
                                field,
                                implementation: &p.implementation.id,
                                primitive: p.primitive,
                            },
                            CaptureCopyAction::Text { copy, dispose } => {
                                let (Some(copy), Some(dispose)) = (copy.binding, dispose.binding)
                                else {
                                    return Err(Stop::Pending(node));
                                };
                                if copy.operation != PrimitiveOperation::CopyText
                                    || dispose.operation != PrimitiveOperation::DisposeText
                                {
                                    return Err(Stop::Pending(node));
                                }
                                *drop = Some(field.node);
                                CaptureCopyEffect::Text {
                                    field,
                                    copy: copy.operation,
                                    dispose: dispose.operation,
                                }
                            }
                        };
                        captures.push(effect);
                    }
                    if source.capture_drops != drops {
                        return Err(Stop::Pending(node));
                    }
                    // The effect initializes a fresh owner, not another
                    // descriptor of the source environment. Its cleanup starts
                    // only after both capture copy actions (if present).
                    state[into] = State::Initialized(Held {
                        capture_drops: drops,
                        callable_owner: source.callable_owner,
                        pair_origin: source.pair_origin,
                        ..Held::default()
                    });
                    Operation::CopyClosure {
                        from: local(from),
                        into: local(into),
                        origin: selection.origin,
                        captures,
                    }
                }
                Action::InvokeMutableI32Cell { receiver, into } => {
                    let Some(SingleBody::MutableCell(cell)) =
                        initialized(&state[receiver], node)?.single_body
                    else {
                        return Err(Stop::Pending(node));
                    };
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        receiver,
                        (node, Conflict::Access),
                        Access::Exclusive,
                        budget,
                    )?;
                    // The cell contains only a scalar. The access ends before
                    // the continuation and no receiver loan escapes in the result.
                    state[into] = State::Initialized(empty.clone());
                    Operation::InvokeMutableI32Cell {
                        receiver: local(receiver),
                        origin: site_at(module, cell.origin),
                        field: site_at(module, cell.field),
                        addend: cell.addend,
                        into: local(into),
                    }
                }
                Action::InvokeClosurePair {
                    receiver,
                    into,
                    call,
                } => {
                    let descriptor = initialized(&state[receiver], node)?;
                    let owner = if descriptor.pair_origin.is_some() {
                        receiver
                    } else if matches!(call, PairCall::ReadOnly { .. })
                        && descriptor.callable_owner.is_none()
                        && !descriptor.loans.is_empty()
                    {
                        budget
                            .step(facts.nodes().len().checked_ilog2().unwrap_or(0) as usize + 1)?;
                        let invocation = facts
                            .nodes()
                            .get(&node)
                            .and_then(|fact| fact.callable.as_ref()?.invocation.as_deref())
                            .ok_or(Stop::Pending(node))?;
                        if invocation.access != crate::analysis::scalars::data::Access::ReadOnly {
                            return Err(Stop::Pending(node));
                        }
                        shared_pair_owner(descriptor, node, budget)?
                    } else {
                        return Err(Stop::Pending(node));
                    };
                    let held = initialized(&state[owner], node)?;
                    // A transport layout is not a locally checked body.
                    let (origin, mode) = held.pair_origin.ok_or(Stop::Pending(node))?;
                    // The frame's argument is the written actual's own stage.
                    if matches!(call, PairCall::DeclaredOnce { .. })
                        && origin != slots[receiver].node
                    {
                        return Err(Stop::Pending(node));
                    }
                    let (kind, access) = if let PairCall::ReadOnly { typed_origin } = call {
                        if mode != InvocationMode::ReadOnly
                            || typed_origin.is_some_and(|expected| origin != expected)
                            || held.capture_drops != [None, None]
                        {
                            return Err(Stop::Pending(node));
                        }
                        (Conflict::Access, Access::Observe)
                    } else {
                        (Conflict::Release, Access::Exclusive)
                    };
                    let loans = held.loans.clone();
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        receiver,
                        (node, kind),
                        access,
                        budget,
                    )?;
                    if owner != receiver {
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            owner,
                            (node, Conflict::Access),
                            Access::Observe,
                            budget,
                        )?;
                    }
                    let storage = slots[into].storage(into);
                    budget.step(storage.len())?;
                    state[storage].fill(State::Initialized(empty.clone()));
                    // Reference members consult the environment's carried sources
                    // at the tuple root, not the access loan to the environment.
                    state[into] = State::Initialized(Held {
                        loans,
                        ..Held::default()
                    });
                    match call {
                        PairCall::ReadOnly { .. } => Operation::InvokeReadOnlyClosurePair {
                            receiver: local(owner),
                            origin: site_at(module, origin),
                            into: local(into),
                        },
                        PairCall::Once => {
                            state[receiver] = State::Moved(node);
                            retire_loans(&slots, &mut state, &[receiver], budget)?;
                            Operation::InvokeClosurePair {
                                receiver: local(receiver),
                                origin: site_at(module, origin),
                                into: local(into),
                            }
                        }
                        // The frame ends exactly the staged actual: the formal
                        // receives it and its one invocation consumes it.
                        PairCall::DeclaredOnce {
                            callee,
                            formal,
                            invocation,
                        } => {
                            state[receiver] = State::Moved(node);
                            retire_loans(&slots, &mut state, &[receiver], budget)?;
                            Operation::InvokeDeclaredOnce {
                                callee,
                                formal: site_at(module, formal),
                                invocation: site_at(module, invocation),
                                argument: local(receiver),
                                origin: site_at(module, origin),
                                into: local(into),
                            }
                        }
                    }
                }
                Action::InvokeDeclaredShared {
                    argument,
                    into,
                    callee,
                    formal,
                    invocation,
                } => {
                    // The argument is a plain shared loan of one local owner.
                    let descriptor = initialized(&state[argument], node)?;
                    if descriptor.callable_owner.is_some()
                        || descriptor.pair_origin.is_some()
                        || descriptor.single_body.is_some()
                    {
                        return Err(Stop::Pending(node));
                    }
                    let owner = shared_pair_owner(descriptor, node, budget)?;
                    let held = initialized(&state[owner], node)?;
                    // The reaching formation's body is its Bool capture: the
                    // result copies it and nothing in it is borrowed or dropped.
                    let Some(SingleBody::Scalar(origin)) = held.single_body else {
                        return Err(Stop::Pending(node));
                    };
                    if held.capture_drops != [None, None] || !held.loans.is_empty() {
                        return Err(Stop::Pending(node));
                    }
                    for slot in [argument, owner] {
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            slot,
                            (node, Conflict::Access),
                            Access::Observe,
                            budget,
                        )?;
                    }
                    state[into] = State::Initialized(empty.clone());
                    // The formal receives the loan and its one invocation is
                    // its last use. The owner stays with the caller.
                    state[argument] = State::Moved(node);
                    retire_loans(&slots, &mut state, &[argument], budget)?;
                    Operation::InvokeDeclaredShared {
                        callee,
                        formal: site_at(module, formal),
                        invocation: site_at(module, invocation),
                        argument: local(argument),
                        receiver: local(owner),
                        origin: site_at(module, origin),
                        into: local(into),
                    }
                }
                Action::Dispose {
                    operand: slot,
                    target,
                } => {
                    require_initialized(&slots, &state, slot, node, budget)?;
                    forbid_custom_partial(&slots, &state, slot, node)?;
                    forbid_active_escape(&state[slot], node)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        slot,
                        (node, Conflict::Release),
                        Access::Exclusive,
                        budget,
                    )?;
                    let implementation = owed_disposer(slots[slot], &state[slot]);
                    let storage = slots[slot].storage(slot);
                    budget.step(storage.len())?;
                    state[storage].fill(State::Moved(node));
                    retire_loans(&slots, &mut state, &[slot], budget)?;
                    if let Some(implementation) = implementation {
                        budget.reserve(1)?;
                        Operation::Cleanup(vec![Cleanup::InvokeDisposer {
                            operand: local(slot),
                            implementation,
                        }])
                    } else if let Some(release) = slots[slot].guard_release() {
                        budget.reserve(1)?;
                        Operation::Cleanup(vec![release.cleanup(local(slot))])
                    } else if let StorageCleanup::DisposeListI32(disposal) = slots[slot].cleanup {
                        budget.reserve(1)?;
                        Operation::Cleanup(vec![disposal.cleanup(local(slot))])
                    } else if let StorageCleanup::DisposeListText(disposal) = slots[slot].cleanup {
                        budget.reserve(1)?;
                        Operation::Cleanup(vec![Cleanup::DisposeListText {
                            operand: local(slot),
                            implementation: disposal.list.implementation,
                            bridge: disposal.list.bridge,
                            element: disposal.element,
                        }])
                    } else if let WrittenDisposal::Text(element) = target {
                        Operation::DisposeText {
                            operand: local(slot),
                            element,
                        }
                    } else if let WrittenDisposal::Scalar(implementation) = target {
                        Operation::CallScalarDispose {
                            implementation,
                            operand: local(slot),
                        }
                    } else if let WrittenDisposal::Descriptor = target {
                        Operation::EndDescriptor {
                            operand: local(slot),
                        }
                    } else {
                        Operation::Dispose {
                            callee: dispose.ok_or(Stop::Pending(node))?,
                            operand: local(slot),
                        }
                    }
                }
                Action::Displace { target, .. } => {
                    // Check the enclosing storage, not its fields: this write
                    // can repair a partial move but cannot rebuild a moved owner.
                    let owner = slots[target].owner;
                    if owner != target {
                        budget.step(1)?;
                        if !matches!(state[owner], State::Initialized(_)) {
                            return Err(Stop::Pending(node));
                        }
                    }
                    if slots[target].needs_boundary_cleanup() {
                        if disposal_origin(&state[target]).is_some() {
                            return Err(Stop::Pending(node));
                        }
                        Operation::Cleanup(cleanup_boundary(
                            cleanup_context,
                            &slots,
                            &mut state,
                            &[target],
                            &live[pc],
                            (node, Boundary::Replacement),
                            budget,
                        )?)
                    } else {
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            target,
                            (node, Conflict::Release),
                            Access::Exclusive,
                            budget,
                        )?;
                        let displaced = std::mem::replace(&mut state[target], State::Vacant);
                        retire_loans(&slots, &mut state, &[target], budget)?;
                        cleanup_operation(cleanup_context, slots[target], displaced, node, budget)?
                    }
                }
                Action::Install { target, incoming } => {
                    require_initialized(&slots, &state, incoming, node, budget)?;
                    if slots[target].members != slots[incoming].members {
                        return Err(Stop::Pending(node));
                    }
                    let storage = slots[target].storage(target);
                    budget.step(storage.len())?;
                    for member in storage {
                        state[member] = std::mem::replace(
                            &mut state[incoming + (member - target)],
                            State::Vacant,
                        );
                    }
                    Operation::Install {
                        target: local(target),
                        incoming: local(incoming),
                    }
                }
                Action::ReadListI32 {
                    view,
                    index,
                    into,
                    owner,
                    representation,
                    provider,
                } => {
                    budget.step(1)?;
                    let held = initialized(&state[view], node)?;
                    let [
                        Loan {
                            source: backing,
                            exclusive,
                            kind: LoanKind::Backing,
                            ..
                        },
                    ] = held.loans.as_ref()
                    else {
                        return Err(Stop::Pending(node));
                    };
                    let (backing, exclusive) = (*backing, *exclusive);
                    if exclusive && incoming_list_arrow != Some((view, backing, true)) {
                        return Err(Stop::Pending(node));
                    }
                    require_initialized(&slots, &state, backing, node, budget)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        view,
                        (node, Conflict::Access),
                        Access::Observe,
                        budget,
                    )?;
                    // Only the transferred descriptor may access its backing
                    // through this loan. Keep the restriction on all other
                    // access; this operation reads only the scalar element.
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        backing,
                        (node, Conflict::Access),
                        if exclusive {
                            Access::ThroughExclusive(view)
                        } else {
                            Access::Observe
                        },
                        budget,
                    )?;
                    if let Some(into) = into {
                        state[into] = State::Initialized(empty.clone());
                    }
                    Operation::ReadListI32 {
                        view: local(view),
                        backing: local(backing),
                        exclusive,
                        index,
                        into: into.map(local),
                        owner,
                        representation,
                        provider,
                    }
                }
                Action::EqualI32Literal {
                    from,
                    literal,
                    into,
                } => {
                    budget.step(1)?;
                    require_initialized(&slots, &state, from, node, budget)?;
                    if !matches!(state[into], State::Vacant) {
                        return Err(Stop::Pending(node));
                    }
                    // Both slots are fresh scalars in the List child, with
                    // no incoming aliases to the read scratch or result.
                    state[from] = State::Vacant;
                    state[into] = State::Initialized(empty.clone());
                    Operation::EqualI32Literal {
                        from: local(from),
                        literal,
                        into: local(into),
                    }
                }
                Action::CheckedI32Multiply { left, into, .. }
                | Action::CheckedI32Add { left, into, .. } => {
                    let right = match action {
                        Action::CheckedI32Add { right, .. } => Some(right),
                        _ => None,
                    };
                    for operand in std::iter::once(left).chain(right) {
                        require_initialized(&slots, &state, operand, node, budget)?;
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            operand,
                            (node, Conflict::Access),
                            Access::Observe,
                            budget,
                        )?;
                    }
                    budget.step(1)?;
                    if !matches!(state[into], State::Vacant) {
                        return Err(Stop::Pending(node));
                    }
                    // Propagate only the normal edge. The checked operation
                    // itself terminates execution on overflow, not this frame.
                    state[into] = State::Initialized(empty.clone());
                    match action {
                        Action::CheckedI32Multiply { literal, .. } => {
                            Operation::CheckedI32Multiply {
                                left: local(left),
                                literal,
                                into: local(into),
                            }
                        }
                        Action::CheckedI32Add { right, .. } => Operation::CheckedI32Add {
                            left: local(left),
                            right: local(right),
                            into: local(into),
                        },
                        _ => unreachable!(),
                    }
                }
                Action::EqualI32 { left, right, into } => {
                    require_initialized(&slots, &state, left, node, budget)?;
                    require_initialized(&slots, &state, right, node, budget)?;
                    if !matches!(state[into], State::Vacant) {
                        return Err(Stop::Pending(node));
                    }
                    // The equality body supplies fresh loan-free scalar scratch slots.
                    state[left] = State::Vacant;
                    state[right] = State::Vacant;
                    state[into] = State::Initialized(empty.clone());
                    Operation::EqualI32 {
                        left: local(left),
                        right: local(right),
                        into: local(into),
                    }
                }
                Action::FormatI32Template { first, count, into } => {
                    budget.step(count)?;
                    budget.reserve(count)?;
                    let mut holes = Vec::with_capacity(count);
                    for scratch in first..first + count {
                        require_initialized(&slots, &state, scratch, node, budget)?;
                        holes.push(local(scratch));
                    }
                    if !matches!(state[into], State::Vacant) {
                        return Err(Stop::Pending(node));
                    }
                    // The interpolation supplies fresh loan-free i32 scratch slots.
                    for scratch in &mut state[first..first + count] {
                        *scratch = State::Vacant;
                    }
                    state[into] = State::Initialized(empty.clone());
                    Operation::FormatI32Template {
                        holes,
                        into: local(into),
                    }
                }
                Action::CopyThrough { descriptor, into } => {
                    let target = exact_target(&state, descriptor, node, budget)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        descriptor,
                        (node, Conflict::Access),
                        Access::Observe,
                        budget,
                    )?;
                    if let Some(into) = into {
                        let contents = target.contents_slot();
                        // Reference extraction still includes aggregate backing.
                        let loans = if let Some(loans) =
                            aggregate_carried_loans(&state, &slots, contents, budget)?
                        {
                            loans
                        } else {
                            match &state[contents] {
                                State::Initialized(value) => value.loans.clone(),
                                // No stored value in the analysis means there
                                // has not been a local write to caller storage.
                                State::Vacant => empty.loans.clone(),
                                State::Moved(_) => return Err(Stop::Pending(node)),
                            }
                        };
                        state[into] = State::Initialized(Held {
                            loans,
                            ..Held::default()
                        });
                    }
                    Operation::CopyThrough {
                        descriptor: local(descriptor),
                        into: into.map(local),
                    }
                }
                Action::InstallThrough {
                    descriptor,
                    incoming,
                } => {
                    let target = exact_target(&state, descriptor, node, budget)?;
                    let mut value = initialized(&state[incoming], node)?.clone();
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        descriptor,
                        (node, Conflict::Access),
                        Access::Exclusive,
                        budget,
                    )?;
                    // Overwrite exact contents, not the access descriptor.
                    value.origin = if matches!(target, ExactTarget::Caller { .. }) {
                        ValueOrigin::RetainedStore(node)
                    } else {
                        ValueOrigin::Untracked
                    };
                    state[target.contents_slot()] = State::Initialized(value);
                    state[incoming] = State::Vacant;
                    Operation::InstallThrough {
                        descriptor: local(descriptor),
                        incoming: local(incoming),
                    }
                }
                Action::InstallThroughTwoHop { route, incoming } => {
                    initialized(&state[route.descriptor], node)?;
                    initialized(&state[route.contents], node)?;
                    let mut value = initialized(&state[incoming], node)?.clone();
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        route.descriptor,
                        (node, Conflict::Access),
                        Access::Exclusive,
                        budget,
                    )?;
                    value.origin = ValueOrigin::RetainedStore(node);
                    state[route.contents] = State::Initialized(value);
                    state[incoming] = State::Vacant;
                    Operation::InstallThroughTwoHop {
                        parameter: local(route.descriptor),
                        incoming: local(incoming),
                    }
                }
                Action::CheckRetained => {
                    check_retained(
                        &retained,
                        &state,
                        (node, Conflict::Escape),
                        retains_exclusive_bool,
                        budget,
                    )?;
                    Operation::None
                }
                Action::CheckCallableReturn(slot)
                | Action::Export(slot)
                | Action::StageScopedValue(slot) => {
                    require_initialized(&slots, &state, slot, node, budget)?;
                    let owner = initialized(&state[slot], node)?.callable_owner;
                    forbid_active_escape(&state[slot], node)?;
                    let mut escape: Option<Loan> = None;
                    visit_loans(&state, slots[slot].storage(slot), budget, |loan, _| {
                        if loan.kind.borrows_local_storage()
                            && escape.is_none_or(|old| loan.kind < old.kind)
                        {
                            escape = Some(loan);
                        }
                        Ok(())
                    })?;
                    if let Some(loan) = escape {
                        return Err(Stop::Rejected {
                            site: node,
                            related: if callable_frame.is_some()
                                || matches!(owner, Some(CallableOwner::Returned(_)))
                                || slots[loan.source].projection.is_some()
                            {
                                loan.site
                            } else {
                                slots[loan.source].node
                            },
                            kind: Conflict::Escape,
                        });
                    }
                    if let Some((layout, output)) = callable_frame {
                        if slot != output || owner != Some(CallableOwner::Formal(layout)) {
                            return Err(Stop::Pending(node));
                        }
                        if matches!(action, Action::CheckCallableReturn(_)) {
                            // The return stage stays initialized across lexical
                            // and parameter cleanup and is exported only after.
                            return Ok(Operation::None);
                        }
                        callable_exported = true;
                    } else if matches!(
                        owner,
                        Some(CallableOwner::LocalExclusive(_) | CallableOwner::Returned(_))
                    ) {
                        // Nothing proves a further erased output boundary.
                        return Err(Stop::Pending(node));
                    }
                    let operation = if matches!(action, Action::StageScopedValue(_)) {
                        budget.step(1)?;
                        let held = initialized(&state[slot], node)?;
                        if slots[slot].members != 0
                            || slots[slot].custom_disposer.is_some()
                            || !held.loans.is_empty()
                            || owner.is_some()
                            || !matches!(
                                slots[slot].cleanup,
                                StorageCleanup::End | StorageCleanup::FreshBoolResult(_)
                            )
                        {
                            return Err(Stop::Pending(node));
                        }
                        Operation::StageScopedValue {
                            from: local(slot),
                            retirement: cleanup_of(
                                cleanup_context,
                                slots[slot],
                                held,
                                node,
                                budget,
                            )?,
                        }
                    } else {
                        Operation::Export(local(slot))
                    };
                    let region = slots[slot].storage(slot);
                    budget.step(region.len())?;
                    state[region].fill(State::Vacant);
                    operation
                }
                Action::Cleanup(index) | Action::EndScope(index) => {
                    Operation::Cleanup(cleanup_boundary(
                        cleanup_context,
                        &slots,
                        &mut state,
                        &cleanups[index],
                        &live[pc],
                        (
                            node,
                            Boundary::End {
                                lexical: matches!(action, Action::EndScope(_)),
                                remainder,
                            },
                        ),
                        budget,
                    )?)
                }
                Action::End(slot) => {
                    if matches!(state[slot], State::Initialized(_)) {
                        conflict(
                            &slots,
                            &state,
                            &live[pc],
                            slot,
                            (node, Conflict::Release),
                            Access::EndScope,
                            budget,
                        )?;
                        let ended = std::mem::replace(&mut state[slot], State::Vacant);
                        retire_loans(&slots, &mut state, &[slot], budget)?;
                        cleanup_operation(cleanup_context, slots[slot], ended, node, budget)?
                    } else {
                        state[slot] = State::Vacant;
                        Operation::None
                    }
                }
            })
        })();
        let operation = match operation {
            Ok(operation) => operation,
            Err(Stop::Pending(node)) => {
                // This path cannot continue without the missing judgment. Keep
                // checking independent reached paths for definite rejection.
                blocked.get_or_insert(node);
                blocks.push(Block {
                    site,
                    operation: Operation::None,
                    terminator: Terminator::Return,
                    fresh_result_storage: None,
                });
                continue;
            }
            Err(stop) => return Err(stop),
        };
        #[cfg(all(test, target_arch = "aarch64", target_os = "macos"))]
        if let Some(probe) = cache
            .as_deref_mut()
            .and_then(|cache| cache.join_probe.as_mut())
        {
            probe.observe(
                &CancellationFrame {
                    module,
                    disposer: Some(disposer),
                    slots: &slots,
                    locals: body_scope.map_or(&[], |scope| &cleanups[scope]),
                    params: &params,
                    retained: &retained,
                    indexed: facts.is_indexed(),
                },
                (&input.state, &state),
                (action, edges),
                &operation,
                (pc, site),
            );
        }
        if active.is_some()
            && let Operation::Call(id) = operation
        {
            single_active_call &= active_leaf.replace(id).is_none();
        }
        if replacement_candidate
            && matches!(action, Action::Displace { .. })
            && matches!(&operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Cleanup::InvokeDisposer { .. }]))
        {
            replacement_candidate = displacement.replace(blocks.len()).is_none();
        }
        if matches!(edges, Edges::Branch { .. }) {
            budget.reserve(state.len())?;
            // Record binder fields are initialized beside their root below.
            if let Action::AdvanceOwnedList {
                into: Some(item), ..
            } = action
            {
                budget.step(slots[item].members)?;
            }
        }
        let mut successor = |pc, state| known.add(pc, state, budget);
        let terminator = match edges {
            // Publication returns a lowering PC and a checked caller state.
            // Intern the pair; the PC alone is not a MIR block index.
            Edges::Next(next) => Terminator::Goto(successor(join_resume.unwrap_or(next), state)?),
            Edges::Branch { yes, no } => {
                let mut on_yes = state.clone();
                // Both outcomes retain the unique cursor until lexical cleanup.
                // Only a successful advance initializes the iteration binder.
                if let Action::AdvanceOwnedList {
                    into: Some(item), ..
                } = action
                {
                    on_yes[slots[item].storage(item)].fill(State::Initialized(empty.clone()));
                }
                Terminator::Branch {
                    yes: successor(yes, on_yes)?,
                    no: successor(no, state)?,
                }
            }
            Edges::End => Terminator::Return,
            Edges::FinishStartup => Terminator::FinishStartup,
            Edges::FinishExpectation => Terminator::FinishExpectation,
        };
        blocks.push(Block {
            site,
            operation,
            terminator,
            fresh_result_storage,
        });
    }
    if let Some(node) = blocked {
        return Err(Stop::Pending(node));
    }
    if callable_frame.is_some() && !callable_exported {
        return Err(Stop::Pending(body));
    }
    // Add the List- or guard-backed delivery judgment only for frames with a
    // reached loop check. Entry-only owners keep their ordinary-only contract.
    // A fresh Result export needs ordinary construction evidence. Do not add
    // an entry-only delivery proof just because this frame can now retire it.
    let (trace, replacement) = if candidate
        && (!loop_owner || (flat && loop_delivery))
        && (!fresh_result || loop_delivery || calls_reached != 0)
    {
        (
            Some(CancellationTrace {
                module,
                disposer,
                body,
                slots,
                locals: body_scope.map_or_else(Vec::new, |index| cleanups.swap_remove(index)),
                params,
                retained,
                facts,
                bindings,
                code,
                calls,
                known,
                live,
                flat,
                loop_delivery,
                forwarding_backedge,
                loop_owner,
                branching,
                calls_reached,
                call_block,
            }),
            None,
        )
    } else if replacement_candidate
        && let (Some(displacement), Some(scope)) = (displacement, body_scope)
        && let Some(site) = replacement_site(&blocks, &code, &known, displacement)
    {
        (
            None,
            Some(ReplacementCandidate {
                site,
                module,
                slots,
                locals: cleanups.swap_remove(scope),
                params,
                known,
                indexed: facts.is_indexed(),
            }),
        )
    } else {
        (None, None)
    };
    Ok(Lowered {
        mir: Mir {
            entry,
            blocks,
            disposer_bodies: BTreeMap::new(),
            cancellation: None,
            replacement: None,
            active_leaf: active_leaf.filter(|_| single_active_call),
        },
        trace,
        replacement,
        empty_entry: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_cache_failed_reservation_does_not_insert_pc() {
        let mut known = KnownStates::default();
        let mut budget = Budget {
            limits: AnalysisLimits {
                max_entries: 3,
                ..AnalysisLimits::default()
            },
            statistics: Statistics::default(),
        };
        assert!(matches!(
            known.add(7, vec![State::Vacant], &mut budget),
            Err(Stop::Limit)
        ));
        assert!(
            !known.heads.contains_key(&7),
            "an unreserved PC entered the cache"
        );
        assert!(known.blocks.is_empty());
        assert_eq!(budget.statistics.entries, 0);
    }

    #[test]
    fn state_cache_reserves_work_for_each_wide_comparison() {
        let mut known = KnownStates::default();
        let mut budget = Budget {
            limits: AnalysisLimits::default(),
            statistics: Statistics::default(),
        };
        let state = |last| {
            vec![
                State::Initialized(Held {
                    loans: Arc::from([0, 1, 2].map(|source| Loan {
                        source,
                        site: 0,
                        exclusive: false,
                        kind: LoanKind::Backing,
                    })),
                    ..Held::default()
                }),
                State::Moved(last),
            ]
        };
        for last in [0, 1] {
            assert_eq!(known.add(7, state(last), &mut budget).ok(), Some(last));
        }
        // Probe the first state, behind one other state with equal loan contents.
        // The two slots and three loans cost six units per exact comparison.
        for max_work in [11, 12] {
            let mut budget = Budget {
                limits: AnalysisLimits {
                    max_work,
                    max_entries: 0,
                    ..AnalysisLimits::default()
                },
                statistics: Statistics::default(),
            };
            let result = known.add(7, state(0), &mut budget);
            if max_work == 11 {
                assert!(matches!(result, Err(Stop::Limit)));
            } else {
                assert_eq!(result.ok(), Some(0));
                assert_eq!(budget.statistics.work, 12);
            }
            assert_eq!(known.blocks.len(), 2);
            assert_eq!(budget.statistics.entries, 0);
        }
    }

    #[test]
    fn shared_slot_adds_incoming_storage_to_the_carried_restrictions() {
        let mut budget = Budget {
            limits: AnalysisLimits::default(),
            statistics: Statistics::default(),
        };
        let carried = Loan {
            source: 0,
            site: 1,
            exclusive: true,
            kind: LoanKind::IncomingExclusive,
        };
        let access = Loan {
            source: 2,
            site: 3,
            exclusive: false,
            kind: LoanKind::IncomingSharedSlot,
        };
        let held = Held {
            loans: Arc::from([carried]),
            ..Held::default()
        };
        let mut bind = |kind| {
            borrowed_member_value(kind, &held, access.source, access.site, &mut budget)
                .ok()
                .expect("default budget")
        };
        let copied = bind(BorrowedMemberBinding::CopyReference);
        let slot = bind(BorrowedMemberBinding::SharedSlot);
        assert_eq!(&*copied.loans, &[carried]);
        assert_eq!(&*slot.loans, &[carried, access]);
        assert!(access.kind.is_incoming() && !access.kind.borrows_local_storage());
    }
}
