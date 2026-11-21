//! The bounded scoped pair with one fresh Result and one Bool job.
use super::*;
use crate::analysis::typed::{ScopedJoinCall, ScopedJoinSlot};

struct ValueJob<'g> {
    origin: NodeId,
    expression: NodeId,
    depth: usize,
    capture: Option<&'g DeclarationId>,
}

impl<'a, 'g> Build<'a, 'g, '_> {
    // Only a selected fresh join can create this pair layout. Named whole
    // moves may use the exact layout already allocated for that join.
    pub(super) fn scoped_result_destination(
        &mut self,
        source: NodeId,
        site: NodeId,
        ty: &Type<'g>,
    ) -> ResultOf<Option<Slot>> {
        let TypeTerm::Tuple(types) = ty.as_ref() else {
            return Ok(None);
        };
        let [left, right] = types.as_slice() else {
            return Ok(None);
        };
        let result = |t: &Type<'g>| {
            bool_result_success(t) && matches!(self.scalar_result_role(t), Some((_, None)))
        };
        let position = match (scalar::scalar(left), scalar::scalar(right)) {
            (_, Some(Primitive::Bool)) if result(left) => 0,
            (Some(Primitive::Bool), _) if result(right) => 1,
            _ => return Ok(None),
        };
        let source = self.ungroup(source)?;
        let facts = self.normal_value(source)?;
        if facts.ty != *ty {
            return Ok(None);
        }
        let join = facts
            .call
            .as_ref()
            .and_then(|call| call.scoped_join.as_deref());
        if !join.is_some_and(|join| {
            join.slots[position].result_origin.is_some()
                && join.slots[1 - position].result_origin.is_none()
                && join
                    .slots
                    .iter()
                    .zip(types)
                    .all(|(slot, ty)| slot.arrow().result == *ty)
        }) {
            if let Some(path) = &facts.path
                && facts.access == Access::Owned
                && path.projections.is_empty()
            {
                graph::charge_key(&path.root.module, self.locals.len(), self.budget)?;
                if let Some(&from) = self.locals.get(path.root)
                    && self.slots[from].members == 2
                    && matches!(
                        self.slots[from + 1 + position].cleanup,
                        StorageCleanup::FreshBoolResult(_)
                    )
                    && self.slots[from + 2 - position].cleanup == StorageCleanup::End
                {
                    return self.clone_region(from, site).map(Some);
                }
            }
            return Ok(None);
        }
        if self.outer_disposer(site, ty)?.is_some() {
            return Err(Stop::Pending(site));
        }
        let root = self.allocate_slot(site, true, StorageCleanup::End, None)?;
        self.slots[root].members = 2;
        for (i, ty) in types.iter().enumerate() {
            self.budget.step(1)?;
            let member = self.slot(site, ty)?;
            if !match self.slots[member].cleanup {
                StorageCleanup::FreshBoolResult(_) => i == position,
                StorageCleanup::End => i != position,
                _ => false,
            } {
                return Err(Stop::Pending(site));
            }
            self.slots[member].owner = root;
            self.slots[member].projection = Some(Projection::TupleElement(i));
        }
        Ok(Some(root))
    }

    pub(super) fn stage_fresh_scoped_result(
        &mut self,
        node: NodeId,
        join: &ScopedJoinCall<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let pending = Stop::Pending(node);
        let into = into.ok_or(pending.clone())?;
        let (tuple, tuple_depth) = self.ungroup_expression_at(join.argument, depth + 1)?;
        if self.module.key.origin != Origin::Application
            || self.slots[into].members != 2
            || self.slots[into].custom_disposer.is_some()
            || self.module.syntax.nodes[tuple].children.as_slice()
                != join.slots.each_ref().map(|slot| slot.child.element)
            || !matches!(
                join.slots.each_ref().map(|slot| slot.result_origin),
                [Some(_), None] | [None, Some(_)]
            )
        {
            return Err(pending);
        }
        let mut plan = |i: usize| -> ResultOf<_> {
            self.element(node, into, i)?;
            let slot = &join.slots[i];
            let job = self.scoped_value_job(slot, tuple_depth + 1)?;
            let capture = if let Some(root) = job.capture {
                let (_, path) = self
                    .named_owned_place(job.expression)?
                    .ok_or(Stop::Pending(job.expression))?;
                if !std::ptr::eq(path.root, root) || !path.projections.is_empty() {
                    return Err(Stop::Pending(job.expression));
                }
                let from = self.resolve_place(job.expression, path)?;
                if self.slots[from].moves || self.slots[from].cleanup != StorageCleanup::End {
                    return Err(Stop::Pending(job.expression));
                }
                Some(from)
            } else {
                None
            };
            let closure = &self.normal_value(job.origin)?.ty;
            let receiver = self.slot(job.origin, closure)?;
            Ok((receiver, capture, job.expression))
        };
        let jobs = [plan(0)?, plan(1)?];
        let mut next = self.emit(
            node,
            Action::ScopedJoin {
                pair: ScopedFrames::FreshResult { tuple_depth },
                receivers: jobs.map(|(receiver, _, _)| receiver),
                into,
                role: join.role,
            },
            Edges::Next(next),
        )?;
        // No checkpoint between forming the two environments, or between a
        // scalar copy and its transfer into its own environment.
        for (into, capture, expression) in jobs.into_iter().rev() {
            let origin = self.slots[into].node;
            next = self.emit(
                if capture.is_some() {
                    expression
                } else {
                    origin
                },
                Action::FormScopedValueArrow { capture, into },
                Edges::Next(next),
            )?;
        }
        Ok(next)
    }

    fn scoped_value_job(
        &mut self,
        slot: &ScopedJoinSlot<'g>,
        depth: usize,
    ) -> ResultOf<ValueJob<'g>> {
        let child = &slot.child;
        let (origin, depth) = self.ungroup_expression_at(child.element, depth)?;
        let pending = Stop::Pending(origin);
        let arrow = slot.arrow();
        if origin != arrow.site.node
            || !std::ptr::eq(arrow.site.module, &self.module.key)
            || !std::ptr::eq(child.body.module, &self.module.key)
            || slot.reference_result.is_some()
            || child.body.node != child.expression
        {
            return Err(pending);
        }
        let (expression, expression_depth) =
            self.ungroup_expression_at(child.expression, depth + 1)?;
        let facts = self.normal_value(expression)?;
        let syntax = &self.module.syntax.nodes[expression];
        let capture = if let Some(occurrence) = slot.result_origin {
            if occurrence.node != expression
                || !std::ptr::eq(occurrence.module, &self.module.key)
                || !arrow.captures.is_empty()
                || !bool_result_success(&arrow.result)
                || !matches!(self.scalar_result_role(&arrow.result), Some((_, None)))
            {
                return Err(pending);
            }
            // Typed Result-use has already checked the literal payload and
            // actual variant at this occurrence. General expression lowering
            // rechecks the installed constructor and its storage contract.
            None
        } else {
            if scalar::scalar(&arrow.result) != Some(Primitive::Bool)
                || facts.ty != arrow.result
                || facts.call.is_some()
                || facts.data.is_some()
            {
                return Err(pending);
            }
            match arrow.captures.as_slice() {
                [] if syntax.kind == NodeKind::Literal => None,
                [capture]
                    if syntax.kind == NodeKind::ValueReference
                        && capture.reachable
                        && capture.through.is_none()
                        && capture.form == CaptureForm::Value
                        && capture.ty == arrow.result
                        && capture.storage.as_ref() == Some(&arrow.result)
                        && capture.path.projections.is_empty()
                        && std::ptr::eq(capture.site.module, &self.module.key)
                        && self.ungroup_expression_at(capture.site.node, depth + 1)?.0
                            == expression
                        && facts.path.as_ref().is_some_and(|path| {
                            path.projections.is_empty()
                                && std::ptr::eq(path.root, capture.path.root)
                        }) =>
                {
                    Some(capture.path.root)
                }
                _ => return Err(pending),
            }
        };
        Ok(ValueJob {
            origin,
            expression,
            depth: expression_depth,
            capture,
        })
    }

    fn lower_scoped_value_child(
        &mut self,
        slot: &ScopedJoinSlot<'g>,
        tuple_depth: usize,
        environment: Place<'g>,
        result: Place<'g>,
    ) -> ResultOf<ScopedJoinFrame<'g>> {
        let job = self.scoped_value_job(slot, tuple_depth + 1)?;
        if environment.root.node != job.origin || environment.projection.is_some() {
            return Err(Stop::Pending(job.origin));
        }
        let arrow = slot.arrow();
        let environment_type = &self.normal_value(job.origin)?.ty;
        let mut frame = Build::with_facts(
            self.typed,
            self.module,
            slot.child.body.node,
            None,
            BuildFacts::Indexed(self.facts.nodes()),
            self.budget,
        );
        let received = if let Some(capture) = job.capture {
            frame.local(capture, job.origin, &arrow.result)?
        } else {
            frame.slot(job.origin, environment_type)?
        };
        let output = frame.slot(job.expression, &arrow.result)?;
        let end = frame.emit(job.expression, Action::None, Edges::End)?;
        let cleanup = frame.cleanup(job.expression, &[received], end)?;
        let export = frame.emit(
            job.expression,
            Action::StageScopedValue(output),
            Edges::Next(cleanup),
        )?;
        let entry = frame.expression(job.expression, Some(output), export, job.depth)?;
        let capture = job
            .capture
            .map(|_| frame.slots[received].place(self.module));
        let body = flow::run(frame, entry, vec![received], None, false, None)?.mir;
        self.budget.reserve(1)?;
        Ok(ScopedJoinFrame {
            origin: arrow.site,
            invocation: slot.child.invocation,
            body_site: slot.child.body,
            environment,
            capture,
            backing: None,
            incoming_list_backing: None,
            exclusive: false,
            result,
            body: Arc::new(body),
            value: true,
            result_origin: slot.result_origin,
        })
    }
}

pub(in crate::analysis::ownership) fn lower_fresh_scoped_children<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    facts: &'a NodeIndex<'a, 'g>,
    node: NodeId,
    tuple_depth: usize,
    bindings: [(Place<'g>, Place<'g>); 2],
    budget: &mut Budget,
) -> ResultOf<[ScopedJoinFrame<'g>; 2]> {
    let mut build = Build::with_facts(
        typed,
        module,
        node,
        None,
        BuildFacts::Indexed(facts),
        budget,
    );
    let join = build
        .normal_value(node)?
        .call
        .as_ref()
        .and_then(|call| call.scoped_join.as_deref())
        .ok_or(Stop::Pending(node))?;
    let [(a, first), (b, second)] = bindings;
    Ok([
        build.lower_scoped_value_child(&join.slots[0], tuple_depth, a, first)?,
        build.lower_scoped_value_child(&join.slots[1], tuple_depth, b, second)?,
    ])
}
