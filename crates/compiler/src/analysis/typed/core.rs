//! Admission of delivered contracts from the installed capture.
use super::*;
use crate::analysis::scalars::BodyOwner;
use crate::analysis::{
    Binder, Declaration, DeclarationFacts, DeclarationShape, Payload, Type, TypeTerm,
};
use crate::loader::{DeclarationKind, Module, Namespace, Target};

mod inventories;
mod result;
pub(super) mod routine;
mod scalar;
pub(in crate::analysis) use inventories::InstalledOrdinaryScope;
pub(super) use inventories::{InventoryCompleteness, check_inventories};
pub(super) use result::check_result;
pub use result::{
    DisplayProtection, ErrorBridge, ErrorOperation, ErrorProviders, ErrorRepresentation, ResultRole,
};
pub(in crate::analysis) use scalar::PrimitiveCore;
pub(super) use scalar::check_show;
pub use scalar::{
    PrimitiveBinding, PrimitiveCoverage, PrimitiveOverlap, PrimitiveProtection, PrimitiveProvider,
    PrimitiveRole,
};

#[derive(Clone, Copy, Debug)]
pub struct OptionRole<'g> {
    pub declaration: &'g Declaration,
    pub some: &'g Declaration,
    pub none: &'g Declaration,
}

#[derive(Debug, Default)]
pub(super) struct InstalledCore<'g> {
    pub(super) option: Option<OptionRole<'g>>,
    pub(super) option_inspection: Option<&'g Declaration>,
    pub(super) result: Option<ResultRole<'g>>,
    pub(super) result_inspection: Option<&'g Declaration>,
    pub(super) show: Option<&'g Declaration>,
    pub(super) primitives: PrimitiveCore<'g>,
    pub(super) conversions: numeric::ConversionCore<'g>,
}

impl InstalledCore<'_> {
    /// No registration issue and no primitive overlap (metering rule 8).
    pub(super) fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            primitives,
            conversions,
            // Pinned role declarations: facts, not findings.
            option: _option,
            option_inspection: _option_inspection,
            result: _result,
            result_inspection: _result_inspection,
            show: _show,
        } = self;
        primitives.publishes_nothing_at_limit() && conversions.publishes_nothing_at_limit()
    }
}

