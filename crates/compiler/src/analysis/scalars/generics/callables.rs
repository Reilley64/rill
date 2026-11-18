//! Typed callable source facts. No acquisition, transfer or loan checking.
use super::*;
use std::{cell::RefCell, rc::Rc};

mod captures;
pub use captures::{ArrowFacts, CaptureFact, CaptureForm};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::scalars) struct CallableContext<'g> {
    pub parameters: Vec<Option<Type<'g>>>,
    pub result: Option<Type<'g>>,
}

#[derive(Debug)]
pub struct CallableNodeFacts<'g> {
    pub invocation: Option<Arc<InvocationFacts<'g>>>,
    pub function_value: Option<Arc<FunctionValueFacts<'g>>>,
    /// Known arrow origin; an erased callable without a known origin has none.
    pub arrow: Option<Arc<ArrowFacts<'g>>>,
}

#[derive(Debug)]
pub struct InvocationFacts<'g> {
    pub receiver: NodeId,
    pub mode: InvocationMode,
    pub access: data::Access,
    pub path: Option<data::PlacePath<'g>>,
    pub assumption: Option<GoalId>,
}

#[derive(Debug)]
pub struct FunctionValueFacts<'g> {
    pub declaration: &'g DeclarationId,
    pub bindings: Vec<&'g Binding>,
    pub substitution: Vec<Substitution<'g>>,
    pub defaults: Vec<DefaultArgumentUse<'g>>,
}

#[derive(Debug)]
pub(in crate::analysis::scalars) struct ArrowHead<'g> {
    pub(in crate::analysis::scalars) parameters: Vec<(&'g Declaration, Option<Type<'g>>)>,
    pub(in crate::analysis::scalars) result: Option<Type<'g>>,
    pub(super) reads_result: bool,
}

