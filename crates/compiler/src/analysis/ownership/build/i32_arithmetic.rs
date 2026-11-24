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

    /// Whether a binding of type `ty` is written `&mut record.field`: an
    /// exclusive `i32` field of a local record, owned and one projection deep.
    /// Screened from syntax and facts already in hand, so it costs nothing.
    pub(super) fn exclusive_field_reborrow(&self, initializer: NodeId, ty: &Type<'g>) -> bool {
        let syntax = &self.module.syntax.nodes;
        let (NodeKind::Prefix("&mut"), &[field]) = (
            syntax[initializer].kind,
            syntax[initializer].children.as_slice(),
        ) else {
            return false;
        };
        let (NodeKind::Field, &[record, _]) =
            (syntax[field].kind, syntax[field].children.as_slice())
        else {
            return false;
        };
        let (
            Ok(field_facts),
            TypeTerm::Reference {
                mutable: true,
                referent,
            },
        ) = (self.value(field), ty.as_ref())
        else {
            return false;
        };
        syntax[record].kind == NodeKind::ValueReference
            && field_facts.ty == *referent
            && matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            && field_facts.access == Access::Owned
            && field_facts.place == TypedPlace::Writable
            && field_facts.path.as_ref().is_some_and(|path| {
                path.root.kind == DeclarationKind::Binding
                    && matches!(path.projections.as_slice(), [PathProjection::Field(_)])
            })
    }

    /// Admit `*parameter += literal` through an incoming `&mut i32`: read the
    /// referent, add the literal with the checked scalar addition, install the
    /// sum through the descriptor and end the scratch slots. The descriptor
    /// may also be an `&mut i32` field of a local record, `*record.field +=
    /// literal` (RFC 0007 O7): its member slot carries the record's loan.
    pub(super) fn add_through_exclusive_i32(
        &mut self,
        node: NodeId,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let pending = Stop::Pending(node);
        let syntax = &self.module.syntax.nodes;
        // Screen from the written shape before any charge.
        let [left, right] = *syntax[node].children.as_slice() else {
            return Err(pending);
        };
        if syntax[right].kind != NodeKind::Literal {
            return Err(pending);
        }
        let (NodeKind::Prefix("*"), &[target]) =
            (syntax[left].kind, syntax[left].children.as_slice())
        else {
            return Err(pending);
        };
        let member_target = syntax[target].kind == NodeKind::Field;
        if syntax[target].kind != NodeKind::ValueReference && !member_target {
            return Err(pending);
        }
        // Facts already in hand: `value` reads them without a charge.
        let target_facts = self.value(target)?;
        let TypeTerm::Reference {
            mutable: true,
            referent,
        } = target_facts.ty.as_ref()
        else {
            return Err(pending);
        };
        let (Some(path), true) = (
            target_facts.path.as_ref(),
            matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"),
        ) else {
            return Err(pending);
        };
        if member_target {
            if !matches!(path.projections.as_slice(), [PathProjection::Field(_)])
                || path.root.kind != DeclarationKind::Binding
                || target_facts.access != Access::Owned
            {
                return Err(pending);
            }
        } else if !path.projections.is_empty()
            || target_facts.access != Access::Owned
            || target_facts.binding != Some(path.root)
        {
            return Err(pending);
        }
        // A local descriptor must be written `&mut record.field`. The count is
        // zero, so ruling the form out costs nothing, unless some binding in
        // the body was written that way.
        let mut local_descriptor = None;
        match path.root.kind {
            // The descriptor is the record's own member slot, resolved below
            // like a parameter's.
            DeclarationKind::Binding if member_target => {}
            DeclarationKind::Parameter => {}
            DeclarationKind::Binding if self.exclusive_field_reborrows != 0 => {
                let slot = self.place(target)?;
                if !self.slots[slot].exclusive_field_reborrow {
                    return Err(pending);
                }
                local_descriptor = Some(slot);
            }
            _ => return Err(pending),
        }
        let (place, literal) = (self.value(left)?, self.value(right)?);
        if place.ty != *referent
            || literal.ty != *referent
            || place.place != TypedPlace::Writable
            || self.arithmetic_literal(right, literal).is_none()
        {
            return Err(pending);
        }
        self.depth(depth + 1)?;
        self.normal_value(target)?;
        self.normal_value(left)?;
        self.normal_value(right)?;
        let ty = place.ty.clone();
        let descriptor = match local_descriptor {
            Some(slot) => slot,
            None => self.place(target)?,
        };
        let current = self.slot(left, &ty)?;
        let operand = self.slot(right, &ty)?;
        let sum = self.slot(node, &ty)?;
        let install = self.emit(
            node,
            Action::InstallScalarThrough {
                descriptor,
                incoming: sum,
            },
            Edges::Next(next),
        )?;
        let end_operand = self.emit(right, Action::End(operand), Edges::Next(install))?;
        let end_current = self.emit(left, Action::End(current), Edges::Next(end_operand))?;
        let add = self.emit(
            node,
            Action::CheckedI32Add {
                left: current,
                right: operand,
                into: sum,
            },
            Edges::Next(end_current),
        )?;
        let operand_init = self.expression(right, Some(operand), add, depth + 1)?;
        self.expression(left, Some(current), operand_init, depth + 1)
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
        let left_temp = self.call_field_operand(left, depth + 1)?;
        let right_temp = self.call_field_operand(right, depth + 1)?;
        self.budget
            .reserve(usize::from(left_temp.is_some()) + usize::from(right_temp.is_some()))?;
        let scratch: Vec<Slot> = left_temp
            .iter()
            .chain(&right_temp)
            .map(|temp| temp.scratch)
            .collect();
        // The projected call results end after the comparison reads them.
        let next = self.cleanup(node, &scratch, next)?;
        let compare = self.emit(
            node,
            Action::EqualI32 {
                left: left_slot,
                right: right_slot,
                into,
            },
            Edges::Next(next),
        )?;
        let right = self.stage_value(right, right_slot, compare, right_temp, depth + 1)?;
        self.stage_value(left, left_slot, right, left_temp, depth + 1)
    }

    /// The projection of `call().field` as an equality operand, if written so.
    fn call_field_operand(
        &mut self,
        operand: NodeId,
        depth: usize,
    ) -> ResultOf<Option<ProjectedTemporary>> {
        // Screen on the written shape, through groups, before any charge.
        let nodes = &self.module.syntax.nodes;
        let mut shape = operand;
        while nodes[shape].kind == NodeKind::Group {
            shape = nodes[shape].children[0];
        }
        if nodes[shape].kind != NodeKind::Field {
            return Ok(None);
        }
        let (shape, _) = self.ungroup_expression_at(operand, depth)?;
        self.temporary_projection(shape, true)
    }

    /// `left && right` on two i32 equalities (ri-e9t8). Evaluate `left` into a
    /// scratch `bool`, end it on both edges, then either evaluate `right`
    /// into `into` or, when `left` is false, initialize `into` without
    /// evaluating `right`. Both edges reach `next` with `into` initialized.
    pub(super) fn i32_equality_conjunction(
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
        // A dead operand grants nothing and stops here, before any charge.
        if ![left, right]
            .into_iter()
            .all(|operand| self.flows_normally(operand))
        {
            return Err(Stop::Pending(node));
        }
        let left_facts = self.normal_value(left)?;
        // A statically decided left operand has a dead edge, which grants nothing.
        if Self::outcomes(left_facts) != (true, true) {
            return Err(Stop::Pending(node));
        }
        let scratch = self.slot(left, &left_facts.ty)?;
        let skipped = self.emit(node, Action::Init(into), Edges::Next(next))?;
        let no = self.emit(node, Action::End(scratch), Edges::Next(skipped))?;
        let evaluated = self.expression(right, Some(into), next, depth + 1)?;
        let yes = self.emit(node, Action::End(scratch), Edges::Next(evaluated))?;
        let branch = self.branch(left, yes, no)?;
        self.expression(left, Some(scratch), branch, depth + 1)
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
