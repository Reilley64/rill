//! Stage checked reference jobs and lower their reached child frames.
use super::list_arrows::{ArrowBody, ListArrow};
use super::*;
use crate::analysis::typed::{ScopedJoinCall, ScopedJoinReference};

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn stage_scoped_reference_results(
        &mut self,
        node: NodeId,
        join: &ScopedJoinCall<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let (tuple, tuple_depth) = self.ungroup_expression_at(join.argument, depth + 1)?;
        if self.module.syntax.nodes[tuple].children.as_slice()
            != join.slots.each_ref().map(|s| s.child.element)
        {
            return Err(Stop::Pending(node));
        }
        let all_incoming = join.slots.iter().all(|slot| {
            matches!(
                slot.reference_result,
                Some(ScopedJoinReference::Incoming { .. })
            )
        });
        let mut prepare = |slot: &crate::analysis::typed::ScopedJoinSlot<'g>| -> ResultOf<_> {
            let child = &slot.child;
            let (origin, arrow_depth) =
                self.ungroup_expression_at(child.element, tuple_depth + 1)?;
            if origin != slot.arrow().site.node
                || !std::ptr::eq(slot.arrow().site.module, &self.module.key)
            {
                return Err(Stop::Pending(node));
            }
            let body_depth = arrow_depth + 1;
            let (body, body_depth) = self.ungroup_expression_at(child.body.node, body_depth)?;
            let (entry, site, action) = match slot.reference_result.ok_or(Stop::Pending(node))? {
                ScopedJoinReference::Incoming { parameter, value } => {
                    if body != value.node {
                        return Err(Stop::Pending(node));
                    }
                    let (_, path) = self.named_owned_place(body)?.ok_or(Stop::Pending(node))?;
                    if !std::ptr::eq(path.root, parameter) {
                        return Err(Stop::Pending(node));
                    }
                    let from = self.resolve_place(body, path)?;
                    // The sole capture is a copied shared descriptor. Stage
                    // its value with its backing, not a borrow of its slot.
                    let entry = self.slot(
                        if all_incoming { body } else { origin },
                        &slot.arrow().result,
                    )?;
                    (
                        entry,
                        body,
                        Action::Read {
                            from,
                            into: Some(entry),
                        },
                    )
                }
                ScopedJoinReference::ChildLocal { .. } => {
                    let closure = &self.normal_value(origin)?.ty;
                    let entry = self.slot(origin, closure)?;
                    (entry, origin, Action::Init(entry))
                }
            };
            Ok((entry, body_depth, site, action))
        };
        let plans = [prepare(&join.slots[0])?, prepare(&join.slots[1])?];
        let (action, receivers) = if all_incoming {
            let into = into.ok_or(Stop::Pending(node))?;
            if self.module.key.origin != Origin::Application
                || self.slots[into].members != 2
                || self.slots[into].custom_disposer.is_some()
            {
                return Err(Stop::Pending(node));
            }
            let mut receiver = |i: usize| -> ResultOf<_> {
                self.element(node, into, i)?;
                let (staged, _, _, action) = plans[i];
                let Action::Read { from, .. } = action else {
                    return Err(Stop::Pending(node));
                };
                let lookup = 1 + self.incoming_shared_i32.len().max(1).ilog2() as usize;
                self.budget.step(lookup)?;
                if !self.incoming_shared_i32.contains(&from) {
                    self.budget.step(lookup)?;
                    self.budget.reserve(1)?;
                    self.incoming_shared_i32.insert(from);
                }
                let slot = &join.slots[i];
                let child = &slot.child;
                let origin = slot.arrow().site.node;
                let closure = &self.normal_value(origin)?.ty;
                Ok((
                    self.slot(origin, closure)?,
                    ListArrow {
                        invocation: child.invocation,
                        body_site: child.body,
                        source: from,
                        capture: self.slots[staged].place(self.module),
                        body: ArrowBody::Reference(slot.arrow().result.clone()),
                    },
                ))
            };
            let (a, left) = receiver(0)?;
            let (b, right) = receiver(1)?;
            self.budget.reserve(3)?;
            let pair = self.list_arrow_pairs.len();
            self.list_arrow_pairs.push(ListArrowPair {
                implementation: &join.role.implementation.id,
                body: left.invocation,
                children: [left, right],
            });
            let receivers = [a, b];
            (
                Action::ScopedJoin {
                    pair: ScopedFrames::Prepared(pair),
                    receivers,
                    into,
                    role: join.role,
                },
                Some(receivers),
            )
        } else {
            (
                Action::CheckScopedReferenceResults {
                    entries: plans.map(|p| p.0),
                    body_depths: plans.map(|p| p.1),
                },
                None,
            )
        };
        let mut entry = self.emit(node, action, Edges::Next(next))?;
        for (i, (capture, _, site, action)) in plans.into_iter().enumerate().rev() {
            if let Some(receivers) = receivers {
                let into = receivers[i];
                entry = self.emit(
                    self.slots[into].node,
                    Action::FormListArrow {
                        capture,
                        into,
                        mode: InvocationMode::ReadOnly,
                    },
                    Edges::Next(entry),
                )?;
            }
            entry = self.emit(site, action, Edges::Next(entry))?;
        }
        Ok(entry)
    }
}

