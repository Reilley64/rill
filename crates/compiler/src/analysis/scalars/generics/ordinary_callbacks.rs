//! Written callable context is not a proof of callable capability.
use super::*;
use ordinary::RequirementTarget;
use ordinary::source::SourceCandidate;

#[derive(Clone, Debug)]
pub struct OrdinaryCallbackContext<'g> {
    pub argument: NodeId,
    pub parameters: Vec<Option<Type<'g>>>,
    pub result: Option<Type<'g>>,
}

struct TypeProjection<'g> {
    ty: Option<Type<'g>>,
    incompatible: bool,
}

fn is_callable_clause(target: Option<RequirementTarget<'_>>) -> bool {
    matches!(target, Some(RequirementTarget::Predicate(id)) if matches!(id.name(), "callable" | "callableMut" | "callableOnce"))
}

struct SignatureEquation<'g> {
    receiver: Type<'g>,
    input: Type<'g>,
    output: Type<'g>,
}

impl<'r, 'g> Checker<'r, 'g> {
    /// Read chosen callable signatures; never feed these equations back into ranking.
    pub(super) fn infer_callable_outputs(
        &mut self,
        family: &'g DeclarationId,
        selected: &SourceCandidate<'r, 'g>,
    ) -> Work<(Vec<Option<Type<'g>>>, bool)> {
        let ordinary = self.input.ordinary.unwrap();
        let mut equations = Vec::new();
        let mut has_requirements = false;
        let selected_header = selected.body.substitution.as_deref().unwrap();
        let empty: &[Substitution<'g>] = &[];
        for (contract, substitution) in [
            (family, selected_header),
            (&selected.body.source.declaration, empty),
        ] {
            let Some(header) = ordinary.headers().get(contract) else {
                continue;
            };
            has_requirements |= !header.requirements.is_empty();
            for requirement in &header.requirements {
                self.elab.step(1)?;
                if !is_callable_clause(requirement.target) {
                    continue;
                }
                let [Some(receiver), Some(input), Some(output)] = requirement.arguments.as_slice()
                else {
                    continue;
                };
                self.elab.reserve(1)?;
                equations.push(SignatureEquation {
                    receiver: self.substitute(receiver, substitution, 1)?,
                    input: self.substitute(input, substitution, 1)?,
                    output: self.substitute(output, substitution, 1)?,
                });
            }
        }
        let binders = &selected.body.header.binders;
        self.elab.reserve(binders.len())?;
        let mut slots = vec![None; binders.len()];
        for sub in &selected.substitution {
            slots[sub.position] = Some(sub.argument.clone());
        }
        loop {
            let mut changed = false;
            for equation in &equations {
                self.elab.step(1)?;
                if !self.closed(&equation.receiver, binders, &|i| slots[i].is_some(), 1)? {
                    continue;
                }
                let solved = self.slot_substitutions(binders, &slots)?;
                let receiver = self.substitute(&equation.receiver, &solved, 1)?;
                let receiver = match receiver.as_ref() {
                    TypeTerm::Reference { referent, .. } => referent,
                    _ => &receiver,
                };
                let TypeTerm::Callable {
                    parameters, result, ..
                } = receiver.as_ref()
                else {
                    continue;
                };
                let TypeTerm::Tuple(expected) = equation.input.as_ref() else {
                    continue;
                };
                if parameters.len() != expected.len() {
                    continue;
                }
                self.elab.reserve(slots.len())?;
                let mut proposed = slots.clone();
                let mut matches = true;
                for (left, right) in expected.iter().zip(parameters) {
                    if !self.input_equation(left, right, binders, &mut proposed, &[], 1)? {
                        matches = false;
                        break;
                    }
                }
                if matches
                    && self.input_equation(
                        &equation.output,
                        result,
                        binders,
                        &mut proposed,
                        &[],
                        1,
                    )?
                {
                    self.elab.step(slots.len())?;
                    changed |= slots
                        .iter()
                        .zip(&proposed)
                        .any(|(old, new)| old.is_none() && new.is_some());
                    slots = proposed;
                }
            }
            if !changed {
                break;
            }
        }
        Ok((slots, has_requirements))
    }

    pub(super) fn written_callable_context(
        &mut self,
        header: &'r DeclarationFacts<'g>,
        receiver: &Type<'g>,
        substitution: &[Substitution<'g>],
        argument: NodeId,
    ) -> Work<Option<OrdinaryCallbackContext<'g>>> {
        let mut parameters = None;
        let mut result = TypeProjection {
            ty: None,
            incompatible: false,
        };
        if let TypeTerm::Callable {
            parameters: inputs,
            result: output,
            ..
        } = receiver.as_ref()
        {
            if !self.merge_callable_context(
                header,
                substitution,
                &mut parameters,
                &mut result,
                inputs,
                output,
            )? {
                return Ok(None);
            }
        } else if let TypeTerm::Parameter { declaration, .. } = receiver.as_ref() {
            self.elab.step(substitution.len())?;
            if let Some(sub) = substitution.iter().find(|s| s.binder == *declaration)
                && let TypeTerm::Callable {
                    parameters: inputs,
                    result: output,
                    ..
                } = sub.argument.as_ref()
                && !self.merge_callable_context(
                    header,
                    &[],
                    &mut parameters,
                    &mut result,
                    inputs,
                    output,
                )?
            {
                return Ok(None);
            }
        }
        let ordinary = self.input.ordinary.unwrap();
        if let Some(metadata) = ordinary.headers().get(&header.declaration.id) {
            for requirement in &metadata.requirements {
                self.elab.step(1)?;
                if !matches!(requirement.target,
                    Some(RequirementTarget::Predicate(id)) if matches!(id.name(), "callable" | "callableMut" | "callableOnce"))
                {
                    continue;
                }
                let [Some(subject), Some(inputs), Some(output)] = requirement.arguments.as_slice()
                else {
                    continue;
                };
                if !self.same(subject, receiver)? {
                    continue;
                }
                let TypeTerm::Tuple(inputs) = inputs.as_ref() else {
                    continue;
                };
                if !self.merge_callable_context(
                    header,
                    substitution,
                    &mut parameters,
                    &mut result,
                    inputs,
                    output,
                )? {
                    return Ok(None);
                }
            }
        }
        let Some(parameters) = parameters else {
            return Ok(None);
        };
        self.elab.reserve(parameters.len() + 1)?;
        Ok(Some(OrdinaryCallbackContext {
            argument,
            parameters: parameters
                .into_iter()
                .map(|p| (!p.incompatible).then_some(p.ty).flatten())
                .collect(),
            result: (!result.incompatible).then_some(result.ty).flatten(),
        }))
    }

    fn merge_callable_context(
        &mut self,
        header: &DeclarationFacts<'g>,
        substitution: &[Substitution<'g>],
        parameters: &mut Option<Vec<TypeProjection<'g>>>,
        result: &mut TypeProjection<'g>,
        inputs: &[Type<'g>],
        output: &Type<'g>,
    ) -> Work<bool> {
        if parameters.is_none() {
            self.elab.reserve(inputs.len())?;
            *parameters = Some(
                inputs
                    .iter()
                    .map(|_| TypeProjection {
                        ty: None,
                        incompatible: false,
                    })
                    .collect(),
            );
        }
        let parameters = parameters.as_mut().unwrap();
        if parameters.len() != inputs.len() {
            return Ok(false);
        }
        let mut bound = BTreeSet::new();
        for sub in substitution {
            self.elab.reserve(1)?;
            bound.insert(sub.position);
        }
        for (projection, term) in parameters
            .iter_mut()
            .zip(inputs)
            .chain(std::iter::once((result, output)))
        {
            if self.closed(term, &header.binders, &|i| bound.contains(&i), 1)? {
                let proposed = self.substitute(term, substitution, 1)?;
                if let Some(ref previous) = projection.ty
                    && !self.same(previous, &proposed)?
                {
                    projection.incompatible = true;
                } else {
                    projection.ty = Some(proposed);
                }
            }
        }
        Ok(true)
    }

    pub(super) fn intersect_callable_contexts(
        &mut self,
        available: &mut OrdinaryCallbackContext<'g>,
        required: &OrdinaryCallbackContext<'g>,
    ) -> Work<bool> {
        if available.parameters.len() != required.parameters.len() {
            return Ok(false);
        }
        for (a, r) in available
            .parameters
            .iter_mut()
            .zip(&required.parameters)
            .chain(std::iter::once((&mut available.result, &required.result)))
        {
            if a.is_some()
                && (r.is_none() || !self.same(a.as_ref().unwrap(), r.as_ref().unwrap())?)
            {
                *a = None;
            }
        }
        Ok(true)
    }
}
