//! Closed error operations over original source and capture-bound evidence.
use super::*;
use crate::analysis::typed::ErrorOperation;
use generics::{GoalBlocker, GoalId, GoalState};

pub(in crate::analysis::scalars) struct ClosedErr<'g> {
    pub cause: Option<Type<'g>>,
}

#[derive(Clone, Debug)]
pub struct ErrorConstruction<'g> {
    pub operation: ErrorOperation,
    pub origin: Span,
    pub result: Type<'g>,
    pub failure: &'g DeclarationId,
    pub wrapper: &'g DeclarationId,
    pub representation: crate::analysis::typed::ErrorRepresentation<'g>,
    pub value: NodeId,
    pub payload: Type<'g>,
    /// The determination records the selected body, or the caller's written requirement.
    pub display: GoalId,
    pub retained: Option<RetainedError<'g>>,
}

#[derive(Clone, Debug)]
pub struct RetainedError<'g> {
    /// A wrapper argument for closed Err; the left Result for an infix wrap.
    pub operand: NodeId,
    pub wrapper: Type<'g>,
    pub payload: Type<'g>,
    pub display: GoalId,
    pub no_borrow: GoalId,
    pub sendable: GoalId,
    pub shareable: GoalId,
}

#[derive(Clone, Debug)]
pub struct Wrapping<'g> {
    pub branch: Propagation<'g>,
    /// Evaluated only on the failure edge, after the left Result.
    pub construction: ErrorConstruction<'g>,
}

fn nominal_args<'a, 'g>(ty: &'a Type<'g>, id: &DeclarationId) -> Option<&'a [Type<'g>]> {
    match ty.as_ref() {
        TypeTerm::Nominal {
            declaration,
            arguments,
        } if *declaration == id => Some(arguments),
        _ => None,
    }
}

