//! Construct ownership operations from the M4 type and control facts.
use super::*;
use crate::analysis::MemberType;
use crate::analysis::ordinary::{self, HypotheticalOutcome};
use crate::analysis::scalars::data::{
    Access, Construction, DataExpression, PathProjection, PatternFacts, PatternProjection,
    PlacePath,
};
use crate::analysis::scalars::generics::{
    GoalState,
    callables::{CaptureFact, CaptureForm},
};
use crate::analysis::scalars::{ErrorConstruction, ExpressionFacts, PropagationTarget};
use crate::analysis::typed::{
    ErrorOperation, NativeOwnershipAction, NativeOwnershipProvider, NativeRepresentationKind,
    OptionRole, OwnershipCoverage, OwnershipRule, ResultRole,
};
use crate::loader::Origin;
use crate::parser::NodeKind;
use std::sync::Arc;

fn owns_text(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Primitive(p) if p.name() == "string")
}

fn primitive_reference(ty: &TypeTerm<'_>) -> bool {
    matches!(ty, TypeTerm::Reference { referent, .. } if matches!(referent.as_ref(), TypeTerm::Primitive(_)))
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

enum Binding<'a, 'g> {
    Discard,
    Name {
        declaration: &'g DeclarationId,
        ty: &'a Type<'g>,
        site: NodeId,
    },
    IrrefutableOwnedProduct {
        shape: NodeId,
        facts: &'a PatternFacts<'g>,
    },
}

#[derive(Clone, Copy)]
struct ProjectedTemporary {
    site: NodeId,
    base: NodeId,
    scratch: Slot,
    member: Slot,
}

impl<'a, 'g> Build<'a, 'g> {
    fn depth(&mut self, depth: usize) -> ResultOf<()> {
        self.budget.step(1)?;
        if depth > self.budget.limits.max_depth.min(64) {
            return Err(Stop::Limit);
        }
        self.budget.statistics.max_depth = self.budget.statistics.max_depth.max(depth);
        Ok(())
    }

    pub(super) fn local(
        &mut self,
        id: &'g DeclarationId,
        node: NodeId,
        ty: &Type<'g>,
    ) -> ResultOf<Slot> {
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
            TypeTerm::Callable { .. } => (true, StorageCleanup::ClosureEnvironment),
            TypeTerm::Nominal { .. } if let Some(release) = self.guard_release(ty)? => {
                (true, StorageCleanup::ReleaseGuard(release))
            }
            TypeTerm::Nominal { .. } | TypeTerm::Variant { .. }
                if let Some(role) = self.bool_option_role(ty) =>
            {
                self.prove_no_variant_disposer(node, ty, [role.some, role.none])?;
                (false, StorageCleanup::End)
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

    fn direct_call_type(&self, ty: &TypeTerm<'g>) -> bool {
        matches!(
            ty,
            TypeTerm::Primitive(_) | TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
        ) || primitive_reference(ty)
            || self.bool_option_role(ty).is_some()
            || self.scalar_result_role(ty).is_some()
    }

    fn bool_option_role(&self, ty: &TypeTerm<'g>) -> Option<OptionRole<'g>> {
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
        (*declaration == &role.declaration.id
            && matches!(payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
        .then_some(role)
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

    fn aggregate<'t>(
        &mut self,
        node: NodeId,
        custom_disposer: Option<&'g DeclarationId>,
        members: impl ExactSizeIterator<Item = (Projection<'g>, &'t Type<'g>)>,
    ) -> ResultOf<Slot>
    where
        'g: 't,
    {
        self.budget.reserve(1 + members.len())?;
        let root = self.slots.len();
        self.slots.push(Local {
            node,
            owner: root,
            projection: None,
            members: members.len(),
            reference_member: false,
            has_guard_field: false,
            custom_disposer,
            moves: custom_disposer.is_some(),
            cleanup: StorageCleanup::End,
        });
        for (projection, ty) in members {
            self.budget.step(1)?;
            let reference_member = custom_disposer.is_none()
                && primitive_reference(ty)
                && matches!(ty.as_ref(), TypeTerm::Reference { mutable: false, .. });
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
        let Some(NodeFactKind::Expression(value)) = self.facts.get(&node).map(|f| &f.kind) else {
            return Err(Stop::Pending(node));
        };
        Ok(value)
    }

    fn place(&mut self, node: NodeId) -> ResultOf<Slot> {
        let path = self.value(node)?.path.as_ref().ok_or(Stop::Pending(node))?;
        self.resolve_place(node, path)
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
        if slots.is_empty() {
            return Ok(next);
        }
        self.budget.reserve(1 + slots.len())?;
        let index = self.cleanups.len();
        // Locals are collected in declaration order, and get consecutive slot ids.
        self.cleanups.push(slots.to_vec());
        self.emit(node, Action::Cleanup(index), Edges::Next(next))
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

    fn check_retained(&mut self, node: NodeId, next: Pc) -> ResultOf<Pc> {
        if self.retained.is_empty() {
            Ok(next)
        } else {
            self.emit(node, Action::CheckRetained, Edges::Next(next))
        }
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
            NodeKind::Block => {
                let mut locals = Vec::new();
                let mut prefix = children.len();
                for (index, &child) in children.iter().enumerate() {
                    self.budget.step(1)?;
                    let statement = &self.module.syntax.nodes[child];
                    let control = if matches!(statement.kind, NodeKind::Binding { .. }) {
                        self.declare_binding(child, &mut locals)?;
                        *statement.children.last().unwrap()
                    } else {
                        child
                    };
                    // Typed statement flow already composes nested exits.
                    // An infeasible suffix introduces no slots or obligations.
                    if self
                        .facts
                        .get(&control)
                        .is_some_and(|facts| !facts.flow.normal && !facts.flow.pending)
                    {
                        prefix = index + 1;
                        break;
                    }
                }
                let exits = Exits {
                    returning: self.cleanup(node, &locals, exits.returning)?,
                    looping: match exits.looping {
                        Some(looping) => Some(Loop {
                            breaks: self.cleanup(node, &locals, looping.breaks)?,
                            continues: self.cleanup(node, &locals, looping.continues)?,
                        }),
                        None => None,
                    },
                };
                let mut tail = self.cleanup(node, &locals, next)?;
                if node == self.body {
                    tail = self.check_retained(node, tail)?;
                }
                for &child in children[..prefix].iter().rev() {
                    tail = self.statement(child, tail, exits, depth + 1)?;
                }
                Ok(tail)
            }
            NodeKind::Binding { .. } => {
                let value = *children.last().unwrap();
                let binding = self.binding(node)?;
                let initializer = self.ungroup(value)?;
                if matches!(
                    self.module.syntax.nodes[initializer].kind,
                    NodeKind::Propagate | NodeKind::Wrap
                ) {
                    let into = match binding {
                        Binding::Discard => None,
                        Binding::Name { declaration, .. } => Some(self.locals[declaration]),
                        _ => return Err(Stop::Pending(node)),
                    };
                    return self.propagate(initializer, into, next, exits.returning, depth + 1);
                }
                match binding {
                    Binding::Discard => self.discard(node, value, next, depth + 1),
                    Binding::Name { declaration, .. } => {
                        let into = self.locals[declaration];
                        let temporary = self.temporary_projection(initializer, false)?;
                        let tail = if let Some(temp) = temporary {
                            self.cleanup(node, &[temp.scratch], next)?
                        } else {
                            next
                        };
                        self.stage_value(value, into, tail, temporary, depth + 1)
                    }
                    Binding::IrrefutableOwnedProduct { facts, .. } => {
                        self.bind_product(node, facts, next, depth + 1)
                    }
                }
            }
            NodeKind::ExpressionStatement => self.discard(node, children[0], next, depth + 1),
            NodeKind::Assignment("=") => {
                let (left, value) = (children[0], children[1]);
                if let Some(path) = &self.value(left)?.path
                    && path.projections.as_slice() == [PathProjection::Dereference]
                {
                    let descriptor = self.bool_deref_descriptor_slot(left, path)?;
                    let ty = self.value(value)?.ty.clone();
                    let incoming = self.slot(value, &ty)?;
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
                let target = self.place(left)?;
                if self.slots[target].projection.is_some()
                    && self.slots[target].guard_release().is_some()
                {
                    return Err(Stop::Pending(left));
                }
                let ty = self.value(value)?.ty.clone();
                let incoming = self.slot(value, &ty)?;
                let initializer = self.ungroup(value)?;
                let temporary = self.temporary_projection(initializer, false)?;
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
                self.stage_value(value, incoming, displace, temporary, depth + 1)
            }
            NodeKind::Return => {
                let returning = self.check_retained(node, exits.returning)?;
                if let Some(&value) = children.first() {
                    let ty = self.value(value)?.ty.clone();
                    let output = self.slot(value, &ty)?;
                    if self.slots[output].contains_guard() {
                        return Err(Stop::Pending(value));
                    }
                    let export = self.emit(node, Action::Export(output), Edges::Next(returning))?;
                    let projected = self.ungroup(value)?;
                    let temporary = self.temporary_projection(projected, true)?;
                    let finish = if let Some(temp) = temporary {
                        self.cleanup(node, &[temp.scratch], export)?
                    } else {
                        export
                    };
                    self.stage_value(value, output, finish, temporary, depth + 1)
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
                    let backedge = self.emit(node, Action::None, Edges::End)?;
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

    fn temporary_projection(
        &mut self,
        node: NodeId,
        allow_field: bool,
    ) -> ResultOf<Option<ProjectedTemporary>> {
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
                let Some(NodeFactKind::Pattern(facts)) = self.facts.get(&pattern).map(|f| &f.kind)
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
                let Some(NodeFactKind::Pattern(facts)) = self.facts.get(&pattern).map(|f| &f.kind)
                else {
                    return Err(Stop::Pending(pattern));
                };
                if facts.access != Access::Owned
                    || !facts.coverage.exhaustive()
                    || !matches!(
                        (kind, facts.ty.as_ref()),
                        (NodeKind::BracketPattern, TypeTerm::Tuple(_))
                            | (NodeKind::RecordPattern, TypeTerm::Nominal { .. })
                    )
                {
                    return Err(Stop::Pending(pattern));
                }
                Ok(Binding::IrrefutableOwnedProduct { shape, facts })
            }
            _ => {
                let Some(NodeFactKind::Binding(binding)) = self.facts.get(&node).map(|f| &f.kind)
                else {
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

    fn declare_binding(&mut self, node: NodeId, locals: &mut Vec<Slot>) -> ResultOf<()> {
        let pattern = self.module.syntax.nodes[node].children[0];
        match self.binding(node)? {
            Binding::Discard => Ok(()),
            Binding::Name {
                declaration,
                ty,
                site,
            } => {
                let local = self.local(declaration, site, ty)?;
                self.budget.reserve(1)?;
                locals.push(local);
                Ok(())
            }
            Binding::IrrefutableOwnedProduct { shape, facts } => {
                let syntax = &self.module.syntax.nodes[shape];
                let record = syntax.kind == NodeKind::RecordPattern;
                let first = usize::from(
                    record
                        && syntax.children.first().is_some_and(|&child| {
                            self.module.syntax.nodes[child].kind == NodeKind::PatternPath
                        }),
                );
                let mut binders = facts.binders.iter();
                self.budget.reserve(binders.len())?;
                for (position, &child) in syntax.children[first..].iter().enumerate() {
                    self.budget.step(1)?;
                    let (component, shorthand) = if record {
                        let field = &self.module.syntax.nodes[child];
                        let NodeKind::PatternField { shorthand } = field.kind else {
                            return Err(Stop::Pending(child));
                        };
                        (field.children[usize::from(!shorthand)], shorthand)
                    } else {
                        (child, false)
                    };
                    let name = self.ungroup(component)?;
                    let syntax = &self.module.syntax.nodes[name];
                    if syntax.kind == NodeKind::Wildcard {
                        continue;
                    }
                    if !shorthand && syntax.kind != NodeKind::NamePattern {
                        return Err(Stop::Pending(name));
                    }
                    let binder = binders.next().ok_or(Stop::Pending(name))?;
                    let flat = match binder.projections.as_slice() {
                        [PatternProjection::TupleElement(index)] => !record && *index == position,
                        [PatternProjection::Field(_)] => record,
                        _ => false,
                    };
                    if !flat || binder.declaration.start != syntax.span.start() {
                        return Err(Stop::Pending(name));
                    }
                    locals.push(self.local(binder.declaration, name, &binder.ty)?);
                }
                if binders.next().is_some() {
                    return Err(Stop::Pending(pattern));
                }
                Ok(())
            }
        }
    }

    fn bind_product(
        &mut self,
        node: NodeId,
        facts: &PatternFacts<'g>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        // Evaluate the whole owned initializer once. Bind names from the
        // temporary, then clean the remaining members at statement exit.
        let source = facts.source.node;
        let scratch = self.slot(source, &facts.ty)?;
        if self.slots[scratch].custom_disposer.is_some() || self.slots[scratch].contains_guard() {
            return Err(Stop::Pending(node));
        }
        let mut tail = self.cleanup(node, &[scratch], next)?;
        for binder in facts.binders.iter().rev() {
            self.budget.step(1)?;
            let into = *self
                .locals
                .get(binder.declaration)
                .ok_or(Stop::Pending(node))?;
            let site = self.slots[into].node;
            let from = match binder.projections.as_slice() {
                [PatternProjection::TupleElement(position)] => {
                    self.element(site, scratch, *position)?
                }
                [PatternProjection::Field(field)] => self.field(site, scratch, field)?,
                _ => return Err(Stop::Pending(node)),
            };
            tail = self.emit(
                site,
                Action::Read {
                    from,
                    into: Some(into),
                },
                Edges::Next(tail),
            )?;
        }
        self.expression(source, Some(scratch), tail, depth)
    }

    fn propagate(
        &mut self,
        node: NodeId,
        into: Option<Slot>,
        next: Pc,
        return_cleanup: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let source = self.module.syntax.nodes[node].children[0];
        let operand = self.value(source)?;
        let (id, result) = self.return_contract;
        let (exit, wrap) = match &self.value(node)?.data {
            Some(DataExpression::Propagation(exit)) => (exit.as_ref(), None),
            Some(DataExpression::Wrapping(wrap)) => (&wrap.branch, Some(&wrap.construction)),
            _ => return Err(Stop::Pending(node)),
        };
        if operand.access != Access::Owned
            || !matches!(exit.target, PropagationTarget::Callable(target) if target == id)
        {
            return Err(Stop::Pending(node));
        }
        let wrapper = if let Some(error) = &exit.error {
            let (role, _) = self
                .scalar_result_role(&operand.ty)
                .ok_or(Stop::Pending(node))?;
            if !matches!(self.scalar_result_role(result), Some((_, None)))
                || exit.success != &role.ok.id
                || exit.failure != &role.err.id
                || error.wrapper != &role.wrapper.id
                || !matches!(error.payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            {
                return Err(Stop::Pending(node));
            }
            Some(error.wrapper)
        } else {
            let role = self
                .bool_option_role(&operand.ty)
                .ok_or(Stop::Pending(node))?;
            if wrap.is_some()
                || self.bool_option_role(result).is_none()
                || exit.success != &role.some.id
                || exit.failure != &role.none.id
            {
                return Err(Stop::Pending(node));
            }
            None
        };
        let scratch = self.slot(source, &operand.ty)?;
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
            let success = if exit.can_continue {
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
            } else {
                None
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
                Some(self.emit(node, Action::End(scratch), Edges::Next(empty))?)
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
        self.expression(source, Some(scratch), split, depth)
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
        let role = self.typed.result_role().ok_or(Stop::Pending(node))?;
        let [left, right] = self.module.syntax.nodes[node].children[..] else {
            return Err(Stop::Pending(node));
        };
        let retained = data.retained.as_ref().ok_or(Stop::Pending(node))?;
        self.budget.step(1)?;
        if data.operation != ErrorOperation::Wrap
            || data.result != *self.return_contract.1
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
                cause_display: selected.implementation,
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
        let option = self.bool_option_role(&data.ty);
        let payload = match data.arguments.as_slice() {
            [] if option.is_some_and(|role| *declaration == &role.none.id) => None,
            [value]
                if option.is_some_and(|role| *declaration == &role.some.id)
                    || self
                        .scalar_result_role(&data.ty)
                        .is_some_and(|(role, _)| *declaration == &role.ok.id) =>
            {
                let ty = self.value(*value)?.ty.clone();
                Some((*value, self.slot(*value, &ty)?))
            }
            _ => return Err(Stop::Pending(node)),
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

    fn construct_scalar_error(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        data: &ErrorConstruction<'g>,
        into: Slot,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let (role, None) = self
            .scalar_result_role(&data.result)
            .ok_or(Stop::Pending(node))?
        else {
            return Err(Stop::Pending(node));
        };
        let syntax = &self.module.syntax.nodes[node];
        let providers = role.providers.ok_or(Stop::Pending(node))?;
        if syntax.kind != NodeKind::Call
            || data.operation != ErrorOperation::Construct
            || data.retained.is_some()
            || facts.ty != data.result
            || data.failure != &role.err.id
            || data.wrapper != &role.wrapper.id
            || data.origin.snapshot() != syntax.span.snapshot()
            || data.origin.start() != syntax.span.start()
            || data.origin.end() != syntax.span.start()
            || !matches!(data.payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            || !facts.call.as_ref().is_some_and(|call| {
                call.callee == &providers.constructor.id
                    && call.arguments.as_slice() == [data.value]
            })
        {
            return Err(Stop::Pending(node));
        }
        self.budget.step(1)?;
        if self.value(data.value)?.ty != data.payload {
            return Err(Stop::Pending(node));
        }
        let payload = self.slot(data.value, &data.payload)?;
        let construct = self.emit(
            node,
            Action::ConstructResultError {
                constructor: &providers.constructor.id,
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
        self.depth(depth)?;
        let syntax = &self.module.syntax.nodes[node];
        let facts = self.value(node)?;
        let flow = &self.facts[&node].flow;
        if !flow.normal
            || flow.pending
            || flow.returns
            || !flow.breaks.is_empty()
            || !flow.continues.is_empty()
        {
            return Err(Stop::Pending(node));
        }
        if let Some(DataExpression::Construction(data)) = &facts.data
            && (self.bool_option_role(&data.ty).is_some()
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
        if projection(facts).is_some() {
            let action = if facts.access != Access::Owned && owns_text(&facts.ty) {
                let path = facts.path.as_ref().ok_or(Stop::Pending(node))?;
                let root = *self.locals.get(path.root).ok_or(Stop::Pending(node))?;
                Action::ForbiddenBorrowedMove(root)
            } else {
                Action::Read {
                    from: self.place(node)?,
                    into,
                }
            };
            return self.emit(node, action, Edges::Next(next));
        }
        let action = match syntax.kind {
            NodeKind::Group => return self.expression(syntax.children[0], into, next, depth + 1),
            NodeKind::Literal => into.map_or(Action::None, Action::Init),
            NodeKind::ValueReference => Action::Read {
                from: self.place(node)?,
                into,
            },
            NodeKind::Prefix("&" | "&mut") => {
                let operand = self.ungroup(syntax.children[0])?;
                let into = into.ok_or(Stop::Pending(node))?;
                let exclusive = syntax.kind == NodeKind::Prefix("&mut");
                let source = &self.module.syntax.nodes[operand];
                if source.kind == NodeKind::Prefix("*") && primitive_reference(&facts.ty) {
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
                    let from = self.place(operand)?;
                    if self.slots[from].contains_guard() {
                        return Err(Stop::Pending(node));
                    }
                    // Initialization is per position. It does not establish a
                    // loan-overlap rule for separately borrowed tuple elements.
                    if matches!(
                        self.slots[from].projection,
                        Some(Projection::TupleElement(_))
                    ) {
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
                Action::Dereference { from, into }
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
                return self.initialize_members(node, into, next, depth + 1, initializers);
            }
            NodeKind::Construction => {
                let into = into.ok_or(Stop::Pending(node))?;
                let Some(DataExpression::Construction(data)) = facts.data.as_ref() else {
                    return Err(Stop::Pending(node));
                };
                if data.base.is_some()
                    || !data.arguments.is_empty()
                    || self.slots[into].members != data.fields.len()
                {
                    return Err(Stop::Pending(node));
                }
                let initializers = data
                    .fields
                    .iter()
                    .map(|field| (Projection::Field(field.field), field.expression));
                return self.initialize_members(node, into, next, depth + 1, initializers);
            }
            NodeKind::Arrow => {
                return self.closure(node, into.ok_or(Stop::Pending(node))?, next, depth + 1);
            }
            NodeKind::Call => return self.call(node, into, next, depth + 1),
            _ => return Err(Stop::Pending(node)),
        };
        self.emit(node, action, Edges::Next(next))
    }

    fn initialize_members(
        &mut self,
        node: NodeId,
        into: Slot,
        next: Pc,
        depth: usize,
        initializers: impl DoubleEndedIterator<Item = (Projection<'g>, NodeId)>,
    ) -> ResultOf<Pc> {
        let mut tail = self.emit(node, Action::Init(into), Edges::Next(next))?;
        for (projection, value) in initializers.rev() {
            let target = self.member(node, into, projection)?;
            tail = self.expression(value, Some(target), tail, depth)?;
        }
        Ok(tail)
    }

    fn closure(&mut self, node: NodeId, into: Slot, next: Pc, depth: usize) -> ResultOf<Pc> {
        self.budget.step(1)?;
        let arrow = self
            .facts
            .get(&node)
            .and_then(|f| f.callable.as_ref()?.arrow.as_deref())
            .ok_or(Stop::Pending(node))?;
        let body = self.module.syntax.nodes[node].children[1];
        let mode = arrow.mode.ok_or(Stop::Pending(node))?;
        if arrow.premise.is_some() || !arrow.parameters.is_empty() {
            return Err(Stop::Pending(node));
        }
        // Admit only direct stable sources, either the entire body or a
        // written pair. A tuple of these sources adds no body computation
        // to run at formation.
        let (first, second) = match arrow.captures.as_slice() {
            [capture] if capture.site.node == body => (capture, None),
            [first, second] if first.path.root != second.path.root => {
                self.budget.step(1)?;
                let Some(DataExpression::Tuple { elements }) = &self.value(body)?.data else {
                    return Err(Stop::Pending(node));
                };
                if elements.as_slice() == [first.site.node, second.site.node] {
                    (first, Some(second))
                } else if elements.as_slice() == [second.site.node, first.site.node] {
                    (second, Some(first))
                } else {
                    return Err(Stop::Pending(node));
                }
            }
            _ => return Err(Stop::Pending(node)),
        };
        let capture = self.capture_slot(first)?;
        let second = second
            .map(|fact| self.capture_slot(fact).map(|slot| (fact.site.node, slot)))
            .transpose()?;
        let form = self.emit(
            node,
            Action::FormClosure {
                capture,
                second: second.map(|(_, slot)| slot),
                into,
                mode,
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

    fn capture_slot(&mut self, capture: &CaptureFact<'g>) -> ResultOf<Slot> {
        let node = capture.site.node;
        let syntax = &self.module.syntax.nodes[node];
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

    fn call_destination(
        &mut self,
        node: NodeId,
        call: &crate::analysis::scalars::CallFacts<'g>,
        position: usize,
        args: &[Slot],
    ) -> ResultOf<CallDestination> {
        // This contract is determined by the declaration, not by inspecting
        // its body for a narrower effect. Only one possible input is admitted.
        self.budget.step(1)?;
        let signature = self
            .typed
            .inventory()
            .type_facts()
            .declarations
            .get(call.callee)
            .ok_or(Stop::Pending(node))?;
        let (contents, formal) =
            retained_bool_call_types(&signature.shape, position).ok_or(Stop::Pending(node))?;
        if call.arguments.len() != 2 || call.pipe_input.is_some() {
            return Err(Stop::Pending(node));
        }
        let input = 1 - position;
        self.budget.step(1)?;
        if self.value(call.arguments[input])?.ty != *formal {
            return Err(Stop::Pending(node));
        }
        let argument = self.ungroup(call.arguments[position])?;
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
            let storage = if let Some(&contents) = self.retained.get(&descriptor) {
                CallStorage::ThroughParameter {
                    descriptor,
                    contents,
                }
            } else if path.root.kind == DeclarationKind::Binding {
                CallStorage::ThroughAlias {
                    descriptor,
                    argument: args[position],
                }
            } else {
                return Err(Stop::Pending(argument));
            };
            return Ok(CallDestination {
                storage,
                input: args[input],
            });
        }
        if syntax.kind != NodeKind::Prefix("&mut") {
            return Err(Stop::Pending(argument));
        }
        let target = syntax.children[0];
        self.budget.step(1)?;
        let facts = self.value(target)?;
        let Some(path) = &facts.path else {
            return Err(Stop::Pending(target));
        };
        if facts.access != Access::Owned
            || facts.ty != *contents
            || path.root.kind != DeclarationKind::Binding
        {
            return Err(Stop::Pending(target));
        }
        if !matches!(path.projections.as_slice(), [] | [PathProjection::Field(_)]) {
            return Err(Stop::Pending(target));
        }
        let place = self.resolve_place(target, path)?;
        Ok(CallDestination {
            storage: CallStorage::Local(place),
            input: args[input],
        })
    }

    fn call(&mut self, node: NodeId, into: Option<Slot>, next: Pc, depth: usize) -> ResultOf<Pc> {
        let facts = self.value(node)?;
        let call = facts.call.as_deref().ok_or(Stop::Pending(node))?;
        if Some(call.callee) == self.dispose {
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
        if call.callee.kind != DeclarationKind::Function
            || call.callee.module.origin != Origin::Application
            || !call.substitution.is_empty()
            || !self.direct_call_type(&facts.ty)
            || into.is_some_and(|slot| self.slots[slot].contains_guard())
        {
            return Err(Stop::Pending(node));
        }
        let mut retains_result = primitive_reference(&facts.ty);
        if matches!(
            facts.ty.as_ref(),
            TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
        ) {
            // A flat tuple can hold the conservative input union at its
            // root; a carried member read consults that whole root.
            let output = into.ok_or(Stop::Pending(node))?;
            for member in self.slots[output].storage(output).skip(1) {
                self.budget.step(1)?;
                if self.slots[member].reference_member {
                    if !matches!(facts.ty.as_ref(), TypeTerm::Tuple(_)) {
                        return Err(Stop::Pending(node));
                    }
                    retains_result = true;
                }
            }
        }
        self.budget.reserve(1 + call.arguments.len())?;
        let mut args = Vec::with_capacity(call.arguments.len());
        let mut destination = None;
        for (position, arg) in call.argument_evaluation_order().enumerate() {
            self.budget.step(1)?;
            let ty = self.value(arg)?.ty.clone();
            if retained_bool_contents_type(&ty).is_some() {
                if destination.replace(position).is_some() {
                    return Err(Stop::Pending(arg));
                }
            } else if !self.direct_call_type(&ty) {
                return Err(Stop::Pending(arg));
            }
            let slot = self.slot(arg, &ty)?;
            if self.slots[slot].contains_guard() {
                return Err(Stop::Pending(arg));
            }
            args.push(slot);
        }
        let index = self.calls.len();
        let destination = destination
            .map(|position| self.call_destination(node, call, position, &args))
            .transpose()?;
        self.calls.push(Call {
            callee: call.callee,
            args,
            into,
            retains_result,
            destination,
        });
        let mut next = self.emit(node, Action::Call(index), Edges::Next(next))?;
        for (i, &arg) in call.arguments.iter().enumerate().rev() {
            let slot = self.calls[index].args[i];
            let root = self.ungroup(arg)?;
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
                            | CallStorage::ThroughAlias { descriptor, .. },
                        input,
                    }) if input != slot => descriptor,
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
