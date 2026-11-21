//! Ownership transitions for the two-child scoped compiler operation.
//!
//! These are static edges, not scheduling events. In particular, ordinary
//! `CallListArrows` supplies no wait or delivery proof.
use super::*;

pub(super) mod installed;

#[cfg(all(test, target_arch = "aarch64", target_os = "macos"))]
pub(in crate::analysis::ownership) mod tests;

/// A result in join-owned storage, distinct from the caller's vacant tuple.
#[derive(Clone, Copy, Debug)]
struct ResultSlot<'g> {
    slot: usize,
    from: Place<'g>,
    origin: ValueOrigin,
    retirement: Cleanup<'g>,
}

#[derive(Clone, Copy, Debug)]
struct ResultExport<'g> {
    stage: ResultSlot<'g>,
    into: Place<'g>,
}

#[derive(Clone, Copy)]
struct Child<'g> {
    backing: Option<Place<'g>>,
    export: ResultExport<'g>,
    value: bool,
    reference: bool,
    exclusive: bool,
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
        frame: &ScopedJoinFrame<'g>,
        slot: usize,
        node: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Self> {
        budget.step(4)?;
        let pending = Stop::Pending(node);
        let compared = match frame.body.blocks.len() {
            4 => false,
            5 if frame.incoming_list_backing.is_some() && !frame.exclusive => {
                budget.step(1)?;
                true
            }
            _ => return Err(pending),
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
        let (view, output) = match (&entry.operation, frame.incoming_list_backing) {
            (
                Operation::ReadListI32 {
                    view,
                    backing,
                    exclusive,
                    into: Some(output),
                    ..
                },
                Some(incoming),
            ) if *exclusive == frame.exclusive && at_root(*backing, incoming.root) => {
                (*view, *output)
            }
            (
                Operation::Copy {
                    from,
                    into: Some(output),
                },
                None,
            ) if !frame.exclusive && !same_root(from.root, output.root) => (*from, *output),
            _ => return Err(pending),
        };
        let (output, export) = if compared {
            let comparison = successor(entry)?;
            let Operation::EqualI32Literal { from, into, .. } = comparison.operation else {
                return Err(pending);
            };
            if !at_root(from, output.root)
                || output.projection.is_some()
                || same_root(from.root, into.root)
                || same_root(view.root, into.root)
            {
                return Err(pending);
            }
            (into, successor(comparison)?)
        } else {
            (output, successor(entry)?)
        };
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
        let capture = frame.capture.ok_or(pending.clone())?;
        if !at_root(view, capture.root)
            || output.projection.is_some()
            || !at_root(*from, output.root)
            || !at_root(*ended, capture.root)
        {
            return Err(pending);
        }
        // This entry has exactly one owned initialized input: the transferred
        // capture. A List backing proxy is non-owning. The result is vacant
        // before the expression and leaves the frame at export. A comparison
        // consumes its scratch read before export. Thus this EndStorage is
        // also the entire cleanup on delivery at child entry.
        Ok(Self {
            backing: frame.backing,
            value: false,
            reference: frame.incoming_list_backing.is_none(),
            exclusive: frame.exclusive,
            export: ResultExport {
                stage: ResultSlot {
                    slot,
                    from: *from,
                    origin: ValueOrigin::Untracked,
                    retirement: Cleanup::EndStorage(*from),
                },
                into: frame.result,
            },
            end_view: *end_view,
            entry: frame.body_site,
            returned: returned.site,
        })
    }

    fn check_value(
        frame: &ScopedJoinFrame<'g>,
        slot: usize,
        destination: StorageCleanup<'g>,
        node: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Self> {
        let pending = Stop::Pending(node);
        let blocks = &frame.body.blocks;
        let fresh = frame.result_origin;
        budget.step(4 + usize::from(fresh.is_some()))?;
        if blocks.len() != 4 + usize::from(fresh.is_some()) {
            return Err(pending);
        }
        let successor = |block: &Block<'g>| match block.terminator {
            Terminator::Goto(next) => blocks.get(next).ok_or(Stop::Pending(node)),
            _ => Err(Stop::Pending(node)),
        };
        let entry = blocks.get(frame.body.entry).ok_or(pending.clone())?;
        let initial = match (&entry.operation, frame.capture) {
            (Operation::Initialize(place), None) => *place,
            (
                Operation::Copy {
                    from,
                    into: Some(place),
                },
                Some(capture),
            ) if fresh.is_none()
                && at_root(*from, capture.root)
                && at_root(capture, frame.origin) =>
            {
                *place
            }
            _ => return Err(pending),
        };
        if same_root(initial.root, frame.origin) {
            return Err(pending);
        }
        let (construct, export) = if let Some(occurrence) = fresh {
            let construct = successor(entry)?;
            if !same_root(construct.site, occurrence) {
                return Err(pending);
            }
            (Some(construct), successor(construct)?)
        } else {
            (None, successor(entry)?)
        };
        let end = successor(export)?;
        let returned = successor(end)?;
        let (
            Operation::StageScopedValue { from, retirement },
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
        let [end_view @ Cleanup::EndStorage(environment)] = cleanup.as_slice() else {
            return Err(pending);
        };
        if !at_root(*environment, frame.origin) || from.projection.is_some() {
            return Err(pending);
        }
        let origin = match (construct.map(|b| &b.operation), retirement, destination) {
            (None, Cleanup::EndStorage(operand), StorageCleanup::End)
                if at_root(*from, initial.root) && at_root(*operand, from.root) =>
            {
                ValueOrigin::Untracked
            }
            (
                Some(Operation::ConstructVariant {
                    variant,
                    payload: Some(payload),
                    into,
                }),
                Cleanup::EndStorage(operand),
                StorageCleanup::FreshBoolResult(storage),
            ) if *variant != storage.err
                && at_root(*payload, initial.root)
                && at_root(*into, from.root)
                && at_root(*operand, from.root) =>
            {
                ValueOrigin::FreshResultOk
            }
            (
                Some(Operation::ConstructResultError {
                    variant,
                    wrapper,
                    payload,
                    into,
                    ..
                }),
                Cleanup::DropFreshResultError {
                    operand,
                    origin,
                    variant: drop_variant,
                    wrapper: drop_wrapper,
                    storage_source,
                },
                StorageCleanup::FreshBoolResult(storage),
            ) if same_root(*origin, fresh.ok_or(pending.clone())?)
                && *variant == storage.err
                && *drop_variant == storage.err
                && *wrapper == storage.wrapper
                && *drop_wrapper == storage.wrapper
                && *storage_source == storage.storage_source
                && at_root(*payload, initial.root)
                && at_root(*into, from.root)
                && at_root(*operand, from.root) =>
            {
                ValueOrigin::FreshResultErr(origin.node)
            }
            _ => return Err(pending),
        };
        Ok(Self {
            backing: None,
            export: ResultExport {
                stage: ResultSlot {
                    slot,
                    from: *from,
                    origin,
                    retirement: *retirement,
                },
                into: frame.result,
            },
            value: true,
            reference: false,
            exclusive: false,
            end_view: *end_view,
            entry: frame.body_site,
            returned: returned.site,
        })
    }
}

fn check_value_receiver(
    slots: &[Local<'_>],
    input: &[State],
    receiver: Slot,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<()> {
    budget.step(1)?;
    let expected = Held {
        callable_owner: Some(CallableOwner::ScopedValue(slots[receiver].node)),
        ..Held::default()
    };
    if initialized(&input[receiver], node)? != &expected {
        return Err(Stop::Pending(node));
    }
    Ok(())
}

fn check_receiver<'g>(
    parent: &CancellationFrame<'_, 'g>,
    input: &[State],
    receiver: Slot,
    frame: &ScopedJoinFrame<'g>,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<()> {
    budget.step(1)?;
    let environment = parent.slots[receiver].place(parent.module);
    let backing = frame.backing.ok_or(Stop::Pending(node))?;
    let capture = frame.capture.ok_or(Stop::Pending(node))?;
    let held = initialized(&input[receiver], node)?;
    let [
        Loan {
            source,
            site: acquisition,
            exclusive,
            kind,
        },
    ] = held.loans.as_ref()
    else {
        return Err(Stop::Pending(node));
    };
    budget.step(membership_work(parent))?;
    if *exclusive != frame.exclusive
        || !at_root(frame.environment, environment.root)
        || !same_root(frame.origin, environment.root)
        || held.callable_owner != Some(CallableOwner::ListArrow(environment.root.node))
        || !at_root(backing, parent.slots[*source].place(parent.module).root)
    {
        return Err(Stop::Pending(node));
    }
    match (frame.incoming_list_backing, kind) {
        (Some(_), LoanKind::Backing)
            if same_root(capture.root, site_at(parent.module, *acquisition))
                && matches!(
                    parent.slots[*source].cleanup,
                    StorageCleanup::DisposeListI32(_)
                )
                && (parent.locals.binary_search(source).is_ok()
                    || parent.params.binary_search(source).is_ok()) => {}
        (None, LoanKind::IncomingSharedI32)
            if !*exclusive
                && *acquisition == parent.slots[*source].node
                && parent.params.binary_search(source).is_ok() =>
        {
            // This key is not a loan of the parameter slot. The source was
            // checked by the copy; ending that slot cannot end its referent.
            return Ok(());
        }
        _ => return Err(Stop::Pending(node)),
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
    if !parent.check_initialized(&terminal, node, true, budget)? {
        return Err(Stop::Pending(node));
    }
    let exit = parent.exit(Cow::Owned(terminal), node, budget)?;
    // Ordinary return can export an owner whose disposer is needed only on
    // delivery. Do not certify that disposer from the ordinary call graph.
    // The scoped edge closes only end-storage and pinned scalar-List release.
    budget.step(exit.cleanup.len())?;
    if exit.cleanup.iter().any(|action| {
        !matches!(
            action,
            Cleanup::EndStorage(_) | Cleanup::DisposeListI32 { .. }
        )
    }) {
        return Err(Stop::Pending(node));
    }
    Ok(exit)
}

pub(super) struct BeforeCommit<'g> {
    environments: Vec<Cleanup<'g>>,
    parent_exit: CancellationExit<'g>,
}

pub(super) fn before_fresh_commit<'g>(
    parent: &CancellationFrame<'_, 'g>,
    input: &[State],
    receivers: [Slot; 2],
    live: &BTreeSet<Slot>,
    site: Site<'g>,
    budget: &mut Budget,
) -> ResultOf<BeforeCommit<'g>> {
    // Both direct captures have formed. Check their actual transfer before
    // testing the parent exit; a cleanup blocker must not hide a conflict.
    for receiver in receivers {
        check_value_receiver(parent.slots, input, receiver, site.node, budget)?;
        conflict(
            parent.slots,
            input,
            live,
            receiver,
            (site.node, Conflict::Access),
            Access::Exclusive,
            budget,
        )?;
    }
    before_commit_exit(parent, input, receivers, None, site, budget)
}

