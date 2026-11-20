//! Result-use transfer for one closed source pattern, not a scoped-work operation.
use super::*;
use crate::analysis::InvocationMode;
use crate::analysis::generics::{GoalFacts, GoalJudgment, GoalState, callables::CaptureForm};
use crate::analysis::ordinary::RequirementTarget;
use crate::analysis::scalars::CallFacts;

pub(super) type PairBodies<'g> = BTreeMap<&'g DeclarationId, [Slot<'g>; 2]>;

#[derive(Clone, Copy)]
pub(super) struct Slot<'g> {
    job: (&'g DeclarationId, usize),
    result: (&'g DeclarationId, usize),
    // Written callableOnce, sendable(job), sendable(result), respectively.
    requirements: [NodeId; 3],
}

pub(super) fn summaries<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    bodies: &GenericReport<'g>,
    nodes: &Nodes<'_, 'g>,
    checked: &BTreeSet<&'g DeclarationId>,
    headers: &BTreeMap<&'g DeclarationId, &DeclarationFacts<'g>>,
    budget: &mut Budget,
) -> Work<PairBodies<'g>> {
    let mut summaries = BTreeMap::new();
    for module in inventory.graph().modules() {
        for declaration in &module.declarations {
            budget.step(1)?;
            let id = &declaration.id;
            if checked.contains(id)
                && let Some(signature) = headers.get(id)
                && let Some(metadata) = inventory.headers().get(id)
                && let Some(slots) = summarize(module, signature, metadata, bodies, nodes)
            {
                budget.reserve(2)?;
                summaries.insert(id, slots);
            }
        }
    }
    Ok(summaries)
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
    let body = *syntax[signature.declaration.node].children.get(1)?;
    let (returned, tuple) = single_return(module, body)?;
    let fact = |node| nodes.get(&(&module.key, node)).copied();
    let expression = |node| match &fact(node)?.kind {
        NodeFactKind::Expression(value) if value.ordinary_context.is_none() => Some(value),
        _ => None,
    };
    if !matches!(fact(returned)?.kind, NodeFactKind::Return { target } if *target == signature.declaration.id)
    {
        return None;
    }
    let value = expression(tuple)?;
    let Some(DataExpression::Tuple { elements }) = &value.data else {
        return None;
    };
    let [first, second] = elements.as_slice() else {
        return None;
    };
    if value.ty != *result {
        return None;
    }
    let slot = |i: usize, call: NodeId| {
        let invocation = fact(call)?.callable.as_ref()?.invocation.as_ref()?;
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
        })
    };
    Some([slot(0, *first)?, slot(1, *second)?])
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

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn transfer_pair(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        call: &CallFacts<'g>,
        state: &mut Residence<'g>,
        slots: [Slot<'g>; 2],
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
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
        let tuple = self.ungroup_value(*argument)?;
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
        let first = self.ungroup_value(*first)?;
        let second = self.ungroup_value(*second)?;
        if first == second {
            return Ok(false);
        }
        let mut outputs = [Vec::new(), Vec::new()];
        for (i, ((origin, job, output), slot)) in [(first, job0, out0), (second, job1, out1)]
            .into_iter()
            .zip(slots)
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
                || !self.same_job(job, job_sub, depth + 1)?
                || !self.result_agrees(output, result_sub, depth + 1)?
                || !self.result_agrees(output, &arrow.result, depth + 1)?
            {
                return Ok(false);
            }
            // The selected goal and CallFacts share the winner's substitution.
            // Link the instantiated premises by written occurrence, not by
            // searching the goal list for another job/result of the same type.
            let mut proved = [false; 3];
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
                    if proved[index]
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
                    proved[index] = true;
                }
            }
            if proved != [true; 3] {
                return Ok(false);
            }
            let slot_path = ResultComponent::TupleElement(i);
            let Some(result) =
                self.pair_child(origin, arrow, &stored, slot_path, state, depth + 1)?
            else {
                return Ok(false);
            };
            outputs[i] = result;
        }
        // Commit only after both exact environments and both outgoing residences
        // checked. Sibling residences in the caller are not changed or observed.
        state.take(root, &[], self.budget)?;
        for (i, result) in outputs.into_iter().enumerate() {
            state.install(
                result,
                Root::Temporary(node),
                &[ResultComponent::TupleElement(i)],
                self.budget,
            )?;
        }
        Ok(true)
    }

    fn same_job(&mut self, left: &Type<'g>, right: &Type<'g>, depth: usize) -> Work<bool> {
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
        self.result_agrees(a, b, depth)
    }

    // Only primitive leaves and closed products of those leaves can be
    // forwarded by this slice. No assumption on nominal or erased storage.
    fn result_agrees(&mut self, left: &Type<'g>, right: &Type<'g>, depth: usize) -> Work<bool> {
        self.depth(depth)?;
        match (left.as_ref(), right.as_ref()) {
            (TypeTerm::Primitive(a), TypeTerm::Primitive(b)) => {
                Ok(a == b && !matches!(a.name(), "void" | "never"))
            }
            (TypeTerm::Tuple(a), TypeTerm::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    if !self.result_agrees(a, b, depth + 1)? {
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
        stored: &[Located<'g>],
        slot: ResultComponent<'g>,
        caller: &Residence<'g>,
        depth: usize,
    ) -> Work<Option<Vec<Located<'g>>>> {
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
                if arrow.mode == Some(InvocationMode::ReadOnly)
                    && capture.reachable
                    && capture.through.is_none()
                    && capture.form == (CaptureForm::ExplicitBorrow { mutable: false })
                    && capture.path.projections.is_empty() =>
            {
                Some(capture)
            }
            _ => return Ok(None),
        };
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
                        && capture.is_some_and(|c| c.path.root == *root) =>
                {
                    alias = true
                }
                _ => return Ok(None),
            }
        }
        if !callable || marker != capture.is_some() || alias != capture.is_some() {
            return Ok(None);
        }
        let list = if let Some(capture) = capture {
            let Some(TypeTerm::Reference {
                mutable: false,
                referent,
            }) = capture.storage.as_deref()
            else {
                return Ok(None);
            };
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
        if !self.closed_child_expression(body, list, depth + 1)? {
            return Ok(None);
        }
        let Some(value) = self.expression_facts(body)? else {
            return Ok(None);
        };
        if !self.result_agrees(&arrow.result, &value.ty, depth + 1)? {
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
        .then_some(result))
    }

    fn closed_child_expression(
        &mut self,
        node: NodeId,
        list: Option<(&'g DeclarationId, NodeId)>,
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
        let node = self.ungroup_value(node)?;
        let Some(facts) = self.facts(node)? else {
            return Ok(false);
        };
        let NodeFactKind::Expression(value) = &facts.kind else {
            return Ok(false);
        };
        if value.ordinary_context.is_some() || value.call.is_some() || !facts.flow.normal {
            return Ok(false);
        }
        match &value.data {
            Some(DataExpression::Tuple { elements }) => {
                for &element in elements {
                    if !self.closed_child_expression(element, list, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Some(DataExpression::ListIndex {
                declaration,
                base,
                selector,
            }) => Ok(list == Some((*declaration, self.ungroup_value(*base)?))
                && self.module.syntax.nodes[*selector].kind == NodeKind::Literal
                && matches!(value.ty.as_ref(), TypeTerm::Primitive(_))),
            None => Ok(self.module.syntax.nodes[node].kind == NodeKind::Literal
                && matches!(value.ty.as_ref(), TypeTerm::Primitive(_))),
            _ => Ok(false),
        }
    }
}
