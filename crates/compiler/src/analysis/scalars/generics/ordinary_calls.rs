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
    pub explicit: Vec<Substitution<'g>>,
    pub candidates: Vec<OrdinaryCandidate<'g>>,
    pub blocker: Option<SelectionBlocker>,
    pub outcome: Option<HypotheticalOutcome<'g>>,
    /// Type from the selected header alone, not a checked result or evidence.
    pub selected_result: Option<Type<'g>>,
    pub output_agreement: OutputAgreement<'g>,
}

struct Comparison<'r, 'g> {
    viable: Vec<SourceCandidate<'r, 'g>>,
    outcome: Option<HypotheticalOutcome<'g>>,
    blocker: Option<SelectionBlocker>,
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
    ) -> Work<()> {
        let call = inference.calls.remove(&node).unwrap();
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
        if !consistent {
            outcome = None;
        }
        if consistent {
            self.commit_brackets(inference, &call, &substitutions, &mut viable)?;
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
        let (selected_result, output_agreement) = self.conditional_output(
            inference.slots[&node].expected.as_ref(),
            &call,
            &viable,
            outcome,
        )?;
        self.elab
            .reserve(1 + call.fact.bindings.len() + call.fact.substitution.len() + viable.len())?;
        self.ordinary_calls.insert(
            (&module.key, node),
            OrdinaryCall {
                site: Self::site(module, node),
                family: call.fact.callee,
                premise: PREMISE,
                routes: call.fact.bindings.clone(),
                inputs,
                receiving,
                explicit: call.fact.substitution.clone(),
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
                selected_result,
                output_agreement,
            },
        );
        self.elab.reserve(1)?;
        inference.calls.insert(node, call);
        Ok(())
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
        let filters: Vec<_> = call
            .fact
            .substitution
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
        expected: Option<&Type<'g>>,
        call: &PreparedCall<'g>,
        viable: &[SourceCandidate<'_, 'g>],
        outcome: Option<HypotheticalOutcome<'g>>,
    ) -> Work<(Option<Type<'g>>, OutputAgreement<'g>)> {
        let Some(HypotheticalOutcome::Winner(winner)) = outcome else {
            return Ok((None, OutputAgreement::NotDetermined));
        };
        self.elab.step(viable.len())?;
        let selected = viable
            .iter()
            .find(|c| c.covers && c.body.source.declaration == *winner)
            .unwrap();
        let DeclarationShape::Function { result, .. } = &selected.body.header.shape else {
            unreachable!()
        };
        if !self.candidate_closed(result, selected)? {
            return Ok((None, OutputAgreement::NotDetermined));
        }
        let result = self.substitute(result, &selected.substitution, 1)?;
        for explicit in &call.fact.substitution {
            let header = &selected.body.substitution.as_ref().unwrap()[explicit.position].argument;
            if !self.candidate_closed(header, selected)? {
                return Ok((Some(result), OutputAgreement::NotDetermined));
            }
            let actual = self.substitute(header, &selected.substitution, 1)?;
            if !self.same(&actual, &explicit.argument)? {
                return Ok((
                    Some(result),
                    OutputAgreement::Disagrees {
                        actual,
                        required: explicit.argument.clone(),
                    },
                ));
            }
        }
        if let Some(expected) = expected
            && !self.same(&result, expected)?
        {
            return Ok((
                Some(result.clone()),
                OutputAgreement::Disagrees {
                    actual: result,
                    required: expected.clone(),
                },
            ));
        }
        Ok((Some(result), OutputAgreement::Agrees))
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
            let premise = if finding.site.module == &inference.module.key {
                inference
                    .slots
                    .get(&finding.site.node)
                    .and_then(|s| s.ordinary_context)
            } else {
                None
            };
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
