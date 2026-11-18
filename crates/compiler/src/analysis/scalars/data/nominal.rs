//! Per-use nominal inference over declared M4.4 member and payload types.
use super::*;

type MemberConstraints<'a, 'g> = Vec<(NodeId, &'a Type<'g>)>;

pub(in crate::analysis::scalars) struct PreparedNominal<'g> {
    declaration: &'g DeclarationId,
    variant: Option<&'g DeclarationId>,
    fixed: Vec<Substitution<'g>>,
    sources: Vec<ArgumentSource<'g>>,
    bindings: Vec<&'g Binding>,
    fields: Vec<FieldValue<'g>>,
    members: Vec<(NodeId, Type<'g>)>,
    base: Option<NodeId>,
    pub(super) arguments_closed: bool,
    pub(in crate::analysis::scalars) receiving_reads: BTreeSet<usize>,
}

impl<'g> PreparedNominal<'g> {
    pub(in crate::analysis::scalars) fn receiving_arguments<'a>(
        &self,
        ty: &'a Type<'g>,
    ) -> Option<&'a [Type<'g>]> {
        let parent = match ty.as_ref() {
            TypeTerm::Variant {
                declaration,
                parent,
            } if self.variant == Some(*declaration) => parent,
            _ => ty,
        };
        match parent.as_ref() {
            TypeTerm::Nominal {
                declaration,
                arguments,
            } if *declaration == self.declaration => Some(arguments),
            _ => None,
        }
    }
}

impl<'r, 'g> Checker<'r, 'g> {
    fn prepare_nominal(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<PreparedNominal<'g>>> {
        let nodes = &module.syntax.nodes;
        let syntax = &nodes[node];
        let constructor = if syntax.kind == NodeKind::Field {
            node
        } else {
            syntax.children[0]
        };
        let path = &nodes[constructor];
        let (name, variant_name) = if path.kind == NodeKind::Field {
            (path.children[0], Some(path.children[1]))
        } else {
            (
                constructor,
                path.children
                    .last()
                    .copied()
                    .filter(|&i| nodes[i].kind == NodeKind::Name),
            )
        };
        if nodes[name].kind != NodeKind::ValueReference {
            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            return Ok(None);
        }
        let binders = context.signature.map_or(&[][..], |s| &s.binders);
        let mut bindings = Vec::new();
        let Some(NameTarget::Declaration(declaration)) =
            self.elab
                .named_target(module, name, binders, &mut bindings)?
        else {
            self.lookup_finding(module, node, ScalarFindingKind::TypeMismatch, bindings)?;
            return Ok(None);
        };
        let Some(facts) = self.input.facts.declarations.get(&declaration.id) else {
            self.lookup_finding(module, node, ScalarFindingKind::TypeMismatch, bindings)?;
            return Ok(None);
        };
        if declaration.id.module != module.key && !declaration.public {
            self.lookup_finding(module, node, ScalarFindingKind::PrivateValue, bindings)?;
            return Ok(None);
        }
        let mut variant = None;
        let mut fields = Vec::new();
        let mut members = Vec::new();
        let base = if syntax.kind == NodeKind::Construction {
            syntax
                .children
                .get(1)
                .filter(|&&id| nodes[id].kind == NodeKind::Spread)
                .map(|&id| nodes[id].children[0])
        } else {
            None
        };
        let record_fields = match (&facts.shape, variant_name) {
            (DeclarationShape::Record { fields }, None)
                if syntax.kind == NodeKind::Construction =>
            {
                Some(fields)
            }
            (DeclarationShape::Enum { variants }, Some(name)) => {
                self.elab.step(variants.len())?;
                let spelling = nodes[name].span.bytes();
                let Some(selected) = variants.iter().find(|v| {
                    v.declaration
                        .name
                        .as_ref()
                        .is_some_and(|s| s.as_bytes() == spelling)
                }) else {
                    self.lookup_finding(module, node, ScalarFindingKind::MissingField, bindings)?;
                    return Ok(None);
                };
                variant = Some(&selected.declaration.id);
                match (&selected.payload, syntax.kind) {
                    (Payload::Unit, NodeKind::Field) => None,
                    (Payload::Tuple(types), NodeKind::Call) => {
                        let args = &syntax.children[1..];
                        if args.len() != types.len() {
                            self.finding(module, node, ScalarFindingKind::Arity)?;
                            return Ok(None);
                        }
                        self.elab.reserve(args.len())?;
                        members.extend(args.iter().copied().zip(types.iter().cloned()));
                        None
                    }
                    (Payload::Record(fields), NodeKind::Construction) => Some(fields),
                    _ => {
                        self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                        return Ok(None);
                    }
                }
            }
            _ => {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            }
        };
        if let Some(record_fields) = record_fields {
            self.elab.reserve(record_fields.len())?;
            if declaration.id.module != module.key
                && record_fields.iter().any(|f| !f.declaration.public)
            {
                self.lookup_finding(module, node, ScalarFindingKind::PrivateValue, bindings)?;
                return Ok(None);
            }
            let types: Vec<_> = record_fields
                .iter()
                .map(|f| (f.declaration, f.ty.clone()))
                .collect();
            let Some(initializers) =
                self.record_initializers(module, node, &types, base.is_some())?
            else {
                return Ok(None);
            };
            fields = initializers.fields;
            members = initializers.contexts;
        }
        let Some(arguments) = self.elab.written_arguments(module, name, binders, 1)? else {
            return Ok(None);
        };
        if arguments.len() > facts.binders.len() {
            self.lookup_finding(module, node, ScalarFindingKind::Arity, bindings)?;
            return Ok(None);
        }
        let written = self.nominal_substitution(&facts.binders, &arguments)?;
        self.elab.reserve(written.len())?;
        let sources = nodes[name]
            .children
            .get(1)
            .filter(|&&i| nodes[i].kind == NodeKind::TypeArguments)
            .into_iter()
            .flat_map(|&i| nodes[i].children.iter())
            .enumerate()
            .map(|(position, &id)| ArgumentSource {
                position,
                site: Self::site(module, id),
                origin: ArgumentOrigin::Written,
            })
            .collect();
        Ok(Some(PreparedNominal {
            declaration: &declaration.id,
            variant,
            fixed: written,
            sources,
            bindings,
            fields,
            members,
            base,
            arguments_closed: false,
            receiving_reads: BTreeSet::new(),
        }))
    }

