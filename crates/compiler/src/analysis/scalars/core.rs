//! Typed checking of independent source under unavailable core contracts.
use super::*;

impl<'r, 'g> Checker<'r, 'g> {
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
        if step.kind == (NodeKind::PipeStep { propagate: true }) {
            if context.signature.is_none() && context.arrow_return.is_none() {
                self.finding(module, children[1], ScalarFindingKind::InvalidTarget)?;
            } else {
                self.defer(module, children[1], ScalarUnsupported::CoreAdmission)?;
            }
            inference.slots.get_mut(&node).unwrap().blocked = true;
        } else if invocation {
            self.defer(module, node, ScalarUnsupported::CallableInvocation)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
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
        let before_failure = self.marks();
        let failure = self.statement(module, *children.last().unwrap(), depth + 1, context)?;
        context.scopes.pop();
        if failure.normal && before_failure == self.marks() {
            self.finding(module, node, ScalarFindingKind::GuardFallthrough)?;
        }
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
