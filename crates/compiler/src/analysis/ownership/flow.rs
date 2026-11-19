//! Compute loan liveness, initialized states and path-specific cleanup.
use super::*;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

// Retain distinct continuing states at a join. Every feasible incoming state
// must authorize a read, and each MIR successor cleans only what it still owns.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    Vacant,
    Initialized(Held),
    Moved(NodeId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct Held {
    loans: Arc<[Loan]>,
    capture_drop: Option<NodeId>,
    /// The original parameter of an already-running outer disposer.
    disposal_started_at: Option<NodeId>,
    stored_at: Option<NodeId>,
    /// Identity of the stored descriptor, independent of the access loans.
    exact_target: Option<ExactTarget>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
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
}

impl LoanKind {
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
            Self::End => (None, None),
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
    retained: &BTreeMap<Slot, Slot>,
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
            Action::None | Action::Cleanup(_) | Action::CheckRetained => use_one(None, None),
            Action::Init(slot) | Action::End(slot) => use_one(None, Some(slot)),
            Action::Read { from, into } | Action::Dereference { from, into } => {
                use_one(Some(slots[from].carried_loan_root(from)), into)
            }
            Action::ForbiddenBorrowedMove(slot) => use_one(Some(slot), None),
            Action::Borrow { from, into, .. } => use_one(Some(from), Some(into)),
            Action::Reborrow { parent, into, .. } => {
                use_one(Some(slots[parent].carried_loan_root(parent)), Some(into))
            }
            Action::Dispose(slot) | Action::Export(slot) => use_one(Some(slot), Some(slot)),
            Action::Displace { target, incoming } | Action::Install { target, incoming } => {
                use_one(Some(incoming), Some(target))
            }
            Action::FormClosure { capture, into, .. } => use_one(Some(capture), Some(into)),
            Action::Call(index) => use_one(None, calls[index].into),
            // External contents are already live at every instruction.
            Action::CopyThrough {
                descriptor, into, ..
            } => use_one(Some(descriptor), into),
            Action::InstallThrough {
                descriptor,
                incoming,
            } => ([Some(descriptor), Some(incoming)], None),
            Action::TestVariant { operand, .. } => use_one(Some(operand), None),
            Action::CopyOptionPayload { operand, into } => use_one(Some(operand), Some(into)),
            Action::ConstructVariant { payload, into, .. } => use_one(payload, Some(into)),
            Action::TakeResultSuccess { operand, into } => use_one(Some(operand), into),
            Action::ForwardResultFailure { operand, into, .. } => {
                use_one(Some(operand), Some(into))
            }
        };
        let cleaned = match instruction.action {
            Action::Cleanup(index) => cleanups[index].as_slice(),
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
            .chain(retained.values().copied())
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
        if loan.kind == LoanKind::IncomingExclusive {
            return Ok(());
        }
        let blocks = match access {
            Access::Observe => loan.exclusive,
            Access::Exclusive => true,
            Access::EndScope => loan.kind.borrows_local_storage(),
        };
        if blocks && overlap(slots, loan.source, source) {
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

fn cleanup_of<'g>(
    module: &'g Module,
    local: Local<'g>,
    held: &Held,
    node: NodeId,
) -> ResultOf<Cleanup<'g>> {
    let place = local.place(module);
    match local.cleanup {
        StorageCleanup::Unlowered => Err(Stop::Pending(node)),
        StorageCleanup::Drop => Ok(Cleanup::Drop(place)),
        StorageCleanup::End if let Some(field) = held.capture_drop => Ok(Cleanup::DropCapture {
            closure: place.root,
            field: site_at(module, field),
        }),
        StorageCleanup::End => Ok(Cleanup::EndStorage(place)),
    }
}

fn cleanup_operation<'g>(
    module: &'g Module,
    local: Local<'g>,
    state: State,
    node: NodeId,
) -> ResultOf<Operation<'g>> {
    let State::Initialized(held) = state else {
        return Ok(Operation::None);
    };
    Ok(match cleanup_of(module, local, &held, node)? {
        Cleanup::Drop(place) => Operation::Drop(place),
        Cleanup::DropCapture { closure, field } => Operation::DropCapture { closure, field },
        Cleanup::EndStorage(_) => Operation::None,
        Cleanup::InvokeDisposer { .. } | Cleanup::DeferDisposerFields { .. } => {
            unreachable!("local storage cleanup cannot transfer disposer obligations")
        }
    })
}

