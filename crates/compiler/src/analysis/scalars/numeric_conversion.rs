//! Compiler conversion operations at the shared pair determination boundary.
use super::*;
use crate::analysis::typed::{
    ConversionProtection, ConversionProvider, InfallibleNumericContract, NumericConversion,
    Primitive,
};

pub(super) struct ProtectedConversion {
    pub protection: ConversionProtection,
    pub provider: Option<ConversionProvider>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn installed_conversion(
        &self,
        family: &DeclarationId,
    ) -> bool {
        self.installed_conversions
            .is_some_and(|core| core.families.contains_key(family))
    }

    pub(super) fn is_linked_bridge(&self, id: &DeclarationId) -> bool {
        self.collections.is_bridge(id)
            || self
                .installed_conversions
                .is_some_and(|core| core.bridges.contains_key(id))
    }

    pub(super) fn protected_conversion(
        &mut self,
        fallible: bool,
        destination: &Type<'g>,
        source: &Type<'g>,
    ) -> Work<Option<ProtectedConversion>> {
        use ConversionProtection::*;
        if !fallible && self.same(destination, source)? {
            return Ok(Some(ProtectedConversion {
                protection: RequiredIdentity,
                provider: Some(ConversionProvider::Identity),
            }));
        }
        let primitive = |ty: &Type<'g>| primitive_name(ty).and_then(Primitive::named);
        if let Some((to, from)) = primitive(destination).zip(primitive(source))
            && let Some(contract) = InfallibleNumericContract::for_pair(to, from)
        {
            if !fallible && contract == InfallibleNumericContract::Gap {
                return Ok(Some(ProtectedConversion {
                    protection: ProhibitedNumeric,
                    provider: None,
                }));
            }
            let operation = NumericConversion::for_pair(to, from);
            let provider = operation.map(if fallible {
                ConversionProvider::NumericTryInto
            } else {
                ConversionProvider::NumericInto
            });
            return Ok(Some(ProtectedConversion {
                protection: RequiredNumeric,
                provider,
            }));
        }
        Ok(None)
    }

    pub(super) fn explicit_conversion_source_context(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<bool> {
        // An independently typed source is never retagged by the written filter.
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
}
