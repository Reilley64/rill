use super::*;

/// An operand of a checked addition: a local's own slot, or a scratch slot
/// that holds a field read or a product until the addition has read it.
#[derive(Clone, Copy)]
enum Operand {
    Local(Slot),
    Field(Slot, NodeId, usize),
    Product(Slot, NodeId, usize),
}

impl Operand {
    fn slot(self) -> Slot {
        match self {
            Operand::Local(slot) | Operand::Field(slot, ..) | Operand::Product(slot, ..) => slot,
        }
    }
}

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
        let syntax = &self.module.syntax.nodes;
        let [left, right] = *syntax[node].children.as_slice() else {
            return Err(pending);
        };
        let (left, left_depth) = self.ungroup_expression_at(left, depth + 1)?;
        let (right, right_depth) = self.ungroup_expression_at(right, depth + 1)?;
        match syntax[node].kind {
            NodeKind::Binary("*") => {
                let left = self.arithmetic_local(left, node)?;
                let literal = self.normal_value(right)?;
                let literal = self.arithmetic_literal(right, literal).ok_or(pending)?;
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
                let right = self.arithmetic_operand(right, node, right_depth)?;
                let left = if syntax[left].kind == NodeKind::Binary("*") {
                    Operand::Product(self.slot(left, &facts.ty)?, left, left_depth)
                } else {
                    self.arithmetic_operand(left, node, left_depth)?
                };
                // The addition reads the left operand, then the right. A scratch
                // slot ends after it, the left before the right.
                let mut next = next;
                for operand in [right, left] {
                    if let Operand::Field(slot, site, _) | Operand::Product(slot, site, _) = operand
                    {
                        next = self.emit(site, Action::End(slot), Edges::Next(next))?;
                    }
                }
                let mut pc = self.emit(
                    node,
                    Action::CheckedI32Add {
                        left: left.slot(),
                        right: right.slot(),
                        into,
                    },
                    Edges::Next(next),
                )?;
                if let Operand::Field(slot, site, site_depth) = right {
                    pc = self.expression(site, Some(slot), pc, site_depth)?;
                }
                match left {
                    // The multiplication can trap before the right operand is read.
                    Operand::Product(slot, site, site_depth) => {
                        let product = self.normal_value(site)?;
                        self.checked_i32_arithmetic(site, product, Some(slot), pc, site_depth)
                    }
                    Operand::Field(slot, site, site_depth) => {
                        self.expression(site, Some(slot), pc, site_depth)
                    }
                    Operand::Local(_) => Ok(pc),
                }
            }
            _ => Err(pending),
        }
    }

    /// Evaluate both `i32` operands left to right into scratch slots, which
    /// the comparison ends, then initialize the `bool` result.
    pub(super) fn i32_equality(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let into = into.ok_or(Stop::Pending(node))?;
        let [left, right] = *self.module.syntax.nodes[node].children.as_slice() else {
            return Err(Stop::Pending(node));
        };
        let left_ty = &self.normal_value(left)?.ty;
        let right_ty = &self.normal_value(right)?.ty;
        let left_slot = self.slot(left, left_ty)?;
        let right_slot = self.slot(right, right_ty)?;
        let compare = self.emit(
            node,
            Action::EqualI32 {
                left: left_slot,
                right: right_slot,
                into,
            },
            Edges::Next(next),
        )?;
        let right = self.expression(right, Some(right_slot), compare, depth + 1)?;
        self.expression(left, Some(left_slot), right, depth + 1)
    }

    fn arithmetic_literal(&self, node: NodeId, facts: &ExpressionFacts<'g>) -> Option<i32> {
        let (
            NodeKind::Literal,
            LiteralValue::Integer {
                magnitude,
                negative: false,
            },
        ) = (self.module.syntax.nodes[node].kind, facts.literal?.value)
        else {
            return None;
        };
        i32::try_from(magnitude).ok()
    }

    fn arithmetic_local_path<'f>(
        &self,
        node: NodeId,
        facts: &'f ExpressionFacts<'g>,
    ) -> Option<&'f PlacePath<'g>> {
        let path = facts.path.as_ref()?;
        (self.module.syntax.nodes[node].kind == NodeKind::ValueReference
            && facts.access == Access::Owned
            && path.projections.is_empty()
            && facts.binding.is_some_and(|id| std::ptr::eq(id, path.root)))
        .then_some(path)
    }

    /// A field read goes through a scratch slot. M4's facts admit only an
    /// `i32` field of a local record here, so nothing is spent before the
    /// slot is allocated.
    fn arithmetic_operand(
        &mut self,
        operand: NodeId,
        node: NodeId,
        depth: usize,
    ) -> ResultOf<Operand> {
        let facts = self.normal_value(operand)?;
        if projection(facts).is_some() {
            return Ok(Operand::Field(
                self.slot(operand, &facts.ty)?,
                operand,
                depth,
            ));
        }
        self.arithmetic_local_of(operand, facts, node)
            .map(Operand::Local)
    }

    fn arithmetic_local(&mut self, operand: NodeId, node: NodeId) -> ResultOf<Slot> {
        let facts = self.normal_value(operand)?;
        self.arithmetic_local_of(operand, facts, node)
    }

    fn arithmetic_local_of(
        &mut self,
        operand: NodeId,
        facts: &ExpressionFacts<'g>,
        node: NodeId,
    ) -> ResultOf<Slot> {
        let path = self
            .arithmetic_local_path(operand, facts)
            .ok_or(Stop::Pending(node))?;
        // The key ends in the declaration's module path: charge each probe.
        let probes = 1 + self.locals.len().max(1).ilog2() as usize;
        graph::charge_key(&path.root.module, probes, self.budget)?;
        self.locals
            .get(path.root)
            .copied()
            .ok_or(Stop::Pending(node))
    }
}
