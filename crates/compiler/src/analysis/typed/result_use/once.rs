//! A private once wrapper can defer one environment check to its actual calls.
use super::*;
use crate::analysis::InvocationMode;
use crate::analysis::generics::{GoalFacts, GoalJudgment, GoalState, callables::CaptureForm};
use crate::analysis::ordinary::RequirementTarget;
use crate::analysis::scalars::CallFacts;
use crate::loader::{Origin, Target};

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Route<'g> {
    declaration: &'g DeclarationId,
    name: NodeId,
    invocation: NodeId,
}

pub(super) type ClosedCalls<'g> = BTreeMap<Route<'g>, BTreeMap<NodeId, bool>>;

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn once_call(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        call: &CallFacts<'g>,
    ) -> Work<Option<(Route<'g>, NodeId, NodeId)>> {
        let syntax = &self.module.syntax.nodes;
        let ([argument], [substitution]) =
            (call.arguments.as_slice(), call.substitution.as_slice())
        else {
            return Ok(None);
        };
        let [callee, written] = syntax[node].children.as_slice() else {
            return Ok(None);
        };
        if self.once_return != Some(node)
            || self.proving_callable
            || self.dead_depth != 0
            || self.module.key.origin != Origin::Application
            || call.callee.module != self.module.key
            || call.callee.kind != DeclarationKind::Function
            || call.selection.is_some()
            || call.pipe_input.is_some()
            || !call.defaults.is_empty()
            || syntax[node].kind != NodeKind::Call
            || written != argument
            || syntax[*argument].kind != NodeKind::Arrow
            || !shared_bool_pair(&value.ty)
        {
            return Ok(None);
        }
        let body_node = syntax[*argument].children[1];
        let body = &syntax[body_node];
        if body.kind != NodeKind::Bracket
            || !matches!(body.children.as_slice(), [left, right]
                if syntax[*left].kind == NodeKind::ValueReference
                    && syntax[*right].kind == NodeKind::ValueReference)
        {
            return Ok(None);
        }
        // Filter unrelated source bodies before opening typed evidence.
        let Some(signature) = self.signature(call.callee) else {
            return Ok(None);
        };
        let declaration = signature.declaration;
        if declaration.public
            || declaration.scope.is_some()
            || !matches!(syntax[declaration.node].children.as_slice(), [_, body]
                if syntax[*body].children.len() == 1)
        {
            return Ok(None);
        }
        // Only the direct carried shared/primitive pair is admitted here. In
        // particular, a block with owned Result captures is not a pair proof.
        let Some(actual) = self.facts(*argument)? else {
            return Ok(None);
        };
        let Some(arrow) = actual.callable.as_ref().and_then(|f| f.arrow.as_deref()) else {
            return Ok(None);
        };
        let [a, b] = arrow.captures.as_slice() else {
            return Ok(None);
        };
        let carried = CaptureForm::CarriedReference { mutable: false };
        if !matches!((a.form, b.form), (CaptureForm::Value, form) | (form, CaptureForm::Value) if form == carried)
            || arrow.site.module != &self.module.key
            || arrow.site.node != *argument
            || arrow.premise.is_some()
            || arrow.mode != Some(InvocationMode::ReadOnly)
            || !arrow.parameters.is_empty()
            || arrow.result != value.ty
            || !self.primitive_capture_pair(*argument, arrow)?
        {
            return Ok(None);
        }
        let Some(actual) = expression(actual) else {
            return Ok(None);
        };
        if actual.access != Access::Owned
            || !matches!(actual.ty.as_ref(), TypeTerm::Callable { mode: InvocationMode::ReadOnly, parameters, result }
            if parameters.is_empty() && result == &arrow.result)
            || actual.ty != substitution.argument
            || !self
                .expression_facts(body_node)?
                .is_some_and(|body| body.ordinary_context.is_none() && body.ty == arrow.result)
        {
            return Ok(None);
        }
        self.budget.step(1)?;
        if !self.checked.contains(call.callee) {
            return Ok(None);
        }
        let Some(name) = direct_name(syntax, *callee) else {
            return Ok(None);
        };
        let Some((route, requirement)) = self.once_body(signature, substitution, &value.ty)? else {
            return Ok(None);
        };
        self.budget
            .step(syntax[name].span.end() - syntax[name].span.start())?;
        if syntax[name].span.bytes() != syntax[route.name].span.bytes() {
            return Ok(None);
        }
        let mut discharged = false;
        for goal in self.bodies.goals() {
            self.budget.step(1)?;
            if goal.site.module == &self.module.key
                && goal.site.node == node
                && goal.source.module == &self.module.key
                && goal.source.node == requirement
                && callable_once(goal, GoalState::Proved, &actual.ty, &arrow.result)
            {
                if discharged || !goal.assumptions.is_empty() {
                    return Ok(None);
                }
                discharged = true;
            }
        }
        Ok(discharged.then_some((route, name, *argument)))
    }

    fn once_body(
        &mut self,
        signature: &DeclarationFacts<'g>,
        substitution: &Substitution<'g>,
        output: &Type<'g>,
    ) -> Work<Option<(Route<'g>, NodeId)>> {
        self.budget.step(4)?;
        let id = &signature.declaration.id;
        let syntax = &self.module.syntax.nodes;
        Ok((|| {
            let [binder] = signature.binders.as_slice() else {
                return None;
            };
            let DeclarationShape::Function { parameters, result } = &signature.shape else {
                return None;
            };
            let [formal] = parameters.as_slice() else {
                return None;
            };
            if binder.position != 0
                || substitution.position != 0
                || *substitution.binder != binder.declaration.id
                || output != result
                || !matches!(formal.ty.as_ref(), TypeTerm::Parameter { declaration, position: 0 } if **declaration == binder.declaration.id)
            {
                return None;
            }
            let metadata = self.inventory.headers().get(id)?;
            let [requirement] = metadata.requirements.as_slice() else {
                return None;
            };
            if !metadata.defaults.is_empty() {
                return None;
            }
            let [head, body] = syntax[signature.declaration.node].children.as_slice() else {
                return None;
            };
            let name = *syntax[*head].children.first()?;
            let [returned] = syntax[*body].children.as_slice() else {
                return None;
            };
            let [invocation] = syntax[*returned].children.as_slice() else {
                return None;
            };
            let fact = |node| {
                self.nodes
                    .get(&(&self.module.key, node))
                    .map(|&(_, fact)| fact)
            };
            if syntax[*body].kind != NodeKind::Block
                || syntax[*returned].kind != NodeKind::Return
                || !matches!(fact(*returned)?.kind, NodeFactKind::Return { target } if target == id)
                || syntax[*invocation].kind != NodeKind::Call
            {
                return None;
            }
            let called = fact(*invocation)?;
            let invoke = called.callable.as_ref()?.invocation.as_ref()?;
            let path = invoke.path.as_ref()?;
            let receiver = expression(fact(invoke.receiver)?)?;
            if syntax[*invocation].children.as_slice() != [invoke.receiver]
                || direct_name(syntax, invoke.receiver).is_none()
                || invoke.mode != InvocationMode::Once
                || invoke.access != Access::Owned
                || *path.root != formal.declaration.id
                || !path.projections.is_empty()
                || receiver.access != Access::Owned
                || receiver.ty != formal.ty
                || expression(called)?.ty != *result
            {
                return None;
            }
            let goal = self.bodies.goal(invoke.assumption.as_ref()?).ok()?;
            if goal.source.module != &self.module.key
                || goal.source.node != requirement.node
                || goal.site.module != &self.module.key
                || goal.site.node != *body
                || !callable_once(goal, GoalState::Assumption, &formal.ty, result)
            {
                return None;
            }
            Some((
                Route {
                    declaration: id,
                    name,
                    invocation: *invocation,
                },
                requirement.node,
            ))
        })())
    }

    pub(super) fn record_once_call(
        &mut self,
        route: Route<'g>,
        name: NodeId,
        proved: bool,
    ) -> Work<()> {
        self.budget.reserve(2)?;
        self.once_calls
            .entry(route)
            .or_default()
            .entry(name)
            .and_modify(|closed| *closed &= proved)
            .or_insert(proved);
        Ok(())
    }

    pub(super) fn finish_once_calls(&mut self) -> Work<()> {
        for (route, sites) in std::mem::take(&mut self.once_calls) {
            let nodes = &self.module.syntax.nodes;
            let name = nodes[route.name].span.bytes();
            let mut closed = true;
            for binding in self.inventory.graph().bindings() {
                self.budget.step(1)?;
                if matches!(&binding.target, Target::Declaration(id) if id == route.declaration)
                    && (binding.public
                        || binding.module != self.module.key
                        || binding.import.is_some())
                {
                    closed = false;
                    break;
                }
            }
            if !closed {
                continue;
            }
            // Check the captured syntax too: a typed-node walk alone could miss
            // a dead call, deferred function value, or an untyped reference.
            for (node, written) in nodes.iter().enumerate() {
                self.budget.step(1)?;
                if written.kind == NodeKind::Name
                    && node != route.name
                    && written.span.end() - written.span.start() == name.len()
                {
                    self.budget.step(name.len())?;
                    if written.span.bytes() == name && sites.get(&node) != Some(&true) {
                        closed = false;
                        break;
                    }
                }
            }
            if closed {
                // The ordinary body walk already checked this root and its
                // return. Resolve only the deferred receiver at that exact site.
                self.budget.step(self.report.pending.len())?;
                if let Some(index) = self.report.pending.iter().position(|pending| {
                    pending.site.module == &self.module.key
                        && pending.site.node == route.invocation
                        && pending.blocker == ResultUseBlocker::CallBoundary
                }) {
                    self.report.pending.remove(index);
                }
            }
        }
        Ok(())
    }
}

