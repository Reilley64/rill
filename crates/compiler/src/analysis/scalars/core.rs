//! Core-dependent checking over admitted identities; absent contracts stay pending.
use super::*;

mod errors;
pub(super) use errors::ClosedErr;

#[derive(Clone, Copy, Debug)]
pub enum PropagationTarget<'g> {
    Callable(&'g DeclarationId),
    Arrow(NodeId),
}

#[derive(Clone, Debug)]
pub struct PropagationError<'g> {
    /// The existing wrapper type constructor, not a newly constructed error.
    pub wrapper: &'g DeclarationId,
    pub payload: Type<'g>,
}

#[derive(Clone, Debug)]
pub struct Propagation<'g> {
    pub target: PropagationTarget<'g>,
    pub success: &'g DeclarationId,
    pub failure: &'g DeclarationId,
    pub can_continue: bool,
    pub can_return: bool,
    pub error: Option<PropagationError<'g>>,
}

pub(super) struct OptionOperand<'g> {
    pub(super) payload: Type<'g>,
    pub(super) access: data::Access,
    pub(super) truth: BooleanSet,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn option_guard_operand(
        &self,
        value: &ExpressionResult<'g>,
    ) -> Option<OptionOperand<'g>> {
        let role = self.option?;
        let (ty, access) = match value.ty.as_ref() {
            TypeTerm::Reference { mutable, referent } => (
                referent.as_ref(),
                if *mutable && value.access != data::Access::ReadOnly {
                    data::Access::Writable
                } else {
                    data::Access::ReadOnly
                },
            ),
            ty => (ty, value.access),
        };
        let (container, variant) = match ty {
            TypeTerm::Variant {
                parent,
                declaration,
            } => (parent.as_ref(), Some(*declaration)),
            container => (container, None),
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = container
        else {
            return None;
        };
        if *declaration != &role.declaration.id {
            return None;
        }
        let truth = if !value.normal {
            BooleanSet::EMPTY
        } else if variant == Some(&role.some.id) {
            BooleanSet::TRUE
        } else if variant == Some(&role.none.id) {
            BooleanSet::FALSE
        } else {
            BooleanSet::BOTH
        };
        Some(OptionOperand {
            payload: arguments[0].clone(),
            access,
            truth,
        })
    }

    pub(super) fn propagation_target<'a>(
        &self,
        context: &'a Context<'r, 'g>,
    ) -> Option<(PropagationTarget<'g>, Option<&'a Type<'g>>)> {
        if let Some(arrow) = &context.arrow_return {
            Some((
                PropagationTarget::Arrow(arrow.node),
                arrow.expected.as_ref(),
            ))
        } else {
            let signature = context.signature?;
            let DeclarationShape::Function { result, .. } = &signature.shape else {
                unreachable!()
            };
            Some((
                PropagationTarget::Callable(&signature.declaration.id),
                Some(result),
            ))
        }
    }

    pub(super) fn postfix_propagation(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let operand = inference.module.syntax.nodes[node].children[0];
        let slot = &inference.slots[&operand];
        let (Some(ty), Some((target, Some(result)))) =
            (slot.ty.clone(), self.propagation_target(context))
        else {
            return Ok(());
        };
        if inference.slots[&node].propagation.is_some() || slot.blocked {
            return Ok(());
        }
        if let Some(payload) =
            self.resolve_propagation(inference, node, (&ty, slot.access), (target, result))?
        {
            self.establish(inference, node, payload, false)?;
        }
        Ok(())
    }

    pub(super) fn pipe_propagation_output(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        ty: &Type<'g>,
    ) -> Work<Option<Type<'g>>> {
        if (self.option.is_none() && self.result.is_none()) || inference.slots[&node].blocked {
            return Ok(None);
        }
        let Some((target, Some(result))) = inference.slots[&node].pipe_target.clone() else {
            return Ok(None);
        };
        self.resolve_propagation(
            inference,
            node,
            (ty, data::Access::Owned),
            (target, &result),
        )
    }

    fn resolve_propagation(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        (ty, access): (&Type<'g>, data::Access),
        (target, result): (PropagationTarget<'g>, &Type<'g>),
    ) -> Work<Option<Type<'g>>> {
        let module = inference.module;
        let (access, container) = match ty.as_ref() {
            TypeTerm::Reference { referent, .. } => (data::Access::ReadOnly, referent.as_ref()),
            container => (access, container),
        };
        let (container, variant) = match container {
            TypeTerm::Variant {
                parent,
                declaration,
            } => (parent.as_ref(), Some(*declaration)),
            container => (container, None),
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = container
        else {
            if !matches!(container, TypeTerm::Parameter { .. })
                && primitive_name(ty) != Some("never")
            {
                self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
            return Ok(None);
        };
        let (success, failure, error) = if let Some(role) = self.option
            && *declaration == &role.declaration.id
        {
            (&role.some.id, &role.none.id, None)
        } else if let Some(role) = self.result
            && *declaration == &role.declaration.id
        {
            (
                &role.ok.id,
                &role.err.id,
                Some(PropagationError {
                    wrapper: &role.wrapper.id,
                    payload: arguments[1].clone(),
                }),
            )
        } else {
            return Ok(None);
        };
        let TypeTerm::Nominal {
            declaration: returned,
            arguments: returned_args,
        } = result.as_ref()
        else {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(None);
        };
        if access != data::Access::Owned || declaration != returned {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(None);
        }
        if let Some(error) = &error
            && !self.same(&error.payload, &returned_args[1])?
        {
            self.mismatch(module, node, &error.payload, &returned_args[1])?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(None);
        }
        let payload = arguments[0].clone();
        let propagation = Propagation {
            target,
            success,
            failure,
            can_continue: variant != Some(failure),
            can_return: variant != Some(success),
            error,
        };
        self.elab.reserve(1)?;
        inference.slots.get_mut(&node).unwrap().propagation = Some(propagation);
        Ok(Some(payload))
    }

    pub(super) fn propagating_pipe(module: &Module, node: NodeId) -> bool {
        let syntax = &module.syntax.nodes[node];
        syntax.kind == NodeKind::Pipe
            && module.syntax.nodes[syntax.children[1]].kind
                == NodeKind::PipeStep { propagate: true }
    }

    pub(super) fn valid_propagation_target(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<bool> {
        let valid = self.propagation_target(context).is_some_and(|(_, result)| {
            result.is_none_or(|ty| {
                matches!(
                    ty.as_ref(),
                    TypeTerm::Nominal { .. } | TypeTerm::Parameter { .. }
                )
            })
        });
        if !valid {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
        }
        Ok(valid)
    }

    pub(super) fn prepare_pipe(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
        stack: &mut Vec<NodeId>,
    ) -> Work<()> {
        let module = inference.module;
        let nodes = &module.syntax.nodes;
        let children = &nodes[node].children;
        let (input, step) = (children[0], &nodes[children[1]]);
        let callee = step.children[0];
        let arguments = step
            .children
            .get(1)
            .map_or(&[][..], |&args| &nodes[args].children);
        self.elab.reserve(1 + arguments.len())?;
        let mut bound = Vec::with_capacity(1 + arguments.len());
        let mut inserted = false;
        for &arg in arguments {
            self.elab.step(1)?;
            let placeholder = nodes[arg].kind == NodeKind::PipePlaceholder;
            inserted |= placeholder;
            bound.push(if placeholder { input } else { arg });
        }
        if !inserted {
            self.elab.step(bound.len())?;
            bound.insert(0, input);
        }
        let invocation = self.value_callee(module, callee, context)?;
        if Self::propagating_pipe(module, node)
            && self.valid_propagation_target(module, children[1], context)?
        {
            inference.slots.get_mut(&node).unwrap().pipe_target = self
                .propagation_target(context)
                .map(|(target, result)| (target, result.cloned()));
        }
        if invocation {
            self.elab.reserve(2 + bound.len())?;
            inference.value_calls.insert(node);
            inference.pipe_invocations.insert(
                node,
                PipeInvocation {
                    receiver: callee,
                    arguments: Arc::from(bound.as_slice()),
                },
            );
        } else if let Some(mut call) =
            self.prepare_named_call(module, node, callee, &bound, context)?
        {
            call.fact.pipe_input = Some(input);
            self.elab.reserve(1)?;
            inference.calls.insert(node, call);
        } else {
            inference.slots.get_mut(&node).unwrap().blocked = true;
        }
        for arg in bound.into_iter().chain(invocation.then_some(callee)) {
            self.elab.reserve(2)?;
            inference.parents.insert(arg, node);
            stack.push(arg);
        }
        Ok(())
    }

    pub(super) fn guard_failure(
        &mut self,
        module: &'g Module,
        node: NodeId,
        failure: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        let flow = self.statement(module, failure, depth, context)?;
        if flow.normal && !flow.pending {
            self.finding(module, node, ScalarFindingKind::GuardFallthrough)?;
        }
        Ok(flow)
    }

    fn pending_binder(
        &mut self,
        declaration: &'g Declaration,
        context: &mut Context<'r, 'g>,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        context
            .scopes
            .last_mut()
            .unwrap()
            .entry(declaration.name.as_deref().unwrap())
            .or_insert(Local {
                declaration: &declaration.id,
                ty: None,
                mutable: false,
                boolean: None,
            });
        Ok(())
    }

    pub(super) fn pending_pattern(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &mut Context<'r, 'g>,
    ) -> Work<()> {
        let pattern = &module.syntax.nodes[node].span;
        self.elab
            .step(module.declarations.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        let start = module
            .declarations
            .partition_point(|d| d.id.start < pattern.start());
        for declaration in &module.declarations[start..] {
            self.elab.step(1)?;
            if declaration.id.start >= pattern.end() {
                break;
            }
            if declaration.id.kind == DeclarationKind::Binding {
                self.pending_binder(declaration, context)?;
            }
        }
        Ok(())
    }

    pub(super) fn integrated_guard_binding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        let children = &module.syntax.nodes[node].children;
        let binder = &module.syntax.nodes[children[0]];
        let pattern = binder.children[0];
        let operand = *binder.children.last().unwrap();
        let annotation = if binder.children.len() == 3 {
            self.elab.ty(
                module,
                binder.children[1],
                context.signature.map_or(&[], |s| &s.binders),
                1,
            )?
        } else {
            None
        };
        let input = self.expression(module, operand, None, context)?;
        let option = input
            .as_ref()
            .and_then(|value| self.option_guard_operand(value));
        self.elab.reserve(1)?;
        context.scopes.push(BTreeMap::new());
        if children.len() == 3 {
            let error = self.elab.declaration_at(module, children[1])?;
            if option.is_some() {
                self.finding(module, children[1], ScalarFindingKind::InvalidTarget)?;
            }
            self.pending_binder(error, context)?;
        }
        let failure =
            self.guard_failure(module, node, *children.last().unwrap(), depth + 1, context)?;
        context.scopes.pop();
        let mut flow = input.map_or_else(Flow::normal, |e| e.flow);
        if let Some(option) = option {
            if let Some(expected) = annotation
                && !self.same(&option.payload, &expected)?
            {
                self.mismatch(module, binder.children[1], &option.payload, &expected)?;
            }
            let Some(pattern_truth) =
                self.guard_payload_pattern(module, pattern, operand, &option, depth + 1, context)?
            else {
                flow.pending = true;
                return Ok(flow);
            };
            flow.normal = option.truth.can_be_true() && pattern_truth.can_be_true();
            if option.truth.can_be_false()
                || option.truth.can_be_true() && pattern_truth.can_be_false()
            {
                flow = self.union(flow, &failure)?;
            }
        } else {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            self.pending_pattern(module, pattern, context)?;
        }
        Ok(flow)
    }
}
