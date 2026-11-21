//! Shared context, selection and evidence for ordinary family uses.
use super::*;
use declarations::Budget;
use ordinary::source::{self, SourceCandidate};
use ordinary::{HypotheticalOutcome, InputPremise, SelectionBlocker};

const PREMISE: InputPremise = InputPremise::HypotheticalOrdinaryRole;

#[derive(Debug)]
pub struct ConditionalFinding<'g> {
    pub premise: InputPremise,
    pub finding: ScalarFinding<'g>,
}

#[derive(Debug)]
pub struct OrdinaryCandidate<'g> {
    pub implementation: &'g DeclarationId,
    pub alternative: usize,
    pub covers_observed_inputs: bool,
    pub substitution: Vec<Substitution<'g>>,
}

#[derive(Debug)]
pub enum OutputAgreement<'g> {
    NotDetermined,
    Agrees,
    Disagrees {
        actual: Type<'g>,
        required: Type<'g>,
    },
}

#[derive(Debug)]
pub struct OrdinaryCall<'g> {
    pub site: Site<'g>,
    pub family: &'g DeclarationId,
    pub checked: bool,
    checked_local_disposer: bool,
    /// A premise retained by the comparison or its dependent evidence.
    pub premise: Option<InputPremise>,
    pub routes: Vec<&'g Binding>,
    pub inputs: Vec<Option<Type<'g>>>,
    pub receiving: Vec<Option<Type<'g>>>,
    pub callbacks: Vec<OrdinaryCallbackContext<'g>>,
    pub explicit: Vec<Substitution<'g>>,
    pub defaults: Vec<DefaultArgumentUse<'g>>,
    pub candidates: Vec<OrdinaryCandidate<'g>>,
    pub blocker: Option<SelectionBlocker>,
    pub outcome: Option<HypotheticalOutcome<'g>>,
    pub goal: Option<GoalId>,
    /// Proof of the selected contract after filling free slots at this source use.
    pub instantiation: Option<GoalId>,
    /// Selected requirements formed after a genuinely free output was instantiated at this use.
    pub instantiated_requirements: Vec<GoalId>,
    /// Type from the selected header alone, not a checked result or evidence.
    pub selected_result: Option<Type<'g>>,
    pub output_agreement: OutputAgreement<'g>,
}

pub(in crate::analysis::scalars) struct SourceSelection<'g> {
    winner: GoalSelection<'g>,
    argument_pair: Option<evidence::ArgumentPair<'g>>,
}

struct CallablePath {
    column: usize,
    tuple_indices: Vec<usize>,
    node: NodeId,
}

#[derive(Debug)]
pub struct OrdinaryValue<'g> {
    pub goal: GoalId,
    pub instantiation: Option<GoalId>,
    pub output_agreement: OutputAgreement<'g>,
}

type SelectedSlots<'r, 'g> = (
    &'r ordinary::ImplementationFacts<'g>,
    usize,
    Vec<Option<Type<'g>>>,
);

struct ConditionalOutput<'g> {
    goal: Option<GoalId>,
    instantiation: Option<GoalId>,
    ty: Option<Type<'g>>,
    agreement: OutputAgreement<'g>,
    defaults: Vec<DefaultArgumentUse<'g>>,
    instantiated_requirements: Vec<GoalId>,
}

impl Default for ConditionalOutput<'_> {
    fn default() -> Self {
        Self {
            goal: None,
            instantiation: None,
            ty: None,
            agreement: OutputAgreement::NotDetermined,
            defaults: Vec::new(),
            instantiated_requirements: Vec::new(),
        }
    }
}

#[derive(PartialEq, Eq)]
enum ScopedRetention {
    Ordinary,
    Retain,
    Refresh,
}

struct Comparison<'r, 'g> {
    viable: Vec<SourceCandidate<'r, 'g>>,
    outcome: Option<HypotheticalOutcome<'g>>,
    blocker: Option<SelectionBlocker>,
}

fn written_arguments<'a, 'g>(call: &'a PreparedCall<'g>) -> &'a [Substitution<'g>] {
    &call.fact.substitution[..call.fact.substitution.len() - call.fact.defaults.len()]
}

