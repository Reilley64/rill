use super::declarations::{Budget, FieldClass};
use super::{DeclarationShape, Type, TypeTerm, ordinary};
use crate::loader::{DeclarationId, keyed};

pub(super) struct EligibleRecord<'a, 'g> {
    contents: &'a Type<'g>,
    declaration: &'g DeclarationId,
}

pub(super) fn screen_i32_record<'a, 'g>(
    inventory: &ordinary::OrdinaryAnalysis<'g>,
    contents: &'a Type<'g>,
) -> Option<EligibleRecord<'a, 'g>> {
    let TypeTerm::Reference { referent, .. } = contents.as_ref() else {
        return None;
    };
    let TypeTerm::Nominal { declaration, .. } = referent.as_ref() else {
        return None;
    };
    if matches!(
        inventory.field_class(declaration),
        FieldClass::Text | FieldClass::Other
    ) {
        return None;
    }
    Some(EligibleRecord {
        contents,
        declaration,
    })
}

pub(super) fn prove_i32_record<'a, 'g>(
    eligible: EligibleRecord<'a, 'g>,
    inventory: &ordinary::OrdinaryAnalysis<'g>,
    budget: &mut Budget,
) -> Result<Option<&'a Type<'g>>, ()> {
    let EligibleRecord {
        contents,
        declaration,
    } = eligible;
    let declarations = &inventory.type_facts().declarations;
    keyed::charge(&declaration.module, declarations.len(), budget)?;
    let Some(facts) = declarations.get(declaration) else {
        return Ok(None);
    };
    let DeclarationShape::Record { fields } = &facts.shape else {
        return Ok(None);
    };
    if !facts.binders.is_empty() {
        return Ok(None);
    }
    budget.step(fields.len())?;
    Ok(fields
        .iter()
        .all(|field| matches!(field.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))
        .then_some(contents))
}
