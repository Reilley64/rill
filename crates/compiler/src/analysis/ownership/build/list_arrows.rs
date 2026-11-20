//! Construct the checked List-reading arrow frames before their distinct call actions.
use super::*;
use crate::analysis::typed::{PairCall, PairChild, ScopedJoinCall};

pub(super) enum ListArrowUse<'a, 'g> {
    Serial(&'a PairCall<'g>),
    Scoped(&'a ScopedJoinCall<'g>),
}

struct ListArrowPlan<'a, 'g> {
    read: NodeId,
    operand: NodeId,
    source: Slot,
    exclusive: bool,
    op: ListI32Read<'a, 'g>,
    read_type: &'a Type<'g>,
    comparison: Option<(NodeId, i32)>,
    output: &'a Type<'g>,
    closure: &'a Type<'g>,
}

pub(in crate::analysis::ownership) struct ListArrowPair<'g> {
    pub implementation: &'g DeclarationId,
    pub body: Site<'g>,
    pub children: [ListArrow<'g>; 2],
}

pub(in crate::analysis::ownership) struct ListArrow<'g> {
    pub invocation: Site<'g>,
    pub body_site: Site<'g>,
    pub source: Slot,
    pub capture: Place<'g>,
    pub body: ArrowBody<'g>,
}

pub(in crate::analysis::ownership) enum ArrowBody<'g> {
    List {
        incoming_backing: Place<'g>,
        mir: Arc<Mir<'g>>,
    },
    Reference(Type<'g>),
}