/// Check the full installed role inventory without validating provider availability.
/// A partial capture still retains all individually validated roles.
pub(super) fn roles_complete(
    inventory: &OrdinaryAnalysis<'_>,
    core: &InstalledCore<'_>,
    collections: &CollectionCore<'_>,
    ownership: &ownership::Report<'_>,
    budget: &mut Budget,
) -> Work<bool> {
    let graph = inventory.graph();
    let (Some(option), Some(result), Some(list), Some(copy), Some(dispose)) = (
        core.option,
        core.result,
        collections.list,
        core.primitives.copy,
        core.primitives.dispose,
    ) else {
        return Ok(false);
    };
    let Some(constructor) = result.constructor else {
        return Ok(false);
    };
    if core.show.is_none()
        || core.primitives.equality.is_none()
        || result.display.is_none()
        || result.origin.is_none()
        || result.observe_cause.is_none()
        || !core.conversions.roles_complete()
        || !ownership.roles_complete()
        || collections.protection.iter().any(Option::is_none)
    {
        return Ok(false);
    }
    if !inventories::pinned_scope(graph, budget)? {
        return Ok(false);
    }
    let facade = pinned_module(graph, "std.rill", budget)?.unwrap();
    use Namespace::{TypeModule, ValueFunction};
    for (name, ns, expected) in [
        ("copy", ValueFunction, copy),
        ("dispose", ValueFunction, dispose),
        ("List", TypeModule, list.declaration),
        ("Option", TypeModule, option.declaration),
        ("Result", TypeModule, result.declaration),
        ("Err", TypeModule, result.wrapper),
        ("Err", ValueFunction, constructor),
        ("Error", TypeModule, result.default_error),
    ] {
        let mut seen = false;
        for binding in graph.bindings() {
            budget.step(1)?;
            if binding.module == facade.key
                && binding.name == name
                && binding.namespace == ns
                && binding.public
            {
                if seen || !matches!(&binding.target, Target::Declaration(id) if *id == expected.id)
                {
                    return Ok(false);
                }
                seen = true;
            }
        }
        if !seen {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn export<'g>(
    graph: &'g Graph,
    module: &'g Module,
    namespace: Namespace,
    name: &str,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    for binding in graph.bindings() {
        budget.step(1)?;
        if binding.module == module.key
            && binding.namespace == namespace
            && binding.name == name
            && binding.public
            && let Target::Declaration(id) = &binding.target
        {
            for declaration in &module.declarations {
                budget.step(1)?;
                if declaration.id == *id && declaration.public {
                    return Ok(Some(declaration));
                }
            }
        }
    }
    Ok(None)
}

pub(super) fn unconditional_declaration<'a, 'g>(
    inventory: &'a OrdinaryAnalysis<'g>,
    declaration: &Declaration,
    budget: &mut Budget,
) -> Work<Option<&'a DeclarationFacts<'g>>> {
    budget.step(1)?;
    let Some(facts) = inventory.type_facts().declarations.get(&declaration.id) else {
        return Ok(None);
    };
    budget.step(1)?;
    if inventory.declaration_unusable(&declaration.id) {
        return Ok(None);
    }
    budget.step(1)?;
    let plain = inventory
        .headers()
        .get(&declaration.id)
        .is_some_and(|header| header.defaults.is_empty() && header.requirements.is_empty());
    Ok(plain.then_some(facts))
}

pub(super) fn check_roles<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    budget: &mut Budget,
) -> Work<Option<OptionRole<'g>>> {
    let graph = inventory.graph();
    let Some(module) = pinned_module(graph, "option.rill", budget)? else {
        return Ok(None);
    };
    let Some(option) = export(graph, module, Namespace::TypeModule, "Option", budget)? else {
        return Ok(None);
    };
    let Some(facts) = unconditional_declaration(inventory, option, budget)? else {
        return Ok(None);
    };
    let (DeclarationShape::Enum { variants }, [parameter]) =
        (&facts.shape, facts.binders.as_slice())
    else {
        return Ok(None);
    };
    let [some, none] = variants.as_slice() else {
        return Ok(None);
    };
    let Payload::Tuple(payload) = &some.payload else {
        return Ok(None);
    };
    let [element] = payload.as_slice() else {
        return Ok(None);
    };
    if some.declaration.name.as_deref() != Some("Some")
        || none.declaration.name.as_deref() != Some("None")
        || !binder_is(element, parameter)
        || !matches!(none.payload, Payload::Unit)
    {
        return Ok(None);
    }
    budget.reserve(1)?;
    Ok(Some(OptionRole {
        declaration: option,
        some: some.declaration,
        none: none.declaration,
    }))
}

pub(super) fn pinned_module<'g>(
    graph: &'g Graph,
    path: &str,
    budget: &mut Budget,
) -> Work<Option<&'g Module>> {
    for module in graph.modules() {
        budget.step(1)?;
        if module.key.path == path && graph.is_pinned_standard_source(module) {
            return Ok(Some(module));
        }
    }
    Ok(None)
}

fn binder_is(term: &Type<'_>, binder: &Binder<'_>) -> bool {
    matches!(term.as_ref(), TypeTerm::Parameter { declaration, position }
        if **declaration == binder.declaration.id && *position == binder.position)
}

pub(super) fn parameter_is(term: &Type<'_>, binder: &Declaration) -> bool {
    matches!(term.as_ref(), TypeTerm::Parameter { declaration, position: 0 } if **declaration == binder.id)
}

pub(super) fn check_providers<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    bodies: &GenericReport<'g>,
    budget: &mut Budget,
    admitted: &mut InstalledCore<'g>,
) -> Work<()> {
    if let Some(role) = admitted.option {
        admitted.option_inspection =
            checked_inspection(inventory, bodies, budget, role.declaration, "isSome", 1)?;
    }
    if let Some(role) = admitted.result {
        admitted.result_inspection =
            checked_inspection(inventory, bodies, budget, role.declaration, "isOk", 2)?;
    }
    Ok(())
}

