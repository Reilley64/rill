//! Check child-local escape without admitting reference-result publication.
use super::*;
use crate::analysis::typed::{ScopedJoinCall, ScopedJoinReference};

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn stage_scoped_reference_results(
        &mut self,
        node: NodeId,
        join: &ScopedJoinCall<'g>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let (tuple, tuple_depth) = self.ungroup_expression_at(join.argument, depth + 1)?;
        if self.module.syntax.nodes[tuple].children.as_slice()
            != join.slots.each_ref().map(|s| s.child.element)
        {
            return Err(Stop::Pending(node));
        }
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
                    let entry = self.slot(origin, &slot.arrow().result)?;
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
        let entries = plans.map(|p| p.0);
        let body_depths = plans.map(|p| p.1);
        let mut entry = self.emit(
            node,
            Action::CheckScopedReferenceResults {
                entries,
                body_depths,
            },
            Edges::Next(next),
        )?;
        for (_, _, site, action) in plans.into_iter().rev() {
            entry = self.emit(site, action, Edges::Next(entry))?;
        }
        Ok(entry)
    }
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
    // No transition is published: even caller-backed descriptors still need
    // a checked result retention contract through stop, cleanup and last use.
    Ok(())
}
