//! Construct ownership operations from the M4 type and control facts.
use super::*;
use crate::analysis::MemberType;
use crate::analysis::ordinary::{self, HypotheticalOutcome};
use crate::analysis::scalars::data::{
    Access, Construction, Constructor, DataExpression, PathProjection, Pattern, PatternBinder,
    PatternFacts, PatternProjection, PlacePath,
};
use crate::analysis::scalars::generics::{
    GoalState,
    callables::{ArrowFacts, CaptureFact, CaptureForm, InvocationFacts},
};
use crate::analysis::scalars::{
    CallFacts, ErrorConstruction, ExpressionFacts, LiteralValue, Place as TypedPlace, Propagation,
    PropagationTarget,
};
use crate::analysis::typed::{
    ErrorOperation, NativeOwnershipAction, NativeOwnershipProvider, NativeRepresentationKind,
    OptionRole, OwnershipCoverage, OwnershipRule, PrimitiveCoverage, PrimitiveRole, ResultRole,
};
use crate::loader::Origin;
use crate::parser::NodeKind;
use std::sync::Arc;

enum PropagatingCallOutput<'a, 'g> {
    Value(&'a Type<'g>),
    DiscardedTextPair(&'a Type<'g>),
    CheckedPipeCarrier { primitive_arguments_checked: bool },
}

#[derive(Clone, Copy)]
enum ExpressionProof<'t, 'g> {
    CallCarrier(&'t Type<'g>),
    NestedTupleParent(&'t PlacePath<'g>),
    NestedTupleSource(Slot),
    NamedNestedTupleLeaf,
    NestedRecordConstruction,
    UnloweredRebuildRead(Slot),
    NamedRecordRebuild,
    NestedRecordFieldTemporary(ProjectedTemporary),
}

struct PropagatingValueCall<'a, 'g> {
    node: NodeId,
    call: &'a CallFacts<'g>,
    result: &'a Type<'g>,
    discard_needs_cleanup: bool,
    arguments: PropagatingArguments,
}

struct PropagatingArguments {
    continuing: Vec<PropagatingArgument>,
    terminal_failure: Option<PropagatingArgument>,
    can_return: bool,
    pipe_formal: Option<usize>,
    returning: Pc,
}

fn argument_staging_order(
    arguments: &[NodeId],
    pipe_formal: Option<usize>,
) -> impl DoubleEndedIterator<Item = (usize, NodeId)> + ExactSizeIterator + Clone {
    (0..arguments.len()).map(move |position| {
        let formal = match pipe_formal {
            Some(pipe) if position == 0 => pipe,
            Some(pipe) if position <= pipe => position - 1,
            _ => position,
        };
        (formal, arguments[formal])
    })
}

#[derive(Clone, Copy)]
struct PropagatingArgument {
    evaluation_position: usize,
    root: NodeId,
    can_return: bool,
}

enum OptionPayload<'t, 'g> {
    Bool,
    Text(&'t Type<'g>),
}

#[derive(Clone, Copy)]
enum OptionTextSource {
    Move(Slot),
    Constructor,
    Factory,
    PipeStep,
}

#[derive(Clone, Copy)]
struct CheckedPropagation<'a, 'g> {
    facts: &'a ExpressionFacts<'g>,
    exit: &'a Propagation<'g>,
    wrap: Option<&'a ErrorConstruction<'g>>,
    fresh: Option<&'a ErrorConstruction<'g>>,
    piped: bool,
    source: NodeId,
    operand: &'a Type<'g>,
    wrapper: Option<&'g DeclarationId>,
    option_text: Option<(&'a Type<'g>, OptionTextSource)>,
}

fn owns_text(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Primitive(p) if p.name() == "string")
}

fn primitive_reference(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Reference { referent, .. } if matches!(referent.as_ref(), TypeTerm::Primitive(_)))
}

fn shared_primitive_reference(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Reference { mutable: false, .. }) && primitive_reference(ty)
}

fn shared_pattern_member_type(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Primitive(_)) || shared_primitive_reference(ty)
}

fn borrowed_binding_mode(
    access: Access,
    binder: &PatternBinder<'_>,
) -> Option<BorrowedMemberBinding> {
    if !shared_pattern_member_type(binder.source_type.as_ref()) {
        return None;
    }
    if binder.ty == binder.source_type {
        Some(if shared_primitive_reference(binder.ty.as_ref()) {
            BorrowedMemberBinding::CopyReference
        } else {
            BorrowedMemberBinding::CopyScalar
        })
    } else if matches!(binder.ty.as_ref(), TypeTerm::Reference { mutable, referent }
        if *mutable == (access == Access::Writable) && *referent == binder.source_type)
    {
        Some(if access == Access::Writable {
            BorrowedMemberBinding::ExclusiveView
        } else {
            BorrowedMemberBinding::SharedView
        })
    } else {
        None
    }
}

fn projection<'g>(facts: &ExpressionFacts<'g>) -> Option<(NodeId, Projection<'g>)> {
    match &facts.data {
        Some(DataExpression::Projection(field)) => {
            Some((field.base, Projection::Field(field.field)))
        }
        Some(DataExpression::TupleIndex {
            base, component, ..
        }) => Some((*base, Projection::TupleElement(*component))),
        _ => None,
    }
}

enum ProductMember<'g> {
    Field(&'g DeclarationId),
    NestedField {
        outer: &'g DeclarationId,
        inner: &'g DeclarationId,
    },
    TupleElement(usize),
    NestedTupleElement {
        outer: usize,
        inner: usize,
    },
}

fn product_member<'g>(
    access: Access,
    route: &[PatternProjection<'g>],
) -> Option<ProductMember<'g>> {
    let members = match (access, route) {
        (Access::Owned, members) => members,
        (_, [PatternProjection::Dereference, members @ ..]) => members,
        _ => return None,
    };
    match members {
        [PatternProjection::Field(field)] => Some(ProductMember::Field(field)),
        [
            PatternProjection::Field(outer),
            PatternProjection::Field(inner),
        ] if access == Access::Owned => Some(ProductMember::NestedField { outer, inner }),
        [PatternProjection::TupleElement(position)] => Some(ProductMember::TupleElement(*position)),
        [
            PatternProjection::TupleElement(outer),
            PatternProjection::TupleElement(inner),
        ] => Some(ProductMember::NestedTupleElement {
            outer: *outer,
            inner: *inner,
        }),
        _ => None,
    }
}

enum Binding<'a, 'g> {
    Discard,
    Name {
        declaration: &'g DeclarationId,
        ty: &'a Type<'g>,
        site: NodeId,
    },
    IrrefutableProduct {
        shape: NodeId,
        facts: &'a PatternFacts<'g>,
        access: Access,
        nested_reborrow: Option<&'a PlacePath<'g>>,
    },
}

#[derive(Clone, Copy)]
enum AssignmentPlan<'a, 'g> {
    General,
    Exact(&'a Type<'g>),
    NestedTuple(Slot),
    StagedNestedRecord(Slot),
    NestedRecordLeaf { ty: &'a Type<'g>, child: Slot },
}

#[derive(Clone, Copy)]
struct ProjectedTemporary {
    site: NodeId,
    base: NodeId,
    scratch: Slot,
    member: Slot,
}

