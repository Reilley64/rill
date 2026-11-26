//! Result-use transfers for an ordinary two-invocation body and the distinct installed join.
use super::*;
use crate::analysis::InvocationMode;
use crate::analysis::generics::{
    GoalFacts, GoalId, GoalJudgment, GoalState, callables::CaptureForm,
};
use crate::analysis::ordinary::RequirementTarget;
use crate::analysis::scalars::CallFacts;

mod references;
mod results;
use references::shared_i32;

pub(super) type PairBodies<'g> = keyed::PathMap<&'g DeclarationId, PairBody<'g>>;

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
    /// Original construction for a checked fresh Result, before slot publication.
    pub result_origin: Option<Site<'g>>,
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
struct ListComparison<'g> {
    expression: NodeId,
    list: &'g DeclarationId,
    base: NodeId,
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
    let Some(table) = nodes.table(&module.key, budget)? else {
        return Ok(None);
    };
    let Some((_, facts)) = table.get(&tuple) else {
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
        table,
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
    table: &NodeTable<'_, 'g>,
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
    let fact = |node| table.get(&node).map(|&(_, f)| f);
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
    let Some(metadata) = inventory.headers().get(&role.implementation.id) else {
        return Ok(None);
    };
    if metadata.requirements.len() != 6 {
        return Ok(None);
    }
    let Some(table) = nodes.table(&module.key, budget)? else {
        return Ok(None);
    };
    budget.step(8 + 6 * metadata.requirements.len())?;
    let checked = || {
        let body = *module.syntax.nodes[role.implementation.node]
            .children
            .get(1)?;
        let (returned, operation) = single_return(module, body)?;
        let fact = |n| table.get(&n).map(|&(_, f)| f);
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
        let fresh_result = scoped.and_then(|_| self.fresh_result_slot(&value.ty));
        let compared = if scoped.is_some()
            && let TypeTerm::Tuple(items) = value.ty.as_ref()
            && let [a, b] = items.as_slice()
            && let (TypeTerm::Primitive(a), TypeTerm::Primitive(b)) = (a.as_ref(), b.as_ref())
        {
            match (a.name(), b.name()) {
                ("bool", "i32") => Some(0),
                ("i32", "bool") => Some(1),
                _ => None,
            }
        } else {
            None
        };
        let (tuple, tuple_depth) = if references || compared.is_some() || fresh_result.is_some() {
            self.value_root_at(*argument, depth + 1)?
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
        // Reject unsupported comparisons before cloning residences or proving
        // either sibling. Retain the checked List and base for the capture check.
        let mut comparisons = [None, None];
        if let Some(i) = compared {
            let Some(shape) = self.list_comparison(elements[i], tuple_depth + 1)? else {
                return Ok(false);
            };
            comparisons[i] = Some(shape);
        }
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
        let [(first, first_depth), (second, second_depth)] = if fresh_result.is_some() {
            [
                self.value_root_at(*first, tuple_depth + 1)?,
                self.value_root_at(*second, tuple_depth + 1)?,
            ]
        } else if let Some(shapes) = shapes {
            shapes.map(|s| (s.origin, depth + 1))
        } else {
            [
                (self.ungroup_value(*first)?, depth + 1),
                (self.ungroup_value(*second)?, depth + 1),
            ]
        };
        if first == second {
            return Ok(false);
        }
        let mut reference_plans = [None, None];
        let mut fresh_plans = [None, None];
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
                || if fresh_result.is_some() {
                    !matches!(job.as_ref(), TypeTerm::Callable { mode, parameters, result }
                        if Some(*mode) == arrow.mode && parameters.is_empty() && result == output)
                        || job != job_sub
                        || output != result_sub
                        || output != &arrow.result
                } else {
                    !self.same_job(job, job_sub, references, depth + 1)?
                        || !self.result_agrees(output, result_sub, references, depth + 1)?
                        || !self.result_agrees(output, &arrow.result, references, depth + 1)?
                }
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
            let mut result_origin = None;
            let (expression, result, reference_result) = if let Some(result_slot) = fresh_result {
                let Some(plan) = self.fresh_pair_plan(
                    origin,
                    arrow,
                    (&stored, slot_path),
                    i == result_slot,
                    [first_depth, second_depth][i],
                )?
                else {
                    return Ok(false);
                };
                let plan = fresh_plans[i].insert(plan);
                result_origin = plan.construction;
                (Some(plan.expression), Vec::new(), None)
            } else if let Some(shapes) = shapes {
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
                    (scoped.is_some(), comparisons[i]),
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
                self.budget.reserve(3)?; // three goal handles, before the slot is built
                scoped_slots[i] = Some(ScopedJoinSlot {
                    path: [i],
                    child,
                    callable_once: callable_once.clone(),
                    send_job: send_job.clone(),
                    send_result: send_result.clone(),
                    reference_result,
                    result_origin,
                });
            } else {
                children[i] = child;
            }
        }
        // Neither isolated child checker is needed until both source jobs,
        // their environments, and their selected premises pass preflight.
        if let [Some(left), Some(right)] = fresh_plans {
            for (output, plan) in outputs.iter_mut().zip([left, right]) {
                let Some(proved) = self.prove_fresh_pair_plan(plan, state)? else {
                    return Ok(false);
                };
                *output = proved;
            }
        }
        if let [Some(left), Some(right)] = reference_plans {
            self.budget.reserve(2)?; // two reference origins; the goal handles are reserved above
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
            let index = self.table.get(&node).unwrap().0;
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
            let index = self.table.get(&node).unwrap().0;
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
        (scoped, comparison): (bool, Option<ListComparison<'g>>),
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
                    ) if scoped => Some((capture, true)),
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
        let comparison = comparison.filter(|_| capture.is_some_and(|(_, exclusive)| !exclusive));
        let Some(root) = self.closed_child_expression(body, list, comparison, depth + 1)? else {
            return Ok(None);
        };
        let list_result = (list.is_some()
            && match nodes[root].kind {
                NodeKind::Index => {
                    matches!(arrow.result.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                }
                NodeKind::Binary("==") if comparison.is_some() => {
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
        let mut proof = self.callable_check(self.module, self.table)?;
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

    fn list_comparison(
        &mut self,
        element: NodeId,
        depth: usize,
    ) -> Work<Option<ListComparison<'g>>> {
        let nodes = &self.module.syntax.nodes;
        let (arrow, depth) = self.value_root_at(element, depth)?;
        if nodes[arrow].kind != NodeKind::Arrow {
            return Ok(None);
        }
        let body = nodes[arrow].children[1];
        let (expression, depth) = if nodes[body].kind == NodeKind::Block {
            let Some((_, returned)) = single_return(self.module, body) else {
                return Ok(None);
            };
            (returned, depth + 3)
        } else {
            (body, depth + 1)
        };
        let (expression, depth) = self.value_root_at(expression, depth)?;
        let (NodeKind::Binary("=="), [left, right]) = (
            nodes[expression].kind,
            nodes[expression].children.as_slice(),
        ) else {
            return Ok(None);
        };
        let (read, read_depth) = self.value_root_at(*left, depth + 1)?;
        if nodes[read].kind != NodeKind::Index {
            return Ok(None);
        }
        let (literal, _) = self.value_root_at(*right, depth + 1)?;
        if nodes[literal].kind != NodeKind::Literal {
            return Ok(None);
        }
        let mut scalar = |node| -> Work<Option<&'a ExpressionFacts<'g>>> {
            Ok(self.facts(node)?.and_then(|f| match &f.kind {
                NodeFactKind::Expression(v)
                    if f.flow.normal
                        && v.ordinary_context.is_none()
                        && v.call.is_none()
                        && matches!(v.ty.as_ref(),TypeTerm::Primitive(p) if p.name()=="i32") =>
                {
                    Some(v)
                }
                _ => None,
            }))
        };
        let Some(read) = scalar(read)? else {
            return Ok(None);
        };
        let Some(DataExpression::ListIndex {
            declaration,
            base,
            selector,
        }) = &read.data
        else {
            return Ok(None);
        };
        if !scalar(literal)?.is_some_and(|r| r.literal.is_some()) {
            return Ok(None);
        }
        let (base, _) = self.value_root_at(*base, read_depth + 1)?;
        let (selector, _) = self.value_root_at(*selector, read_depth + 1)?;
        Ok(
            (nodes[selector].kind == NodeKind::Literal).then_some(ListComparison {
                expression,
                list: declaration,
                base,
            }),
        )
    }

    fn closed_child_expression(
        &mut self,
        node: NodeId,
        list: Option<(&'g DeclarationId, NodeId)>,
        comparison: Option<ListComparison<'g>>,
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
        let checked = match &value.data {
            None if let Some(shape) = comparison
                && node == shape.expression =>
            {
                list.is_some_and(|(owner, base)| {
                    std::ptr::eq(owner, shape.list) && base == shape.base
                }) && matches!(value.ty.as_ref(),TypeTerm::Primitive(p) if p.name()=="bool")
            }
            Some(DataExpression::Tuple { elements }) => {
                for &element in elements {
                    if self
                        .closed_child_expression(element, list, None, depth + 1)?
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
