//! Result-use transfers for an ordinary two-invocation body and the distinct installed join.
use super::*;
use crate::analysis::InvocationMode;
use crate::analysis::generics::{
    GoalFacts, GoalId, GoalJudgment, GoalState, callables::CaptureForm,
};
use crate::analysis::ordinary::RequirementTarget;
use crate::analysis::scalars::CallFacts;

mod references;
use references::shared_i32;

pub(super) type PairBodies<'g> = BTreeMap<&'g DeclarationId, PairBody<'g>>;

#[derive(Clone, Copy)]
pub(super) struct PairBody<'g> {
    body: Site<'g>,
    slots: [Slot<'g>; 2],
}

/// Exact source routing and concrete environments proved at one call. This
/// does not acquire loans or prove ownership, cleanup, or cancellation.
#[derive(Debug)]
pub(in crate::analysis) struct PairCall<'g> {
    pub implementation: &'g DeclarationId,
    pub body: Site<'g>,
    pub argument: NodeId,
    pub children: [PairChild<'g>; 2],
}

#[derive(Debug)]
pub(in crate::analysis) struct PairChild<'g> {
    pub element: NodeId,
    pub invocation: Site<'g>,
    pub body: Site<'g>,
    pub expression: NodeId,
    pub arrow: Arc<crate::analysis::generics::callables::ArrowFacts<'g>>,
}

/// Concrete checked input and output route for the selected compiler operation.
/// This is not an ownership or cancellation judgment.
#[derive(Debug)]
pub struct ScopedJoinCall<'g> {
    pub role: super::super::RoutineRole<'g>,
    pub alternative: usize,
    pub slots: [ScopedJoinSlot<'g>; 2],
    pub(in crate::analysis) argument: NodeId,
}

#[derive(Debug)]
pub struct ScopedJoinSlot<'g> {
    pub path: [usize; 1],
    pub callable_once: GoalId,
    pub send_job: GoalId,
    pub send_result: GoalId,
    /// Exact source for a checked shared descriptor result, if this is a reference pair.
    pub reference_result: Option<ScopedJoinReference<'g>>,
    pub(in crate::analysis) child: PairChild<'g>,
}

/// Source of a returned shared descriptor, not an environment loan or a
/// proof that the descriptor can leave the child frame.
#[derive(Clone, Copy, Debug)]
pub enum ScopedJoinReference<'g> {
    Incoming {
        parameter: &'g DeclarationId,
        value: Site<'g>,
    },
    ChildLocal {
        binding: Site<'g>,
        returning: Site<'g>,
    },
}

impl<'g> ScopedJoinSlot<'g> {
    pub fn arrow(&self) -> &crate::analysis::generics::callables::ArrowFacts<'g> {
        &self.child.arrow
    }
}

#[derive(Debug)]
pub(super) enum CheckedTransfer<'g> {
    Serial(Box<PairCall<'g>>),
    Scoped(Box<ScopedJoinCall<'g>>),
}

#[derive(Clone, Copy)]
pub(super) enum PairBoundary<'g> {
    Serial(PairBody<'g>),
    Scoped(super::super::RoutineRole<'g>, PairBody<'g>),
}

#[derive(Clone, Copy)]
pub(super) struct Slot<'g> {
    job: (&'g DeclarationId, usize),
    result: (&'g DeclarationId, usize),
    // Written callableOnce, sendable(job), sendable(result), respectively.
    requirements: [NodeId; 3],
    invocation: Site<'g>,
}

pub(super) fn summary<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    signature: &DeclarationFacts<'g>,
    metadata: &crate::analysis::ordinary::HeaderMetadata<'g>,
    bodies: &GenericReport<'g>,
    nodes: &Nodes<'_, 'g>,
    budget: &mut Budget,
) -> Work<Option<PairBody<'g>>> {
    let modules = inventory.graph().modules();
    budget.step(modules.len())?;
    let Some(module) = modules
        .iter()
        .find(|m| m.key == signature.declaration.id.module)
    else {
        return Ok(None);
    };
    let Some(&body) = module.syntax.nodes[signature.declaration.node]
        .children
        .get(1)
    else {
        return Ok(None);
    };
    let Some((returned, tuple)) = single_return(module, body) else {
        return Ok(None);
    };
    let tuple = values::ungroup(module, tuple, budget)?;
    let Some((_, facts)) = nodes.get(&(&module.key, tuple)) else {
        return Ok(None);
    };
    let NodeFactKind::Expression(value) = &facts.kind else {
        return Ok(None);
    };
    let Some(DataExpression::Tuple { elements }) = &value.data else {
        return Ok(None);
    };
    let [first, second] = elements.as_slice() else {
        return Ok(None);
    };
    let calls = [
        values::ungroup(module, *first, budget)?,
        values::ungroup(module, *second, budget)?,
    ];
    Ok(summarize(
        module,
        signature,
        metadata,
        bodies,
        nodes,
        (returned, tuple, calls),
    )
    .map(|slots| PairBody {
        body: Site {
            module: &module.key,
            node: body,
            span: &module.syntax.nodes[body].span,
        },
        slots,
    }))
}

