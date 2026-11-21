//! Ordinary source frames that need no storage for a rigid referent.
use super::*;
use crate::analysis::{
    Binder, Substitution,
    ordinary::{Compatibility, ImplementationFacts},
    scalars::{CallFacts, ExpressionFacts},
};
use crate::{loader::Origin, parser::NodeKind};

pub(super) fn source_body(
    module: &Module,
    family: &DeclarationFacts<'_>,
    implementation: &ImplementationFacts<'_>,
    budget: &mut Budget,
) -> ResultOf<NodeId> {
    let header = &implementation.header;
    let pending = Stop::Pending(header.declaration.node);
    let body = implementation.source.body_node;
    budget.step(1)?;
    if implementation.compatibility != Compatibility::Compatible
        || !private_family(module, family)
        || rigid_binder(family).is_none()
        || rigid_binder(header).is_none()
    {
        return Err(pending);
    }

    // Require the entire written body, not a folded Bool or a checked call
    // result. This leaves no way to evaluate, copy or dispose of the referent.
    let nodes = &module.syntax.nodes;
    budget.step(5)?;
    for declaration in [family.declaration, header.declaration] {
        let [signature, ..] = nodes[declaration.node].children.as_slice() else {
            return Err(pending);
        };
        if nodes[*signature]
            .children
            .last()
            .is_some_and(|&node| nodes[node].kind == NodeKind::Requirements)
        {
            return Err(pending);
        }
    }
    let (NodeKind::Block, [statement]) = (nodes[body].kind, nodes[body].children.as_slice()) else {
        return Err(pending);
    };
    let (NodeKind::Return, [literal]) = (
        nodes[*statement].kind,
        nodes[*statement].children.as_slice(),
    ) else {
        return Err(pending);
    };
    let literal = &nodes[*literal];
    if literal.kind != NodeKind::Literal || !matches!(literal.span.bytes(), b"true" | b"false") {
        return Err(pending);
    }

    // Normal frame lowering allocates the incoming descriptor only: its
    // cleanup is EndStorage, and the result is independent of the descriptor.
    Ok(body)
}

// Admit a closed input/result contract, not the selected body. The ordinary
// dependency must still reach that exact source frame before the caller checks.
pub(super) fn call_target<'g>(
    build: &mut Build<'_, 'g, '_>,
    node: NodeId,
    call: &CallFacts<'g>,
    facts: &ExpressionFacts<'g>,
) -> ResultOf<&'g DeclarationId> {
    let pending = Stop::Pending(node);
    let module = build.module;
    build.budget.step(1)?;
    let syntax = &module.syntax.nodes[node];
    let (NodeKind::Call, [_, argument], [typed_argument]) = (
        syntax.kind,
        syntax.children.as_slice(),
        call.arguments.as_slice(),
    ) else {
        return Err(pending);
    };
    if argument != typed_argument
        || call.pipe_input.is_some()
        || !call.defaults.is_empty()
        || facts.admission.is_some()
        || facts.ordinary_context.is_some()
    {
        return Err(pending);
    }
    let TypeTerm::Reference {
        mutable: false,
        referent,
    } = build.normal_value(*argument)?.ty.as_ref()
    else {
        return Err(pending);
    };
    let (Some(selection), [family_arg]) = (&call.selection, call.substitution.as_slice()) else {
        return Err(pending);
    };
    let [impl_arg] = selection.substitution.as_slice() else {
        return Err(pending);
    };
    build.budget.step(1)?;
    if selection.alternative != 0
        || scalar::scalar(referent).is_none()
        || family_arg.argument != *referent
        || impl_arg.argument != *referent
    {
        return Err(pending);
    }
    let family = build
        .typed
        .inventory()
        .captured_family(call.callee)
        .ok_or(Stop::Pending(node))?;
    let family_header = &family.header;
    if !std::ptr::eq(call.callee, &family_header.declaration.id)
        || !private_family(module, family_header)
        || !rigid_binder(family_header).is_some_and(|binder| matches_binder(family_arg, binder))
    {
        return Err(pending);
    }
    for implementation in &family.implementations {
        build.budget.step(1)?;
        let header = &implementation.header;
        if !std::ptr::eq(selection.implementation, &implementation.source.declaration) {
            continue;
        }
        if implementation.compatibility == Compatibility::Compatible
            && header.declaration.id.module == module.key
            && rigid_binder(header).is_some_and(|binder| matches_binder(impl_arg, binder))
            && matches!(&header.shape, DeclarationShape::Function { result, .. } if result == &facts.ty)
        {
            return Ok(&header.declaration.id);
        }
        break;
    }
    Err(pending)
}

// Targeted queries do not share the whole-graph root index.
pub(super) fn lower_selected<'a, 'g>(
    typed: &'a TypedReport<'g>,
    module: &'g Module,
    id: &DeclarationId,
    lowering: Lowering<'_, 'a, 'g>,
) -> ResultOf<Lowered<'a, 'g>> {
    let pending = || Stop::Pending(module.syntax.root);
    lowering
        .budget
        .step(1 + module.declarations.len().max(1).ilog2() as usize)?;
    let index = module
        .declarations
        .binary_search_by(|candidate| candidate.id.cmp(id))
        .map_err(|_| pending())?;
    let declaration = &module.declarations[index];
    if !std::ptr::eq(id, &declaration.id) {
        return Err(pending());
    }
    lower_implementation(typed, module, declaration, lowering)
}

fn matches_binder(argument: &Substitution<'_>, binder: &Binder<'_>) -> bool {
    std::ptr::eq(argument.binder, &binder.declaration.id) && argument.position == binder.position
}

fn private_family(module: &Module, header: &DeclarationFacts<'_>) -> bool {
    let declaration = header.declaration;
    module.key.origin == Origin::Application
        && declaration.id.kind == DeclarationKind::Family
        && declaration.id.module == module.key
        && !declaration.public
}

fn rigid_binder<'a, 'g>(header: &'a DeclarationFacts<'g>) -> Option<&'a Binder<'g>> {
    let [binder] = header.binders.as_slice() else {
        return None;
    };
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return None;
    };
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    let TypeTerm::Reference {
        mutable: false,
        referent,
    } = parameter.ty.as_ref()
    else {
        return None;
    };
    (matches!(referent.as_ref(), TypeTerm::Parameter { declaration, position }
        if std::ptr::eq(*declaration, &binder.declaration.id) && *position == binder.position)
        && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
    .then_some(binder)
}
