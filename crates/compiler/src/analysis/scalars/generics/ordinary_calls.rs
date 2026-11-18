//! Conditional context and selection for ordinary families. No role admission.
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
    pub premise: InputPremise,
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
    /// Selected requirements formed after a genuinely free output was instantiated at this use.
    pub instantiated_requirements: Vec<GoalId>,
    /// Type from the selected header alone, not a checked result or evidence.
    pub selected_result: Option<Type<'g>>,
    pub output_agreement: OutputAgreement<'g>,
}

#[derive(Clone, Copy)]
struct CallablePath {
    column: usize,
    tuple_index: Option<usize>,
    node: NodeId,
}

#[derive(Debug)]
pub struct OrdinaryValue<'g> {
    pub goal: GoalId,
    pub output_agreement: OutputAgreement<'g>,
}

struct ConditionalOutput<'g> {
    goal: Option<GoalId>,
    ty: Option<Type<'g>>,
    agreement: OutputAgreement<'g>,
    defaults: Vec<DefaultArgumentUse<'g>>,
    instantiated_requirements: Vec<GoalId>,
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
    pub(in crate::analysis::scalars) fn ordinary_call_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        default_step: bool,
    ) -> Work<bool> {
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
        let mut consistent = true;
        for (parameter, &argument) in call.parameters.iter().zip(&call.fact.arguments) {
            let actual = inference.slots[&argument]
                .ty
                .as_ref()
                .filter(|t| primitive_name(t) != Some("never"));
            if let Some(actual) = actual
                && !self.accept_input(
                    parameter,
                    actual,
                    binders,
                    &mut slots,
                    &call.fact.substitution,
                )?
            {
                consistent = false;
            }
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
        let mut callbacks = Vec::new();
        if !consistent {
            outcome = None;
        }
        if consistent {
            self.commit_brackets(inference, &call, &substitutions, &mut viable)?;
            for (column, &node) in call.fact.arguments.iter().enumerate() {
                self.elab.step(1)?;
                let syntax = &module.syntax.nodes[node];
                if syntax.kind == NodeKind::Bracket && inference.ordinary_tuples.contains(&node) {
                    for (tuple_index, &element) in syntax.children.iter().enumerate() {
                        self.append_callback(
                            inference,
                            &call,
                            &substitutions,
                            &viable,
                            CallablePath {
                                column,
                                tuple_index: Some(tuple_index),
                                node: element,
                            },
                            &mut callbacks,
                        )?;
                    }
                } else {
                    self.append_callback(
                        inference,
                        &call,
                        &substitutions,
                        &viable,
                        CallablePath {
                            column,
                            tuple_index: None,
                            node,
                        },
                        &mut callbacks,
                    )?;
                }
            }
        }
        for context in &callbacks {
            let leaf = self.source_leaf(inference.module, context.argument)?;
            if inference.slots[&leaf].ty.is_some() || inference.slots[&leaf].blocked {
                continue;
            }
            inference.slots.get_mut(&leaf).unwrap().ordinary_context = Some(PREMISE);
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
        for (i, context) in receiving.iter_mut().enumerate() {
            if inputs[i].is_some() || !consistent {
                continue;
            }
            if context.is_none() && blocker.is_none() {
                *context = self.agreed_receiving(&viable, i)?;
            }
            if let Some(ty) = context.clone() {
                self.conditional_context(inference, call.fact.arguments[i], ty)?;
            }
        }
        let output = if consistent {
            self.conditional_output(inference, node, &call, &inputs, &viable, outcome)?
        } else {
            ConditionalOutput {
                goal: None,
                ty: None,
                agreement: OutputAgreement::NotDetermined,
                defaults: Vec::new(),
                instantiated_requirements: Vec::new(),
            }
        };
        self.elab.reserve(
            1 + call.fact.bindings.len()
                + call.fact.substitution.len()
                + viable.len()
                + output.defaults.len(),
        )?;
        let mut defaults = call.fact.defaults.clone();
        defaults.extend(output.defaults);
        self.ordinary_calls.insert(
            (&module.key, node),
            OrdinaryCall {
                site: Self::site(module, node),
                family: call.fact.callee,
                premise: PREMISE,
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
                instantiated_requirements: output.instantiated_requirements,
                selected_result: output.ty,
                output_agreement: output.agreement,
            },
        );
        self.elab.reserve(1)?;
        inference.calls.insert(node, call);
        Ok(false)
    }

    fn compare_ordinary(
        &self,
        call: &PreparedCall<'g>,
        inputs: &[Option<Type<'g>>],
        budget: &mut Budget,
    ) -> Work<Comparison<'r, 'g>> {
        let family = match source::family(self.input.ordinary.unwrap(), call.fact.callee, budget)? {
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
        index: Option<usize>,
    ) -> Work<Option<&'a Type<'g>>> {
        let Some(index) = index else {
            return Ok(Some(receiver));
        };
        let tuple = self.receiving_root(receiver, substitutions)?;
        Ok(match tuple.as_ref() {
            TypeTerm::Tuple(fields) => fields.get(index),
            _ => None,
        })
    }

    fn append_callback(
        &mut self,
        inference: &Inference<'g>,
        call: &PreparedCall<'g>,
        declared: &[Substitution<'g>],
        candidates: &[SourceCandidate<'r, 'g>],
        path: CallablePath,
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
            self.receiver_at(&call.parameters[path.column], declared, path.tuple_index)?
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
                path.tuple_index,
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

    fn commit_brackets(
        &mut self,
        inference: &mut Inference<'g>,
        call: &PreparedCall<'g>,
        declared: &[Substitution<'g>],
        viable: &mut Vec<SourceCandidate<'r, 'g>>,
    ) -> Work<()> {
        for (column, &node) in call.fact.arguments.iter().enumerate() {
            self.elab.step(1)?;
            let syntax = &inference.module.syntax.nodes[node];
            if syntax.kind != NodeKind::Bracket || inference.slots[&node].ty.is_some() {
                continue;
            }
            let declared = self.receiving_root(&call.parameters[column], declared)?;
            let fixed = matches!(declared.as_ref(), TypeTerm::Tuple(_));
            if !fixed {
                let mut unanimous = !viable.is_empty();
                for candidate in viable.iter() {
                    let root =
                        self.receiving_root(parameter(candidate, column), &candidate.substitution)?;
                    unanimous &= matches!(root.as_ref(), TypeTerm::Tuple(_));
                }
                if !unanimous {
                    continue;
                }
            }
            let count = syntax.children.len();
            let before = std::mem::take(viable);
            self.elab.reserve(before.len())?;
            for candidate in before {
                let root =
                    self.receiving_root(parameter(&candidate, column), &candidate.substitution)?;
                if !matches!(root.as_ref(), TypeTerm::Tuple(fields) if fields.len() != count) {
                    viable.push(candidate);
                }
            }
            if let TypeTerm::Tuple(fields) = declared.as_ref()
                && fields.len() != count
            {
                continue;
            }
            if viable.is_empty() && !fixed {
                continue;
            }
            self.elab.reserve(1)?;
            if inference.ordinary_tuples.insert(node) {
                inference.slots.get_mut(&node).unwrap().ordinary_context = Some(PREMISE);
                self.enqueue(inference, node)?;
            }
            for (position, &child) in syntax.children.iter().enumerate() {
                self.elab.step(1)?;
                if inference.slots[&child].ty.is_some() {
                    continue;
                }
                let mut agreed = None;
                for candidate in viable.iter() {
                    let root =
                        self.receiving_root(parameter(candidate, column), &candidate.substitution)?;
                    let TypeTerm::Tuple(fields) = root.as_ref() else {
                        agreed = None;
                        break;
                    };
                    let field = &fields[position];
                    if !self.candidate_closed(field, candidate)? {
                        agreed = None;
                        break;
                    }
                    let proposed = self.substitute(field, &candidate.substitution, 1)?;
                    if let Some(ref previous) = agreed
                        && !self.same(previous, &proposed)?
                    {
                        agreed = None;
                        break;
                    }
                    agreed = Some(proposed);
                }
                if let Some(ty) = agreed {
                    self.conditional_context(inference, child, ty)?;
                }
            }
        }
        Ok(())
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

    fn agreed_receiving(
        &mut self,
        candidates: &[SourceCandidate<'_, 'g>],
        column: usize,
    ) -> Work<Option<Type<'g>>> {
        let mut agreed = None;
        for candidate in candidates {
            let parameter = parameter(candidate, column);
            if !self.candidate_closed(parameter, candidate)? {
                return Ok(None);
            }
            let proposed = self.substitute(parameter, &candidate.substitution, 1)?;
            if let Some(ref previous) = agreed
                && !self.same(previous, &proposed)?
            {
                return Ok(None);
            }
            agreed = Some(proposed);
        }
        Ok(agreed)
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
        viable: &[SourceCandidate<'r, 'g>],
        outcome: Option<HypotheticalOutcome<'g>>,
    ) -> Work<ConditionalOutput<'g>> {
        let mut output = ConditionalOutput {
            goal: None,
            ty: None,
            agreement: OutputAgreement::NotDetermined,
            defaults: Vec::new(),
            instantiated_requirements: Vec::new(),
        };
        self.elab.step(inputs.len())?;
        if inputs.iter().any(Option::is_none) {
            return Ok(output);
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
        let goal = if let Some(id) = self
            .ordinary_calls
            .get(&(&module.key, node))
            .and_then(|call| call.goal.as_ref())
        {
            id.clone()
        } else {
            let selection = if let Some((c, _, substitution)) = &solved {
                self.elab.reserve(substitution.len())?;
                Some(GoalSelection {
                    implementation: &c.body.source.declaration,
                    alternative: c.alternative,
                    substitution: substitution.clone(),
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
        let expected = inference.slots[&node].expected.clone();
        output.goal = Some(goal.clone());
        if let Some(independent) = self.evidence.get(&goal).unwrap().output.as_ref() {
            let result = independent.result.clone();
            self.elab.reserve(independent.arguments.len())?;
            let arguments = independent.arguments.clone();
            output.agreement = self.check_selected_output(
                written_arguments(call),
                expected.as_ref(),
                &result,
                &arguments,
            )?;
            output.ty = Some(result);
            return Ok(output);
        }
        // If a selected header cannot yet supply its entire evidence output,
        // retain any independently fixed result, not a checked call.
        if let Some((selected, mut slots, _)) = solved {
            let binders = &selected.body.header.binders;
            let DeclarationShape::Function { result, .. } = &selected.body.header.shape else {
                unreachable!()
            };
            if let Some(proof_selection) = self.evidence.get(&goal).unwrap().selection.as_ref() {
                self.elab.step(proof_selection.substitution.len())?;
                for sub in &proof_selection.substitution {
                    slots[sub.position] = Some(sub.argument.clone());
                }
            }
            let header = selected.body.substitution.as_deref().unwrap();
            if let Some(pending) = self.pending_output_slots(
                &[
                    (call.fact.callee, header),
                    (&selected.body.source.declaration, &[]),
                ],
                binders,
                &slots,
            )? {
                for explicit in written_arguments(call) {
                    self.free_output_equation(
                        &header[explicit.position].argument,
                        &explicit.argument,
                        binders,
                        &mut slots,
                        &pending,
                    )?;
                }
                if let Some(expected) = &expected {
                    self.free_output_equation(result, expected, binders, &mut slots, &pending)?;
                }
                output.defaults = self.selected_output_defaults(
                    call.fact.callee,
                    selected,
                    &mut slots,
                    &pending,
                )?;
            }
            let substitution = self.slot_substitutions(binders, &slots)?;
            if self.closed(result, binders, &|i| slots[i].is_some(), 1)? {
                output.ty = Some(self.substitute(result, &substitution, 1)?);
                if slots.iter().all(Option::is_some) {
                    let header = selected.body.substitution.as_ref().unwrap();
                    self.elab.reserve(header.len().checked_mul(2).ok_or(())?)?;
                    let instantiated = header
                        .iter()
                        .map(|s| {
                            Ok(Substitution {
                                argument: self.substitute(&s.argument, &substitution, 1)?,
                                ..s.clone()
                            })
                        })
                        .collect::<Work<Vec<_>>>()?;
                    let arguments: Vec<_> =
                        instantiated.iter().map(|s| s.argument.clone()).collect();
                    output.agreement = self.check_selected_output(
                        written_arguments(call),
                        expected.as_ref(),
                        output.ty.as_ref().unwrap(),
                        &arguments,
                    )?;
                    if matches!(output.agreement, OutputAgreement::Agrees) {
                        output.instantiated_requirements = self.source_contract_requirements(
                            Self::site(module, node),
                            &[
                                (call.fact.callee, &instantiated),
                                (&selected.body.source.declaration, &substitution),
                            ],
                        )?;
                    }
                }
            }
        }
        Ok(output)
    }

    fn free_output_equation(
        &mut self,
        pattern: &Type<'g>,
        actual: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
        pending: &BTreeSet<usize>,
    ) -> Work<bool> {
        if self.closed(pattern, binders, &|i| slots[i].is_some(), 1)?
            || !self.closed(pattern, binders, &|i| !pending.contains(&i), 1)?
        {
            return Ok(false);
        }
        self.elab.reserve(slots.len())?;
        let mut proposed = slots.to_vec();
        let fits = self.input_equation(pattern, actual, binders, &mut proposed, &[], 1)?;
        if fits {
            slots.clone_from_slice(&proposed);
        }
        Ok(fits)
    }

    fn selected_output_defaults(
        &mut self,
        callee: &'g DeclarationId,
        selected: &SourceCandidate<'r, 'g>,
        slots: &mut [Option<Type<'g>>],
        pending: &BTreeSet<usize>,
    ) -> Work<Vec<DefaultArgumentUse<'g>>> {
        let ordinary = self.input.ordinary.unwrap();
        let family = ordinary.captured_family(callee).unwrap();
        let Some(written) = ordinary.headers().get(callee) else {
            return Ok(Vec::new());
        };
        let binders = &selected.body.header.binders;
        let header = selected.body.substitution.as_deref().unwrap();
        let mut used = Vec::new();
        for default in &written.defaults {
            self.elab.step(header.len())?;
            let Some(output) = header.iter().find(|s| s.binder == default.binder) else {
                continue;
            };
            if self.closed(&output.argument, binders, &|i| slots[i].is_some(), 1)? {
                continue;
            }
            let Some(ty) = &default.ty else {
                continue;
            };
            let solved = self.slot_substitutions(binders, slots)?;
            let mut known = Vec::new();
            let mut positions = BTreeSet::new();
            for slot in &header[..output.position] {
                if self.closed(&slot.argument, binders, &|i| slots[i].is_some(), 1)? {
                    self.elab.reserve(2)?;
                    known.push(Substitution {
                        argument: self.substitute(&slot.argument, &solved, 1)?,
                        ..slot.clone()
                    });
                    positions.insert(slot.position);
                }
            }
            if self.closed(ty, &family.header.binders, &|i| positions.contains(&i), 1)? {
                let actual = self.substitute(ty, &known, 1)?;
                if self.free_output_equation(&output.argument, &actual, binders, slots, pending)? {
                    self.elab.reserve(1)?;
                    used.push(DefaultArgumentUse {
                        binder: default.binder,
                        node: default.node,
                        span: default.span,
                    });
                }
            }
        }
        Ok(used)
    }

    fn determine_source(
        &mut self,
        site: Site<'g>,
        callee: &'g DeclarationId,
        inputs: Vec<Type<'g>>,
        explicit: &[Substitution<'g>],
        bindings: &[&'g Binding],
        selection: Option<GoalSelection<'g>>,
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
        let goal = self.issue_determination(
            GoalJudgment::Determine {
                family: callee,
                inputs,
                filters,
            },
            site,
            Self::site(origin, origin.syntax.nodes[declaration.node].children[0]),
            bindings,
            selection,
        )?;
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
            let output_agreement = if let Some(output) =
                self.evidence.get(&goal).unwrap().output.as_ref()
            {
                let result = output.result.clone();
                self.elab.reserve(output.arguments.len())?;
                let arguments = output.arguments.clone();
                let required =
                    receiving_result.or_else(|| projected.as_ref().and_then(|p| p.result.as_ref()));
                self.check_selected_output(&written.substitution, required, &result, &arguments)?
            } else {
                OutputAgreement::NotDetermined
            };
            self.elab.reserve(1)?;
            self.ordinary_values.insert(
                (&inference.module.key, node),
                OrdinaryValue {
                    goal,
                    output_agreement,
                },
            );
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

    fn conditional_context(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        ty: Type<'g>,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        let mut pending = vec![node];
        while let Some(child) = pending.pop() {
            self.elab.step(1)?;
            let slot = inference.slots.get_mut(&child).unwrap();
            if slot.ty.is_some() {
                continue;
            }
            slot.ordinary_context = Some(PREMISE);
            let children = self.runtime_children(inference, child)?;
            self.elab.reserve(children.len())?;
            pending.extend(children);
        }
        self.establish(inference, node, ty, true)
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

    pub(in crate::analysis::scalars) fn defer_ordinary_calls(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<()> {
        let module = inference.module;
        for (&node, call) in &inference.calls {
            self.elab.step(1)?;
            if !call.family {
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
