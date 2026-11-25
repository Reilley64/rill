//! Closure of the pinned role and protection catalogues.
use super::*;
use crate::analysis::ordinary::{Compatibility, Placement, source};
use crate::loader::{DeclarationId, GraphId, Origin};
use crate::parser::NodeKind;

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::analysis::typed) struct InventoryCompleteness {
    pub providers: bool,
    pub protection: bool,
    pub protected_violation: bool,
}

/// Families eligible for ordinary source comparison in the installed capture.
/// This does not select a provider or clear a protected standard operation.
#[derive(Clone)]
pub(in crate::analysis) struct InstalledOrdinaryScope<'g> {
    context: GraphId,
    routine: Option<super::routine::RoutineRole<'g>>,
    output: Vec<&'g DeclarationId>,
}

impl<'g> InstalledOrdinaryScope<'g> {
    pub(in crate::analysis::typed) fn from_closed_roles(
        graph: &Graph,
        routine: Option<super::routine::RoutineRole<'g>>,
        output: &[(&'g DeclarationId, PrimitiveOperation)],
    ) -> Self {
        Self {
            context: graph.id(),
            routine,
            output: output.iter().map(|(family, _)| *family).collect(),
        }
    }

    pub(in crate::analysis) fn contains(&self, family: &DeclarationId) -> bool {
        family.context == self.context
            && family.kind == DeclarationKind::Family
            && (family.module.origin == Origin::Application
                || self.routine.is_some_and(|role| &role.family.id == family)
                || self.output.contains(&family))
    }

    pub(in crate::analysis) fn routine(&self) -> Option<super::routine::RoutineRole<'g>> {
        self.routine
    }
}

pub(in crate::analysis::typed) fn check_inventories(
    inventory: &OrdinaryAnalysis<'_>,
    core: &InstalledCore<'_>,
    collections: &CollectionCore<'_>,
    ownership: &ownership::Report<'_>,
    collection_overlaps: &[CollectionOverlap<'_>],
    budget: &mut Budget,
    status: &mut InventoryCompleteness,
) -> Work<()> {
    let conversion_violation = core.conversions.protected_violation(budget)?;
    budget.step(core.primitives.protection.len())?;
    status.protected_violation = conversion_violation
        || !ownership.overlaps.is_empty()
        || !collection_overlaps.is_empty()
        || core
            .primitives
            .protection
            .iter()
            .any(|p| !p.overlaps.is_empty())
        || core
            .result
            .is_some_and(|r| r.constructor.is_some() && !r.constructor_protected);
    if pinned_scope(inventory.graph(), budget)? {
        status.providers = providers_complete(inventory, core, collections, ownership, budget)?;
        // Enumeration can finish with a violation. Keep it distinct from
        // a missing provider or an unresolved overlapping input region.
        status.protection = protection_complete(inventory, core, collections, ownership, budget)?;
    }
    Ok(())
}

pub(super) fn pinned_scope(graph: &Graph, budget: &mut Budget) -> Work<bool> {
    let Some(keys) = graph.installed_source_keys() else {
        return Ok(false);
    };
    for key in keys {
        if pinned_module(graph, key, budget)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn providers_complete(
    inventory: &OrdinaryAnalysis<'_>,
    core: &InstalledCore<'_>,
    collections: &CollectionCore<'_>,
    ownership: &ownership::Report<'_>,
    budget: &mut Budget,
) -> Work<bool> {
    let Some(result) = core.result else {
        return Ok(false);
    };
    Ok(core.option_inspection.is_some()
        && core.result_inspection.is_some()
        && result.providers.is_some_and(|p| {
            p.renderers.iter().all(Option::is_some) && p.observers.iter().all(Option::is_some)
        })
        && core.primitives.providers_complete(budget)?
        && collections.providers_complete(budget)?
        && ownership.providers_complete(budget)?
        && core.conversions.providers_complete(budget)?
        && written_providers(inventory, core, result, budget)?)
}

fn written_providers(
    inventory: &OrdinaryAnalysis<'_>,
    core: &InstalledCore<'_>,
    result: ResultRole<'_>,
    budget: &mut Budget,
) -> Work<bool> {
    // Closed operations and private bridge declarations are validated by their
    // owners above. These named operations are supplied by pinned Rill bodies.
    for (key, names) in [
        (
            "option.rill",
            ["asRef", "asMut", "map", "andThen"].as_slice(),
        ),
        (
            "result.rill",
            &[
                "Ok",
                "formatError",
                "value",
                "intoValue",
                "ok",
                "err",
                "map",
                "mapErr",
                "andThen",
            ],
        ),
        ("error.rill", &["error", "formatLocation", "formatCause"]),
    ] {
        let Some(module) = pinned_module(inventory.graph(), key, budget)? else {
            return Ok(false);
        };
        for name in names {
            let Some(decl) = export(
                inventory.graph(),
                module,
                Namespace::ValueFunction,
                name,
                budget,
            )?
            else {
                return Ok(false);
            };
            budget.step(2)?;
            if decl.id.kind != DeclarationKind::Function
                || inventory.declaration_unusable(&decl.id)
                || !has_body(module, decl, budget)?
            {
                return Ok(false);
            }
        }
    }
    // Counts refer only to the exact pinned source bodies for each canonical
    // family. Client registrations cannot substitute for a missing body.
    for (family, sources) in [
        (
            core.show,
            [
                ("format.rill", Primitive::ALL.len() + 2),
                ("result.rill", 1),
                ("error.rill", 1),
            ]
            .as_slice(),
        ),
        (
            core.primitives.equality,
            &[
                ("compare.rill", Primitive::ALL.len() - 1),
                ("number.rill", 2),
                ("char.rill", 1),
            ],
        ),
        (
            result.display,
            &[
                ("error.rill", Primitive::ALL.len() + 1),
                ("number.rill", 2),
                ("char.rill", 1),
                ("string.rill", 1),
            ],
        ),
        (result.origin, &[("result.rill", 1), ("error.rill", 1)]),
        (
            result.observe_cause,
            &[("result.rill", 1), ("error.rill", 1)],
        ),
    ] {
        let Some(family) = family.and_then(|family| inventory.captured_family(&family.id)) else {
            return Ok(false);
        };
        for &(key, required) in sources {
            let Some(module) = pinned_module(inventory.graph(), key, budget)? else {
                return Ok(false);
            };
            let mut found = 0;
            for body in &family.implementations {
                budget.step(1)?;
                let decl = body.header.declaration;
                if decl.id.module == module.key
                    && body.compatibility == Compatibility::Compatible
                    && body.placement != Placement::OutsideOwners
                    && !body.alternatives.is_empty()
                    && !module.syntax.nodes[body.source.body_node]
                        .children
                        .is_empty()
                {
                    found += 1;
                }
            }
            if found != required {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn has_body(module: &Module, declaration: &Declaration, budget: &mut Budget) -> Work<bool> {
    let children = &module.syntax.nodes[declaration.node].children;
    budget.step(children.len())?;
    Ok(children.iter().any(|&child| {
        let node = &module.syntax.nodes[child];
        node.kind == NodeKind::Block && !node.children.is_empty()
    }))
}

fn protection_complete(
    inventory: &OrdinaryAnalysis<'_>,
    core: &InstalledCore<'_>,
    collections: &CollectionCore<'_>,
    ownership: &ownership::Report<'_>,
    budget: &mut Budget,
) -> Work<bool> {
    let Some(result) = core.result else {
        return Ok(false);
    };
    if !(result.constructor.is_some()
        && result.providers.is_some()
        && result.display_protection.is_some()
        && core.primitives.protection_complete(budget)?
        && collections.protection_complete(budget)?
        && ownership.protection_complete(budget)?
        && core.conversions.protection_complete(budget)?)
    {
        return Ok(false);
    }
    // The owning catalogues assign no protected action to show, eq, display
    // or the ordinary error observers. Resolve their captured families before
    // closing that scoped absence. This does not clear any other family or
    // replace ordinary selection and evidence checking at a call.
    for family in [
        core.show,
        core.primitives.equality,
        result.origin,
        result.observe_cause,
    ] {
        let Some(family) = family else {
            return Ok(false);
        };
        if source::admitted_family(inventory, &family.id, budget)?.is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}
