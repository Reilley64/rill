//! A private once or read-only wrapper can defer one environment check to its actual calls.
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

/// How the written actual reaches the wrapper's formal.
#[derive(Clone, Copy)]
pub(super) enum Actual {
    /// The arrow itself moves into an owned `callableOnce` formal.
    Moved(NodeId),
    /// `&name` lends a local descriptor to a `&F` formal with a `callable`
    /// requirement. The descriptor stays in its caller's residence.
    Lent(NodeId),
}

impl Actual {
    fn predicate(self) -> &'static str {
        match self {
            Self::Moved(_) => "callableOnce",
            Self::Lent(_) => "callable",
        }
    }
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn once_call(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        call: &CallFacts<'g>,
        states: &[Residence<'g>],
    ) -> Work<Option<(Route<'g>, NodeId, Actual)>> {
        let syntax = &self.module.syntax.nodes;
        if let [_, written] = syntax[node].children.as_slice()
            && syntax[*written].kind == NodeKind::Prefix("&")
        {
            return self.lent_call(node, value, call, states);
        }
        let ([argument], [substitution]) =
            (call.arguments.as_slice(), call.substitution.as_slice())
        else {
            return Ok(None);
        };
        let [_, written] = syntax[node].children.as_slice() else {
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
        // The pair's type is the substituted `F`; its result is the call's.
        self.close_route(
            node,
            call,
            signature,
            substitution,
            &value.ty,
            Actual::Moved(*argument),
        )
    }

    /// `return run(&f)` where `run<F>(job: &F): bool where callable(F, [], bool)`
    /// only invokes its borrowed formal and `f` is a local read-only descriptor.
    fn lent_call(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        call: &CallFacts<'g>,
        states: &[Residence<'g>],
    ) -> Work<Option<(Route<'g>, NodeId, Actual)>> {
        let syntax = &self.module.syntax.nodes;
        let ([argument], [substitution]) =
            (call.arguments.as_slice(), call.substitution.as_slice())
        else {
            return Ok(None);
        };
        let [_, written] = syntax[node].children.as_slice() else {
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
            || !matches!(syntax[*argument].children.as_slice(), [operand]
                if syntax[*operand].kind == NodeKind::ValueReference)
            || !matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            || !matches!(substitution.argument.as_ref(), TypeTerm::Callable { mode: InvocationMode::ReadOnly, parameters, result }
                if parameters.is_empty() && *result == value.ty)
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
        // Only an unprojected local binding: a formal or field has no actual
        // arrow. `&` gives the argument the descriptor's type behind `&F`.
        let Some(descriptor) = self.expression_facts(syntax[*argument].children[0])? else {
            return Ok(None);
        };
        if descriptor.ty != substitution.argument
            || !descriptor.path.as_ref().is_some_and(|path| {
                path.root.kind == DeclarationKind::Binding && path.projections.is_empty()
            })
        {
            return Ok(None);
        }
        // Screen each staged residence before summarizing the wrapper body.
        for state in states {
            if self.lent_descriptor(state, *argument)?.is_none() {
                return Ok(None);
            }
        }
        self.close_route(
            node,
            call,
            signature,
            substitution,
            &value.ty,
            Actual::Lent(*argument),
        )
    }

    /// Bind the screened call to its wrapper body and to the one proved
    /// requirement discharge at this call.
    fn close_route(
        &mut self,
        node: NodeId,
        call: &CallFacts<'g>,
        signature: &DeclarationFacts<'g>,
        substitution: &Substitution<'g>,
        output: &Type<'g>,
        actual: Actual,
    ) -> Work<Option<(Route<'g>, NodeId, Actual)>> {
        let syntax = &self.module.syntax.nodes;
        self.budget.step(1)?;
        if !self.checked.contains(call.callee) {
            return Ok(None);
        }
        let Some(name) = direct_name(syntax, syntax[node].children[0]) else {
            return Ok(None);
        };
        let Some((route, requirement)) =
            self.wrapper_body(signature, substitution, output, actual)?
        else {
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
                && callable_goal(
                    goal,
                    actual.predicate(),
                    GoalState::Proved,
                    &substitution.argument,
                    output,
                )
            {
                if discharged || !goal.assumptions.is_empty() {
                    return Ok(None);
                }
                discharged = true;
            }
        }
        Ok(discharged.then_some((route, name, actual)))
    }

    fn wrapper_body(
        &mut self,
        signature: &DeclarationFacts<'g>,
        substitution: &Substitution<'g>,
        output: &Type<'g>,
        actual: Actual,
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
            // The requirement's subject: an owned `F`, or the `F` behind `&F`.
            let subject = match (actual, formal.ty.as_ref()) {
                (Actual::Moved(_), _) => &formal.ty,
                (
                    Actual::Lent(_),
                    TypeTerm::Reference {
                        mutable: false,
                        referent,
                    },
                ) => referent,
                (Actual::Lent(_), _) => return None,
            };
            if binder.position != 0
                || substitution.position != 0
                || *substitution.binder != binder.declaration.id
                || output != result
                || !matches!(subject.as_ref(), TypeTerm::Parameter { declaration, position: 0 } if **declaration == binder.declaration.id)
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
                self.table
                    .and_then(|table| table.get(&node))
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
            // A lent formal is invoked through its borrow, never consumed.
            let (mode, access, through_borrow) = match actual {
                Actual::Moved(_) => (InvocationMode::Once, Access::Owned, false),
                Actual::Lent(_) => (InvocationMode::ReadOnly, Access::ReadOnly, true),
            };
            if syntax[*invocation].children.as_slice() != [invoke.receiver]
                || direct_name(syntax, invoke.receiver).is_none()
                || invoke.mode != mode
                || invoke.access != access
                || *path.root != formal.declaration.id
                || !match path.projections.as_slice() {
                    [] => !through_borrow,
                    [PathProjection::Dereference] => through_borrow,
                    _ => false,
                }
                || (!through_borrow && receiver.access != Access::Owned)
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
                || !callable_goal(
                    goal,
                    actual.predicate(),
                    GoalState::Assumption,
                    subject,
                    result,
                )
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

fn callable_goal(
    goal: &GoalFacts<'_>,
    predicate: &str,
    state: GoalState,
    receiver: &Type<'_>,
    output: &Type<'_>,
) -> bool {
    goal.state == state
        && goal.premise.is_none()
        && goal.hypothetical.is_none()
        && matches!(&goal.judgment, GoalJudgment::Requirement { target: RequirementTarget::Predicate(id), arguments }
            if id.name() == predicate && matches!(arguments.as_slice(), [job, args, result]
                if job == receiver && result == output && matches!(args.as_ref(), TypeTerm::Tuple(args) if args.is_empty())))
}