impl<'a, 'g, 'b> Build<'a, 'g, 'b> {
    pub(super) fn new(
        typed: &'a TypedReport<'g>,
        module: &'g Module,
        body: NodeId,
        return_contract: Option<(&'g DeclarationId, &'a Type<'g>)>,
        indexed_facts: Option<&'a NodeIndex<'a, 'g>>,
        budget: &'b mut Budget,
    ) -> ResultOf<Self> {
        let mut facts = BTreeMap::new();
        if indexed_facts.is_none() {
            for fact in typed.bodies().typed_nodes() {
                budget.step(1)?;
                if fact.site.module == &module.key {
                    budget.reserve(1)?;
                    facts.insert(fact.site.node, fact);
                }
            }
        }
        Ok(Self {
            typed,
            module,
            body,
            facts,
            indexed_facts,
            locals: BTreeMap::new(),
            nested_record_values: BTreeMap::new(),
            nested_tuple_parameters: BTreeMap::new(),
            nested_tuple_bindings: BTreeMap::new(),
            retained: BTreeMap::new(),
            retains_exclusive_bool: false,
            incoming_exclusive_bools: Vec::new(),
            slots: Vec::new(),
            code: Vec::new(),
            calls: Vec::new(),
            cleanups: Vec::new(),
            body_scope: None,
            scalar_copy_read: None,
            dispose: typed.dispose_role().map(|d| &d.id),
            return_contract,
            proved_variant_cleanup: Vec::new(),
            budget,
        })
    }

    fn fact(&self, node: &NodeId) -> Option<&'a NodeFacts<'g>> {
        self.indexed_facts.unwrap_or(&self.facts).get(node).copied()
    }

    pub(super) fn check_scalar_copy_body(
        &mut self,
        formal: &MemberType<'g>,
        result: &Type<'g>,
    ) -> ResultOf<()> {
        let body = self.body;
        let pending = Stop::Pending(body);
        let nodes = &self.module.syntax.nodes;
        self.budget.step(7)?;
        let (NodeKind::Block, [statement]) = (nodes[body].kind, nodes[body].children.as_slice())
        else {
            return Err(pending);
        };
        let (NodeKind::Return, [read]) = (
            nodes[*statement].kind,
            nodes[*statement].children.as_slice(),
        ) else {
            return Err(pending);
        };
        let (NodeKind::Prefix("*"), [operand]) =
            (nodes[*read].kind, nodes[*read].children.as_slice())
        else {
            return Err(pending);
        };
        let formal = &formal.declaration.id;
        if !matches!(self.fact(statement).map(|fact| &fact.kind),
            Some(NodeFactKind::Return { target }) if self.return_contract.is_some_and(|(owner, _)| std::ptr::eq(*target, owner)))
        {
            return Err(pending);
        }
        let descriptor = self.normal_value(*operand)?;
        let value = self.normal_value(*read)?;
        if nodes[*operand].kind != NodeKind::ValueReference
            || !descriptor
                .binding
                .is_some_and(|binding| std::ptr::eq(binding, formal))
            || !matches!(&descriptor.path, Some(path) if std::ptr::eq(path.root, formal) && path.projections.is_empty())
            || descriptor.access != Access::Owned
            || scalar::shared_input(&descriptor.ty, result).is_none()
            || &value.ty != result
            || value.access != Access::ReadOnly
            || value.place != TypedPlace::ReadOnly
            || !value.read_only_projection
            || !matches!(&value.path, Some(path) if std::ptr::eq(path.root, formal)
                && path.projections.as_slice() == [PathProjection::Dereference])
            || descriptor.call.is_some()
            || descriptor.data.is_some()
            || descriptor.admission.is_some()
            || value.call.is_some()
            || value.data.is_some()
            || value.admission.is_some()
        {
            return Err(pending);
        }
        self.scalar_copy_read = Some(*read);
        Ok(())
    }

    fn depth(&mut self, depth: usize) -> ResultOf<()> {
        self.budget.step(1)?;
        if depth > self.budget.limits.max_depth.min(64) {
            return Err(Stop::Limit);
        }
        self.budget.statistics.max_depth = self.budget.statistics.max_depth.max(depth);
        Ok(())
    }

    pub(super) fn parameter(&mut self, parameter: &MemberType<'g>) -> ResultOf<Slot> {
        let node = parameter.declaration.node;
        let ty = &parameter.ty;
        if let Some(slot) = self.nested_record_region(node, ty)? {
            self.budget.reserve(1)?;
            self.nested_record_values
                .insert(&parameter.declaration.id, slot);
            return Ok(slot);
        }
        if let Some(slot) = self.nested_tuple_region(node, ty)? {
            self.budget.reserve(1)?;
            self.nested_tuple_parameters
                .insert(&parameter.declaration.id, slot);
            return Ok(slot);
        }
        self.local(&parameter.declaration.id, node, ty)
    }

    fn nested_tuple_region(&mut self, node: NodeId, ty: &Type<'g>) -> ResultOf<Option<Slot>> {
        let TypeTerm::Tuple(elements) = ty.as_ref() else {
            return Ok(None);
        };
        if !matches!(
            self.primitive_tuple_width(elements, |_| Ok(true))?,
            Some((_, true))
        ) {
            return Ok(None);
        }
        if self.outer_disposer(node, ty)?.is_some() {
            return Err(Stop::Pending(node));
        }
        let root = self.aggregate_root(node, None, 0)?;
        for (outer, element) in elements.iter().enumerate() {
            self.budget.step(1)?;
            let child = if let TypeTerm::Tuple(inner) = element.as_ref() {
                if self.outer_disposer(node, element)?.is_some() {
                    return Err(Stop::Pending(node));
                }
                self.aggregate(
                    node,
                    None,
                    inner
                        .iter()
                        .enumerate()
                        .map(|(inner, ty)| (Projection::NestedTupleElement { outer, inner }, ty)),
                )?
            } else {
                self.slot(node, element)?
            };
            self.embed_member(root, child, Projection::TupleElement(outer))?;
        }
        self.slots[root].members = self.slots.len() - root - 1;
        Ok(Some(root))
    }

    fn nested_record_region(&mut self, node: NodeId, ty: &Type<'g>) -> ResultOf<Option<Slot>> {
        let Some(fields) = self.record_fields(ty.as_ref())? else {
            return Ok(None);
        };
        let mut nested = false;
        for field in fields {
            self.budget.step(1)?;
            if self.record_fields(&field.ty)?.is_some() {
                nested = true;
                break;
            }
        }
        if !nested {
            return Ok(None);
        }
        if self.outer_disposer(node, ty)?.is_some() {
            return Err(Stop::Pending(node));
        }
        // Keep each child record in one contiguous region with the same
        // slot order as its standalone type. A field transfer moves that
        // region; outer cleanup still walks the remaining slots in reverse.
        let root = self.aggregate_root(node, None, 0)?;
        for outer in fields {
            self.budget.step(1)?;
            let child = if matches!(outer.ty.as_ref(), TypeTerm::Primitive(_)) {
                self.slot(node, &outer.ty)?
            } else {
                let inner_fields = self.record_fields(&outer.ty)?.ok_or(Stop::Pending(node))?;
                self.budget.step(inner_fields.len())?;
                if !inner_fields
                    .iter()
                    .all(|inner| matches!(inner.ty.as_ref(), TypeTerm::Primitive(_)))
                    || self.outer_disposer(node, &outer.ty)?.is_some()
                {
                    return Err(Stop::Pending(node));
                }
                self.aggregate(
                    node,
                    None,
                    inner_fields.iter().map(|inner| {
                        (
                            Projection::NestedField {
                                outer: outer.declaration,
                                inner: inner.declaration,
                            },
                            &inner.ty,
                        )
                    }),
                )?
            };
            self.embed_member(root, child, Projection::Field(outer.declaration))?;
        }
        self.slots[root].members = self.slots.len() - root - 1;
        Ok(Some(root))
    }

    fn embed_member(
        &mut self,
        root: Slot,
        child: Slot,
        projection: Projection<'g>,
    ) -> ResultOf<()> {
        self.budget.step(1)?;
        self.slots[child].projection = Some(projection);
        self.slots[root].moves |= self.slots[child].moves;
        for slot in self.slots[child].storage(child) {
            self.budget.step(1)?;
            self.slots[slot].owner = root;
        }
        Ok(())
    }

    fn local(&mut self, id: &'g DeclarationId, node: NodeId, ty: &Type<'g>) -> ResultOf<Slot> {
        let slot = self.slot(node, ty)?;
        self.budget.reserve(1)?;
        self.locals.insert(id, slot);
        Ok(slot)
    }

    pub(super) fn slot(&mut self, node: NodeId, ty: &Type<'g>) -> ResultOf<Slot> {
        let (moves, cleanup) = match ty.as_ref() {
            TypeTerm::Primitive(_) if owns_text(ty) => (true, StorageCleanup::Drop),
            TypeTerm::Primitive(_) => (false, StorageCleanup::End),
            TypeTerm::Reference { mutable, .. } => (*mutable, StorageCleanup::End),
            TypeTerm::Callable { .. } => (true, StorageCleanup::End),
            TypeTerm::Nominal { .. } if let Some(release) = self.guard_release(ty)? => {
                (true, StorageCleanup::ReleaseGuard(release))
            }
            TypeTerm::Nominal { .. } | TypeTerm::Variant { .. }
                if let Some((role, payload)) = self.admitted_option(ty) =>
            {
                self.prove_no_variant_disposer(node, ty, [role.some, role.none])?;
                match payload {
                    OptionPayload::Bool => (false, StorageCleanup::End),
                    OptionPayload::Text(_) if matches!(ty.as_ref(), TypeTerm::Variant { declaration, .. } if *declaration == &role.none.id) => {
                        (false, StorageCleanup::End)
                    }
                    OptionPayload::Text(_) => {
                        (true, StorageCleanup::DropOptionPayload(&role.some.id))
                    }
                }
            }
            TypeTerm::Nominal { .. } | TypeTerm::Variant { .. }
                if let Some((role, variant)) = self.scalar_result_role(ty) =>
            {
                self.prove_no_variant_disposer(node, ty, [role.ok, role.err])?;
                if variant == Some(&role.ok.id) {
                    (false, StorageCleanup::End)
                } else {
                    // Storage may contain a sealed Err, which this slice can
                    // transfer but cannot dispose.
                    (true, StorageCleanup::Unlowered)
                }
            }
            TypeTerm::Nominal { .. } => {
                let fields = self.record_fields(ty)?.ok_or(Stop::Pending(node))?;
                let disposer = self.outer_disposer(node, ty)?;
                return self.aggregate(
                    node,
                    disposer,
                    fields
                        .iter()
                        .map(|field| (Projection::Field(field.declaration), &field.ty)),
                );
            }
            TypeTerm::Tuple(elements) => {
                if self.outer_disposer(node, ty)?.is_some() {
                    return Err(Stop::Pending(node));
                }
                return self.aggregate(
                    node,
                    None,
                    elements
                        .iter()
                        .enumerate()
                        .map(|(i, element)| (Projection::TupleElement(i), element)),
                );
            }
            _ => return Err(Stop::Pending(node)),
        };
        self.budget.reserve(1)?;
        let slot = self.slots.len();
        self.slots.push(Local {
            node,
            owner: slot,
            projection: None,
            members: 0,
            reference_member: false,
            has_guard_field: false,
            custom_disposer: None,
            scalar_borrow_temporary: false,
            moves,
            cleanup,
        });
        Ok(slot)
    }

    fn guard_release(&mut self, ty: &TypeTerm<'g>) -> ResultOf<Option<GuardRelease<'g>>> {
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = ty
        else {
            return Ok(None);
        };
        let [argument] = arguments.as_slice() else {
            return Ok(None);
        };
        if !matches!(argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32") {
            return Ok(None);
        }
        let Some(provider) = self.guard_provider(ty)? else {
            return Ok(None);
        };
        let representations = self.typed.native_representations();
        self.budget.step(representations.len())?;
        Ok(representations
            .iter()
            .any(|representation| {
                &representation.owner.id == *declaration
                    && representation.kind == NativeRepresentationKind::GuardPayloadAndSource
                    && std::ptr::eq(representation.artifact, provider.artifact)
            })
            .then_some(GuardRelease {
                implementation: &provider.implementation.id,
                entry: &provider.entry.id,
            }))
    }

    fn guard_provider(
        &mut self,
        ty: &TypeTerm<'g>,
    ) -> ResultOf<Option<&'a NativeOwnershipProvider<'g>>> {
        let TypeTerm::Nominal { declaration, .. } = ty else {
            return Ok(None);
        };
        let scopes = self.typed.ownership_protection();
        self.budget.step(scopes.len())?;
        Ok(scopes.iter().find_map(|scope| {
            (&scope.owner.id == *declaration
                && Some(scope.family) == self.dispose
                && scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                && scope.coverage == OwnershipCoverage::Clear)
                .then_some(scope.provider.as_ref())
                .flatten()
        }))
    }

    fn primitive_tuple_elements(&mut self, elements: &[Type<'g>]) -> ResultOf<bool> {
        self.budget.step(elements.len())?;
        Ok(elements
            .iter()
            .all(|element| matches!(element.as_ref(), TypeTerm::Primitive(_))))
    }

    fn primitive_tuple_width(
        &mut self,
        elements: &[Type<'g>],
        allows_nesting: impl FnOnce(&mut Self) -> ResultOf<bool>,
    ) -> ResultOf<Option<(usize, bool)>> {
        let mut allows_nesting = Some(allows_nesting);
        let mut width = elements.len();
        let mut nested = false;
        self.budget.step(width)?;
        for element in elements {
            match element.as_ref() {
                TypeTerm::Primitive(_) => {}
                TypeTerm::Tuple(inner) => {
                    if let Some(check) = allows_nesting.take()
                        && !check(self)?
                    {
                        return Ok(None);
                    }
                    if !self.primitive_tuple_elements(inner)? {
                        return Ok(None);
                    }
                    nested = true;
                    width = width.checked_add(inner.len()).ok_or(Stop::Limit)?;
                }
                _ => return Ok(None),
            }
        }
        Ok(Some((width, nested)))
    }

    fn admits_direct_tuple_or_leaf_call(
        &mut self,
        call: &crate::analysis::scalars::CallFacts<'g>,
        result: &Type<'g>,
    ) -> ResultOf<bool> {
        // Check the whole signature once. None of these arguments can
        // receive a retained reference, including in a reordered pipe.
        if !(matches!(result.as_ref(), TypeTerm::Primitive(_)) || primitive_reference(result)) {
            return Ok(false);
        }
        self.budget.step(2)?;
        let Some(function) = self
            .typed
            .inventory()
            .type_facts()
            .declarations
            .get(call.callee)
        else {
            return Ok(false);
        };
        let DeclarationShape::Function {
            parameters,
            result: output,
        } = &function.shape
        else {
            return Ok(false);
        };
        if parameters.len() != call.arguments.len() || output != result {
            return Ok(false);
        }
        for (formal, &node) in parameters.iter().zip(&call.arguments) {
            self.budget.step(3)?;
            let actual = &self.value(node)?.ty;
            if !matches!(actual.as_ref(), TypeTerm::Primitive(_))
                && !shared_primitive_reference(actual)
            {
                let TypeTerm::Reference { mutable, referent } = actual.as_ref() else {
                    return Ok(false);
                };
                let TypeTerm::Tuple(elements) = referent.as_ref() else {
                    return Ok(false);
                };
                let Some((width, _)) = self.primitive_tuple_width(elements, |this| {
                    // Each nested exclusive argument must name an incoming
                    // parameter. Staging still checks overlapping reborrows.
                    Ok(!mutable
                        || this
                            .whole_named_owned_place(node)?
                            .is_some_and(|(_, path)| path.root.kind == DeclarationKind::Parameter))
                })?
                else {
                    return Ok(false);
                };
                // Charge the proved tuple shape before comparing types.
                self.budget.step(width.checked_add(2).ok_or(Stop::Limit)?)?;
            }
            if *actual != formal.ty {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn direct_application_call(call: &crate::analysis::scalars::CallFacts<'g>) -> bool {
        call.callee.kind == DeclarationKind::Function
            && call.callee.module.origin == Origin::Application
            && call.substitution.is_empty()
    }

    fn direct_application_signature(
        &mut self,
        node: NodeId,
        call: &crate::analysis::scalars::CallFacts<'g>,
    ) -> ResultOf<(&'a [MemberType<'g>], &'a Type<'g>)> {
        if !Self::direct_application_call(call) {
            return Err(Stop::Pending(node));
        }
        self.budget.step(1)?;
        let signature = self
            .typed
            .inventory()
            .type_facts()
            .declarations
            .get(call.callee)
            .ok_or(Stop::Pending(node))?;
        let DeclarationShape::Function { parameters, result } = &signature.shape else {
            return Err(Stop::Pending(node));
        };
        if parameters.len() != call.arguments.len() {
            return Err(Stop::Pending(node));
        }
        Ok((parameters, result))
    }

    fn check_exact_primitive_call(
        &mut self,
        node: NodeId,
        call: &CallFacts<'g>,
        expected: &Type<'g>,
    ) -> ResultOf<()> {
        let (parameters, result) = self.direct_application_signature(node, call)?;
        if result != expected {
            return Err(Stop::Pending(node));
        }
        self.check_exact_call_arguments(node, parameters, &call.arguments, false)
    }

    fn check_exact_call_arguments(
        &mut self,
        node: NodeId,
        parameters: &[MemberType<'g>],
        arguments: &[NodeId],
        allow_scalar_carriers: bool,
    ) -> ResultOf<()> {
        for (formal, &arg) in parameters.iter().zip(arguments) {
            self.budget.step(2)?;
            if !(matches!(formal.ty.as_ref(), TypeTerm::Primitive(_))
                || allow_scalar_carriers
                    && (self.bool_option_role(&formal.ty).is_some()
                        || self.scalar_result_role(&formal.ty).is_some()))
                || self.value(arg)?.ty != formal.ty
            {
                return Err(Stop::Pending(node));
            }
        }
        Ok(())
    }

    fn direct_call_type(&self, ty: &TypeTerm<'g>) -> bool {
        matches!(
            ty,
            TypeTerm::Primitive(_) | TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
        ) || primitive_reference(ty)
            || self.bool_option_role(ty).is_some()
            || self.scalar_result_role(ty).is_some()
    }

    fn bool_option_role(&self, ty: &TypeTerm<'g>) -> Option<OptionRole<'g>> {
        let (role, payload) = self.admitted_option(ty)?;
        matches!(payload, OptionPayload::Bool).then_some(role)
    }

    fn admitted_option<'t>(
        &self,
        ty: &'t TypeTerm<'g>,
    ) -> Option<(OptionRole<'g>, OptionPayload<'t, 'g>)> {
        let role = *self.typed.option_role()?;
        let parent = match ty {
            TypeTerm::Variant {
                parent,
                declaration,
            } if *declaration == &role.some.id || *declaration == &role.none.id => parent,
            other => other,
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = parent
        else {
            return None;
        };
        let [payload] = arguments.as_slice() else {
            return None;
        };
        if *declaration != &role.declaration.id {
            return None;
        }
        let TypeTerm::Primitive(primitive) = payload.as_ref() else {
            return None;
        };
        let payload = match primitive.name() {
            "bool" => OptionPayload::Bool,
            "string" => OptionPayload::Text(payload),
            _ => return None,
        };
        Some((role, payload))
    }

    fn scalar_result_role(
        &self,
        ty: &TypeTerm<'g>,
    ) -> Option<(&'a ResultRole<'g>, Option<&'g DeclarationId>)> {
        let role = self.typed.result_role()?;
        role.representation?;
        let (parent, variant) = match ty {
            TypeTerm::Variant {
                parent,
                declaration,
            } if *declaration == &role.ok.id || *declaration == &role.err.id => {
                (parent.as_ref(), Some(*declaration))
            }
            other => (other, None),
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = parent
        else {
            return None;
        };
        let [success, error] = arguments.as_slice() else {
            return None;
        };
        (*declaration == &role.declaration.id
            && matches!(success.as_ref(), TypeTerm::Primitive(p) if matches!(p.name(), "bool" | "i32"))
            && matches!(error.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))
            .then_some((role, variant))
    }

    fn prove_no_variant_disposer(
        &mut self,
        node: NodeId,
        ty: &Type<'g>,
        variants: [&'g Declaration; 2],
    ) -> ResultOf<()> {
        let parent = match ty.as_ref() {
            TypeTerm::Variant { parent, .. } => parent,
            _ => ty,
        };
        // The admitted parents have only scalar arguments, so comparison
        // is bounded. A successful proof covers both variants of this exact
        // instantiation, not other success or error types.
        for proved in &self.proved_variant_cleanup {
            self.budget.step(1)?;
            if proved == parent {
                return Ok(());
            }
        }
        if self.outer_disposer(node, parent)?.is_some() {
            return Err(Stop::Pending(node));
        }
        for variant in variants {
            self.budget.reserve(1)?;
            let ty = Arc::new(TypeTerm::Variant {
                parent: parent.clone(),
                declaration: &variant.id,
            });
            if self.outer_disposer(node, &ty)?.is_some() {
                return Err(Stop::Pending(node));
            }
        }
        self.budget.reserve(1)?;
        self.proved_variant_cleanup.push(parent.clone());
        Ok(())
    }

    fn aggregate_root(
        &mut self,
        node: NodeId,
        custom_disposer: Option<&'g DeclarationId>,
        members: usize,
    ) -> ResultOf<Slot> {
        self.budget.reserve(1 + members)?;
        let root = self.slots.len();
        self.slots.push(Local {
            node,
            owner: root,
            projection: None,
            members,
            reference_member: false,
            has_guard_field: false,
            custom_disposer,
            scalar_borrow_temporary: false,
            moves: custom_disposer.is_some(),
            cleanup: StorageCleanup::End,
        });
        Ok(root)
    }

    fn aggregate<'t>(
        &mut self,
        node: NodeId,
        custom_disposer: Option<&'g DeclarationId>,
        members: impl ExactSizeIterator<Item = (Projection<'g>, &'t Type<'g>)>,
    ) -> ResultOf<Slot>
    where
        'g: 't,
    {
        let root = self.aggregate_root(node, custom_disposer, members.len())?;
        for (projection, ty) in members {
            self.budget.step(1)?;
            let reference_member = custom_disposer.is_none() && shared_primitive_reference(ty);
            let cleanup = if owns_text(ty) {
                StorageCleanup::Drop
            } else if reference_member || matches!(ty.as_ref(), TypeTerm::Primitive(_)) {
                StorageCleanup::End
            } else if matches!(projection, Projection::Field(_))
                && let Some(release) = self.guard_release(ty)?
            {
                self.slots[root].has_guard_field = true;
                StorageCleanup::ReleaseGuard(release)
            } else {
                return Err(Stop::Pending(node));
            };
            let moves = cleanup != StorageCleanup::End;
            self.slots[root].moves |= moves;
            self.slots.push(Local {
                node,
                owner: root,
                projection: Some(projection),
                members: 0,
                reference_member,
                has_guard_field: false,
                custom_disposer: None,
                scalar_borrow_temporary: false,
                moves,
                cleanup,
            });
        }
        Ok(root)
    }

    fn record_fields(&mut self, ty: &TypeTerm<'g>) -> ResultOf<Option<&'a [MemberType<'g>]>> {
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = ty
        else {
            return Ok(None);
        };
        if declaration.module.origin != Origin::Application || !arguments.is_empty() {
            return Ok(None);
        }
        self.budget.step(1)?;
        let inventory = self.typed.inventory();
        let Some(facts) = inventory.type_facts().declarations.get(declaration) else {
            return Ok(None);
        };
        let DeclarationShape::Record { fields } = &facts.shape else {
            return Ok(None);
        };
        Ok(facts.binders.is_empty().then_some(fields.as_slice()))
    }

    pub(super) fn guard_bearing_type(&mut self, ty: &TypeTerm<'g>) -> ResultOf<bool> {
        if self.guard_provider(ty)?.is_some() {
            return Ok(true);
        }
        if let Some(fields) = self.record_fields(ty)? {
            for field in fields {
                self.budget.step(1)?;
                if self.guard_provider(&field.ty)?.is_some() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn outer_disposer(
        &mut self,
        node: NodeId,
        ty: &Type<'g>,
    ) -> ResultOf<Option<&'g DeclarationId>> {
        let inventory = self.typed.inventory();
        let dispose = self.dispose.ok_or(Stop::Pending(node))?;
        let family = ordinary::source::admitted_family(inventory, dispose, self.budget)?
            .map_err(|_| Stop::Pending(node))?;
        self.budget.reserve(1)?;
        let viable = ordinary::source::project(family, &[Some(ty.clone())], &[], self.budget)?;
        match ordinary::source::rank(family, &viable, self.budget)? {
            Some(HypotheticalOutcome::NoMatch) => Ok(None),
            Some(HypotheticalOutcome::Winner(winner)) => {
                // A selected but conditional implementation is not proof of a
                // custom disposer. Keep substitution/evidence-dependent cases open.
                self.budget.step(viable.len())?;
                let selected = viable
                    .iter()
                    .find(|choice| &choice.body.source.declaration == winner && choice.covers)
                    .ok_or(Stop::Pending(node))?;
                self.budget.step(2)?;
                if selected.body.header.binders.is_empty()
                    && selected.substitution.is_empty()
                    && [dispose, winner].into_iter().all(|id| {
                        inventory
                            .headers()
                            .get(id)
                            .is_some_and(|h| h.requirements.is_empty())
                    })
                {
                    Ok(Some(winner))
                } else {
                    Err(Stop::Pending(node))
                }
            }
            _ => Err(Stop::Pending(node)),
        }
    }

    fn value(&self, node: NodeId) -> ResultOf<&'a ExpressionFacts<'g>> {
        let Some(NodeFactKind::Expression(value)) = self.fact(&node).map(|f| &f.kind) else {
            return Err(Stop::Pending(node));
        };
        Ok(value)
    }

    fn normal_value(&mut self, node: NodeId) -> ResultOf<&'a ExpressionFacts<'g>> {
        self.budget.step(1)?;
        let fact = self.fact(&node).ok_or(Stop::Pending(node))?;
        let NodeFactKind::Expression(facts) = &fact.kind else {
            return Err(Stop::Pending(node));
        };
        let flow = &fact.flow;
        if !flow.normal
            || flow.pending
            || flow.returns
            || !flow.breaks.is_empty()
            || !flow.continues.is_empty()
        {
            return Err(Stop::Pending(node));
        }
        Ok(facts)
    }

    fn place(&mut self, node: NodeId) -> ResultOf<Slot> {
        let path = self.value(node)?.path.as_ref().ok_or(Stop::Pending(node))?;
        self.resolve_place(node, path)
    }

    fn nested_record_root(&mut self, path: &PlacePath<'g>) -> ResultOf<Option<Slot>> {
        if self.nested_record_values.is_empty() {
            return Ok(None);
        }
        self.budget.step(1)?;
        Ok(self.nested_record_values.get(path.root).copied())
    }

    fn read_value_place(&mut self, node: NodeId, facts: &ExpressionFacts<'g>) -> ResultOf<Slot> {
        let path = facts.path.as_ref().ok_or(Stop::Pending(node))?;
        if facts.access == Access::Owned
            && let Some(root) = self.nested_record_root(path)?
        {
            return match path.projections.as_slice() {
                [] => Ok(root),
                [PathProjection::Field(field)] => self.field(node, root, field),
                [PathProjection::Field(outer), PathProjection::Field(inner)]
                    if matches!(facts.ty.as_ref(), TypeTerm::Primitive(_)) =>
                {
                    self.nested_field(node, root, outer, inner)
                        .map(|(_, leaf)| leaf)
                }
                _ => Err(Stop::Pending(node)),
            };
        }
        self.resolve_place(node, path)
    }

    fn assignment_target(
        &mut self,
        left: NodeId,
        facts: &'a ExpressionFacts<'g>,
        value: NodeId,
        rhs_depth: usize,
    ) -> ResultOf<(Slot, AssignmentPlan<'a, 'g>)> {
        let path = facts.path.as_ref().ok_or(Stop::Pending(left))?;
        if facts.access == Access::Owned
            && facts.place == TypedPlace::Writable
            && matches!(facts.ty.as_ref(), TypeTerm::Tuple(_))
            && path.projections.is_empty()
            && !self.nested_tuple_bindings.is_empty()
        {
            self.budget.step(1)?;
            if let Some(&(target, _)) = self.nested_tuple_bindings.get(path.root) {
                let from = self
                    .nested_tuple_transfer_source(value, &facts.ty)?
                    .ok_or(Stop::Pending(value))?;
                return Ok((target, AssignmentPlan::NestedTuple(from)));
            }
        }
        if facts.place == TypedPlace::Writable
            && path.root.kind != DeclarationKind::Parameter
            && let Some(target) = self.named_nested_tuple_leaf(left, facts)?
        {
            self.budget.step(1)?;
            if self.value(value)?.ty != facts.ty {
                return Err(Stop::Pending(value));
            }
            return Ok((target, AssignmentPlan::Exact(&facts.ty)));
        }
        if facts.access == Access::Owned
            && facts.place == TypedPlace::Writable
            && path.projections.is_empty()
            && let Some(target) = self.nested_record_root(path)?
        {
            let (name, _) = self.ungroup_expression_at(value, rhs_depth)?;
            let incoming = self
                .nested_transfer_slot(name, value, &facts.ty)?
                .ok_or(Stop::Pending(value))?;
            return Ok((target, AssignmentPlan::StagedNestedRecord(incoming)));
        }
        if facts.access == Access::Owned
            && facts.place == TypedPlace::Writable
            && let [PathProjection::Field(field)] = path.projections.as_slice()
            && let Some(root) = self.nested_record_root(path)?
        {
            let child = self.field(left, root, field)?;
            self.budget.step(1)?;
            // Only replace a nonempty child region from a standalone value.
            if self.slots[child].members == 0 {
                return Err(Stop::Pending(left));
            }
            let Some((source, _)) = self.whole_named_owned_place(value)? else {
                return Err(Stop::Pending(value));
            };
            self.budget.step(1)?;
            if source.ty != facts.ty {
                return Err(Stop::Pending(value));
            }
            return Ok((child, AssignmentPlan::Exact(&source.ty)));
        }
        if facts.access == Access::Owned
            && facts.place == TypedPlace::Writable
            && path.root.kind == DeclarationKind::Binding
            && owns_text(&facts.ty)
            && let [PathProjection::Field(outer), PathProjection::Field(inner)] =
                path.projections.as_slice()
            && let Some(root) = self.nested_record_root(path)?
        {
            let (child, target) = self.nested_field(left, root, outer, inner)?;
            let (name, _) = self.ungroup_expression_at(value, rhs_depth)?;
            let Some((source, _)) = self.named_owned_place(name)? else {
                return Err(Stop::Pending(value));
            };
            self.budget.step(1)?;
            if source.ty != facts.ty {
                return Err(Stop::Pending(value));
            }
            return Ok((
                target,
                AssignmentPlan::NestedRecordLeaf {
                    ty: &source.ty,
                    child,
                },
            ));
        }
        Ok((self.resolve_place(left, path)?, AssignmentPlan::General))
    }

    fn nested_transfer_slot(
        &mut self,
        value: NodeId,
        dest: NodeId,
        expected: &Type<'g>,
    ) -> ResultOf<Option<Slot>> {
        if self.nested_record_values.is_empty() {
            return Ok(None);
        }
        let Some((facts, path)) = self.whole_named_owned_place(value)? else {
            return Ok(None);
        };
        let Some(from) = self.nested_record_root(path)? else {
            return Ok(None);
        };
        // The source layout is already proved for this exact nominal type.
        self.budget.step(1)?;
        if facts.ty != *expected {
            return Err(Stop::Pending(value));
        }
        self.clone_region(from, dest).map(Some)
    }

    fn named_nested_tuple_leaf(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
    ) -> ResultOf<Option<Slot>> {
        if facts.access != Access::Owned
            || !matches!(facts.ty.as_ref(), TypeTerm::Primitive(_))
            || self.nested_tuple_parameters.is_empty() && self.nested_tuple_bindings.is_empty()
        {
            return Ok(None);
        }
        let Some(path) = &facts.path else {
            return Ok(None);
        };
        let (outer, inner) = match path.projections.as_slice() {
            [PathProjection::TupleElement(outer)] => (*outer, None),
            [
                PathProjection::TupleElement(outer),
                PathProjection::TupleElement(inner),
            ] => (*outer, Some(*inner)),
            _ => return Ok(None),
        };
        let Some(root) = self.named_nested_tuple_root(path)? else {
            return Ok(None);
        };
        if !matches!(&facts.data, Some(DataExpression::TupleIndex { .. })) {
            // Typed groups preserve the exact place; indexing data stays on the inner node.
            let inner = self.ungroup(node)?;
            if inner == node {
                return Ok(None);
            }
            self.budget.step(1)?;
            if !matches!(
                &self.value(inner)?.data,
                Some(DataExpression::TupleIndex { .. })
            ) {
                return Ok(None);
            }
        }
        self.nested_tuple_member_slot(node, root, outer, inner)
            .map(Some)
    }

    fn named_nested_tuple_root(&mut self, path: &PlacePath<'g>) -> ResultOf<Option<Slot>> {
        self.budget.step(1)?;
        Ok(match path.root.kind {
            DeclarationKind::Parameter => self.nested_tuple_parameters.get(path.root).copied(),
            _ => self
                .nested_tuple_bindings
                .get(path.root)
                .map(|&(slot, _)| slot),
        })
    }

    fn nested_tuple_transfer_source(
        &mut self,
        value: NodeId,
        expected: &Type<'g>,
    ) -> ResultOf<Option<Slot>> {
        if !matches!(expected.as_ref(), TypeTerm::Tuple(_))
            || self.nested_tuple_parameters.is_empty() && self.nested_tuple_bindings.is_empty()
        {
            return Ok(None);
        }
        let Some((facts, path)) = self.whole_named_owned_place(value)? else {
            return Ok(None);
        };
        let Some(from) = self.named_nested_tuple_root(path)? else {
            return Ok(None);
        };
        self.budget.step(self.slots[from].storage(from).len())?;
        if facts.ty != *expected {
            return Err(Stop::Pending(value));
        }
        Ok(Some(from))
    }

    fn clone_region(&mut self, from: Slot, dest: NodeId) -> ResultOf<Slot> {
        let region = self.slots[from].storage(from);
        let owner = self.slots.len();
        self.budget.step(region.len())?;
        self.budget.reserve(region.len())?;
        for slot in region {
            self.slots.push(Local {
                node: dest,
                owner,
                ..self.slots[slot]
            });
        }
        Ok(owner)
    }

    fn nested_construction_slot(
        &mut self,
        value: NodeId,
        dest: NodeId,
        expected: &Type<'g>,
    ) -> ResultOf<Option<Slot>> {
        if !matches!(expected.as_ref(), TypeTerm::Nominal { arguments, .. } if arguments.is_empty())
        {
            return Ok(None);
        }
        let source = self.ungroup(value)?;
        self.budget.step(2)?;
        if self.module.syntax.nodes[source].kind != NodeKind::Construction {
            return Ok(None);
        }
        let Some(DataExpression::Construction(data)) = &self.value(source)?.data else {
            return Ok(None);
        };
        if data.base.is_some() || !data.arguments.is_empty() || data.ty != *expected {
            return Ok(None);
        }
        self.nested_record_region(dest, expected)
    }

    fn binding_destination(&mut self, node: NodeId, id: &DeclarationId) -> ResultOf<(Slot, bool)> {
        self.budget.step(1)?;
        if let Some(&slot) = self.locals.get(id) {
            return Ok((slot, false));
        }
        self.budget.step(1)?;
        self.nested_record_values
            .get(id)
            .map(|&slot| (slot, true))
            .ok_or(Stop::Pending(node))
    }

    fn resolve_place(&mut self, node: NodeId, path: &PlacePath<'g>) -> ResultOf<Slot> {
        self.budget.step(1)?;
        let root = *self.locals.get(path.root).ok_or(Stop::Pending(node))?;
        match path.projections.as_slice() {
            [] => Ok(root),
            [PathProjection::Field(field)] => self.field(node, root, field),
            [PathProjection::TupleElement(element)] => self.element(node, root, *element),
            _ => Err(Stop::Pending(node)),
        }
    }

    fn field(&mut self, node: NodeId, root: Slot, id: &DeclarationId) -> ResultOf<Slot> {
        for field in self.slots[root].storage(root).skip(1) {
            self.budget.step(1)?;
            if matches!(self.slots[field].projection, Some(Projection::Field(f)) if f.id == *id) {
                return Ok(field);
            }
        }
        Err(Stop::Pending(node))
    }

    fn nested_field(
        &mut self,
        node: NodeId,
        root: Slot,
        outer: &DeclarationId,
        inner: &DeclarationId,
    ) -> ResultOf<(Slot, Slot)> {
        let child = self.field(node, root, outer)?;
        for field in self.slots[child].storage(child).skip(1) {
            self.budget.step(1)?;
            if matches!(self.slots[field].projection, Some(Projection::NestedField { outer: parent, inner: leaf })
                if parent.id == *outer && leaf.id == *inner)
            {
                return Ok((child, field));
            }
        }
        Err(Stop::Pending(node))
    }

    fn nested_tuple_member_slot(
        &mut self,
        node: NodeId,
        root: Slot,
        outer: usize,
        inner: Option<usize>,
    ) -> ResultOf<Slot> {
        for slot in self.slots[root].storage(root).skip(1) {
            self.budget.step(1)?;
            match (inner, self.slots[slot].projection) {
                (None, Some(Projection::TupleElement(index))) if index == outer => return Ok(slot),
                (Some(index), Some(Projection::NestedTupleElement { outer: a, inner: b }))
                    if (a, b) == (outer, index) =>
                {
                    return Ok(slot);
                }
                _ => {}
            }
        }
        Err(Stop::Pending(node))
    }

    fn element(&mut self, node: NodeId, root: Slot, position: usize) -> ResultOf<Slot> {
        self.budget.step(1)?;
        if position >= self.slots[root].members {
            return Err(Stop::Pending(node));
        }
        let slot = root + 1 + position;
        match self.slots[slot].projection {
            Some(Projection::TupleElement(index)) if index == position => Ok(slot),
            _ => Err(Stop::Pending(node)),
        }
    }

    fn member(&mut self, node: NodeId, root: Slot, projection: Projection<'g>) -> ResultOf<Slot> {
        match projection {
            Projection::Field(field) => self.field(node, root, &field.id),
            Projection::TupleElement(position) => self.element(node, root, position),
            Projection::NestedField { .. } | Projection::NestedTupleElement { .. } => {
                Err(Stop::Pending(node))
            }
        }
    }

    pub(super) fn emit(&mut self, node: NodeId, action: Action<'g>, edges: Edges) -> ResultOf<Pc> {
        self.budget.reserve(1)?;
        let pc = self.code.len();
        self.code.push(Instruction {
            node,
            action,
            edges,
        });
        Ok(pc)
    }

    pub(super) fn cleanup(&mut self, node: NodeId, slots: &[Slot], next: Pc) -> ResultOf<Pc> {
        self.emit_cleanup(node, slots, next, Action::Cleanup)
    }

    pub(super) fn cleanup_scope(&mut self, node: NodeId, slots: &[Slot], next: Pc) -> ResultOf<Pc> {
        self.emit_cleanup(node, slots, next, Action::EndScope)
    }

    fn emit_cleanup(
        &mut self,
        node: NodeId,
        slots: &[Slot],
        next: Pc,
        action: fn(usize) -> Action<'g>,
    ) -> ResultOf<Pc> {
        if slots.is_empty() {
            return Ok(next);
        }
        self.budget.reserve(1 + slots.len())?;
        let index = self.cleanups.len();
        // Locals are collected in declaration order, and get consecutive slot ids.
        self.cleanups.push(slots.to_vec());
        self.emit(node, action(index), Edges::Next(next))
    }

    fn bool_deref_descriptor_slot(&mut self, node: NodeId, path: &PlacePath<'g>) -> ResultOf<Slot> {
        let dereference = self.ungroup(node)?;
        let syntax = &self.module.syntax.nodes[dereference];
        if syntax.kind != NodeKind::Prefix("*")
            || path.projections.as_slice() != [PathProjection::Dereference]
        {
            return Err(Stop::Pending(node));
        }
        self.budget.step(2)?;
        let source = self.value(syntax.children[0])?;
        if retained_bool_contents_type(&source.ty).is_none() {
            return Err(Stop::Pending(node));
        }
        self.locals
            .get(path.root)
            .copied()
            .ok_or(Stop::Pending(node))
    }

    fn immutable_named_value(
        &mut self,
        node: NodeId,
    ) -> ResultOf<(&'g DeclarationId, &'a Type<'g>)> {
        self.budget.step(1)?;
        let facts = self.normal_value(node)?;
        let path = facts.path.as_ref().ok_or(Stop::Pending(node))?;
        if self.module.syntax.nodes[node].kind != NodeKind::ValueReference
            || facts.place != TypedPlace::ReadOnly
            || facts.access != Access::Owned
            || facts.binding != Some(path.root)
            || !path.projections.is_empty()
            || facts.refinement.is_some()
        {
            return Err(Stop::Pending(node));
        }
        Ok((path.root, &facts.ty))
    }

    fn original_two_hop_parameter(
        &mut self,
        node: NodeId,
        root: &DeclarationId,
    ) -> ResultOf<OriginalTwoHopBool> {
        if root.kind != DeclarationKind::Parameter {
            return Err(Stop::Pending(node));
        }
        self.budget.step(2)?;
        // The parameter cannot be reassigned. Ordinary place resolution
        // doesn't admit a dereference here, and the one-hop cases cannot
        // move, replace, or expose the inner exclusive descriptor.
        let descriptor = *self.locals.get(root).ok_or(Stop::Pending(node))?;
        let Some(&RetainedContents::TwoHop(contents)) = self.retained.get(&descriptor) else {
            return Err(Stop::Pending(node));
        };
        Ok(OriginalTwoHopBool {
            descriptor,
            contents,
        })
    }

    fn terminal_two_hop_store(
        &mut self,
        node: NodeId,
        left: NodeId,
        rhs: NodeId,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.budget.step(1)?;
        // Match the typed handoff's flat terminal shape. No earlier action
        // may touch the outer or inner descriptor.
        let binding = match self.module.syntax.nodes[self.body].children.as_slice() {
            [store] if *store == node => None,
            [binding, store] if *store == node => Some(*binding),
            _ => return Err(Stop::Pending(left)),
        };
        let nodes = &self.module.syntax.nodes;
        self.depth(depth + 1)?;
        let (NodeKind::Prefix("*"), [inner]) = (nodes[left].kind, nodes[left].children.as_slice())
        else {
            return Err(Stop::Pending(left));
        };
        self.depth(depth + 2)?;
        let (NodeKind::Prefix("*"), [parameter]) =
            (nodes[*inner].kind, nodes[*inner].children.as_slice())
        else {
            return Err(Stop::Pending(*inner));
        };
        self.depth(depth + 3)?;
        let (root, parameter_ty) = self.immutable_named_value(*parameter)?;
        let route = self.original_two_hop_parameter(*parameter, root)?;
        let TypeTerm::Reference {
            mutable: true,
            referent: inner_ty,
        } = parameter_ty.as_ref()
        else {
            return Err(Stop::Pending(*parameter));
        };
        let shared_ty = retained_bool_contents_type(inner_ty).ok_or(Stop::Pending(*inner))?;
        for (hop, ty, projections) in [
            (*inner, inner_ty, &[PathProjection::Dereference][..]),
            (
                left,
                shared_ty,
                &[PathProjection::Dereference, PathProjection::Dereference][..],
            ),
        ] {
            let facts = self.normal_value(hop)?;
            if facts.ty != *ty
                || facts.place != TypedPlace::Writable
                || facts.access != Access::Writable
                || facts.refinement.is_some()
                || facts.read_only_projection
                || !facts
                    .path
                    .as_ref()
                    .is_some_and(|path| path.root == root && path.projections == projections)
            {
                return Err(Stop::Pending(hop));
            }
        }
        if let Some(binding) = binding {
            self.budget.step(3)?;
            let (NodeKind::Binding { mutable: false }, [name, literal]) =
                (nodes[binding].kind, nodes[binding].children.as_slice())
            else {
                return Err(Stop::Pending(binding));
            };
            self.depth(depth + 1)?;
            let (NodeKind::Prefix("&"), [flag]) = (nodes[rhs].kind, nodes[rhs].children.as_slice())
            else {
                return Err(Stop::Pending(rhs));
            };
            let TypeTerm::Reference {
                referent: bool_ty, ..
            } = shared_ty.as_ref()
            else {
                return Err(Stop::Pending(rhs));
            };
            self.depth(depth + 2)?;
            let (backing, backing_ty) = self.immutable_named_value(*flag)?;
            if backing_ty != bool_ty
                || self.normal_value(rhs)?.ty != *shared_ty
                || nodes[*name].kind != NodeKind::NamePattern
                || nodes[*literal].kind != NodeKind::Literal
                || self.normal_value(*literal)?.ty != *bool_ty
                || !matches!(self.binding(binding)?, Binding::Name { declaration, .. }
                    if declaration == backing)
            {
                return Err(Stop::Pending(rhs));
            }
        } else {
            self.depth(depth + 1)?;
            let (source, source_ty) = self.immutable_named_value(rhs)?;
            if source_ty != shared_ty || source.kind != DeclarationKind::Parameter {
                return Err(Stop::Pending(rhs));
            }
        }
        let incoming = self.slot(rhs, shared_ty)?;
        let install = self.emit(
            node,
            Action::InstallThroughTwoHop { route, incoming },
            Edges::Next(next),
        )?;
        self.expression(rhs, Some(incoming), install, depth + 1)
    }

    fn check_retained(&mut self, node: NodeId, next: Pc) -> ResultOf<Pc> {
        if self.retained.is_empty() {
            Ok(next)
        } else {
            self.emit(node, Action::CheckRetained, Edges::Next(next))
        }
    }

    pub(super) fn expectation(&mut self, result: &Type<'g>) -> ResultOf<Pc> {
        let root = self.body;
        let [expression] = self.module.syntax.nodes[root].children[..] else {
            return Err(Stop::Pending(root));
        };
        if !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            || self.normal_value(expression)?.ty != *result
        {
            return Err(Stop::Pending(expression));
        }
        let output = self.slot(expression, result)?;
        let finish = self.emit(expression, Action::Export(output), Edges::FinishExpectation)?;
        let (projected, projection_depth) = self.ungroup_expression_at(expression, 1)?;
        if let Some(temporary) = self.temporary_projection(projected, true)? {
            let cleanup = self.cleanup(expression, &[temporary.scratch], finish)?;
            return self.stage_value(
                expression,
                output,
                cleanup,
                Some(temporary),
                projection_depth + 1,
            );
        }
        self.expression(expression, Some(output), finish, 1)
    }

    pub(super) fn startup(&mut self) -> ResultOf<Pc> {
        let root = self.body;
        self.depth(1)?;
        let mut children = Vec::new();
        for &child in &self.module.syntax.nodes[root].children {
            self.budget.step(1)?;
            // Same child selection as the integrated typed startup pass.
            if !matches!(
                self.module.syntax.nodes[child].kind,
                NodeKind::Export
                    | NodeKind::Function
                    | NodeKind::Import
                    | NodeKind::Record
                    | NodeKind::Enum
                    | NodeKind::Expect
                    | NodeKind::Implementation
            ) {
                self.budget.reserve(1)?;
                children.push(child);
            }
        }
        let end = self.emit(root, Action::None, Edges::FinishStartup)?;
        self.sequence(
            root,
            &children,
            end,
            Exits {
                returning: None,
                looping: None,
            },
            1,
        )
    }

    fn sequence(
        &mut self,
        node: NodeId,
        children: &[NodeId],
        next: Pc,
        exits: Exits,
        depth: usize,
    ) -> ResultOf<Pc> {
        let mut locals = Vec::new();
        let mut prefix = children.len();
        for (index, &child) in children.iter().enumerate() {
            self.budget.step(1)?;
            let statement = &self.module.syntax.nodes[child];
            let control = if matches!(statement.kind, NodeKind::Binding { .. }) {
                let initializer = *statement.children.last().unwrap();
                self.declare_binding(child, initializer, &mut locals)?;
                initializer
            } else {
                child
            };
            // Typed statement flow already composes nested exits.
            // An infeasible suffix introduces no slots or obligations.
            if self
                .fact(&control)
                .is_some_and(|facts| !facts.flow.normal && !facts.flow.pending)
            {
                prefix = index + 1;
                break;
            }
        }
        let exits = Exits {
            returning: exits
                .returning
                .map(|target| self.cleanup_scope(node, &locals, target))
                .transpose()?,
            looping: match exits.looping {
                Some(looping) => Some(Loop {
                    breaks: self.cleanup_scope(node, &locals, looping.breaks)?,
                    continues: self.cleanup_scope(node, &locals, looping.continues)?,
                }),
                None => None,
            },
        };
        // Module startup has no closing brace. Attribute its normal lexical
        // exit to the last reached startup statement, not the entire file.
        let exit_site = if node == self.module.syntax.root && prefix != 0 {
            children[prefix - 1]
        } else {
            node
        };
        let mut tail = self.cleanup_scope(exit_site, &locals, next)?;
        if node == self.body {
            if !locals.is_empty()
                && let Action::EndScope(index) = self.code[tail].action
            {
                self.body_scope = Some(index);
            }
            tail = self.check_retained(node, tail)?;
        }
        for &child in children[..prefix].iter().rev() {
            tail = self.statement(child, tail, exits, depth + 1)?;
        }
        Ok(tail)
    }

    pub(super) fn statement(
        &mut self,
        node: NodeId,
        next: Pc,
        exits: Exits,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let syntax = &self.module.syntax.nodes[node];
        let children = &syntax.children;
        match syntax.kind {
            NodeKind::Block => self.sequence(node, children, next, exits, depth),
            // A nested function is a separate source root, not an executed statement.
            NodeKind::Function => Ok(next),
            NodeKind::Binding { .. } => {
                let value = *children.last().unwrap();
                let binding = self.binding(node)?;
                let (initializer, initializer_depth) =
                    self.ungroup_expression_at(value, depth + 1)?;
                if self.propagates_directly(initializer)? {
                    let into = match binding {
                        Binding::Discard => None,
                        Binding::Name { declaration, .. } => {
                            Some(self.binding_destination(node, declaration)?.0)
                        }
                        _ => return Err(Stop::Pending(node)),
                    };
                    return self.propagate(
                        initializer,
                        into,
                        next,
                        exits.returning.ok_or(Stop::Pending(initializer))?,
                        initializer_depth,
                    );
                }
                if let Some(call) = self.check_value_call_propagation(
                    initializer,
                    false,
                    exits.returning,
                    initializer_depth,
                )? {
                    let into = match binding {
                        Binding::Discard => None,
                        Binding::Name { declaration, .. } => {
                            Some(self.binding_destination(node, declaration)?.0)
                        }
                        _ => return Err(Stop::Pending(node)),
                    };
                    return self.lower_propagating_value_call(
                        node,
                        call,
                        into,
                        next,
                        initializer_depth,
                    );
                }
                match binding {
                    Binding::Discard => self.discard(node, value, next, depth + 1),
                    Binding::Name {
                        declaration, ty, ..
                    } => {
                        if matches!(ty.as_ref(), TypeTerm::Tuple(_))
                            && !self.nested_tuple_bindings.is_empty()
                        {
                            self.budget.step(1)?;
                            if let Some(&(into, from)) = self.nested_tuple_bindings.get(declaration)
                            {
                                return self.expression_with_proof(
                                    value,
                                    Some(into),
                                    next,
                                    depth + 1,
                                    Some(ExpressionProof::NestedTupleSource(from)),
                                );
                            }
                        }
                        let (into, nested) = self.binding_destination(node, declaration)?;
                        if nested {
                            return self.expression_with_proof(
                                value,
                                Some(into),
                                next,
                                depth + 1,
                                Some(ExpressionProof::NestedRecordConstruction),
                            );
                        }
                        self.budget.step(1)?;
                        if self.module.syntax.nodes[initializer].kind == NodeKind::Construction {
                            if let Some(entry) = self.initialize_propagating_record(
                                value,
                                initializer,
                                into,
                                next,
                                exits.returning,
                                depth + 1,
                            )? {
                                return Ok(entry);
                            }
                            return self.expression_with_proof(
                                value,
                                Some(into),
                                next,
                                depth + 1,
                                Some(ExpressionProof::NamedRecordRebuild),
                            );
                        }
                        if matches!(
                            self.module.syntax.nodes[initializer].kind,
                            NodeKind::Prefix("*" | "&")
                        ) && let Some(entry) = self.binding_temporary_borrow(
                            node,
                            initializer,
                            into,
                            next,
                            initializer_depth,
                        )? {
                            return Ok(entry);
                        }
                        let temporary = self.temporary_projection(initializer, false)?;
                        let tail = if let Some(temp) = temporary {
                            self.cleanup(node, &[temp.scratch], next)?
                        } else {
                            next
                        };
                        self.stage_named_transfer(value, into, tail, temporary, depth + 1)
                    }
                    Binding::IrrefutableProduct {
                        facts,
                        access,
                        nested_reborrow,
                        ..
                    } => self.bind_product(node, facts, access, nested_reborrow, next, depth + 1),
                }
            }
            NodeKind::ExpressionStatement => {
                let value = children[0];
                let (expression, expression_depth) =
                    self.ungroup_expression_at(value, depth + 1)?;
                if self.propagates_directly(expression)? {
                    let returning = exits.returning.ok_or(Stop::Pending(expression))?;
                    self.propagate(expression, None, next, returning, expression_depth)
                } else if let Some(call) = self.check_value_call_propagation(
                    expression,
                    true,
                    exits.returning,
                    expression_depth,
                )? {
                    self.lower_propagating_value_call(node, call, None, next, expression_depth)
                } else {
                    self.discard(node, value, next, depth + 1)
                }
            }
            NodeKind::Assignment("=") => {
                let (left, value) = (children[0], children[1]);
                self.budget.step(1)?;
                let facts = self.value(left)?;
                if let Some(path) = &facts.path
                    && path.projections.as_slice() == [PathProjection::Dereference]
                {
                    let descriptor = self.bool_deref_descriptor_slot(left, path)?;
                    self.budget.step(1)?;
                    let ty = &self.value(value)?.ty;
                    let incoming = self.slot(value, ty)?;
                    let install = self.emit(
                        node,
                        Action::InstallThrough {
                            descriptor,
                            incoming,
                        },
                        Edges::Next(next),
                    )?;
                    return self.expression(value, Some(incoming), install, depth + 1);
                }
                if facts.path.as_ref().is_some_and(|path| {
                    path.projections.as_slice()
                        == [PathProjection::Dereference, PathProjection::Dereference]
                }) {
                    return self.terminal_two_hop_store(node, left, value, next, depth);
                }
                let (target, source) = self.assignment_target(left, facts, value, depth + 1)?;
                if self.slots[target].projection.is_some()
                    && self.slots[target].guard_release().is_some()
                {
                    return Err(Stop::Pending(left));
                }
                let (incoming, temporary) = if let AssignmentPlan::NestedTuple(from) = source {
                    (self.clone_region(from, value)?, None)
                } else if let AssignmentPlan::StagedNestedRecord(incoming) = source {
                    (incoming, None)
                } else {
                    let ty = if let AssignmentPlan::Exact(ty)
                    | AssignmentPlan::NestedRecordLeaf { ty, .. } = source
                    {
                        ty
                    } else {
                        self.budget.step(1)?;
                        &self.value(value)?.ty
                    };
                    let incoming = self.slot(value, ty)?;
                    let initializer = self.ungroup(value)?;
                    (incoming, self.temporary_projection(initializer, false)?)
                };
                let next = if let Some(temp) = temporary {
                    self.cleanup(node, &[temp.scratch], next)?
                } else {
                    next
                };
                let install = self.emit(
                    node,
                    Action::Install { target, incoming },
                    Edges::Next(next),
                )?;
                let displace = self.emit(
                    node,
                    Action::Displace { target, incoming },
                    Edges::Next(install),
                )?;
                if let AssignmentPlan::NestedTuple(from) = source {
                    return self.expression_with_proof(
                        value,
                        Some(incoming),
                        displace,
                        depth + 1,
                        Some(ExpressionProof::NestedTupleSource(from)),
                    );
                }
                let stage =
                    self.stage_named_transfer(value, incoming, displace, temporary, depth + 1)?;
                if let AssignmentPlan::NestedRecordLeaf { child, .. } = source {
                    return self.emit(
                        node,
                        Action::CheckNestedFieldBase(child),
                        Edges::Next(stage),
                    );
                }
                Ok(stage)
            }
            NodeKind::Return => {
                let (_, result) = self.return_contract.ok_or(Stop::Pending(node))?;
                let returning =
                    self.check_retained(node, exits.returning.ok_or(Stop::Pending(node))?)?;
                if let Some(&value) = children.first() {
                    let ty = self.value(value)?.ty.clone();
                    let tuple_source = self.nested_tuple_transfer_source(value, result)?;
                    let output = if let Some(from) = tuple_source {
                        self.clone_region(from, value)?
                    } else if let Some(slot) = self.nested_transfer_slot(value, value, result)? {
                        slot
                    } else {
                        self.slot(value, &ty)?
                    };
                    if self.slots[output].contains_guard() {
                        return Err(Stop::Pending(value));
                    }
                    let export = self.emit(node, Action::Export(output), Edges::Next(returning))?;
                    if let Some(from) = tuple_source {
                        return self.expression_with_proof(
                            value,
                            Some(output),
                            export,
                            depth + 1,
                            Some(ExpressionProof::NestedTupleSource(from)),
                        );
                    }
                    let projected = self.ungroup(value)?;
                    let temporary = self.temporary_projection(projected, true)?;
                    let finish = if let Some(temp) = temporary {
                        self.cleanup(node, &[temp.scratch], export)?
                    } else {
                        export
                    };
                    self.stage_named_transfer(value, output, finish, temporary, depth + 1)
                } else {
                    Ok(returning)
                }
            }
            NodeKind::IfStatement => {
                let cond = children[0];
                let (can_be_true, can_be_false) = self.outcomes(cond)?;
                let yes = if can_be_true {
                    self.statement(children[1], next, exits, depth + 1)?
                } else {
                    next
                };
                let no = if let Some(&alternate) = children.get(2).filter(|_| can_be_false) {
                    self.statement(alternate, next, exits, depth + 1)?
                } else {
                    next
                };
                self.condition(cond, yes, no, depth + 1)
            }
            NodeKind::While => {
                let cond = children[0];
                let (can_be_true, _) = self.outcomes(cond)?;
                let yes = if can_be_true {
                    let backedge = self.emit(node, Action::BudgetBackedge, Edges::End)?;
                    let looping = Some(Loop {
                        breaks: next,
                        continues: backedge,
                    });
                    let yes = self.statement(
                        children[1],
                        backedge,
                        Exits { looping, ..exits },
                        depth + 1,
                    )?;
                    let head = self.condition(cond, yes, next, depth + 1)?;
                    self.code[backedge].edges = Edges::Next(head);
                    return Ok(head);
                } else {
                    next
                };
                self.condition(cond, yes, next, depth + 1)
            }
            NodeKind::Break => Ok(exits.looping.ok_or(Stop::Pending(node))?.breaks),
            NodeKind::Continue => Ok(exits.looping.ok_or(Stop::Pending(node))?.continues),
            _ => Err(Stop::Pending(node)),
        }
    }

    fn binding_temporary_borrow(
        &mut self,
        statement: NodeId,
        root: NodeId,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Option<Pc>> {
        let (read, borrow, depth) = if self.module.syntax.nodes[root].kind == NodeKind::Prefix("*")
        {
            let (borrow, depth) =
                self.ungroup_expression_at(self.module.syntax.nodes[root].children[0], depth + 1)?;
            (Some(root), borrow, depth)
        } else {
            (None, root, depth)
        };
        let syntax = &self.module.syntax.nodes[borrow];
        if syntax.kind != NodeKind::Prefix("&") {
            return Ok(None);
        }
        let producer = syntax.children[0];
        let (call_node, _) = self.ungroup_expression_at(producer, depth + 1)?;
        if self.module.syntax.nodes[call_node].kind != NodeKind::Call {
            return Ok(None);
        }
        self.budget.step(1)?;
        let produced = self.value(call_node)?;
        let Some(call) = produced.call.as_deref() else {
            return Ok(None);
        };
        let descriptor = self.normal_value(borrow)?;
        if !matches!(produced.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            || produced.path.is_some()
            || produced.access != Access::Owned
            || !matches!(descriptor.ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if *referent == produced.ty)
        {
            return Ok(None);
        }
        if let Some(read) = read
            && self.normal_value(read)?.ty != produced.ty
        {
            return Ok(None);
        }
        self.check_exact_primitive_call(call_node, call, &produced.ty)?;
        // The pinned Bool disposer has no effect. Do not silently replace a
        // selected user disposer, or assume default cleanup when coverage is pending.
        let cleanup = self.typed.primitive_protection();
        self.budget.step(cleanup.len())?;
        if !cleanup.iter().any(|scope| {
            Some(scope.family) == self.dispose
                && scope.role == PrimitiveRole::Dispose
                && scope.primitive.name() == "bool"
                && scope.coverage == PrimitiveCoverage::Clear
                && scope.required_winner.is_some()
        }) {
            return Err(Stop::Pending(call_node));
        }

        let backing = self.slot(call_node, &produced.ty)?;
        self.slots[backing].scalar_borrow_temporary = true;
        let (holder, after_borrow) = if let Some(read) = read {
            let holder = self.slot(borrow, &descriptor.ty)?;
            self.slots[holder].scalar_borrow_temporary = true;
            let finish = self.cleanup(statement, &[backing, holder], next)?;
            let observe = self.emit(
                read,
                Action::Dereference {
                    from: holder,
                    into: Some(into),
                    explicit_copy: true,
                },
                Edges::Next(finish),
            )?;
            (holder, observe)
        } else {
            // The binding is lexical, but the backing is not. Liveness at
            // this cleanup decides whether an outstanding use needs the loan.
            (into, self.cleanup(statement, &[backing], next)?)
        };
        let take = self.emit(
            borrow,
            Action::Borrow {
                from: backing,
                into: holder,
                exclusive: false,
                retained_target: false,
            },
            Edges::Next(after_borrow),
        )?;
        self.expression(producer, Some(backing), take, depth + 1)
            .map(Some)
    }

    fn ungroup_expression_at(
        &mut self,
        mut node: NodeId,
        mut depth: usize,
    ) -> ResultOf<(NodeId, usize)> {
        loop {
            self.depth(depth)?;
            let syntax = &self.module.syntax.nodes[node];
            if syntax.kind != NodeKind::Group {
                return Ok((node, depth));
            }
            node = syntax.children[0];
            depth += 1;
        }
    }

    fn temporary_projection(
        &mut self,
        node: NodeId,
        allow_field: bool,
    ) -> ResultOf<Option<ProjectedTemporary>> {
        self.budget.step(1)?;
        let facts = self.value(node)?;
        if facts.path.is_some() || facts.access != Access::Owned {
            return Ok(None);
        }
        let Some((base, projection)) = projection(facts) else {
            return Ok(None);
        };
        if !allow_field && matches!(projection, Projection::Field(_)) {
            return Ok(None);
        }
        self.budget.step(1)?;
        let ty = self.value(base)?.ty.clone();
        let scratch = self.slot(base, &ty)?;
        let member = self.member(node, scratch, projection)?;
        Ok(Some(ProjectedTemporary {
            site: node,
            base,
            scratch,
            member,
        }))
    }

    fn stage_named_transfer(
        &mut self,
        value: NodeId,
        into: Slot,
        next: Pc,
        temporary: Option<ProjectedTemporary>,
        depth: usize,
    ) -> ResultOf<Pc> {
        if temporary.is_some() {
            self.stage_value(value, into, next, temporary, depth)
        } else {
            // Do not admit these layouts into generic expressions or places.
            self.expression_with_proof(
                value,
                Some(into),
                next,
                depth,
                Some(ExpressionProof::NamedNestedTupleLeaf),
            )
        }
    }

    fn stage_value(
        &mut self,
        value: NodeId,
        into: Slot,
        next: Pc,
        temporary: Option<ProjectedTemporary>,
        depth: usize,
    ) -> ResultOf<Pc> {
        if let Some(temp) = temporary {
            let read = self.emit(
                temp.site,
                Action::Read {
                    from: temp.member,
                    into: Some(into),
                },
                Edges::Next(next),
            )?;
            self.expression(temp.base, Some(temp.scratch), read, depth)
        } else {
            self.expression(value, Some(into), next, depth)
        }
    }

    fn binding(&mut self, node: NodeId) -> ResultOf<Binding<'a, 'g>> {
        let pattern = self.module.syntax.nodes[node].children[0];
        let shape = self.ungroup(pattern)?;
        match self.module.syntax.nodes[shape].kind {
            NodeKind::Wildcard => Ok(Binding::Discard),
            NodeKind::NamePattern if shape != pattern => {
                let Some(NodeFactKind::Pattern(facts)) = self.fact(&pattern).map(|f| &f.kind)
                else {
                    return Err(Stop::Pending(pattern));
                };
                let [binder] = facts.binders.as_slice() else {
                    return Err(Stop::Pending(pattern));
                };
                if facts.access != Access::Owned
                    || !facts.coverage.exhaustive()
                    || !binder.projections.is_empty()
                {
                    return Err(Stop::Pending(pattern));
                }
                Ok(Binding::Name {
                    declaration: binder.declaration,
                    ty: &binder.ty,
                    site: shape,
                })
            }
            kind @ (NodeKind::BracketPattern | NodeKind::RecordPattern) => {
                self.budget.step(1)?;
                let Some(NodeFactKind::Pattern(facts)) = self.fact(&pattern).map(|f| &f.kind)
                else {
                    return Err(Stop::Pending(pattern));
                };
                if facts.access != Access::Owned || !facts.coverage.exhaustive() {
                    return Err(Stop::Pending(pattern));
                }
                let mut nested_reborrow = None;
                let access = match (kind, facts.ty.as_ref()) {
                    (NodeKind::BracketPattern, TypeTerm::Tuple(_))
                    | (NodeKind::RecordPattern, TypeTerm::Nominal { .. }) => Access::Owned,
                    (
                        NodeKind::BracketPattern,
                        TypeTerm::Reference {
                            mutable: false,
                            referent,
                        },
                    ) if let TypeTerm::Tuple(elements) = referent.as_ref() => {
                        self.budget.step(elements.len())?;
                        let mut nested = false;
                        let mut reference = false;
                        for element in elements {
                            match element.as_ref() {
                                TypeTerm::Primitive(_) => {}
                                TypeTerm::Tuple(inner)
                                    if self.primitive_tuple_elements(inner)? =>
                                {
                                    nested = true
                                }
                                ty if shared_primitive_reference(ty) => reference = true,
                                _ => return Err(Stop::Pending(pattern)),
                            }
                        }
                        // The carried-reference rule is still flat-only.
                        if nested && reference {
                            return Err(Stop::Pending(pattern));
                        }
                        Access::ReadOnly
                    }
                    (
                        NodeKind::RecordPattern,
                        TypeTerm::Reference {
                            mutable: false,
                            referent,
                        },
                    ) if matches!(referent.as_ref(), TypeTerm::Nominal { .. }) => Access::ReadOnly,
                    (
                        NodeKind::BracketPattern,
                        TypeTerm::Reference {
                            mutable: true,
                            referent,
                        },
                    ) if let TypeTerm::Tuple(elements) = referent.as_ref() => {
                        nested_reborrow =
                            self.check_exclusive_pattern_source(pattern, facts, elements)?;
                        Access::Writable
                    }
                    _ => return Err(Stop::Pending(pattern)),
                };
                Ok(Binding::IrrefutableProduct {
                    shape,
                    facts,
                    access,
                    nested_reborrow,
                })
            }
            _ => {
                let Some(NodeFactKind::Binding(binding)) = self.fact(&node).map(|f| &f.kind) else {
                    return Err(Stop::Pending(node));
                };
                let ty = binding.ty.as_ref().ok_or(Stop::Pending(node))?;
                Ok(Binding::Name {
                    declaration: binding.declaration,
                    ty,
                    site: pattern,
                })
            }
        }
    }

    fn exclusive_tuple_reborrow_path(
        &mut self,
        operand: NodeId,
        result: &Type<'g>,
    ) -> ResultOf<Option<&'a PlacePath<'g>>> {
        if self.module.syntax.nodes[operand].kind != NodeKind::Prefix("*") {
            return Ok(None);
        }
        let TypeTerm::Reference {
            mutable: true,
            referent,
        } = result.as_ref()
        else {
            return Ok(None);
        };
        let TypeTerm::Tuple(elements) = referent.as_ref() else {
            return Ok(None);
        };
        if !self.primitive_tuple_elements(elements)? {
            return Ok(None);
        }
        self.named_exclusive_tuple_parent(operand, result, elements.len())
    }

    fn named_exclusive_tuple_parent(
        &mut self,
        operand: NodeId,
        result: &Type<'g>,
        width: usize,
    ) -> ResultOf<Option<&'a PlacePath<'g>>> {
        let Some((input, path)) =
            self.whole_named_owned_place(self.module.syntax.nodes[operand].children[0])?
        else {
            return Ok(None);
        };
        // Compare only after the caller has proved and metered the tuple shape.
        self.budget.step(width)?;
        Ok((*result == input.ty).then_some(path))
    }

    fn whole_named_owned_place(
        &mut self,
        node: NodeId,
    ) -> ResultOf<Option<(&'a ExpressionFacts<'g>, &'a PlacePath<'g>)>> {
        let name = self.ungroup(node)?;
        self.named_owned_place(name)
    }

    fn named_owned_place(
        &mut self,
        name: NodeId,
    ) -> ResultOf<Option<(&'a ExpressionFacts<'g>, &'a PlacePath<'g>)>> {
        self.budget.step(1)?;
        if self.module.syntax.nodes[name].kind != NodeKind::ValueReference {
            return Ok(None);
        }
        self.budget.step(1)?;
        let input = self.value(name)?;
        Ok(input
            .path
            .as_ref()
            .filter(|path| {
                input.access == Access::Owned
                    && matches!(
                        path.root.kind,
                        DeclarationKind::Binding | DeclarationKind::Parameter
                    )
                    && path.projections.is_empty()
            })
            .map(|path| (input, path)))
    }

    fn check_exclusive_pattern_source(
        &mut self,
        pattern: NodeId,
        facts: &PatternFacts<'g>,
        elements: &[Type<'g>],
    ) -> ResultOf<Option<&'a PlacePath<'g>>> {
        let source = self.ungroup(facts.source.node)?;
        let syntax = &self.module.syntax.nodes[source];
        if syntax.kind != NodeKind::Prefix("&mut") {
            return Err(Stop::Pending(pattern));
        }
        let operand = self.ungroup(syntax.children[0])?;
        self.budget.step(facts.binders.len())?;
        let nested_reborrow = if self.module.syntax.nodes[operand].kind == NodeKind::Prefix("*") {
            let (width, nested) = self
                .primitive_tuple_width(elements, |_| Ok(true))?
                .ok_or(Stop::Pending(pattern))?;
            let path = self
                .named_exclusive_tuple_parent(operand, &facts.ty, width)?
                .ok_or(Stop::Pending(pattern))?;
            if nested && path.root.kind != DeclarationKind::Parameter {
                return Err(Stop::Pending(pattern));
            }
            nested.then_some(path)
        } else {
            self.budget.step(1)?;
            let input = self.value(operand)?;
            if self.module.syntax.nodes[operand].kind != NodeKind::ValueReference
                || input.access != Access::Owned
                || !matches!(&input.path, Some(path) if path.root.kind == DeclarationKind::Binding
                    && path.projections.is_empty())
                || !self.primitive_tuple_elements(elements)?
            {
                return Err(Stop::Pending(pattern));
            }
            None
        };
        let supported = match facts.binders.as_slice() {
            [binder] => matches!(
                borrowed_binding_mode(Access::Writable, binder),
                Some(BorrowedMemberBinding::CopyScalar | BorrowedMemberBinding::ExclusiveView)
            ),
            binders => binders.iter().all(|binder| {
                borrowed_binding_mode(Access::Writable, binder)
                    == Some(BorrowedMemberBinding::CopyScalar)
            }),
        };
        if !supported {
            return Err(Stop::Pending(pattern));
        }
        Ok(nested_reborrow)
    }

    fn declare_binding(
        &mut self,
        node: NodeId,
        initializer: NodeId,
        locals: &mut Vec<Slot>,
    ) -> ResultOf<()> {
        let pattern = self.module.syntax.nodes[node].children[0];
        match self.binding(node)? {
            Binding::Discard => Ok(()),
            Binding::Name {
                declaration,
                ty,
                site,
            } => {
                let nested = match self.nested_transfer_slot(initializer, site, ty)? {
                    Some(slot) => Some(slot),
                    None => self.nested_construction_slot(initializer, site, ty)?,
                };
                let local = if let Some(slot) = nested {
                    self.budget.reserve(1)?;
                    self.nested_record_values.insert(declaration, slot);
                    slot
                } else if let Some(from) = self.nested_tuple_transfer_source(initializer, ty)? {
                    let slot = self.clone_region(from, site)?;
                    self.budget.reserve(1)?;
                    self.nested_tuple_bindings.insert(declaration, (slot, from));
                    slot
                } else {
                    self.local(declaration, site, ty)?
                };
                self.budget.reserve(1)?;
                locals.push(local);
                Ok(())
            }
            Binding::IrrefutableProduct {
                shape,
                facts,
                access,
                ..
            } => {
                self.budget.step(1)?;
                let syntax = &self.module.syntax.nodes[shape];
                let mut binders = facts.binders.iter();
                self.budget.reserve(binders.len())?;
                if syntax.kind == NodeKind::RecordPattern {
                    self.declare_record_pattern(shape, access, false, &mut binders, locals)?;
                    return if binders.next().is_none() {
                        Ok(())
                    } else {
                        Err(Stop::Pending(pattern))
                    };
                }
                for (position, &child) in syntax.children.iter().enumerate() {
                    self.budget.step(1)?;
                    let name = self.ungroup(child)?;
                    let syntax = &self.module.syntax.nodes[name];
                    if syntax.kind == NodeKind::BracketPattern {
                        for (inner, &child) in syntax.children.iter().enumerate() {
                            self.budget.step(1)?;
                            let leaf = self.ungroup(child)?;
                            match self.module.syntax.nodes[leaf].kind {
                                NodeKind::Wildcard => continue,
                                NodeKind::NamePattern => {}
                                _ => return Err(Stop::Pending(leaf)),
                            }
                            let binder = binders.next().ok_or(Stop::Pending(leaf))?;
                            if !matches!(product_member(access, &binder.projections),
                                Some(ProductMember::NestedTupleElement { outer, inner: index })
                                    if outer == position && index == inner)
                            {
                                return Err(Stop::Pending(leaf));
                            }
                            self.declare_product_local(leaf, binder, locals)?;
                        }
                        continue;
                    }
                    if syntax.kind == NodeKind::Wildcard {
                        continue;
                    }
                    if syntax.kind != NodeKind::NamePattern {
                        return Err(Stop::Pending(name));
                    }
                    let binder = binders.next().ok_or(Stop::Pending(name))?;
                    if !matches!(product_member(access, &binder.projections), Some(ProductMember::TupleElement(index)) if index == position)
                    {
                        return Err(Stop::Pending(name));
                    }
                    self.declare_product_local(name, binder, locals)?;
                }
                if binders.next().is_some() {
                    return Err(Stop::Pending(pattern));
                }
                Ok(())
            }
        }
    }

    fn declare_record_pattern(
        &mut self,
        shape: NodeId,
        access: Access,
        nested: bool,
        binders: &mut std::slice::Iter<'_, PatternBinder<'g>>,
        locals: &mut Vec<Slot>,
    ) -> ResultOf<()> {
        self.budget.step(2)?;
        let syntax = &self.module.syntax.nodes[shape];
        let first =
            usize::from(syntax.children.first().is_some_and(|&child| {
                self.module.syntax.nodes[child].kind == NodeKind::PatternPath
            }));
        for &child in &syntax.children[first..] {
            self.budget.step(1)?;
            let field = &self.module.syntax.nodes[child];
            let NodeKind::PatternField { shorthand } = field.kind else {
                return Err(Stop::Pending(child));
            };
            let name = self.ungroup(field.children[usize::from(!shorthand)])?;
            self.budget.step(1)?;
            let kind = self.module.syntax.nodes[name].kind;
            // Only one nested record level: deeper constructors stay incomplete.
            if kind == NodeKind::RecordPattern && access == Access::Owned && !nested {
                self.declare_record_pattern(name, access, true, binders, locals)?;
            } else if kind != NodeKind::Wildcard {
                if !shorthand && kind != NodeKind::NamePattern {
                    return Err(Stop::Pending(name));
                }
                let binder = binders.next().ok_or(Stop::Pending(name))?;
                if !matches!(
                    (nested, product_member(access, &binder.projections)),
                    (false, Some(ProductMember::Field(_)))
                        | (true, Some(ProductMember::NestedField { .. }))
                ) {
                    return Err(Stop::Pending(name));
                }
                self.declare_product_local(name, binder, locals)?;
            }
        }
        Ok(())
    }

    fn declare_product_local(
        &mut self,
        name: NodeId,
        binder: &PatternBinder<'g>,
        locals: &mut Vec<Slot>,
    ) -> ResultOf<()> {
        self.budget.step(1)?;
        if binder.declaration.start != self.module.syntax.nodes[name].span.start() {
            return Err(Stop::Pending(name));
        }
        locals.push(self.local(binder.declaration, name, &binder.ty)?);
        Ok(())
    }

    fn shared_record_fields(
        &mut self,
        site: NodeId,
        referent: &Type<'g>,
    ) -> ResultOf<&'a [MemberType<'g>]> {
        let fields = self
            .record_fields(referent.as_ref())?
            .ok_or(Stop::Pending(site))?;
        self.budget.step(fields.len())?;
        if !fields
            .iter()
            .all(|field| shared_pattern_member_type(field.ty.as_ref()))
            || self.outer_disposer(site, referent)?.is_some()
        {
            return Err(Stop::Pending(site));
        }
        Ok(fields)
    }

    fn bind_product(
        &mut self,
        node: NodeId,
        facts: &PatternFacts<'g>,
        access: Access,
        nested_reborrow: Option<&PlacePath<'g>>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let shared_fields = if access == Access::ReadOnly
            && let TypeTerm::Reference { referent, .. } = facts.ty.as_ref()
            && matches!(referent.as_ref(), TypeTerm::Nominal { .. })
        {
            let pattern = self.module.syntax.nodes[node].children[0];
            Some(self.shared_record_fields(pattern, referent)?)
        } else {
            None
        };
        // Stage the initializer once. For borrowed patterns this is only
        // the descriptor; the aggregate storage stays with its owner.
        let source = facts.source.node;
        // Inspect the typed shape even when a nested pattern has no binders.
        let (mut nested_record, mut nested_tuple) = (false, false);
        if access == Access::Owned
            && let Pattern::Constructor(kind, children, _) = facts.pattern.as_ref()
        {
            self.budget.step(children.len())?;
            for child in children {
                match (kind, child.as_ref()) {
                    (Constructor::Record(_), Pattern::Constructor(Constructor::Record(_), ..)) => {
                        nested_record = true
                    }
                    (Constructor::Tuple(_), Pattern::Constructor(Constructor::Tuple(_), ..)) => {
                        nested_tuple = true
                    }
                    _ => {}
                }
            }
        }
        let mut tuple_literal = None;
        let tuple_source = if nested_tuple {
            let (root, source_depth) = self.ungroup_expression_at(source, depth)?;
            self.budget.step(1)?;
            if self.module.syntax.nodes[root].kind == NodeKind::Bracket {
                tuple_literal = Some((root, source_depth));
                None
            } else {
                Some(
                    self.nested_tuple_transfer_source(root, &facts.ty)?
                        .ok_or(Stop::Pending(source))?,
                )
            }
        } else {
            None
        };
        let scratch = if let Some(from) = tuple_source {
            self.clone_region(from, source)?
        } else if tuple_literal.is_some() {
            self.nested_tuple_region(source, &facts.ty)?
                .ok_or(Stop::Pending(source))?
        } else if nested_record {
            self.nested_transfer_slot(source, source, &facts.ty)?
                .ok_or(Stop::Pending(source))?
        } else {
            self.slot(source, &facts.ty)?
        };
        if self.slots[scratch].custom_disposer.is_some() || self.slots[scratch].contains_guard() {
            return Err(Stop::Pending(node));
        }
        let mut tail = if access != Access::Owned {
            self.emit(node, Action::End(scratch), Edges::Next(next))?
        } else {
            self.cleanup(node, &[scratch], next)?
        };
        for binder in facts.binders.iter().rev() {
            self.budget.step(1)?;
            let into = *self
                .locals
                .get(binder.declaration)
                .ok_or(Stop::Pending(node))?;
            let site = self.slots[into].node;
            if (nested_record || nested_tuple)
                && !matches!(binder.source_type.as_ref(), TypeTerm::Primitive(_))
            {
                return Err(Stop::Pending(site));
            }
            let member = product_member(access, &binder.projections).ok_or(Stop::Pending(site))?;
            let action = if access != Access::Owned {
                let projection = match member {
                    ProductMember::TupleElement(position) => Projection::TupleElement(position),
                    ProductMember::NestedTupleElement { outer, inner } => {
                        Projection::NestedTupleElement { outer, inner }
                    }
                    ProductMember::Field(field) => {
                        let fields = shared_fields.ok_or(Stop::Pending(site))?;
                        self.budget.step(fields.len())?;
                        let field = fields
                            .iter()
                            .find(|candidate| &candidate.declaration.id == field)
                            .ok_or(Stop::Pending(site))?;
                        Projection::Field(field.declaration)
                    }
                    ProductMember::NestedField { .. } => return Err(Stop::Pending(site)),
                };
                let binding = borrowed_binding_mode(access, binder).ok_or(Stop::Pending(site))?;
                Action::BindBorrowedMember {
                    descriptor: scratch,
                    projection,
                    into,
                    binding,
                }
            } else {
                let from = match member {
                    ProductMember::TupleElement(position) if nested_tuple => {
                        self.nested_tuple_member_slot(site, scratch, position, None)?
                    }
                    ProductMember::TupleElement(position) => {
                        self.element(site, scratch, position)?
                    }
                    ProductMember::Field(field) => self.field(site, scratch, field)?,
                    ProductMember::NestedField { outer, inner } => {
                        self.nested_field(site, scratch, outer, inner)?.1
                    }
                    ProductMember::NestedTupleElement { outer, inner } if nested_tuple => {
                        self.nested_tuple_member_slot(site, scratch, outer, Some(inner))?
                    }
                    ProductMember::NestedTupleElement { .. } => return Err(Stop::Pending(site)),
                };
                Action::Read {
                    from,
                    into: Some(into),
                }
            };
            tail = self.emit(site, action, Edges::Next(tail))?;
        }
        if let Some((literal, source_depth)) = tuple_literal {
            return self.initialize_tuple_pattern_literal(
                literal,
                scratch,
                &facts.ty,
                tail,
                source_depth,
            );
        }
        let proof = tuple_source
            .map(ExpressionProof::NestedTupleSource)
            .or_else(|| nested_reborrow.map(ExpressionProof::NestedTupleParent));
        self.expression_with_proof(source, Some(scratch), tail, depth, proof)
    }

    fn initialize_tuple_pattern_literal(
        &mut self,
        node: NodeId,
        into: Slot,
        expected: &Type<'g>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let (node, depth) = self.ungroup_expression_at(node, depth)?;
        let facts = self.normal_value(node)?;
        let (NodeKind::Bracket, Some(DataExpression::Tuple { elements }), TypeTerm::Tuple(types)) = (
            &self.module.syntax.nodes[node].kind,
            &facts.data,
            expected.as_ref(),
        ) else {
            return Err(Stop::Pending(node));
        };
        self.budget.step(self.slots[into].storage(into).len())?;
        if facts.access != Access::Owned || facts.ty != *expected || elements.len() != types.len() {
            return Err(Stop::Pending(node));
        }
        let outer = match self.slots[into].projection {
            None => None,
            Some(Projection::TupleElement(position)) => Some(position),
            _ => return Err(Stop::Pending(node)),
        };
        // Use typed positions, not flat slot offsets: each child has its own header.
        // Build the continuation backward so the written operands run before their header.
        let mut tail = self.emit(node, Action::Init(into), Edges::Next(next))?;
        for (position, (&value, ty)) in elements.iter().zip(types).enumerate().rev() {
            let target = self.nested_tuple_member_slot(
                node,
                into,
                outer.unwrap_or(position),
                outer.map(|_| position),
            )?;
            tail = match (outer, ty.as_ref()) {
                (None, TypeTerm::Tuple(_)) => {
                    self.initialize_tuple_pattern_literal(value, target, ty, tail, depth + 1)?
                }
                (_, TypeTerm::Primitive(_)) => {
                    self.expression(value, Some(target), tail, depth + 1)?
                }
                _ => return Err(Stop::Pending(value)),
            };
        }
        Ok(tail)
    }

    fn propagates_directly(&mut self, node: NodeId) -> ResultOf<bool> {
        self.budget.step(1)?;
        let syntax = &self.module.syntax.nodes[node];
        Ok(match syntax.kind {
            NodeKind::Propagate | NodeKind::Wrap => true,
            NodeKind::Pipe => {
                self.budget.step(1)?;
                self.module.syntax.nodes[syntax.children[1]].kind
                    == (NodeKind::PipeStep { propagate: true })
            }
            _ => false,
        })
    }

    fn checked_propagation(
        &mut self,
        node: NodeId,
        facts: &'a ExpressionFacts<'g>,
    ) -> ResultOf<CheckedPropagation<'a, 'g>> {
        let syntax = &self.module.syntax.nodes[node];
        let (id, result) = self.return_contract.ok_or(Stop::Pending(node))?;
        let (exit, wrap, fresh) = match &facts.data {
            Some(DataExpression::Propagation(exit)) => (exit.as_ref(), None, None),
            Some(DataExpression::Wrapping(wrap)) => (&wrap.branch, Some(&wrap.construction), None),
            Some(DataExpression::ErrorPipe {
                construction,
                propagation,
            }) if syntax.kind == NodeKind::Pipe
                && propagation.can_return
                && !propagation.can_continue =>
            {
                (propagation.as_ref(), None, Some(construction.as_ref()))
            }
            _ => return Err(Stop::Pending(node)),
        };
        if !matches!(exit.target, PropagationTarget::Callable(target) if target == id) {
            return Err(Stop::Pending(node));
        }
        let piped = syntax.kind == NodeKind::Pipe;
        let (source, operand, source_facts) = if let Some(construction) = fresh {
            (node, &construction.result, None)
        } else if piped {
            (node, self.propagating_pipe_result(node, facts)?, None)
        } else {
            let source = syntax.children[0];
            let operand = self.value(source)?;
            if operand.access != Access::Owned {
                return Err(Stop::Pending(node));
            }
            (source, &operand.ty, Some(operand))
        };
        let (wrapper, option_text) = if let Some(error) = &exit.error {
            let (role, _) = self
                .scalar_result_role(operand)
                .ok_or(Stop::Pending(node))?;
            if !matches!(self.scalar_result_role(result), Some((_, None)))
                || exit.success != &role.ok.id
                || exit.failure != &role.err.id
                || error.wrapper != &role.wrapper.id
                || !matches!(error.payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            {
                return Err(Stop::Pending(node));
            }
            (Some(error.wrapper), None)
        } else {
            let (role, payload) = self.admitted_option(operand).ok_or(Stop::Pending(node))?;
            if wrap.is_some()
                || self.bool_option_role(result).is_none()
                || exit.success != &role.some.id
                || exit.failure != &role.none.id
            {
                return Err(Stop::Pending(node));
            }
            let text = match payload {
                OptionPayload::Bool => None,
                OptionPayload::Text(ty) => {
                    let source = if piped {
                        OptionTextSource::PipeStep
                    } else if syntax.kind == NodeKind::Propagate {
                        self.check_option_text_source(
                            node,
                            source,
                            operand,
                            source_facts.ok_or(Stop::Pending(node))?,
                        )?
                    } else {
                        return Err(Stop::Pending(node));
                    };
                    Some((ty, source))
                }
            };
            (None, text)
        };
        Ok(CheckedPropagation {
            facts,
            exit,
            wrap,
            fresh,
            piped,
            source,
            operand,
            wrapper,
            option_text,
        })
    }

    fn check_option_text_source(
        &mut self,
        node: NodeId,
        source: NodeId,
        operand: &Type<'g>,
        facts: &'a ExpressionFacts<'g>,
    ) -> ResultOf<OptionTextSource> {
        self.budget.step(1)?;
        if let Some(path) = facts
            .path
            .as_ref()
            .filter(|path| path.projections.is_empty())
        {
            self.budget.step(1)?;
            return Ok(OptionTextSource::Move(self.resolve_place(source, path)?));
        }
        let root = self.ungroup(source)?;
        let origin = if root == source {
            facts
        } else {
            self.budget.step(1)?;
            self.value(root)?
        };
        if matches!(origin.data, Some(DataExpression::Construction(_))) {
            return Ok(OptionTextSource::Constructor);
        }
        self.budget.step(1)?;
        let call = origin.call.as_deref().ok_or(Stop::Pending(node))?;
        if self.module.syntax.nodes[root].kind != NodeKind::Call || call.pipe_input.is_some() {
            return Err(Stop::Pending(node));
        }
        self.check_exact_primitive_call(node, call, operand)?;
        Ok(OptionTextSource::Factory)
    }

    fn propagate(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        return_cleanup: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let facts = self.value(node)?;
        let checked = self.checked_propagation(node, facts)?;
        self.lower_checked_propagation(node, into, next, return_cleanup, depth, checked)
    }

    fn lower_checked_propagation(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        return_cleanup: Pc,
        depth: usize,
        checked: CheckedPropagation<'a, 'g>,
    ) -> ResultOf<Pc> {
        let CheckedPropagation {
            facts,
            exit,
            wrap,
            fresh,
            piped,
            source,
            operand,
            wrapper,
            option_text,
        } = checked;
        let pipe_arguments = if piped
            && fresh.is_none()
            && let Some(call) = facts.call.as_deref()
            && let Some(arguments) = self.propagating_arguments(
                node,
                PropagatingCallOutput::CheckedPipeCarrier {
                    primitive_arguments_checked: matches!(
                        option_text,
                        Some((_, OptionTextSource::PipeStep))
                    ),
                },
                call,
                Some(return_cleanup),
                depth,
            )? {
            self.budget.step(1)?;
            let flow = &self.fact(&node).ok_or(Stop::Pending(node))?.flow;
            let reaches_step = arguments.terminal_failure.is_none();
            if flow.pending
                || flow.normal != (reaches_step && exit.can_continue)
                || flow.returns != (arguments.can_return || reaches_step && exit.can_return)
            {
                return Err(Stop::Pending(node));
            }
            if !reaches_step {
                // Only argument exits are reachable; do not stage a carrier or its edges.
                return self
                    .call_with_propagating_arguments(node, call, None, next, depth, arguments);
            }
            Some((call, arguments))
        } else {
            None
        };
        let (_, result) = self.return_contract.ok_or(Stop::Pending(node))?;
        let scratch = self.slot(source, operand)?;
        let return_slot = if exit.can_return {
            let output = self.slot(node, result)?;
            let returning = self.check_retained(node, return_cleanup)?;
            let export = self.emit(node, Action::Export(output), Edges::Next(returning))?;
            Some((output, export))
        } else {
            None
        };
        let (success, failure) = if let Some(wrapper) = wrapper {
            let success = exit
                .can_continue
                .then(|| {
                    self.emit(
                        node,
                        Action::TakeResultSuccess {
                            operand: scratch,
                            into,
                        },
                        Edges::Next(next),
                    )
                })
                .transpose()?;
            let failure = return_slot
                .map(|(output, export)| {
                    if let Some(data) = wrap {
                        self.wrap_result_failure(node, scratch, data, output, export, depth)
                    } else {
                        self.emit(
                            node,
                            Action::ForwardResultFailure {
                                operand: scratch,
                                into: output,
                                variant: exit.failure,
                                wrapper,
                            },
                            Edges::Next(export),
                        )
                    }
                })
                .transpose()?;
            (success, failure)
        } else {
            let success = if !exit.can_continue {
                None
            } else if let Some((ty, _)) = option_text {
                let (into, next) = if let Some(into) = into {
                    (into, next)
                } else {
                    let payload = self.slot(node, ty)?;
                    (payload, self.cleanup(node, &[payload], next)?)
                };
                Some(self.emit(
                    node,
                    Action::TakeOptionPayload {
                        operand: scratch,
                        into,
                    },
                    Edges::Next(next),
                )?)
            } else {
                let end = self.emit(node, Action::End(scratch), Edges::Next(next))?;
                Some(if let Some(into) = into {
                    self.emit(
                        node,
                        Action::CopyOptionPayload {
                            operand: scratch,
                            into,
                        },
                        Edges::Next(end),
                    )?
                } else {
                    end
                })
            };
            let failure = if let Some((output, export)) = return_slot {
                let empty = self.emit(
                    node,
                    Action::ConstructVariant {
                        variant: exit.failure,
                        payload: None,
                        into: output,
                    },
                    Edges::Next(export),
                )?;
                let end = if option_text.is_some() {
                    Action::EndEmptyOption(scratch)
                } else {
                    Action::End(scratch)
                };
                Some(self.emit(node, end, Edges::Next(empty))?)
            } else {
                None
            };
            (success, failure)
        };
        let split = match (success, failure) {
            (Some(yes), Some(no)) => self.emit(
                node,
                Action::TestVariant {
                    operand: scratch,
                    success: exit.success,
                },
                Edges::Branch { yes, no },
            )?,
            (Some(edge), None) | (None, Some(edge)) => edge,
            (None, None) => return Err(Stop::Pending(node)),
        };
        if let Some(construction) = fresh {
            self.construct_scalar_error(node, facts, construction, scratch, split, depth)
        } else if let Some((call, arguments)) = pipe_arguments {
            self.call_with_propagating_arguments(node, call, Some(scratch), split, depth, arguments)
        } else if piped {
            self.call(node, facts, Some(scratch), split, depth, Some(operand))
        } else if let Some((_, OptionTextSource::Factory)) = option_text {
            self.expression_with_proof(
                source,
                Some(scratch),
                split,
                depth,
                Some(ExpressionProof::CallCarrier(operand)),
            )
        } else if let Some((_, OptionTextSource::Move(from))) = option_text {
            self.depth(depth)?;
            self.emit(
                source,
                Action::Move {
                    from,
                    into: Some(scratch),
                },
                Edges::Next(split),
            )
        } else {
            self.expression(source, Some(scratch), split, depth)
        }
    }

    fn contains_unadmitted_propagation(
        &mut self,
        node: NodeId,
        admitted_root: Option<NodeId>,
        depth: usize,
    ) -> ResultOf<bool> {
        self.depth(depth)?;
        if Some(node) != admitted_root && self.propagates_directly(node)? {
            return Ok(true);
        }
        self.budget.step(1)?;
        for &child in &self.module.syntax.nodes[node].children {
            if self.contains_unadmitted_propagation(child, admitted_root, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn check_skipped_argument(
        &mut self,
        enclosing: NodeId,
        mut arg: NodeId,
        root: Option<NodeId>,
        mut depth: usize,
    ) -> ResultOf<()> {
        if let Some(root) = root {
            // The caller found the root by peeling groups. Charge their depth
            // without visiting the same operand subtree twice.
            while arg != root {
                self.depth(depth)?;
                self.budget.step(1)?;
                arg = self.module.syntax.nodes[arg].children[0];
                depth += 1;
            }
            self.depth(depth)?;
            self.budget.step(2)?;
            let facts = self.value(root)?;
            let checked = self.checked_propagation(root, facts)?;
            if let Some(wrap) = checked.wrap
                && checked.exit.can_return
            {
                self.checked_wrap_failure(root, wrap)?;
            }
            if let Some(fresh) = checked.fresh {
                self.checked_scalar_error(root, facts, fresh)?;
            } else if !checked.piped {
                self.check_skipped_carrier_producer(checked.source)?;
            }
            if checked.piped && checked.fresh.is_none() {
                let call = facts.call.as_deref().ok_or(Stop::Pending(root))?;
                return self.check_skipped_pipe(
                    enclosing,
                    root,
                    call,
                    checked.option_text.is_some(),
                    depth,
                );
            }
        }
        if self.contains_unadmitted_propagation(arg, root, depth)? {
            return Err(Stop::Pending(enclosing));
        }
        Ok(())
    }

    fn check_skipped_carrier_producer(&mut self, source: NodeId) -> ResultOf<()> {
        let node = self.ungroup(source)?;
        self.budget.step(2)?;
        let facts = self.value(node)?;
        if let Some(DataExpression::ErrorConstruction(data)) = &facts.data {
            self.checked_scalar_error(node, facts, data)?;
        } else if let Some(DataExpression::Construction(data)) = &facts.data {
            self.checked_variant_payload(node, data)?;
        } else {
            // An admitted carrier type does not prove an arbitrary producer.
            // Preserve the reached place and direct-call shapes without reading flow.
            match self.module.syntax.nodes[node].kind {
                _ if projection(facts).is_some() => {}
                NodeKind::ValueReference => {}
                NodeKind::Call
                    if facts
                        .call
                        .as_deref()
                        .is_some_and(Self::direct_application_call) => {}
                _ => return Err(Stop::Pending(node)),
            }
        }
        Ok(())
    }

    fn check_skipped_pipe(
        &mut self,
        enclosing: NodeId,
        root: NodeId,
        call: &CallFacts<'g>,
        primitive_arguments_checked: bool,
        depth: usize,
    ) -> ResultOf<()> {
        // Only the input and written argument slots can contain checked roots.
        // Neither the callable nor a subtree inside an operand receives an exemption.
        self.budget.step(3)?;
        let [input, step] = self.module.syntax.nodes[root].children[..] else {
            return Err(Stop::Pending(root));
        };
        self.depth(depth + 1)?;
        let (callable, arguments) = match self.module.syntax.nodes[step].children[..] {
            [callable] => (callable, None),
            [callable, arguments] => (callable, Some(arguments)),
            _ => return Err(Stop::Pending(root)),
        };
        // Recurse only through complete roots, with the same depth meter as
        // the skipped syntax walk.
        self.check_skipped_argument(enclosing, callable, None, depth + 2)?;
        let check_operand = |this: &mut Self, arg, depth| -> ResultOf<bool> {
            let root = this.ungroup(arg)?;
            let direct = this.propagates_directly(root)?;
            this.check_skipped_argument(enclosing, arg, direct.then_some(root), depth)?;
            Ok(direct)
        };
        let mut has_propagating_argument = check_operand(self, input, depth + 1)?;
        if let Some(arguments) = arguments {
            self.depth(depth + 2)?;
            self.budget.step(1)?;
            for &arg in &self.module.syntax.nodes[arguments].children {
                has_propagating_argument |= check_operand(self, arg, depth + 3)?;
            }
        }
        if has_propagating_argument && !primitive_arguments_checked {
            self.check_primitive_pipe_arguments(root, call)?;
        }
        Ok(())
    }

    fn check_primitive_pipe_arguments(
        &mut self,
        node: NodeId,
        call: &CallFacts<'g>,
    ) -> ResultOf<()> {
        // The step signature already matched the actual arguments.
        // Composition is narrower: every argument must be primitive.
        for &arg in &call.arguments {
            self.budget.step(1)?;
            if !matches!(self.value(arg)?.ty.as_ref(), TypeTerm::Primitive(_)) {
                return Err(Stop::Pending(node));
            }
        }
        Ok(())
    }

    fn check_value_call_propagation(
        &mut self,
        node: NodeId,
        statement_discard: bool,
        returning: Option<Pc>,
        depth: usize,
    ) -> ResultOf<Option<PropagatingValueCall<'a, 'g>>> {
        self.budget.step(1)?;
        let kind = self.module.syntax.nodes[node].kind;
        if !matches!(kind, NodeKind::Call | NodeKind::Pipe) {
            return Ok(None);
        }
        self.budget.step(1)?;
        if let Some(fact) = self.fact(&node)
            && let NodeFactKind::Expression(facts) = &fact.kind
            && let Some(call) = facts.call.as_deref()
        {
            let mut output = PropagatingCallOutput::Value(&facts.ty);
            if statement_discard
                && kind == NodeKind::Call
                && let TypeTerm::Tuple(elements) = facts.ty.as_ref()
                && let [left, right] = elements.as_slice()
            {
                self.budget.step(2)?;
                if owns_text(left) && owns_text(right) {
                    output = PropagatingCallOutput::DiscardedTextPair(&facts.ty);
                }
            }
            let text_pair = matches!(output, PropagatingCallOutput::DiscardedTextPair(_));
            let Some(arguments) =
                self.propagating_arguments(node, output, call, returning, depth)?
            else {
                return Ok(None);
            };
            if fact.flow.pending
                || fact.flow.normal != arguments.terminal_failure.is_none()
                || fact.flow.returns != arguments.can_return
                || text_pair
                    && (fact.flow.diverges
                        || !fact.flow.breaks.is_empty()
                        || !fact.flow.continues.is_empty())
            {
                return Err(Stop::Pending(node));
            }
            Ok(Some(PropagatingValueCall {
                node,
                call,
                result: &facts.ty,
                discard_needs_cleanup: text_pair || owns_text(&facts.ty),
                arguments,
            }))
        } else {
            Ok(None)
        }
    }

    fn propagating_arguments(
        &mut self,
        node: NodeId,
        output: PropagatingCallOutput<'_, 'g>,
        call: &crate::analysis::scalars::CallFacts<'g>,
        returning: Option<Pc>,
        depth: usize,
    ) -> ResultOf<Option<PropagatingArguments>> {
        let pipe_formal = if let Some(input) = call.pipe_input {
            let mut position = None;
            for (i, &arg) in call.arguments.iter().enumerate() {
                self.budget.step(1)?;
                if arg == input {
                    position = Some(i);
                    break;
                }
            }
            Some(position.ok_or(Stop::Pending(node))?)
        } else {
            None
        };
        let order = || argument_staging_order(&call.arguments, pipe_formal).enumerate();
        let mut roots = Vec::new();
        let mut terminal_root_index = None;
        let mut can_return = false;
        for (position, (_, arg)) in order() {
            self.budget.step(1)?;
            let root = self.ungroup(arg)?;
            if self.propagates_directly(root)? {
                self.budget.step(1)?;
                let fact = self.fact(&root).ok_or(Stop::Pending(root))?;
                let NodeFactKind::Expression(value) = &fact.kind else {
                    return Err(Stop::Pending(root));
                };
                let exit = match &value.data {
                    Some(DataExpression::Propagation(exit)) => exit.as_ref(),
                    Some(DataExpression::Wrapping(wrap)) => &wrap.branch,
                    Some(DataExpression::ErrorPipe { propagation, .. }) => propagation.as_ref(),
                    _ => return Err(Stop::Pending(root)),
                };
                let mut composed_pipe = false;
                if terminal_root_index.is_none() {
                    self.budget.step(1)?;
                    if self.module.syntax.nodes[root].kind == NodeKind::Pipe
                        && let Some(pipe) = value.call.as_deref()
                    {
                        for &inner in &pipe.arguments {
                            self.budget.step(1)?;
                            let inner = self.ungroup(inner)?;
                            if self.propagates_directly(inner)? {
                                composed_pipe = true;
                                break;
                            }
                        }
                    }
                }
                let (continues, returns) = if composed_pipe {
                    // The last step's variant alone cannot describe a pipe
                    // that may return while staging its own arguments.
                    let flow = &fact.flow;
                    if flow.pending
                        || flow.diverges
                        || !flow.breaks.is_empty()
                        || !flow.continues.is_empty()
                    {
                        return Err(Stop::Pending(root));
                    }
                    (flow.normal, flow.returns)
                } else {
                    (exit.can_continue, exit.can_return)
                };
                if !(returns || continues) {
                    return Err(Stop::Pending(node));
                }
                if terminal_root_index.is_none() {
                    can_return |= returns;
                    if !continues {
                        terminal_root_index = Some(roots.len());
                    }
                }
                self.budget.reserve(1)?;
                roots.push(PropagatingArgument {
                    evaluation_position: position,
                    root,
                    can_return: returns,
                });
            }
        }
        let Some(first) = roots.first() else {
            return Ok(None);
        };
        let returning = returning.ok_or(Stop::Pending(first.root))?;
        match output {
            PropagatingCallOutput::Value(expected) => {
                if !(owns_text(expected)
                    || matches!(expected.as_ref(), TypeTerm::Primitive(p) if matches!(p.name(), "bool" | "i32")))
                {
                    return Err(Stop::Pending(node));
                }
                self.check_exact_primitive_call(node, call, expected)?;
            }
            PropagatingCallOutput::DiscardedTextPair(expected) => {
                let ([arg], [postfix]) = (call.arguments.as_slice(), roots.as_slice()) else {
                    return Err(Stop::Pending(node));
                };
                self.budget.step(3)?;
                let syntax = &self.module.syntax.nodes[postfix.root];
                let argument = self.fact(arg).ok_or(Stop::Pending(*arg))?;
                let NodeFactKind::Expression(value) = &argument.kind else {
                    return Err(Stop::Pending(*arg));
                };
                let flow = &argument.flow;
                if call.pipe_input.is_some()
                    || syntax.kind != NodeKind::Propagate
                    || flow.pending
                    || flow.diverges
                    || !flow.breaks.is_empty()
                    || !flow.continues.is_empty()
                    || flow.normal != terminal_root_index.is_none()
                    || flow.returns != postfix.can_return
                    || !matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
                    || self
                        .bool_option_role(&self.value(syntax.children[0])?.ty)
                        .is_none()
                    || self.contains_unadmitted_propagation(node, Some(postfix.root), depth)?
                {
                    return Err(Stop::Pending(node));
                }
                self.check_exact_primitive_call(node, call, expected)?;
                // The reached route proves default cleanup when allocating the
                // tuple slot. Prove it here if the postfix makes that impossible.
                if terminal_root_index.is_some() && self.outer_disposer(node, expected)?.is_some() {
                    return Err(Stop::Pending(node));
                }
            }
            PropagatingCallOutput::CheckedPipeCarrier {
                primitive_arguments_checked: true,
            } => {}
            PropagatingCallOutput::CheckedPipeCarrier {
                primitive_arguments_checked: false,
            } => {
                self.check_primitive_pipe_arguments(node, call)?;
            }
        }
        // A known failure skips the suffix, but only a checked complete
        // argument root may contain propagation there.
        let terminal_failure = if let Some(index) = terminal_root_index {
            self.budget.step(1)?;
            let terminal = roots[index];
            let mut skipped = roots[index + 1..].iter().peekable();
            for (position, (_, arg)) in order().skip(terminal.evaluation_position + 1) {
                self.budget.step(1)?;
                let admitted_root = skipped
                    .next_if(|argument| argument.evaluation_position == position)
                    .map(|argument| argument.root);
                self.check_skipped_argument(node, arg, admitted_root, depth + 1)?;
            }
            roots.truncate(index);
            Some(terminal)
        } else {
            None
        };
        for argument in &roots {
            self.budget.step(1)?;
            if argument.can_return {
                continue;
            }
            self.budget.step(1)?;
            let syntax = &self.module.syntax.nodes[argument.root];
            if syntax.kind == NodeKind::Wrap
                && self.contains_unadmitted_propagation(syntax.children[1], None, depth + 1)?
            {
                return Err(Stop::Pending(node));
            }
        }
        Ok(Some(PropagatingArguments {
            continuing: roots,
            terminal_failure,
            can_return,
            pipe_formal,
            returning,
        }))
    }

    fn propagating_pipe_result(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
    ) -> ResultOf<&'a Type<'g>> {
        let call = facts.call.as_deref().ok_or(Stop::Pending(node))?;
        if call.pipe_input.is_none() {
            return Err(Stop::Pending(node));
        }
        // The pipe node's expression type describes the success payload.
        // The declared result is the carrier to stage before branching.
        let (parameters, result) = self.direct_application_signature(node, call)?;
        let option = self.admitted_option(result);
        if !(option.is_some() || self.scalar_result_role(result).is_some()) {
            return Err(Stop::Pending(node));
        }
        let owns_payload = matches!(option, Some((_, OptionPayload::Text(_))));
        self.check_exact_call_arguments(node, parameters, &call.arguments, !owns_payload)?;
        Ok(result)
    }

    fn checked_wrap_failure(
        &mut self,
        node: NodeId,
        data: &ErrorConstruction<'g>,
    ) -> ResultOf<(NodeId, &'g Span, &'g DeclarationId)> {
        let role = self.typed.result_role().ok_or(Stop::Pending(node))?;
        let [left, right] = self.module.syntax.nodes[node].children[..] else {
            return Err(Stop::Pending(node));
        };
        let retained = data.retained.as_ref().ok_or(Stop::Pending(node))?;
        let (_, result) = self.return_contract.ok_or(Stop::Pending(node))?;
        self.budget.step(1)?;
        if data.operation != ErrorOperation::Wrap
            || data.result != *result
            || data.failure != &role.err.id
            || data.wrapper != &role.wrapper.id
            || data.value != right
            || retained.operand != left
            || !matches!(&*retained.wrapper, TypeTerm::Nominal { declaration, arguments }
                if *declaration == data.wrapper && matches!(arguments.as_slice(), [payload] if *payload == retained.payload))
            || !matches!(retained.payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            || !matches!(data.payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            || self.value(right)?.ty != data.payload
        {
            return Err(Stop::Pending(node));
        }
        self.budget.step(4)?;
        let bodies = self.typed.bodies();
        for goal in [&retained.no_borrow, &retained.sendable, &retained.shareable] {
            if !bodies
                .goal(goal)
                .is_ok_and(|fact| fact.state == GoalState::Proved)
            {
                return Err(Stop::Pending(node));
            }
        }
        let display = bodies
            .goal(&retained.display)
            .map_err(|_| Stop::Pending(node))?;
        let selected = display
            .selection
            .as_ref()
            .filter(|selected| {
                display.state == GoalState::Proved && selected.substitution.is_empty()
            })
            .ok_or(Stop::Pending(node))?;
        let tokens = &self.module.syntax.lexed.tokens;
        self.budget
            .step(1 + tokens.len().checked_ilog2().unwrap_or(0) as usize)?;
        let origin = &tokens
            .get(tokens.partition_point(|token| token.span.start() < data.origin.start()))
            .filter(|token| token.span == data.origin && token.span.bytes() == b"?")
            .ok_or(Stop::Pending(node))?
            .span;
        Ok((right, origin, selected.implementation))
    }

    fn wrap_result_failure(
        &mut self,
        node: NodeId,
        operand: Slot,
        data: &ErrorConstruction<'g>,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let (right, origin, cause_display) = self.checked_wrap_failure(node, data)?;
        let payload = self.slot(right, &data.payload)?;
        let source = self.ungroup(right)?;
        let temporary = self.temporary_projection(source, false)?;
        let next = if let Some(temp) = temporary {
            self.cleanup(node, &[temp.scratch], next)?
        } else {
            next
        };
        let wrap = self.emit(
            node,
            Action::WrapResultFailure {
                operand,
                payload,
                into,
                variant: data.failure,
                wrapper: data.wrapper,
                origin,
                cause_display,
            },
            Edges::Next(next),
        )?;
        self.stage_value(right, payload, wrap, temporary, depth)
    }

    fn construct_variant(
        &mut self,
        node: NodeId,
        data: &Construction<'g>,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let TypeTerm::Variant { declaration, .. } = data.ty.as_ref() else {
            return Err(Stop::Pending(node));
        };
        let payload = if let Some(value) = self.checked_variant_payload(node, data)? {
            self.budget.step(1)?;
            let ty = &self.value(value)?.ty;
            Some((value, self.slot(value, ty)?))
        } else {
            None
        };
        let construct = self.emit(
            node,
            Action::ConstructVariant {
                variant: declaration,
                payload: payload.map(|(_, slot)| slot),
                into,
            },
            Edges::Next(next),
        )?;
        if let Some((value, slot)) = payload {
            self.expression(value, Some(slot), construct, depth)
        } else {
            Ok(construct)
        }
    }

    fn checked_variant_payload(
        &self,
        node: NodeId,
        data: &Construction<'g>,
    ) -> ResultOf<Option<NodeId>> {
        let TypeTerm::Variant { declaration, .. } = data.ty.as_ref() else {
            return Err(Stop::Pending(node));
        };
        let option = self.admitted_option(&data.ty).map(|(role, _)| role);
        match data.arguments.as_slice() {
            [] if option.is_some_and(|role| *declaration == &role.none.id) => Ok(None),
            [value]
                if option.is_some_and(|role| *declaration == &role.some.id)
                    || self
                        .scalar_result_role(&data.ty)
                        .is_some_and(|(role, _)| *declaration == &role.ok.id) =>
            {
                Ok(Some(*value))
            }
            _ => Err(Stop::Pending(node)),
        }
    }

    fn checked_scalar_error(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        data: &ErrorConstruction<'g>,
    ) -> ResultOf<(NodeId, &'g DeclarationId)> {
        let (role, None) = self
            .scalar_result_role(&data.result)
            .ok_or(Stop::Pending(node))?
        else {
            return Err(Stop::Pending(node));
        };
        let syntax = &self.module.syntax.nodes[node];
        let piped = syntax.kind == NodeKind::Pipe;
        let (origin_node, origin) = match syntax.kind {
            NodeKind::Call if facts.ty == data.result => (node, &syntax.span),
            NodeKind::Pipe if matches!(facts.data, Some(DataExpression::ErrorPipe { .. })) => {
                self.budget.step(1)?;
                let step = syntax.children[1];
                (step, &self.module.syntax.nodes[step].span)
            }
            _ => return Err(Stop::Pending(node)),
        };
        let providers = role.providers.ok_or(Stop::Pending(node))?;
        if data.operation != ErrorOperation::Construct
            || data.retained.is_some()
            || data.failure != &role.err.id
            || data.wrapper != &role.wrapper.id
            || data.origin.snapshot() != origin.snapshot()
            || data.origin.start() != origin.start()
            || data.origin.end() != origin.start()
            || !matches!(data.payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            || !facts.call.as_ref().is_some_and(|call| {
                call.callee == &providers.constructor.id
                    && call.arguments.as_slice() == [data.value]
                    && (!piped || call.pipe_input == Some(data.value))
            })
        {
            return Err(Stop::Pending(node));
        }
        self.budget.step(1)?;
        if self.value(data.value)?.ty != data.payload {
            return Err(Stop::Pending(node));
        }
        Ok((origin_node, &providers.constructor.id))
    }

    fn construct_scalar_error(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        data: &ErrorConstruction<'g>,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let (origin_node, constructor) = self.checked_scalar_error(node, facts, data)?;
        let payload = self.slot(data.value, &data.payload)?;
        let construct = self.emit(
            origin_node,
            Action::ConstructResultError {
                constructor,
                variant: data.failure,
                wrapper: data.wrapper,
                payload,
                into,
            },
            Edges::Next(next),
        )?;
        self.expression(data.value, Some(payload), construct, depth)
    }

    fn discard(
        &mut self,
        statement: NodeId,
        value: NodeId,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let ty = self.value(value)?.ty.clone();
        let slot = self.slot(value, &ty)?;
        let end = if self.slots[slot].needs_boundary_cleanup() {
            self.cleanup(statement, &[slot], next)?
        } else {
            self.emit(statement, Action::End(slot), Edges::Next(next))?
        };
        self.expression(value, Some(slot), end, depth)
    }

    fn ungroup(&mut self, mut node: NodeId) -> ResultOf<NodeId> {
        while matches!(
            self.module.syntax.nodes[node].kind,
            NodeKind::Group | NodeKind::GroupPattern
        ) {
            self.budget.step(1)?;
            node = self.module.syntax.nodes[node].children[0];
        }
        Ok(node)
    }

    fn outcomes(&self, node: NodeId) -> ResultOf<(bool, bool)> {
        Ok(self
            .value(node)?
            .boolean
            .map_or((true, true), |b| (b.can_be_true(), b.can_be_false())))
    }

    fn condition(&mut self, node: NodeId, yes: Pc, no: Pc, depth: usize) -> ResultOf<Pc> {
        let edges = match self.outcomes(node)? {
            (false, true) => Edges::Next(no),
            (true, false) => Edges::Next(yes),
            (true, true) => Edges::Branch { yes, no },
            (false, false) => return Err(Stop::Pending(node)),
        };
        let branch = self.emit(node, Action::None, edges)?;
        self.expression(node, None, branch, depth + 1)
    }

    fn expression(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.expression_with_proof(node, into, next, depth, None)
    }

    fn incoming_shared_reference_field(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
    ) -> ResultOf<(Slot, Projection<'g>)> {
        let Some(DataExpression::Projection(field)) = &facts.data else {
            return Err(Stop::Pending(node));
        };
        if facts.access != Access::ReadOnly || !shared_primitive_reference(&facts.ty) {
            return Err(Stop::Pending(node));
        }
        self.budget.step(1)?;
        if self.module.syntax.nodes[field.base].kind != NodeKind::ValueReference {
            return Err(Stop::Pending(node));
        }
        let base = self.value(field.base)?;
        let TypeTerm::Reference {
            mutable: false,
            referent,
        } = base.ty.as_ref()
        else {
            return Err(Stop::Pending(node));
        };
        let path = base.path.as_ref().ok_or(Stop::Pending(node))?;
        if path.root.kind != DeclarationKind::Parameter
            || !path.projections.is_empty()
            || !facts.path.as_ref().is_some_and(|projected| {
                projected.root == path.root
                    && matches!(projected.projections.as_slice(),
                        [PathProjection::Dereference, PathProjection::Field(id)] if **id == field.field.id)
            })
        {
            return Err(Stop::Pending(node));
        }
        let fields = self.shared_record_fields(node, referent)?;
        self.budget.step(fields.len() + 1)?;
        let member = fields
            .iter()
            .find(|member| member.declaration.id == field.field.id && member.ty == facts.ty)
            .ok_or(Stop::Pending(node))?;
        let descriptor = *self.locals.get(path.root).ok_or(Stop::Pending(node))?;
        Ok((descriptor, Projection::Field(member.declaration)))
    }

    fn expression_with_proof(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
        proof: Option<ExpressionProof<'_, 'g>>,
    ) -> ResultOf<Pc> {
        self.depth(depth)?;
        let checked_carrier = match proof {
            Some(ExpressionProof::CallCarrier(carrier)) => Some(carrier),
            _ => None,
        };
        if checked_carrier.is_some() {
            self.budget.step(2)?;
        }
        let syntax = &self.module.syntax.nodes[node];
        let facts = self.normal_value(node)?;
        if let Some(DataExpression::Construction(data)) = &facts.data
            && (self.admitted_option(&data.ty).is_some()
                || self.scalar_result_role(&data.ty).is_some())
        {
            return self.construct_variant(
                node,
                data,
                into.ok_or(Stop::Pending(node))?,
                next,
                depth + 1,
            );
        }
        if let Some(DataExpression::ErrorConstruction(data)) = &facts.data {
            return self.construct_scalar_error(
                node,
                facts,
                data,
                into.ok_or(Stop::Pending(node))?,
                next,
                depth + 1,
            );
        }
        if let Some(ExpressionProof::NestedRecordFieldTemporary(temp)) = proof
            && node == temp.site
        {
            let into = into.ok_or(Stop::Pending(node))?;
            let finish = self.cleanup(node, &[temp.scratch], next)?;
            return self.stage_value(node, into, finish, Some(temp), depth + 1);
        }
        if let Some((_, projection)) = projection(facts) {
            let action = if let Some(ExpressionProof::UnloweredRebuildRead(slot)) = proof {
                Action::UnloweredRebuildRead(slot)
            } else if facts.access != Access::Owned && owns_text(&facts.ty) {
                let path = facts.path.as_ref().ok_or(Stop::Pending(node))?;
                let root = *self.locals.get(path.root).ok_or(Stop::Pending(node))?;
                Action::ForbiddenBorrowedMove(root)
            } else if matches!(projection, Projection::Field(_))
                && facts.access == Access::ReadOnly
                && shared_primitive_reference(&facts.ty)
            {
                let into = into.ok_or(Stop::Pending(node))?;
                let (descriptor, projection) = self.incoming_shared_reference_field(node, facts)?;
                Action::BindBorrowedMember {
                    descriptor,
                    projection,
                    into,
                    binding: BorrowedMemberBinding::CopyReference,
                }
            } else {
                let leaf = if matches!(proof, Some(ExpressionProof::NamedNestedTupleLeaf)) {
                    self.named_nested_tuple_leaf(node, facts)?
                } else {
                    None
                };
                Action::Read {
                    from: match leaf {
                        Some(slot) => slot,
                        None => self.read_value_place(node, facts)?,
                    },
                    into,
                }
            };
            return self.emit(node, action, Edges::Next(next));
        }
        let action = match syntax.kind {
            NodeKind::Group => {
                return self.expression_with_proof(
                    syntax.children[0],
                    into,
                    next,
                    depth + 1,
                    proof,
                );
            }
            NodeKind::Literal => into.map_or(Action::None, Action::Init),
            NodeKind::ValueReference => Action::Read {
                // Only exact named transfers and owned pattern staging supply this slot.
                from: match proof {
                    Some(ExpressionProof::NestedTupleSource(slot)) => slot,
                    _ => self.read_value_place(node, facts)?,
                },
                into,
            },
            NodeKind::Prefix("&" | "&mut") => {
                let (operand, _) = self.ungroup_expression_at(syntax.children[0], depth + 1)?;
                let into = into.ok_or(Stop::Pending(node))?;
                let exclusive = syntax.kind == NodeKind::Prefix("&mut");
                let source = &self.module.syntax.nodes[operand];
                let tuple_parent = if exclusive {
                    match proof {
                        // Only the exact pattern gate supplies a nested parent.
                        Some(ExpressionProof::NestedTupleParent(path)) => Some(path),
                        _ => self.exclusive_tuple_reborrow_path(operand, &facts.ty)?,
                    }
                } else {
                    None
                };
                if let Some(path) = tuple_parent {
                    Action::Reborrow {
                        parent: self.resolve_place(operand, path)?,
                        into,
                        exclusive: true,
                        suspends: true,
                    }
                } else if source.kind == NodeKind::Prefix("*") && primitive_reference(&facts.ty) {
                    let parent = source.children[0];
                    let suspends = matches!(
                        self.value(parent)?.ty.as_ref(),
                        TypeTerm::Reference { mutable: true, .. }
                    );
                    let parent = self.place(parent)?;
                    if self.slots[parent].contains_guard() {
                        return Err(Stop::Pending(node));
                    }
                    Action::Reborrow {
                        parent,
                        into,
                        exclusive,
                        suspends,
                    }
                } else {
                    let operand_facts = self.value(operand)?;
                    if !exclusive
                        && operand_facts.access == Access::ReadOnly
                        && shared_primitive_reference(&operand_facts.ty)
                        && matches!(operand_facts.data, Some(DataExpression::Projection(_)))
                        && matches!(facts.ty.as_ref(), TypeTerm::Reference { mutable: false, referent }
                            if *referent == operand_facts.ty)
                    {
                        let (descriptor, projection) =
                            self.incoming_shared_reference_field(operand, operand_facts)?;
                        return self.emit(
                            node,
                            Action::BindBorrowedMember {
                                descriptor,
                                projection,
                                into,
                                binding: BorrowedMemberBinding::SharedSlot,
                            },
                            Edges::Next(next),
                        );
                    }
                    if matches!(
                        self.admitted_option(&operand_facts.ty),
                        Some((_, OptionPayload::Text(_)))
                    ) {
                        return Err(Stop::Pending(node));
                    }
                    let path = operand_facts.path.as_ref().ok_or(Stop::Pending(operand))?;
                    let from = if operand_facts.access == Access::Owned
                        && matches!(operand_facts.ty.as_ref(), TypeTerm::Primitive(_))
                        && let [PathProjection::Field(outer), PathProjection::Field(inner)] =
                            path.projections.as_slice()
                        && let Some(root) = self.nested_record_root(path)?
                    {
                        if (exclusive && operand_facts.place != TypedPlace::Writable)
                            || !matches!(facts.ty.as_ref(), TypeTerm::Reference { mutable, referent }
                                if *mutable == exclusive && *referent == operand_facts.ty)
                        {
                            return Err(Stop::Pending(operand));
                        }
                        self.nested_field(operand, root, outer, inner)?.1
                    } else {
                        self.resolve_place(operand, path)?
                    };
                    if self.slots[from].contains_guard()
                        || (matches!(
                            self.slots[from].projection,
                            Some(Projection::TupleElement(_))
                        ) && (operand_facts.access != Access::Owned
                            || !matches!(operand_facts.ty.as_ref(), TypeTerm::Primitive(_))))
                    {
                        return Err(Stop::Pending(node));
                    }
                    Action::Borrow {
                        from,
                        into,
                        exclusive,
                        retained_target: retained_bool_contents_type(&facts.ty).is_some(),
                    }
                }
            }
            NodeKind::Prefix("*") if matches!(facts.ty.as_ref(), TypeTerm::Primitive(p) if p.name() != "string") =>
            {
                let from = self.place(syntax.children[0])?;
                if self.slots[from].guard_release().is_some() {
                    return Err(Stop::Pending(node));
                }
                Action::Dereference {
                    from,
                    into,
                    explicit_copy: self.scalar_copy_read == Some(node),
                }
            }
            NodeKind::Prefix("*") => {
                let Some(path) = &facts.path else {
                    return Err(Stop::Pending(node));
                };
                let descriptor = self.bool_deref_descriptor_slot(node, path)?;
                Action::CopyThrough { descriptor, into }
            }
            NodeKind::Bracket => {
                let into = into.ok_or(Stop::Pending(node))?;
                let Some(DataExpression::Tuple { elements }) = &facts.data else {
                    return Err(Stop::Pending(node));
                };
                if self.slots[into].members != elements.len() {
                    return Err(Stop::Pending(node));
                }
                let initializers = elements
                    .iter()
                    .enumerate()
                    .map(|(i, &value)| (Projection::TupleElement(i), value));
                return self.initialize_members(node, into, next, depth + 1, initializers, false);
            }
            NodeKind::Construction => {
                let into = into.ok_or(Stop::Pending(node))?;
                let Some(DataExpression::Construction(data)) = facts.data.as_ref() else {
                    return Err(Stop::Pending(node));
                };
                if !data.arguments.is_empty() {
                    return Err(Stop::Pending(node));
                }
                if let Some(base) = data.base {
                    if !matches!(proof, Some(ExpressionProof::NamedRecordRebuild)) {
                        return Err(Stop::Pending(node));
                    }
                    return self.rebuild_record(node, data, base, into, next, depth + 1);
                }
                let nested = matches!(proof, Some(ExpressionProof::NestedRecordConstruction));
                if nested {
                    self.check_nested_record_fields(node, data)?;
                } else if self.slots[into].members != data.fields.len() {
                    return Err(Stop::Pending(node));
                }
                let initializers = data
                    .fields
                    .iter()
                    .map(|field| (Projection::Field(field.field), field.expression));
                return self.initialize_members(node, into, next, depth + 1, initializers, nested);
            }
            NodeKind::Arrow => {
                return self.closure(node, into.ok_or(Stop::Pending(node))?, next, depth + 1);
            }
            NodeKind::Call => {
                return self.call(node, facts, into, next, depth + 1, checked_carrier);
            }
            NodeKind::Pipe
                if self.module.syntax.nodes[syntax.children[1]].kind
                    == (NodeKind::PipeStep { propagate: false })
                    && facts
                        .call
                        .as_ref()
                        .is_some_and(|call| call.pipe_input.is_some()) =>
            {
                return self.call(node, facts, into, next, depth + 1, None);
            }
            _ => return Err(Stop::Pending(node)),
        };
        self.emit(node, action, Edges::Next(next))
    }

    fn rebuild_record(
        &mut self,
        node: NodeId,
        data: &Construction<'g>,
        base: NodeId,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let fields = self.record_fields(&data.ty)?.ok_or(Stop::Pending(node))?;
        let (input, path) = self
            .whole_named_owned_place(base)?
            .ok_or(Stop::Pending(node))?;
        let from = self.resolve_place(base, path)?;
        self.budget.step(3 + fields.len())?;
        if input.ty != data.ty
            || !self.slots[from].moves
            || self.slots[from].members != fields.len()
            || self.slots[into].members != fields.len()
            || !fields
                .iter()
                .all(|field| matches!(field.ty.as_ref(), TypeTerm::Primitive(_)))
        {
            return Err(Stop::Pending(node));
        }

        struct Replacement {
            value: NodeId,
            target: Slot,
            incoming: Slot,
            unlowered_read: Option<Slot>,
        }
        self.budget.reserve(data.fields.len())?;
        let mut replacements = Vec::with_capacity(data.fields.len());
        let first_stage = self.slots.len();
        let mut observable = false;
        for field in &data.fields {
            let value = self.ungroup(field.expression)?;
            self.budget.step(3)?;
            let facts = self.value(value)?;
            if !matches!(facts.ty.as_ref(), TypeTerm::Primitive(_)) || facts.access != Access::Owned
            {
                return Err(Stop::Pending(value));
            }
            let unlowered_read = match self.module.syntax.nodes[value].kind {
                NodeKind::Literal | NodeKind::ValueReference => None,
                NodeKind::Field => {
                    let Some(source) = &facts.path else {
                        return Err(Stop::Pending(value));
                    };
                    let [PathProjection::Field(member)] = source.projections.as_slice() else {
                        return Err(Stop::Pending(value));
                    };
                    if source.root != path.root {
                        return Err(Stop::Pending(value));
                    }
                    // Check the read before blocking an operand that would open
                    // a hole in the base. Do not actually move the original field.
                    owns_text(&facts.ty)
                        .then(|| self.field(value, from, member))
                        .transpose()?
                }
                _ => return Err(Stop::Pending(value)),
            };
            let target = self.field(node, from, &field.field.id)?;
            // The scoped witness does not settle ordering several observable disposals.
            if self.slots[target].moves && std::mem::replace(&mut observable, true) {
                return Err(Stop::Pending(node));
            }
            let incoming = self.slot(field.expression, &facts.ty)?;
            replacements.push(Replacement {
                value: field.expression,
                target,
                incoming,
                unlowered_read,
            });
        }

        // Commit on the original place, then transfer the completed owner.
        let mut tail = self.emit(
            node,
            Action::Move {
                from,
                into: Some(into),
            },
            Edges::Next(next),
        )?;
        for &Replacement {
            target, incoming, ..
        } in replacements.iter().rev()
        {
            tail = self.emit(
                node,
                Action::Install { target, incoming },
                Edges::Next(tail),
            )?;
            tail = self.emit(
                node,
                Action::Displace { target, incoming },
                Edges::Next(tail),
            )?;
        }
        tail = self.emit(
            node,
            Action::CheckRebuild {
                base: from,
                staged: Some((first_stage, replacements.len())),
            },
            Edges::Next(tail),
        )?;
        for replacement in replacements.into_iter().rev() {
            let proof = replacement
                .unlowered_read
                .map(ExpressionProof::UnloweredRebuildRead);
            tail = self.expression_with_proof(
                replacement.value,
                Some(replacement.incoming),
                tail,
                depth,
                proof,
            )?;
        }
        self.emit(
            base,
            Action::CheckRebuild {
                base: from,
                staged: None,
            },
            Edges::Next(tail),
        )
    }

    fn check_nested_record_fields(
        &mut self,
        node: NodeId,
        data: &Construction<'g>,
    ) -> ResultOf<()> {
        let fields = self.record_fields(&data.ty)?.ok_or(Stop::Pending(node))?;
        if data.fields.len() != fields.len() {
            return Err(Stop::Pending(node));
        }
        for written in &data.fields {
            self.budget.step(1 + fields.len())?;
            let member = fields
                .iter()
                .find(|member| member.declaration.id == written.field.id)
                .ok_or(Stop::Pending(written.expression))?;
            if matches!(member.ty.as_ref(), TypeTerm::Primitive(_)) {
                continue;
            }
            // The flat child moves or copies as one region from a named owner.
            let (source, _) = self
                .whole_named_owned_place(written.expression)?
                .ok_or(Stop::Pending(written.expression))?;
            self.budget.step(1)?;
            if source.ty != member.ty {
                return Err(Stop::Pending(written.expression));
            }
        }
        Ok(())
    }

    fn initialize_propagating_record(
        &mut self,
        value: NodeId,
        node: NodeId,
        into: Slot,
        next: Pc,
        returning: Option<Pc>,
        depth: usize,
    ) -> ResultOf<Option<Pc>> {
        self.budget.step(1)?;
        let Some(DataExpression::Construction(data)) = &self.value(node)?.data else {
            return Ok(None);
        };
        if data.base.is_some() || !data.arguments.is_empty() {
            return Ok(None);
        }
        let mut postfix = None;
        for (position, field) in data.fields.iter().enumerate() {
            self.budget.step(1)?;
            let root = self.ungroup(field.expression)?;
            if self.module.syntax.nodes[root].kind == NodeKind::Propagate
                && postfix.replace((position, root)).is_some()
            {
                return Err(Stop::Pending(node));
            }
        }
        let Some((position, postfix)) = postfix else {
            return Ok(None);
        };
        let returning = returning.ok_or(Stop::Pending(postfix))?;
        let fields = self.record_fields(&data.ty)?.ok_or(Stop::Pending(node))?;
        self.budget.step(fields.len())?;
        if self.slots[into].members != fields.len()
            || fields.len() != data.fields.len()
            || !fields
                .iter()
                .all(|field| matches!(field.ty.as_ref(), TypeTerm::Primitive(_)))
            || self.contains_unadmitted_propagation(value, Some(postfix), depth)?
        {
            return Err(Stop::Pending(node));
        }
        // Read the postfix syntax and facts, its operand facts, and constructor flow.
        self.budget.step(4)?;
        let checked = self.checked_propagation(postfix, self.value(postfix)?)?;
        let flow = &self.fact(&node).ok_or(Stop::Pending(node))?.flow;
        if self.bool_option_role(checked.operand).is_none()
            || flow.pending
            || flow.normal != checked.exit.can_continue
            || flow.returns != checked.exit.can_return
            || !flow.breaks.is_empty()
            || !flow.continues.is_empty()
        {
            return Err(Stop::Pending(node));
        }
        let mut failure = returning;
        if checked.exit.can_return {
            // The record header is still vacant. End just the reached fields,
            // in reverse written order, before the regular lexical cleanup.
            for field in &data.fields[..position] {
                let slot = self.field(node, into, &field.field.id)?;
                failure = self.emit(postfix, Action::End(slot), Edges::Next(failure))?;
            }
        }
        let (reached, mut tail) = if checked.exit.can_continue {
            (
                data.fields.len(),
                self.emit(node, Action::Init(into), Edges::Next(next))?,
            )
        } else {
            (position + 1, next)
        };
        for (index, field) in data.fields[..reached].iter().enumerate().rev() {
            self.budget.step(1)?;
            if index == position {
                let target = checked
                    .exit
                    .can_continue
                    .then(|| self.field(node, into, &field.field.id))
                    .transpose()?;
                self.depth(depth + 1)?;
                tail = self.lower_checked_propagation(
                    postfix,
                    target,
                    tail,
                    failure,
                    depth + 1,
                    checked,
                )?;
            } else {
                let target = self.field(node, into, &field.field.id)?;
                tail = self.expression(field.expression, Some(target), tail, depth + 1)?;
            }
        }
        Ok(Some(tail))
    }

    fn initialize_members(
        &mut self,
        node: NodeId,
        into: Slot,
        next: Pc,
        depth: usize,
        initializers: impl DoubleEndedIterator<Item = (Projection<'g>, NodeId)>,
        nested: bool,
    ) -> ResultOf<Pc> {
        let mut tail = self.emit(node, Action::Init(into), Edges::Next(next))?;
        for (projection, value) in initializers.rev() {
            let target = self.member(node, into, projection)?;
            let proof = if nested {
                let initializer = self.ungroup(value)?;
                self.temporary_projection(initializer, false)?
                    .map(ExpressionProof::NestedRecordFieldTemporary)
            } else {
                None
            };
            tail = self.expression_with_proof(value, Some(target), tail, depth, proof)?;
        }
        Ok(tail)
    }

    fn closure(&mut self, node: NodeId, into: Slot, next: Pc, depth: usize) -> ResultOf<Pc> {
        self.budget.step(1)?;
        let arrow = self
            .fact(&node)
            .and_then(|f| f.callable.as_ref()?.arrow.as_deref())
            .ok_or(Stop::Pending(node))?;
        let body = self.module.syntax.nodes[node].children[1];
        let mode = arrow.mode.ok_or(Stop::Pending(node))?;
        if arrow.premise.is_some() || !arrow.parameters.is_empty() {
            return Err(Stop::Pending(node));
        }
        if mode == InvocationMode::Mutable
            && matches!(arrow.result.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
        {
            return self.mutable_i32_cell(node, body, arrow, into, next, depth);
        }
        // Admit only direct stable sources, either the entire body or a
        // written pair. A tuple of these sources adds no body computation
        // to run at formation.
        let (first, second) = match arrow.captures.as_slice() {
            [capture] if self.ungroup(capture.site.node)? == self.ungroup(body)? => (capture, None),
            [first, second] => {
                self.budget.step(1)?;
                let Some(DataExpression::Tuple { elements }) = &self.value(body)?.data else {
                    return Err(Stop::Pending(node));
                };
                let [left, right] = elements.as_slice() else {
                    return Err(Stop::Pending(node));
                };
                let written = [self.ungroup(*left)?, self.ungroup(*right)?];
                let a = self.ungroup(first.site.node)?;
                let b = self.ungroup(second.site.node)?;
                if written == [a, b] {
                    (first, Some(second))
                } else if written == [b, a] {
                    (second, Some(first))
                } else {
                    return Err(Stop::Pending(node));
                }
            }
            _ => return Err(Stop::Pending(node)),
        };
        let capture = self.capture_slot(first)?;
        let second = second
            .map(|fact| {
                let slot = self.capture_slot(fact)?;
                // Repeated copies and shared borrows are non-consuming. A
                // repeated move needs the full capture/use judgment.
                if first.path.root == fact.path.root
                    && (self.slots[capture].moves || self.slots[slot].moves)
                {
                    return Err(Stop::Pending(node));
                }
                Ok((fact.site.node, slot))
            })
            .transpose()?;
        let form = self.emit(
            node,
            Action::FormClosure {
                capture,
                second: second.map(|(_, slot)| slot),
                into,
                mode,
                cell_addend: None,
            },
            Edges::Next(next),
        )?;
        let tail = if let Some((site, slot)) = second {
            self.expression(site, Some(slot), form, depth + 1)?
        } else {
            form
        };
        self.expression(first.site.node, Some(capture), tail, depth + 1)
    }

    fn mutable_i32_cell(
        &mut self,
        node: NodeId,
        body: NodeId,
        arrow: &ArrowFacts<'g>,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        // The update and returned value are two source occurrences of one
        // captured binding. Only the initial scalar copy runs at formation.
        self.depth(depth)?;
        self.budget.step(8)?;
        let pending = Stop::Pending(node);
        let syntax = &self.module.syntax.nodes;
        let (NodeKind::Block, [update, returning]) =
            (syntax[body].kind, syntax[body].children.as_slice())
        else {
            return Err(pending);
        };
        self.depth(depth + 1)?;
        let (NodeKind::Assignment("+="), [write, operand]) =
            (syntax[*update].kind, syntax[*update].children.as_slice())
        else {
            return Err(pending);
        };
        let (NodeKind::Return, [read]) = (
            syntax[*returning].kind,
            syntax[*returning].children.as_slice(),
        ) else {
            return Err(pending);
        };
        if !std::ptr::eq(arrow.site.module, &self.module.key)
            || arrow.site.node != node
            || !matches!(self.fact(returning).map(|f| &f.kind),
                Some(NodeFactKind::ArrowReturn { target }) if *target == node)
        {
            return Err(pending);
        }
        let (write, write_depth) = self.ungroup_expression_at(*write, depth + 2)?;
        let (read, _) = self.ungroup_expression_at(*read, depth + 2)?;
        let (operand, _) = self.ungroup_expression_at(*operand, depth + 2)?;
        let [first, second] = arrow.captures.as_slice() else {
            return Err(pending);
        };
        let capture = if [first.site.node, second.site.node] == [write, read] {
            first
        } else if [second.site.node, first.site.node] == [write, read] {
            second
        } else {
            return Err(pending);
        };
        if capture.path.root.kind != DeclarationKind::Binding {
            return Err(pending);
        }
        for use_ in [first, second] {
            let value = self.normal_value(use_.site.node)?;
            if !std::ptr::eq(use_.site.module, &self.module.key)
                || !use_.reachable
                || use_.through.is_some()
                || use_.form != CaptureForm::Value
                || use_.access != Access::Owned
                || use_.ty != arrow.result
                || use_.storage.as_ref() != Some(&arrow.result)
                || !std::ptr::eq(use_.path.root, capture.path.root)
                || !use_.path.projections.is_empty()
                || syntax[use_.site.node].kind != NodeKind::ValueReference
                || !value
                    .binding
                    .is_some_and(|id| std::ptr::eq(id, capture.path.root))
                || value.ty != arrow.result
                || (use_.site.node == write && value.place != TypedPlace::Writable)
            {
                return Err(pending);
            }
        }
        let literal = self.normal_value(operand)?;
        let Some(LiteralValue::Integer {
            magnitude,
            negative: false,
        }) = literal.literal.map(|e| e.value)
        else {
            return Err(pending);
        };
        if syntax[operand].kind != NodeKind::Literal || literal.ty != arrow.result {
            return Err(pending);
        }
        let addend = i32::try_from(magnitude).map_err(|_| pending)?;
        let stage = self.capture_slot(capture)?;
        let form = self.emit(
            node,
            Action::FormClosure {
                capture: stage,
                second: None,
                into,
                mode: InvocationMode::Mutable,
                cell_addend: Some(addend),
            },
            Edges::Next(next),
        )?;
        self.expression(write, Some(stage), form, write_depth)
    }

    fn capture_slot(&mut self, capture: &CaptureFact<'g>) -> ResultOf<Slot> {
        let node = capture.site.node;
        let shape = self.ungroup(node)?;
        self.budget.step(1)?;
        let syntax = &self.module.syntax.nodes[shape];
        if !capture.reachable
            || capture.through.is_some()
            || !capture.path.projections.is_empty()
            || !matches!(
                (capture.form, syntax.kind),
                (
                    CaptureForm::ExplicitBorrow { mutable: false },
                    NodeKind::Prefix("&")
                ) | (
                    CaptureForm::Value | CaptureForm::CarriedReference { mutable: false },
                    NodeKind::ValueReference
                )
            )
            || !(matches!(capture.ty.as_ref(), TypeTerm::Primitive(_))
                || primitive_reference(&capture.ty))
        {
            return Err(Stop::Pending(node));
        }
        self.slot(node, &capture.ty)
    }

    fn retained_call_contract(
        &mut self,
        node: NodeId,
        call: &crate::analysis::scalars::CallFacts<'g>,
        position: usize,
        result: &Type<'g>,
        value_use: bool,
    ) -> ResultOf<RetainedBoolContract<'a, 'g>> {
        // The declaration determines the possible inputs. Do not use the
        // callee body to narrow them.
        let (formals, declared_result) = self.direct_application_signature(node, call)?;
        if declared_result != result {
            return Err(Stop::Pending(node));
        }
        let contract = if value_use
            && call.pipe_input.is_none()
            && matches!(declared_result.as_ref(), TypeTerm::Nominal { .. })
            && self.bool_option_role(declared_result).is_some()
        {
            // This value-use slice admits the full canonical carrier, not
            // individual variants, other payloads or extra reference inputs.
            let ([destination, seed, gate], 0) = (formals, position) else {
                return Err(Stop::Pending(node));
            };
            self.budget.step(3)?;
            let contents =
                retained_bool_contents_type(&destination.ty).ok_or(Stop::Pending(node))?;
            if seed.ty != *contents || gate.ty != *declared_result {
                return Err(Stop::Pending(node));
            }
            RetainedBoolContract {
                contents,
                extra: Some(RetainedCallExtra::OptionBool),
                exclusive_source: false,
            }
        } else {
            retained_bool_contract(formals, declared_result, position, self.budget)?
                .ok_or(Stop::Pending(node))?
        };
        for (formal, &actual) in formals.iter().zip(&call.arguments) {
            self.budget.step(1)?;
            if self.value(actual)?.ty != formal.ty {
                return Err(Stop::Pending(node));
            }
        }
        Ok(contract)
    }

    fn two_hop_call_route(
        &mut self,
        node: NodeId,
        call: &CallFacts<'g>,
        result: &Type<'g>,
        depth: usize,
    ) -> ResultOf<OriginalTwoHopBool> {
        let [destination, input] = call.arguments.as_slice() else {
            return Err(Stop::Pending(node));
        };
        self.budget.step(1)?;
        if self.module.syntax.nodes[node].kind != NodeKind::Call
            || call.pipe_input.is_some()
            || !call.defaults.is_empty()
            || call.selection.is_some()
            || !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
        {
            return Err(Stop::Pending(node));
        }
        self.budget.step(1)?;
        let header = self
            .typed
            .inventory()
            .type_facts()
            .declarations
            .get(call.callee)
            .ok_or(Stop::Pending(node))?;
        let (formal, contents) =
            retained_two_hop_bool_contract(header).ok_or(Stop::Pending(node))?;
        if self.normal_value(*input)?.ty != *contents {
            return Err(Stop::Pending(*input));
        }
        let (destination, _) = self.ungroup_expression_at(*destination, depth)?;
        let (root, ty) = self.immutable_named_value(destination)?;
        if *ty != formal.ty {
            return Err(Stop::Pending(destination));
        }
        self.original_two_hop_parameter(destination, root)
    }

    fn call_destination(
        &mut self,
        call: &crate::analysis::scalars::CallFacts<'g>,
        position: usize,
        args: &[Slot],
        contract: RetainedBoolContract<'a, 'g>,
        depth: usize,
    ) -> ResultOf<CallDestination> {
        self.budget.step(args.len())?;
        let independent = contract.extra.map(|extra| match extra {
            RetainedCallExtra::Bool { position } => position,
            RetainedCallExtra::OptionBool => 2,
        });
        let mut sources = (0..args.len())
            .filter(|&i| i != position && independent != Some(i))
            .map(|i| args[i]);
        let input = sources.next().unwrap();
        let second_input = sources.next();
        let destination = |storage| CallDestination {
            storage,
            argument: args[position],
            input,
            second_input,
            extra: contract.extra,
        };
        let option_call = matches!(contract.extra, Some(RetainedCallExtra::OptionBool));
        let (argument, _) = self.ungroup_expression_at(call.arguments[position], depth)?;
        let syntax = &self.module.syntax.nodes[argument];
        if syntax.kind == NodeKind::ValueReference {
            self.budget.step(1)?;
            let Some(path) = &self.value(argument)?.path else {
                return Err(Stop::Pending(argument));
            };
            if !path.projections.is_empty() {
                return Err(Stop::Pending(argument));
            }
            self.budget.step(2)?;
            let descriptor = *self.locals.get(path.root).ok_or(Stop::Pending(argument))?;
            let storage =
                if let Some(&RetainedContents::OneHop(contents)) = self.retained.get(&descriptor) {
                    CallStorage::ThroughParameter {
                        descriptor,
                        contents,
                    }
                } else if !option_call && path.root.kind == DeclarationKind::Binding {
                    CallStorage::ThroughAlias { descriptor }
                } else {
                    return Err(Stop::Pending(argument));
                };
            return Ok(destination(storage));
        }
        // The Option case only forwards the original incoming descriptor.
        if option_call || syntax.kind != NodeKind::Prefix("&mut") {
            return Err(Stop::Pending(argument));
        }
        let target = syntax.children[0];
        self.budget.step(1)?;
        let facts = self.value(target)?;
        let Some(path) = &facts.path else {
            return Err(Stop::Pending(target));
        };
        if facts.access != Access::Owned
            || facts.ty != *contract.contents
            || path.root.kind != DeclarationKind::Binding
        {
            return Err(Stop::Pending(target));
        }
        if !matches!(path.projections.as_slice(), [] | [PathProjection::Field(_)]) {
            return Err(Stop::Pending(target));
        }
        let place = self.resolve_place(target, path)?;
        Ok(destination(CallStorage::Local(place)))
    }

    fn invoke_closure_pair(
        &mut self,
        node: NodeId,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.budget.step(2)?;
        let invocation = self
            .fact(&node)
            .and_then(|fact| fact.callable.as_ref()?.invocation.as_deref())
            .ok_or(Stop::Pending(node))?;
        let syntax = &self.module.syntax.nodes[node];
        let path = invocation.path.as_ref().ok_or(Stop::Pending(node))?;
        if syntax.kind != NodeKind::Call
            || syntax.children.as_slice() != [invocation.receiver]
            || !matches!(
                invocation.mode,
                InvocationMode::Once | InvocationMode::ReadOnly
            )
            || invocation.access != Access::Owned
            || invocation.assumption.is_some()
            || path.root.kind != DeclarationKind::Binding
            || !path.projections.is_empty()
        {
            return Err(Stop::Pending(node));
        }
        let receiver = if invocation.mode == InvocationMode::ReadOnly {
            self.ungroup_expression_at(invocation.receiver, depth + 1)?
                .0
        } else {
            self.ungroup(invocation.receiver)?
        };
        if self.module.syntax.nodes[receiver].kind != NodeKind::ValueReference {
            return Err(Stop::Pending(node));
        }
        let TypeTerm::Callable {
            mode,
            parameters,
            result,
        } = self.value(receiver)?.ty.as_ref()
        else {
            return Err(Stop::Pending(node));
        };
        let TypeTerm::Tuple(elements) = result.as_ref() else {
            return Err(Stop::Pending(node));
        };
        if *mode != invocation.mode
            || !parameters.is_empty()
            || elements.len() != 2
            || self.slots[into].members != 2
        {
            return Err(Stop::Pending(node));
        }
        let call = if *mode == InvocationMode::ReadOnly {
            self.budget.step(3)?;
            if self.slots[into].moves
                || !elements.iter().all(|ty| {
                    matches!(ty.as_ref(), TypeTerm::Primitive(_)) && !owns_text(ty)
                        || shared_primitive_reference(ty)
                })
            {
                return Err(Stop::Pending(node));
            }
            // A replaceable binding need not have a single typed origin. Flow
            // must still identify a checked read-only formation on each path.
            let arrow = self
                .fact(&invocation.receiver)
                .and_then(|fact| fact.callable.as_ref()?.arrow.as_deref());
            if arrow.is_some_and(|arrow| {
                !std::ptr::eq(arrow.site.module, &self.module.key)
                    || arrow.mode != Some(InvocationMode::ReadOnly)
                    || arrow.result != *result
            }) {
                return Err(Stop::Pending(node));
            }
            PairCall::ReadOnly {
                typed_origin: arrow.map(|arrow| arrow.site.node),
            }
        } else {
            PairCall::Once
        };
        let receiver = self.resolve_place(receiver, path)?;
        self.emit(
            node,
            Action::InvokeClosurePair {
                receiver,
                into,
                call,
            },
            Edges::Next(next),
        )
    }

    fn lower_propagating_argument(
        &mut self,
        argument: PropagatingArgument,
        into: Option<Slot>,
        next: Pc,
        failure_cleanup: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        // The proof already read this root; lowering reads its syntax and facts again.
        self.budget.step(2)?;
        self.propagate(argument.root, into, next, failure_cleanup, depth + 1)
    }

    fn stage_propagating_argument(
        &mut self,
        exits: &mut Vec<(PropagatingArgument, Pc)>,
        position: usize,
        slot: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Option<Pc>> {
        if exits.is_empty() {
            return Ok(None);
        }
        self.budget.step(1)?;
        let Some((argument, failure_cleanup)) =
            exits.pop_if(|(argument, _)| argument.evaluation_position == position)
        else {
            return Ok(None);
        };
        self.lower_propagating_argument(argument, Some(slot), next, failure_cleanup, depth)
            .map(Some)
    }

    fn lower_propagating_value_call(
        &mut self,
        statement: NodeId,
        checked: PropagatingValueCall<'a, 'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let PropagatingValueCall {
            node,
            call,
            result,
            discard_needs_cleanup,
            arguments,
        } = checked;
        // Only a reached call can produce a discarded owner. Its cleanup
        // belongs to the call's normal continuation, not any argument exit.
        let (into, next) =
            if into.is_none() && arguments.terminal_failure.is_none() && discard_needs_cleanup {
                let slot = self.slot(node, result)?;
                (Some(slot), self.cleanup(statement, &[slot], next)?)
            } else {
                (into, next)
            };
        self.call_with_propagating_arguments(node, call, into, next, depth, arguments)
    }

    fn call_with_propagating_arguments(
        &mut self,
        node: NodeId,
        call: &crate::analysis::scalars::CallFacts<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
        arguments: PropagatingArguments,
    ) -> ResultOf<Pc> {
        if Some(call.callee) == self.dispose
            || into.is_some_and(|slot| self.slots[slot].contains_guard())
        {
            return Err(Stop::Pending(node));
        }
        let PropagatingArguments {
            continuing,
            terminal_failure,
            pipe_formal,
            returning,
            ..
        } = arguments;
        let reached = terminal_failure.map_or(call.arguments.len(), |arg| arg.evaluation_position);
        let order = argument_staging_order(&call.arguments, pipe_formal).take(reached);
        self.budget
            .reserve(reached + usize::from(terminal_failure.is_none()))?;
        let mut args = Vec::with_capacity(reached);
        for (_, arg) in order.clone() {
            self.budget.step(1)?;
            let ty = &self.value(arg)?.ty;
            args.push(self.slot(arg, ty)?);
        }
        self.budget.reserve(continuing.len())?;
        let mut argument_exits = Vec::with_capacity(continuing.len());
        for argument in continuing {
            self.budget.step(1)?;
            // These slots are still in evaluation order, not formal order.
            let failure_cleanup = if argument.can_return {
                self.cleanup(
                    argument.root,
                    &args[..argument.evaluation_position],
                    returning,
                )?
            } else {
                returning
            };
            argument_exits.push((argument, failure_cleanup));
        }
        let (call_index, mut entry) = if let Some(argument) = terminal_failure {
            let failure_cleanup = self.cleanup(argument.root, &args, returning)?;
            (
                None,
                self.lower_propagating_argument(argument, None, next, failure_cleanup, depth)?,
            )
        } else {
            if let Some(pipe) = pipe_formal {
                // Restore formal order after using evaluation order for cleanup.
                self.budget.step(pipe + 1)?;
                args[..=pipe].rotate_left(1);
            }
            let index = self.calls.len();
            self.calls.push(Call {
                callee: call.callee,
                args: std::mem::take(&mut args),
                into,
                result: CallResult::Plain,
                destination: None,
            });
            (
                Some(index),
                self.emit(node, Action::Call(index), Edges::Next(next))?,
            )
        };
        for (position, (formal, arg)) in order.enumerate().rev() {
            self.budget.step(1)?;
            let slot = match call_index {
                Some(index) => self.calls[index].args[formal],
                None => args[position],
            };
            entry = if let Some(early) =
                self.stage_propagating_argument(&mut argument_exits, position, slot, entry, depth)?
            {
                early
            } else {
                self.expression(arg, Some(slot), entry, depth + 1)?
            };
        }
        Ok(entry)
    }

    fn borrows_record_result(
        &mut self,
        node: NodeId,
        call: &CallFacts<'g>,
        result: &Type<'g>,
        depth: usize,
    ) -> ResultOf<bool> {
        let (None, &[arg]) = (call.pipe_input, call.arguments.as_slice()) else {
            return Ok(false);
        };
        let TypeTerm::Reference {
            mutable: false,
            referent,
        } = result.as_ref()
        else {
            return Ok(false);
        };
        if !matches!(referent.as_ref(), TypeTerm::Primitive(_))
            && !shared_primitive_reference(referent)
        {
            return Ok(false);
        }
        self.budget.step(1)?;
        let actual = self.value(arg)?;
        let TypeTerm::Reference {
            mutable: false,
            referent: record,
        } = actual.ty.as_ref()
        else {
            return Ok(false);
        };
        if !matches!(record.as_ref(), TypeTerm::Nominal { .. }) {
            return Ok(false);
        }
        let leaf = match referent.as_ref() {
            TypeTerm::Reference {
                mutable: false,
                referent: leaf,
            } => leaf.as_ref(),
            ty => ty,
        };
        if !matches!(leaf, TypeTerm::Primitive(p) if p.name() == "bool")
            || !call.defaults.is_empty()
            || call.selection.is_some()
        {
            return Err(Stop::Pending(node));
        }
        let (formals, declared_result) = self.direct_application_signature(node, call)?;
        let fields = self.shared_record_fields(node, record)?;
        let [formal] = formals else {
            return Err(Stop::Pending(node));
        };
        self.budget.step(2 + fields.len())?;
        // Matching a member admits the result shape; it never selects which
        // contained sources the result keeps.
        if formal.ty != actual.ty
            || declared_result != result
            || !fields.iter().any(|field| {
                shared_primitive_reference(&field.ty)
                    && (field.ty == *result || field.ty == *referent)
            })
        {
            return Err(Stop::Pending(node));
        }

        // Use only an incoming descriptor or a whole borrow of a named owner.
        // No alias resolution or result-to-field identity is inferred here.
        let (argument, argument_depth) = self.ungroup_expression_at(arg, depth + 1)?;
        let syntax = &self.module.syntax.nodes[argument];
        match syntax.kind {
            NodeKind::ValueReference => {
                let path = actual.path.as_ref().ok_or(Stop::Pending(arg))?;
                if path.root.kind != DeclarationKind::Parameter || !path.projections.is_empty() {
                    return Err(Stop::Pending(arg));
                }
            }
            NodeKind::Prefix("&") => {
                let (source, _) =
                    self.ungroup_expression_at(syntax.children[0], argument_depth + 1)?;
                let (owner, _) = self.named_owned_place(source)?.ok_or(Stop::Pending(arg))?;
                if owner.ty != *record {
                    return Err(Stop::Pending(arg));
                }
            }
            _ => return Err(Stop::Pending(arg)),
        }

        Ok(true)
    }

    fn written_scalar_copy(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        call: &CallFacts<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let pending = || Stop::Pending(node);
        let syntax = &self.module.syntax.nodes[node];
        let (NodeKind::Call, [callee, argument], [typed_argument]) = (
            syntax.kind,
            syntax.children.as_slice(),
            call.arguments.as_slice(),
        ) else {
            return Err(pending());
        };
        self.budget.step(1)?;
        if argument != typed_argument
            || call.pipe_input.is_some()
            || !call.defaults.is_empty()
            || facts.admission.is_some()
            || facts.ordinary_context.is_some()
            || into.is_some_and(|slot| self.slots[slot].contains_guard())
            || !matches!(self.fact(callee).map(|fact| &fact.kind),
                Some(NodeFactKind::Callee { declaration }) if *declaration == call.callee)
        {
            return Err(pending());
        }

        let input = &self.normal_value(*argument)?.ty;
        let implementation = scalar::call_target(
            self.typed,
            self.module,
            node,
            call,
            facts,
            input,
            self.budget,
        )?;
        let (root, argument_depth) = self.ungroup_expression_at(*argument, depth + 1)?;
        let syntax = &self.module.syntax.nodes[root];
        match syntax.kind {
            NodeKind::ValueReference if self.named_owned_place(root)?.is_some() => {}
            NodeKind::Prefix("&") => {
                let [owner] = syntax.children.as_slice() else {
                    return Err(pending());
                };
                let (owner, _) = self.ungroup_expression_at(*owner, argument_depth + 1)?;
                let Some((owner, _)) = self.named_owned_place(owner)? else {
                    return Err(pending());
                };
                if owner.ty != facts.ty {
                    return Err(pending());
                }
            }
            _ => return Err(pending()),
        }

        // Stage the input using the ordinary descriptor copy or borrow. The
        // call consumes only that stage and the scalar result has no loans.
        let stage = self.slot(*argument, input)?;
        self.budget.reserve(2)?;
        let index = self.calls.len();
        self.calls.push(Call {
            callee: implementation,
            args: vec![stage],
            into,
            result: CallResult::ScalarCopy,
            destination: None,
        });
        let invoke = self.emit(node, Action::Call(index), Edges::Next(next))?;
        self.expression(*argument, Some(stage), invoke, depth + 1)
    }

    fn invoke_mutable_cell(
        &mut self,
        node: NodeId,
        invocation: &InvocationFacts<'g>,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        self.budget.step(2)?;
        let pending = Stop::Pending(node);
        let syntax = &self.module.syntax.nodes[node];
        let Some(path) = &invocation.path else {
            return Err(pending);
        };
        if syntax.kind != NodeKind::Call
            || syntax.children.as_slice() != [invocation.receiver]
            || invocation.access != Access::Owned
            || invocation.assumption.is_some()
            || path.root.kind != DeclarationKind::Binding
            || !path.projections.is_empty()
        {
            return Err(pending);
        }
        let (receiver, _) = self.ungroup_expression_at(invocation.receiver, depth + 1)?;
        let value = self.normal_value(receiver)?;
        if self.module.syntax.nodes[receiver].kind != NodeKind::ValueReference
            || value.place != TypedPlace::Writable
            || !value.binding.is_some_and(|id| std::ptr::eq(id, path.root))
            || !matches!(value.ty.as_ref(), TypeTerm::Callable {
                mode: InvocationMode::Mutable, parameters, result,
            } if parameters.is_empty() && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                && *result == self.value(node)?.ty)
        {
            return Err(pending);
        }
        // The writable binding's arrow fact is absent. The reaching
        // ownership state must supply a checked environment instead.
        let receiver = self.resolve_place(receiver, path)?;
        self.emit(
            node,
            Action::InvokeMutableI32Cell { receiver, into },
            Edges::Next(next),
        )
    }

    fn call(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
        checked_carrier: Option<&Type<'g>>,
    ) -> ResultOf<Pc> {
        let Some(call) = facts.call.as_deref() else {
            let into = into.ok_or(Stop::Pending(node))?;
            if matches!(facts.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32") {
                self.budget.step(1)?;
                let invocation = self
                    .fact(&node)
                    .and_then(|fact| fact.callable.as_ref()?.invocation.as_deref())
                    .filter(|invocation| invocation.mode == InvocationMode::Mutable)
                    .ok_or(Stop::Pending(node))?;
                return self.invoke_mutable_cell(node, invocation, into, next, depth);
            }
            return self.invoke_closure_pair(node, into, next, depth);
        };
        if Some(call.callee) == self.dispose {
            if call.pipe_input.is_some() {
                return Err(Stop::Pending(node));
            }
            let [arg] = call.arguments.as_slice() else {
                return Err(Stop::Pending(node));
            };
            let operand = self.place(*arg)?;
            if let Some(release) = self.slots[operand].guard_release() {
                let Some(DataExpression::NativeOwnership(native)) = &facts.data else {
                    return Err(Stop::Pending(node));
                };
                if native.action != NativeOwnershipAction::GuardRelease
                    || native.implementation != release.implementation
                    || native.entry != release.entry
                    || !matches!(native.substitution.as_slice(), [binding]
                        if matches!(binding.argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))
                {
                    return Err(Stop::Pending(node));
                }
            } else if !matches!(self.value(*arg)?.ty.as_ref(), TypeTerm::Primitive(_))
                && !matches!((self.slots[operand].custom_disposer, &call.selection),
                    (Some(expected), Some(selection))
                        if selection.implementation == expected && selection.substitution.is_empty())
            {
                return Err(Stop::Pending(node));
            }
            let next = if let Some(into) = into {
                self.emit(node, Action::Init(into), Edges::Next(next))?
            } else {
                next
            };
            return self.emit(node, Action::Dispose(operand), Edges::Next(next));
        }
        if self
            .typed
            .copy_role()
            .is_some_and(|copy| call.callee == &copy.id)
        {
            if checked_carrier.is_some() {
                return Err(Stop::Pending(node));
            }
            return self.written_scalar_copy(node, facts, call, into, next, depth);
        }
        if !Self::direct_application_call(call)
            || into.is_some_and(|slot| self.slots[slot].contains_guard())
        {
            return Err(Stop::Pending(node));
        }
        let result = checked_carrier.unwrap_or(&facts.ty);
        let borrows_record = checked_carrier.is_none()
            && into.is_some()
            && self.borrows_record_result(node, call, result, depth)?;
        if !(borrows_record
            || self.direct_call_type(result)
            || checked_carrier.is_some() && self.admitted_option(result).is_some())
        {
            return Err(Stop::Pending(node));
        }
        let piped = call.pipe_input.is_some();
        // A propagating pipe already passed its exact carrier signature check.
        let ordinary_pipe = piped && checked_carrier.is_none();
        let mut tuple_or_leaf_admitted =
            ordinary_pipe && self.admits_direct_tuple_or_leaf_call(call, result)?;
        let mut pipe_retention = None;
        if ordinary_pipe && !tuple_or_leaf_admitted {
            // Prove the exact retaining signature before building slots.
            if !matches!(call.arguments.len(), 2..=4)
                || !matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
            {
                return Err(Stop::Pending(node));
            }
            for (position, &arg) in call.arguments.iter().enumerate() {
                self.budget.step(1)?;
                if retained_bool_contents_type(&self.value(arg)?.ty).is_some() {
                    pipe_retention = Some((
                        position,
                        self.retained_call_contract(node, call, position, result, false)?,
                    ));
                    break;
                }
            }
            if pipe_retention.is_none() {
                return Err(Stop::Pending(node));
            }
        }
        let mut retains_result = primitive_reference(result);
        if matches!(
            result.as_ref(),
            TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
        ) {
            // A flat tuple can hold the conservative input union at its
            // root; a carried member read consults that whole root.
            let output = into.ok_or(Stop::Pending(node))?;
            for member in self.slots[output].storage(output).skip(1) {
                self.budget.step(1)?;
                if self.slots[member].reference_member {
                    if !matches!(result.as_ref(), TypeTerm::Tuple(_)) {
                        return Err(Stop::Pending(node));
                    }
                    retains_result = true;
                }
            }
        }
        self.budget.reserve(call.arguments.len() + 1)?;
        let mut args = Vec::with_capacity(call.arguments.len());
        let mut destination = None;
        let mut two_hop = None;
        let mut pipe_formal = None;
        for (position, &arg) in call.arguments.iter().enumerate() {
            self.budget.step(1)?;
            if Some(arg) == call.pipe_input {
                pipe_formal = Some(position);
            }
            let ty = self.value(arg)?.ty.clone();
            if retained_two_hop_bool_type(&ty).is_some() {
                if position != 0 || checked_carrier.is_some() {
                    return Err(Stop::Pending(arg));
                }
                two_hop = Some(self.two_hop_call_route(node, call, result, depth + 1)?);
            } else if retained_bool_contents_type(&ty).is_some() {
                if !piped && destination.replace(position).is_some() {
                    return Err(Stop::Pending(arg));
                }
            } else if !borrows_record && !self.direct_call_type(&ty) && !tuple_or_leaf_admitted {
                if !self.admits_direct_tuple_or_leaf_call(call, result)? {
                    return Err(Stop::Pending(arg));
                }
                tuple_or_leaf_admitted = true;
            }
            let slot = self.slot(arg, &ty)?;
            if self.slots[slot].contains_guard() {
                return Err(Stop::Pending(arg));
            }
            args.push(slot);
        }
        let index = self.calls.len();
        let retention = match (pipe_retention, destination) {
            (Some(checked), _) => Some(checked),
            (None, Some(position)) => Some((
                position,
                self.retained_call_contract(
                    node,
                    call,
                    position,
                    result,
                    into.is_some() && checked_carrier.is_none(),
                )?,
            )),
            (None, None) => None,
        };
        let destination = if let Some(route) = two_hop {
            Some(CallDestination {
                storage: CallStorage::ThroughTwoHopParameter(route),
                argument: args[0],
                input: args[1],
                second_input: None,
                extra: None,
            })
        } else {
            retention
                .map(|(position, contract)| {
                    self.call_destination(call, position, &args, contract, depth + 1)
                })
                .transpose()?
        };
        self.calls.push(Call {
            callee: call.callee,
            args,
            into,
            result: if borrows_record {
                CallResult::RetainsRecordInput
            } else if retains_result {
                CallResult::RetainsInputs
            } else {
                CallResult::Plain
            },
            destination,
        });
        let mut next = self.emit(node, Action::Call(index), Edges::Next(next))?;
        // Slots follow parameter order. Build edges backwards so the
        // pipe input runs once, ahead of all written arguments.
        for (i, arg) in argument_staging_order(&call.arguments, pipe_formal).rev() {
            self.budget.step(1)?;
            let slot = self.calls[index].args[i];
            let root = if destination.is_some() {
                self.ungroup_expression_at(arg, depth + 1)?.0
            } else {
                self.ungroup(arg)?
            };
            let existing_exclusive = matches!(
                self.value(arg)?.ty.as_ref(),
                TypeTerm::Reference { mutable: true, .. }
            ) && !matches!(
                self.module.syntax.nodes[root].kind,
                NodeKind::Prefix("&mut")
            );
            next = if existing_exclusive {
                // Only reuse the checked descriptor for the destination.
                // A retained exclusive source reborrows its own place.
                let parent = match destination {
                    Some(CallDestination {
                        storage:
                            CallStorage::ThroughParameter { descriptor, .. }
                            | CallStorage::ThroughTwoHopParameter(OriginalTwoHopBool {
                                descriptor, ..
                            })
                            | CallStorage::ThroughAlias { descriptor, .. },
                        argument,
                        ..
                    }) if argument == slot => descriptor,
                    _ => {
                        self.budget.step(1)?;
                        self.place(root)?
                    }
                };
                self.emit(
                    arg,
                    Action::Reborrow {
                        parent,
                        into: slot,
                        exclusive: true,
                        suspends: true,
                    },
                    Edges::Next(next),
                )?
            } else {
                self.expression(arg, Some(slot), next, depth + 1)?
            };
        }
        Ok(next)
    }
}
