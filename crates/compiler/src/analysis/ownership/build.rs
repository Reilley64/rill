//! Construct ownership operations from the M4 type and control facts.
use super::*;
use crate::analysis::MemberType;
use crate::analysis::ordinary::{self, HypotheticalOutcome};
use crate::analysis::scalars::ExpressionFacts;
use crate::analysis::scalars::data::{Access, DataExpression, PathProjection};
use crate::analysis::scalars::generics::callables::CaptureForm;
use crate::loader::Origin;
use crate::parser::NodeKind;

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
        let (moves, drops) = match ty.as_ref() {
            TypeTerm::Primitive(_) => {
                let owned = owns_text(ty);
                (owned, owned)
            }
            TypeTerm::Reference { mutable, .. } => (*mutable, false),
            TypeTerm::Callable { .. } => (true, false),
            TypeTerm::Nominal { .. } => {
                let fields = self.record_fields(node, ty)?;
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
            custom_disposer: None,
            moves,
            drops,
        });
        Ok(slot)
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
            custom_disposer,
            moves: custom_disposer.is_some(),
            drops: false,
        });
        for (projection, ty) in members {
            self.budget.step(1)?;
            let reference_member = custom_disposer.is_none()
                && primitive_reference(ty)
                && matches!(ty.as_ref(), TypeTerm::Reference { mutable: false, .. });
            if !reference_member && !matches!(ty.as_ref(), TypeTerm::Primitive(_)) {
                return Err(Stop::Pending(node));
            }
            let owned = owns_text(ty);
            self.slots[root].moves |= owned;
            self.slots.push(Local {
                node,
                owner: root,
                projection: Some(projection),
                members: 0,
                reference_member,
                custom_disposer: None,
                moves: owned,
                drops: owned,
            });
        }
        Ok(root)
    }

    fn record_fields(&self, node: NodeId, ty: &TypeTerm<'g>) -> ResultOf<&'a [MemberType<'g>]> {
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = ty
        else {
            return Err(Stop::Pending(node));
        };
        if declaration.module.origin != Origin::Application || !arguments.is_empty() {
            return Err(Stop::Pending(node));
        }
        let inventory = self.typed.inventory();
        let facts = inventory
            .type_facts()
            .declarations
            .get(declaration)
            .ok_or(Stop::Pending(node))?;
        let DeclarationShape::Record { fields } = &facts.shape else {
            return Err(Stop::Pending(node));
        };
        if !facts.binders.is_empty() {
            return Err(Stop::Pending(node));
        }
        Ok(fields)
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

    pub(super) fn emit(&mut self, node: NodeId, action: Action, edges: Edges) -> ResultOf<Pc> {
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
                for &child in children {
                    self.budget.step(1)?;
                    let statement = &self.module.syntax.nodes[child];
                    if matches!(statement.kind, NodeKind::Binding { .. })
                        && self.module.syntax.nodes[statement.children[0]].kind
                            != NodeKind::Wildcard
                    {
                        let Some(NodeFactKind::Binding(binding)) =
                            self.facts.get(&child).map(|f| &f.kind)
                        else {
                            return Err(Stop::Pending(child));
                        };
                        let ty = binding.ty.as_ref().ok_or(Stop::Pending(child))?;
                        let local = self.local(
                            binding.declaration,
                            self.module.syntax.nodes[child].children[0],
                            ty,
                        )?;
                        self.budget.reserve(1)?;
                        locals.push(local);
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
                for &child in children.iter().rev() {
                    tail = self.statement(child, tail, exits, depth + 1)?;
                }
                Ok(tail)
            }
            NodeKind::Binding { .. } => {
                let value = *children.last().unwrap();
                if self.module.syntax.nodes[children[0]].kind == NodeKind::Wildcard {
                    return self.discard(node, value, next, depth + 1);
                }
                let Some(NodeFactKind::Binding(binding)) = self.facts.get(&node).map(|f| &f.kind)
                else {
                    return Err(Stop::Pending(node));
                };
                let slot = self.locals[binding.declaration];
                self.expression(value, Some(slot), next, depth + 1)
            }
            NodeKind::ExpressionStatement => self.discard(node, children[0], next, depth + 1),
            NodeKind::Assignment("=") => {
                let target = self.place(children[0])?;
                let value = children[1];
                let ty = self.value(value)?.ty.clone();
                let incoming = self.slot(value, &ty)?;
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
                self.expression(value, Some(incoming), displace, depth + 1)
            }
            NodeKind::Return => {
                if let Some(&value) = children.first() {
                    let ty = self.value(value)?.ty.clone();
                    let output = self.slot(value, &ty)?;
                    let export =
                        self.emit(node, Action::Export(output), Edges::Next(exits.returning))?;
                    let projected = self.ungroup(value)?;
                    let facts = self.value(projected)?;
                    if facts.path.is_none()
                        && facts.access == Access::Owned
                        && let Some((base, member)) = projection(facts)
                    {
                        // Stage the result before cleaning the rest of the
                        // return expression's scratch aggregate.
                        let ty = self.value(base)?.ty.clone();
                        let scratch = self.slot(base, &ty)?;
                        let from = self.member(projected, scratch, member)?;
                        let finish = self.cleanup(node, &[scratch], export)?;
                        let read = self.emit(
                            projected,
                            Action::Read {
                                from,
                                into: Some(output),
                            },
                            Edges::Next(finish),
                        )?;
                        return self.expression(base, Some(scratch), read, depth + 1);
                    }
                    self.expression(value, Some(output), export, depth + 1)
                } else {
                    Ok(exits.returning)
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
        while self.module.syntax.nodes[node].kind == NodeKind::Group {
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
                    Action::Reborrow {
                        parent: self.place(parent)?,
                        into,
                        exclusive,
                        suspends,
                    }
                } else {
                    let from = self.place(operand)?;
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
                    }
                }
            }
            NodeKind::Prefix("*") if matches!(facts.ty.as_ref(), TypeTerm::Primitive(p) if p.name() != "string") => {
                Action::Dereference {
                    from: self.place(syntax.children[0])?,
                    into,
                }
            }
            NodeKind::Bracket => {
                let into = into.ok_or(Stop::Pending(node))?;
                let Some(DataExpression::Tuple { elements }) = &facts.data else {
                    return Err(Stop::Pending(node));
                };
                if self.slots[into].members != elements.len() {
                    return Err(Stop::Pending(node));
                }
                let mut tail = self.emit(node, Action::Init(into), Edges::Next(next))?;
                for (position, &value) in elements.iter().enumerate().rev() {
                    let target = self.element(node, into, position)?;
                    tail = self.expression(value, Some(target), tail, depth + 1)?;
                }
                return Ok(tail);
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
                let mut tail = self.emit(node, Action::Init(into), Edges::Next(next))?;
                for field in data.fields.iter().rev() {
                    let target = self.field(node, into, &field.field.id)?;
                    tail = self.expression(field.expression, Some(target), tail, depth + 1)?;
                }
                return Ok(tail);
            }
            NodeKind::Arrow => {
                return self.closure(node, into.ok_or(Stop::Pending(node))?, next, depth + 1);
            }
            NodeKind::Call => return self.call(node, into, next, depth + 1),
            _ => return Err(Stop::Pending(node)),
        };
        self.emit(node, action, Edges::Next(next))
    }

    fn closure(&mut self, node: NodeId, into: Slot, next: Pc, depth: usize) -> ResultOf<Pc> {
        let arrow = self
            .facts
            .get(&node)
            .and_then(|f| f.callable.as_ref()?.arrow.as_deref())
            .ok_or(Stop::Pending(node))?;
        let body = self.module.syntax.nodes[node].children[1];
        let [capture] = arrow.captures.as_slice() else {
            return Err(Stop::Pending(node));
        };
        let mode = arrow.mode.ok_or(Stop::Pending(node))?;
        // Only an arrow whose entire body returns its single captured value is
        // lowered here. It has no additional dynamic access or cleanup to check.
        if arrow.premise.is_some()
            || !arrow.parameters.is_empty()
            || capture.site.node != body
            || !capture.reachable
            || capture.through.is_some()
            || !capture.path.projections.is_empty()
            || !matches!(
                (capture.form, self.module.syntax.nodes[body].kind),
                (
                    CaptureForm::ExplicitBorrow { mutable: false },
                    NodeKind::Prefix("&")
                ) | (
                    CaptureForm::Value | CaptureForm::CarriedReference { mutable: false },
                    NodeKind::ValueReference
                )
            )
        {
            return Err(Stop::Pending(node));
        }
        let ty = self.value(body)?.ty.clone();
        if !matches!(ty.as_ref(), TypeTerm::Primitive(_)) && !primitive_reference(&ty) {
            return Err(Stop::Pending(node));
        }
        let capture = self.slot(body, &ty)?;
        let form = self.emit(
            node,
            Action::FormClosure {
                capture,
                into,
                mode,
            },
            Edges::Next(next),
        )?;
        self.expression(body, Some(capture), form, depth + 1)
    }

    fn call(&mut self, node: NodeId, into: Option<Slot>, next: Pc, depth: usize) -> ResultOf<Pc> {
        let facts = self.value(node)?;
        let call = facts.call.as_deref().ok_or(Stop::Pending(node))?;
        if Some(call.callee) == self.dispose {
            let [arg] = call.arguments.as_slice() else {
                return Err(Stop::Pending(node));
            };
            // This slice lowers only explicit canonical disposal of primitives.
            if !matches!(self.value(*arg)?.ty.as_ref(), TypeTerm::Primitive(_)) {
                return Err(Stop::Pending(node));
            }
            let next = if let Some(into) = into {
                self.emit(node, Action::Init(into), Edges::Next(next))?
            } else {
                next
            };
            let operand = self.place(*arg)?;
            return self.emit(node, Action::Dispose(operand), Edges::Next(next));
        }
        if call.callee.kind != DeclarationKind::Function
            || call.callee.module.origin != Origin::Application
            || !call.substitution.is_empty()
            || !matches!(
                facts.ty.as_ref(),
                TypeTerm::Primitive(_) | TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
            ) && !primitive_reference(&facts.ty)
        {
            return Err(Stop::Pending(node));
        }
        if matches!(
            facts.ty.as_ref(),
            TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
        ) {
            // Aggregate result loans need their own routes. Only admit
            // borrow-free flat aggregate results here.
            let output = into.ok_or(Stop::Pending(node))?;
            for member in self.slots[output].storage(output).skip(1) {
                self.budget.step(1)?;
                if self.slots[member].reference_member {
                    return Err(Stop::Pending(node));
                }
            }
        }
        self.budget.reserve(1 + call.arguments.len())?;
        let mut args = Vec::with_capacity(call.arguments.len());
        for arg in call.argument_evaluation_order() {
            self.budget.step(1)?;
            let ty = self.value(arg)?.ty.clone();
            if !matches!(
                ty.as_ref(),
                TypeTerm::Primitive(_) | TypeTerm::Nominal { .. } | TypeTerm::Tuple(_)
            ) && !primitive_reference(&ty)
            {
                return Err(Stop::Pending(arg));
            }
            args.push(self.slot(arg, &ty)?);
        }
        let index = self.calls.len();
        self.calls.push(Call {
            callee: call.callee,
            args,
            into,
            retains: primitive_reference(&facts.ty),
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
                let parent = self.place(arg)?;
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