pub(in crate::analysis::ownership) fn lower_reference_child<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    child: &ListArrow<'g>,
    origin: NodeId,
    ty: &Type<'g>,
    budget: &mut Budget,
) -> ResultOf<Arc<Mir<'g>>> {
    // This child receives a descriptor, not a loan of the parent's slot.
    // Its incoming key is bound to the actual carried loan by the join.
    let body = child.body_site.node;
    let value = child.capture.root.node;
    let mut frame = Build::with_facts(
        typed,
        module,
        body,
        None,
        BuildFacts::Local(BTreeMap::new()),
        budget,
    );
    let capture = frame.slot(value, ty)?;
    let output = frame.slot(origin, ty)?;
    frame.budget.reserve(2)?;
    frame.incoming_shared_i32.insert(capture);
    let end = frame.emit(body, Action::None, Edges::End)?;
    let cleanup = frame.cleanup(body, &[capture], end)?;
    let export = frame.emit(value, Action::Export(output), Edges::Next(cleanup))?;
    let entry = frame.emit(
        value,
        Action::Read {
            from: capture,
            into: Some(output),
        },
        Edges::Next(export),
    )?;
    Ok(Arc::new(
        flow::run(frame, entry, vec![capture], None, false, None)?.mir,
    ))
}

pub(in crate::analysis::ownership) fn check_scoped_reference_results<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    facts: &NodeIndex<'a, 'g>,
    node: NodeId,
    body_depths: [usize; 2],
    budget: &mut Budget,
) -> ResultOf<()> {
    budget.step(1)?;
    let Some(NodeFactKind::Expression(value)) = facts.get(&node).map(|f| &f.kind) else {
        return Err(Stop::Pending(node));
    };
    let join = value
        .call
        .as_ref()
        .and_then(|call| call.scoped_join.as_deref())
        .ok_or(Stop::Pending(node))?;
    for (slot, depth) in join.slots.iter().zip(body_depths) {
        budget.step(1)?;
        let Some(ScopedJoinReference::ChildLocal { binding, returning }) = slot.reference_result
        else {
            continue;
        };
        // Run the actual binding initializer, borrow and return export in an
        // isolated frame. Export tests the loan before the local is cleaned.
        let body = slot.child.body.node;
        let expression = slot.child.expression;
        let mut frame = Build::with_facts(
            typed,
            module,
            body,
            None,
            BuildFacts::Indexed(facts),
            budget,
        );
        let initializer = *module.syntax.nodes[binding.node].children.last().unwrap();
        let mut locals = Vec::new();
        frame.declare_binding(binding.node, initializer, &mut locals, depth + 2)?;
        let end = frame.emit(body, Action::None, Edges::End)?;
        let cleanup = frame.cleanup(body, &locals, end)?;
        let ty = &slot.arrow().result;
        let output = frame.slot(expression, ty)?;
        let export = frame.emit(returning.node, Action::Export(output), Edges::Next(cleanup))?;
        let borrow = frame.expression(expression, Some(output), export, depth + 2)?;
        let entry = frame.statement(
            binding.node,
            borrow,
            Exits {
                returning: None,
                looping: None,
            },
            depth + 1,
        )?;
        flow::run(frame, entry, Vec::new(), None, false, None)?;
    }
    // A child-local route never publishes a transition.
    Ok(())
}
