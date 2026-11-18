//! Admission of delivered contracts from the installed capture.
use super::*;
use crate::analysis::scalars::BodyOwner;
use crate::analysis::{Declaration, DeclarationShape, Payload, Type, TypeTerm};
use crate::loader::{DeclarationKind, Module, Namespace, Target};

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
}

fn export<'g>(
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

pub(super) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    bodies: &GenericReport<'g>,
    budget: &mut Budget,
    admitted: &mut InstalledCore<'g>,
) -> Work<()> {
    let graph = inventory.graph();
    let mut module = None;
    for candidate in graph.modules() {
        budget.step(1)?;
        if candidate.key.path == "option.rill" && graph.is_pinned_standard_source(candidate) {
            module = Some(candidate);
            break;
        }
    }
    let Some(module) = module else {
        return Ok(());
    };
    let types = inventory.type_facts();
    let Some(option) = export(graph, module, Namespace::TypeModule, "Option", budget)? else {
        return Ok(());
    };
    let Some(facts) = types.declarations.get(&option.id) else {
        return Ok(());
    };
    let (DeclarationShape::Enum { variants }, [parameter]) =
        (&facts.shape, facts.binders.as_slice())
    else {
        return Ok(());
    };
    let [some, none] = variants.as_slice() else {
        return Ok(());
    };
    let Payload::Tuple(payload) = &some.payload else {
        return Ok(());
    };
    let [element] = payload.as_slice() else {
        return Ok(());
    };
    let parameter_is = |term: &Type<'_>, binder: &Declaration| matches!(term.as_ref(), TypeTerm::Parameter { declaration, position: 0 } if **declaration == binder.id);
    if some.declaration.name.as_deref() != Some("Some")
        || none.declaration.name.as_deref() != Some("None")
        || !parameter_is(element, parameter.declaration)
        || !matches!(none.payload, Payload::Unit)
        || inventory.declaration_unusable(&option.id)
    {
        return Ok(());
    }
    budget.reserve(1)?;
    admitted.option = Some(OptionRole {
        declaration: option,
        some: some.declaration,
        none: none.declaration,
    });

    let Some(inspection) = export(graph, module, Namespace::ValueFunction, "isSome", budget)?
    else {
        return Ok(());
    };
    if inspection.id.kind != DeclarationKind::Function
        || inventory.declaration_unusable(&inspection.id)
    {
        return Ok(());
    }
    let Some(signature) = types.declarations.get(&inspection.id) else {
        return Ok(());
    };
    let (DeclarationShape::Function { parameters, result }, [binder]) =
        (&signature.shape, signature.binders.as_slice())
    else {
        return Ok(());
    };
    let [input] = parameters.as_slice() else {
        return Ok(());
    };
    let TypeTerm::Reference {
        mutable: false,
        referent,
    } = input.ty.as_ref()
    else {
        return Ok(());
    };
    let TypeTerm::Nominal {
        declaration,
        arguments,
    } = referent.as_ref()
    else {
        return Ok(());
    };
    if **declaration != option.id
        || !matches!(arguments.as_slice(), [argument] if parameter_is(argument, binder.declaration))
        || !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
    {
        return Ok(());
    }
    if let Some(checked) =
        bodies.checked_body(module, BodyOwner::Declaration(&inspection.id), budget)?
        && checked.flow.returns
        && !checked.flow.normal
    {
        budget.reserve(1)?;
        admitted.option_inspection = Some(inspection);
    }
    Ok(())
}
