//! Project the checked operation into its observable MIR successors.
use super::*;
use crate::analysis::typed::RoutineRole;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    delivery: JoinDelivery,
    stopping: bool,
    children: [JoinChildState; 2],
    stop_requested: [bool; 2],
    stages: [bool; 2],
}

impl From<ParentDelivery> for JoinDelivery {
    fn from(context: ParentDelivery) -> Self {
        Self {
            enabled: context.enabled,
            pending: context.pending,
        }
    }
}

impl<'g> Waiting<'g> {
    fn key(&self) -> Key {
        Key {
            delivery: self.context.into(),
            stopping: self.stopping,
            children: self.children.each_ref().map(|child| match child {
                ChildState::Entry(_) => JoinChildState::Entry,
                ChildState::Running(_) => JoinChildState::Running,
                ChildState::Cleaning { .. } => JoinChildState::Cleaning,
                ChildState::AtExit(ChildExit::Normal(_)) => JoinChildState::AtNormalReturn,
                ChildState::AtExit(ChildExit::Delivered(_)) => JoinChildState::AtDeliveredExit,
                ChildState::Complete => JoinChildState::Complete,
            }),
            stop_requested: self.stop_requested,
            stages: self.results.each_ref().map(Option::is_some),
        }
    }

    fn fork(&self, budget: &mut Budget) -> ResultOf<Box<Self>> {
        budget.reserve(1)?;
        Ok(Box::new(Self {
            proof: self.proof.fork(budget)?,
            children: self.children.clone(),
            results: self.results.clone(),
            context: self.context,
            stopping: self.stopping,
            stop_requested: self.stop_requested,
        }))
    }
}

impl<'g> Staged<'g> {
    // The projected paths share only checked, immutable frame identities. A
    // work item owns its own caller state and cancellation exit.
    fn fork(&self, budget: &mut Budget) -> ResultOf<Self> {
        budget.reserve(self.parent_state.len())?;
        budget.reserve(self.precommit_cleanup.len())?;
        budget.reserve(self.parent_exit.cleanup.len())?;
        debug_assert!(self.parent_exit.retained.is_none());
        Ok(Self {
            children: self.children,
            parent_exit: CancellationExit {
                retained: None,
                cleanup: self.parent_exit.cleanup.clone(),
            },
            precommit_cleanup: self.precommit_cleanup.clone(),
            parent_state: self.parent_state.clone(),
            receivers: self.receivers,
            into: self.into,
            site: self.site,
            resume: self.resume,
        })
    }
}

#[derive(Default)]
struct Successors<'g> {
    ids: BTreeMap<Key, usize>,
    pending: VecDeque<Box<Waiting<'g>>>,
    normal: Option<(Pc, Vec<State>)>,
    cancel: Option<CancellationExit<'g>>,
}

impl<'g> Successors<'g> {
    fn intern(&mut self, waiting: Box<Waiting<'g>>, budget: &mut Budget) -> ResultOf<JoinNext<'g>> {
        // In one operation all sites, slots, source loans, and the underlying
        // post-commit caller state are fixed. The key retains everything that
        // varies, including delivery and whether a cleaning child staged a result.
        budget.step(1 + self.ids.len().max(1).ilog2() as usize)?;
        let next = self.ids.len();
        let id = match self.ids.entry(waiting.key()) {
            std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                budget.reserve(2)?;
                entry.insert(next);
                self.pending.push_back(waiting);
                next
            }
        };
        Ok(JoinNext::Waiting(id))
    }

    fn advance(
        &mut self,
        waiting: &Waiting<'g>,
        event: Event,
        budget: &mut Budget,
    ) -> ResultOf<JoinNext<'g>> {
        match waiting.fork(budget)?.advance(event, budget)? {
            Successor::Wait(next) => self.intern(next, budget),
            Successor::Publish {
                results,
                retained_results,
                context,
                resume,
                parent_state,
            } => {
                if let Some((normal_resume, normal_state)) = &self.normal {
                    budget.step(parent_state.len())?;
                    if resume != *normal_resume || parent_state != *normal_state {
                        return Err(Stop::Pending(waiting.proof.site.node));
                    }
                } else {
                    self.normal = Some((resume, parent_state));
                }
                Ok(JoinNext::Publish {
                    delivery: context.into(),
                    transfers: results.map(|result| (result.stage.from, result.into)),
                    retained_results,
                })
            }
            // The exit is the same checked projection for all post-commit
            // deliveries of this caller state. Entry delivery also uses it,
            // but has its own staged-environment cleanup first.
            Successor::Cancel {
                retired,
                parent_exit,
            } => {
                self.cancel.get_or_insert(parent_exit);
                Ok(JoinNext::Cancel {
                    retired: retired.map(|stage| stage.map(|value| value.from)),
                })
            }
        }
    }
}