    pub(in crate::analysis::scalars) fn nominal_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        if inference.slots[&node].blocked || inference.data.contains_key(&node) {
            return Ok(());
        }
        let Some(mut frame) = inference
            .nominals
            .remove(&node)
            .map(Some)
            .map_or_else(|| self.prepare_nominal(inference.module, node, context), Ok)?
        else {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        self.solve_nominal(inference, node, &mut frame, false)?;
        if !inference.data.contains_key(&node) && !inference.slots[&node].blocked {
            self.elab.reserve(1)?;
            inference.nominals.insert(node, frame);
        }
        Ok(())
    }

    pub(in crate::analysis::scalars) fn apply_ready_nominal_default(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<bool> {
        self.elab.reserve(inference.nominals.len())?;
        let nodes: Vec<_> = inference.nominals.keys().copied().collect();
        for node in nodes {
            let mut frame = inference.nominals.remove(&node).unwrap();
            let applied = self.solve_nominal(inference, node, &mut frame, true)?;
            if !inference.data.contains_key(&node) && !inference.slots[&node].blocked {
                self.elab.reserve(1)?;
                inference.nominals.insert(node, frame);
            }
            if applied || !inference.queue.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn nominal_inputs(&mut self, frame: &PreparedNominal<'g>) -> Work<Vec<NodeId>> {
        self.elab
            .reserve(frame.members.len() + usize::from(frame.base.is_some()))?;
        Ok(frame
            .base
            .into_iter()
            .chain(frame.members.iter().map(|(i, _)| *i))
            .collect())
    }

    pub(super) fn has_blocked_input(
        &mut self,
        inference: &Inference<'g>,
        mut stack: Vec<NodeId>,
    ) -> Work<bool> {
        while let Some(node) = stack.pop() {
            self.elab.step(1)?;
            let slot = &inference.slots[&node];
            if slot.blocked {
                return Ok(true);
            }
            if slot.ty.is_none() {
                stack.extend(self.runtime_children(inference, node)?);
            }
        }
        Ok(false)
    }

    // A fresh bracket under a written tuple shape contributes independent
    // element anchors. Never decompose an existing tuple as fresh source.
    fn nominal_constraints<'a>(
        &mut self,
        inference: &mut Inference<'g>,
        members: &'a [(NodeId, Type<'g>)],
    ) -> Work<Option<MemberConstraints<'a, 'g>>> {
        self.elab.reserve(members.len())?;
        let mut stack: Vec<_> = members.iter().rev().map(|(i, ty)| (*i, ty)).collect();
        let mut constraints = Vec::new();
        while let Some((node, pattern)) = stack.pop() {
            self.elab.step(1)?;
            let syntax = &inference.module.syntax.nodes[node];
            if syntax.kind == NodeKind::Bracket
                && let TypeTerm::Tuple(elements) = pattern.as_ref()
            {
                let slot = inference.slots.get_mut(&node).unwrap();
                slot.nominal_tuple = true;
                if syntax.children.len() != elements.len() {
                    slot.blocked = true;
                    self.finding(inference.module, node, ScalarFindingKind::Arity)?;
                    return Ok(None);
                }
                self.elab.reserve(elements.len())?;
                stack.extend(syntax.children.iter().copied().zip(elements).rev());
            } else {
                self.elab.reserve(1)?;
                constraints.push((node, pattern));
            }
        }
        Ok(Some(constraints))
    }

