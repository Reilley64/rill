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
    /// A suspended parent is not backing storage for the reborrow.
    suspension: bool,
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
                if Some(slot) != killed
                    && cleaned.binary_search(&slot).is_err()
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

fn conflict(
    state: &[State],
    live: &BTreeSet<Slot>,
    source: Slot,
    site: NodeId,
    kind: Conflict,
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
                    Access::EndScope => !loan.suspension,
                };
                if loan.source == source && blocks {
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

fn cleanup_operation(module: &Module, local: Local, state: State) -> Operation<'_> {
    let State::Initialized(held) = state else {
        return Operation::None;
    };
    let place = site_at(module, local.node);
    if let Some(field) = held.capture_drop {
        Operation::DropCapture {
            closure: place,
            field: site_at(module, field),
        }
    } else if local.drops {
        Operation::Drop(place)
    } else {
        Operation::None
    }
}

fn cleanup_boundary<'g>(
    module: &'g Module,
    slots: &[Local],
    state: &mut [State],
    scope: &[Slot],
    live: &BTreeSet<Slot>,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<Operation<'g>> {
    budget.reserve(3 * scope.len())?;
    let mut indegrees = vec![0usize; scope.len()];
    let mut remaining = 0usize;
    // Count an incoming edge at each source for every dependent that must
    // retire before it. Only initialized storage participates.
    for &slot in scope {
        budget.step(1)?;
        let State::Initialized(held) = &state[slot] else {
            continue;
        };
        remaining += 1;
        conflict(
            state,
            live,
            slot,
            node,
            Conflict::Release,
            Access::EndScope,
            budget,
        )?;
        for loan in held.loans.iter() {
            budget.step(1)?;
            if !loan.suspension
                && let Ok(source) = scope.binary_search(&loan.source)
                && matches!(state[loan.source], State::Initialized(_))
            {
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
    while let Some(i) = ready.pop_last() {
        budget.step(1)?;
        let slot = scope[i];
        let State::Initialized(held) = std::mem::replace(&mut state[slot], State::Vacant) else {
            return Err(Stop::Pending(node));
        };
        for loan in held.loans.iter() {
            budget.step(1)?;
            if !loan.suspension
                && let Ok(source) = scope.binary_search(&loan.source)
                && matches!(state[loan.source], State::Initialized(_))
            {
                indegrees[source] -= 1;
                if indegrees[source] == 0 {
                    ready.insert(source);
                }
            }
        }
        let place = site_at(module, slots[slot].node);
        actions.push(if let Some(field) = held.capture_drop {
            Cleanup::DropCapture {
                closure: place,
                field: site_at(module, field),
            }
        } else if slots[slot].drops {
            Cleanup::Drop(place)
        } else {
            Cleanup::EndStorage(place)
        });
    }
    if actions.len() != remaining {
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
        state[slot] = State::Vacant;
    }
    Ok(Operation::ScopeExit(actions))
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
    let live = liveness(&code, &calls, &cleanups, budget)?;
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
        state[slot] = State::Initialized(empty.clone());
    }
    let entry = add(entry, state, &mut known, &mut pending, budget)?;
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
        let local = |slot: Slot| site_at(module, slots[slot].node);
        let operation = match action {
            Action::None => Operation::None,
            Action::Init(slot) => {
                state[slot] = State::Initialized(empty.clone());
                Operation::Initialize(local(slot))
            }
            Action::Read { from, into } => {
                let held = initialized(&state[from], node)?.clone();
                let moves = slots[from].moves;
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
                conflict(&state, &live[pc], from, node, kind, access, budget)?;
                if moves {
                    state[from] = State::Moved(node);
                }
                if let Some(into) = into {
                    state[into] = State::Initialized(held);
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
            Action::Borrow {
                from,
                into,
                exclusive,
            } => {
                initialized(&state[from], node)?;
                conflict(
                    &state,
                    &live[pc],
                    from,
                    node,
                    Conflict::Access,
                    if exclusive {
                        Access::Exclusive
                    } else {
                        Access::Observe
                    },
                    budget,
                )?;
                budget.reserve(1)?;
                state[into] = State::Initialized(Held {
                    loans: Arc::from([Loan {
                        source: from,
                        site: node,
                        exclusive,
                        suspension: false,
                    }]),
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
                    &state,
                    &live[pc],
                    parent,
                    node,
                    Conflict::Access,
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
                        suspension: true,
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
                    &state,
                    &live[pc],
                    from,
                    node,
                    Conflict::Access,
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
                    closure: local(into),
                    capture: local(capture),
                    mode,
                }
            }
            Action::Dispose(slot) => {
                initialized(&state[slot], node)?;
                conflict(
                    &state,
                    &live[pc],
                    slot,
                    node,
                    Conflict::Release,
                    Access::Exclusive,
                    budget,
                )?;
                state[slot] = State::Moved(node);
                Operation::Dispose {
                    callee: dispose.ok_or(Stop::Pending(node))?,
                    operand: local(slot),
                }
            }
            Action::Displace { target, .. } => {
                conflict(
                    &state,
                    &live[pc],
                    target,
                    node,
                    Conflict::Release,
                    Access::Exclusive,
                    budget,
                )?;
                let displaced = std::mem::replace(&mut state[target], State::Vacant);
                cleanup_operation(module, slots[target], displaced)
            }
            Action::Install { target, incoming } => {
                initialized(&state[incoming], node)?;
                state[target] = std::mem::replace(&mut state[incoming], State::Vacant);
                Operation::Install {
                    target: local(target),
                    incoming: local(incoming),
                }
            }
            Action::Export(slot) => {
                if let Some(loan) = initialized(&state[slot], node)?
                    .loans
                    .iter()
                    .find(|loan| !loan.suspension)
                {
                    return Err(Stop::Rejected {
                        site: node,
                        related: slots[loan.source].node,
                        kind: Conflict::Escape,
                    });
                }
                state[slot] = State::Vacant;
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
                    conflict(
                        &state,
                        &live[pc],
                        slot,
                        node,
                        Conflict::Release,
                        Access::EndScope,
                        budget,
                    )?;
                    let ended = std::mem::replace(&mut state[slot], State::Vacant);
                    cleanup_operation(module, slots[slot], ended)
                } else {
                    state[slot] = State::Vacant;
                    Operation::None
                }
            }
        };
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
    Ok(Mir { entry, blocks })
}