fn parameter<'a, 'g>(candidate: &'a SourceCandidate<'_, 'g>, column: usize) -> &'a Type<'g> {
    let DeclarationShape::Function { parameters, .. } = &candidate.body.header.shape else {
        unreachable!()
    };
    &parameters[column].ty
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn has_installed_ordinary_scope(&self, family: &DeclarationId) -> bool {
        self.ordinary_scope
            .as_ref()
            .is_some_and(|scope| scope.contains(family))
    }

    fn ordinary_use_premise(&self, family: &DeclarationId) -> Option<InputPremise> {
        if self.has_installed_ordinary_scope(family) {
            self.arrow_premise
        } else {
            Some(self.arrow_premise.map_or(PREMISE, |p| p.combine(PREMISE)))
        }
    }

    // Cheap prefilter only. Callers charge for and compare the full IDs
    // before interpreting operand premises or stamping a determination.
    fn possible_scoped_winner(
        &self,
        call: &PreparedCall<'g>,
        outcome: Option<HypotheticalOutcome<'g>>,
    ) -> Option<(&'g DeclarationId, &'g DeclarationId)> {
        let role = self.ordinary_scope.as_ref()?.routine()?;
        let HypotheticalOutcome::Winner(winner) = outcome? else {
            return None;
        };
        let installed = &role.implementation.id;
        if !std::ptr::eq(call.fact.callee, &role.family.id)
            || winner.module.origin != installed.module.origin
            || winner.start != installed.start
            || call.fact.pipe_input.is_some()
        {
            return None;
        }
        Some((winner, installed))
    }

    fn scoped_result_candidate(
        &self,
        call: &PreparedCall<'g>,
        outcome: Option<HypotheticalOutcome<'g>>,
        proof: Option<&GoalFacts<'g>>,
    ) -> Option<(&'g DeclarationId, &'g DeclarationId)> {
        let (winner, installed) = self.possible_scoped_winner(call, outcome)?;
        let goal = proof?;
        if !matches!(goal.judgment, GoalJudgment::Determine { .. })
            || !goal
                .selection
                .as_ref()
                .is_some_and(|s| std::ptr::eq(s.implementation, winner))
        {
            return None;
        }
        let TypeTerm::Tuple(outputs) = goal.output.as_ref()?.result.as_ref() else {
            return None;
        };
        let [left, right] = outputs.as_slice() else {
            return None;
        };
        self.scoped_result_outputs(left, right)
            .then_some((winner, installed))
    }

    fn scoped_result_outputs(&self, left: &Type<'g>, right: &Type<'g>) -> bool {
        let Some(result) = self.result.filter(|role| role.representation.is_some()) else {
            return false;
        };
        let canonical_result = |ty: &Type<'g>| {
            matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
                if std::ptr::eq(*declaration, &result.declaration.id)
                    && matches!(arguments.as_slice(), [ok, err]
                        if primitive_name(ok) == Some("bool") && primitive_name(err) == Some("i32")))
        };
        primitive_name(right) == Some("bool") && canonical_result(left)
            || primitive_name(left) == Some("bool") && canonical_result(right)
    }

    fn scoped_result_operand_premise(
        &mut self,
        inference: &Inference<'g>,
        call: &PreparedCall<'g>,
        inputs: &[Option<Type<'g>>],
        outcome: Option<HypotheticalOutcome<'g>>,
    ) -> Work<Option<InputPremise>> {
        let Some((winner, installed)) = self.possible_scoped_winner(call, outcome) else {
            return Ok(None);
        };
        let [Some(input)] = inputs else {
            return Ok(None);
        };
        let TypeTerm::Tuple(jobs) = input.as_ref() else {
            return Ok(None);
        };
        let [a, b] = jobs.as_slice() else {
            return Ok(None);
        };
        let (TypeTerm::Callable { result: left, .. }, TypeTerm::Callable { result: right, .. }) =
            (a.as_ref(), b.as_ref())
        else {
            return Ok(None);
        };
        if !self.scoped_result_outputs(left, right) {
            return Ok(None);
        }
        self.elab.step(1 + installed.module.path.len())?;
        if winner != installed {
            return Ok(None);
        }
        Ok(self.scoped_pair_context(inference, call)?.1)
    }

    fn retain_scoped_result_call(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        slot: &Slot<'g>,
    ) -> Work<ScopedRetention> {
        // The stamp lives in the expression slot already read by the caller.
        // No other ordinary call needs to look up an earlier determination.
        let levels = 1 + self.ordinary_calls.len().max(1).ilog2() as usize;
        let lookup = levels
            .checked_mul(1 + inference.module.key.path.len())
            .ok_or(())?;
        self.elab.step(lookup.checked_add(2).ok_or(())?)?;
        let prior = &self.ordinary_calls[&(&inference.module.key, node)];
        let call = &inference.calls[&node];
        let id = prior.goal.as_ref().unwrap();
        let goal = self.evidence.get(id).unwrap();
        let output = goal.output.as_ref().unwrap();
        // Ordinary fallback may still reuse the prior goal to compare an
        // expected output. It is not sufficient when the proof context changes.
        if prior.premise.is_some()
            || slot.ordinary_context.is_some()
            || self.arrow_premise.is_some()
            || !prior.defaults.is_empty()
            || !call.fact.defaults.is_empty()
            || prior.instantiation.is_some()
            || slot.blocked
            || !matches!(
                goal.state,
                GoalState::Proved | GoalState::Unresolved(GoalBlocker::CallableEnvironment)
            )
        {
            return Ok(ScopedRetention::Refresh);
        }
        let ([argument], [formal], [Some(input)]) = (
            call.fact.arguments.as_slice(),
            call.parameters.as_slice(),
            prior.inputs.as_slice(),
        ) else {
            return Ok(ScopedRetention::Refresh);
        };
        let GoalJudgment::Determine { inputs, .. } = &goal.judgment else {
            unreachable!();
        };
        let [determined_input] = inputs.as_slice() else {
            return Ok(ScopedRetention::Refresh);
        };
        let written = written_arguments(call);
        self.elab.step(1 + goal.assumptions.len() + written.len())?;
        let actual = &inference.slots[argument];
        if actual.blocked
            || actual.ordinary_context.is_some()
            || !self.evidence.uses_current_assumptions(goal)
            || !Arc::ptr_eq(determined_input, input)
            || !actual.ty.as_ref().is_some_and(|ty| Arc::ptr_eq(ty, input))
            || written.len() != prior.explicit.len()
            || !written.iter().zip(&prior.explicit).all(|(now, was)| {
                now.position == was.position
                    && std::ptr::eq(now.binder, was.binder)
                    && Arc::ptr_eq(&now.argument, &was.argument)
            })
        {
            return Ok(ScopedRetention::Refresh);
        }
        // The same input and written filters preserve the ordinary rank.
        // Immutable argument syntax fixes the occurrences, but their arrow
        // evidence must still be compared in order.
        let old_origins = goal.argument_pair.clone();
        let erased = old_origins.is_none();
        let result = output.result.clone();
        let state = goal.state;
        let agrees = matches!(prior.output_agreement, OutputAgreement::Agrees);
        let checked = prior.checked;
        let (settled, premise) = self.scoped_pair_context(inference, call)?;
        if !settled || premise.is_some() {
            return Ok(ScopedRetention::Refresh);
        }
        let current = self
            .direct_argument_pair(inference, call, formal)?
            .unwrap_or_default();
        self.elab.step(2)?;
        let same_origins = match old_origins {
            Some(old) => old.iter().zip(&current).all(|(old, now)| match (old, now) {
                (Some(old), Some(now)) => Arc::ptr_eq(old, now),
                (None, None) => true,
                _ => false,
            }),
            None => current.iter().all(Option::is_none),
        };
        if !same_origins {
            return Ok(ScopedRetention::Refresh);
        }
        if (erased && state != GoalState::Unresolved(GoalBlocker::CallableEnvironment))
            || !agrees
            || match &slot.ty {
                Some(ty) => !checked || !Arc::ptr_eq(ty, &result),
                None => checked,
            }
        {
            return Ok(ScopedRetention::Ordinary);
        }
        // Expected output is not a ranking input. On disagreement let the
        // ordinary path compare it against the existing determination.
        let agrees = slot
            .expected
            .as_ref()
            .map_or(Ok(true), |expected| self.same(expected, &result))?;
        Ok(if agrees {
            ScopedRetention::Retain
        } else {
            ScopedRetention::Ordinary
        })
    }

    pub(in crate::analysis::scalars) fn ordinary_call_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        default_step: bool,
    ) -> Work<bool> {
        let slot = &inference.slots[&node];
        if matches!(
            slot.installed_operation,
            Some(data::DataExpression::NativeOwnership(_) | data::DataExpression::CallableCopy(_))
        ) {
            return Ok(false);
        }
        let slot_had_scoped_determination = slot.scoped_result_determination;
        let retention = if !default_step && slot_had_scoped_determination {
            self.retain_scoped_result_call(inference, node, slot)?
        } else {
            ScopedRetention::Ordinary
        };
        if retention == ScopedRetention::Retain {
            return Ok(false);
        }
        let mut call = inference.calls.remove(&node).unwrap();
        let module = inference.module;
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap();
        let binders = &family.header.binders;
        self.elab.reserve(binders.len())?;
        let mut slots = vec![None; binders.len()];
        for sub in &call.fact.substitution {
            slots[sub.position] = Some(sub.argument.clone());
        }
        self.elab.reserve(call.parameters.len())?;
        let mut inputs = Vec::with_capacity(call.parameters.len());
        let mut consistent = self.source_equations(inference, &call, binders, &mut slots)?;
        for argument in &call.fact.arguments {
            self.elab.step(1)?;
            let actual = inference.slots[argument]
                .ty
                .as_ref()
                .filter(|t| primitive_name(t) != Some("never"));
            inputs.push(actual.cloned());
        }
        let substitutions = self.slot_substitutions(binders, &slots)?;
        if default_step {
            let mut applied = false;
            if consistent && slots.iter().any(Option::is_none) {
                let mut budget = Budget {
                    limits: self.elab.limits,
                    statistics: self.elab.statistics,
                };
                let positions = source::input_positions(family, &mut budget);
                self.elab.statistics = budget.statistics;
                applied = self.try_call_default(
                    inference,
                    node,
                    &mut call,
                    binders,
                    &slots,
                    &substitutions,
                    Some(&positions?),
                )?;
            }
            self.elab.reserve(1)?;
            inference.calls.insert(node, call);
            if applied {
                self.enqueue(inference, node)?;
            }
            return Ok(applied);
        }
        self.elab.reserve(call.parameters.len())?;
        let mut receiving = Vec::with_capacity(call.parameters.len());
        for (i, parameter) in call.parameters.iter().enumerate() {
            let context =
                if consistent && self.closed(parameter, binders, &|i| slots[i].is_some(), 1)? {
                    Some(self.substitute(parameter, &substitutions, 1)?)
                } else {
                    None
                };
            if inputs[i].is_none() && inference.slots[&call.fact.arguments[i]].ty.is_some() {
                // Bare never does not solve a slot or narrow a pattern. An
                // independent declared receiving type can fill this contract.
                inputs[i] = context.clone();
            }
            receiving.push(context);
        }

        let mut budget = Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let compared = self.compare_ordinary(&call, &inputs, &mut budget);
        self.elab.statistics = budget.statistics;
        let Comparison {
            mut viable,
            mut outcome,
            blocker,
        } = compared?;
        if consistent
            && viable.is_empty()
            && self.known_callable_copy(inference, node, &mut call, &inputs)?
        {
            self.elab.reserve(1)?;
            inference.calls.insert(node, call);
            return Ok(false);
        }
        let mut callbacks = Vec::new();
        if !consistent {
            outcome = None;
        }
        if consistent {
            if let Some(paths) =
                self.commit_brackets(inference, &call, &substitutions, &mut viable)?
            {
                for path in &paths {
                    self.append_callback(
                        inference,
                        &call,
                        &substitutions,
                        &viable,
                        path,
                        &mut callbacks,
                    )?;
                }
            } else {
                consistent = false;
                outcome = None;
            }
        }
        for context in &callbacks {
            let leaf = self.source_leaf(inference.module, context.argument)?;
            if inference.slots[&leaf].ty.is_some() || inference.slots[&leaf].blocked {
                continue;
            }
            if let Some(premise) = self.ordinary_use_premise(call.fact.callee) {
                let slot = inference.slots.get_mut(&leaf).unwrap();
                slot.ordinary_context = Some(
                    slot.ordinary_context
                        .map_or(premise, |p| p.combine(premise)),
                );
            }
            self.elab.reserve(context.parameters.len())?;
            self.project_callable_context(
                inference,
                leaf,
                callables::CallableContext {
                    parameters: context.parameters.clone(),
                    result: context.result.clone(),
                },
            )?;
        }
        for i in 0..receiving.len() {
            if inputs[i].is_some() || !consistent {
                continue;
            }
            if receiving[i].is_none() && blocker.is_none() {
                receiving[i] = self.agreed_path_receiving(
                    &viable,
                    &CallablePath {
                        column: i,
                        tuple_indices: Vec::new(),
                        node: call.fact.arguments[i],
                    },
                )?;
            }
            if let Some(ty) = receiving[i].clone() {
                let argument = call.fact.arguments[i];
                let leaf = self.source_leaf(module, argument)?;
                // A named operand has its own type even through grouping.
                // Do not give it the call's hypothetical receiving premise
                // before its independent type reaches the containing group.
                if module.syntax.nodes[leaf].kind == NodeKind::ValueReference
                    && self.local_disposer_subject(call.fact.callee, &ty)?
                {
                    continue;
                }
                // A named borrow of a known callable does not get its type from
                // the explicit copy argument. Let its independent type arrive.
                if module.syntax.nodes[leaf].kind == NodeKind::Prefix("&")
                    && matches!(ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if matches!(referent.as_ref(), TypeTerm::Callable { .. }))
                    && self
                        .primitives
                        .and_then(|core| core.copy)
                        .is_some_and(|copy| std::ptr::eq(&copy.id, call.fact.callee))
                {
                    let owner = self.source_leaf(module, module.syntax.nodes[leaf].children[0])?;
                    if module.syntax.nodes[owner].kind == NodeKind::ValueReference
                        && inference.slots[&owner].ordinary_context.is_none()
                    {
                        let TypeTerm::Reference { referent, .. } = ty.as_ref() else {
                            unreachable!()
                        };
                        if let Some(actual) = &inference.slots[&owner].ty
                            && self.same(actual, referent)?
                        {
                            continue;
                        }
                    }
                }
                if !inference.ordinary_tuples.contains(&leaf) {
                    // List get has a width context only after the receiver is
                    // known and ordinary context agreement establishes u64.
                    let proven_context = matches!(inputs.as_slice(), [Some(source), None]
                        if i == 1 && self.collections.scope(call.fact.callee, &[Some(source), Some(&ty)])
                            .and_then(|scope| scope.winner()).is_some());
                    if proven_context && self.arrow_premise.is_none() {
                        self.establish(inference, argument, ty, true)?;
                    } else {
                        self.ordinary_context(inference, argument, ty, call.fact.callee)?;
                    }
                }
            }
        }
        if blocker.is_none()
            && let Some(HypotheticalOutcome::Winner(winner)) = outcome
            && let Some((operation, result)) =
                self.native_call(inference, node, &call, winner, &viable)?
        {
            self.elab.reserve(1)?;
            call.fact.substitution = substitutions;
            inference.slots.get_mut(&node).unwrap().installed_operation = Some(operation);
            inference.calls.insert(node, call);
            self.establish(inference, node, result, false)?;
            return Ok(false);
        }
        let eligible_local_disposer = matches!(inputs.as_slice(), [Some(subject)]
            if self.local_disposer_subject(call.fact.callee, subject)?);
        let (output, use_premise) = if consistent {
            let prior = self.arrow_premise;
            let scope = match inputs.as_slice() {
                [source] => self.collections.scope(call.fact.callee, &[source.as_ref()]),
                [source, key] => self
                    .collections
                    .scope(call.fact.callee, &[source.as_ref(), key.as_ref()]),
                _ => None,
            };
            let result = (|| {
                if self.has_installed_ordinary_scope(call.fact.callee)
                    || eligible_local_disposer
                    || scope.and_then(|scope| scope.winner()).is_some()
                {
                    // Admission of the callee cannot settle premises of its operands.
                    for operand in call.fact.arguments.iter().copied().chain([node]) {
                        self.elab.step(1)?;
                        if let Some(premise) = inference.slots[&operand].ordinary_context {
                            self.arrow_premise =
                                Some(self.arrow_premise.map_or(premise, |p| p.combine(premise)));
                        }
                    }
                }
                if let Some(p) =
                    self.scoped_result_operand_premise(inference, &call, &inputs, outcome)?
                {
                    self.arrow_premise = Some(self.arrow_premise.map_or(p, |old| old.combine(p)));
                }
                let output = self.conditional_output(
                    inference,
                    node,
                    &call,
                    &inputs,
                    (&viable, outcome),
                    retention,
                )?;
                let premise = if eligible_local_disposer {
                    self.arrow_premise
                } else {
                    self.ordinary_use_premise(call.fact.callee)
                };
                Ok((output, premise))
            })();
            self.arrow_premise = prior;
            result?
        } else {
            (
                ConditionalOutput::default(),
                self.ordinary_use_premise(call.fact.callee),
            )
        };
        self.elab.reserve(
            1 + call.fact.bindings.len()
                + call.fact.substitution.len()
                + viable.len()
                + output.defaults.len(),
        )?;
        let mut defaults = call.fact.defaults.clone();
        defaults.extend(output.defaults);
        let proof = output
            .instantiation
            .as_ref()
            .or(output.goal.as_ref())
            .map(|id| self.evidence.get(id).unwrap());
        let scoped_candidate = self.scoped_result_candidate(&call, outcome, proof);
        let mut premise = use_premise;
        let mut checked = false;
        if let Some(proof) = proof {
            for id in output.goal.iter().chain(&output.instantiation) {
                if let Some(p) = self.evidence.get(id).unwrap().premise {
                    premise = Some(premise.map_or(p, |a| a.combine(p)));
                }
            }
            let actual_scope =
                eligible_local_disposer || self.has_installed_ordinary_scope(call.fact.callee);
            if proof.premise.is_none() && (!actual_scope || premise.is_none()) {
                match (proof.state, &output.agreement, &output.ty) {
                    (GoalState::Proved, OutputAgreement::Agrees, Some(ty)) => {
                        self.establish(inference, node, ty.clone(), false)?;
                        checked =
                            inference.slots[&node].ty.is_some() && !inference.slots[&node].blocked;
                    }
                    (GoalState::Proved, OutputAgreement::Disagrees { actual, required }, _) => {
                        self.mismatch(module, node, actual, required)?;
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                    }
                    (
                        GoalState::Disproved
                        | GoalState::Unresolved(GoalBlocker::MissingAssumption),
                        _,
                        _,
                    ) if actual_scope => {
                        let finding = match outcome {
                            Some(HypotheticalOutcome::NoMatch) => {
                                ScalarFindingKind::NoApplicableImplementation
                            }
                            Some(HypotheticalOutcome::Ambiguous) => {
                                ScalarFindingKind::AmbiguousImplementation
                            }
                            _ => ScalarFindingKind::MissingRequirement,
                        };
                        self.finding(module, node, finding)?;
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                    }
                    _ => (),
                }
            }
        }
        let retainable = if let Some((winner, installed)) = scoped_candidate {
            self.elab.step(1 + installed.module.path.len())?;
            winner == installed
        } else {
            false
        };
        if retainable || slot_had_scoped_determination {
            self.elab.step(1)?;
            inference
                .slots
                .get_mut(&node)
                .unwrap()
                .scoped_result_determination = retainable;
        }
        self.ordinary_calls.insert(
            (&module.key, node),
            OrdinaryCall {
                site: Self::site(module, node),
                family: call.fact.callee,
                checked,
                checked_local_disposer: checked && eligible_local_disposer,
                premise,
                routes: call.fact.bindings.clone(),
                inputs,
                receiving,
                callbacks,
                explicit: written_arguments(&call).to_vec(),
                defaults,
                candidates: viable
                    .into_iter()
                    .map(|c| OrdinaryCandidate {
                        implementation: &c.body.source.declaration,
                        alternative: c.alternative,
                        covers_observed_inputs: c.covers,
                        substitution: c.substitution,
                    })
                    .collect(),
                blocker,
                outcome,
                goal: output.goal,
                instantiation: output.instantiation,
                instantiated_requirements: output.instantiated_requirements,
                selected_result: output.ty,
                output_agreement: output.agreement,
            },
        );
        self.elab.reserve(1)?;
        inference.calls.insert(node, call);
        Ok(false)
    }

    fn native_call(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        call: &PreparedCall<'g>,
        winner: &'g DeclarationId,
        viable: &[SourceCandidate<'r, 'g>],
    ) -> Work<Option<(data::DataExpression<'g>, Type<'g>)>> {
        use crate::analysis::typed::{NativeOwnershipCall, OwnershipCoverage, OwnershipRule};
        if inference.slots[&node].ordinary_context.is_some() || self.arrow_premise.is_some() {
            return Ok(None);
        }
        self.elab
            .step(call.fact.arguments.len() + self.native_ownership.len())?;
        if call.fact.arguments.iter().any(|arg| {
            let slot = &inference.slots[arg];
            slot.blocked || slot.ordinary_context.is_some() || slot.ty.is_none()
        }) {
            return Ok(None);
        }
        let Some((action, provider)) = self.native_ownership.iter().find_map(|scope| {
            let provider = scope.provider.as_ref()?;
            let OwnershipRule::Required(action) = scope.rule else {
                return None;
            };
            (scope.family == call.fact.callee
                && scope.coverage == OwnershipCoverage::Clear
                && provider.implementation.id == *winner)
                .then_some((action, provider))
        }) else {
            return Ok(None);
        };
        self.elab.step(viable.len())?;
        let Some(candidate) = viable
            .iter()
            .find(|candidate| candidate.covers && candidate.body.source.declaration == *winner)
        else {
            return Ok(None);
        };
        let header = &candidate.body.header;
        let DeclarationShape::Function { result, .. } = &header.shape else {
            unreachable!()
        };
        if candidate.substitution.len() != header.binders.len() {
            return Ok(None);
        }
        // The admitted, closed header supplies this result, not the hypothetical
        // output worklist or an assumption from the caller.
        let result = self.substitute(result, &candidate.substitution, 1)?;
        self.elab.reserve(1 + candidate.substitution.len())?;
        let operation = NativeOwnershipCall {
            action,
            implementation: winner,
            entry: &provider.entry.id,
            artifact: provider.artifact,
            substitution: candidate.substitution.clone(),
        };
        Ok(Some((
            data::DataExpression::NativeOwnership(Arc::new(operation)),
            result,
        )))
    }

    fn compare_ordinary(
        &self,
        call: &PreparedCall<'g>,
        inputs: &[Option<Type<'g>>],
        budget: &mut Budget,
    ) -> Work<Comparison<'r, 'g>> {
        let inventory = self.input.ordinary.unwrap();
        let id = call.fact.callee;
        let family = if self.has_installed_ordinary_scope(id) {
            source::admitted_family(inventory, id, budget)?
        } else {
            source::family(inventory, id, budget)?
        };
        let family = match family {
            Ok(family) => family,
            Err(blocker) => {
                return Ok(Comparison {
                    viable: Vec::new(),
                    outcome: None,
                    blocker: Some(blocker),
                });
            }
        };
        let positions = source::input_positions(family, budget)?;
        budget.reserve(call.fact.substitution.len())?;
        let filters: Vec<_> = written_arguments(call)
            .iter()
            .filter(|s| positions[s.position])
            .map(|s| (s.position, &s.argument))
            .collect();
        let viable = source::project(family, inputs, &filters, budget)?;
        budget.step(inputs.len())?;
        let outcome = if inputs.iter().all(Option::is_some) {
            source::rank(family, &viable, budget)?
        } else {
            None
        };
        Ok(Comparison {
            viable,
            outcome,
            blocker: None,
        })
    }

    fn receiver_at<'a>(
        &mut self,
        receiver: &'a Type<'g>,
        substitutions: &'a [Substitution<'g>],
        path: &CallablePath,
    ) -> Work<Option<&'a Type<'g>>> {
        let mut term = receiver;
        for &i in &path.tuple_indices {
            let root = self.receiving_root(term, substitutions)?;
            let TypeTerm::Tuple(fields) = root.as_ref() else {
                return Ok(None);
            };
            let Some(field) = fields.get(i) else {
                return Ok(None);
            };
            term = field;
        }
        Ok(Some(term))
    }

    fn append_callback(
        &mut self,
        inference: &Inference<'g>,
        call: &PreparedCall<'g>,
        declared: &[Substitution<'g>],
        candidates: &[SourceCandidate<'r, 'g>],
        path: &CallablePath,
        contexts: &mut Vec<OrdinaryCallbackContext<'g>>,
    ) -> Work<()> {
        let node = self.source_leaf(inference.module, path.node)?;
        let syntax = &inference.module.syntax.nodes[node];
        if let Some(previous) = inference.callable_contexts.get(&node)
            && (inference.slots[&node].ty.is_some() || inference.slots[&node].blocked)
        {
            self.elab.reserve(1 + previous.parameters.len())?;
            contexts.push(OrdinaryCallbackContext {
                argument: path.node,
                parameters: previous.parameters.clone(),
                result: previous.result.clone(),
            });
            return Ok(());
        }
        if !matches!(syntax.kind, NodeKind::Arrow | NodeKind::ValueReference) {
            return Ok(());
        }
        let header = &self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap()
            .header;
        let written = if let Some(receiver) =
            self.receiver_at(&call.parameters[path.column], declared, path)?
        {
            self.written_callable_context(header, receiver, declared, path.node)?
        } else {
            None
        };
        let mut agreed = None;
        for candidate in candidates {
            let receiver = self.receiver_at(
                parameter(candidate, path.column),
                &candidate.substitution,
                path,
            )?;
            let found = if let Some(receiver) = receiver {
                self.written_callable_context(
                    &candidate.body.header,
                    receiver,
                    &candidate.substitution,
                    path.node,
                )?
            } else {
                None
            };
            let Some(found) = found else {
                agreed = None;
                break;
            };
            if let Some(old) = &mut agreed {
                if !self.intersect_callable_contexts(old, &found)? {
                    agreed = None;
                    break;
                }
            } else {
                agreed = Some(found);
            }
        }
        let context = match (written, agreed) {
            (Some(mut written), Some(agreement))
                if written.parameters.len() == agreement.parameters.len() =>
            {
                self.elab.step(written.parameters.len())?;
                for (needed, available) in written.parameters.iter_mut().zip(agreement.parameters) {
                    if needed.is_none() {
                        *needed = available;
                    }
                }
                if written.result.is_none() {
                    written.result = agreement.result;
                }
                Some(written)
            }
            (written, agreement) => written.or(agreement),
        };
        if let Some(context) = context {
            self.elab.reserve(1)?;
            contexts.push(context);
        }
        Ok(())
    }

    /// Walk only fresh source tuples. A tuple's category is settled before its
    /// arity can narrow the alternatives seen by any of its descendants.
    fn commit_brackets(
        &mut self,
        inference: &mut Inference<'g>,
        call: &PreparedCall<'g>,
        declared: &[Substitution<'g>],
        viable: &mut Vec<SourceCandidate<'r, 'g>>,
    ) -> Work<Option<Vec<CallablePath>>> {
        let module = inference.module;
        let header = &self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap()
            .header;
        self.elab
            .reserve(call.fact.arguments.len() + declared.len())?;
        let bound: BTreeSet<_> = declared.iter().map(|s| s.position).collect();
        let mut pending: Vec<_> = call
            .fact
            .arguments
            .iter()
            .copied()
            .enumerate()
            .rev()
            .map(|(column, node)| CallablePath {
                column,
                node,
                tuple_indices: Vec::new(),
            })
            .collect();
        let mut leaves = Vec::new();
        while let Some(path) = pending.pop() {
            self.equation_depth(1 + path.tuple_indices.len())?;
            let node = self.source_leaf(module, path.node)?;
            let syntax = &module.syntax.nodes[node];
            if inference.slots[&node].blocked {
                return Ok(None);
            }
            if syntax.kind != NodeKind::Bracket {
                self.elab.reserve(1)?;
                leaves.push(path);
                continue;
            }
            let committed = inference.ordinary_tuples.contains(&node);
            if !committed && inference.slots[&node].ty.is_some() {
                continue;
            }
            let written = self.tuple_arity(&call.parameters[path.column], declared, &path)?;
            if !committed && written.is_none() {
                let mut unanimous = !viable.is_empty();
                for candidate in &*viable {
                    unanimous &= self
                        .tuple_arity(
                            parameter(candidate, path.column),
                            &candidate.substitution,
                            &path,
                        )?
                        .is_some();
                }
                if !unanimous {
                    self.elab.reserve(1)?;
                    leaves.push(path);
                    continue;
                }
            }
            if !committed {
                self.elab.reserve(1)?;
                inference.ordinary_tuples.insert(node);
                if let Some(premise) = self.ordinary_use_premise(call.fact.callee) {
                    let slot = inference.slots.get_mut(&node).unwrap();
                    slot.ordinary_context = Some(
                        slot.ordinary_context
                            .map_or(premise, |p| p.combine(premise)),
                    );
                }
                self.enqueue(inference, node)?;
            }
            let count = syntax.children.len();
            let before = std::mem::take(viable);
            let projected = written.is_none() && !before.is_empty();
            self.elab.reserve(before.len())?;
            for candidate in before {
                let arity = self.tuple_arity(
                    parameter(&candidate, path.column),
                    &candidate.substitution,
                    &path,
                )?;
                if arity.is_none_or(|a| a == count) {
                    viable.push(candidate);
                }
            }
            if written.is_some_and(|a| a != count) || (projected && viable.is_empty()) {
                if let Some(premise) = self.ordinary_use_premise(call.fact.callee) {
                    self.conditional_finding(module, node, ScalarFindingKind::Arity, premise)?;
                } else {
                    self.finding(module, node, ScalarFindingKind::Arity)?;
                }
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(None);
            }
            for (position, &child) in syntax.children.iter().enumerate().rev() {
                self.equation_depth(2 + path.tuple_indices.len())?;
                self.elab.reserve(2 + path.tuple_indices.len())?;
                let mut tuple_indices = path.tuple_indices.clone();
                tuple_indices.push(position);
                let child_path = CallablePath {
                    column: path.column,
                    tuple_indices,
                    node: child,
                };
                pending.push(child_path);
            }
        }
        for path in &leaves {
            if path.tuple_indices.is_empty() || inference.slots[&path.node].ty.is_some() {
                continue;
            }
            let written = self.receiver_at(&call.parameters[path.column], declared, path)?;
            let context = if let Some(term) = written
                && self.closed(term, &header.binders, &|i| bound.contains(&i), 1)?
            {
                Some(self.substitute(term, declared, 1)?)
            } else {
                self.agreed_path_receiving(viable, path)?
            };
            if let Some(ty) = context {
                self.ordinary_context(inference, path.node, ty, call.fact.callee)?;
            }
        }
        Ok(Some(leaves))
    }

    fn tuple_arity(
        &mut self,
        receiver: &Type<'g>,
        substitution: &[Substitution<'g>],
        path: &CallablePath,
    ) -> Work<Option<usize>> {
        let Some(term) = self.receiver_at(receiver, substitution, path)? else {
            return Ok(None);
        };
        let ty = self.receiving_root(term, substitution)?;
        Ok(match ty.as_ref() {
            TypeTerm::Tuple(fields) => Some(fields.len()),
            _ => None,
        })
    }

    fn agreed_path_receiving(
        &mut self,
        candidates: &[SourceCandidate<'_, 'g>],
        path: &CallablePath,
    ) -> Work<Option<Type<'g>>> {
        let mut agreed = None;
        for candidate in candidates {
            let Some(field) = self.receiver_at(
                parameter(candidate, path.column),
                &candidate.substitution,
                path,
            )?
            else {
                return Ok(None);
            };
            if !self.candidate_closed(field, candidate)? {
                return Ok(None);
            }
            let proposed = self.substitute(field, &candidate.substitution, 1)?;
            if let Some(ref previous) = agreed
                && !self.same(previous, &proposed)?
            {
                return Ok(None);
            }
            agreed = Some(proposed);
        }
        Ok(agreed)
    }

    fn receiving_root<'a>(
        &mut self,
        ty: &'a Type<'g>,
        substitution: &'a [Substitution<'g>],
    ) -> Work<&'a Type<'g>> {
        self.elab.step(1)?;
        if let TypeTerm::Parameter { declaration, .. } = ty.as_ref() {
            self.elab.step(substitution.len())?;
            if let Some(s) = substitution.iter().find(|s| s.binder == *declaration) {
                return Ok(&s.argument);
            }
        }
        Ok(ty)
    }

    pub(in crate::analysis::scalars) fn gather_ordinary_tuple(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<()> {
        let children = &inference.module.syntax.nodes[node].children;
        self.elab.reserve(children.len())?;
        let mut fields = Vec::with_capacity(children.len());
        for child in children {
            self.elab.step(1)?;
            let slot = &inference.slots[child];
            let Some(ty) = slot.ty.as_ref() else {
                return Ok(());
            };
            let chosen = if let Some(expected) = &slot.expected
                && (primitive_name(ty) == Some("never") || self.data_admission(ty, expected)?)
            {
                expected
            } else {
                ty
            };
            if primitive_name(chosen) == Some("never") {
                return Ok(());
            }
            fields.push(chosen.clone());
        }
        self.elab.reserve(
            children
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(2))
                .ok_or(())?,
        )?;
        let ty = Arc::new(TypeTerm::Tuple(fields));
        inference.data.insert(
            node,
            data::PreparedData {
                ty: ty.clone(),
                flow: None,
                contexts: Vec::new(),
                inputs: children.clone(),
                fact: data::DataExpression::Tuple {
                    elements: children.clone(),
                },
            },
        );
        self.establish(inference, node, ty, false)
    }

    fn candidate_closed(
        &mut self,
        ty: &Type<'g>,
        candidate: &SourceCandidate<'_, 'g>,
    ) -> Work<bool> {
        // Charge the search through the retained substitution before each type walk.
        let mut indices = BTreeSet::new();
        for sub in &candidate.substitution {
            self.elab.reserve(1)?;
            indices.insert(sub.position);
        }
        self.closed(
            ty,
            &candidate.body.header.binders,
            &|i| indices.contains(&i),
            1,
        )
    }

    fn conditional_output(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        call: &PreparedCall<'g>,
        inputs: &[Option<Type<'g>>],
        (viable, outcome): (&[SourceCandidate<'r, 'g>], Option<HypotheticalOutcome<'g>>),
        retention: ScopedRetention,
    ) -> Work<ConditionalOutput<'g>> {
        self.elab.step(inputs.len())?;
        if inputs.iter().any(Option::is_none) {
            return Ok(ConditionalOutput::default());
        }
        let chosen = match outcome {
            Some(HypotheticalOutcome::Winner(winner)) => {
                self.elab.step(viable.len())?;
                viable
                    .iter()
                    .find(|c| c.covers && c.body.source.declaration == *winner)
            }
            _ => None,
        };
        let module = inference.module;
        let mut solved = None;
        if let Some(selected) = chosen {
            let slots = self.infer_callable_outputs(call.fact.callee, selected)?;
            let substitution = self.slot_substitutions(&selected.body.header.binders, &slots)?;
            solved = Some((selected, slots, substitution));
        }
        let previous = self.ordinary_calls.get(&(&module.key, node));
        let (prior_goal, prior_instantiation) = if retention == ScopedRetention::Refresh {
            (None, None)
        } else {
            previous.map_or((None, None), |prior| {
                (prior.goal.clone(), prior.instantiation.clone())
            })
        };
        let goal = if let Some(id) = prior_goal {
            id
        } else {
            let selection = if let Some((c, _, substitution)) = &solved {
                self.elab.reserve(substitution.len())?;
                let argument_pair = match &c.body.header.shape {
                    DeclarationShape::Function { parameters, .. }
                        if let [formal] = parameters.as_slice() =>
                    {
                        self.direct_argument_pair(inference, call, &formal.ty)?
                    }
                    _ => None,
                };
                Some(SourceSelection {
                    winner: GoalSelection {
                        implementation: &c.body.source.declaration,
                        alternative: c.alternative,
                        substitution: substitution.clone(),
                    },
                    argument_pair,
                })
            } else {
                None
            };
            self.elab.reserve(inputs.len())?;
            let inputs = inputs.iter().map(|t| t.as_ref().unwrap().clone()).collect();
            self.determine_source(
                Self::site(module, node),
                call.fact.callee,
                inputs,
                written_arguments(call),
                &call.fact.bindings,
                selection,
            )?
        };
        self.complete_source_output(
            goal,
            solved.map(|(c, slots, _)| (c.body, c.alternative, slots)),
            written_arguments(call),
            inference.slots[&node].expected.as_ref(),
            prior_instantiation.as_ref(),
        )
    }

    fn complete_source_output(
        &mut self,
        goal: GoalId,
        selected: Option<SelectedSlots<'r, 'g>>,
        explicit: &[Substitution<'g>],
        expected: Option<&Type<'g>>,
        prior: Option<&GoalId>,
    ) -> Work<ConditionalOutput<'g>> {
        let mut output = ConditionalOutput {
            goal: Some(goal.clone()),
            ..ConditionalOutput::default()
        };
        if let Some(independent) = self.evidence.get(&goal).unwrap().output.as_ref() {
            let result = independent.result.clone();
            self.elab.reserve(independent.arguments.len())?;
            let arguments = independent.arguments.clone();
            output.agreement =
                self.check_selected_output(explicit, expected, &result, &arguments)?;
            output.ty = Some(result);
            return Ok(output);
        }
        // Context can fill genuinely free positions, but it cannot change the
        // independent determination or the winner chosen from the input vector.
        if let Some((body, alternative, mut slots)) = selected {
            let GoalJudgment::Determine { family: callee, .. } =
                self.evidence.get(&goal).unwrap().judgment
            else {
                unreachable!()
            };
            let binders = &body.header.binders;
            let DeclarationShape::Function { result, .. } = &body.header.shape else {
                unreachable!()
            };
            let determination = self.evidence.get(&goal).unwrap();
            if determination.state == GoalState::Unresolved(GoalBlocker::ConflictingAssumptions) {
                return Ok(output);
            }
            if let Some(proof_selection) = determination.selection.as_ref() {
                self.elab.step(proof_selection.substitution.len())?;
                for sub in &proof_selection.substitution {
                    slots[sub.position] = Some(sub.argument.clone());
                }
            }
            if matches!(
                determination.state,
                GoalState::Unresolved(
                    GoalBlocker::SelectedOutput
                        | GoalBlocker::Cycle
                        | GoalBlocker::MissingAssumption
                        | GoalBlocker::UnavailableRequirement
                        | GoalBlocker::InputSelection
                )
            ) {
                let header = body.substitution.as_deref().unwrap();
                let contracts = [(callee, header), (&body.source.declaration, &[][..])];
                if let Some(pending) = self.pending_output_slots(&contracts, binders, &slots)? {
                    self.elab.reserve(slots.len())?;
                    let established: Vec<_> = slots.iter().map(Option::is_some).collect();
                    for explicit in explicit {
                        self.free_output_equation(
                            &header[explicit.position].argument,
                            &explicit.argument,
                            binders,
                            &mut slots,
                            &pending,
                        )?;
                    }
                    if let Some(expected) = expected {
                        self.free_output_equation(result, expected, binders, &mut slots, &pending)?;
                    }
                    let substitution = self.slot_substitutions(binders, &slots)?;
                    self.elab.reserve(substitution.len())?;
                    let free: Vec<_> = substitution
                        .iter()
                        .filter(|s| !established[s.position])
                        .cloned()
                        .collect();
                    let has_defaults = self
                        .input
                        .ordinary
                        .unwrap()
                        .headers()
                        .get(callee)
                        .is_some_and(|h| !h.defaults.is_empty());
                    if !free.is_empty() || has_defaults {
                        let id = self.instantiate_source_contract(
                            &goal,
                            prior,
                            GoalSelection {
                                implementation: &body.source.declaration,
                                alternative,
                                substitution,
                            },
                            free,
                        )?;
                        output.instantiation = Some(id.clone());
                        let proof = self.evidence.get(&id).unwrap();
                        self.elab
                            .reserve(proof.dependencies.len() + proof.defaults.len())?;
                        output.defaults = proof.defaults.clone();
                        output.instantiated_requirements = proof.dependencies.clone();
                        if let Some(instantiated) = &proof.output {
                            let result = instantiated.result.clone();
                            self.elab.reserve(instantiated.arguments.len())?;
                            let arguments = instantiated.arguments.clone();
                            output.agreement = self
                                .check_selected_output(explicit, expected, &result, &arguments)?;
                            output.ty = Some(result);
                            return Ok(output);
                        }
                        let substitution = &proof.selection.as_ref().unwrap().substitution;
                        self.elab.step(substitution.len())?;
                        for sub in substitution {
                            slots[sub.position] = Some(sub.argument.clone());
                        }
                    }
                }
            }
            if self.closed(result, binders, &|i| slots[i].is_some(), 1)? {
                let substitution = self.slot_substitutions(binders, &slots)?;
                output.ty = Some(self.substitute(result, &substitution, 1)?);
            }
        }
        Ok(output)
    }

    pub(in crate::analysis::scalars) fn determine_source(
        &mut self,
        site: Site<'g>,
        callee: &'g DeclarationId,
        inputs: Vec<Type<'g>>,
        explicit: &[Substitution<'g>],
        bindings: &[&'g Binding],
        selection: Option<SourceSelection<'g>>,
    ) -> Work<GoalId> {
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(callee)
            .unwrap();
        let mut budget = Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let positions = source::input_positions(family, &mut budget);
        self.elab.statistics = budget.statistics;
        let positions = positions?;
        self.elab.reserve(inputs.len() + explicit.len())?;
        let filters = explicit
            .iter()
            .filter(|s| positions[s.position])
            .cloned()
            .collect();
        let origin = self.evidence_module(&callee.module)?;
        let declaration = family.header.declaration;
        let (winner, argument_pair) = selection.map_or((None, None), |source| {
            (Some(source.winner), source.argument_pair)
        });
        let goal = self.issue_determination(
            GoalJudgment::Determine {
                family: callee,
                inputs,
                filters,
            },
            site,
            Self::site(origin, origin.syntax.nodes[declaration.node].children[0]),
            bindings,
            winner,
        )?;
        if let Some(pair) = argument_pair {
            self.attach_argument_pair(&goal, pair)?;
        }
        self.solve_goal(&goal)?;
        Ok(goal)
    }

    pub(super) fn ordinary_value_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        written: &FunctionValueFacts<'g>,
    ) -> Work<bool> {
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(written.declaration)
            .unwrap();
        let DeclarationShape::Function { parameters, .. } = &family.header.shape else {
            unreachable!()
        };
        let receiving = inference.slots[&node].expected.clone();
        let projected = inference.callable_contexts.get(&node).cloned();
        let (receiving_parameters, receiving_result) = match receiving.as_deref() {
            Some(TypeTerm::Callable {
                parameters, result, ..
            }) => (Some(parameters), Some(result)),
            _ => (None, None),
        };
        let binders = &family.header.binders;
        self.elab.reserve(binders.len() + parameters.len())?;
        let mut slots = vec![None; binders.len()];
        for sub in &written.substitution {
            slots[sub.position] = Some(sub.argument.clone());
        }
        let mut fits = receiving_parameters.is_none_or(|p| p.len() == parameters.len())
            && projected
                .as_ref()
                .is_none_or(|p| p.parameters.len() == parameters.len());
        if fits {
            for (i, formal) in parameters.iter().enumerate() {
                for actual in [
                    receiving_parameters.map(|p| &p[i]),
                    projected.as_ref().and_then(|p| p.parameters[i].as_ref()),
                ]
                .into_iter()
                .flatten()
                {
                    fits &= self.input_equation(&formal.ty, actual, binders, &mut slots, &[], 1)?;
                }
            }
        }
        if fits {
            let substitutions = self.slot_substitutions(binders, &slots)?;
            let mut inputs = Vec::new();
            for formal in parameters {
                if !self.closed(&formal.ty, binders, &|i| slots[i].is_some(), 1)? {
                    return Ok(false);
                }
                inputs.push(self.substitute(&formal.ty, &substitutions, 1)?);
            }
            let goal = self.determine_source(
                Self::site(inference.module, node),
                written.declaration,
                inputs,
                &written.substitution,
                &written.bindings,
                None,
            )?;
            let selection = self.evidence.get(&goal).unwrap().selection.as_ref();
            let scoped = self
                .ordinary_scope
                .as_ref()
                .and_then(|scope| scope.routine())
                .is_some_and(|role| {
                    role.needs_scoped_evidence(
                        written.declaration,
                        selection.map(|s| s.implementation),
                    )
                });
            let selected = if let Some(selection) = selection {
                self.elab.step(family.implementations.len())?;
                let body = family
                    .implementations
                    .iter()
                    .find(|b| b.source.declaration == *selection.implementation)
                    .unwrap();
                self.elab.reserve(body.header.binders.len())?;
                Some((
                    body,
                    selection.alternative,
                    vec![None; body.header.binders.len()],
                ))
            } else {
                None
            };
            let required =
                receiving_result.or_else(|| projected.as_ref().and_then(|p| p.result.as_ref()));
            let output = self.complete_source_output(
                goal.clone(),
                selected,
                &written.substitution,
                required,
                None,
            )?;
            // The compiler operation needs actual argument occurrences at a
            // direct call; ordinary winners of the same family do not.
            let formed = self.has_installed_ordinary_scope(written.declaration)
                && !scoped
                && self.form_ordinary_value(inference, node, written, &goal, &output)?;
            self.elab.reserve(1)?;
            self.ordinary_values.insert(
                (&inference.module.key, node),
                OrdinaryValue {
                    goal,
                    instantiation: output.instantiation,
                    output_agreement: output.agreement,
                },
            );
            if formed {
                return Ok(true);
            }
        }
        self.elab.reserve(written.bindings.len())?;
        self.deferred_declaration(
            inference.module,
            node,
            ScalarUnsupported::FamilySelection,
            family.header.declaration,
            written.bindings.clone(),
        )?;
        inference.slots.get_mut(&node).unwrap().blocked = true;
        Ok(true)
    }

    fn form_ordinary_value(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        written: &FunctionValueFacts<'g>,
        goal: &GoalId,
        output: &ConditionalOutput<'g>,
    ) -> Work<bool> {
        let determination = self.evidence.get(goal).unwrap();
        let proof = self
            .evidence
            .get(output.instantiation.as_ref().unwrap_or(goal))
            .unwrap();
        if !matches!(output.agreement, OutputAgreement::Agrees)
            || inference.slots[&node].ordinary_context.is_some()
            || proof.state != GoalState::Proved
            || proof.premise.is_some()
            || determination.premise.is_some()
        {
            return Ok(false);
        }
        let Some(selection) = &proof.selection else {
            return Ok(false);
        };
        let result = proof.output.as_ref().unwrap();
        let GoalJudgment::Determine { inputs, .. } = &determination.judgment else {
            unreachable!()
        };
        self.elab.reserve(
            2 + inputs.len()
                + selection.substitution.len()
                + written.bindings.len()
                + output.defaults.len()
                + result.arguments.len(),
        )?;
        let ty = Arc::new(TypeTerm::Callable {
            mode: InvocationMode::ReadOnly,
            parameters: inputs.clone(),
            result: result.result.clone(),
        });
        let selection = selection.clone();
        let arguments = result.arguments.clone();
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(written.declaration)
            .unwrap();
        let substitution = self.family_substitutions(&family.header, &arguments)?;
        self.establish(inference, node, ty, false)?;
        inference.slots.get_mut(&node).unwrap().function_value =
            Some(Arc::new(FunctionValueFacts {
                declaration: written.declaration,
                selection: Some(selection),
                bindings: written.bindings.clone(),
                substitution,
                defaults: output.defaults.clone(),
            }));
        Ok(true)
    }

    fn check_selected_output(
        &mut self,
        explicit: &[Substitution<'g>],
        expected: Option<&Type<'g>>,
        result: &Type<'g>,
        arguments: &[Type<'g>],
    ) -> Work<OutputAgreement<'g>> {
        for explicit in explicit {
            let actual = &arguments[explicit.position];
            if !self.same(actual, &explicit.argument)? {
                return Ok(OutputAgreement::Disagrees {
                    actual: actual.clone(),
                    required: explicit.argument.clone(),
                });
            }
        }
        if let Some(expected) = expected
            && !self.same(result, expected)?
        {
            return Ok(OutputAgreement::Disagrees {
                actual: result.clone(),
                required: expected.clone(),
            });
        }
        Ok(OutputAgreement::Agrees)
    }

    fn ordinary_context(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        ty: Type<'g>,
        family: &DeclarationId,
    ) -> Work<()> {
        if let Some(premise) = self.ordinary_use_premise(family) {
            self.conditional_context(inference, node, ty, premise)
        } else {
            self.establish(inference, node, ty, true)
        }
    }

    pub(super) fn conditional_context(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        ty: Type<'g>,
        premise: InputPremise,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        let mut pending = vec![node];
        while let Some(child) = pending.pop() {
            self.elab.step(1)?;
            let slot = inference.slots.get_mut(&child).unwrap();
            if slot.ty.is_some() {
                continue;
            }
            slot.ordinary_context = Some(
                slot.ordinary_context
                    .map_or(premise, |p| p.combine(premise)),
            );
            let children = self.runtime_children(inference, child)?;
            self.elab.reserve(children.len())?;
            pending.extend(children);
        }
        self.establish(inference, node, ty, true)
    }

    fn conditional_finding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        kind: ScalarFindingKind,
        premise: InputPremise,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        self.conditional_findings.push(ConditionalFinding {
            premise,
            finding: ScalarFinding {
                site: Self::site(module, node),
                kind,
                related: None,
                actual: None,
                expected: None,
                bindings: Vec::new(),
            },
        });
        Ok(())
    }

    pub(in crate::analysis::scalars) fn separate_ordinary_findings(
        &mut self,
        inference: &Inference<'g>,
        start: usize,
    ) -> Work<()> {
        let size = self.facts.findings.len() - start;
        if size == 0 {
            return Ok(());
        }
        self.elab.reserve(size.checked_mul(2).ok_or(())?)?;
        for finding in self.facts.findings.split_off(start) {
            let premise = self.arrow_premise.or_else(|| {
                (finding.site.module == &inference.module.key)
                    .then(|| {
                        inference
                            .slots
                            .get(&finding.site.node)
                            .and_then(|s| s.ordinary_context)
                    })
                    .flatten()
            });
            if let Some(premise) = premise
                && matches!(
                    finding.kind,
                    ScalarFindingKind::TypeMismatch
                        | ScalarFindingKind::LiteralRange
                        | ScalarFindingKind::NumericContext
                        | ScalarFindingKind::InsufficientContext
                        | ScalarFindingKind::Arity
                        | ScalarFindingKind::ReadOnly
                )
            {
                self.conditional_findings
                    .push(ConditionalFinding { premise, finding });
            } else {
                self.facts.findings.push(finding);
            }
        }
        Ok(())
    }

    pub(in crate::analysis::scalars) fn record_checked_ordinary_call(
        &mut self,
        module: &'g Module,
        node: NodeId,
        facts: &mut CallFacts<'g>,
    ) -> Work<()> {
        let Some(call) = self
            .ordinary_calls
            .get(&(&module.key, node))
            .filter(|call| call.checked)
        else {
            return Ok(());
        };
        let proof = self
            .evidence
            .get(call.instantiation.as_ref().or(call.goal.as_ref()).unwrap())
            .unwrap();
        if self.has_installed_ordinary_scope(call.family) || call.checked_local_disposer {
            self.elab.reserve(call.defaults.len())?;
            facts.defaults = call.defaults.clone();
            if let Some(selection) = &proof.selection {
                self.elab.reserve(1 + selection.substitution.len())?;
                facts.selection = Some(selection.clone());
            }
        }
        let output = proof.output.as_ref().unwrap();
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.family)
            .unwrap();
        self.elab.reserve(output.arguments.len())?;
        facts.substitution = family
            .header
            .binders
            .iter()
            .zip(&output.arguments)
            .map(|(binder, ty)| Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument: ty.clone(),
            })
            .collect();
        Ok(())
    }

    pub(in crate::analysis::scalars) fn defer_ordinary_calls(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<()> {
        let module = inference.module;
        for (&node, call) in &inference.calls {
            self.elab.step(1)?;
            if !call.family
                || matches!(
                    inference.slots[&node].installed_operation,
                    Some(
                        data::DataExpression::NativeOwnership(_)
                            | data::DataExpression::CallableCopy(_)
                    )
                )
                || self
                    .ordinary_calls
                    .get(&(&module.key, node))
                    .is_some_and(|fact| {
                        fact.checked
                            || matches!(fact.output_agreement, OutputAgreement::Disagrees { .. })
                                && fact.goal.as_ref().is_some_and(|id| {
                                    self.evidence.get(id).unwrap().state == GoalState::Proved
                                })
                    })
                || ((self.is_canonical_scalar_family(call.fact.callee)
                    || self.has_installed_ordinary_scope(call.fact.callee))
                    && inference.slots[&node].blocked)
            {
                continue;
            }
            let header = self
                .input
                .ordinary
                .unwrap()
                .captured_family(call.fact.callee)
                .unwrap()
                .header
                .declaration;
            self.elab
                .reserve(call.fact.bindings.len() + call.fact.arguments.len())?;
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::FamilySelection,
                header,
                call.fact.bindings.clone(),
            )?;
            let mut pending = call.fact.arguments.clone();
            while let Some(argument) = pending.pop() {
                self.elab.step(1)?;
                if inference.slots[&argument].ty.is_some() {
                    continue;
                }
                inference.slots.get_mut(&argument).unwrap().blocked = true;
                let children = self.runtime_children(inference, argument)?;
                self.elab.reserve(children.len())?;
                pending.extend(children);
            }
        }
        Ok(())
    }
}
