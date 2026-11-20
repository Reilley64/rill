//! Compute loan liveness, initialized states and path-specific cleanup.
use super::*;
use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

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
    /// A checked pair body acquired at this local arrow, not just a callable type.
    pair_origin: Option<(NodeId, InvocationMode)>,
    /// The checked single-cell body of the environment currently in this slot.
    mutable_cell: Option<MutableCell>,
    /// The original parameter of an already-running outer disposer.
    disposal_started_at: Option<NodeId>,
    stored_at: Option<NodeId>,
    /// Identity of the stored descriptor, independent of the access loans.
    exact_target: Option<ExactTarget>,
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
    // Shared access to a caller-owned field slot, keyed conservatively by
    // the incoming parameter, not by the parameter's local storage.
    IncomingSharedSlot,
}

impl LoanKind {
    fn is_incoming(self) -> bool {
        matches!(self, Self::IncomingExclusive | Self::IncomingSharedSlot)
    }

    fn borrows_local_storage(self) -> bool {
        matches!(self, Self::Backing | Self::Contents)
    }
}

#[derive(Clone, Copy)]
enum Access {
    Observe,
    Exclusive,
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
// leaving its scope kills liveness of the old version. No supported cleanup
// operation can observe a borrowed value: droppable slots contain only strings.
fn liveness(
    slots: &[Local<'_>],
    code: &[Instruction<'_>],
    calls: &[Call<'_>],
    cleanups: &[Vec<Slot>],
    retained: &BTreeMap<Slot, RetainedContents>,
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
    let mut pending: VecDeque<_> = (0..code.len()).collect();
    while let Some(pc) = pending.pop_front() {
        budget.step(1)?;
        let instruction = code[pc];
        let use_one = |used, killed| ([used, None], killed);
        let (used, killed) = match instruction.action {
            Action::None
            | Action::BudgetBackedge
            | Action::Cleanup(_)
            | Action::EndScope(_)
            | Action::CheckRetained => use_one(None, None),
            Action::Init(slot) | Action::End(slot) => use_one(None, Some(slot)),
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
            Action::Reborrow { parent, into, .. } => {
                use_one(Some(slots[parent].carried_loan_root(parent)), Some(into))
            }
            Action::Dispose(slot) | Action::Export(slot) | Action::EndEmptyOption(slot) => {
                use_one(Some(slot), Some(slot))
            }
            Action::Displace { target, incoming } | Action::Install { target, incoming } => {
                use_one(Some(incoming), Some(target))
            }
            Action::FormClosure {
                capture,
                second,
                into,
                ..
            } => ([Some(capture), second], Some(into)),
            Action::Call(index) => use_one(None, calls[index].into),
            Action::InvokeClosurePair { receiver, into, .. }
            | Action::InvokeMutableI32Cell { receiver, into } => {
                use_one(Some(receiver), Some(into))
            }
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
            for &slot in there {
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
        // Caller storage can still expose its installed value until a write
        // commits, even if normal local liveness would kill that version.
        for slot in used
            .into_iter()
            .flatten()
            .chain(args.iter().copied())
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
    let check = |loan: Loan, _: &mut Budget| -> ResultOf<()> {
        // The parameter key for incoming backing is not a local place.
        if loan.kind.is_incoming() {
            return Ok(());
        }
        let blocks = match access {
            Access::Observe => loan.exclusive,
            Access::Exclusive => true,
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

fn cleanup_of<'g>(module: &'g Module, local: Local<'g>, node: NodeId) -> ResultOf<Cleanup<'g>> {
    let place = local.place(module);
    match local.cleanup {
        StorageCleanup::Unlowered => Err(Stop::Pending(node)),
        StorageCleanup::Drop => Ok(Cleanup::Drop(place)),
        StorageCleanup::DropOptionPayload(some) => Ok(Cleanup::DropOptionPayload {
            operand: place,
            some,
        }),
        StorageCleanup::ReleaseGuard(release) => Ok(release.cleanup(place)),
        StorageCleanup::End => Ok(Cleanup::EndStorage(place)),
    }
}

fn cleanup_operation<'g>(
    module: &'g Module,
    local: Local<'g>,
    state: State,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<Operation<'g>> {
    let State::Initialized(held) = state else {
        return Ok(Operation::None);
    };
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
    Ok(match cleanup_of(module, local, node)? {
        Cleanup::Drop(place) => Operation::Drop(place),
        Cleanup::DropCapture { closure, field } => Operation::DropCapture { closure, field },
        cleanup @ Cleanup::DropOptionPayload { .. } => {
            budget.reserve(1)?;
            Operation::Cleanup(vec![cleanup])
        }
        Cleanup::EndStorage(_) => Operation::None,
        Cleanup::InvokeDisposer { .. }
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

fn cleanup_boundary<'g>(
    module: &'g Module,
    slots: &[Local<'g>],
    state: &mut [State],
    scope: &[Slot],
    live: &BTreeSet<Slot>,
    (node, boundary): (NodeId, Boundary),
    budget: &mut Budget,
) -> ResultOf<Vec<Cleanup<'g>>> {
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
                let drops = held.capture_drops.into_iter().flatten();
                let count = drops.clone().count();
                if count == 0 {
                    actions.push(cleanup_of(module, slots[member], node)?);
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
        let site = value.stored_at.unwrap_or(node);
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
        Action::None
        | Action::BudgetBackedge
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
    body: NodeId,
    slots: Vec<Local<'g>>,
    locals: Vec<Slot>,
    params: Vec<Slot>,
    retained: BTreeMap<Slot, RetainedContents>,
    facts: NodeIndex<'a, 'g>,
    indexed_facts: Option<&'a NodeIndex<'a, 'g>>,
    bindings: BTreeMap<&'g DeclarationId, Slot>,
    code: Vec<Instruction<'g>>,
    calls: Vec<Call<'g>>,
    known: KnownStates,
    live: Vec<BTreeSet<Slot>>,
    flat: bool,
    loop_delivery: bool,
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
    destination: Slot,
    suspended: Vec<State>,
}

impl<'g> EnabledRetainingCall<'_, 'g> {
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
        self.indexed_facts
            .unwrap_or(&self.facts)
            .get(&node)
            .map(|fact| &fact.kind)
    }

    fn frame(&self) -> CancellationFrame<'_, 'g> {
        CancellationFrame {
            module: self.module,
            slots: &self.slots,
            locals: &self.locals,
            params: &self.params,
            retained: &self.retained,
            indexed: self.indexed_facts.is_some(),
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
            if self.indexed_facts.is_some() {
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
        if facts.pipe_input.is_some()
            || self
                .branch_parameter(self.slots[gate].node, budget)?
                .is_none()
        {
            return Ok(None);
        }
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
        if self.indexed_facts.is_some() {
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
        if self.indexed_facts.is_some() {
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
            if (callee.is_some() || matches!(action, Action::BudgetBackedge))
                && !deliveries.contains_key(&block)
            {
                // Even after losing the receipt, a feasible independent path
                // can expose a forbidden outward loan. Don't build cleanup
                // or accumulate successors for a proof we cannot publish.
                let admitted = frame.check(self.known.input(block), node, budget)?;
                complete &= admitted;
                if complete {
                    budget.reserve(1)?;
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
        if !complete {
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
        let contents = self
            .retained
            .values()
            .next()
            .map(|contents| contents.slot());
        for (slot, held) in state.iter().enumerate() {
            budget.step(1)?;
            if matches!(held, State::Initialized(_))
                && Some(slot) != contents
                && self.locals.binary_search(&slot).is_err()
                && self.params.binary_search(&slot).is_err()
            {
                // Nested scopes and in-flight staging are not flat boundaries.
                return Ok(false);
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
                State::Initialized(held) => held.stored_at.map(|store| site_at(self.module, store)),
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
        let mut cleanup = cleanup_boundary(
            self.module,
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
            self.module,
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

pub(super) fn run<'a, 'g>(
    build: Build<'a, 'g, '_>,
    entry: Pc,
    params: Vec<Slot>,
    active: Option<DisposerFrame>,
    collect_trace: bool,
) -> ResultOf<(Mir<'g>, Option<CancellationTrace<'a, 'g>>)> {
    let mut candidate = collect_trace
        && build.module.key.origin == crate::loader::Origin::Application
        && !build.retains_exclusive_bool
        && build.retained.len() <= 1
        && !matches!(
            build.retained.values().next(),
            Some(RetainedContents::TwoHop(_))
        );
    let mut flat = true;
    let mut loop_delivery = false;
    let mut branching = false;
    let mut calls_reached = 0;
    let mut call_block = None;
    let Build {
        module,
        body,
        facts,
        indexed_facts,
        locals: bindings,
        return_contract,
        slots,
        code,
        calls,
        mut cleanups,
        body_scope,
        dispose,
        retained,
        retains_exclusive_bool,
        incoming_exclusive_bools,
        budget,
        ..
    } = build;
    let live = liveness(&slots, &code, &calls, &cleanups, &retained, budget)?;
    let empty = Held::default();
    let mut known = KnownStates::default();
    let mut blocks = Vec::new();
    // A worklist index is also the eventual MIR block index.
    budget.reserve(slots.len())?;
    let mut state: Vec<_> = slots
        .iter()
        .map(|slot| {
            candidate = candidate
                && slot.projection.is_none()
                && !slot.needs_boundary_cleanup()
                && matches!(slot.cleanup, StorageCleanup::End | StorageCleanup::Drop);
            State::Vacant
        })
        .collect();
    for &slot in &params {
        let region = slots[slot].storage(slot);
        budget.step(region.len())?;
        state[region].fill(State::Initialized(empty.clone()));
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
        // Tag reached sites during the ordinary visit without a second walk.
        if candidate {
            flat &= flat_action(action, &slots);
            loop_delivery |= matches!(action, Action::BudgetBackedge);
            branching |= matches!(edges, Edges::Branch { .. });
            if let Action::Call(index) = action {
                calls_reached += 1;
                call_block = Some(blocks.len());
                if let Some((id, _)) = return_contract {
                    let callee = calls[index].callee;
                    if callee.start == id.start && callee.kind == id.kind {
                        if indexed_facts.is_some() {
                            budget.step(id.module.path.len().checked_add(1).ok_or(Stop::Limit)?)?;
                        }
                        flat &= callee != id;
                    }
                }
            }
        }
        let operation = (|| -> ResultOf<_> {
            Ok(match action {
                Action::None | Action::BudgetBackedge => Operation::None,
                Action::Init(slot) => {
                    for field in slots[slot].storage(slot).skip(1) {
                        budget.step(1)?;
                        initialized(&state[field], node)?;
                    }
                    state[slot] = State::Initialized(empty.clone());
                    Operation::Initialize(local(slot))
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
                    state[operand] = State::Vacant;
                    state[into] = State::Initialized(empty.clone());
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
                } => {
                    if let Some(payload) = payload {
                        initialized(&state[payload], node)?;
                        state[payload] = State::Vacant;
                    }
                    state[into] = State::Initialized(empty.clone());
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
                } => {
                    initialized(&state[payload], node)?;
                    state[into] = std::mem::replace(&mut state[payload], State::Vacant);
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
                    Operation::ForwardResultFailure {
                        operand: local(operand),
                        into: local(into),
                        variant,
                        wrapper,
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
                            stored_at: matches!(
                                target,
                                RetainedPlace::ThroughParameter(_)
                                    | RetainedPlace::ThroughTwoHopParameter { .. }
                                    | RetainedPlace::ThroughParameterAlias { .. }
                            )
                            .then_some(node),
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
                            CallResult::RetainsInputs | CallResult::RetainsRecordInput
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
                    } else if call.result == CallResult::ScalarCopy {
                        let [argument] = call.args.as_slice() else {
                            return Err(Stop::Pending(node));
                        };
                        Operation::CallScalarCopy {
                            implementation: call.callee,
                            argument: local(*argument),
                            into: call.into.map(local),
                        }
                    } else if call.result == CallResult::RetainsRecordInput {
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
                Action::FormClosure {
                    capture,
                    second,
                    into,
                    mode,
                    cell_addend,
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
                    state[into] = State::Initialized(Held {
                        loans,
                        capture_drops: [drop_of(capture), second.and_then(drop_of)],
                        pair_origin: second.map(|_| (node, mode)),
                        mutable_cell: cell_addend.map(|addend| MutableCell {
                            origin: node,
                            field: slots[capture].node,
                            addend,
                        }),
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
                Action::InvokeMutableI32Cell { receiver, into } => {
                    let cell = initialized(&state[receiver], node)?
                        .mutable_cell
                        .ok_or(Stop::Pending(node))?;
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
                    let held = initialized(&state[receiver], node)?;
                    let (origin, mode) = held.pair_origin.ok_or(Stop::Pending(node))?;
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
                    let storage = slots[into].storage(into);
                    budget.step(storage.len())?;
                    state[storage].fill(State::Initialized(empty.clone()));
                    // Reference members consult the conservative carried sources
                    // at the tuple root. They are not borrows of the receiver slot.
                    state[into] = State::Initialized(Held {
                        loans,
                        ..Held::default()
                    });
                    if matches!(call, PairCall::ReadOnly { .. }) {
                        Operation::InvokeReadOnlyClosurePair {
                            receiver: local(receiver),
                            origin: site_at(module, origin),
                            into: local(into),
                        }
                    } else {
                        state[receiver] = State::Moved(node);
                        retire_loans(&slots, &mut state, &[receiver], budget)?;
                        Operation::InvokeClosurePair {
                            receiver: local(receiver),
                            origin: site_at(module, origin),
                            into: local(into),
                        }
                    }
                }
                Action::Dispose(slot) => {
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
                            module,
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
                        cleanup_operation(module, slots[target], displaced, node, budget)?
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
                    value.stored_at = matches!(target, ExactTarget::Caller { .. }).then_some(node);
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
                    value.stored_at = Some(node);
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
                Action::Export(slot) => {
                    require_initialized(&slots, &state, slot, node, budget)?;
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
                            related: if slots[loan.source].projection.is_some() {
                                loan.site
                            } else {
                                slots[loan.source].node
                            },
                            kind: Conflict::Escape,
                        });
                    }
                    let region = slots[slot].storage(slot);
                    budget.step(region.len())?;
                    state[region].fill(State::Vacant);
                    Operation::Export(local(slot))
                }
                Action::Cleanup(index) | Action::EndScope(index) => {
                    Operation::Cleanup(cleanup_boundary(
                        module,
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
                        cleanup_operation(module, slots[slot], ended, node, budget)?
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
                });
                continue;
            }
            Err(stop) => return Err(stop),
        };
        if matches!(edges, Edges::Branch { .. }) {
            budget.reserve(state.len())?;
        }
        let mut successor = |pc, state| known.add(pc, state, budget);
        let terminator = match edges {
            Edges::Next(next) => Terminator::Goto(successor(next, state)?),
            Edges::Branch { yes, no } => Terminator::Branch {
                yes: successor(yes, state.clone())?,
                no: successor(no, state)?,
            },
            Edges::End => Terminator::Return,
            Edges::FinishStartup => Terminator::FinishStartup,
            Edges::FinishExpectation => Terminator::FinishExpectation,
        };
        blocks.push(Block {
            site,
            operation,
            terminator,
        });
    }
    if let Some(node) = blocked {
        return Err(Stop::Pending(node));
    }
    let trace = candidate.then(|| CancellationTrace {
        module,
        body,
        slots,
        locals: body_scope.map_or_else(Vec::new, |index| cleanups.swap_remove(index)),
        params,
        retained,
        facts,
        indexed_facts,
        bindings,
        code,
        calls,
        known,
        live,
        flat,
        loop_delivery,
        branching,
        calls_reached,
        call_block,
    });
    Ok((
        Mir {
            entry,
            blocks,
            disposer_bodies: BTreeMap::new(),
            cancellation: None,
        },
        trace,
    ))
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