/// Cleanup if control exits before commit. This projection does not add a
/// poll between capture formations: delivery requires a separately proved
/// enabled checkpoint, and direct capture formation is not one.
/// Without frames, the already-checked receivers are owned fresh-join inputs.
fn before_commit_exit<'g>(
    parent: &CancellationFrame<'_, 'g>,
    input: &[State],
    receivers: [Slot; 2],
    frames: Option<&[ScopedJoinFrame<'g>; 2]>,
    site: Site<'g>,
    budget: &mut Budget,
) -> ResultOf<BeforeCommit<'g>> {
    if !parent.retained.is_empty() || receivers[0] >= receivers[1] {
        return Err(Stop::Pending(site.node));
    }
    let mut n = 0;
    for (i, r) in receivers.into_iter().enumerate() {
        budget.step(1)?;
        match input[r] {
            State::Initialized(_) if i == n => {
                if let Some(frames) = frames {
                    check_receiver(parent, input, r, &frames[i], site.node, budget)?;
                }
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
    Ok(BeforeCommit {
        environments: prefix,
        parent_exit: parent_exit(parent, terminal, site.node, budget)?,
    })
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
        (frames, before_commit): (&[ScopedJoinFrame<'g>; 2], Option<BeforeCommit<'g>>),
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
            || ((frames[0].exclusive || frames[1].exclusive)
                && matches!((frames[0].backing, frames[1].backing), (Some(a), Some(b)) if same_root(a.root, b.root)))
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
            if frame.value != before_commit.is_some() {
                return Err(pending);
            }
            if frame.value {
                budget.step(1)?;
                let environment = parent.slots[receivers[i]].place(parent.module);
                if !at_root(frame.environment, environment.root)
                    || !same_root(frame.origin, environment.root)
                    || frame.backing.is_some()
                    || frame.incoming_list_backing.is_some()
                    || frame.exclusive
                {
                    return Err(pending);
                }
            }
            let result = parent.slots[into + 1 + i].place(parent.module);
            let destination = parent.slots[into + 1 + i];
            if destination.members != 0
                || !match frame.result_origin {
                    None => destination.cleanup == StorageCleanup::End,
                    Some(_) => {
                        frame.value
                            && matches!(destination.cleanup, StorageCleanup::FreshBoolResult(_))
                    }
                }
                || !same_root(frame.result.root, result.root)
                || result.tuple_element() != Some(i)
                || frame.result.tuple_element() != Some(i)
            {
                return Err(pending);
            }
        }
        let mut child = |i: usize| {
            let frame = &frames[i];
            if frame.value {
                Child::check_value(frame, i, parent.slots[into + 1 + i].cleanup, node, budget)
            } else {
                Child::check(frame, i, node, budget)
            }
        };
        let children = [child(0)?, child(1)?];

        // At an entry delivery, end the staged environments before
        // parent cleanup. After commit the children, not the parent, end
        // those views; the same parent exit is then gated on both terminals.
        let BeforeCommit {
            environments,
            parent_exit,
        } = match before_commit {
            Some(checked) => checked,
            None => before_commit_exit(parent, input, receivers, Some(frames), site, budget)?,
        };
        budget.reserve(input.len())?;
        Ok(Self {
            children,
            parent_exit,
            precommit_cleanup: environments,
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
            results: [None, None],
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

#[derive(Clone)]
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
    #[cfg(all(test, target_arch = "aarch64", target_os = "macos"))]
    HandleDrop,
}

struct Waiting<'g> {
    proof: Staged<'g>,
    children: [ChildState<'g>; 2],
    results: [Option<StagedResult<'g>>; 2],
    context: ParentDelivery,
    stopping: bool,
    stop_requested: [bool; 2],
}

#[derive(Clone)]
struct StagedResult<'g> {
    place: ResultSlot<'g>,
    loans: Arc<[Loan]>,
}

enum Successor<'g> {
    Wait(Box<Waiting<'g>>),
    Publish {
        results: [ResultExport<'g>; 2],
        retained_results: [Option<Place<'g>>; 2],
        context: ParentDelivery,
        resume: Pc,
        parent_state: Vec<State>,
    },
    Cancel {
        retired: [Option<ResultSlot<'g>>; 2],
        parent_exit: CancellationExit<'g>,
    },
}

impl ChildState<'_> {
    fn environment(&self) -> Option<&Held> {
        match self {
            Self::Entry(held)
            | Self::Running(held)
            | Self::Cleaning {
                environment: held, ..
            } => Some(held),
            Self::AtExit(_) | Self::Complete => None,
        }
    }
}

impl<'g> Waiting<'g> {
    fn backings(&self) -> [Option<Place<'g>>; 2] {
        self.proof.children.each_ref().map(|child| child.backing)
    }

    fn environment_backings(&self) -> [Option<Place<'g>>; 2] {
        std::array::from_fn(|i| {
            self.children[i].environment()?;
            self.proof.children[i].backing
        })
    }

    fn exclusive_backings(&self) -> [Option<Place<'g>>; 2] {
        std::array::from_fn(|i| {
            let environment = self.children[i].environment()?;
            if matches!(environment.loans.as_ref(), [loan] if loan.exclusive) {
                self.proof.children[i].backing
            } else {
                None
            }
        })
    }

    fn retained_results(&self) -> [Option<Place<'g>>; 2] {
        self.results.each_ref().map(|stage| {
            let stage = stage.as_ref()?;
            // The checked copy admits only the immutable caller source
            // carried by this slot's actual environment.
            if stage.loans.is_empty() {
                None
            } else {
                self.proof.children[stage.place.slot].backing
            }
        })
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
            Event::WaitYield => {}
            #[cfg(all(test, target_arch = "aarch64", target_os = "macos"))]
            Event::HandleDrop => {}
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
                        self.results[i] = Some(StagedResult {
                            place: proof.export.stage,
                            loans: if proof.reference {
                                environment.loans.clone()
                            } else {
                                Arc::default()
                            },
                        });
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
                        if !(if proof.value {
                            environment.loans.is_empty()
                        } else {
                            matches!(environment.loans.as_ref(), [loan]
                            if loan.exclusive == proof.exclusive && if proof.reference {
                                loan.kind == LoanKind::IncomingSharedI32
                            } else {
                                loan.kind == LoanKind::Backing && loan.site == view.root.node
                            })
                        }) {
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
        // On stop, consume only initialized unpublished stages, including
        // their loans, before the checked parent exit. On ordinary completion
        // their loans move into the destination instead of ending here.
        budget.reserve(2)?;
        if self.stopping {
            return Ok(Successor::Cancel {
                retired: self.results.map(|stage| stage.map(|stage| stage.place)),
                parent_exit: self.proof.parent_exit,
            });
        }
        let [Some(first), Some(second)] = self.results else {
            return Err(pending);
        };
        let into = self.proof.into;
        budget.step(3)?;
        self.proof.parent_state[into] = State::Initialized(Held::default());
        let results = [first, second].map(|stage| {
            let i = stage.place.slot;
            self.proof.parent_state[into + 1 + i] = State::Initialized(Held {
                loans: stage.loans,
                origin: stage.place.origin,
                ..Held::default()
            });
            self.proof.children[i].export
        });
        let mut retained_results = [None; 2];
        for (i, child) in self.proof.children.iter().enumerate() {
            if child.reference {
                budget.step(1)?;
                let value =
                    initialized(&self.proof.parent_state[into + 1 + i], self.proof.site.node)?;
                let backing = child.backing.ok_or(pending.clone())?;
                if !matches!(value.loans.as_ref(), [loan] if loan.site == backing.root.node) {
                    return Err(pending);
                }
                retained_results[i] = Some(backing);
            }
        }
        Ok(Successor::Publish {
            results,
            retained_results,
            context: self.context,
            resume: self.proof.resume,
            parent_state: self.proof.parent_state,
        })
    }
}
