//! Negative implicit-copy evidence from actual source components.
use super::*;
use crate::analysis::typed::{OwnershipCoverage, OwnershipProtection, OwnershipRule};

#[derive(Clone, Copy)]
pub(super) struct Inputs<'r, 'g> {
    inventory: &'r ordinary::OrdinaryAnalysis<'g>,
    native: &'r [OwnershipProtection<'g>],
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn copy_inputs(&self) -> Option<Inputs<'r, 'g>> {
        Some(Inputs {
            inventory: self.input.ordinary.filter(|_| self.integrated)?,
            native: self.native_ownership,
        })
    }
}

struct Arguments<'a, 'g> {
    binders: &'a [Binder<'g>],
    types: &'a [Type<'g>],
    outer: Option<&'a Arguments<'a, 'g>>,
}

pub(super) fn implicit_copy<'g>(
    elab: &mut Analyzer<'g>,
    inputs: Option<Inputs<'_, 'g>>,
    ty: &Type<'g>,
) -> Work<Option<bool>> {
    let Some(inputs) = inputs else {
        return Ok(leaf(ty));
    };
    classify(elab, inputs, ty, None, 1)
}

fn classify<'g>(
    elab: &mut Analyzer<'g>,
    inputs: Inputs<'_, 'g>,
    ty: &Type<'g>,
    arguments: Option<&Arguments<'_, 'g>>,
    depth: usize,
) -> Work<Option<bool>> {
    elab.step(1)?;
    if depth > elab.limits.max_depth.min(64) {
        return Err(());
    }
    elab.statistics.max_depth = elab.statistics.max_depth.max(depth);
    if let TypeTerm::Parameter {
        declaration,
        position,
    } = ty.as_ref()
    {
        let mut frame = arguments;
        while let Some(current) = frame {
            elab.step(1)?;
            if current
                .binders
                .get(*position)
                .is_some_and(|binder| binder.declaration.id == **declaration)
            {
                return classify(
                    elab,
                    inputs,
                    &current.types[*position],
                    current.outer,
                    depth + 1,
                );
            }
            frame = current.outer;
        }
        return Ok(Some(false));
    }
    if let Some(value) = leaf(ty) {
        return Ok(Some(value));
    }
    let (head, variant) = match ty.as_ref() {
        TypeTerm::Variant {
            parent,
            declaration,
        } => (parent.as_ref(), Some(*declaration)),
        head => (head, None),
    };
    match head {
        TypeTerm::Tuple(elements) => {
            if has_noncopying(elab, inputs, elements.iter(), arguments, depth + 1)? {
                return Ok(Some(false));
            }
        }
        TypeTerm::Nominal {
            declaration,
            arguments: types,
        } => {
            elab.step(2)?;
            let Some(facts) = inputs.inventory.type_facts().declarations.get(declaration) else {
                return Ok(None);
            };
            if inputs.inventory.declaration_unusable(declaration)
                || facts.binders.len() != types.len()
            {
                return Ok(None);
            }
            elab.step(inputs.native.len())?;
            let dispose = &inputs.inventory.graph().prelude().dispose;
            if inputs.native.iter().any(|scope| {
                &scope.owner.id == *declaration
                    && scope.coverage == OwnershipCoverage::Clear
                    && (scope.rule == OwnershipRule::ProhibitedCopy
                        || scope.family == dispose && scope.provider.is_some())
            }) {
                return Ok(Some(false));
            }
            let frame = Arguments {
                binders: &facts.binders,
                types,
                outer: arguments,
            };
            match &facts.shape {
                DeclarationShape::Record { fields } => {
                    if has_noncopying(
                        elab,
                        inputs,
                        fields.iter().map(|field| &field.ty),
                        Some(&frame),
                        depth + 1,
                    )? {
                        return Ok(Some(false));
                    }
                }
                DeclarationShape::Enum { variants } => {
                    for candidate in variants {
                        elab.step(1)?;
                        if variant.is_some_and(|id| *id != candidate.declaration.id) {
                            continue;
                        }
                        let negative = match &candidate.payload {
                            Payload::Unit => false,
                            Payload::Tuple(types) => {
                                has_noncopying(elab, inputs, types.iter(), Some(&frame), depth + 1)?
                            }
                            Payload::Record(fields) => has_noncopying(
                                elab,
                                inputs,
                                fields.iter().map(|field| &field.ty),
                                Some(&frame),
                                depth + 1,
                            )?,
                        };
                        if negative {
                            return Ok(Some(false));
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    // Field evidence can disprove copying. It cannot establish the absence of
    // a custom disposer or an opaque or protected storage contract.
    Ok(None)
}

fn has_noncopying<'a, 'g: 'a>(
    elab: &mut Analyzer<'g>,
    inputs: Inputs<'_, 'g>,
    types: impl Iterator<Item = &'a Type<'g>>,
    arguments: Option<&Arguments<'_, 'g>>,
    depth: usize,
) -> Work<bool> {
    for ty in types {
        if classify(elab, inputs, ty, arguments, depth)? == Some(false) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn leaf(ty: &Type<'_>) -> Option<bool> {
    match ty.as_ref() {
        TypeTerm::Primitive(p) => Some(p.name() != "string"),
        TypeTerm::Reference { mutable, .. } => Some(!mutable),
        TypeTerm::Parameter { .. } => Some(false),
        _ => None,
    }
}
