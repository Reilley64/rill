//! Ownership transitions for the unregistered two-List-arrow scoped operation.
//!
//! These are static edges, not scheduling events. In particular, ordinary
//! `CallListArrows` supplies no wait or delivery proof.
use super::*;

#[cfg(test)]
pub(in crate::analysis::ownership) mod tests;

#[derive(Clone, Copy, Debug)]
struct ScalarExport<'g> {
    from: Place<'g>,
    into: Place<'g>,
}

struct Child<'g> {
    environment: Place<'g>,
    backing: Place<'g>,
    scalar: ScalarExport<'g>,
    end_view: Cleanup<'g>,
    entry: Site<'g>,
}

fn same_root(a: Site<'_>, b: Site<'_>) -> bool {
    std::ptr::eq(a.module, b.module) && a.node == b.node
}

fn at_root(place: Place<'_>, root: Site<'_>) -> bool {
    place.projection.is_none() && same_root(place.root, root)
}

impl<'g> Child<'g> {
    fn check(frame: &ListArrowFrame<'g>, node: NodeId, budget: &mut Budget) -> ResultOf<Self> {
        budget.step(4)?;
        let pending = Stop::Pending(node);
        let [_, _, _, _] = frame.body.blocks.as_slice() else {
            return Err(pending);
        };
        let successor = |block: &Block<'g>| match block.terminator {
            Terminator::Goto(next) => frame.body.blocks.get(next).ok_or(Stop::Pending(node)),
            _ => Err(Stop::Pending(node)),
        };
        let entry = frame
            .body
            .blocks
            .get(frame.body.entry)
            .ok_or(pending.clone())?;
        let Operation::ReadListI32 {
            view,
            backing,
            into: Some(output),
            ..
        } = entry.operation
        else {
            return Err(pending);
        };
        let export = successor(entry)?;
        let end = successor(export)?;
        let returned = successor(end)?;
        let (
            Operation::Export(from),
            Operation::Cleanup(cleanup),
            Operation::None,
            Terminator::Return,
        ) = (
            &export.operation,
            &end.operation,
            &returned.operation,
            returned.terminator,
        )
        else {
            return Err(pending);
        };
        let [end_view @ Cleanup::EndStorage(ended)] = cleanup.as_slice() else {
            return Err(pending);
        };
        if !at_root(view, frame.capture.root)
            || !at_root(backing, frame.incoming_backing.root)
            || output.projection.is_some()
            || !at_root(*from, output.root)
            || !at_root(*ended, frame.capture.root)
        {
            return Err(pending);
        }
        // This entry has exactly one owned initialized input: the transferred
        // view. The incoming backing is a non-owning proxy. Before the read,
        // the scalar is vacant; after export it is gone. Thus the checked
        // EndStorage is also the entire cleanup on delivery at child entry.
        Ok(Self {
            environment: frame.environment,
            backing: frame.backing,
            scalar: ScalarExport {
                from: *from,
                into: frame.result,
            },
            end_view: *end_view,
            entry: entry.site,
        })
    }
}

/// Both parent-owned inputs have already formed in source order. Neither can
/// start until this state is consumed by the single two-environment commit.
pub(super) struct Staged<'g> {
    children: [Child<'g>; 2],
    parent_exit: CancellationExit<'g>,
    parent_state: Vec<State>,
    receivers: [Slot; 2],
    into: Slot,
    site: Site<'g>,
}

impl<'g> Staged<'g> {
    fn check(
        parent: &CancellationFrame<'_, 'g>,
        input: &[State],
        receivers: [Slot; 2],
        into: Slot,
        frames: &[ListArrowFrame<'g>; 2],
        site: Site<'g>,
        budget: &mut Budget,
    ) -> ResultOf<Self> {
        let node = site.node;
        let pending = Stop::Pending(node);
        budget.step(1)?;
        if budget.limits.max_depth < 2 {
            return Err(Stop::Limit);
        }
        budget.statistics.max_depth = budget.statistics.max_depth.max(2);
        if receivers[0] == receivers[1]
            || same_root(frames[0].environment.root, frames[1].environment.root)
            || !parent.retained.is_empty()
            || parent.slots[into].members != 2
            || parent.slots[into].custom_disposer.is_some()
            || !matches!(
                &input[parent.slots[into].storage(into)],
                [State::Vacant, State::Vacant, State::Vacant]
            )
        {
            return Err(pending);
        }
        budget.reserve(3)?;
        for i in 0..2 {
            budget.step(1)?;
            let frame = &frames[i];
            let environment = parent.slots[receivers[i]].place(parent.module);
            let result = parent.slots[into + 1 + i].place(parent.module);
            let held = initialized(&input[receivers[i]], node)?;
            let [
                Loan {
                    source,
                    site: acquisition,
                    exclusive: false,
                    kind: LoanKind::Backing,
                },
            ] = held.loans.as_ref()
            else {
                return Err(pending);
            };
            if !at_root(frame.environment, environment.root)
                || !same_root(frame.origin, environment.root)
                || held.callable_owner != Some(CallableOwner::ListArrow(environment.root.node))
                || !same_root(frame.capture.root, site_at(parent.module, *acquisition))
                || !at_root(
                    frame.backing,
                    parent.slots[*source].place(parent.module).root,
                )
                || !matches!(
                    parent.slots[*source].cleanup,
                    StorageCleanup::DisposeListI32(_)
                )
                || (parent.locals.binary_search(source).is_err()
                    && parent.params.binary_search(source).is_err())
                || parent.slots[into + 1 + i].members != 0
                || !matches!(parent.slots[into + 1 + i].cleanup, StorageCleanup::End)
                || !same_root(frame.result.root, result.root)
                || result.tuple_element() != Some(i)
                || frame.result.tuple_element() != Some(i)
            {
                return Err(pending);
            }
            require_initialized(parent.slots, input, *source, node, budget)?;
        }
        let [left, right] = frames;
        let children = [
            Child::check(left, node, budget)?,
            Child::check(right, node, budget)?,
        ];

        // Check cleanup on the terminal projection of the actual caller
        // state. Transferred environments and unpublished result slots are
        // not parent cleanup operands. This projection is inaccessible until
        // both children really finish and their staged results are retired.
        budget.reserve(input.len())?;
        let mut terminal = input.to_vec();
        for receiver in receivers {
            terminal[receiver] = State::Vacant;
        }
        if !parent.check(&terminal, node, budget)? {
            return Err(pending);
        }
        let parent_exit = parent.exit(Cow::Owned(terminal), node, budget)?;
        budget.reserve(input.len())?;
        Ok(Self {
            children,
            parent_exit,
            parent_state: input.to_vec(),
            receivers,
            into,
            site,
        })
    }

    fn commit(mut self, budget: &mut Budget) -> ResultOf<Box<Waiting<'g>>> {
        budget.reserve(3)?;
        // No fallible step or successor between the two transfers. The
        // checked child bindings retain the loans from both input owners.
        let children = self.receivers.map(|receiver| {
            let State::Initialized(environment) = std::mem::replace(
                &mut self.parent_state[receiver],
                State::Moved(self.site.node),
            ) else {
                unreachable!("checked initialized environments")
            };
            ChildState::Entry(environment)
        });
        Ok(Box::new(Waiting {
            proof: self,
            children,
            results: [None; 2],
            pending: false,
            stopping: false,
            stop_requested: [false; 2],
        }))
    }
}

