use super::*;
use crate::analysis::scalars::{
    Place as TypedPlace,
    data::{Access, PathProjection},
};
use crate::parser::NodeKind;

/// Whether a compare.rill implementation is written `eq(left: &i32, right:
/// &i32): bool`. This reads a fixed number of syntax nodes and no facts, so
/// every other width and the generic adapter keep their pre-equality early
/// exit and cost. It only screens: admission still needs the typed provider.
pub(in crate::analysis::ownership) fn i32_equality_header(
    module: &Module,
    declaration: &Declaration,
) -> bool {
    let nodes = &module.syntax.nodes;
    let named = |node: NodeId, name: &[u8]| {
        nodes[node].kind == NodeKind::NamedType && nodes[node].span.bytes() == name
    };
    let shared_i32 = |parameter: &NodeId| match nodes[*parameter].children.as_slice() {
        [_, ty] => match (nodes[*ty].kind, nodes[*ty].children.as_slice()) {
            (NodeKind::ReferenceType { mutable: false }, [referent]) => named(*referent, b"i32"),
            _ => false,
        },
        _ => false,
    };
    let [signature, _] = nodes[declaration.node].children.as_slice() else {
        return false;
    };
    let [_, parameters, result] = nodes[*signature].children.as_slice() else {
        return false;
    };
    nodes[*parameters].kind == NodeKind::ValueParameters
        && nodes[*parameters].children.len() == 2
        && nodes[*parameters].children.iter().all(shared_i32)
        && named(*result, b"bool")
}

pub(super) fn implementation_frame<'g>(
    typed: &TypedReport<'g>,
    implementation: &ImplementationFacts<'g>,
    body: &BodyFacts<'g>,
    budget: &mut Budget,
) -> ResultOf<Frame> {
    let declaration = implementation.header.declaration;
    let pending = Stop::Pending(declaration.node);
    let family = typed
        .equality_role()
        .ok_or(Stop::Pending(declaration.node))?;
    let DeclarationShape::Function { parameters, result } = &implementation.header.shape else {
        return Err(pending);
    };
    let [left, right] = parameters.as_slice() else {
        return Err(pending);
    };
    if family.id != implementation.source.family
        || ![left, right].iter().all(|formal| {
            matches!(formal.ty.as_ref(), TypeTerm::Reference { mutable: false, referent }
                if scalar(referent) == Some(Primitive::I32))
        })
        || scalar(result) != Some(Primitive::Bool)
        || body.result.as_ref() != Some(result)
        || !body.flow.returns
        || body.flow.normal
        || body.flow.reaches_postfix
    {
        return Err(pending);
    }
    // M4 pins the compatible signature (including no defaults or requirements),
    // not the body. In particular, this is not a protected overload region.
    let providers = typed.primitive_providers();
    budget.step(providers.len())?;
    if !providers.iter().any(|provider| {
        provider.role == PrimitiveRole::Equality
            && provider.primitive == Primitive::I32
            && provider.binding.is_none()
            && std::ptr::eq(provider.family, &family.id)
            && std::ptr::eq(provider.implementation, declaration)
    }) {
        return Err(pending);
    }
    Ok(Frame::ScalarI32Equality {
        body: body.site.node,
    })
}