    fn solve_nominal(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        frame: &mut PreparedNominal<'g>,
        defaults_ready: bool,
    ) -> Work<bool> {
        let module = inference.module;
        let facts = &self.input.facts.declarations[frame.declaration];
        let binders = &facts.binders;
        self.elab.reserve(binders.len() + frame.sources.len())?;
        let mut slots = vec![None; binders.len()];
        let mut sources = frame.sources.clone();
        for fixed in &frame.fixed {
            slots[fixed.position] = Some(fixed.argument.clone());
        }
        let mut valid = true;
        for (site, anchor, origin, exact) in [
            (
                node,
                inference.slots[&node].expected.as_ref(),
                ArgumentOrigin::Expected,
                false,
            ),
            (
                frame.base.unwrap_or(node),
                frame.base.and_then(|i| inference.slots[&i].ty.as_ref()),
                ArgumentOrigin::RebuildBase,
                true,
            ),
        ] {
            let Some(anchor) = anchor else {
                continue;
            };
            let (parent, variant) = match anchor.as_ref() {
                TypeTerm::Variant {
                    parent,
                    declaration,
                } => (parent, Some(*declaration)),
                _ => (anchor, None),
            };
            match parent.as_ref() {
                TypeTerm::Nominal {
                    declaration,
                    arguments,
                } if *declaration == frame.declaration => {
                    if (exact || variant.is_some()) && variant != frame.variant {
                        self.finding(module, site, ScalarFindingKind::TypeMismatch)?;
                        valid = false;
                    }
                    for (position, (slot, ty)) in slots.iter_mut().zip(arguments).enumerate() {
                        if let Some(old) = slot
                            && !self.same(old, ty)?
                        {
                            self.mismatch(module, site, ty, old)?;
                            valid = false;
                        } else {
                            *slot = Some(ty.clone());
                            self.elab.reserve(1)?;
                            sources.push(ArgumentSource {
                                position,
                                site: Self::site(module, site),
                                origin,
                            });
                        }
                    }
                }
                _ if exact && primitive_name(anchor) != Some("never") => {
                    self.finding(module, site, ScalarFindingKind::TypeMismatch)?;
                    valid = false;
                }
                _ => (),
            }
        }
        let Some(constraints) = self.nominal_constraints(inference, &frame.members)? else {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(false);
        };
        let fixed = self.slot_substitutions(binders, &slots)?;
        for (member, pattern) in &constraints {
            self.elab.step(1)?;
            if !inference.slots[member].blocked
                && let Some(ty) = &inference.slots[member].ty
                && primitive_name(ty) != Some("never")
            {
                self.elab.reserve(slots.len())?;
                let mut constrained = slots.clone();
                if self.accept_input(pattern, ty, binders, &mut constrained, &fixed)? {
                    for (position, (before, after)) in slots.iter().zip(&constrained).enumerate() {
                        if before.is_none() && after.is_some() {
                            self.elab.reserve(1)?;
                            sources.push(ArgumentSource {
                                position,
                                site: Self::site(module, *member),
                                origin: ArgumentOrigin::Member,
                            });
                        }
                    }
                    slots = constrained;
                } else {
                    self.finding(module, *member, ScalarFindingKind::TypeMismatch)?;
                    inference.slots.get_mut(member).unwrap().blocked = true;
                    valid = false;
                }
            }
        }
        if !valid {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(false);
        }
        if defaults_ready
            && frame
                .base
                .is_none_or(|base| inference.slots[&base].ty.is_some())
        {
            let owner = self.elab.owner(&frame.declaration.module)?;
            if let Some((substitution, source)) = self.try_nominal_default(
                inference,
                node,
                owner,
                binders,
                &slots,
                &constraints,
                &mut frame.receiving_reads,
            )? {
                self.elab.reserve(2)?;
                frame.fixed.push(substitution);
                frame.sources.push(source);
                self.enqueue(inference, node)?;
                return Ok(true);
            }
            if inference.slots[&node].blocked {
                return Ok(false);
            }
        }
        let substitution = self.slot_substitutions(binders, &slots)?;
        frame.arguments_closed = substitution.len() == binders.len();
        self.elab.reserve(frame.members.len())?;
        let mut contexts = Vec::new();
        for (member, pattern) in &frame.members {
            if self.closed(pattern, binders, &|i| slots[i].is_some(), 1)? {
                let expected = self.substitute(pattern, &substitution, 1)?;
                self.establish(inference, *member, expected.clone(), true)?;
                contexts.push((*member, expected));
            }
        }
        if !frame.arguments_closed
            || frame.base.is_some_and(|i| inference.slots[&i].ty.is_none())
            || frame
                .members
                .iter()
                .any(|(i, _)| inference.slots[i].ty.is_none())
        {
            return Ok(false);
        }
        self.elab
            .reserve(3 + substitution.len() + frame.members.len() * 2)?;
        let parent = Arc::new(TypeTerm::Nominal {
            declaration: frame.declaration,
            arguments: substitution.iter().map(|s| s.argument.clone()).collect(),
        });
        let ty = match frame.variant {
            Some(declaration) => Arc::new(TypeTerm::Variant {
                parent,
                declaration,
            }),
            None => parent,
        };
        if let Some(base) = frame.base {
            self.establish(inference, base, ty.clone(), true)?;
        }
        let inputs = self.nominal_inputs(frame)?;
        let arguments = if module.syntax.nodes[node].kind == NodeKind::Call {
            frame.members.iter().map(|(i, _)| *i).collect()
        } else {
            Vec::new()
        };
        self.establish(inference, node, ty.clone(), false)?;
        inference.data.insert(
            node,
            PreparedData {
                ty: ty.clone(),
                flow: None,
                contexts,
                inputs,
                fact: DataExpression::Construction(Box::new(Construction {
                    ty,
                    fields: std::mem::take(&mut frame.fields),
                    arguments,
                    base: frame.base,
                    substitution,
                    argument_sources: sources,
                    bindings: std::mem::take(&mut frame.bindings),
                })),
            },
        );
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn try_nominal_default(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        owner: &'g Module,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
        constraints: &[(NodeId, &Type<'g>)],
        receiving_reads: &mut BTreeSet<usize>,
    ) -> Work<Option<(Substitution<'g>, ArgumentSource<'g>)>> {
        let mut producers = Vec::new();
        for &(member, pattern) in constraints {
            self.source_producers(
                inference,
                member,
                pattern,
                (binders, None),
                slots,
                &mut producers,
            )?;
        }
        let mut defaulted = BTreeSet::new();
        for binder in binders {
            self.elab.step(1)?;
            if self.elab.written_default(owner, binder).is_some() {
                self.elab.reserve(1)?;
                defaulted.insert(binder.position);
            }
        }
        let mandatory = self.necessary_reads(&producers, slots, &defaulted)?;
        receiving_reads.clear();
        for (binder, slot) in binders.iter().zip(slots) {
            self.elab.step(1)?;
            if slot.is_some() {
                continue;
            }
            let position = binder.position;
            if !self.default_is_ready(position, &producers, &mandatory)? {
                continue;
            }
            let Some(default) = self.elab.written_default(owner, binder) else {
                self.elab.reserve(1)?;
                receiving_reads.insert(position);
                continue;
            };
            let earlier = &slots[..position];
            if !self.elab.default_dependencies_ready(
                owner,
                default.node,
                &binders[..position],
                earlier,
            )? {
                continue;
            }
            let Some(argument) = self
                .elab
                .elaborate_default(owner, binders, &default, earlier, 1)?
            else {
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(None);
            };
            return Ok(Some((
                Substitution {
                    binder: default.binder,
                    position,
                    argument,
                },
                ArgumentSource {
                    position,
                    site: Self::site(owner, default.node),
                    origin: ArgumentOrigin::Default,
                },
            )));
        }
        Ok(None)
    }
}