enum ChildState<'g> {
    Entry(Held),
    Running(Held),
    Cleaning {
        environment: Held,
        end_view: Cleanup<'g>,
        entry_delivery: Option<Site<'g>>,
    },
    Complete,
}

#[derive(Clone, Copy)]
enum ChildEvent {
    Enter,
    EntryDelivery,
    Export,
    CleanupCompleted,
}

#[derive(Clone, Copy)]
enum Event {
    Child(usize, ChildEvent),
    RequestStop,
    EnabledYield,
    MaskedYield,
    HandleDrop,
}

struct Waiting<'g> {
    proof: Staged<'g>,
    children: [ChildState<'g>; 2],
    results: [Option<ScalarExport<'g>>; 2],
    pending: bool,
    stopping: bool,
    stop_requested: [bool; 2],
}

enum Successor<'g> {
    Wait(Box<Waiting<'g>>),
    Publish {
        results: [ScalarExport<'g>; 2],
        pending: bool,
        parent_state: Vec<State>,
    },
    Cancel {
        retired: [Option<ScalarExport<'g>>; 2],
        parent_exit: CancellationExit<'g>,
    },
}

impl<'g> Waiting<'g> {
    /// Keep both backing dependencies through the wait, including the one
    /// for a child whose own captured view has already ended.
    fn backings(&self) -> [Place<'g>; 2] {
        self.proof.children.each_ref().map(|child| child.backing)
    }

    fn advance(mut self: Box<Self>, event: Event, budget: &mut Budget) -> ResultOf<Successor<'g>> {
        budget.step(1)?;
        let pending = Stop::Pending(self.proof.site.node);
        match event {
            Event::RequestStop => self.pending = true,
            Event::EnabledYield if self.pending && !self.stopping => {
                self.pending = false;
                self.stopping = true;
                self.stop_requested = self
                    .children
                    .each_ref()
                    .map(|child| !matches!(child, ChildState::Complete));
            }
            Event::EnabledYield | Event::MaskedYield | Event::HandleDrop => {}
            Event::Child(i, event) => {
                let child = self.children.get_mut(i).ok_or(pending.clone())?;
                let proof = &self.proof.children[i];
                *child = match (std::mem::replace(child, ChildState::Complete), event) {
                    (ChildState::Entry(environment), ChildEvent::Enter) => {
                        ChildState::Running(environment)
                    }
                    (ChildState::Entry(environment), ChildEvent::EntryDelivery)
                        if self.stop_requested[i] =>
                    {
                        ChildState::Cleaning {
                            environment,
                            end_view: proof.end_view,
                            entry_delivery: Some(proof.entry),
                        }
                    }
                    (ChildState::Running(environment), ChildEvent::Export) => {
                        self.results[i] = Some(proof.scalar);
                        ChildState::Cleaning {
                            environment,
                            end_view: proof.end_view,
                            entry_delivery: None,
                        }
                    }
                    (ChildState::Cleaning { .. }, ChildEvent::CleanupCompleted) => {
                        ChildState::Complete
                    }
                    _ => return Err(pending),
                };
            }
        }
        if !matches!(self.children, [ChildState::Complete, ChildState::Complete]) {
            return Ok(Successor::Wait(self));
        }
        // This is the only point at which the source dependencies can end.
        // Scalar retirement consumes only the initialized stages, without a
        // disposer. The checked parent exit follows, never the normal suffix.
        budget.reserve(2)?;
        if self.stopping {
            return Ok(Successor::Cancel {
                retired: self.results,
                parent_exit: self.proof.parent_exit,
            });
        }
        let [Some(first), Some(second)] = self.results else {
            return Err(pending);
        };
        let into = self.proof.into;
        self.proof.parent_state[into..into + 3].fill(State::Initialized(Held::default()));
        Ok(Successor::Publish {
            results: [first, second],
            pending: self.pending,
            parent_state: self.proof.parent_state,
        })
    }
}
