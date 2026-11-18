//! Implicit-copy classification for borrowed binders and by-value captures.
use super::*;
use crate::analysis::typed::{OwnershipCoverage, OwnershipRule};
use crate::loader::Origin;

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn implicit_copy(&mut self, site: Site<'g>, ty: &Type<'g>) -> Work<Option<bool>> {
        self.classify_copy(site, ty, 1)
    }

    fn classify_copy(&mut self, site: Site<'g>, ty: &Type<'g>, depth: usize) -> Work<Option<bool>> {
        let inventory = self.input.ordinary.filter(|_| self.integrated);
        if inventory.is_some() {
            self.elab.step(1)?;
            if depth > self.elab.limits.max_depth.min(64) {
                return Err(());
            }
            self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        }
        let (head, variant) = match ty.as_ref() {
            TypeTerm::Primitive(p) => return Ok(Some(p.name() != "string")),
            TypeTerm::Reference { mutable, .. } => return Ok(Some(!mutable)),
            // Explicit copy evidence never changes the rigid body rule.
            TypeTerm::Parameter { .. } => return Ok(Some(false)),
            TypeTerm::Variant {
                parent,
                declaration,
            } => (parent, Some(*declaration)),
            _ => (ty, None),
        };
        let Some(inventory) = inventory else {
            return Ok(None);
        };

        let stored = match head.as_ref() {
            TypeTerm::Tuple(elements) => {
                self.copy_components(site, elements.iter(), &[], depth + 1)?
            }
            TypeTerm::Nominal {
                declaration,
                arguments,
            } => {
                let Some(facts) = inventory.type_facts().declarations.get(declaration) else {
                    return Ok(None);
                };
                if self.declaration_unusable(declaration)? || facts.binders.len() != arguments.len()
                {
                    return Ok(None);
                }
                if self.collections.representation(head).is_some()
                    || self
                        .installed_conversions
                        .and_then(|core| core.bytes_representation())
                        .is_some_and(|bytes| &bytes.id == *declaration)
                {
                    return Ok(Some(false));
                }
                if let Some(role) = self.result
                    && (role.cause.id == **declaration || role.wrapper.id == **declaration)
                {
                    return Ok(role.representation.map(|_| false));
                }
                // These canonical regions cover all applications of the validated
                // owner head. Neither a missing action nor a source seal is evidence.
                let prelude = self.input.graph.prelude();
                let mut protected = false;
                for scope in self.native_ownership {
                    self.elab.step(1)?;
                    if &scope.owner.id != *declaration {
                        continue;
                    }
                    protected = true;
                    if scope.coverage == OwnershipCoverage::Clear
                        && (scope.family == &prelude.copy
                            && scope.rule == OwnershipRule::ProhibitedCopy
                            || scope.family == &prelude.dispose && scope.provider.is_some())
                    {
                        return Ok(Some(false));
                    }
                }
                if protected {
                    return Ok(None);
                }
                let substitutions = self.nominal_substitution(&facts.binders, arguments)?;
                match &facts.shape {
                    DeclarationShape::Record { fields } => self.copy_components(
                        site,
                        fields.iter().map(|field| &field.ty),
                        &substitutions,
                        depth + 1,
                    )?,
                    DeclarationShape::Enum { variants } => {
                        // A standard source seal is not a summary of opaque storage.
                        if declaration.module.origin == Origin::Standard && variants.is_empty() {
                            return Ok(None);
                        }
                        let mut stored = Some(true);
                        for item in variants {
                            self.elab.step(1)?;
                            if variant.is_some_and(|id| *id != item.declaration.id) {
                                continue;
                            }
                            let payload = match &item.payload {
                                Payload::Unit => Some(true),
                                Payload::Tuple(types) => self.copy_components(
                                    site,
                                    types.iter(),
                                    &substitutions,
                                    depth + 1,
                                )?,
                                Payload::Record(fields) => self.copy_components(
                                    site,
                                    fields.iter().map(|field| &field.ty),
                                    &substitutions,
                                    depth + 1,
                                )?,
                            };
                            if payload == Some(false) {
                                return Ok(Some(false));
                            }
                            stored = stored.and(payload);
                        }
                        stored
                    }
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        if stored == Some(false) {
            return Ok(stored);
        }
        Ok(match self.aggregate_has_disposer(site, ty)? {
            Some(true) => Some(false),
            Some(false) => stored,
            None => None,
        })
    }

    fn copy_components<'a>(
        &mut self,
        site: Site<'g>,
        types: impl Iterator<Item = &'a Type<'g>>,
        substitutions: &[Substitution<'g>],
        depth: usize,
    ) -> Work<Option<bool>>
    where
        'g: 'a,
    {
        let mut stored = Some(true);
        for ty in types {
            self.elab.step(1)?;
            let ty =
                if substitutions.is_empty() || matches!(ty.as_ref(), TypeTerm::Reference { .. }) {
                    ty.clone()
                } else {
                    self.substitute(ty, substitutions, depth)?
                };
            let component = self.classify_copy(site, &ty, depth)?;
            if component == Some(false) {
                return Ok(component);
            }
            stored = stored.and(component);
        }
        Ok(stored)
    }
}
