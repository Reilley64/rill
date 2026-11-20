//! Exact standard scalar copy contracts, backed by ordinary body checking.
use super::*;
use crate::analysis::{
    MemberType,
    ordinary::{Compatibility, ImplementationFacts},
    scalars::{
        BodyFacts, CallFacts, ExpressionFacts,
        data::DataExpression,
        generics::{GoalJudgment, GoalState},
    },
    typed::{Primitive, PrimitiveCoverage, PrimitiveRole},
};
use crate::loader::Origin;

/// Targeted callers share this index across distinct selected scalar widths.
/// Graph queries already have module and root indexes of their own.
#[derive(Default)]
pub(super) struct TargetIndex<'a, 'g> {
    nodes: NodeIndex<'a, 'g>,
    bodies: BTreeMap<NodeId, &'a BodyFacts<'g>>,
    implementations: BTreeMap<usize, &'a ImplementationFacts<'g>>,
}

impl<'a, 'g> TargetIndex<'a, 'g> {
    pub(super) fn new(
        typed: &'a TypedReport<'g>,
        module: &'g Module,
        site: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Self> {
        let mut index = Self::default();
        let role = typed.copy_role().ok_or(Stop::Pending(site))?;
        budget.step(1)?;
        let family = typed
            .inventory()
            .captured_family(&role.id)
            .ok_or(Stop::Pending(site))?;
        for implementation in &family.implementations {
            budget.step(1)?;
            let declaration = implementation.header.declaration;
            if declaration.id.module == module.key {
                budget.reserve(1)?;
                index
                    .implementations
                    .insert(declaration.id.start, implementation);
            }
        }
        for facts in typed.bodies().checked_bodies() {
            budget.step(1)?;
            if std::ptr::eq(facts.site.module, &module.key) {
                budget.reserve(1)?;
                index.bodies.insert(facts.site.node, facts);
            }
        }
        for facts in typed.bodies().typed_nodes() {
            budget.step(1)?;
            if std::ptr::eq(facts.site.module, &module.key) {
                budget.reserve(1)?;
                index.nodes.insert(facts.site.node, facts);
            }
        }
        Ok(index)
    }

    pub(super) fn lower_selected(
        &self,
        typed: &TypedReport<'g>,
        module: &'g Module,
        id: &DeclarationId,
        site: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Mir<'g>> {
        budget.step(2)?;
        let implementation = self
            .implementations
            .get(&id.start)
            .ok_or(Stop::Pending(site))?;
        let declaration = implementation.header.declaration;
        if !std::ptr::eq(id, &declaration.id) {
            return Err(Stop::Pending(site));
        }
        let facts = self
            .bodies
            .get(&implementation.source.body_node)
            .ok_or(Stop::Pending(declaration.node))?;
        let frame =
            implementation_frame(typed, module, declaration, implementation, facts, budget)?;
        Ok(lower(
            typed,
            module,
            &implementation.header,
            frame,
            false,
            Some(&self.nodes),
            budget,
        )?
        .mir)
    }
}

fn scalar(ty: &TypeTerm<'_>) -> Option<Primitive> {
    let TypeTerm::Primitive(intrinsic) = ty else {
        return None;
    };
    Primitive::named(intrinsic.name()).filter(|p| *p != Primitive::String)
}

pub(super) fn shared_input<'g>(input: &Type<'g>, result: &Type<'g>) -> Option<Primitive> {
    let primitive = scalar(result)?;
    matches!(input.as_ref(), TypeTerm::Reference { mutable: false, referent }
        if referent == result)
    .then_some(primitive)
}

fn required_provider<'g>(
    typed: &TypedReport<'g>,
    family: &'g DeclarationId,
    primitive: Primitive,
    budget: &mut Budget,
) -> ResultOf<Option<&'g Declaration>> {
    let providers = typed.primitive_providers();
    let protection = typed.primitive_protection();
    budget.step(providers.len() + protection.len())?;
    let Some(provider) = providers.iter().find(|p| {
        p.role == PrimitiveRole::Copy
            && std::ptr::eq(p.family, family)
            && p.primitive == primitive
            && p.binding.is_none()
    }) else {
        return Ok(None);
    };
    Ok(protection
        .iter()
        .any(|scope| {
            scope.role == PrimitiveRole::Copy
                && std::ptr::eq(scope.family, family)
                && scope.primitive == primitive
                && scope.coverage == PrimitiveCoverage::Clear
                && scope.overlaps.is_empty()
                && scope
                    .required_winner
                    .is_some_and(|winner| std::ptr::eq(winner, &provider.implementation.id))
        })
        .then_some(provider.implementation))
}

