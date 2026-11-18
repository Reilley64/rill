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
    code: &[Instruction],
    calls: &[Call<'_>],
    cleanups: &[Vec<Slot>],
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
        let (used, killed) = match instruction.action {
            Action::None | Action::Cleanup(_) => (None, None),
            Action::Init(slot) | Action::End(slot) => (None, Some(slot)),
            Action::Read { from, into } | Action::Dereference { from, into } => (Some(from), into),
            Action::ForbiddenBorrowedMove(slot) => (Some(slot), None),
            Action::Borrow { from, into, .. } => (Some(from), Some(into)),
            Action::Reborrow { parent, into, .. } => (Some(parent), Some(into)),
            Action::Dispose(slot) | Action::Export(slot) => (Some(slot), Some(slot)),
            Action::Displace { target, incoming } | Action::Install { target, incoming } => {
                (Some(incoming), Some(target))
            }
            Action::FormClosure { capture, into, .. } => (Some(capture), Some(into)),
            Action::Call(index) => (None, calls[index].into),
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
        for slot in used.into_iter().chain(args.iter().copied()) {
            budget.step(1)?;
            if !live[pc].contains(&slot) {
                budget.reserve(1)?;
                live[pc].insert(slot);
                changed = true;
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
    for &holder in live {
        budget.step(1)?;
        if let State::Initialized(held) = &state[holder] {
            for loan in held.loans.iter() {
                budget.step(1)?;
                let blocks = match access {
                    Access::Observe => loan.exclusive,
                    Access::Exclusive => true,
                    Access::EndScope => loan.kind != LoanKind::Suspension,
                };
                if blocks && overlap(slots, loan.source, source) {
                    return Err(Stop::Rejected {
                        site,
                        related: loan.site,
                        kind,
                    });
                }
            }
        }
    }
    Ok(())
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

fn forbid_custom_partial(slots: &[Local<'_>], slot: Slot, node: NodeId) -> ResultOf<()> {
    let owner = slots[slot].owner;
    if owner != slot && slots[owner].has_custom_disposer {
        return Err(Stop::Rejected {
            site: node,
            related: slots[owner].node,
            kind: Conflict::ForbiddenMove,
        });
    }
    Ok(())
}

fn cleanup_of<'g>(module: &'g Module, local: Local<'g>, held: &Held) -> Cleanup<'g> {
    let place = local.place(module);
    if let Some(field) = held.capture_drop {
        Cleanup::DropCapture {
            closure: place.root,
            field: site_at(module, field),
        }
    } else if local.drops {
        Cleanup::Drop(place)
    } else {
        Cleanup::EndStorage(place)
    }
}

fn cleanup_operation<'g>(module: &'g Module, local: Local<'g>, state: State) -> Operation<'g> {
    let State::Initialized(held) = state else {
        return Operation::None;
    };
    match cleanup_of(module, local, &held) {
        Cleanup::Drop(place) => Operation::Drop(place),
        Cleanup::DropCapture { closure, field } => Operation::DropCapture { closure, field },
        Cleanup::EndStorage(_) => Operation::None,
    }
}

fn cleanup_boundary<'g>(
    module: &'g Module,
    slots: &[Local<'g>],
    state: &mut [State],
    scope: &[Slot],
    live: &BTreeSet<Slot>,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<Operation<'g>> {
    budget.reserve(3 * scope.len())?;
    let mut indegrees = vec![0usize; scope.len()];
    let mut remaining = 0usize;
    let source_rank = |loan: &Loan, state: &[State]| {
        if loan.kind == LoanKind::Suspension {
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
        let State::Initialized(held) = &state[slot] else {
            continue;
        };
        if slots[slot].has_custom_disposer {
            return Err(Stop::Pending(node));
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
        for loan in held.loans.iter() {
            budget.step(1)?;
            if let Some(source) = source_rank(loan, state) {
                indegrees[source] += 1;
            }
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
        let State::Initialized(held) = std::mem::replace(&mut state[slot], State::Vacant) else {
            return Err(Stop::Pending(node));
        };
        for loan in held.loans.iter() {
            budget.step(1)?;
            if let Some(source) = source_rank(loan, state) {
                indegrees[source] -= 1;
                if indegrees[source] == 0 {
                    ready.insert(source);
                }
            }
        }
        scheduled += 1;
        for field in slots[slot].storage(slot).skip(1).rev() {
            budget.step(1)?;
            if let State::Initialized(held) = std::mem::replace(&mut state[field], State::Vacant) {
                budget.reserve(1)?;
                actions.push(cleanup_of(module, slots[field], &held));
            }
        }
        actions.push(cleanup_of(module, slots[slot], &held));
    }
    if scheduled != remaining {
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
        let current = |loan: &Loan| !retired.iter().any(|&old| overlap(slots, old, loan.source));
        if held.loans.iter().any(|loan| !current(loan)) {
            budget.reserve(held.loans.len())?;
            held.loans = Arc::from_iter(held.loans.iter().copied().filter(current));
        }
    }
    Ok(())
}

pub(super) fn run<'g>(build: Build<'_, 'g>, entry: Pc, params: &[Slot]) -> ResultOf<Mir<'g>> {
    let Build {
        module,
        slots,
        code,
        calls,
        cleanups,
        dispose,
        budget,
        ..
    } = build;
    let live = liveness(&slots, &code, &calls, &cleanups, budget)?;
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
                        forbid_custom_partial(&slots, from, node)?;
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
                        && slots[from].fields != slots[into].fields
                    {
                        return Err(Stop::Pending(node));
                    }
                    let region = slots[from].storage(from);
                    budget.step(region.len())?;
                    for member in region {
                        let held = if moves {
                            std::mem::replace(&mut state[member], State::Moved(node))
                        } else {
                            state[member].clone()
                        };
                        if let Some(into) = into {
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
                } => {
                    require_initialized(&slots, &state, from, node, budget)?;
                    let held = initialized(&state[from], node)?;
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
                    budget.reserve(2 * (held.loans.len() + 1))?;
                    let mut loans = Vec::with_capacity(held.loans.len() + 1);
                    loans.push(Loan {
                        source: from,
                        site: node,
                        exclusive,
                        kind: LoanKind::Backing,
                    });
                    // A borrowed descriptor exposes its stored reference. Preserve
                    // that backing without calling it the address of the descriptor.
                    loans.extend(held.loans.iter().map(|loan| Loan {
                        kind: if loan.kind == LoanKind::Suspension {
                            LoanKind::Suspension
                        } else {
                            LoanKind::Contents
                        },
                        ..*loan
                    }));
                    loans.sort_unstable();
                    loans.dedup();
                    state[into] = State::Initialized(Held {
                        loans: Arc::from(loans),
                        capture_drop: None,
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
                    let loans = if suspends {
                        budget.reserve(held.loans.len() + 1)?;
                        let mut loans = held.loans.to_vec();
                        loans.push(Loan {
                            source: parent,
                            site: node,
                            exclusive: true,
                            kind: LoanKind::Suspension,
                        });
                        loans.sort_unstable();
                        loans.dedup();
                        Arc::from(loans)
                    } else {
                        held.loans.clone()
                    };
                    state[into] = State::Initialized(Held {
                        loans,
                        capture_drop: None,
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
                    for &arg in &call.args {
                        budget.step(1)?;
                        let loans = &initialized(&state[arg], node)?.loans;
                        if call.retains && call.into.is_some() {
                            for loan in loans.iter().copied() {
                                budget.step(1)?;
                                if !retained.contains(&loan) {
                                    budget.reserve(1)?;
                                    retained.insert(loan);
                                }
                            }
                        }
                        state[arg] = State::Vacant;
                    }
                    if let Some(into) = call.into {
                        budget.reserve(retained.len())?;
                        state[into] = State::Initialized(Held {
                            loans: Arc::from_iter(retained),
                            capture_drop: None,
                        });
                    }
                    Operation::Call(call.callee)
                }
                Action::FormClosure {
                    capture,
                    into,
                    mode,
                } => {
                    let mut held = initialized(&state[capture], node)?.clone();
                    held.capture_drop = slots[capture].drops.then_some(slots[capture].node);
                    state[capture] = State::Vacant;
                    state[into] = State::Initialized(held);
                    Operation::FormClosure {
                        closure: local(into).root,
                        capture: local(capture).root,
                        mode,
                    }
                }
                Action::Dispose(slot) => {
                    initialized(&state[slot], node)?;
                    forbid_custom_partial(&slots, slot, node)?;
                    conflict(
                        &slots,
                        &state,
                        &live[pc],
                        slot,
                        (node, Conflict::Release),
                        Access::Exclusive,
                        budget,
                    )?;
                    state[slot] = State::Moved(node);
                    retire_loans(&slots, &mut state, &[slot], budget)?;
                    Operation::Dispose {
                        callee: dispose.ok_or(Stop::Pending(node))?,
                        operand: local(slot),
                    }
                }
                Action::Displace { target, .. }
                    if slots[target].fields > 0 || slots[target].has_custom_disposer =>
                {
                    cleanup_boundary(
                        module,
                        &slots,
                        &mut state,
                        &[target],
                        &live[pc],
                        node,
                        budget,
                    )?
                }
                Action::Displace { target, .. } => {
                    let owner = slots[target].owner;
                    if owner != target
                        && (slots[owner].has_custom_disposer
                            || !matches!(state[owner], State::Initialized(_)))
                    {
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
                    cleanup_operation(module, slots[target], displaced)
                }
                Action::Install { target, incoming } => {
                    require_initialized(&slots, &state, incoming, node, budget)?;
                    if slots[target].fields != slots[incoming].fields {
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
                Action::Export(slot) => {
                    require_initialized(&slots, &state, slot, node, budget)?;
                    let loans = &initialized(&state[slot], node)?.loans;
                    budget.step(loans.len())?;
                    if let Some(loan) = loans.iter().find(|loan| loan.kind == LoanKind::Backing) {
                        return Err(Stop::Rejected {
                            site: node,
                            related: if slots[loan.source].field.is_some() {
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
                    node,
                    budget,
                )?,
                Action::End(slot) => {
                    if matches!(state[slot], State::Initialized(_)) {
                        if slots[slot].has_custom_disposer {
                            return Err(Stop::Pending(node));
                        }
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
                        cleanup_operation(module, slots[slot], ended)
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
    Ok(Mir { entry, blocks })
}
