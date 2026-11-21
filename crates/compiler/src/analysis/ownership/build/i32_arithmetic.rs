use super::*;

impl<'a, 'g> Build<'a, 'g, '_> {
    /// Admit `local * literal` and addition of a local or that product to a
    /// local. The reads use the actual scalar slots, not initializer values.
    pub(super) fn checked_i32_arithmetic(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let into = into.ok_or(Stop::Pending(node))?;
        let pending = Stop::Pending(node);
        if scalar::scalar(&facts.ty) != Some(Primitive::I32) {
            return Err(pending);
        }
        let syntax = &self.module.syntax.nodes;
        let [left, right] = *syntax[node].children.as_slice() else {
            return Err(pending);
        };
        let (left, left_depth) = self.ungroup_expression_at(left, depth + 1)?;
        let (right, _) = self.ungroup_expression_at(right, depth + 1)?;
        match syntax[node].kind {
            NodeKind::Binary("*") => {
                let left = self.arithmetic_local(left, node)?;
                let literal = self.normal_value(right)?;
                let Some(LiteralValue::Integer {
                    magnitude,
                    negative: false,
                }) = literal.literal.map(|literal| literal.value)
                else {
                    return Err(pending);
                };
                if syntax[right].kind != NodeKind::Literal || literal.ty != facts.ty {
                    return Err(pending);
                }
                let literal = i32::try_from(magnitude).map_err(|_| pending)?;
                self.emit(
                    node,
                    Action::CheckedI32Multiply {
                        left,
                        literal,
                        into,
                    },
                    Edges::Next(next),
                )
            }
            NodeKind::Binary("+") => {
                let right = self.arithmetic_local(right, node)?;
                let product = syntax[left].kind == NodeKind::Binary("*");
                let (lhs, next) = if product {
                    let lhs = self.slot(left, &facts.ty)?;
                    let end = self.emit(left, Action::End(lhs), Edges::Next(next))?;
                    (lhs, end)
                } else {
                    (self.arithmetic_local(left, node)?, next)
                };
                let add = self.emit(
                    node,
                    Action::CheckedI32Add {
                        left: lhs,
                        right,
                        into,
                    },
                    Edges::Next(next),
                )?;
                // The multiplication can trap before the right operand is read.
                if product {
                    let product = self.normal_value(left)?;
                    self.checked_i32_arithmetic(left, product, Some(lhs), add, left_depth)
                } else {
                    Ok(add)
                }
            }
            _ => Err(pending),
        }
    }

    fn arithmetic_local(&mut self, operand: NodeId, node: NodeId) -> ResultOf<Slot> {
        let facts = self.normal_value(operand)?;
        let path = facts.path.as_ref().ok_or(Stop::Pending(node))?;
        if self.module.syntax.nodes[operand].kind != NodeKind::ValueReference
            || scalar::scalar(&facts.ty) != Some(Primitive::I32)
            || facts.access != Access::Owned
            || !path.projections.is_empty()
            || !facts.binding.is_some_and(|id| std::ptr::eq(id, path.root))
        {
            return Err(Stop::Pending(node));
        }
        self.resolve_place(operand, path)
    }
}
