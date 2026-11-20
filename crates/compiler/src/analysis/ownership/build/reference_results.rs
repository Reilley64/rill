//! Check child-local escape without admitting reference-result publication.
use super::*;
use crate::analysis::typed::ScopedJoinReference;

pub(in crate::analysis::ownership) fn check_scoped_reference_results<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    facts: &NodeIndex<'a, 'g>,
    node: NodeId,
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
    for slot in &join.slots {
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
        frame.declare_binding(binding.node, initializer, &mut locals, 1)?;
        let end = frame.emit(body, Action::None, Edges::End)?;
        let cleanup = frame.cleanup(body, &locals, end)?;
        let ty = &slot.arrow().result;
        let output = frame.slot(expression, ty)?;
        let export = frame.emit(returning.node, Action::Export(output), Edges::Next(cleanup))?;
        let borrow = frame.expression(expression, Some(output), export, 2)?;
        let entry = frame.statement(
            binding.node,
            borrow,
            Exits {
                returning: None,
                looping: None,
            },
            1,
        )?;
        flow::run(frame, entry, Vec::new(), None, false, None)?;
    }
    // No transition is published: even caller-backed descriptors still need
    // a checked result retention contract through stop, cleanup and last use.
    Ok(())
}
