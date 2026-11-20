//! Ownership transitions for the unregistered two-List-arrow scoped operation.
//!
//! These are static edges, not scheduling events. In particular, ordinary
//! `CallListArrows` supplies no wait or delivery proof.
use super::*;

#[cfg(all(test, target_arch = "aarch64", target_os = "macos"))]
pub(in crate::analysis::ownership) mod tests;

/// A result in join-owned storage, distinct from the caller's vacant tuple.
#[derive(Clone, Copy, Debug)]
struct ScalarStage<'g> {
    slot: usize,
    from: Place<'g>,
}

#[derive(Clone, Copy, Debug)]
struct ScalarExport<'g> {
    stage: ScalarStage<'g>,
    into: Place<'g>,
}

struct Child<'g> {
    environment: Place<'g>,
    backing: Place<'g>,
    scalar: ScalarExport<'g>,
    end_view: Cleanup<'g>,
    entry: Site<'g>,
    returned: Site<'g>,
}

fn membership_work(frame: &CancellationFrame<'_, '_>) -> usize {
    // Upper bound for the two binary searches on sorted slot ids.
    frame.locals.len().max(1).ilog2() as usize + frame.params.len().max(1).ilog2() as usize + 4
}

fn same_root(a: Site<'_>, b: Site<'_>) -> bool {
    std::ptr::eq(a.module, b.module) && a.node == b.node
}

fn at_root(place: Place<'_>, root: Site<'_>) -> bool {
    place.projection.is_none() && same_root(place.root, root)
}

impl<'g> Child<'g> {
    fn check(
        frame: &ListArrowFrame<'g>,
        slot: usize,
        node: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Self> {
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
                stage: ScalarStage { slot, from: *from },
                into: frame.result,
            },
            end_view: *end_view,
            entry: frame.body_site,
            returned: returned.site,
        })
    }
}

fn check_receiver<'g>(
    parent: &CancellationFrame<'_, 'g>,
    input: &[State],
    receiver: Slot,
    frame: &ListArrowFrame<'g>,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<()> {
    budget.step(1)?;
    let environment = parent.slots[receiver].place(parent.module);
    let held = initialized(&input[receiver], node)?;
    let [
        Loan {
            source,
            site: acquisition,
            exclusive: false,
            kind: LoanKind::Backing,
        },
    ] = held.loans.as_ref()
    else {
        return Err(Stop::Pending(node));
    };
    budget.step(membership_work(parent))?;
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
    {
        return Err(Stop::Pending(node));
    }
    require_initialized(parent.slots, input, *source, node, budget)
}

fn parent_exit<'g>(
    parent: &CancellationFrame<'_, 'g>,
    terminal: Vec<State>,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<CancellationExit<'g>> {
    budget.step(
        terminal
            .len()
            .checked_mul(membership_work(parent))
            .ok_or(Stop::Limit)?,
    )?;
    if !parent.check(&terminal, node, budget)? {
        return Err(Stop::Pending(node));
    }
    parent.exit(Cow::Owned(terminal), node, budget)
}

/// Cleanup if control exits before commit. This projection does not add a
/// poll between capture formations: delivery requires a separately proved
/// enabled checkpoint, and a direct shared borrow is not one.
fn before_commit_exit<'g>(
    parent: &CancellationFrame<'_, 'g>,
    input: &[State],
    receivers: [Slot; 2],
    frames: &[ListArrowFrame<'g>; 2],
    site: Site<'g>,
    budget: &mut Budget,
) -> ResultOf<(Vec<Cleanup<'g>>, CancellationExit<'g>)> {
    if !parent.retained.is_empty() || receivers[0] >= receivers[1] {
        return Err(Stop::Pending(site.node));
    }
    let mut n = 0;
    for (i, r) in receivers.into_iter().enumerate() {
        budget.step(1)?;
        match input[r] {
            State::Initialized(_) if i == n => {
                check_receiver(parent, input, r, &frames[i], site.node, budget)?;
                n += 1;
            }
            State::Vacant => {}
            _ => return Err(Stop::Pending(site.node)),
        }
    }
    budget.reserve(input.len())?;
    let mut terminal = input.to_vec();
    budget.reserve(parent.locals.len() + parent.params.len())?;
    let live = parent.locals.iter().chain(parent.params).copied().collect();
    let prefix = cleanup_boundary(
        parent.module,
        parent.slots,
        &mut terminal,
        &receivers[..n],
        &live,
        (
            site.node,
            Boundary::End {
                lexical: true,
                remainder: None,
            },
        ),
        budget,
    )?;
    Ok((prefix, parent_exit(parent, terminal, site.node, budget)?))
}

/// Both parent-owned inputs have already formed in source order. Neither can
/// start until this state is consumed by the single two-environment commit.
pub(super) struct Staged<'g> {
    children: [Child<'g>; 2],
    parent_exit: CancellationExit<'g>,
    precommit_cleanup: Vec<Cleanup<'g>>,
    parent_state: Vec<State>,
    receivers: [Slot; 2],
    into: Slot,
    site: Site<'g>,
    resume: Pc,
}