pub(super) fn list_arrow_result_pair(ty: &Type<'_>, scoped: bool) -> bool {
    let TypeTerm::Tuple(items) = ty.as_ref() else {
        return false;
    };
    let [a, b] = items.as_slice() else {
        return false;
    };
    match (scalar::scalar(a), scalar::scalar(b)) {
        (Some(Primitive::I32), Some(Primitive::I32)) => true,
        (Some(Primitive::I32), Some(Primitive::Bool))
        | (Some(Primitive::Bool), Some(Primitive::I32)) => scoped,
        _ => false,
    }
}

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn call_two_list_arrows(
        &mut self,
        node: NodeId,
        call: ListArrowUse<'_, 'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let pending = Stop::Pending(node);
        let into = into.ok_or(Stop::Pending(node))?;
        let (implementation, body, argument, [first, second], scoped) = match call {
            ListArrowUse::Serial(pair)
                if pair.implementation.module.origin == Origin::Application =>
            {
                (
                    pair.implementation,
                    pair.body,
                    pair.argument,
                    pair.children.each_ref(),
                    None,
                )
            }
            ListArrowUse::Scoped(join) => (
                &join.role.implementation.id,
                join.slots[0].child.invocation,
                join.argument,
                join.slots.each_ref().map(|slot| &slot.child),
                Some(join.role),
            ),
            _ => return Err(pending),
        };
        if self.module.key.origin != Origin::Application
            || self.slots[into].members != 2
            || self.slots[into].custom_disposer.is_some()
        {
            return Err(pending);
        }
        for i in 0..2 {
            self.element(node, into, i)?;
        }
        let (tuple, tuple_depth) = self.ungroup_expression_at(argument, depth + 1)?;
        if self.module.syntax.nodes[tuple].children.as_slice() != [first.element, second.element] {
            return Err(pending);
        }
        let left = self.list_arrow_plan(first, tuple_depth + 1, scoped.is_some())?;
        let right = self.list_arrow_plan(second, tuple_depth + 1, scoped.is_some())?;
        let needs_lifecycle = left.exclusive || right.exclusive;
        let sources = [
            (left.source, left.exclusive),
            (right.source, right.exclusive),
        ];
        let (a, a_capture, a_body) = self.list_arrow(first, left, needs_lifecycle)?;
        let (b, b_capture, b_body) = self.list_arrow(second, right, needs_lifecycle)?;
        let action = if let (Some(a_body), Some(b_body)) = (a_body, b_body) {
            self.budget.reserve(3)?;
            let index = self.list_arrow_pairs.len();
            self.list_arrow_pairs.push(ListArrowPair {
                implementation,
                body,
                children: [a_body, b_body],
            });
            if let Some(role) = scoped {
                Action::ScopedJoin {
                    pair: index,
                    receivers: [a, b],
                    into,
                    role,
                }
            } else {
                Action::CallListArrows {
                    pair: index,
                    receivers: [a, b],
                    into,
                }
            }
        } else {
            // Check both real acquisitions, but do not build or commit an
            // exclusive child frame without a checked lifecycle for it.
            Action::UnprovedScopedJoin {
                receivers: [a, b],
                into,
            }
        };
        let mut next = self.emit(node, action, Edges::Next(next))?;
        // Lower backwards: both closures form in written argument order before
        // dispatch or commit. Neither capture copies the List.
        for ((receiver, capture), (source, exclusive)) in [(a, a_capture), (b, b_capture)]
            .into_iter()
            .zip(sources)
            .rev()
        {
            let form = self.emit(
                self.slots[receiver].node,
                Action::FormListArrow {
                    capture,
                    into: receiver,
                    mode: if exclusive {
                        InvocationMode::Mutable
                    } else {
                        InvocationMode::ReadOnly
                    },
                },
                Edges::Next(next),
            )?;
            next = self.emit(
                self.slots[capture].node,
                Action::Borrow {
                    from: source,
                    into: capture,
                    exclusive,
                    retained_target: false,
                },
                Edges::Next(form),
            )?;
        }
        Ok(next)
    }

    fn list_arrow_plan(
        &mut self,
        child: &PairChild<'g>,
        depth: usize,
        scoped: bool,
    ) -> ResultOf<ListArrowPlan<'a, 'g>> {
        let arrow = &child.arrow;
        let (origin, depth) = self.ungroup_expression_at(child.element, depth)?;
        if origin != arrow.site.node {
            return Err(Stop::Pending(origin));
        }
        let pending = Stop::Pending(origin);
        let [capture] = arrow.captures.as_slice() else {
            return Err(pending);
        };
        let exclusive = match (capture.form, arrow.mode) {
            (CaptureForm::ExplicitBorrow { mutable: false }, Some(InvocationMode::ReadOnly)) => {
                false
            }
            (CaptureForm::ExplicitBorrow { mutable: true }, Some(InvocationMode::Mutable))
                if scoped =>
            {
                true
            }
            _ => return Err(pending),
        };
        if !std::ptr::eq(arrow.site.module, &self.module.key)
            || !std::ptr::eq(capture.site.module, &self.module.key)
            || capture.storage.as_ref() != Some(&capture.ty)
        {
            return Err(pending);
        }
        let mut expression_depth = depth + 1;
        if self.module.syntax.nodes[child.body.node].kind == NodeKind::Block {
            self.depth(expression_depth)?;
            self.depth(expression_depth + 1)?;
            expression_depth += 2;
        }
        let (result, result_depth) =
            self.ungroup_expression_at(child.expression, expression_depth)?;
        let value = self.normal_value(result)?;
        if arrow.result != value.ty {
            return Err(pending);
        }
        let nodes = &self.module.syntax.nodes;
        let (read, depth, comparison) = match scalar::scalar(&value.ty) {
            Some(Primitive::I32) => (result, result_depth, None),
            Some(Primitive::Bool) if scoped && !exclusive => {
                let (NodeKind::Binary("=="), [lhs, rhs]) =
                    (nodes[result].kind, nodes[result].children.as_slice())
                else {
                    return Err(pending);
                };
                let (read, depth) = self.ungroup_expression_at(*lhs, result_depth + 1)?;
                let (literal, _) = self.ungroup_expression_at(*rhs, result_depth + 1)?;
                let rhs = self.normal_value(literal)?;
                let Some(LiteralValue::Integer {
                    magnitude,
                    negative: false,
                }) = rhs.literal.map(|l| l.value)
                else {
                    return Err(pending);
                };
                if nodes[literal].kind != NodeKind::Literal
                    || scalar::scalar(&rhs.ty) != Some(Primitive::I32)
                {
                    return Err(pending);
                }
                let rhs = i32::try_from(magnitude).map_err(|_| Stop::Pending(literal))?;
                (read, depth, Some((result, rhs)))
            }
            _ => return Err(pending),
        };
        let read_facts = if comparison.is_some() {
            self.normal_value(read)?
        } else {
            value
        };
        let read_op = self.check_list_i32_read(read, read_facts, depth, exclusive)?;
        let base = read_op.base;
        let (NodeKind::Prefix(operator), [operand]) =
            (nodes[base].kind, nodes[base].children.as_slice())
        else {
            return Err(pending);
        };
        if operator != if exclusive { "&mut" } else { "&" }
            || base != capture.site.node
            || read_op.view.ty != capture.ty
        {
            return Err(pending);
        }
        let (operand, _) = self.ungroup_expression_at(*operand, read_op.base_depth + 1)?;
        let named = self.normal_value(operand)?;
        let path = named.path.as_ref().ok_or(Stop::Pending(origin))?;
        if nodes[operand].kind != NodeKind::ValueReference
            || !std::ptr::eq(path.root, capture.path.root)
            || !path.projections.is_empty()
            || named.access != Access::Owned
            || !matches!(capture.ty.as_ref(), TypeTerm::Reference { referent, .. } if *referent == named.ty)
        {
            return Err(pending);
        }
        let source = self.resolve_place(operand, path)?;
        let backing = self.slots[source];
        if !matches!(backing.cleanup, StorageCleanup::DisposeListI32(_))
            || backing.projection.is_some()
        {
            return Err(pending);
        }
        let closure = &self.normal_value(origin)?.ty;
        Ok(ListArrowPlan {
            read,
            operand,
            source,
            exclusive,
            op: read_op,
            read_type: &read_facts.ty,
            comparison,
            output: &value.ty,
            closure,
        })
    }

    fn list_arrow(
        &mut self,
        child: &PairChild<'g>,
        plan: ListArrowPlan<'a, 'g>,
        needs_lifecycle: bool,
    ) -> ResultOf<(Slot, Slot, Option<ListArrow<'g>>)> {
        let ListArrowPlan {
            read,
            operand,
            source,
            op: read_op,
            read_type,
            comparison,
            output,
            closure,
            ..
        } = plan;
        let base = read_op.base;
        let view_ty = &read_op.view.ty;
        let backing = self.slots[source];
        let receiver = self.slot(child.arrow.site.node, closure)?;
        let staged = self.slot(base, view_ty)?;
        if needs_lifecycle {
            return Ok((receiver, staged, None));
        }

        // Check the actual expression in a separate frame using a transferred view.
        // The backing proxy is initialized but is not owned by this frame and
        // must not join its cleanup list. No second borrow is acquired here.
        let result = comparison.map_or(read, |(node, _)| node);
        let mut frame = Build::with_facts(
            self.typed,
            self.module,
            result,
            None,
            BuildFacts::Local(BTreeMap::new()),
            self.budget,
        );
        let view = frame.slot(base, view_ty)?;
        let output = frame.slot(result, output)?;
        frame.budget.reserve(4)?;
        let incoming = frame.slots.len();
        frame.slots.push(Local {
            node: operand,
            owner: incoming,
            cleanup: StorageCleanup::End,
            ..backing
        });
        frame.incoming_list_arrow = Some((view, incoming));
        let end = frame.emit(result, Action::None, Edges::End)?;
        let cleanup = frame.cleanup(result, &[view], end)?;
        let exported = frame.emit(result, Action::Export(output), Edges::Next(cleanup))?;
        let (read_into, after_read) = if let Some((node, literal)) = comparison {
            let scratch = frame.slot(read, read_type)?;
            let compare = frame.emit(
                node,
                Action::EqualI32Literal {
                    from: scratch,
                    literal,
                    into: output,
                },
                Edges::Next(exported),
            )?;
            (scratch, compare)
        } else {
            (output, exported)
        };
        let entry = frame.emit(
            read,
            read_op.action(view, Some(read_into)),
            Edges::Next(after_read),
        )?;
        let capture = frame.slots[view].place(self.module);
        let incoming_backing = frame.slots[incoming].place(self.module);
        let body = flow::run(frame, entry, vec![view, incoming], None, false, None)?.mir;
        Ok((
            receiver,
            staged,
            Some(ListArrow {
                invocation: child.invocation,
                body_site: child.body,
                source,
                capture,
                body: ArrowBody::List {
                    incoming_backing,
                    mir: Arc::new(body),
                },
            }),
        ))
    }
}
