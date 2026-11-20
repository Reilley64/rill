//! Result-use evidence for a terminal write through two incoming exclusive layers.
use super::*;
use crate::analysis::scalars::Place;

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn terminal_nested_bool_write(
        &mut self,
        node: NodeId,
        target: &ExpressionFacts<'g>,
        state: &Residence<'g>,
        incoming: &[Located<'g>],
        depth: usize,
    ) -> Work<bool> {
        let Some(path) = &target.path else {
            return Ok(false);
        };
        let [PathProjection::Dereference, PathProjection::Dereference] =
            path.projections.as_slice()
        else {
            return Ok(false);
        };
        let Some(body) = self.nested_write_body else {
            return Ok(false);
        };
        let nodes = &self.module.syntax.nodes;
        self.budget.step(1)?;
        // No earlier writes, calls, or reborrows can redirect either hop.
        let local = match nodes[body].children.as_slice() {
            [store] if *store == node => None,
            [binding, store] if *store == node => Some(*binding),
            _ => return Ok(false),
        };
        let Some(slot_ty) = self.incoming_parameter_type(path.root)? else {
            return Ok(false);
        };
        let TypeTerm::Reference {
            mutable: true,
            referent: inner_ty,
        } = slot_ty.as_ref()
        else {
            return Ok(false);
        };
        let Some(shared_ty) = retained_bool_contents_type(inner_ty) else {
            return Ok(false);
        };
        let [left, right] = nodes[node].children.as_slice() else {
            return Ok(false);
        };
        self.depth(depth + 1)?;
        let (NodeKind::Prefix("*"), [inner]) =
            (nodes[*left].kind, nodes[*left].children.as_slice())
        else {
            return Ok(false);
        };
        if !writable_dereference(target, path.root, &path.projections, shared_ty) {
            return Ok(false);
        }
        self.depth(depth + 2)?;
        let (NodeKind::Prefix("*"), [slot]) =
            (nodes[*inner].kind, nodes[*inner].children.as_slice())
        else {
            return Ok(false);
        };
        if !self.expression_facts(*inner)?.is_some_and(|value| {
            writable_dereference(value, path.root, &[PathProjection::Dereference], inner_ty)
        }) {
            return Ok(false);
        }
        self.depth(depth + 3)?;
        if self.named_place(*slot, slot_ty)? != Some(path.root)
            || !state
                .contents(Root::Local(path.root), &[], self.budget)?
                .is_empty()
        {
            return Ok(false);
        }
        // Both checked steps start from the unchanged incoming descriptor.
        // The intermediate descriptor, displaced value and final referent
        // have closed Bool/reference types, not unknown residence types.
        self.depth(depth + 1)?;
        match local {
            None => {
                let Some(input) = self.named_place(*right, shared_ty)? else {
                    return Ok(false);
                };
                Ok(incoming.is_empty()
                    && self.incoming_parameter_type(input)? == Some(shared_ty)
                    && state
                        .contents(Root::Local(input), &[], self.budget)?
                        .is_empty())
            }
            Some(binding) => {
                let (NodeKind::Prefix("&"), [flag]) =
                    (nodes[*right].kind, nodes[*right].children.as_slice())
                else {
                    return Ok(false);
                };
                if !self
                    .expression_facts(*right)?
                    .is_some_and(|rhs| &rhs.ty == shared_ty)
                {
                    return Ok(false);
                }
                let TypeTerm::Reference {
                    referent: bool_ty, ..
                } = shared_ty.as_ref()
                else {
                    return Ok(false);
                };
                self.depth(depth + 2)?;
                let Some(flag) = self.named_place(*flag, bool_ty)? else {
                    return Ok(false);
                };
                self.budget.step(2)?;
                let (NodeKind::Binding { mutable: false }, [name, initializer]) =
                    (nodes[binding].kind, nodes[binding].children.as_slice())
                else {
                    return Ok(false);
                };
                if nodes[*name].kind != NodeKind::NamePattern
                    || nodes[*initializer].kind != NodeKind::Literal
                    || !self.expression_facts(*initializer)?.is_some_and(|value| &value.ty == bool_ty)
                    || !self.facts(binding)?.is_some_and(|facts| {
                        matches!(&facts.kind, NodeFactKind::Binding(b) if b.declaration == flag && b.ty.as_ref() == Some(bool_ty))
                    })
                {
                    return Ok(false);
                }
                let [value] = incoming else {
                    return Ok(false);
                };
                let root = Root::Local(flag);
                // Require the actual borrow address. The typed operand remains
                // available to ownership; this does not install a local residence.
                if !value.component.is_empty()
                    || !matches!(&value.content, Content::Alias((backing, path)) if *backing == root && path.is_empty())
                {
                    return Ok(false);
                }
                let contents = state.contents(root, &[], self.budget)?;
                Ok(
                    matches!(contents.as_slice(), [value] if value.component.is_empty()
                    && matches!(&value.content, Content::Test(tag, _) if matches!(tag.as_ref(), Constructor::Boolean(_)))),
                )
            }
        }
    }

    fn incoming_parameter_type(&mut self, root: &DeclarationId) -> Work<Option<&'a Type<'g>>> {
        if root.kind != DeclarationKind::Parameter {
            return Ok(None);
        }
        self.budget.step(self.current_parameters.len())?;
        Ok(self
            .current_parameters
            .iter()
            .find_map(|p| (&p.declaration.id == root).then_some(&p.ty)))
    }

    fn named_place(&mut self, node: NodeId, ty: &Type<'g>) -> Work<Option<&'g DeclarationId>> {
        self.budget.step(1)?;
        if self.module.syntax.nodes[node].kind != NodeKind::ValueReference {
            return Ok(None);
        }
        Ok(self.expression_facts(node)?.and_then(|value| {
            let path = value.path.as_ref()?;
            (&value.ty == ty
                && value.binding == Some(path.root)
                && path.projections.is_empty()
                && value.refinement.is_none())
            .then_some(path.root)
        }))
    }
}

fn writable_dereference<'g>(
    value: &ExpressionFacts<'g>,
    root: &DeclarationId,
    projections: &[PathProjection<'g>],
    ty: &Type<'g>,
) -> bool {
    &value.ty == ty
        && value.place == Place::Writable
        && value.access == Access::Writable
        && !value.read_only_projection
        && value.refinement.is_none()
        && value
            .path
            .as_ref()
            .is_some_and(|path| path.root == root && path.projections == projections)
}
