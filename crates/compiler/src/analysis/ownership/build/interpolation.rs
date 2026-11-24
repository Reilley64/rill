use super::*;

impl<'a, 'g> Build<'a, 'g, '_> {
    /// Admit a template whose every hole is an owned `i32` local, field of
    /// an owned local, or literal, formatted by the selected primitive Show
    /// (RFC 0007 O20). Each hole is read in written order into its own
    /// scratch slot, which the formatting ends; the direct reads therefore
    /// end before the next hole, and no owner of the read place is released.
    pub(super) fn i32_interpolation(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let pending = || Stop::Pending(node);
        // Screen from the paid-for facts before the first charge (rule 1).
        let (Some(into), Some(DataExpression::Interpolation(calls))) = (into, &facts.data) else {
            return Err(pending());
        };
        let nodes = &self.module.syntax.nodes;
        let holes = nodes[node]
            .children
            .iter()
            .filter(|&&child| nodes[child].kind == NodeKind::TemplateHole);
        if holes.clone().count() != calls.len() {
            return Err(pending());
        }
        for (&hole, call) in holes.zip(calls.iter()) {
            if call.expression != nodes[hole].children[0] || call.primitive != Some(Primitive::I32)
            {
                return Err(pending());
            }
            let mut value = call.expression;
            while nodes[value].kind == NodeKind::Group {
                value = nodes[value].children[0];
            }
            let Ok(operand) = self.value(value) else {
                return Err(pending());
            };
            let admitted = operand.access == Access::Owned
                && scalar::scalar(&operand.ty) == Some(Primitive::I32)
                && match nodes[value].kind {
                    NodeKind::Literal => true,
                    NodeKind::ValueReference => operand
                        .path
                        .as_ref()
                        .is_some_and(|p| p.projections.is_empty()),
                    NodeKind::Field => operand.path.as_ref().is_some_and(|p| {
                        matches!(p.projections.as_slice(), [PathProjection::Field(_)])
                    }),
                    _ => false,
                };
            if !admitted {
                return Err(pending());
            }
        }

        self.budget.step(calls.len())?;
        let first = self.slots.len();
        for (index, call) in calls.iter().enumerate() {
            let ty = &self.value(call.expression)?.ty;
            if self.slot(call.expression, ty)? != first + index {
                return Err(pending());
            }
        }
        let mut pc = self.emit(
            node,
            Action::FormatI32Template {
                first,
                count: calls.len(),
                into,
            },
            Edges::Next(next),
        )?;
        for (index, call) in calls.iter().enumerate().rev() {
            pc = self.expression(call.expression, Some(first + index), pc, depth + 1)?;
        }
        Ok(pc)
    }
}
