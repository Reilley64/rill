//! Direct calls on the admitted ground numeric slice of std::convert.
use super::*;
use crate::analysis::typed::{InfallibleNumericContract, NumericConversion, Primitive};

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn is_installed_into(&self, family: &DeclarationId) -> bool {
        self.installed_into
            .is_some_and(|into| into.declaration.id == *family)
    }

    pub(super) fn numeric_into_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<()> {
        let module = inference.module;
        let call = &inference.calls[&node];
        let [argument] = call.fact.arguments.as_slice() else {
            self.finding(module, node, ScalarFindingKind::Arity)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        let argument = *argument;
        let source = &inference.slots[&argument];
        let filter = call.fact.substitution.get(1).map(|s| s.argument.clone());
        if source.blocked {
            return Ok(());
        }
        let Some(source) = source.ty.clone() else {
            return Ok(());
        };
        let Some(destination) = call
            .fact
            .substitution
            .first()
            .map(|s| &s.argument)
            .or(inference.slots[&node].expected.as_ref())
            .cloned()
        else {
            return Ok(());
        };
        let Some(to) = primitive_name(&destination).and_then(Primitive::named) else {
            return Ok(());
        };
        let Some(from) = primitive_name(&source).and_then(Primitive::named) else {
            return Ok(());
        };
        let Some(contract) = InfallibleNumericContract::for_pair(to, from) else {
            return Ok(());
        };
        if !self.installed_into.unwrap().issues.is_empty() {
            // The captured registrations did not pass independent protection checks.
            return Ok(());
        }
        let filter_agrees = match filter {
            Some(ref filter) => self.same(filter, &source)?,
            None => true,
        };
        if contract == InfallibleNumericContract::Gap || !filter_agrees {
            let reason = if contract == InfallibleNumericContract::Gap {
                ScalarFindingKind::ProtectedNumericGap
            } else {
                ScalarFindingKind::MissingConversionPair
            };
            self.finding(module, node, reason)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let Some(operation) = NumericConversion::for_pair(to, from) else {
            return Ok(()); // Required provider is missing, not a gap.
        };
        let call = inference.calls.get_mut(&node).unwrap();
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap();
        self.elab.reserve(3)?;
        call.fact.substitution = family
            .header
            .binders
            .iter()
            .zip([destination.clone(), source.clone()])
            .map(|(binder, argument)| Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument,
            })
            .collect();
        call.parameters[0] = source;
        call.result = destination.clone();
        call.family = false;
        call.infer = false;
        self.record_callee(module, call.callee_node, call.fact.callee)?;
        inference.slots.get_mut(&node).unwrap().installed_operation =
            Some(data::DataExpression::NumericInto(operation));
        self.establish(inference, node, destination, false)
    }

    pub(super) fn explicit_into_source_context(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<bool> {
        // First allow source inference with no conversion-specific context.
        // A suffix or other independently established width is never retagged.
        let call = &inference.calls[&node];
        let ([argument], Some(filter)) = (
            call.fact.arguments.as_slice(),
            call.fact.substitution.get(1),
        ) else {
            return Ok(false);
        };
        let argument = *argument;
        let slot = &inference.slots[&argument];
        if inference.slots[&node].blocked
            || slot.blocked
            || slot.ty.is_some()
            || slot.expected.is_some()
        {
            return Ok(false);
        }
        let filter = filter.argument.clone();
        self.establish(inference, argument, filter, true)?;
        Ok(true)
    }

    pub(super) fn finish_installed_into(&mut self, inference: &mut Inference<'g>) -> Work<()> {
        if self.installed_into.is_none() {
            return Ok(());
        }
        for (&node, call) in &mut inference.calls {
            self.elab.step(1)?;
            if !call.family || !self.is_installed_into(call.fact.callee) {
                continue;
            }
            call.family = false; // Never fall back to ordinary dispatch.
            if !inference.slots[&node].blocked {
                let [argument] = call.fact.arguments.as_slice() else {
                    unreachable!()
                };
                let source = &inference.slots[argument];
                if source.ty.is_some() && !source.blocked {
                    if call.fact.substitution.is_empty()
                        && inference.slots[&node].expected.is_none()
                    {
                        self.finding(
                            inference.module,
                            node,
                            ScalarFindingKind::InsufficientContext,
                        )?;
                    } else {
                        self.defer(
                            inference.module,
                            node,
                            ScalarUnsupported::ConversionAdmission,
                        )?;
                    }
                }
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
        }
        Ok(())
    }
}
