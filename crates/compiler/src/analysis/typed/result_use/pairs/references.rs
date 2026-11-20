//! Result residence and exact origins for the installed pair of shared i32 results.
use super::*;
use crate::analysis::generics::callables::ArrowFacts;

fn i32_type(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
}

pub(super) fn shared_i32(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if i32_type(referent))
}

#[derive(Clone, Copy)]
pub(super) enum ReferenceSource {
    Incoming {
        value: NodeId,
    },
    ChildLocal {
        binding: NodeId,
        returning: NodeId,
        operand: NodeId,
    },
}

#[derive(Clone, Copy)]
pub(super) struct ReferenceShape {
    pub origin: NodeId,
    expression: NodeId,
    source: ReferenceSource,
}

#[derive(Clone, Copy)]
pub(super) struct ReferencePlan<'g> {
    pub expression: NodeId,
    pub reference: ScopedJoinReference<'g>,
    binding: Option<(NodeId, &'g DeclarationId)>,
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn reference_shape(&mut self, element: NodeId) -> Work<Option<ReferenceShape>> {
        let origin = self.ungroup_value(element)?;
        let nodes = &self.module.syntax.nodes;
        if nodes[origin].kind != NodeKind::Arrow {
            return Ok(None);
        }
        let body = nodes[origin].children[1];
        let root = self.ungroup_value(body)?;
        let (expression, source) = match nodes[root].kind {
            NodeKind::ValueReference => (body, ReferenceSource::Incoming { value: root }),
            NodeKind::Block => {
                self.budget.step(4)?;
                let [binding, returning] = nodes[root].children.as_slice() else {
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
                let borrowed = self.ungroup_value(*expression)?;
                if nodes[*name].kind != NodeKind::NamePattern
                    || nodes[self.ungroup_value(*initializer)?].kind != NodeKind::Literal
                    || nodes[borrowed].kind != NodeKind::Prefix("&")
                {
                    return Ok(None);
                };
                let operand = self.ungroup_value(nodes[borrowed].children[0])?;
                if nodes[operand].kind != NodeKind::ValueReference {
                    return Ok(None);
                };
                (
                    *expression,
                    ReferenceSource::ChildLocal {
                        binding: *binding,
                        returning: *returning,
                        operand,
                    },
                )
            }
            _ => return Ok(None),
        };
        Ok(Some(ReferenceShape {
            origin,
            expression,
            source,
        }))
    }

    pub(super) fn reference_plan(
        &mut self,
        shape: ReferenceShape,
        arrow: &ArrowFacts<'g>,
        stored: (&[Located<'g>], ResultComponent<'g>),
        caller: &Residence<'g>,
        depth: usize,
    ) -> Work<Option<ReferencePlan<'g>>> {
        self.depth(depth)?;
        if arrow.mode != Some(InvocationMode::ReadOnly) {
            return Ok(None);
        }
        let ReferenceShape {
            origin,
            expression,
            source,
        } = shape;
        let (binding, capture, reference) = match source {
            ReferenceSource::Incoming { value: root } => {
                let [capture] = arrow.captures.as_slice() else {
                    return Ok(None);
                };
                let parameter = capture.path.root;
                if !std::ptr::eq(capture.site.module, &self.module.key)
                    || self.ungroup_value(capture.site.node)? != root
                    || !capture.reachable
                    || capture.through.is_some()
                    || !capture.path.projections.is_empty()
                    || capture.form != (CaptureForm::CarriedReference { mutable: false })
                    || capture.access != Access::Owned
                    || capture.storage.as_ref() != Some(&arrow.result)
                    || caller.contains_local(parameter, self.budget)?
                {
                    return Ok(None);
                }
                self.budget.step(self.current_parameters.len())?;
                if !self
                    .current_parameters
                    .iter()
                    .any(|p| std::ptr::eq(&p.declaration.id, parameter) && p.ty == arrow.result)
                    || !self.expression_facts(root)?.is_some_and(|value| {
                        value.binding.is_some_and(|id| std::ptr::eq(id, parameter))
                            && value.access == Access::Owned
                    })
                {
                    return Ok(None);
                }
                // The value of an incoming descriptor, not a borrow of its slot.
                (
                    None,
                    Some(parameter),
                    ScopedJoinReference::Incoming {
                        parameter,
                        value: self.site(root),
                    },
                )
            }
            ReferenceSource::ChildLocal {
                binding,
                returning,
                operand,
            } if arrow.captures.is_empty() => {
                if !self.facts(returning)?.is_some_and(
                    |f| matches!(f.kind, NodeFactKind::ArrowReturn { target } if target == origin),
                ) {
                    return Ok(None);
                }
                let Some(NodeFactKind::Binding(local)) = self.facts(binding)?.map(|f| &f.kind)
                else {
                    return Ok(None);
                };
                if !self.expression_facts(operand)?.is_some_and(|v| {
                    v.ordinary_context.is_none()
                        && v.access == Access::Owned
                        && v.path.as_ref().is_some_and(|p| {
                            std::ptr::eq(p.root, local.declaration) && p.projections.is_empty()
                        })
                }) || !local.ty.as_ref().is_some_and(i32_type)
                {
                    return Ok(None);
                }
                (
                    Some((binding, local.declaration)),
                    None,
                    ScopedJoinReference::ChildLocal {
                        binding: self.site(binding),
                        returning: self.site(returning),
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
        Ok(Some(ReferencePlan {
            expression,
            binding,
            reference,
        }))
    }

    pub(super) fn prove_reference_plan(
        &mut self,
        plan: ReferencePlan<'g>,
        depth: usize,
    ) -> Work<bool> {
        let ReferencePlan {
            expression,
            binding,
            ..
        } = plan;
        let mut proof = self.callable_check(self.module)?;
        let mut entry = Residence::default();
        if let Some((binding, _)) = binding {
            let mut states = proof.statement(binding, entry, depth + 1)?;
            if states.len() != 1 {
                return Ok(false);
            }
            entry = states.pop().unwrap();
        }
        let expression_depth = depth + if binding.is_some() { 2 } else { 1 };
        let mut states = proof.expression(expression, entry, expression_depth)?;
        let [state] = states.as_mut_slice() else {
            return Ok(false);
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
        Ok(residence && !proof.has_pending && !state.may_owe_result(proof.budget)?)
    }
}
