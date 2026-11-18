use super::*;

impl<'r, 'g> Checker<'r, 'g> {
    // Do not choose the default while an enclosing ordinary call is still
    // negotiating a receiving category. Tuple and List contexts can conflict.
    fn pending_bracket_receiver(
        &mut self,
        inference: &Inference<'g>,
        mut node: NodeId,
    ) -> Work<bool> {
        while let Some(&parent) = inference.parents.get(&node) {
            self.elab.step(1)?;
            if inference.calls.get(&parent).is_some_and(|call| call.family) {
                return Ok(true);
            }
            node = parent;
        }
        Ok(false)
    }

    pub(in crate::analysis::scalars) fn advance_list_defaults(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<bool> {
        if self.collections.list.is_none() {
            return Ok(false);
        }
        self.elab.reserve(inference.data_nodes.len())?;
        let brackets: Vec<_> = inference
            .data_nodes
            .iter()
            .copied()
            .filter(|&node| {
                inference.module.syntax.nodes[node].kind == NodeKind::Bracket
                    && !inference.slots[&node].blocked
                    && !inference.data.contains_key(&node)
                    && !inference.ordinary_tuples.contains(&node)
            })
            .collect();
        let mut changed = false;
        for node in brackets {
            if !self.pending_bracket_receiver(inference, node)? {
                let before = inference.queue.len();
                self.with_node_premise(inference, node, |checker, inference| {
                    checker.list_step(inference, node, true)
                })?;
                changed |= inference.queue.len() != before;
            }
        }
        Ok(changed)
    }

    pub(in crate::analysis::scalars) fn list_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        allow_default: bool,
    ) -> Work<()> {
        let Some(list) = self.collections.list else {
            return Ok(());
        };
        if inference.data.contains_key(&node) {
            return Ok(());
        }
        let module = inference.module;
        let elements = &module.syntax.nodes[node].children;
        let expected = inference.slots[&node].expected.clone();
        let mut element = expected.as_ref().and_then(|ty| list.element(ty));
        if element.is_none() {
            if !allow_default
                || expected
                    .as_ref()
                    .is_some_and(|ty| matches!(ty.as_ref(), TypeTerm::Tuple(_)))
            {
                return Ok(());
            }
            let mut untyped = false;
            for &child in elements {
                self.elab.step(1)?;
                let slot = &inference.slots[&child];
                if slot.blocked {
                    return Ok(());
                }
                let Some(actual) = &slot.ty else {
                    untyped = true;
                    continue;
                };
                if primitive_name(actual) == Some("never") {
                    continue;
                }
                element = Some(if let Some(prior) = element {
                    if self.same(&prior, actual)? {
                        prior
                    } else if let Some(ty) = self.data_join(&prior, actual)? {
                        ty
                    } else {
                        self.mismatch(module, child, actual, &prior)?;
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                        return Ok(());
                    }
                } else {
                    actual.clone()
                });
            }
            if untyped {
                if let Some(element) = &element {
                    for &child in elements {
                        self.elab.step(1)?;
                        let slot = &inference.slots[&child];
                        if slot.ty.is_none()
                            && slot.expected.is_none()
                            && (self.needs_numeric_width(module, child, 1)?
                                || (module.syntax.nodes[child].kind == NodeKind::Bracket
                                    && list.element(element).is_some()))
                        {
                            self.establish(inference, child, element.clone(), true)?;
                        }
                    }
                }
                return Ok(());
            }
        }
        let Some(element) = element else {
            return Ok(());
        };
        self.elab.reserve(3 + elements.len() * 3)?;
        let ty = Arc::new(TypeTerm::Nominal {
            declaration: &list.declaration.id,
            arguments: vec![element.clone()],
        });
        inference.data.insert(
            node,
            PreparedData {
                ty: ty.clone(),
                flow: None,
                inputs: elements.clone(),
                contexts: Vec::new(),
                fact: DataExpression::List {
                    declaration: &list.declaration.id,
                    elements: elements.clone(),
                },
            },
        );
        self.establish(inference, node, ty, false)?;
        for &child in elements {
            self.establish(inference, child, element.clone(), true)?;
        }
        Ok(())
    }

    pub(super) fn unresolved_list(&mut self, inference: &Inference<'g>, node: NodeId) -> Work<()> {
        let module = inference.module;
        if self.collections.list.is_none() {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
        } else if self.pending_bracket_receiver(inference, node)? {
            self.defer(module, node, ScalarUnsupported::FamilySelection)?;
        } else {
            let children = self.runtime_children(inference, node)?;
            if !self.has_blocked_input(inference, children)? {
                self.finding(module, node, ScalarFindingKind::InsufficientContext)?;
            }
        }
        Ok(())
    }
}