impl<'g> Staged<'g> {
    fn check(
        parent: &CancellationFrame<'_, 'g>,
        input: &[State],
        receivers: [Slot; 2],
        into: Slot,
        frames: &[ListArrowFrame<'g>; 2],
        (site, resume): (Site<'g>, Pc),
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
            initialized(&input[receivers[i]], node)?;
            let result = parent.slots[into + 1 + i].place(parent.module);
            if parent.slots[into + 1 + i].members != 0
                || !matches!(parent.slots[into + 1 + i].cleanup, StorageCleanup::End)
                || !same_root(frame.result.root, result.root)
                || result.tuple_element() != Some(i)
                || frame.result.tuple_element() != Some(i)
            {
                return Err(pending);
            }
        }
        let [left, right] = frames;
        let children = [
            Child::check(left, 0, node, budget)?,
            Child::check(right, 1, node, budget)?,
        ];

        // At an entry delivery, end the staged shared environments before
        // parent cleanup. After commit the children, not the parent, end
        // those views; the same parent exit is then gated on both terminals.
        let (precommit_cleanup, parent_exit) =
            before_commit_exit(parent, input, receivers, frames, site, budget)?;
        budget.reserve(input.len())?;
        Ok(Self {
            children,
            parent_exit,
            precommit_cleanup,
            parent_state: input.to_vec(),
            receivers,
            into,
            site,
            resume,
        })
    }

    /// Evaluate the selected operation's real entry checkpoint after its
    /// arguments have formed, before either environment can start a child.
    fn at_entry(
        self,
        mut context: ParentDelivery,
        yielded: bool,
        budget: &mut Budget,
    ) -> ResultOf<Admission<'g>> {
        budget.step(1)?;
        if yielded && context.at_yield() {
            return Ok(Admission::DeliveredBeforeCommit {
                environments: self.precommit_cleanup,
                parent_exit: self.parent_exit,
            });
        }
        Ok(Admission::Committed(self.commit(context, budget)?))
    }

    fn commit(
        mut self,
        context: ParentDelivery,
        budget: &mut Budget,
    ) -> ResultOf<Box<Waiting<'g>>> {
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
            context,
            stopping: false,
            stop_requested: [false; 2],
        }))
    }
}

enum Admission<'g> {
    DeliveredBeforeCommit {
        environments: Vec<Cleanup<'g>>,
        parent_exit: CancellationExit<'g>,
    },
    Committed(Box<Waiting<'g>>),
}

enum ChildState<'g> {
    Entry(Held),
    Running(Held),
    Cleaning {
        environment: Held,
        end_view: Cleanup<'g>,
        entry_delivery: Option<Site<'g>>,
    },
    AtExit(ChildExit<'g>),
    Complete,
}

#[derive(Clone, Copy)]
enum ChildExit<'g> {
    Normal(Site<'g>),
    Delivered(Site<'g>),
}

#[derive(Clone, Copy)]
enum ChildEvent {
    Entry { yielded: bool },
    Export,
    EndView,
    Finish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParentDelivery {
    enabled: bool,
    pending: bool,
}

impl ParentDelivery {
    fn at_yield(&mut self) -> bool {
        self.enabled && std::mem::take(&mut self.pending)
    }
}

#[derive(Clone, Copy)]
enum Event {
    Child(usize, ChildEvent),
    RequestStop,
    WaitYield,
    HandleDrop,
}

struct Waiting<'g> {
    proof: Staged<'g>,
    children: [ChildState<'g>; 2],
    results: [Option<ScalarStage<'g>>; 2],
    context: ParentDelivery,
    stopping: bool,
    stop_requested: [bool; 2],
}

enum Successor<'g> {
    Wait(Box<Waiting<'g>>),
    Publish {
        results: [ScalarExport<'g>; 2],
        context: ParentDelivery,
        resume: Pc,
        parent_state: Vec<State>,
    },
    Cancel {
        retired: [Option<ScalarStage<'g>>; 2],
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
            Event::RequestStop => self.context.pending = true,
            Event::WaitYield if !self.stopping && self.context.at_yield() => {
                self.stopping = true;
                self.stop_requested = self
                    .children
                    .each_ref()
                    .map(|child| !matches!(child, ChildState::Complete));
            }
            Event::WaitYield | Event::HandleDrop => {}
            Event::Child(i, event) => {
                let child = self.children.get_mut(i).ok_or(pending.clone())?;
                let proof = &self.proof.children[i];
                *child = match (std::mem::replace(child, ChildState::Complete), event) {
                    (ChildState::Entry(environment), ChildEvent::Entry { yielded: true })
                        if self.stop_requested[i] =>
                    {
                        ChildState::Cleaning {
                            environment,
                            end_view: proof.end_view,
                            entry_delivery: Some(proof.entry),
                        }
                    }
                    (ChildState::Entry(environment), ChildEvent::Entry { .. }) => {
                        ChildState::Running(environment)
                    }
                    (ChildState::Running(environment), ChildEvent::Export) => {
                        budget.reserve(1)?;
                        self.results[i] = Some(proof.scalar.stage);
                        ChildState::Cleaning {
                            environment,
                            end_view: proof.end_view,
                            entry_delivery: None,
                        }
                    }
                    (
                        ChildState::Cleaning {
                            environment,
                            end_view: Cleanup::EndStorage(view),
                            entry_delivery,
                        },
                        ChildEvent::EndView,
                    ) => {
                        if !matches!(environment.loans.as_ref(), [loan] if loan.site==view.root.node)
                        {
                            return Err(pending);
                        }
                        ChildState::AtExit(
                            entry_delivery
                                .map_or(ChildExit::Normal(proof.returned), ChildExit::Delivered),
                        )
                    }
                    (ChildState::AtExit(ChildExit::Normal(at)), ChildEvent::Finish)
                        if same_root(at, proof.returned) =>
                    {
                        ChildState::Complete
                    }
                    (ChildState::AtExit(ChildExit::Delivered(at)), ChildEvent::Finish)
                        if same_root(at, proof.entry) =>
                    {
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
        budget.step(3)?;
        self.proof.parent_state[into..into + 3].fill(State::Initialized(Held::default()));
        Ok(Successor::Publish {
            results: [first, second].map(|stage| ScalarExport {
                stage,
                into: self.proof.children[stage.slot].scalar.into,
            }),
            context: self.context,
            resume: self.proof.resume,
            parent_state: self.proof.parent_state,
        })
    }
}
