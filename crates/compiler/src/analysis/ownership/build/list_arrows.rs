//! Ordinary specialization of a checked two-invocation application body.
use super::*;
use crate::analysis::typed::PairCall;

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
    pub incoming_backing: Place<'g>,
    pub body: Arc<Mir<'g>>,
}

pub(super) fn scalar_pair(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Tuple(items) if matches!(items.as_slice(), [a, b]
        if scalar::scalar(a) == Some(Primitive::I32) && scalar::scalar(b) == Some(Primitive::I32)))
}

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn call_list_arrows(
        &mut self,
        node: NodeId,
        pair: &PairCall<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let pending = Stop::Pending(node);
        let into = into.ok_or(Stop::Pending(node))?;
        if self.module.key.origin != Origin::Application
            || pair.implementation.module.origin != Origin::Application
            || self.slots[into].members != 2
            || self.slots[into].custom_disposer.is_some()
        {
            return Err(pending);
        }
        for i in 0..2 {
            self.element(node, into, i)?;
        }
        let [first, second] = &pair.children;
        let (a, a_capture, a_body) = self.list_arrow(first, depth + 1)?;
        let (b, b_capture, b_body) = self.list_arrow(second, depth + 1)?;
        self.budget.reserve(3)?;
        let index = self.list_arrow_pairs.len();
        let sources = [a_body.source, b_body.source];
        self.list_arrow_pairs.push(ListArrowPair {
            implementation: pair.implementation,
            body: pair.body,
            children: [a_body, b_body],
        });
        let mut next = self.emit(
            node,
            Action::CallListArrows {
                pair: index,
                receivers: [a, b],
                into,
            },
            Edges::Next(next),
        )?;
        // Lower backwards: both closures form in written argument order before
        // the selected body starts either arrow. Neither capture copies the List.
        for (receiver, capture, source) in [(a, a_capture, sources[0]), (b, b_capture, sources[1])]
            .into_iter()
            .rev()
        {
            let form = self.emit(
                self.slots[receiver].node,
                Action::FormListArrow {
                    capture,
                    into: receiver,
                },
                Edges::Next(next),
            )?;
            next = self.emit(
                self.slots[capture].node,
                Action::Borrow {
                    from: source,
                    into: capture,
                    exclusive: false,
                    retained_target: false,
                },
                Edges::Next(form),
            )?;
        }
        Ok(next)
    }

    fn list_arrow(
        &mut self,
        child: &crate::analysis::typed::PairChild<'g>,
        depth: usize,
    ) -> ResultOf<(Slot, Slot, ListArrow<'g>)> {
        self.depth(depth)?;
        let arrow = &child.arrow;
        let origin = arrow.site.node;
        let pending = Stop::Pending(origin);
        let [capture] = arrow.captures.as_slice() else {
            return Err(pending);
        };
        if !std::ptr::eq(arrow.site.module, &self.module.key)
            || !std::ptr::eq(capture.site.module, &self.module.key)
            || arrow.mode != Some(InvocationMode::ReadOnly)
            || scalar::scalar(&arrow.result) != Some(Primitive::I32)
            || capture.form != (CaptureForm::ExplicitBorrow { mutable: false })
            || capture.storage.as_ref() != Some(&capture.ty)
        {
            return Err(pending);
        }
        let (read, depth) = self.ungroup_expression_at(child.expression, depth + 1)?;
        let value = self.normal_value(read)?;
        let read_op = self.check_list_i32_read(read, value, depth)?;
        let base = read_op.base;
        let nodes = &self.module.syntax.nodes;
        let (NodeKind::Prefix("&"), [operand]) =
            (nodes[base].kind, nodes[base].children.as_slice())
        else {
            return Err(pending);
        };
        if base != capture.site.node || read_op.view.ty != capture.ty {
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
        let closure = self.normal_value(origin)?;
        let receiver = self.slot(origin, &closure.ty)?;
        let staged = self.slot(base, &capture.ty)?;

        // Check the actual read in a separate frame using a transferred view.
        // The backing proxy is initialized but is not owned by this frame and
        // must not join its cleanup list. No second borrow is acquired here.
        let mut frame = Build::with_facts(
            self.typed,
            self.module,
            read,
            None,
            BuildFacts::Local(BTreeMap::new()),
            self.budget,
        );
        let view = frame.slot(base, &capture.ty)?;
        let output = frame.slot(read, &value.ty)?;
        frame.budget.reserve(4)?;
        let incoming = frame.slots.len();
        frame.slots.push(Local {
            node: operand,
            owner: incoming,
            cleanup: StorageCleanup::End,
            ..backing
        });
        frame.incoming_list_arrow = Some((view, incoming));
        let end = frame.emit(read, Action::None, Edges::End)?;
        let exported = frame.emit(read, Action::Export(output), Edges::Next(end))?;
        let cleanup = frame.cleanup(read, &[view], exported)?;
        let entry = frame.emit(
            read,
            read_op.action(view, Some(output)),
            Edges::Next(cleanup),
        )?;
        let capture = frame.slots[view].place(self.module);
        let incoming_backing = frame.slots[incoming].place(self.module);
        let body = flow::run(frame, entry, vec![view, incoming], None, false, None)?.mir;
        Ok((
            receiver,
            staged,
            ListArrow {
                invocation: child.invocation,
                body_site: child.body,
                source,
                capture,
                incoming_backing,
                body: Arc::new(body),
            },
        ))
    }
}