fn cleanup_boundary<'g>(
    module: &'g Module,
    slots: &[Local<'g>],
    state: &mut [State],
    scope: &[Slot],
    live: &BTreeSet<Slot>,
    (node, remainder): (NodeId, Option<Slot>),
    budget: &mut Budget,
) -> ResultOf<Operation<'g>> {
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
    for &slot in scope {
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
        visit_loans(state, slots[slot].storage(slot), budget, |loan, _| {
            if let Some(source) = source_rank(&loan, state) {
                indegrees[source] += 1;
                grouped_dependencies |=
                    slots[slot].members != 0 || slots[scope[source]].members != 0;
            }
            Ok(())
        })?;
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
            if let Some(source) = source_rank(&loan, state) {
                indegrees[source] -= 1;
                if indegrees[source] == 0 {
                    ready.insert(source);
                }
            }
            Ok(())
        })?;
        scheduled += 1;
        if disposal_origin(&state[slot]).is_some()
            && let Some(remainder) = remainder.filter(|&remainder| remainder != slot)
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
                actions.push(cleanup_of(module, slots[member], &held, node)?);
            }
        }
    }
    if scheduled != remaining {
        // Grouping by record can make a cycle out of independently
        // orderable fields. Do not reject without the field cleanup judgment.
        if grouped_dependencies {
            return Err(Stop::Pending(node));
        }
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
    for &slot in scope {
        let region = slots[slot].storage(slot);
        budget.step(region.len())?;
        state[region].fill(State::Vacant);
    }
    retire_loans(slots, state, scope, budget)?;
    Ok(Operation::Cleanup(actions))
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
            loan.kind == LoanKind::IncomingExclusive
                || !retired.iter().any(|&old| overlap(slots, old, loan.source))
        };
        if held.loans.iter().any(|loan| !current(loan)) {
            budget.reserve(held.loans.len())?;
            held.loans = Arc::from_iter(held.loans.iter().copied().filter(current));
        }
    }
    Ok(())
}

