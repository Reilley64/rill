//! Use established endpoints from the shared expression worklist, never from candidates.
use super::*;
use crate::analysis::conversions::{
    ErrorAgreement, PairBlocker, PairOutcome, RoleHypothesis, pairs,
};
use ordinary::{InputPremise, shape};

const PREMISE: InputPremise = InputPremise::HypotheticalConversionRole;

#[derive(Debug)]
pub struct ConversionUse<'g> {
    pub site: Site<'g>,
    pub family: &'g DeclarationId,
    pub role: RoleHypothesis<'g>,
    pub premise: InputPremise,
    pub routes: Vec<&'g Binding>,
    pub destination: Option<Type<'g>>,
    pub actual_source: Option<Type<'g>>,
    pub source_filter: Option<Type<'g>>,
    pub goal: Option<GoalId>,
    pub outcome: Option<PairOutcome<'g>>,
    pub blockers: Vec<PairBlocker>,
    pub selected_parameter: Option<Type<'g>>,
    pub selected_result: Option<Type<'g>>,
    pub error_agreement: ErrorAgreement<'g>,
    pub result_agreement: Option<bool>,
    pub descriptor: Option<Type<'g>>,
    pub descriptor_agreement: Option<bool>,
}

struct UseContext<'a, 'g> {
    site: Site<'g>,
    family: &'g DeclarationId,
    routes: &'a [&'g Binding],
    written: &'a [Substitution<'g>],
    premise: Option<InputPremise>,
    source: Option<Type<'g>>,
    result: Option<Type<'g>>,
    receiving_callable: Option<Type<'g>>,
    value: bool,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn conversion_call_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        finalizing: bool,
    ) -> Work<()> {
        let call = inference.calls.remove(&node).unwrap();
        let module = inference.module;
        let argument = match call.fact.arguments.as_slice() {
            [argument] => Some(*argument),
            _ => None,
        };
        if let Some(argument) = argument
            && let Some(filter) = call.fact.substitution.get(1)
            && inference.slots[&argument].ty.is_none()
            && inference.slots[&argument].expected.is_none()
        {
            self.conditional_context(inference, argument, filter.argument.clone(), PREMISE)?;
        }
        let source = argument
            .and_then(|argument| inference.slots[&argument].ty.clone())
            .filter(|t| primitive_name(t) != Some("never"));
        let use_ = self.conversion_use(
            UseContext {
                site: Self::site(module, node),
                family: call.fact.callee,
                routes: &call.fact.bindings,
                written: &call.fact.substitution,
                premise: match (
                    inference.slots[&node].ordinary_context,
                    argument.and_then(|argument| inference.slots[&argument].ordinary_context),
                ) {
                    (Some(a), Some(b)) => Some(a.combine(b)),
                    (a, b) => a.or(b),
                },
                source,
                result: inference.slots[&node].expected.clone(),
                receiving_callable: None,
                value: false,
            },
            finalizing,
        )?;
        if let Some(use_) = use_ {
            if let Some(result) = &use_.selected_result
                && !matches!(use_.error_agreement, ErrorAgreement::Disagrees { .. })
                && use_.result_agreement != Some(false)
            {
                self.conversion_type(inference, node, result.clone(), use_.premise)?;
            } else {
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
            self.elab.reserve(1)?;
            self.conversion_uses.insert((&module.key, node), use_);
        } else {
            self.elab.reserve(1)?;
            inference.calls.insert(node, call);
        }
        Ok(())
    }

    pub(super) fn conversion_value_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        written: &FunctionValueFacts<'g>,
        finalizing: bool,
    ) -> Work<bool> {
        let expected = inference.slots[&node].expected.clone();
        let projected = inference.callable_contexts.get(&node);
        let (source, result) = match expected.as_deref() {
            Some(TypeTerm::Callable {
                parameters, result, ..
            }) if parameters.len() == 1 => (Some(parameters[0].clone()), Some(result.clone())),
            _ => projected.map_or((None, None), |c| {
                (c.parameters.first().cloned().flatten(), c.result.clone())
            }),
        };
        let use_ = self.conversion_use(
            UseContext {
                site: Self::site(inference.module, node),
                family: written.declaration,
                routes: &written.bindings,
                written: &written.substitution,
                premise: inference.slots[&node].ordinary_context,
                source: source.or_else(|| written.substitution.get(1).map(|s| s.argument.clone())),
                result,
                receiving_callable: expected,
                value: true,
            },
            finalizing,
        )?;
        let Some(use_) = use_ else {
            return Ok(false);
        };
        if let Some(descriptor) = &use_.descriptor
            && use_.descriptor_agreement != Some(false)
            && !matches!(use_.error_agreement, ErrorAgreement::Disagrees { .. })
        {
            self.conversion_type(inference, node, descriptor.clone(), use_.premise)?;
        } else {
            inference.slots.get_mut(&node).unwrap().blocked = true;
        }
        self.elab.reserve(1)?;
        self.conversion_uses
            .insert((&inference.module.key, node), use_);
        Ok(true)
    }

    pub(in crate::analysis::scalars) fn finish_conversion_uses(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<()> {
        if self.conversions.is_none() {
            return Ok(());
        }
        for &node in &inference.conversion_clause_calls {
            self.elab.step(1)?;
            let slot = inference.slots.get_mut(&node).unwrap();
            if slot.ty.is_none() {
                slot.blocked = true;
            }
        }
        let count = inference.calls.len() + inference.function_values.len();
        self.elab.step(count)?;
        self.elab.reserve(count)?;
        let calls: Vec<_> = inference
            .calls
            .iter()
            .filter_map(|(&node, c)| self.conversion_family(c.fact.callee).map(|_| node))
            .collect();
        for node in calls {
            self.with_node_premise(inference, node, |checker, inference| {
                checker.conversion_call_step(inference, node, true)
            })?;
        }
        let values: Vec<_> = inference
            .function_values
            .iter()
            .filter_map(|(&node, v)| self.conversion_family(v.declaration).map(|_| node))
            .collect();
        for node in values {
            let written = inference.function_values.remove(&node).unwrap();
            self.with_node_premise(inference, node, |checker, inference| {
                checker.conversion_value_step(inference, node, &written, true)
            })?;
        }
        Ok(())
    }

    fn conversion_type(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        ty: Type<'g>,
        premise: InputPremise,
    ) -> Work<()> {
        // Send the type to waiting equations without qualifying unrelated siblings.
        self.mark_conversion_dependency(inference, node, premise, true)?;
        self.establish(inference, node, ty, false)
    }

    fn conversion_use(
        &mut self,
        context: UseContext<'_, 'g>,
        finalizing: bool,
    ) -> Work<Option<ConversionUse<'g>>> {
        let UseContext {
            site,
            family,
            routes,
            written,
            premise,
            source,
            result,
            receiving_callable,
            value,
        } = context;
        let role = self.conversion_family(family).unwrap().premise;
        let (_, result_role) = role.bindings();
        let fallible = result_role.is_some();
        let (expected_result, required_error) = match (result_role, result.as_deref()) {
            (
                Some(binding),
                Some(TypeTerm::Nominal {
                    declaration,
                    arguments,
                }),
            ) if matches!(&binding.target, Target::Declaration(id) if id == *declaration) => {
                (arguments.first(), arguments.get(1))
            }
            (None, _) => (result.as_ref(), None),
            _ => (None, None),
        };
        let destination = if let Some(written) = written.first() {
            Some(written.argument.clone())
        } else if let Some(expected) = expected_result
            && primitive_name(expected) != Some("never")
            && !self.conversion_symbolic(expected, 1)?
        {
            Some(expected.clone())
        } else {
            None
        };
        if !finalizing && (destination.is_none() || source.is_none()) {
            return Ok(None);
        }
        self.elab.reserve(routes.len())?;
        let source_filter = written.get(1).map(|s| s.argument.clone());
        let premise = [self.arrow_premise, premise]
            .into_iter()
            .flatten()
            .fold(PREMISE, InputPremise::combine);
        let role_blockers = pairs::role_blockers(role);
        self.elab.reserve(role_blockers.len() + 1)?;
        let mut facts = ConversionUse {
            site,
            family,
            role,
            premise,
            routes: routes.to_vec(),
            destination,
            actual_source: source,
            source_filter,
            goal: None,
            outcome: None,
            blockers: role_blockers.to_vec(),
            selected_parameter: None,
            selected_result: None,
            error_agreement: if required_error.is_some() || (fallible && written.get(2).is_some()) {
                ErrorAgreement::NotDetermined
            } else {
                ErrorAgreement::NotRequested
            },
            result_agreement: None,
            descriptor: None,
            descriptor_agreement: None,
        };
        let (Some(to), Some(from)) = (&facts.destination, &facts.actual_source) else {
            facts.blockers.push(PairBlocker::EndpointContext);
            return Ok(Some(facts));
        };
        let origin = self.evidence_module(&family.module)?;
        let header = self
            .input
            .ordinary
            .unwrap()
            .captured_family(family)
            .unwrap();
        let signature = origin.syntax.nodes[header.header.declaration.node].children[0];
        self.elab.reserve(2 + usize::from(written.len() > 1))?;
        let filters = written.get(1).cloned().into_iter().collect();
        let goal = self.issue_determination(
            GoalJudgment::Determine {
                family,
                inputs: vec![to.clone(), from.clone()],
                filters,
            },
            site,
            Self::site(origin, signature),
            routes,
            None,
        )?;
        self.add_goal_premise(&goal, premise);
        self.solve_goal(&goal)?;
        let proof = self.evidence.get(&goal).unwrap();
        facts.premise = proof.premise.unwrap_or(PREMISE);
        facts.outcome = proof.conversion_outcome;
        if let GoalState::Unresolved(GoalBlocker::ConversionPair(reason)) = proof.state {
            facts.blockers.push(reason);
        } else if matches!(facts.outcome, Some(PairOutcome::RequiredIdentity { .. })) {
            facts.blockers.push(PairBlocker::RequiredIdentityProvider);
        }
        facts.goal = Some(goal.clone());
        let Some(output) = &proof.output else {
            return Ok(Some(facts));
        };
        let actual_site = output.site;
        let result_ty = output.result.clone();
        facts.selected_parameter = proof.conversion_parameter.clone();
        facts.selected_result = Some(result_ty.clone());
        if fallible {
            let actual = output.arguments[2].clone();
            let required = written
                .get(2)
                .map(|s| &s.argument)
                .into_iter()
                .chain(required_error);
            for ty in required {
                if self.record_conversion_equality(
                    &goal,
                    Some(2),
                    ty.clone(),
                    actual.clone(),
                    site,
                    actual_site,
                )? {
                    if matches!(facts.error_agreement, ErrorAgreement::NotDetermined) {
                        facts.error_agreement = ErrorAgreement::Agrees;
                    }
                } else {
                    facts.error_agreement = ErrorAgreement::Disagrees {
                        actual: actual.clone(),
                        required: ty.clone(),
                    };
                }
            }
        }
        if let Some(expected) = &result {
            facts.result_agreement = Some(self.record_conversion_equality(
                &goal,
                None,
                expected.clone(),
                result_ty.clone(),
                site,
                actual_site,
            )?);
        }
        if value && let Some(parameter) = &facts.selected_parameter {
            self.elab.reserve(2)?;
            let descriptor = Arc::new(TypeTerm::Callable {
                mode: InvocationMode::ReadOnly,
                parameters: vec![parameter.clone()],
                result: result_ty,
            });
            facts.descriptor_agreement = if let Some(expected) = receiving_callable {
                Some(
                    self.same(&descriptor, &expected)?
                        || self.callable_admission(&descriptor, &expected)?,
                )
            } else {
                facts.result_agreement
            };
            facts.descriptor = Some(descriptor);
        }
        Ok(Some(facts))
    }

    pub(super) fn conversion_symbolic(&mut self, ty: &Type<'g>, depth: usize) -> Work<bool> {
        self.equation_depth(depth)?;
        if matches!(ty.as_ref(), TypeTerm::Parameter { .. }) {
            return Ok(true);
        }
        for child in shape(ty).1 {
            if self.conversion_symbolic(child, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