// The entire body must consume jobs[i]() exactly once into return slot i. This
// proves only the routing, under the two written assumptions. No concrete
// caller participates in the check, and there is no default or discarded slot.
fn summarize<'g>(
    module: &'g Module,
    signature: &DeclarationFacts<'g>,
    metadata: &crate::analysis::ordinary::HeaderMetadata<'g>,
    bodies: &GenericReport<'g>,
    nodes: &Nodes<'_, 'g>,
    (returned, tuple, calls): (NodeId, NodeId, [NodeId; 2]),
) -> Option<[Slot<'g>; 2]> {
    let DeclarationShape::Function { parameters, result } = &signature.shape else {
        return None;
    };
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    let TypeTerm::Tuple(jobs) = parameter.ty.as_ref() else {
        return None;
    };
    let TypeTerm::Tuple(results) = result.as_ref() else {
        return None;
    };
    if jobs.len() != 2
        || results.len() != 2
        || signature.binders.len() != 4
        || metadata.requirements.len() != 6
    {
        return None;
    }
    let mut positions = [false; 4];
    for ty in jobs.iter().chain(results) {
        let (binder, position) = binder(ty)?;
        if signature.binders.get(position)?.declaration.id != *binder
            || std::mem::replace(&mut positions[position], true)
        {
            return None;
        }
    }
    let syntax = &module.syntax.nodes;
    let fact = |node| nodes.get(&(&module.key, node)).map(|&(_, f)| f);
    let expression = |node| match &fact(node)?.kind {
        NodeFactKind::Expression(value) if value.ordinary_context.is_none() => Some(value),
        _ => None,
    };
    if !matches!(fact(returned)?.kind, NodeFactKind::Return { target } if *target == signature.declaration.id)
    {
        return None;
    }
    let value = expression(tuple)?;
    if value.ty != *result {
        return None;
    }
    let slot = |i: usize, call: NodeId| {
        let call_fact = fact(call)?;
        let invocation = call_fact.callable.as_ref()?.invocation.as_ref()?;
        let path = invocation.path.as_ref()?;
        let receiver = expression(invocation.receiver)?;
        if syntax[call].kind != NodeKind::Call
            || syntax[call].children.as_slice() != [invocation.receiver]
            || invocation.mode != InvocationMode::Once
            || invocation.access != Access::Owned
            || *path.root != parameter.declaration.id
            || path.projections.as_slice() != [PathProjection::TupleElement(i)]
            || receiver.ty != jobs[i]
            || !matches!(receiver.data, Some(DataExpression::TupleIndex { component, .. }) if component == i)
            || expression(call)?.ty != results[i]
        {
            return None;
        }
        let assumption = bodies.goal(invocation.assumption.as_ref()?).ok()?;
        let args = requirement(assumption, "callableOnce", GoalState::Assumption)?;
        let [job, input, output] = args else {
            return None;
        };
        if *job != jobs[i]
            || *output != results[i]
            || !empty_tuple(input)
            || assumption.source.module != &module.key
            || !metadata
                .requirements
                .iter()
                .any(|req| req.node == assumption.source.node)
        {
            return None;
        }
        let send = |ty: &Type<'g>| {
            let mut matches = metadata.requirements.iter().filter(|req| {
                matches!(req.target, Some(RequirementTarget::Predicate(id)) if id.name() == "sendable")
                    && matches!(req.arguments.as_slice(), [Some(arg)] if arg == ty)
            });
            let source = matches.next()?.node;
            matches.next().is_none().then_some(source)
        };
        Some(Slot {
            job: binder(job)?,
            result: binder(output)?,
            requirements: [assumption.source.node, send(job)?, send(output)?],
            invocation: call_fact.site,
        })
    };
    Some([slot(0, calls[0])?, slot(1, calls[1])?])
}

