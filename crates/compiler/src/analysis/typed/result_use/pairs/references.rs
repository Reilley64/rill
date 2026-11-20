//! Result residence and exact origins for the installed pair of shared i32 results.
use super::*;
use crate::analysis::generics::callables::ArrowFacts;

fn i32_type(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
}

pub(super) fn shared_i32(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if i32_type(referent))
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn reference_child(
        &mut self,
        origin: NodeId,
        arrow: &ArrowFacts<'g>,
        stored: (&[Located<'g>], ResultComponent<'g>),
        caller: &Residence<'g>,
        depth: usize,
    ) -> Work<Option<(NodeId, ScopedJoinReference<'g>)>> {
        self.depth(depth)?;
        if arrow.mode != Some(InvocationMode::ReadOnly) {
            return Ok(None);
        }
        let nodes = &self.module.syntax.nodes;
        let body = nodes[origin].children[1];
        let (expression, binding, capture, reference) = match nodes[body].kind {
            NodeKind::ValueReference => {
                let [capture] = arrow.captures.as_slice() else {
                    return Ok(None);
                };
                let parameter = capture.path.root;
                if capture.site.node != body
                    || !std::ptr::eq(capture.site.module, &self.module.key)
                    || !capture.reachable
                    || capture.through.is_some()
                    || !capture.path.projections.is_empty()
                    || capture.form != (CaptureForm::CarriedReference { mutable: false })
                    || capture.access != Access::Owned
                    || capture.storage.as_ref() != Some(&arrow.result)
                    || !caller
                        .contents(Root::Local(parameter), &[], self.budget)?
                        .is_empty()
                {
                    return Ok(None);
                }
                self.budget.step(self.current_parameters.len())?;
                if !self
                    .current_parameters
                    .iter()
                    .any(|p| std::ptr::eq(&p.declaration.id, parameter) && p.ty == arrow.result)
                    || !self.expression_facts(body)?.is_some_and(|value| {
                        value.binding.is_some_and(|id| std::ptr::eq(id, parameter))
                            && value.access == Access::Owned
                    })
                {
                    return Ok(None);
                }
                // The value of an incoming descriptor, not a borrow of its slot.
                (
                    body,
                    None,
                    Some(parameter),
                    ScopedJoinReference::Incoming {
                        parameter,
                        value: self.site(body),
                    },
                )
            }
            NodeKind::Block if arrow.captures.is_empty() => {
                let [binding, returning] = nodes[body].children.as_slice() else {
                    return Ok(None);
                };
                let (NodeKind::Binding { mutable: false }, [name, initializer]) =
                    (nodes[*binding].kind, nodes[*binding].children.as_slice())
                else {
                    return Ok(None);
                };
                let (NodeKind::Return, [expression]) = (
                    nodes[*returning].kind,
                    nodes[*returning].children.as_slice(),
                ) else {
                    return Ok(None);
                };
                if nodes[*name].kind != NodeKind::NamePattern
                    || nodes[*initializer].kind != NodeKind::Literal
                    || nodes[*expression].kind != NodeKind::Prefix("&")
                    || !self.facts(*returning)?.is_some_and(|f| {
                        matches!(f.kind, NodeFactKind::ArrowReturn { target } if target == origin)
                    })
                {
                    return Ok(None);
                }
                let Some(NodeFactKind::Binding(local)) = self.facts(*binding)?.map(|f| &f.kind)
                else {
                    return Ok(None);
                };
                if !local.ty.as_ref().is_some_and(i32_type) {
                    return Ok(None);
                }
                (
                    *expression,
                    Some((*binding, local.declaration)),
                    None,
                    ScopedJoinReference::ChildLocal {
                        binding: self.site(*binding),
                        returning: self.site(*returning),
                    },
                )
            }
            _ => return Ok(None),
        };
        if !self.pair_environment(origin, stored, capture)?
            || !self
                .expression_facts(expression)?
                .is_some_and(|value| value.ordinary_context.is_none() && value.ty == arrow.result)
        {
            return Ok(None);
        }
        let mut proof = self.callable_check(self.module)?;
        let mut entry = Residence::default();
        if let Some((binding, _)) = binding {
            let mut states = proof.statement(binding, entry, depth + 1)?;
            if states.len() != 1 {
                return Ok(None);
            }
            entry = states.pop().unwrap();
        }
        let expression_depth = depth + if binding.is_some() { 2 } else { 1 };
        let mut states = proof.expression(expression, entry, expression_depth)?;
        let [state] = states.as_mut_slice() else {
            return Ok(None);
        };
        let values = state.take(Root::Temporary(expression), &[], proof.budget)?;
        let residence = match (binding, values.as_slice()) {
            (None, []) => true,
            (Some((_, local)), [value]) => {
                value.component.is_empty()
                    && matches!(&value.content, Content::Alias((Root::Local(root), path))
                    if std::ptr::eq(*root, local) && path.is_empty())
            }
            _ => false,
        };
        // Neither descriptor carries a Result occurrence. Keep the exact
        // backing origin separately: a child-local address is not caller storage.
        Ok(
            (residence && !proof.has_pending && !state.may_owe_result(proof.budget)?)
                .then_some((expression, reference)),
        )
    }
}
