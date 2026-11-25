//! Written and scope-exit release edges over flow-checked MIR, not proofs of
//! the provider source roots.
use super::*;
use crate::analysis::{
    scalars::{
        CallFacts, ExpressionFacts,
        data::{Access, DataExpression, PathProjection},
    },
    typed::{
        NativeOwnershipAction, NativeRepresentationKind, OwnershipCoverage, OwnershipRule,
        Primitive, PrimitiveCoverage, PrimitiveOperation, PrimitiveRole,
    },
};
use crate::parser::NodeKind;

#[allow(clippy::too_many_arguments)]
pub(super) fn proved<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    nodes: &NodeIndex<'_, 'g>,
    header: Option<&DeclarationFacts<'g>>,
    block: &Block<'g>,
    edge: Dependency<'g>,
    active: bool,
    family_count: usize,
    budget: &mut Budget,
) -> ResultOf<bool> {
    use DependencyContract::{BuiltinDispose, ProtectedRelease};
    let operand = match (edge.contract, &block.operation) {
        (BuiltinDispose, Operation::Dispose { callee, operand })
            if std::ptr::eq(*callee, edge.declaration) =>
        {
            *operand
        }
        (ProtectedRelease { entry }, Operation::Cleanup(actions)) => {
            // A written guard release is a single action at its call. Any
            // other cleanup is scope-exit release, checked from the types of
            // the released parameters, not from a call.
            let (
                [
                    Cleanup::ReleaseGuard {
                        operand,
                        implementation,
                        entry: release_entry,
                    },
                ],
                NodeKind::Call,
            ) = (
                actions.as_slice(),
                module.syntax.nodes[block.site.node].kind,
            )
            else {
                return scope_exit(typed, module, header, actions, edge, entry, budget);
            };
            if !std::ptr::eq(*implementation, edge.declaration)
                || !std::ptr::eq(*release_entry, entry)
            {
                return Ok(false);
            }
            *operand
        }
        _ => return Ok(false),
    };
    let Some(family) = typed.dispose_role().map(|role| &role.id) else {
        return Ok(false);
    };
    let Some((expr, call, input)) = written_operand(
        module,
        nodes,
        block.site.node,
        operand,
        family,
        active && edge.contract == BuiltinDispose,
        budget,
    )?
    else {
        return Ok(false);
    };
    if let ProtectedRelease { entry } = edge.contract {
        return protected_guard(typed, input, expr, family, edge.declaration, entry, budget);
    }
    if !matches!(input.as_ref(), TypeTerm::Primitive(p) if p.name() == "string") {
        return Ok(false);
    }
    let Some(provider) = text_provider(typed, family, budget)? else {
        return Ok(false);
    };
    // call_target uses the captured family map and the indexed M4 goal. The
    // operands and substitutions compared there have the closed text shape.
    charge_key(&family.module, family_count, budget)?;
    match scalar::call_target(
        typed,
        module,
        block.site.node,
        call,
        expr,
        (input, Some(provider)),
        budget,
    ) {
        Ok(_) => Ok(true),
        Err(Stop::Limit) => Err(Stop::Limit),
        Err(_) => Ok(false),
    }
}