pub(in crate::analysis::ownership::flow) fn check<'g>(
    parent: &CancellationFrame<'_, 'g>,
    input: &[State],
    (receivers, into): ([Slot; 2], Slot),
    frames: [ScopedJoinFrame<'g>; 2],
    site: (Site<'g>, Pc),
    role: RoutineRole<'g>,
    budget: &mut Budget,
) -> ResultOf<(Box<ScopedJoinProof<'g>>, Pc, Vec<State>)> {
    let staged = Staged::check(parent, input, receivers, into, &frames, site, budget)?;
    let mut successors = Successors::default();
    let mut entries = Vec::new();
    let mut delivered = None;
    // The selected operation checks its entry budget only after both direct
    // captures have formed. It inherits the caller mask and request; neither
    // starting a child nor returning enables parent delivery by itself.
    for enabled in [false, true] {
        for pending in [false, true] {
            let context = ParentDelivery { enabled, pending };
            for yielded in [false, true] {
                let next = match staged.fork(budget)?.at_entry(context, yielded, budget)? {
                    Admission::Committed(waiting) => successors.intern(waiting, budget)?,
                    Admission::DeliveredBeforeCommit {
                        environments,
                        parent_exit,
                    } => {
                        debug_assert!(parent_exit.retained.is_none());
                        delivered = Some(environments);
                        JoinNext::CancelBeforeCommit
                    }
                };
                budget.reserve(1)?;
                entries.push(JoinEntry {
                    delivery: context.into(),
                    yielded,
                    next,
                });
            }
        }
    }
    let before_commit = delivered.ok_or(Stop::Pending(site.0.node))?;
    let mut waiting_states = Vec::new();
    while let Some(waiting) = successors.pending.pop_front() {
        budget.step(1)?;
        let key = waiting.key();
        let mut edges = Vec::new();
        let mut edge = |event, observed| -> ResultOf<()> {
            let next = successors.advance(&waiting, event, budget)?;
            budget.reserve(1)?;
            edges.push((observed, next));
            Ok(())
        };
        edge(Event::RequestStop, JoinEvent::RequestStop)?;
        edge(Event::WaitYield, JoinEvent::YieldWhileWaiting)?;
        for (i, child) in waiting.children.iter().enumerate() {
            let mut child_edge = |event, observed| edge(Event::Child(i, event), observed);
            match child {
                ChildState::Entry(_) => {
                    for yielded in [false, true] {
                        child_edge(
                            ChildEvent::Entry { yielded },
                            JoinEvent::EnterChild { slot: i, yielded },
                        )?;
                    }
                }
                ChildState::Running(_) => {
                    child_edge(ChildEvent::Export, JoinEvent::StageResult(i))?
                }
                ChildState::Cleaning { .. } => {
                    child_edge(ChildEvent::EndView, JoinEvent::EndChildView(i))?
                }
                ChildState::AtExit(_) => child_edge(ChildEvent::Finish, JoinEvent::FinishChild(i))?,
                ChildState::Complete => {}
            }
        }
        budget.reserve(1)?;
        waiting_states.push(JoinWait {
            delivery: key.delivery,
            deferred: key.stopping,
            children: key.children,
            stop_requested: key.stop_requested,
            initialized_results: key.stages,
            retained_backings: waiting.backings(),
            environment_backings: waiting.environment_backings(),
            retained_results: waiting.retained_results(),
            successors: edges,
        });
    }
    let (resume, normal) = successors.normal.ok_or(Stop::Pending(site.0.node))?;
    let parent_exit = successors.cancel.ok_or(Stop::Pending(site.0.node))?;
    budget.reserve(1)?;
    Ok((
        Box::new(ScopedJoinProof {
            family: &role.family.id,
            implementation: &role.implementation.id,
            bridge: &role.bridge.id,
            frames,
            entries,
            waiting: waiting_states,
            before_commit,
            parent_exit,
        }),
        resume,
        normal,
    ))
}