fn direct_name(syntax: &[crate::parser::Node], value: NodeId) -> Option<NodeId> {
    let [path] = syntax[value].children.as_slice() else {
        return None;
    };
    let [name] = syntax[*path].children.as_slice() else {
        return None;
    };
    (syntax[value].kind == NodeKind::ValueReference
        && syntax[*path].kind == NodeKind::QualifiedName
        && syntax[*name].kind == NodeKind::Name)
        .then_some(*name)
}

fn expression<'a, 'g>(fact: &'a NodeFacts<'g>) -> Option<&'a ExpressionFacts<'g>> {
    match &fact.kind {
        NodeFactKind::Expression(value) if value.ordinary_context.is_none() => Some(value),
        _ => None,
    }
}

fn shared_bool_pair(ty: &Type<'_>) -> bool {
    let TypeTerm::Tuple(elements) = ty.as_ref() else {
        return false;
    };
    matches!(elements.as_slice(), [shared, scalar]
        if matches!(shared.as_ref(), TypeTerm::Reference { mutable: false, referent }
            if matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
        && matches!(scalar.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
}

fn callable_once(
    goal: &GoalFacts<'_>,
    state: GoalState,
    receiver: &Type<'_>,
    output: &Type<'_>,
) -> bool {
    goal.state == state
        && goal.premise.is_none()
        && goal.hypothetical.is_none()
        && matches!(&goal.judgment, GoalJudgment::Requirement { target: RequirementTarget::Predicate(id), arguments }
            if id.name() == "callableOnce" && matches!(arguments.as_slice(), [job, args, result]
                if job == receiver && result == output && matches!(args.as_ref(), TypeTerm::Tuple(args) if args.is_empty())))
}