pub(super) fn implementation_frame<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    declaration: &'g Declaration,
    implementation: &ImplementationFacts<'g>,
    body: &BodyFacts<'g>,
    budget: &mut Budget,
) -> ResultOf<Frame> {
    let pending = Stop::Pending(declaration.node);
    budget.step(1)?;
    let Some(family) = typed.copy_role() else {
        return Err(pending);
    };
    let source = implementation.source;
    // ImplementationFacts borrows the attachment from graph.implementations().
    // Require both the canonical header and the body of that attachment.
    if module.key.origin != Origin::Standard
        || module.key.path != "core.rill"
        || source.family != family.id
        || source.declaration != declaration.id
        || declaration.scope.is_some()
        || !std::ptr::eq(implementation.header.declaration, declaration)
        || implementation.compatibility != Compatibility::Compatible
        || !implementation.header.binders.is_empty()
        || !std::ptr::eq(body.site.module, &module.key)
        || body.site.node != source.body_node
        || body.flow.pending
        || !body.flow.returns
        || body.flow.normal
        || body.flow.diverges
        || !body.flow.breaks.is_empty()
        || !body.flow.continues.is_empty()
        || !matches!(body.owner, BodyOwner::Declaration(owner) if std::ptr::eq(owner, &declaration.id))
        || module.syntax.nodes[declaration.node].children.as_slice()
            != [source.signature_node, source.body_node]
    {
        return Err(pending);
    }
    let DeclarationShape::Function { parameters, result } = &implementation.header.shape else {
        return Err(pending);
    };
    let [MemberType { ty: input, .. }] = parameters.as_slice() else {
        return Err(pending);
    };
    let Some(primitive) = shared_input(input, result) else {
        return Err(pending);
    };
    if body.result.as_ref() != Some(result)
        || !required_provider(typed, &family.id, primitive, budget)?
            .is_some_and(|provider| std::ptr::eq(provider, declaration))
    {
        return Err(pending);
    }
    budget.step(1)?;
    if !typed
        .inventory()
        .headers()
        .get(&declaration.id)
        .is_some_and(|metadata| metadata.defaults.is_empty() && metadata.requirements.is_empty())
    {
        return Err(pending);
    }
    Ok(Frame::ScalarCopy {
        body: source.body_node,
    })
}

pub(super) fn call_target<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    node: NodeId,
    call: &CallFacts<'g>,
    facts: &ExpressionFacts<'g>,
    input: &Type<'g>,
    budget: &mut Budget,
) -> ResultOf<&'g DeclarationId> {
    let pending = || Stop::Pending(node);
    budget.step(4)?;
    let primitive = shared_input(input, &facts.ty).ok_or_else(pending)?;
    // M4 stores the selection for these calls in the canonical goal,
    // not in CallFacts::selection.
    let Some(DataExpression::CanonicalCall(goal)) = &facts.data else {
        return Err(pending());
    };
    let goal = typed.bodies().goal(goal).map_err(|_| pending())?;
    let (Some(selection), Some(output)) = (&goal.selection, &goal.output) else {
        return Err(pending());
    };
    let GoalJudgment::Determine {
        family,
        inputs,
        filters,
    } = &goal.judgment
    else {
        return Err(pending());
    };
    let ([captured_input], [filter], [substitution], [output_argument]) = (
        inputs.as_slice(),
        filters.as_slice(),
        call.substitution.as_slice(),
        output.arguments.as_slice(),
    ) else {
        return Err(pending());
    };
    let copy = typed.copy_role().ok_or_else(pending)?;
    if call.callee != &copy.id
        || *family != &copy.id
        || goal.site.node != node
        || !std::ptr::eq(goal.site.module, &module.key)
        || goal.state != GoalState::Proved
        || goal.premise.is_some()
        || !goal.defaults.is_empty()
        || !goal.dependencies.is_empty()
        || selection.alternative != 0
        || !selection.substitution.is_empty()
        || *captured_input != *input
        || output.result != facts.ty
        || *output_argument != facts.ty
        || filter.position != 0
        || filter.argument != facts.ty
        || substitution.position != 0
        || substitution.argument != facts.ty
    {
        return Err(pending());
    }
    let family = typed
        .inventory()
        .captured_family(&copy.id)
        .ok_or_else(pending)?;
    let [binder] = family.header.binders.as_slice() else {
        return Err(pending());
    };
    let binder = &binder.declaration.id;
    if filter.binder != binder || substitution.binder != binder {
        return Err(pending());
    }
    let provider = required_provider(typed, &copy.id, primitive, budget)?.ok_or_else(pending)?;
    if !std::ptr::eq(selection.implementation, &provider.id) {
        return Err(pending());
    }
    Ok(selection.implementation)
}