fn written_operand<'a, 'g>(
    module: &'g Module,
    nodes: &NodeIndex<'a, 'g>,
    node: NodeId,
    operand: Place<'g>,
    family: &'g DeclarationId,
    field_disposal: bool,
    budget: &mut Budget,
) -> ResultOf<Option<(&'a ExpressionFacts<'g>, &'a CallFacts<'g>, &'a Type<'g>)>> {
    // A reachable Dispose/ReleaseGuard is emitted only after flow verifies
    // initialization, ownership and conflicting loans on this exact storage.
    // A written `dispose` of a field in an active disposer body (ri-4lq) is
    // screened from its place and the field's own declaration before the
    // first charge.
    let fields = match (operand.projection, field_disposal) {
        (None, _) => [None, None],
        (Some(Projection::Field(field)), true) => [Some(field), None],
        (Some(Projection::NestedField { outer, inner }), true) => [Some(outer), Some(inner)],
        _ => return Ok(None),
    };
    if module.syntax.nodes[node].kind != NodeKind::Call
        || !std::ptr::eq(operand.root.module, &module.key)
        || fields[1]
            .or(fields[0])
            .is_some_and(|leaf| !declared_text(module, leaf))
    {
        return Ok(None);
    }
    budget.step(2)?;
    let Some(NodeFactKind::Expression(expr)) = nodes.get(&node).map(|fact| &fact.kind) else {
        return Ok(None);
    };
    let Some(call) = expr.call.as_deref() else {
        return Ok(None);
    };
    let [argument] = call.arguments.as_slice() else {
        return Ok(None);
    };
    if !std::ptr::eq(call.callee, family)
        || call.pipe_input.is_some()
        || !call.defaults.is_empty()
        || expr.ordinary_context.is_some()
        || expr.admission.is_some()
        || !matches!(expr.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
    {
        return Ok(None);
    }
    let Some(NodeFactKind::Expression(actual)) = nodes.get(argument).map(|fact| &fact.kind) else {
        return Ok(None);
    };
    let Some(path) = actual.path.as_ref() else {
        return Ok(None);
    };
    budget.step(path.projections.len())?;
    let same = |written: &DeclarationId, field: &Declaration| {
        written.start == field.id.start && written.kind == field.id.kind
    };
    let projected = match (path.projections.as_slice(), fields) {
        ([], [None, None]) => true,
        ([PathProjection::Field(written)], [Some(field), None]) => same(written, field),
        (
            [PathProjection::Field(outer), PathProjection::Field(inner)],
            [Some(outer_field), Some(inner_field)],
        ) => same(outer, outer_field) && same(inner, inner_field),
        _ => false,
    };
    if actual.access != Access::Owned
        || !projected
        // A name is its binding; a field read has none, and its root is
        // matched to the operand's slot below.
        || !match fields[0] {
            None => actual.binding.is_some_and(|id| std::ptr::eq(id, path.root)),
            Some(_) => actual.binding.is_none(),
        }
        || actual.refinement.is_some()
        || actual.admission.is_some()
    {
        return Ok(None);
    }
    let syntax = &module.syntax.nodes;
    let slot_node = operand.root.node;
    budget.step(4)?;
    let declaration_node = match (path.root.kind, syntax[slot_node].kind) {
        (DeclarationKind::Parameter, NodeKind::Parameter) => slot_node,
        (DeclarationKind::Binding, NodeKind::NamePattern) => {
            let [qualified] = syntax[syntax[slot_node].children[0]].children.as_slice() else {
                return Ok(None);
            };
            let [name] = syntax[*qualified].children.as_slice() else {
                return Ok(None);
            };
            *name
        }
        _ => return Ok(None),
    };
    // Within one module the loader orders declarations by (start, kind).
    let declarations = &module.declarations;
    budget.step((usize::BITS - declarations.len().leading_zeros() + 1) as usize)?;
    let matched = declarations
        .binary_search_by_key(&(path.root.start, path.root.kind), |d| {
            (d.id.start, d.id.kind)
        })
        .ok()
        .is_some_and(|index| {
            let declaration = &declarations[index];
            std::ptr::eq(&declaration.id, path.root) && declaration.node == declaration_node
        });
    Ok(matched.then_some((expr, call, &actual.ty)))
}

/// `field` is declared in `module` (by address, not by key) with the written
/// type `string`. Syntax only screens; the typed operand is checked after.
fn declared_text(module: &Module, field: &Declaration) -> bool {
    if !module
        .declarations
        .as_ptr_range()
        .contains(&std::ptr::from_ref(field))
    {
        return false;
    }
    let syntax = &module.syntax.nodes;
    let declaration = &syntax[field.node];
    matches!(declaration.kind, NodeKind::RecordField { .. })
        && declaration.children.get(1).is_some_and(|&ty| {
            syntax[ty].kind == NodeKind::NamedType && syntax[ty].span.bytes() == b"string"
        })
}

fn protected_guard<'g>(
    typed: &TypedReport<'g>,
    input: &Type<'g>,
    expr: &ExpressionFacts<'g>,
    family: &'g DeclarationId,
    implementation: &'g DeclarationId,
    entry: &'g DeclarationId,
    budget: &mut Budget,
) -> ResultOf<bool> {
    let (
        TypeTerm::Nominal {
            declaration,
            arguments,
        },
        Some(DataExpression::NativeOwnership(native)),
    ) = (input.as_ref(), &expr.data)
    else {
        return Ok(false);
    };
    // The selected implementation ID is carried by M4's call evidence;
    // it is not necessarily the same allocation as the provider header ID.
    charge_key(&implementation.module, 1, budget)?;
    if !matches!(arguments.as_slice(), [argument] if is_i32(argument))
        || !matches!(native.substitution.as_slice(), [binding] if binding.position == 0 && is_i32(&binding.argument))
        || native.action != NativeOwnershipAction::GuardRelease
        || native.implementation != implementation
        || !std::ptr::eq(native.entry, entry)
    {
        return Ok(false);
    }
    protected_scope(
        typed,
        declaration,
        family,
        implementation,
        entry,
        Some(native.artifact),
        budget,
    )
}