pub(super) fn run<'g>(
    build: Build<'_, 'g>,
    entry: Pc,
    params: &[Slot],
    active: Option<DisposerFrame>,
) -> ResultOf<Mir<'g>> {
    let Build {
        module,
        slots,
        code,
        calls,
        cleanups,
        dispose,
        retained,
        retains_exclusive_bool,
        incoming_exclusive_bools,
        budget,
        ..
    } = build;
    let live = liveness(&slots, &code, &calls, &cleanups, &retained, budget)?;
    let empty = Held::default();
    let mut pending = Vec::new();
    let mut known = BTreeMap::<Pc, BTreeMap<Vec<State>, usize>>::new();
    let mut blocks = Vec::new();
    let add = |pc,
               state: Vec<State>,
               known: &mut BTreeMap<Pc, BTreeMap<Vec<State>, usize>>,
               pending: &mut Vec<(Pc, Vec<State>)>,
               budget: &mut Budget|
     -> ResultOf<usize> {
        budget.step(1 + state.len())?;
        for slot in &state {
            if let State::Initialized(held) = slot {
                budget.step(held.loans.len())?;
            }
        }
        let at_pc = known.entry(pc).or_default();
        if let Some(&block) = at_pc.get(&state) {
            return Ok(block);
        }
        budget.reserve(2 + 2 * state.len())?;
        let block = pending.len();
        at_pc.insert(state.clone(), block);
        pending.push((pc, state));
        Ok(block)
    };
    // A worklist index is also the eventual MIR block index.
    budget.reserve(slots.len())?;
    let mut state = vec![State::Vacant; slots.len()];
    for &slot in params {
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
        state[parameter] = State::Initialized(Held {
            exact_target: Some(ExactTarget::Caller {
                parameter,
                contents,
            }),
            ..Held::default()
        });
    }
    if let Some(frame) = active {
        state[frame.operand] = State::Initialized(Held {
            disposal_started_at: Some(slots[frame.operand].node),
            ..Held::default()
        });
    }
    let remainder = active.map(|frame| frame.remainder);
    let entry = add(entry, state, &mut known, &mut pending, budget)?;
    let mut blocked = None;
    while blocks.len() < pending.len() {
        let (pc, state) = &mut pending[blocks.len()];
        let mut state = std::mem::take(state);
        let pc = *pc;
        let Instruction {
            node,
            action,
            edges,
        } = code[pc];
        let site = site_at(module, node);
        let local = |slot: Slot| slots[slot].place(module);
        let operation = (|| -> ResultOf<_> {
            Ok(match action {
                Action::None => Operation::None,
                Action::Init(slot) => {
                    for field in slots[slot].storage(slot).skip(1) {
                        budget.step(1)?;
                        initialized(&state[field], node)?;
                    }
                    state[slot] = State::Initialized(empty.clone());
                    Operation::Initialize(local(slot))
                }
                Action::Read { from, into } => {
                    require_initialized(&slots, &state, from, node, budget)?;
                    let moves = slots[from].moves;
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
                Action::Dereference { from, into } => {
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
                    Operation::Dereference(local(from))
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
                            CallStorage::ThroughAlias {
                                descriptor,
                                argument,
                            } => match exact_target(&state, argument, node, budget)? {
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
                            },
                        };
                        initialized(&state[access], node)?;
                        initialized(&state[destination.input], node)?;
                        let mut loans = BTreeSet::new();
                        for slot in [contents, destination.input] {
                            include_loans(&mut loans, &state, slots[slot].storage(slot), budget)?;
                        }
                        // The access argument is temporary. It cannot become a
                        // backing source of the very descriptor it gives access to.
                        let held = Held {
                            loans: loan_set(loans, budget)?,
                            stored_at: matches!(
                                target,
                                RetainedPlace::ThroughParameter(_)
                                    | RetainedPlace::ThroughParameterAlias { .. }
                            )
                            .then_some(node),
                            ..Held::default()
                        };
                        Some((target, contents, destination.input, held))
                    } else {
                        None
                    };
                    for &arg in &call.args {
                        require_initialized(&slots, &state, arg, node, budget)?;
                        forbid_active_escape(&state[arg], node)?;
                        let storage = slots[arg].storage(arg);
                        if call.retains_result && call.into.is_some() {
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
                    if let Some((destination, contents, input, held)) = contents {
                        state[contents] = State::Initialized(held);
                        Operation::CallRetaining {
                            callee: call.callee,
                            destination,
                            input: local(input),
                        }
                    } else {
                        Operation::Call(call.callee)
                    }
                }
                Action::FormClosure {
                    capture,
                    into,
                    mode,
                } => {
                    let mut held = initialized(&state[capture], node)?.clone();
                    held.capture_drop = (slots[capture].cleanup == StorageCleanup::Drop)
                        .then_some(slots[capture].node);
                    state[capture] = State::Vacant;
                    state[into] = State::Initialized(held);
                    Operation::FormClosure {
                        closure: local(into).root,
                        capture: local(capture).root,
                        mode,
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
                    } else {
                        Operation::Dispose {
                            callee: dispose.ok_or(Stop::Pending(node))?,
                            operand: local(slot),
                        }
                    }
                }
                Action::Displace { target, .. } if slots[target].needs_boundary_cleanup() => {
                    if disposal_origin(&state[target]).is_some() {
                        return Err(Stop::Pending(node));
                    }
                    cleanup_boundary(
                        module,
                        &slots,
                        &mut state,
                        &[target],
                        &live[pc],
                        (node, remainder),
                        budget,
                    )?
                }
                Action::Displace { target, .. } => {
                    let owner = slots[target].owner;
                    if owner != target && !matches!(state[owner], State::Initialized(_)) {
                        return Err(Stop::Pending(node));
                    }
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
                    cleanup_operation(module, slots[target], displaced, node)?
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
                Action::CheckRetained => {
                    let mut suspended_input = None;
                    for &contents in retained.values() {
                        budget.step(1)?;
                        let State::Initialized(value) = &state[contents] else {
                            continue;
                        };
                        let site = value.stored_at.unwrap_or(node);
                        for loan in value.loans.iter() {
                            budget.step(1)?;
                            if !loan.kind.borrows_local_storage() {
                                if !retains_exclusive_bool {
                                    suspended_input.get_or_insert(site);
                                }
                            } else {
                                return Err(Stop::Rejected {
                                    site,
                                    related: loan.site,
                                    kind: Conflict::Escape,
                                });
                            }
                        }
                    }
                    // A suspension restricts the local access holder. Its
                    // incoming backing can survive that holder and needs a
                    // supported outward contract. Local backing still rejects.
                    if let Some(site) = suspended_input {
                        return Err(Stop::Pending(site));
                    }
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
                Action::Cleanup(index) => cleanup_boundary(
                    module,
                    &slots,
                    &mut state,
                    &cleanups[index],
                    &live[pc],
                    (node, remainder),
                    budget,
                )?,
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
                        cleanup_operation(module, slots[slot], ended, node)?
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
        let mut successor = |pc, state| add(pc, state, &mut known, &mut pending, budget);
        let terminator = match edges {
            Edges::Next(next) => Terminator::Goto(successor(next, state)?),
            Edges::Branch { yes, no } => Terminator::Branch {
                yes: successor(yes, state.clone())?,
                no: successor(no, state)?,
            },
            Edges::End => Terminator::Return,
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
    Ok(Mir {
        entry,
        blocks,
        disposer_bodies: BTreeMap::new(),
    })
}