pub(in crate::analysis::ownership) fn lower_i32_equality<'g>(
    build: &mut Build<'_, 'g, '_>,
    formals: &[MemberType<'g>],
    params: &[Slot],
    finish: Pc,
) -> ResultOf<Pc> {
    // Block → return → comparison → dereference → named descriptor. Preflight
    // the fixed depth of this screened shape before inspecting any of it.
    const DEPTH: usize = 5;
    if build.budget.limits.max_depth < DEPTH {
        return Err(Stop::Limit);
    }
    let body = build.body;
    let pending = Stop::Pending(body);
    let nodes = &build.module.syntax.nodes;
    build.budget.step(7)?;
    let (NodeKind::Block, [ret]) = (nodes[body].kind, nodes[body].children.as_slice()) else {
        return Err(pending);
    };
    let (NodeKind::Return, [test]) = (nodes[*ret].kind, nodes[*ret].children.as_slice()) else {
        return Err(pending);
    };
    let (NodeKind::Binary("=="), &[left, right]) =
        (nodes[*test].kind, nodes[*test].children.as_slice())
    else {
        return Err(Stop::Pending(*test));
    };
    let (owner, result) = build.return_contract.ok_or(Stop::Pending(*ret))?;
    let facts = build.facts.nodes();
    build.budget.step(1)?;
    if !matches!(facts.get(ret).map(|fact| &fact.kind), Some(NodeFactKind::Return { target })
        if std::ptr::eq(*target, owner))
        || normal_value(facts, *test, build.budget)?.ty != *result
    {
        return Err(Stop::Pending(*ret));
    }
    build.budget.statistics.max_depth = build.budget.statistics.max_depth.max(DEPTH);
    let [left_formal, right_formal] = formals else {
        return Err(pending);
    };
    let &[left_param, right_param] = params else {
        return Err(pending);
    };
    let left_ty = referent(build, left, left_formal)?;
    let right_ty = referent(build, right, right_formal)?;
    let left_slot = build.slot(left, left_ty)?;
    let right_slot = build.slot(right, right_ty)?;
    let into = build.slot(*test, result)?;
    let export = build.emit(*ret, Action::Export(into), Edges::Next(finish))?;
    let compare = build.emit(
        *test,
        Action::EqualI32 {
            left: left_slot,
            right: right_slot,
            into,
        },
        Edges::Next(export),
    )?;
    let read_right = build.emit(
        right,
        Action::Dereference {
            from: right_param,
            into: Some(right_slot),
            explicit_copy: true,
        },
        Edges::Next(compare),
    )?;
    build.emit(
        left,
        Action::Dereference {
            from: left_param,
            into: Some(left_slot),
            explicit_copy: true,
        },
        Edges::Next(read_right),
    )
}

fn referent<'a, 'g>(
    build: &mut Build<'a, 'g, '_>,
    read: NodeId,
    formal: &MemberType<'g>,
) -> ResultOf<&'a Type<'g>> {
    let pending = Stop::Pending(read);
    let nodes = &build.module.syntax.nodes;
    let (NodeKind::Prefix("*"), [operand]) = (nodes[read].kind, nodes[read].children.as_slice())
    else {
        return Err(pending);
    };
    let facts = build.facts.nodes();
    let descriptor = normal_value(facts, *operand, build.budget)?;
    let value = normal_value(facts, read, build.budget)?;
    let formal_id = &formal.declaration.id;
    if nodes[*operand].kind != NodeKind::ValueReference
        || !descriptor
            .binding
            .is_some_and(|binding| std::ptr::eq(binding, formal_id))
        || !matches!(&descriptor.path, Some(path) if std::ptr::eq(path.root, formal_id) && path.projections.is_empty())
        || descriptor.access != Access::Owned
        || descriptor.ty != formal.ty
        || shared_input(&descriptor.ty, &value.ty) != Some(Primitive::I32)
        || value.access != Access::ReadOnly
        || value.place != TypedPlace::ReadOnly
        || !value.read_only_projection
        || !matches!(&value.path, Some(path) if std::ptr::eq(path.root, formal_id)
            && path.projections.as_slice() == [PathProjection::Dereference])
    {
        return Err(pending);
    }
    Ok(&value.ty)
}

fn normal_value<'a, 'g>(
    facts: &NodeIndex<'a, 'g>,
    node: NodeId,
    budget: &mut Budget,
) -> ResultOf<&'a ExpressionFacts<'g>> {
    budget.step(1)?;
    let fact = facts.get(&node).ok_or(Stop::Pending(node))?;
    if let NodeFactKind::Expression(value) = &fact.kind
        && fact.flow.normal
        && !fact.flow.pending
        && !fact.flow.diverges
        && !fact.flow.returns
        && !fact.flow.reaches_postfix
        && fact.flow.breaks.is_empty()
        && fact.flow.continues.is_empty()
        && value.call.is_none()
        && value.data.is_none()
        && value.admission.is_none()
    {
        Ok(value)
    } else {
        Err(Stop::Pending(node))
    }
}
