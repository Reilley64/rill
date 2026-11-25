//! Exact standard scalar copy and disposal contracts, backed by ordinary body checking.
use super::*;
use crate::analysis::{
    MemberType,
    ordinary::{Compatibility, ImplementationFacts},
    scalars::{
        BodyFacts, CallFacts, ExpressionFacts,
        data::DataExpression,
        generics::{GoalJudgment, GoalState},
    },
    typed::{Primitive, PrimitiveCoverage, PrimitiveOperation, PrimitiveRole},
};
use crate::loader::Origin;

mod compare;
pub(super) use compare::{i32_equality_header, lower_i32_equality};

pub(super) fn standard_module(module: &Module) -> bool {
    module.key.origin == Origin::Standard
        && matches!(
            module.key.path.as_str(),
            "core.rill" | "compare.rill" | "output.rill"
        )
}

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
        let copy = typed.copy_role().ok_or(Stop::Pending(site))?;
        index.index_family(typed, module, &copy.id, site, budget)?;
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

    fn index_family(
        &mut self,
        typed: &'a TypedReport<'g>,
        module: &'g Module,
        family: &'g DeclarationId,
        site: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<()> {
        budget.step(1)?;
        let family = typed
            .inventory()
            .captured_family(family)
            .ok_or(Stop::Pending(site))?;
        for implementation in &family.implementations {
            budget.step(1)?;
            let declaration = implementation.header.declaration;
            if declaration.id.module == module.key {
                budget.reserve(1)?;
                self.implementations
                    .insert(declaration.id.start, implementation);
            }
        }
        Ok(())
    }

    pub(super) fn lower_selected(
        &mut self,
        typed: &'a TypedReport<'g>,
        module: &'g Module,
        id: &DeclarationId,
        site: NodeId,
        budget: &mut Budget,
    ) -> ResultOf<Mir<'g>> {
        budget.step(2)?;
        let implementation = if let Some(&implementation) = self.implementations.get(&id.start) {
            implementation
        } else {
            // Index disposal headers only if reached. The core body and node
            // indexes are already shared by both families.
            let dispose = typed.dispose_role().ok_or(Stop::Pending(site))?;
            self.index_family(typed, module, &dispose.id, site, budget)?;
            *self
                .implementations
                .get(&id.start)
                .ok_or(Stop::Pending(site))?
        };
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
            Lowering {
                budget,
                cache: None,
            },
        )?
        .mir)
    }
}

pub(super) fn scalar(ty: &TypeTerm<'_>) -> Option<Primitive> {
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
    role: PrimitiveRole,
    family: &'g DeclarationId,
    primitive: Primitive,
    budget: &mut Budget,
) -> ResultOf<Option<&'g Declaration>> {
    let providers = typed.primitive_providers();
    let protection = typed.primitive_protection();
    budget.step(providers.len() + protection.len())?;
    let Some(provider) = providers.iter().find(|p| {
        p.role == role
            && std::ptr::eq(p.family, family)
            && p.primitive == primitive
            && p.binding.is_none()
    }) else {
        return Ok(None);
    };
    Ok(protection
        .iter()
        .any(|scope| {
            scope.role == role
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
    // Other compare.rill bodies stop here uncharged, as before equality.
    if module.key.path == "compare.rill" && !i32_equality_header(module, declaration) {
        return Err(pending);
    }
    budget.step(1)?;
    let source = implementation.source;
    // ImplementationFacts borrows the attachment from graph.implementations().
    // Require both the canonical header and the body of that attachment.
    if !standard_module(module)
        || source.declaration != declaration.id
        || declaration.scope.is_some()
        || !std::ptr::eq(implementation.header.declaration, declaration)
        || implementation.compatibility != Compatibility::Compatible
        || !implementation.header.binders.is_empty()
        || !std::ptr::eq(body.site.module, &module.key)
        || body.site.node != source.body_node
        || body.flow.pending
        || (body.flow.diverges && module.key.path != "output.rill")
        || !body.flow.breaks.is_empty()
        || !body.flow.continues.is_empty()
        || !matches!(body.owner, BodyOwner::Declaration(owner) if std::ptr::eq(owner, &declaration.id))
        || module.syntax.nodes[declaration.node].children.as_slice()
            != [source.signature_node, source.body_node]
    {
        return Err(pending);
    }
    if module.key.path == "compare.rill" {
        return compare::implementation_frame(typed, implementation, body, budget);
    }
    if module.key.path == "output.rill" {
        return output_frame(typed, implementation, body, budget);
    }
    let (role, family) = if let Some(copy) = typed
        .copy_role()
        .filter(|family| family.id == source.family)
    {
        (PrimitiveRole::Copy, &copy.id)
    } else if let Some(dispose) = typed
        .dispose_role()
        .filter(|family| family.id == source.family)
    {
        (PrimitiveRole::Dispose, &dispose.id)
    } else {
        return Err(pending);
    };
    let DeclarationShape::Function { parameters, result } = &implementation.header.shape else {
        return Err(pending);
    };
    let [MemberType { ty: input, .. }] = parameters.as_slice() else {
        return Err(pending);
    };
    let node = source.body_node;
    let (primitive, frame) = match role {
        PrimitiveRole::Copy if body.flow.returns && !body.flow.normal => (
            shared_input(input, result),
            Frame::ScalarCopy { body: node },
        ),
        PrimitiveRole::Dispose if body.flow.normal && !body.flow.returns => {
            budget.step(1)?;
            let syntax = &module.syntax.nodes[node];
            if syntax.kind != crate::parser::NodeKind::Block || !syntax.children.is_empty() {
                return Err(pending);
            }
            // Ordinary parameter setup and lexical cleanup end the received
            // scalar's storage. This is not an active custom-disposer frame.
            (
                dispose_input(input, result),
                Frame::PlainImplementation { body: node },
            )
        }
        _ => return Err(pending),
    };
    let Some(primitive) = primitive else {
        return Err(pending);
    };
    if body.result.as_ref() != Some(result)
        || !required_provider(typed, role, family, primitive, budget)?
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
    Ok(frame)
}

/// A `log` or `panic` implementation over its one private output operation:
/// `string` or `&string` in, `void` (flowing on) or `never` (diverging) out.
fn output_frame<'g>(
    typed: &TypedReport<'g>,
    implementation: &ImplementationFacts<'g>,
    body: &BodyFacts<'g>,
    budget: &mut Budget,
) -> ResultOf<Frame> {
    let declaration = implementation.header.declaration;
    let pending = Stop::Pending(declaration.node);
    let DeclarationShape::Function { parameters, result } = &implementation.header.shape else {
        return Err(pending);
    };
    let [MemberType { ty: input, .. }] = parameters.as_slice() else {
        return Err(pending);
    };
    let text =
        |ty: &Type<'_>| matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "string");
    let screened = text(input)
        || matches!(input.as_ref(), TypeTerm::Reference { mutable: false, referent } if text(referent));
    let diverging = body.flow.diverges && !body.flow.normal;
    if !screened || body.flow.returns || body.flow.diverges == body.flow.normal {
        return Err(pending);
    }
    budget.step(1)?;
    let expected = match typed.output_family(&implementation.source.family) {
        Some(PrimitiveOperation::WriteLine) => "void",
        Some(PrimitiveOperation::Panic) => "never",
        _ => return Err(pending),
    };
    if !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == expected)
        || diverging != (expected == "never")
        || body.result.as_ref() != Some(result)
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
    Ok(Frame::PlainImplementation {
        body: implementation.source.body_node,
    })
}