fn checked_inspection<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    bodies: &GenericReport<'g>,
    budget: &mut Budget,
    container: &'g Declaration,
    name: &str,
    binder_count: usize,
) -> Work<Option<&'g Declaration>> {
    let graph = inventory.graph();
    let module = pinned_module(graph, &container.id.module.path, budget)?
        .expect("admitted inspection roles use pinned source");
    let Some(inspection) = export(graph, module, Namespace::ValueFunction, name, budget)? else {
        return Ok(None);
    };
    if inspection.id.kind != DeclarationKind::Function {
        return Ok(None);
    }
    let Some(signature) = unconditional_declaration(inventory, inspection, budget)? else {
        return Ok(None);
    };
    let DeclarationShape::Function { parameters, result } = &signature.shape else {
        return Ok(None);
    };
    let [input] = parameters.as_slice() else {
        return Ok(None);
    };
    let TypeTerm::Reference {
        mutable: false,
        referent,
    } = input.ty.as_ref()
    else {
        return Ok(None);
    };
    let TypeTerm::Nominal {
        declaration,
        arguments,
    } = referent.as_ref()
    else {
        return Ok(None);
    };
    if **declaration != container.id
        || signature.binders.len() != binder_count
        || arguments.len() != binder_count
        || !arguments
            .iter()
            .zip(&signature.binders)
            .all(|(arg, binder)| binder_is(arg, binder))
        || !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
    {
        return Ok(None);
    }
    if let Some(checked) =
        bodies.checked_body(module, BodyOwner::Declaration(&inspection.id), budget)?
        && checked.flow.returns
        && !checked.flow.normal
    {
        budget.reserve(1)?;
        return Ok(Some(inspection));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{Limits, Prelude, Request, Selector, load};
    use std::{collections::BTreeMap, fs};

    #[test]
    fn closed_option_headers_cannot_add_defaults_or_requirements() {
        let root = std::env::temp_dir().join(format!("rill-option-header-{}", std::process::id()));
        let app = root.join("app");
        let standard = root.join("std");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&standard).unwrap();
        let entry = app.join("main.rill");
        fs::write(
            &entry,
            "
            enum Plain<T> { Some(T), None }
            enum Defaulted<T = bool> { Some(T), None }
            function inspect<T>(value: &Plain<T>): bool { return true; }
            function conditional<T>(value: &Plain<T>): bool where shareable(T) { return true; }
        ",
        )
        .unwrap();
        fs::write(
            standard.join("core.rill"),
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        )
        .unwrap();
        let selector = |name: &str| Selector {
            module: "core.rill".into(),
            name: name.into(),
        };
        let loaded = load(Request {
            entry,
            standard_root: standard,
            prelude: Prelude {
                copy: selector("copy"),
                dispose: selector("dispose"),
            },
            target: "aarch64-apple-darwin".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        });
        fs::remove_dir_all(&root).unwrap();
        let graph = loaded.unwrap();
        let limits = AnalysisLimits::default();
        let inventory = analyze_typed_inventory(&graph, limits);
        let mut budget = Budget {
            limits,
            statistics: inventory.statistics(),
        };
        let app = graph
            .modules()
            .iter()
            .find(|module| module.key.path == "main.rill")
            .unwrap();
        for (name, plain) in [
            ("Plain", true),
            ("Defaulted", false),
            ("inspect", true),
            ("conditional", false),
        ] {
            let declaration = app
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some(name))
                .unwrap();
            assert!(
                inventory
                    .type_facts()
                    .declarations
                    .contains_key(&declaration.id)
            );
            assert_eq!(
                unconditional_declaration(&inventory, declaration, &mut budget)
                    .unwrap()
                    .is_some(),
                plain,
                "{name}"
            );
        }
    }
}