/// Scope-exit release of a `LockGuard<i32>` held by an ordinary function's
/// parameter, whole or as one field of a flat application record. Flow has
/// already placed each release on initialized, unborrowed storage; this
/// checks only that every release on this edge is the protected provider of
/// its operand's declared type.
fn scope_exit<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    header: Option<&DeclarationFacts<'g>>,
    actions: &[Cleanup<'g>],
    edge: Dependency<'g>,
    entry: &'g DeclarationId,
    budget: &mut Budget,
) -> ResultOf<bool> {
    let releases = || {
        actions.iter().filter_map(move |action| match *action {
            Cleanup::ReleaseGuard {
                operand,
                implementation,
                entry: selected,
            } if std::ptr::eq(implementation, edge.declaration)
                && std::ptr::eq(selected, entry) =>
            {
                Some(operand)
            }
            _ => None,
        })
    };
    // Screen from the paid-for MIR and syntax before the first charge.
    let (
        Some(DeclarationFacts {
            shape: DeclarationShape::Function { parameters, .. },
            ..
        }),
        Some(family),
    ) = (header, typed.dispose_role().map(|role| &role.id))
    else {
        return Ok(false);
    };
    let mut any = false;
    for operand in releases() {
        if !std::ptr::eq(operand.root.module, &module.key)
            || module.syntax.nodes[operand.root.node].kind != NodeKind::Parameter
            || !matches!(operand.projection, None | Some(Projection::Field(_)))
        {
            return Ok(false);
        }
        any = true;
    }
    if !any {
        return Ok(false);
    }
    budget.step(actions.len())?;
    for operand in releases() {
        budget.step(parameters.len())?;
        let Some(parameter) = parameters
            .iter()
            .find(|parameter| parameter.declaration.node == operand.root.node)
        else {
            return Ok(false);
        };
        let ty = match operand.projection {
            Some(Projection::Field(field)) => {
                let Some(ty) = field_type(typed, &parameter.ty, field, budget)? else {
                    return Ok(false);
                };
                ty
            }
            _ => &parameter.ty,
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = ty.as_ref()
        else {
            return Ok(false);
        };
        if !matches!(arguments.as_slice(), [argument] if is_i32(argument))
            || !protected_scope(
                typed,
                declaration,
                family,
                edge.declaration,
                entry,
                None,
                budget,
            )?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The declared type of `field` in the flat, non-generic application record `ty`.
fn field_type<'a, 'g>(
    typed: &'a TypedReport<'g>,
    ty: &Type<'g>,
    field: &'g Declaration,
    budget: &mut Budget,
) -> ResultOf<Option<&'a Type<'g>>> {
    let TypeTerm::Nominal {
        declaration,
        arguments,
    } = ty.as_ref()
    else {
        return Ok(None);
    };
    if declaration.module.origin != Origin::Application || !arguments.is_empty() {
        return Ok(None);
    }
    let headers = &typed.inventory().type_facts().declarations;
    let Some(DeclarationFacts {
        binders,
        shape: DeclarationShape::Record { fields },
        ..
    }) = headers.get(declaration)
    else {
        return Ok(None);
    };
    if !binders.is_empty() {
        return Ok(None);
    }
    budget.step(fields.len())?;
    Ok(fields
        .iter()
        .find(|member| std::ptr::eq(member.declaration, field))
        .map(|member| &member.ty))
}

/// The clear required guard-release scope of `owner` names this provider,
/// and the guard's native representation is its payload and source. A
/// written call also fixes the selected artifact.
fn protected_scope<'g>(
    typed: &TypedReport<'g>,
    owner: &DeclarationId,
    family: &'g DeclarationId,
    implementation: &'g DeclarationId,
    entry: &'g DeclarationId,
    artifact: Option<&'g std::path::Path>,
    budget: &mut Budget,
) -> ResultOf<bool> {
    let scopes = typed.ownership_protection();
    budget.step(scopes.len())?;
    let Some((scope, provider)) = scopes.iter().find_map(|scope| {
        let provider = scope.provider.as_ref()?;
        (std::ptr::eq(&scope.owner.id, owner)
            && scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
            && scope.coverage == OwnershipCoverage::Clear
            && std::ptr::eq(&provider.implementation.id, implementation)
            && std::ptr::eq(&provider.entry.id, entry)
            && artifact.is_none_or(|artifact| std::ptr::eq(provider.artifact, artifact)))
        .then_some((scope, provider))
    }) else {
        return Ok(false);
    };
    charge_key(&family.module, 1, budget)?;
    if scope.family != family {
        return Ok(false);
    }
    let representations = typed.native_representations();
    budget.step(representations.len())?;
    Ok(representations.iter().any(|representation| {
        std::ptr::eq(representation.owner, scope.owner)
            && representation.kind == NativeRepresentationKind::GuardPayloadAndSource
            && std::ptr::eq(representation.artifact, provider.artifact)
    }))
}

fn is_i32(ty: &Type<'_>) -> bool {
    matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
}

fn text_provider<'g>(
    typed: &TypedReport<'g>,
    family: &'g DeclarationId,
    budget: &mut Budget,
) -> ResultOf<Option<TextElementDisposal<'g>>> {
    let scopes = typed.primitive_protection();
    budget.step(scopes.len())?;
    let Some(winner) = scopes.iter().find_map(|scope| {
        (std::ptr::eq(scope.family, family)
            && scope.role == PrimitiveRole::Dispose
            && scope.primitive == Primitive::String
            && scope.coverage == PrimitiveCoverage::Clear)
            .then_some(scope.required_winner)
            .flatten()
    }) else {
        return Ok(None);
    };
    let providers = typed.primitive_providers();
    budget.step(providers.len())?;
    Ok(providers.iter().find_map(|provider| {
        let binding = provider.binding?;
        (std::ptr::eq(provider.family, family)
            && provider.role == PrimitiveRole::Dispose
            && provider.primitive == Primitive::String
            && std::ptr::eq(&provider.implementation.id, winner)
            && binding.operation == PrimitiveOperation::DisposeText)
            .then_some(TextElementDisposal {
                implementation: winner,
                bridge: &binding.declaration.id,
            })
    }))
}