fn single_return(module: &Module, body: NodeId) -> Option<(NodeId, NodeId)> {
    let nodes = &module.syntax.nodes;
    let [returned] = nodes[body].children.as_slice() else {
        return None;
    };
    let [value] = nodes[*returned].children.as_slice() else {
        return None;
    };
    (nodes[body].kind == NodeKind::Block && nodes[*returned].kind == NodeKind::Return)
        .then_some((*returned, *value))
}

fn binder<'g>(ty: &Type<'g>) -> Option<(&'g DeclarationId, usize)> {
    let TypeTerm::Parameter {
        declaration,
        position,
    } = ty.as_ref()
    else {
        return None;
    };
    Some((declaration, *position))
}

fn empty_tuple(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Tuple(elements) if elements.is_empty())
}

fn requirement<'a, 'g>(
    goal: &'a GoalFacts<'g>,
    name: &str,
    state: GoalState,
) -> Option<&'a [Type<'g>]> {
    let GoalJudgment::Requirement {
        target: RequirementTarget::Predicate(id),
        arguments,
    } = &goal.judgment
    else {
        return None;
    };
    // Predicate IDs are from the captured prelude, not source declarations.
    (id.name() == name && goal.state == state && goal.premise.is_none()).then_some(arguments)
}

/// Check the designated wrapper's real source route. There is no ordinary
/// invocation body to interpret for either job: the bridge owns both slots.
pub(super) fn scoped_summary<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    role: super::super::RoutineRole<'g>,
    signature: &DeclarationFacts<'g>,
    nodes: &Nodes<'_, 'g>,
    budget: &mut Budget,
) -> Work<Option<PairBody<'g>>> {
    let Some(module) = inventory.graph().installed_routine_source() else {
        return Ok(None);
    };
    // Each comparison against the captured routine key has bounded width;
    // the maps it is looked up in can grow with the application, however.
    let width = module.key.path.len() + 4;
    budget.step(width.checked_mul(inventory.headers().len() + 1).ok_or(())?)?;
    let Some(metadata) = inventory.headers().get(&role.implementation.id) else {
        return Ok(None);
    };
    if metadata.requirements.len() != 6 {
        return Ok(None);
    }
    let lookup = width.checked_mul(nodes.len() + 1).ok_or(())?;
    // Three node lookups below; each slot scans the six written requirements
    // for its callable goal and its two sendability goals.
    budget.step(lookup.checked_mul(3).ok_or(())?)?;
    budget.step(8 + 6 * metadata.requirements.len())?;
    let checked = || {
        let body = *module.syntax.nodes[role.implementation.node]
            .children
            .get(1)?;
        let (returned, operation) = single_return(module, body)?;
        let fact = |n| nodes.get(&(&module.key, n)).map(|&(_, f)| f);
        if !matches!(fact(returned)?.kind, NodeFactKind::Return { target } if *target == role.implementation.id)
        {
            return None;
        }
        let operation = fact(operation)?;
        let NodeFactKind::Expression(returned) = &operation.kind else {
            return None;
        };
        if returned.ordinary_context.is_some() {
            return None;
        }
        let call = returned.call.as_ref()?;
        let [argument] = call.arguments.as_slice() else {
            return None;
        };
        let DeclarationShape::Function { parameters, result } = &signature.shape else {
            return None;
        };
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        let (TypeTerm::Tuple(jobs), TypeTerm::Tuple(results)) =
            (parameter.ty.as_ref(), result.as_ref())
        else {
            return None;
        };
        let arg = fact(*argument)?;
        let NodeFactKind::Expression(input) = &arg.kind else {
            return None;
        };
        if input.ordinary_context.is_some()
            || call.callee != &role.bridge.id
            || call.selection.is_some()
            || call.pipe_input.is_some()
            || !call.defaults.is_empty()
            || returned.ty != *result
            || input.ty != parameter.ty
            || input.access != Access::Owned
            || !input.path.as_ref().is_some_and(|path| {
                path.root == &parameter.declaration.id && path.projections.is_empty()
            })
            || jobs.len() != 2
            || results.len() != 2
            || signature.binders.len() != 4
        {
            return None;
        }
        let mut positions = [false; 4];
        for ty in jobs.iter().chain(results) {
            let (binder, position) = binder(ty)?;
            if signature.binders.get(position)?.declaration.id != *binder
                || std::mem::replace(&mut positions[position], true)
            {
                return None;
            }
        }
        let operation = operation.site;
        let slot = |i: usize| {
            let requirement = |name: &str, arguments: &[&Type<'g>]| {
                let mut requirements = metadata.requirements.iter().filter(|req| {
                    matches!(req.target, Some(RequirementTarget::Predicate(id)) if id.name() == name)
                        && req.arguments.len() == arguments.len()
                        && req.arguments.iter().zip(arguments).all(|(actual, expected)| actual.as_ref() == Some(*expected))
                });
                let node = requirements.next()?.node;
                requirements.next().is_none().then_some(node)
            };
            let mut invokes = metadata.requirements.iter().filter(|req| {
                matches!(req.target, Some(RequirementTarget::Predicate(id)) if id.name() == "callableOnce")
                    && matches!(req.arguments.as_slice(), [Some(job), Some(input), Some(output)]
                        if *job == jobs[i] && empty_tuple(input) && *output == results[i])
            });
            let invoke = invokes.next()?.node;
            if invokes.next().is_some() {
                return None;
            }
            Some(Slot {
                job: binder(&jobs[i])?,
                result: binder(&results[i])?,
                requirements: [
                    invoke,
                    requirement("sendable", &[&jobs[i]])?,
                    requirement("sendable", &[&results[i]])?,
                ],
                invocation: operation,
            })
        };
        Some(PairBody {
            body: operation,
            slots: [slot(0)?, slot(1)?],
        })
    };
    Ok(checked())
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn transfer_pair(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        call: &CallFacts<'g>,
        state: &mut Residence<'g>,
        boundary: PairBoundary<'g>,
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
        let (pair, scoped) = match boundary {
            PairBoundary::Serial(pair) => (pair, None),
            PairBoundary::Scoped(role, pair) => (pair, Some(role)),
        };
        let Some(selected) = &call.selection else {
            return Ok(false);
        };
        let [argument] = call.arguments.as_slice() else {
            return Ok(false);
        };
        if value.ordinary_context.is_some()
            || value.data.is_some()
            || call.pipe_input.is_some()
            || !call.defaults.is_empty()
            || selected.substitution.len() != 4
            || self.module.syntax.nodes[node].kind != NodeKind::Call
        {
            return Ok(false);
        }
        self.budget.step(self.inventory.graph().modules().len())?;
        let Some(ordinary) = self.bodies.ordinary_call(self.module, node).ok().flatten() else {
            return Ok(false);
        };
        let Some(goal) = ordinary
            .goal
            .as_ref()
            .and_then(|id| self.bodies.goal(id).ok())
        else {
            return Ok(false);
        };
        if !ordinary.checked
            || ordinary.premise.is_some()
            || ordinary.instantiation.is_some()
            || ordinary.family != call.callee
            || goal.state != GoalState::Proved
            || goal.premise.is_some()
            || goal.dependencies.len() != 6
            || !goal.selection.as_ref().is_some_and(|winner| {
                winner.implementation == selected.implementation
                    && winner.alternative == selected.alternative
            })
        {
            return Ok(false);
        }
        if let Some(role) = scoped
            && (call.callee != &role.family.id
                || selected.implementation != &role.implementation.id
                // The ordinary comparison first enumerated and projected
                // every captured candidate. An intersecting contender cannot
                // be erased by the operation binding.
                || !matches!(ordinary.candidates.as_slice(), [only]
                    if only.implementation == selected.implementation
                        && only.alternative == selected.alternative
                        && only.covers_observed_inputs))
        {
            return Ok(false);
        }
        let references = scoped.is_some()
            && matches!(value.ty.as_ref(),TypeTerm::Tuple(items) if matches!(items.as_slice(),[a,b] if shared_i32(a) && shared_i32(b)));
        let (tuple, tuple_depth) = if references {
            self.reference_root(*argument, depth + 1)?
        } else {
            (self.ungroup_value(*argument)?, depth + 1)
        };
        let Some(input) = self.expression_facts(tuple)? else {
            return Ok(false);
        };
        let (
            Some(DataExpression::Tuple { elements }),
            TypeTerm::Tuple(inputs),
            TypeTerm::Tuple(outputs),
        ) = (&input.data, input.ty.as_ref(), value.ty.as_ref())
        else {
            return Ok(false);
        };
        let ([first, second], [job0, job1], [out0, out1]) =
            (elements.as_slice(), inputs.as_slice(), outputs.as_slice())
        else {
            return Ok(false);
        };
        let shapes = if references {
            let Some(a) = self.reference_shape(*first, tuple_depth + 1)? else {
                return Ok(false);
            };
            let Some(b) = self.reference_shape(*second, tuple_depth + 1)? else {
                return Ok(false);
            };
            Some([a, b])
        } else {
            None
        };
        let root = Root::Temporary(*argument);
        let stored = state.contents(root, &[], self.budget)?;
        self.budget.step(stored.len())?;
        if stored.iter().any(|v| {
            !matches!(
                v.component.as_slice(),
                [ResultComponent::TupleElement(0 | 1)]
            )
        }) {
            return Ok(false);
        }
        let elements = [*first, *second];
        let [first, second] = if let Some(shapes) = shapes {
            shapes.map(|s| s.origin)
        } else {
            [self.ungroup_value(*first)?, self.ungroup_value(*second)?]
        };
        if first == second {
            return Ok(false);
        }
        let mut reference_plans = [None, None];
        let mut outputs = [Vec::new(), Vec::new()];
        let mut children = [None, None];
        let mut scoped_slots = [None, None];
        for (i, ((origin, job, output), slot)) in [(first, job0, out0), (second, job1, out1)]
            .into_iter()
            .zip(pair.slots)
            .enumerate()
        {
            let sub = |(binder, position): (&DeclarationId, usize)| {
                selected
                    .substitution
                    .get(position)
                    .filter(|s| s.binder == binder && s.position == position)
                    .map(|s| &s.argument)
            };
            let (Some(job_sub), Some(result_sub)) = (sub(slot.job), sub(slot.result)) else {
                return Ok(false);
            };
            let Some(arrow) = self
                .facts(origin)?
                .and_then(|f| f.callable.as_ref()?.arrow.as_ref())
            else {
                return Ok(false);
            };
            if self.module.syntax.nodes[origin].kind != NodeKind::Arrow
                || arrow.site.module != &self.module.key
                || arrow.site.node != origin
                || arrow.premise.is_some()
                || !arrow.parameters.is_empty()
                || !self.same_job(job, job_sub, references, depth + 1)?
                || !self.result_agrees(output, result_sub, references, depth + 1)?
                || !self.result_agrees(output, &arrow.result, references, depth + 1)?
            {
                return Ok(false);
            }
            // The selected goal and CallFacts share the winner's substitution.
            // Link the instantiated premises by written occurrence, not by
            // searching the goal list for another job/result of the same type.
            let mut proved = [None, None, None];
            for id in &goal.dependencies {
                self.budget.step(1)?;
                let Ok(evidence) = self.bodies.goal(id) else {
                    return Ok(false);
                };
                if evidence.source.module == &selected.implementation.module
                    && let Some(index) = slot
                        .requirements
                        .iter()
                        .position(|&n| n == evidence.source.node)
                {
                    if proved[index].is_some()
                        || evidence.state != GoalState::Proved
                        || evidence.premise.is_some()
                        || index == 1
                            && !evidence
                                .arrow_origin
                                .as_ref()
                                .is_some_and(|known| Arc::ptr_eq(known, arrow))
                    {
                        return Ok(false);
                    }
                    proved[index] = Some(id);
                }
            }
            let [Some(callable_once), Some(send_job), Some(send_result)] = proved else {
                return Ok(false);
            };
            let slot_path = ResultComponent::TupleElement(i);
            let (expression, result, reference_result) = if let Some(shapes) = shapes {
                let Some(plan) =
                    self.reference_plan(shapes[i], arrow, (&stored, slot_path), state, depth + 1)?
                else {
                    return Ok(false);
                };
                reference_plans[i] = Some(plan);
                (Some(plan.expression), Vec::new(), Some(plan.reference))
            } else {
                let Some((expression, result)) = self.pair_child(
                    origin,
                    arrow,
                    (&stored, slot_path),
                    state,
                    scoped.map(|_| [out0, out1][1 - i]),
                    depth + 1,
                )?
                else {
                    return Ok(false);
                };
                (expression, result, None)
            };
            outputs[i] = result;
            let child = expression.map(|expression| PairChild {
                element: elements[i],
                invocation: slot.invocation,
                body: self.site(self.module.syntax.nodes[origin].children[1]),
                expression,
                arrow: Arc::clone(arrow),
            });
            if scoped.is_some() {
                let Some(child) = child else {
                    return Ok(false);
                };
                if !references {
                    self.budget.reserve(3)?;
                }
                scoped_slots[i] = Some(ScopedJoinSlot {
                    path: [i],
                    child,
                    callable_once: callable_once.clone(),
                    send_job: send_job.clone(),
                    send_result: send_result.clone(),
                    reference_result,
                });
            } else {
                children[i] = child;
            }
        }
        if let [Some(left), Some(right)] = reference_plans {
            self.budget.reserve(8)?; // two reference origins and three goal handles per slot
            if !self.prove_reference_plan(left, depth + 1)?
                || !self.prove_reference_plan(right, depth + 1)?
            {
                return Ok(false);
            }
        }
        // Transfer Result residence only after checking both environments and
        // both outputs. Sibling residences in the caller are unchanged.
        state.take(root, &[], self.budget)?;
        for (i, result) in outputs.into_iter().enumerate() {
            state.install(
                result,
                Root::Temporary(node),
                &[ResultComponent::TupleElement(i)],
                self.budget,
            )?;
        }
        // Every reached alternative derives the same two source occurrences;
        // the receipt separately requires all residence checks to close.
        if let Some(role) = scoped {
            let [Some(first), Some(second)] = scoped_slots else {
                unreachable!("proved both scoped slots")
            };
            self.budget.step(self.module.key.path.len() + 2)?;
            let index = self.nodes[&(&self.module.key, node)].0;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                self.report.pair_calls.entry(index)
            {
                self.budget.reserve(3)?;
                entry.insert(CheckedTransfer::Scoped(Box::new(ScopedJoinCall {
                    role,
                    alternative: selected.alternative,
                    slots: [first, second],
                    argument: *argument,
                })));
            }
        } else if let [Some(first), Some(second)] = children {
            self.budget.step(self.module.key.path.len() + 2)?;
            let index = self.nodes[&(&self.module.key, node)].0;
            if let std::collections::btree_map::Entry::Vacant(entry) =
                self.report.pair_calls.entry(index)
            {
                self.budget.reserve(3)?;
                entry.insert(CheckedTransfer::Serial(Box::new(PairCall {
                    implementation: selected.implementation,
                    body: pair.body,
                    argument: *argument,
                    children: [first, second],
                })));
            }
        }
        Ok(true)
    }

    fn same_job(
        &mut self,
        left: &Type<'g>,
        right: &Type<'g>,
        references: bool,
        depth: usize,
    ) -> Work<bool> {
        let (
            TypeTerm::Callable {
                mode: left_mode,
                parameters: left,
                result: a,
            },
            TypeTerm::Callable {
                mode: right_mode,
                parameters: right,
                result: b,
            },
        ) = (left.as_ref(), right.as_ref())
        else {
            return Ok(false);
        };
        if left_mode != right_mode || !left.is_empty() || !right.is_empty() {
            return Ok(false);
        }
        self.result_agrees(a, b, references, depth)
    }

    // Closed scalar products, or the exact reference pair with a separate
    // origin check for each descriptor. No nominal or erased storage.
    fn result_agrees(
        &mut self,
        left: &Type<'g>,
        right: &Type<'g>,
        references: bool,
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
        if references {
            return Ok(shared_i32(left) && shared_i32(right));
        }
        match (left.as_ref(), right.as_ref()) {
            (TypeTerm::Primitive(a), TypeTerm::Primitive(b)) => {
                Ok(a == b && !matches!(a.name(), "void" | "never"))
            }
            (TypeTerm::Tuple(a), TypeTerm::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    if !self.result_agrees(a, b, false, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn pair_child(
        &mut self,
        origin: NodeId,
        arrow: &crate::analysis::generics::callables::ArrowFacts<'g>,
        (stored, slot): (&[Located<'g>], ResultComponent<'g>),
        caller: &Residence<'g>,
        scoped_sibling: Option<&Type<'g>>,
        depth: usize,
    ) -> Work<Option<(Option<NodeId>, Vec<Located<'g>>)>> {
        let nodes = &self.module.syntax.nodes;
        let body = nodes[origin].children[1];
        let body = if nodes[body].kind == NodeKind::Block {
            let Some((returned, value)) = single_return(self.module, body) else {
                return Ok(None);
            };
            if !self.facts(returned)?.is_some_and(
                |f| matches!(f.kind, NodeFactKind::ArrowReturn { target } if target == origin),
            ) {
                return Ok(None);
            }
            value
        } else {
            body
        };
        let capture = match arrow.captures.as_slice() {
            [] if arrow.mode.is_some() => None,
            [capture]
                if capture.reachable
                    && capture.through.is_none()
                    && capture.path.projections.is_empty() =>
            {
                match (capture.form, arrow.mode) {
                    (
                        CaptureForm::ExplicitBorrow { mutable: false },
                        Some(InvocationMode::ReadOnly),
                    ) => Some((capture, false)),
                    (
                        CaptureForm::ExplicitBorrow { mutable: true },
                        Some(InvocationMode::Mutable),
                    ) if scoped_sibling.is_some() => Some((capture, true)),
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        if !self.pair_environment(origin, (stored, slot), capture.map(|(c, _)| c.path.root))? {
            return Ok(None);
        }
        let list = if let Some((capture, exclusive)) = capture {
            let Some(TypeTerm::Reference { mutable, referent }) = capture.storage.as_deref() else {
                return Ok(None);
            };
            if *mutable != exclusive || capture.storage.as_ref() != Some(&capture.ty) {
                return Ok(None);
            }
            let TypeTerm::Nominal {
                declaration,
                arguments,
            } = referent.as_ref()
            else {
                return Ok(None);
            };
            if !self
                .core
                .collections
                .list
                .is_some_and(|role| &role.declaration.id == *declaration)
                || !matches!(arguments.as_slice(), [element] if matches!(element.as_ref(), TypeTerm::Primitive(_)))
            {
                return Ok(None);
            }
            let backing = caller.contents(Root::Local(capture.path.root), &[], self.budget)?;
            self.budget.step(backing.len())?;
            if backing
                .iter()
                .any(|v| !matches!(v.content, Content::Test(_, _)))
            {
                return Ok(None);
            }
            Some((*declaration, capture.site.node))
        } else {
            None
        };
        let compare = capture.is_some_and(|(_, exclusive)| !exclusive)
            && scoped_sibling.is_some_and(
                |ty| matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"),
            );
        let Some(root) = self.closed_child_expression(body, list, compare, depth + 1)? else {
            return Ok(None);
        };
        let list_result = (list.is_some()
            && match nodes[root].kind {
                NodeKind::Index => {
                    matches!(arrow.result.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                }
                NodeKind::Binary("==") if compare => {
                    matches!(arrow.result.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
                }
                _ => false,
            })
        .then_some(body);
        // Prove the actual expression's Result residence, not permission to
        // use the captured access. M5 checks the loan when the environment forms.
        if capture.is_some_and(|(_, exclusive)| exclusive) && list_result.is_none() {
            return Ok(None);
        }
        let Some(value) = self.expression_facts(body)? else {
            return Ok(None);
        };
        if !self.result_agrees(&arrow.result, &value.ty, false, depth + 1)? {
            return Ok(None);
        }
        // The restricted child can only read its proved Result-free List
        // referent and construct its result. No owned input is substituted in
        // the body. Keep the output's actual residence (including test tags).
        let mut proof = self.callable_check(self.module)?;
        let mut alternatives = proof.expression(body, Residence::default(), depth + 1)?;
        let [state] = alternatives.as_mut_slice() else {
            return Ok(None);
        };
        let result = state.take(Root::Temporary(body), &[], proof.budget)?;
        proof.budget.step(result.len())?;
        Ok((!proof.has_pending
            && !state.may_owe_result(proof.budget)?
            && result
                .iter()
                .all(|v| matches!(v.content, Content::Test(_, _))))
        .then_some((list_result, result)))
    }

    fn pair_environment(
        &mut self,
        origin: NodeId,
        (stored, slot): (&[Located<'g>], ResultComponent<'g>),
        capture: Option<&'g DeclarationId>,
    ) -> Work<bool> {
        let mut marker = false;
        let mut alias = false;
        let mut callable = false;
        for stored in stored {
            self.budget.step(1)?;
            if stored.component != [slot] {
                continue;
            }
            match &stored.content {
                Content::Callable(found) if *found == origin && !callable => callable = true,
                Content::Unclassified(found)
                    if capture.is_some() && *found == origin && !marker =>
                {
                    marker = true
                }
                Content::PossibleAlias((Root::Local(root), path))
                    if !alias
                        && path.is_empty()
                        && capture.is_some_and(|id| std::ptr::eq(id, *root)) =>
                {
                    alias = true
                }
                _ => return Ok(false),
            }
        }
        Ok(callable && marker == capture.is_some() && alias == capture.is_some())
    }

    fn closed_child_expression(
        &mut self,
        node: NodeId,
        list: Option<(&'g DeclarationId, NodeId)>,
        compare: bool,
        depth: usize,
    ) -> Work<Option<NodeId>> {
        self.depth(depth)?;
        let node = self.ungroup_value(node)?;
        let Some(facts) = self.facts(node)? else {
            return Ok(None);
        };
        let NodeFactKind::Expression(value) = &facts.kind else {
            return Ok(None);
        };
        if value.ordinary_context.is_some() || value.call.is_some() || !facts.flow.normal {
            return Ok(None);
        }
        let nodes = &self.module.syntax.nodes;
        let checked = match &value.data {
            None if compare && nodes[node].kind == NodeKind::Binary("==") => {
                let [left, right] = nodes[node].children.as_slice() else {
                    return Ok(None);
                };
                let Some(read) = self.closed_child_expression(*left, list, false, depth + 1)?
                else {
                    return Ok(None);
                };
                let Some(literal) = self.closed_child_expression(*right, None, false, depth + 1)?
                else {
                    return Ok(None);
                };
                nodes[read].kind == NodeKind::Index
                    && nodes[literal].kind == NodeKind::Literal
                    && self.expression_facts(read)?.is_some_and(
                        |e| matches!(e.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"),
                    )
                    && self.expression_facts(literal)?.is_some_and(
                        |e| matches!(e.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"),
                    )
                    && matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            }
            Some(DataExpression::Tuple { elements }) => {
                for &element in elements {
                    if self
                        .closed_child_expression(element, list, false, depth + 1)?
                        .is_none()
                    {
                        return Ok(None);
                    }
                }
                true
            }
            Some(DataExpression::ListIndex {
                declaration,
                base,
                selector,
            }) => {
                list == Some((*declaration, self.ungroup_value(*base)?))
                    && self.module.syntax.nodes[self.ungroup_value(*selector)?].kind
                        == NodeKind::Literal
                    && matches!(value.ty.as_ref(), TypeTerm::Primitive(_))
            }
            None => {
                self.module.syntax.nodes[node].kind == NodeKind::Literal
                    && matches!(value.ty.as_ref(), TypeTerm::Primitive(_))
            }
            _ => false,
        };
        Ok(checked.then_some(node))
    }
}
