//! Core-dependent checking over admitted identities; absent contracts stay pending.
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum PropagationTarget<'g> {
    Callable(&'g DeclarationId),
    Arrow(NodeId),
}

#[derive(Clone, Copy, Debug)]
pub struct Propagation<'g> {
    pub target: PropagationTarget<'g>,
    pub success: &'g DeclarationId,
    pub failure: &'g DeclarationId,
    pub can_continue: bool,
    pub can_return: bool,
}

impl<'r, 'g> Checker<'r, 'g> {
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

    pub(super) fn option_propagation(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let module = inference.module;
        let operand = module.syntax.nodes[node].children[0];
        let slot = &inference.slots[&operand];
        let (Some(role), Some(ty), Some((target, Some(result)))) = (
            self.option,
            slot.ty.as_ref(),
            self.propagation_target(context),
        ) else {
            return Ok(());
        };
        if inference.slots[&node].propagation.is_some() || slot.blocked {
            return Ok(());
        }
        let (container, variant) = match ty.as_ref() {
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
            return Ok(());
        };
        if *declaration != &role.declaration.id {
            // Result and its contracts have not yet been admitted.
            return Ok(());
        }
        if slot.access != data::Access::Owned
            || !matches!(result.as_ref(), TypeTerm::Nominal { declaration, .. } if *declaration == &role.declaration.id)
        {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let payload = arguments[0].clone();
        let propagation = Propagation {
            target,
            success: &role.some.id,
            failure: &role.none.id,
            can_continue: variant != Some(&role.none.id),
            can_return: variant != Some(&role.some.id),
        };
        self.elab.reserve(1)?;
        inference.slots.get_mut(&node).unwrap().propagation = Some(propagation);
        self.establish(inference, node, payload, false)
    }

    pub(super) fn propagating_pipe(module: &Module, node: NodeId) -> bool {
        let syntax = &module.syntax.nodes[node];
        syntax.kind == NodeKind::Pipe
            && module.syntax.nodes[syntax.children[1]].kind
                == NodeKind::PipeStep { propagate: true }
    }

    pub(super) fn pending_propagation(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let Some((_, result)) = self.propagation_target(context) else {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            return Ok(());
        };
        if result.is_some_and(|ty| {
            !matches!(
                ty.as_ref(),
                TypeTerm::Nominal { .. } | TypeTerm::Parameter { .. }
            )
        }) {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
        } else {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
        }
        Ok(())
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
        if Self::propagating_pipe(module, node) {
            self.pending_propagation(module, children[1], context)?;
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

    fn pending_pattern(
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

    pub(super) fn pending_guard_binding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        let children = &module.syntax.nodes[node].children;
        let binder = &module.syntax.nodes[children[0]];
        if binder.children.len() == 3 {
            self.elab.ty(
                module,
                binder.children[1],
                context.signature.map_or(&[], |s| &s.binders),
                1,
            )?;
        }
        let input = self.expression(module, *binder.children.last().unwrap(), None, context)?;
        self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
        self.elab.reserve(1)?;
        context.scopes.push(BTreeMap::new());
        if children.len() == 3 {
            let error = self.elab.declaration_at(module, children[1])?;
            self.pending_binder(error, context)?;
        }
        self.guard_failure(module, node, *children.last().unwrap(), depth + 1, context)?;
        context.scopes.pop();
        self.pending_pattern(module, binder.children[0], context)?;
        Ok(input.map_or_else(Flow::normal, |e| e.flow))
    }

    pub(super) fn pending_iteration(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        let children = &module.syntax.nodes[node].children;
        let source = self.expression(module, children[1], None, context)?;
        self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
        self.elab.reserve(2)?;
        context.scopes.push(BTreeMap::new());
        context.loops.push(node);
        self.pending_pattern(module, children[0], context)?;
        self.statement(module, children[2], depth + 1, context)?;
        context.loops.pop();
        context.scopes.pop();
        Ok(source.map_or_else(Flow::normal, |e| e.flow))
    }
}