struct CauseInput<'g> {
    operand: NodeId,
    wrapper: Type<'g>,
    payload: Type<'g>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn closed_err(&self, id: &DeclarationId) -> bool {
        self.result
            .and_then(|role| role.constructor)
            .is_some_and(|decl| &decl.id == id)
    }

    pub(in crate::analysis::scalars) fn closed_err_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        settle_inferred: bool,
    ) -> Work<()> {
        let role = self.result.unwrap();
        let module = inference.module;
        let call = &inference.calls[&node];
        let expected = inference.slots[&node]
            .expected
            .as_ref()
            .and_then(|ty| nominal_args(ty, &role.declaration.id));
        let written = &call.fact.substitution;
        let value = call.fact.arguments[0];
        let cause_node = call.fact.arguments.get(1).copied();
        let explicit_cause = call.closed_error.as_ref().unwrap().cause.clone();
        let success = written
            .first()
            .map(|s| s.argument.clone())
            .or_else(|| expected.map(|args| args[0].clone()));
        let contextual_payload = written
            .get(1)
            .map(|s| s.argument.clone())
            .or_else(|| expected.map(|args| args[1].clone()));
        let mut invalid = false;
        for sub in written {
            self.elab.step(1)?;
            if let Some(args) = expected
                && !self.same(&sub.argument, &args[sub.position])?
            {
                self.mismatch(module, node, &sub.argument, &args[sub.position])?;
                invalid = true;
            }
        }
        if let Some(payload) = &contextual_payload {
            self.establish(inference, value, payload.clone(), true)?;
        }
        if let (Some(cause), Some(c)) = (cause_node, &explicit_cause) {
            let wrapper = self.error_nominal(&role.wrapper.id, [c.clone()])?;
            self.establish(inference, cause, wrapper, true)?;
        }
        let mut diverges = false;
        let mut retained = None;
        if let Some(cause) = cause_node {
            let supplied = &inference.slots[&cause];
            if let Some(ty) = &supplied.ty {
                if primitive_name(ty) == Some("never") {
                    diverges = true;
                } else if let Some([inner]) = nominal_args(ty, &role.wrapper.id)
                    && supplied.access == data::Access::Owned
                {
                    if let Some(explicit) = &explicit_cause
                        && !self.same(inner, explicit)?
                    {
                        invalid = true;
                    } else {
                        retained = Some(CauseInput {
                            operand: cause,
                            wrapper: ty.clone(),
                            payload: inner.clone(),
                        });
                    }
                } else {
                    self.finding(module, cause, ScalarFindingKind::InvalidTarget)?;
                    invalid = true;
                }
            }
        }
        if invalid {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let Some(success) = success else {
            return Ok(());
        };
        let Some(actual) = inference.slots[&value].ty.clone() else {
            return Ok(());
        };
        if cause_node.is_some_and(|cause| {
            inference.slots[&cause].ty.is_none() || inference.slots[&cause].blocked
        }) || inference.slots[&value].blocked
        {
            return Ok(());
        }
        let value_never = primitive_name(&actual) == Some("never");
        let payload = if let Some(payload) = contextual_payload {
            if !self.error_payload_fits(&actual, &payload)? {
                // Finalization reports the independent mismatch at the operand.
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
            payload
        } else if !settle_inferred || value_never {
            return Ok(());
        } else {
            actual
        };
        if inference.slots[&node].ty.is_some() {
            return Ok(());
        }
        let result =
            self.error_nominal(&role.declaration.id, [success.clone(), payload.clone()])?;
        let origin = self.error_call_origin(inference, node)?;
        let operation = if retained.is_some() {
            ErrorOperation::Retain
        } else {
            ErrorOperation::Construct
        };
        let Some(construction) = self.check_error_construction(
            inference,
            node,
            (value, payload.clone()),
            retained,
            result,
            origin,
            operation,
        )?
        else {
            return Ok(());
        };
        if Self::propagating_pipe(module, node) {
            // The pipe handoff cannot yet carry both construction and propagation.
            self.defer(
                module,
                module.syntax.nodes[node].children[1],
                ScalarUnsupported::Syntax,
            )?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let call = inference.calls.get_mut(&node).unwrap();
        let header = &self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap()
            .header;
        self.elab.reserve(2)?;
        call.fact.substitution = header
            .binders
            .iter()
            .zip([success, payload])
            .map(|(binder, argument)| Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument,
            })
            .collect();
        self.record_callee(module, call.callee_node, call.fact.callee)?;
        if diverges || value_never {
            // A bottom argument never reaches construction. Preserve its typed return context, not a new origin.
            let never = self.primitive("never")?;
            return self.establish(inference, node, never, false);
        }
        let result = construction.result.clone();
        self.elab.reserve(1)?;
        inference.slots.get_mut(&node).unwrap().installed_operation = Some(
            data::DataExpression::ErrorConstruction(Arc::new(construction)),
        );
        self.establish(inference, node, result, false)
    }

    pub(in crate::analysis::scalars) fn advance_closed_errors(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<bool> {
        let mut ready = Vec::new();
        for (&node, call) in &inference.calls {
            self.elab.step(1)?;
            let slot = &inference.slots[&node];
            if call.closed_error.is_some() && !slot.blocked && slot.ty.is_none() {
                self.elab.reserve(1)?;
                ready.push(node);
            }
        }
        for node in ready {
            self.closed_err_step(inference, node, true)?;
            if !inference.queue.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(in crate::analysis::scalars) fn wrapping_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let Some(role) = self.result else {
            return Ok(());
        };
        if inference.slots[&node].ty.is_some() {
            return Ok(());
        }
        let module = inference.module;
        let [left, right] = module.syntax.nodes[node].children[..] else {
            unreachable!()
        };
        let Some((target, Some(returned))) = self.propagation_target(context) else {
            return Ok(());
        };
        let Some([_, payload]) = nominal_args(returned, &role.declaration.id) else {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        self.establish(inference, right, payload.clone(), true)?;
        let left_slot = &inference.slots[&left];
        let Some(ty) = &left_slot.ty else {
            return Ok(());
        };
        if left_slot.blocked {
            return Ok(());
        }
        let left_never = primitive_name(ty) == Some("never");
        let (container, variant) = match ty.as_ref() {
            TypeTerm::Variant {
                parent,
                declaration,
            } => (parent, Some(*declaration)),
            _ => (ty, None),
        };
        let args = nominal_args(container, &role.declaration.id);
        if !left_never && (args.is_none() || left_slot.access != data::Access::Owned) {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let right_slot = &inference.slots[&right];
        let Some(actual) = &right_slot.ty else {
            return Ok(());
        };
        if right_slot.blocked {
            return Ok(());
        }
        if !self.error_payload_fits(actual, payload)? {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let Some([success, inner]) = args else {
            let never = self.primitive("never")?;
            return self.establish(inference, node, never, false);
        };
        let success = success.clone();
        let inner = inner.clone();
        let wrapper = self.error_nominal(&role.wrapper.id, [inner.clone()])?;
        let origin = self.wrapping_origin(module, left)?;
        let Some(construction) = self.check_error_construction(
            inference,
            node,
            (right, payload.clone()),
            Some(CauseInput {
                operand: left,
                wrapper,
                payload: inner.clone(),
            }),
            returned.clone(),
            origin,
            ErrorOperation::Wrap,
        )?
        else {
            return Ok(());
        };
        self.elab.reserve(1)?;
        inference.slots.get_mut(&node).unwrap().installed_operation =
            Some(data::DataExpression::Wrapping(Arc::new(Wrapping {
                branch: Propagation {
                    target,
                    success: &role.ok.id,
                    failure: &role.err.id,
                    can_continue: variant != Some(&role.err.id),
                    can_return: variant != Some(&role.ok.id),
                    error: Some(PropagationError {
                        wrapper: &role.wrapper.id,
                        payload: inner,
                    }),
                },
                construction,
            })));
        self.establish(inference, node, success, false)
    }

    fn error_payload_fits(&mut self, actual: &Type<'g>, expected: &Type<'g>) -> Work<bool> {
        Ok(primitive_name(actual) == Some("never")
            || self.same(actual, expected)?
            || self.data_admission(actual, expected)?
            || self.callable_admission(actual, expected)?)
    }

    fn error_nominal<const N: usize>(
        &mut self,
        declaration: &'g DeclarationId,
        arguments: [Type<'g>; N],
    ) -> Work<Type<'g>> {
        for argument in &arguments {
            self.type_depth(argument, 2)?;
        }
        self.elab.reserve(1 + N)?;
        Ok(Arc::new(TypeTerm::Nominal {
            declaration,
            arguments: arguments.into(),
        }))
    }

    fn error_call_origin(&mut self, inference: &Inference<'g>, node: NodeId) -> Work<Span> {
        self.elab.step(1)?;
        let nodes = &inference.module.syntax.nodes;
        let syntax = &nodes[node];
        let span = if syntax.kind == NodeKind::Pipe {
            &nodes[syntax.children[1]].span
        } else {
            &syntax.span
        };
        Ok(span
            .snapshot()
            .span(span.start(), span.start())
            .expect("original call anchor"))
    }

    fn wrapping_origin(&mut self, module: &'g Module, left: NodeId) -> Work<Span> {
        let tokens = &module.syntax.lexed.tokens;
        let end = module.syntax.nodes[left].span.end();
        self.elab
            .step(tokens.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        let question = &tokens[tokens.partition_point(|t| t.span.start() < end)].span;
        debug_assert_eq!(question.bytes(), b"?");
        Ok(question.clone())
    }

    #[allow(clippy::too_many_arguments)]
    fn check_error_construction(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        (value, payload): (NodeId, Type<'g>),
        cause: Option<CauseInput<'g>>,
        result: Type<'g>,
        origin: Span,
        operation: ErrorOperation,
    ) -> Work<Option<ErrorConstruction<'g>>> {
        let role = self.result.unwrap();
        let module = inference.module;
        let site = Self::site(module, node);
        let mut blocked = false;
        if !role.constructor_protected
            || (role.display.is_some() && role.display_protection.is_none())
        {
            self.defer(module, node, ScalarUnsupported::CoreProtection)?;
            blocked = true;
        }
        if role.representation.is_none() || role.display.is_none() || role.providers.is_none() {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            blocked = true;
        }
        let display = self.error_display(module, site, &payload, &mut blocked)?;
        let retained = if let Some(cause) = cause {
            let display = self.error_display(
                module,
                Self::site(module, cause.operand),
                &cause.payload,
                &mut blocked,
            )?;
            let mut properties = [None, None, None];
            let intrinsics = &self.input.graph.prelude().intrinsics;
            for (i, name) in ["noBorrow", "sendable", "shareable"]
                .into_iter()
                .enumerate()
            {
                self.elab.step(intrinsics.len())?;
                let intrinsic = intrinsics
                    .iter()
                    .find(|i| i.namespace() == Namespace::ValueFunction && i.name() == name)
                    .expect("fixed property identity");
                let property = self.prove_error_property(
                    Self::site(module, cause.operand),
                    intrinsic,
                    cause.payload.clone(),
                )?;
                properties[i] =
                    self.accept_error_goal(module, cause.operand, property, &mut blocked)?;
            }
            match (display, properties) {
                (Some(display), [Some(no_borrow), Some(sendable), Some(shareable)]) => {
                    Some(RetainedError {
                        operand: cause.operand,
                        wrapper: cause.wrapper,
                        payload: cause.payload,
                        display,
                        no_borrow,
                        sendable,
                        shareable,
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        if blocked {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(None);
        }
        Ok(Some(ErrorConstruction {
            operation,
            origin,
            result,
            failure: &role.err.id,
            wrapper: &role.wrapper.id,
            representation: role.representation.unwrap(),
            value,
            payload,
            display: display.expect("checked display"),
            retained,
        }))
    }

    fn error_display(
        &mut self,
        module: &'g Module,
        site: Site<'g>,
        payload: &Type<'g>,
        blocked: &mut bool,
    ) -> Work<Option<GoalId>> {
        let Some(protection) = self.result.unwrap().display_protection else {
            return Ok(None);
        };
        self.type_depth(payload, 2)?;
        self.elab.reserve(2)?;
        let borrowed = Arc::new(TypeTerm::Reference {
            mutable: false,
            referent: payload.clone(),
        });
        let goal =
            self.determine_source(site, &protection.family.id, vec![borrowed], &[], &[], None)?;
        self.accept_error_goal(module, site.node, goal, blocked)
    }

    fn accept_error_goal(
        &mut self,
        module: &'g Module,
        node: NodeId,
        goal: GoalId,
        blocked: &mut bool,
    ) -> Work<Option<GoalId>> {
        let fact = self.evidence.get(&goal).unwrap();
        match fact.state {
            GoalState::Proved if fact.premise.is_none() => return Ok(Some(goal)),
            GoalState::Disproved | GoalState::Unresolved(GoalBlocker::MissingAssumption)
                if fact.premise.is_none() =>
            {
                self.finding(module, node, ScalarFindingKind::MissingRequirement)?;
            }
            _ => {
                self.defer(module, node, ScalarUnsupported::Requirements)?;
            }
        }
        *blocked = true;
        Ok(None)
    }
}