fn dispose_input(input: &Type<'_>, result: &Type<'_>) -> Option<Primitive> {
    if matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void") {
        scalar(input)
    } else {
        None
    }
}

pub(super) fn call_target<'g>(
    typed: &TypedReport<'g>,
    module: &'g Module,
    node: NodeId,
    call: &CallFacts<'g>,
    facts: &ExpressionFacts<'g>,
    (input, text): (&Type<'g>, Option<TextElementDisposal<'g>>),
    budget: &mut Budget,
) -> ResultOf<&'g DeclarationId> {
    let pending = || Stop::Pending(node);
    budget.step(4)?;
    let (role, family_id, primitive, family_argument) = if let Some(copy) = typed
        .copy_role()
        .filter(|f| std::ptr::eq(&f.id, call.callee))
    {
        (
            PrimitiveRole::Copy,
            &copy.id,
            shared_input(input, &facts.ty),
            &facts.ty,
        )
    } else if let Some(dispose) = typed
        .dispose_role()
        .filter(|f| std::ptr::eq(&f.id, call.callee))
    {
        (
            PrimitiveRole::Dispose,
            &dispose.id,
            dispose_input(input, &facts.ty).or_else(|| {
                (text.is_some()
                    && matches!(input.as_ref(),TypeTerm::Primitive(p) if p.name()=="string")
                    && matches!(facts.ty.as_ref(),TypeTerm::Primitive(p) if p.name()=="void"))
                .then_some(Primitive::String)
            }),
            input,
        )
    } else {
        return Err(pending());
    };
    let primitive = primitive.ok_or_else(pending)?;
    // The canonical typed goal carries the selection even when CallFacts does not.
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
    if !std::ptr::eq(*family, family_id)
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
        || output_argument != family_argument
        || filter.position != 0
        || &filter.argument != family_argument
        || substitution.position != 0
        || &substitution.argument != family_argument
        || call.selection.as_ref().is_some_and(|direct| {
            !std::ptr::eq(direct.implementation, selection.implementation)
                || direct.alternative != 0
                || !direct.substitution.is_empty()
        })
    {
        return Err(pending());
    }
    let family = typed
        .inventory()
        .captured_family(family_id)
        .ok_or_else(pending)?;
    let [binder] = family.header.binders.as_slice() else {
        return Err(pending());
    };
    let binder = &binder.declaration.id;
    if filter.binder != binder || substitution.binder != binder {
        return Err(pending());
    }
    if let Some(text) = text {
        return (role == PrimitiveRole::Dispose
            && primitive == Primitive::String
            && std::ptr::eq(selection.implementation, text.implementation))
        .then_some(text.implementation)
        .ok_or_else(pending);
    }
    let provider =
        required_provider(typed, role, family_id, primitive, budget)?.ok_or_else(pending)?;
    if !std::ptr::eq(selection.implementation, &provider.id) {
        return Err(pending());
    }
    Ok(selection.implementation)
}
