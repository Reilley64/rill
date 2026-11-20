//! Identity of the conditional installed scoped-work group.
use super::*;
use crate::analysis::ordinary::{Compatibility, Placement};

/// The exact source declarations. Neither wrapper nor bridge identity proves
/// that the compiler operation is available or that an ordinary call chose it.
#[derive(Clone, Copy, Debug)]
pub struct RoutineRole<'g> {
    pub family: &'g Declaration,
    pub implementation: &'g Declaration,
    pub bridge: &'g Declaration,
}

pub(in crate::analysis::typed) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    budget: &mut Budget,
) -> Work<Option<RoutineRole<'g>>> {
    let graph = inventory.graph();
    if !graph.is_pinned_standard_source(module) {
        return Ok(None);
    }
    let Some(family) = export(graph, module, Namespace::ValueFunction, "join", budget)? else {
        return Ok(None);
    };
    let Some(facts) = inventory.captured_family(&family.id) else {
        return Ok(None);
    };
    let mut implementation = None;
    for body in &facts.implementations {
        budget.step(1)?;
        if body.source.declaration.module == module.key
            && body.placement == Placement::FamilyModule
            && body.compatibility == Compatibility::Compatible
            && body.alternatives.len() == 1
            && !inventory.declaration_unusable(&body.source.declaration)
        {
            if implementation.is_some() {
                return Ok(None);
            }
            implementation = Some(body.header.declaration);
        }
    }
    let Some(implementation) = implementation else {
        return Ok(None);
    };
    for bridge in &module.declarations {
        budget.step(1)?;
        if !bridge.public
            && bridge.scope.is_none()
            && bridge.id.kind == DeclarationKind::Family
            && bridge.name.as_deref() == Some("joinScopedPair")
            && !inventory.declaration_unusable(&bridge.id)
            && inventory
                .captured_family(&bridge.id)
                .is_some_and(|f| f.implementations.is_empty())
        {
            return Ok(Some(RoutineRole {
                family,
                implementation,
                bridge,
            }));
        }
    }
    Ok(None)
}