#[derive(Clone)]
pub(in crate::analysis::scalars) struct ArrowReturn<'g> {
    pub node: NodeId,
    expected: Option<Type<'g>>,
    contributions: Rc<RefCell<Vec<(NodeId, Type<'g>)>>>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn written_call_arguments(
        module: &Module,
        node: NodeId,
    ) -> &[NodeId] {
        module.syntax.nodes[node]
            .children
            .get(1)
            .map(|&id| &module.syntax.nodes[id].children[..])
            .unwrap_or(&[])
    }

    pub(in crate::analysis::scalars) fn explicit_callable_substitution(
        &mut self,
        module: &'g Module,
        site: NodeId,
        arguments: &[NodeId],
        signature: &DeclarationFacts<'g>,
        context: &Context<'r, 'g>,
    ) -> Work<Option<Vec<Substitution<'g>>>> {
        if arguments.len() > signature.binders.len() {
            self.finding(module, site, ScalarFindingKind::Arity)?;
            return Ok(None);
        }
        self.elab.reserve(arguments.len())?;
        let mut substitutions = Vec::new();
        for (binder, &argument) in signature.binders.iter().zip(arguments) {
            let Some(argument) = self.elab.ty(
                module,
                argument,
                context.signature.map_or(&[], |s| &s.binders),
                1,
            )?
            else {
                return Ok(None);
            };
            substitutions.push(Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument,
            });
        }
        Ok(Some(substitutions))
    }

    pub(in crate::analysis::scalars) fn register_function_value(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
        declaration: &'g Declaration,
        bindings: Vec<&'g Binding>,
    ) -> Work<()> {
        let module = inference.module;
        let ordinary = self.input.ordinary.unwrap();
        let family = ordinary.captured_family(&declaration.id);
        let reason = if declaration.id.kind != DeclarationKind::Function && family.is_none() {
            Some(ScalarUnsupported::FamilySelection)
        } else if family.is_none()
            && ordinary
                .headers()
                .get(&declaration.id)
                .is_some_and(|h| !h.requirements.is_empty())
        {
            Some(ScalarUnsupported::Requirements)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.deferred_declaration(module, node, reason, declaration, bindings)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let Some(signature) = family
            .map(|f| &f.header)
            .or_else(|| self.input.facts.declarations.get(&declaration.id))
        else {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::DeclarationPrerequisite,
                declaration,
                bindings,
            )?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        if self.signature_unavailable(signature)? {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::DeclarationPrerequisite,
                declaration,
                bindings,
            )?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let args = Self::written_call_arguments(module, node);
        if let Some(substitution) =
            self.explicit_callable_substitution(module, node, args, signature, context)?
        {
            self.elab.reserve(1)?;
            inference.function_values.insert(
                node,
                FunctionValueFacts {
                    declaration: &declaration.id,
                    bindings,
                    substitution,
                    defaults: Vec::new(),
                },
            );
        } else {
            inference.slots.get_mut(&node).unwrap().blocked = true;
        }
        Ok(())
    }

    pub(in crate::analysis::scalars) fn advance_function_values(
        &mut self,
        inference: &mut Inference<'g>,
        permit_defaults: bool,
    ) -> Work<bool> {
        self.elab.reserve(inference.function_values.len())?;
        let waiting: Vec<_> = inference.function_values.keys().copied().collect();
        for node in waiting {
            let mut fact = inference.function_values.remove(&node).unwrap();
            if inference.slots[&node].blocked {
                continue;
            }
            if fact.declaration.kind == DeclarationKind::Family {
                let formed = self.with_node_premise(inference, node, |checker, inference| {
                    if checker.conversion_family(fact.declaration).is_some() {
                        checker.conversion_value_step(inference, node, &fact, false)
                    } else {
                        checker.ordinary_value_step(inference, node, &fact)
                    }
                })?;
                if formed {
                    return Ok(true);
                }
                inference.function_values.insert(node, fact);
                continue;
            }
            if self.with_node_premise(inference, node, |checker, inference| {
                checker.function_value_step(inference, node, &mut fact, permit_defaults)
            })? {
                if !inference.slots[&node].blocked {
                    self.elab.reserve(1)?;
                    inference.slots.get_mut(&node).unwrap().function_value = Some(Arc::new(fact));
                }
                return Ok(true);
            }
            inference.function_values.insert(node, fact);
        }
        Ok(false)
    }

    fn function_value_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        fact: &mut FunctionValueFacts<'g>,
        permit_defaults: bool,
    ) -> Work<bool> {
        let signature = &self.input.facts.declarations[fact.declaration];
        let DeclarationShape::Function { parameters, result } = &signature.shape else {
            unreachable!()
        };
        let module = inference.module;
        let slot = &inference.slots[&node];
        let projected = inference.callable_contexts.get(&node);
        let (receiving_parameters, receiving_result) = match slot.expected.as_deref() {
            Some(TypeTerm::Callable {
                parameters, result, ..
            }) => (Some(parameters), Some(result)),
            _ => (None, None),
        };
        if receiving_parameters.is_some_and(|p| p.len() != parameters.len())
            || projected.is_some_and(|p| p.parameters.len() != parameters.len())
        {
            self.finding(module, node, ScalarFindingKind::Arity)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(true);
        }
        let binders = &signature.binders;
        self.elab.reserve(binders.len())?;
        let mut slots = vec![None; binders.len()];
        for written in &fact.substitution {
            slots[written.position] = Some(written.argument.clone());
        }
        let mut compatible = true;
        for (i, parameter) in parameters.iter().enumerate() {
            for actual in [
                receiving_parameters.map(|p| &p[i]),
                projected.and_then(|p| p.parameters[i].as_ref()),
            ]
            .into_iter()
            .flatten()
            {
                compatible &=
                    self.input_equation(&parameter.ty, actual, binders, &mut slots, &[], 1)?;
            }
        }
        for actual in [receiving_result, projected.and_then(|p| p.result.as_ref())]
            .into_iter()
            .flatten()
        {
            compatible &= self.input_equation(result, actual, binders, &mut slots, &[], 1)?;
        }
        if !compatible {
            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(true);
        }
        if permit_defaults
            && let Some(header) = self.input.ordinary.unwrap().headers().get(fact.declaration)
        {
            for default in &header.defaults {
                self.elab.step(binders.len())?;
                let Some(index) = binders
                    .iter()
                    .position(|b| b.declaration.id == *default.binder)
                else {
                    continue;
                };
                let Some(term) = &default.ty else { continue };
                if slots[index].is_none()
                    && self.closed(term, binders, &|i| slots[i].is_some(), 1)?
                {
                    let subs = self.slot_substitutions(binders, &slots)?;
                    slots[index] = Some(self.substitute(term, &subs, 1)?);
                    self.elab.reserve(1)?;
                    fact.defaults.push(DefaultArgumentUse {
                        binder: default.binder,
                        node: default.node,
                        span: default.span,
                    });
                }
            }
        }
        if slots.iter().any(Option::is_none) {
            fact.defaults.clear();
            return Ok(false);
        }
        fact.substitution = self.slot_substitutions(binders, &slots)?;
        self.elab.reserve(parameters.len() + 1)?;
        let parameters = parameters
            .iter()
            .map(|parameter| self.substitute(&parameter.ty, &fact.substitution, 1))
            .collect::<Work<Vec<_>>>()?;
        let result = self.substitute(result, &fact.substitution, 1)?;
        self.establish(
            inference,
            node,
            Arc::new(TypeTerm::Callable {
                mode: InvocationMode::ReadOnly,
                parameters,
                result,
            }),
            false,
        )?;
        Ok(true)
    }

    pub(in crate::analysis::scalars) fn register_arrow(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let module = inference.module;
        let head = &module.syntax.nodes[module.syntax.nodes[node].children[0]];
        let binders = context.signature.map_or(&[][..], |s| &s.binders);
        let mut parameters = Vec::new();
        for &parameter in &module.syntax.nodes[head.children[0]].children {
            let declaration = self.elab.declaration_at(module, parameter)?;
            let annotation = module.syntax.nodes[parameter].children.get(1);
            let ty = if let Some(&annotation) = annotation {
                let ty = self.elab.ty(module, annotation, binders, 1)?;
                if ty.is_none() {
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                }
                ty
            } else {
                None
            };
            self.elab.reserve(1)?;
            parameters.push((declaration, ty));
        }
        let result = if let Some(&annotation) = head.children.get(1) {
            let ty = self.elab.ty(module, annotation, binders, 1)?;
            if ty.is_none() {
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
            ty
        } else {
            None
        };
        let reads_result = result.is_none()
            && self.arrow_needs_width(module, module.syntax.nodes[node].children[1])?;
        self.elab.reserve(1)?;
        inference.arrows.insert(
            node,
            ArrowHead {
                parameters,
                result,
                reads_result,
            },
        );
        Ok(())
    }

    fn arrow_needs_width(&mut self, module: &'g Module, body: NodeId) -> Work<bool> {
        if module.syntax.nodes[body].kind != NodeKind::Block {
            return self.needs_numeric_width(module, body, 1);
        }
        self.elab.reserve(1)?;
        let mut stack = vec![body];
        let mut returns = false;
        while let Some(node) = stack.pop() {
            self.elab.step(1)?;
            let syntax = &module.syntax.nodes[node];
            match syntax.kind {
                NodeKind::Arrow => (),
                NodeKind::Return => {
                    let Some(&value) = syntax.children.first() else {
                        return Ok(false);
                    };
                    if !self.needs_numeric_width(module, value, 1)? {
                        return Ok(false);
                    }
                    returns = true;
                }
                _ => {
                    self.elab.reserve(syntax.children.len())?;
                    stack.extend(&syntax.children);
                }
            }
        }
        Ok(returns)
    }

    // Syntactic wait only: the normal worklist still establishes the type and
    // diagnostics, without a speculative check of the body at another width.
    pub(super) fn needs_numeric_width(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
    ) -> Work<bool> {
        self.equation_depth(depth)?;
        let syntax = &module.syntax.nodes[node];
        Ok(match syntax.kind {
            NodeKind::Literal => matches!(&self.token(module, node)?.kind,
                TokenKind::Number(n) if n.suffix.is_none()),
            NodeKind::Group | NodeKind::Prefix("-" | "~") => {
                self.needs_numeric_width(module, syntax.children[0], depth + 1)?
            }
            NodeKind::Binary("+" | "-" | "*" | "/" | "%" | "&" | "|" | "^") => {
                self.needs_numeric_width(module, syntax.children[0], depth + 1)?
                    && self.needs_numeric_width(module, syntax.children[1], depth + 1)?
            }
            NodeKind::Binary("<<" | ">>") => {
                self.needs_numeric_width(module, syntax.children[0], depth + 1)?
            }
            NodeKind::IfExpression => {
                self.needs_numeric_width(module, syntax.children[1], depth + 1)?
                    && self.needs_numeric_width(module, syntax.children[2], depth + 1)?
            }
            _ => false,
        })
    }

    pub(in crate::analysis::scalars) fn project_callable_context(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        mut projection: CallableContext<'g>,
    ) -> Work<()> {
        let node = self.source_leaf(inference.module, node)?;
        if let Some(previous) = inference.callable_contexts.get(&node) {
            if previous.parameters.len() != projection.parameters.len() {
                self.finding(inference.module, node, ScalarFindingKind::Arity)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
            for (old, new) in previous
                .parameters
                .iter()
                .zip(&mut projection.parameters)
                .chain(std::iter::once((&previous.result, &mut projection.result)))
            {
                self.elab.step(1)?;
                if let Some(old) = old {
                    if let Some(new) = new
                        && !self.same(old, new)?
                    {
                        self.mismatch(inference.module, node, old, new)?;
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                        return Ok(());
                    }
                    *new = Some(old.clone());
                }
            }
            if &projection == previous.as_ref() {
                return Ok(());
            }
        }
        self.elab.reserve(1)?;
        inference
            .callable_contexts
            .insert(node, Arc::new(projection));
        Ok(())
    }

    pub(in crate::analysis::scalars) fn advance_arrows(
        &mut self,
        inference: &mut Inference<'g>,
        context: &Context<'r, 'g>,
        out_of_context: bool,
    ) -> Work<bool> {
        self.elab.reserve(inference.arrows.len())?;
        let waiting: Vec<_> = inference.arrows.keys().copied().collect();
        for node in waiting {
            let slot = &inference.slots[&node];
            if slot.ty.is_some() || slot.blocked {
                continue;
            }
            let head = &inference.arrows[&node];
            if !out_of_context
                && head.reads_result
                && !matches!(slot.expected.as_deref(), Some(TypeTerm::Callable { .. }))
                && !inference
                    .callable_contexts
                    .get(&node)
                    .is_some_and(|c| c.result.is_some())
            {
                continue;
            }
            let inherited = self.arrow_premise.filter(|p| p.has_conversion());
            let premise = match (inherited, slot.ordinary_context) {
                (Some(a), Some(b)) => Some(a.combine(b)),
                (a, b) => a.or(b),
            };
            let first = self.facts.findings.len();
            let previous = std::mem::replace(&mut self.arrow_premise, premise);
            let checked = self.arrow_step(inference, node, context);
            let separated = if premise.is_some() {
                self.separate_ordinary_findings(inference, first)
            } else {
                Ok(())
            };
            self.arrow_premise = previous;
            checked?;
            separated?;
            if inference.slots[&node].ty.is_some() || inference.slots[&node].blocked {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn arrow_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let slot = &inference.slots[&node];
        if slot.ty.is_some() || slot.blocked {
            return Ok(());
        }
        let module = inference.module;
        let expected = slot.expected.clone();
        let premise = self.arrow_premise;
        let callable = inference.callable_contexts.get(&node).cloned();
        let callable = callable.as_deref();
        let (receiving_parameters, receiving_result) = match expected.as_deref() {
            Some(TypeTerm::Callable {
                parameters, result, ..
            }) => (Some(parameters), Some(result)),
            _ => (None, None),
        };
        let head = &inference.arrows[&node];
        if receiving_parameters.is_some_and(|p| p.len() != head.parameters.len())
            || callable.is_some_and(|c| c.parameters.len() != head.parameters.len())
        {
            self.finding(module, node, ScalarFindingKind::Arity)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        self.elab.reserve(1 + head.parameters.len() * 2)?;
        let mut parameters = Vec::new();
        let mut scope = BTreeMap::new();
        for (index, (declaration, written)) in head.parameters.iter().enumerate() {
            let projected = callable.and_then(|p| p.parameters[index].as_ref());
            let receiving = receiving_parameters.map(|p| &p[index]);
            let Some(ty) = written.as_ref().or(receiving).or(projected) else {
                return Ok(());
            };
            for other in [receiving, projected].into_iter().flatten() {
                if !self.same(ty, other)? {
                    self.mismatch(module, declaration.node, ty, other)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                    return Ok(());
                }
            }
            parameters.push(ty.clone());
            if let Some(name) = declaration.name.as_deref()
                && scope
                    .insert(
                        name,
                        Local {
                            declaration: &declaration.id,
                            ty: Some(ty.clone()),
                            mutable: false,
                            boolean: None,
                        },
                    )
                    .is_some()
            {
                self.finding(
                    module,
                    declaration.node,
                    ScalarFindingKind::DuplicateBinding,
                )?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
        }
        let projected = callable.and_then(|c| c.result.as_ref());
        let result = head
            .result
            .as_ref()
            .or(receiving_result)
            .or(projected)
            .cloned();
        if let Some(ty) = &result {
            for other in [receiving_result, projected].into_iter().flatten() {
                if !self.same(ty, other)? {
                    self.mismatch(module, node, ty, other)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                    return Ok(());
                }
            }
        }
        let body = module.syntax.nodes[node].children[1];
        let before = self.marks();
        let first_fact = self.facts.nodes.len();
        let outer_size = context.scopes.iter().map(BTreeMap::len).sum::<usize>();
        self.elab
            .reserve(outer_size + context.scopes.len() + context.views.len())?;
        let mut inner = Context {
            signature: context.signature,
            scopes: context.scopes.clone(),
            inherited_scopes: context.inherited_scopes,
            loops: Vec::new(),
            startup: context.startup,
            arrow_return: None,
            views: context.views.clone(),
        };
        self.elab.reserve(1)?;
        inner.scopes.push(scope);
        let target = ArrowReturn {
            node,
            expected: result.clone(),
            contributions: Rc::default(),
        };
        inner.arrow_return = Some(target.clone());
        let coverage_premise = premise.filter(|_| head.result.is_none());
        self.arrow_depth += 1;
        let inferred = self.check_arrow_body(module, body, &mut inner, &target, coverage_premise);
        self.arrow_depth -= 1;
        if let Some(result) = inferred?
            && self.marks() == before
        {
            let (mode, captures) = self.capture_facts(module, node, first_fact)?;
            self.elab.reserve(2)?;
            let facts = Arc::new(ArrowFacts {
                site: Self::site(module, node),
                premise,
                parameters,
                result,
                mode,
                captures,
            });
            self.facts.arrows.insert((&module.key, node), facts.clone());
            if let Some(mode) = mode {
                self.elab.reserve(1 + facts.parameters.len())?;
                let ty = Arc::new(TypeTerm::Callable {
                    mode,
                    parameters: facts.parameters.clone(),
                    result: facts.result.clone(),
                });
                inference.slots.get_mut(&node).unwrap().arrow = Some(facts);
                return self.establish(inference, node, ty, false);
            }
            self.defer(module, node, ScalarUnsupported::CaptureMode)?;
        }
        inference.slots.get_mut(&node).unwrap().blocked = true;
        Ok(())
    }

    fn check_arrow_body(
        &mut self,
        module: &'g Module,
        body: NodeId,
        context: &mut Context<'r, 'g>,
        target: &ArrowReturn<'g>,
        coverage_premise: Option<ordinary::InputPremise>,
    ) -> Work<Option<Type<'g>>> {
        self.equation_depth(self.arrow_depth)?;
        if module.syntax.nodes[body].kind != NodeKind::Block {
            return Ok(self
                .expression(module, body, target.expected.clone(), context)?
                .map(|actual| target.expected.clone().unwrap_or(actual.ty)));
        }
        let flow = self.statement(module, body, 1, context)?;
        if let Some(expected) = &target.expected {
            if flow.normal && primitive_name(expected) != Some("void") {
                self.finding(module, body, ScalarFindingKind::ReturnCoverage)?;
                if let Some(premise) = coverage_premise {
                    self.elab.reserve(1)?;
                    self.conditional_findings.push(ConditionalFinding {
                        premise,
                        finding: self.facts.findings.pop().unwrap(),
                    });
                }
                return Ok(None);
            }
            return Ok(Some(expected.clone()));
        }
        let mut result = if flow.normal {
            Some(self.primitive("void")?)
        } else {
            None
        };
        for (site, ty) in target.contributions.borrow().iter() {
            if let Some(previous) = &result {
                if self.same(previous, ty)? {
                    continue;
                }
                if let Some(joined) = self.data_join(previous, ty)? {
                    result = Some(joined);
                } else {
                    self.mismatch(module, *site, ty, previous)?;
                    return Ok(None);
                }
            } else {
                result = Some(ty.clone());
            }
        }
        if result.is_none() && flow.diverges {
            result = Some(self.primitive("never")?);
        }
        Ok(result)
    }

    pub(in crate::analysis::scalars) fn arrow_return_statement(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
        target: &ArrowReturn<'g>,
    ) -> Work<Flow> {
        let mut flow = Flow::normal();
        let actual = if let Some(&child) = module.syntax.nodes[node].children.first() {
            let actual = self.expression(module, child, target.expected.clone(), context)?;
            actual.map(|actual| {
                flow = actual.flow;
                actual.ty
            })
        } else {
            let void = self.primitive("void")?;
            if let Some(expected) = &target.expected
                && !self.same(&void, expected)?
            {
                self.mismatch(module, node, &void, expected)?;
            }
            Some(void)
        };
        if target.expected.is_none()
            && let Some(actual) = actual
            && primitive_name(&actual) != Some("never")
        {
            self.elab.reserve(1)?;
            target.contributions.borrow_mut().push((node, actual));
        }
        flow.returns |= flow.normal;
        flow.normal = false;
        Ok(flow)
    }

    pub(in crate::analysis::scalars) fn callable_admission(
        &mut self,
        actual: &Type<'g>,
        expected: &Type<'g>,
    ) -> Work<bool> {
        let (
            TypeTerm::Callable {
                mode: a,
                parameters: ap,
                result: ar,
            },
            TypeTerm::Callable {
                mode: e,
                parameters: ep,
                result: er,
            },
        ) = (actual.as_ref(), expected.as_ref())
        else {
            return Ok(false);
        };
        self.type_depth(actual, 1)?;
        self.type_depth(expected, 1)?;
        Ok(Self::mode_admits(*a, *e) && ap == ep && ar == er)
    }

    pub(in crate::analysis::scalars) fn value_callee(
        &mut self,
        module: &'g Module,
        callee: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<bool> {
        let callee = self.source_leaf(module, callee)?;
        let syntax = &module.syntax.nodes[callee];
        if syntax.kind != NodeKind::ValueReference {
            return Ok(true);
        }
        let path = &module.syntax.nodes[syntax.children[0]].children;
        if let &[name] = path.as_slice() {
            let spelling = std::str::from_utf8(module.syntax.nodes[name].span.bytes()).unwrap();
            return Ok(self.local(context, spelling)?.is_some());
        }
        Ok(false)
    }

    pub(in crate::analysis::scalars) fn local_callable_result_hint(
        &mut self,
        module: &'g Module,
        callee: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<Type<'g>>> {
        let callee = self.source_leaf(module, callee)?;
        let syntax = &module.syntax.nodes[callee];
        if syntax.kind != NodeKind::ValueReference || syntax.children.len() != 1 {
            return Ok(None);
        }
        let Some(Value::Local(Local { ty: Some(ty), .. })) = self.value(module, callee, context)?
        else {
            return Ok(None);
        };
        let ty = match ty.as_ref() {
            TypeTerm::Reference { referent, .. } => referent,
            _ => &ty,
        };
        Ok(match ty.as_ref() {
            TypeTerm::Callable { result, .. } => Some(result.clone()),
            _ => None,
        })
    }

    pub(in crate::analysis::scalars) fn invocation_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<()> {
        let module = inference.module;
        let syntax = &module.syntax.nodes[node];
        let receiver = syntax.children[0];
        let slot = &inference.slots[&receiver];
        let Some(ty) = slot.ty.as_ref() else {
            return Ok(());
        };
        let (ty, borrowed) = match ty.as_ref() {
            TypeTerm::Reference { mutable, referent } => (referent, Some(*mutable)),
            _ => (ty, None),
        };
        let mut assumption = None;
        let ty = if matches!(ty.as_ref(), TypeTerm::Parameter { .. }) {
            let arguments = &syntax.children[1..];
            self.elab.reserve(arguments.len())?;
            let actual: Vec<_> = arguments
                .iter()
                .map(|id| inference.slots[id].ty.clone())
                .collect();
            let contract = self.symbolic_callable(ty, &actual)?;
            if let Some((descriptor, goal)) = contract.selected {
                inference.pending_invocations.remove(&node);
                assumption = Some(goal);
                descriptor
            } else if let Some(kind) = contract.rejection {
                inference.pending_invocations.remove(&node);
                self.finding(module, node, kind)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            } else {
                if !inference.pending_invocations.contains(&node) {
                    self.elab.reserve(1)?;
                    inference.pending_invocations.insert(node);
                }
                for (&argument, ty) in arguments.iter().zip(contract.receiving) {
                    if let Some(ty) = ty
                        && inference.slots[&argument].ty.is_none()
                    {
                        self.establish(inference, argument, ty, true)?;
                    }
                }
                return Ok(());
            }
        } else {
            ty.clone()
        };
        let TypeTerm::Callable {
            mode,
            parameters,
            result,
        } = ty.as_ref()
        else {
            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        let access = borrowed.map_or(slot.access, |mutable| {
            if mutable && !slot.read_only_projection {
                data::Access::Writable
            } else {
                data::Access::ReadOnly
            }
        });
        if (*mode == InvocationMode::Once && access != data::Access::Owned)
            || (*mode == InvocationMode::Mutable
                && (access == data::Access::ReadOnly
                    || (borrowed.is_none() && slot.place == Place::ReadOnly)))
        {
            self.finding(module, node, ScalarFindingKind::ReadOnly)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        if parameters.len() != syntax.children.len() - 1 {
            self.finding(module, node, ScalarFindingKind::Arity)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let path = if borrowed.is_some() {
            self.project_path(slot.path.as_ref(), data::PathProjection::Dereference)?
        } else {
            self.copy_path(slot.path.as_ref())?
        };
        let mode = *mode;
        for (&argument, parameter) in syntax.children[1..].iter().zip(parameters) {
            self.establish(inference, argument, parameter.clone(), true)?;
        }
        self.elab.reserve(1)?;
        inference.slots.get_mut(&node).unwrap().invocation = Some(Arc::new(InvocationFacts {
            receiver,
            mode,
            access,
            path,
            assumption,
        }));
        self.establish(inference, node, result.clone(), false)
    }
}
