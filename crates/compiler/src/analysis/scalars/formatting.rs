//! Canonical calls and interpolation use the shared ordinary evidence solver.
use super::*;
use crate::analysis::typed::{Primitive, PrimitiveOperation, PrimitiveRole};
use generics::{GoalId, GoalJudgment, GoalState};

#[derive(Clone, Debug)]
pub struct FormattingCall<'g> {
    pub expression: NodeId,
    pub family: &'g DeclarationId,
    /// Selected implementation, or the written assumption for a rigid generic body.
    pub goal: GoalId,
    pub primitive: Option<Primitive>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn template_holes(
        module: &Module,
        node: NodeId,
    ) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
        let nodes = &module.syntax.nodes;
        nodes[node]
            .children
            .iter()
            .copied()
            .filter(|&hole| nodes[hole].kind == NodeKind::TemplateHole)
            .map(|hole| (hole, nodes[hole].children[0]))
    }

    /// Show and equality use ordinary selection. This is not prelude protection clearance.
    pub(super) fn ordinary_scalar_role(&self, id: &DeclarationId) -> Option<PrimitiveRole> {
        if self.show.is_some_and(|show| show.id == *id) {
            Some(PrimitiveRole::Show)
        } else if self
            .primitives
            .and_then(|p| p.equality)
            .is_some_and(|eq| eq.id == *id)
        {
            Some(PrimitiveRole::Equality)
        } else {
            None
        }
    }

    pub(super) fn is_canonical_scalar_family(&self, id: &DeclarationId) -> bool {
        self.ordinary_scalar_role(id).is_some()
            || self.is_error_display(id)
            || self.primitives.is_some_and(|p| p.prelude(id).is_some())
    }

    pub(super) fn canonical_scalar_call(&self, inference: &Inference<'g>, node: NodeId) -> bool {
        let call = &inference.calls[&node].fact;
        if self.ordinary_scalar_role(call.callee).is_some() || self.is_error_display(call.callee) {
            return true;
        }
        let Some(role) = self.primitives.and_then(|p| p.prelude(call.callee)) else {
            return false;
        };
        if let Some(written) = call.substitution.first() {
            return primitive_name(&written.argument).is_some();
        }
        let Some(ty) = call
            .arguments
            .first()
            .and_then(|id| inference.slots[id].ty.as_ref())
        else {
            return true;
        };
        match (role, ty.as_ref()) {
            (PrimitiveRole::Copy, TypeTerm::Reference { referent, .. }) => {
                primitive_name(referent).is_some()
            }
            _ => primitive_name(ty).is_some(),
        }
    }

    pub(super) fn is_error_display(&self, id: &DeclarationId) -> bool {
        self.result
            .and_then(|role| role.display_protection)
            .is_some_and(|protection| protection.family.id == *id)
    }

    pub(super) fn clear_primitive_goal(&self, id: &DeclarationId, goal: &GoalJudgment<'g>) -> bool {
        let Some(core) = self.primitives else {
            return false;
        };
        match goal {
            GoalJudgment::Requirement { arguments, .. } => {
                matches!(arguments.as_slice(), [subject] if core.winner(id, subject).is_some())
            }
            GoalJudgment::Determine { inputs, .. } => core.winner_for_inputs(id, inputs).is_some(),
            GoalJudgment::Instantiate { .. } => false,
        }
    }

    pub(super) fn primitive_bridge(&self, id: &DeclarationId) -> Option<PrimitiveOperation> {
        self.primitives?.bridges.get(id).copied()
    }

    fn canonical_borrow(&mut self, referent: Type<'g>) -> Work<Type<'g>> {
        self.type_depth(&referent, 2)?;
        self.elab.reserve(1)?;
        Ok(Arc::new(TypeTerm::Reference {
            mutable: false,
            referent,
        }))
    }

    fn canonical_goal(
        &mut self,
        site: Site<'g>,
        family: &'g DeclarationId,
        inputs: Vec<Type<'g>>,
        substitution: Substitution<'g>,
        routes: &[&'g Binding],
    ) -> Work<(GoalId, GoalState)> {
        self.elab.reserve(1 + inputs.len())?;
        let source = self
            .input
            .ordinary
            .unwrap()
            .captured_family(family)
            .unwrap()
            .header
            .declaration;
        let module = self.evidence_module(&family.module)?;
        let goal = self.issue_determination(
            GoalJudgment::Determine {
                family,
                inputs,
                filters: vec![substitution],
            },
            site,
            Self::site(module, source.node),
            routes,
            None,
        )?;
        let state = self.solve_goal(&goal)?;
        Ok((goal, state))
    }

    fn primitive_show(&self, goal: &GoalId, referent: &Type<'g>) -> Option<Primitive> {
        let selected = self.evidence.get(goal).ok()?.selection.as_ref()?;
        let primitive = primitive_name(referent).and_then(Primitive::named)?;
        self.primitives?
            .providers
            .iter()
            .any(|p| {
                p.role == PrimitiveRole::Show && p.implementation.id == *selected.implementation
            })
            .then_some(primitive)
    }

    pub(super) fn canonical_call_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<()> {
        let call = &inference.calls[&node];
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap();
        let arguments = call.fact.arguments.as_slice();
        self.elab.step(arguments.len())?;
        if arguments.iter().any(|arg| {
            let input = &inference.slots[arg];
            input.blocked || input.ordinary_context.is_some()
        }) {
            return Ok(());
        }
        let by_value = self.primitives.and_then(|p| p.prelude(call.fact.callee))
            == Some(PrimitiveRole::Dispose);
        let referent = if let Some(explicit) = call.fact.substitution.first() {
            explicit.argument.clone()
        } else if let Some(ty) = arguments
            .iter()
            .filter_map(|arg| inference.slots[arg].ty.as_ref())
            .find(|ty| primitive_name(ty) != Some("never"))
        {
            match ty.as_ref() {
                _ if by_value => ty.clone(),
                TypeTerm::Reference {
                    mutable: false,
                    referent,
                } => referent.clone(),
                _ => {
                    self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                    return Ok(());
                }
            }
        } else {
            return Ok(());
        };
        let parameter = if by_value {
            referent.clone()
        } else {
            self.canonical_borrow(referent.clone())?
        };
        let count = arguments.len();
        let mut ready = true;
        for i in 0..count {
            let arg = inference.calls[&node].fact.arguments[i];
            self.establish(inference, arg, parameter.clone(), true)?;
            let Some(actual) = inference.slots[&arg].ty.clone() else {
                ready = false;
                continue;
            };
            if primitive_name(&actual) != Some("never") && !self.same(&actual, &parameter)? {
                self.mismatch(inference.module, arg, &actual, &parameter)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
        }
        if !ready {
            return Ok(());
        }
        let binder = &family.header.binders[0];
        let sub = Substitution {
            binder: &binder.declaration.id,
            position: 0,
            argument: referent,
        };
        let call = &inference.calls[&node];
        let family = call.fact.callee;
        let module = inference.module;
        let (goal, state) = self.canonical_goal(
            Self::site(module, node),
            family,
            vec![parameter.clone(); count],
            sub.clone(),
            &call.fact.bindings,
        )?;
        if !self.canonical_result(module, node, state)? {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let result = self
            .evidence
            .get(&goal)
            .unwrap()
            .output
            .as_ref()
            .unwrap()
            .result
            .clone();
        let operation = self.primitive_show(&goal, &sub.argument).map_or_else(
            || data::DataExpression::CanonicalCall(goal),
            data::DataExpression::PrimitiveShow,
        );
        let call = inference.calls.get_mut(&node).unwrap();
        self.elab.reserve(1)?;
        call.fact.substitution = vec![sub];
        call.parameters.fill(parameter);
        call.result = result.clone();
        call.family = false;
        call.infer = false;
        self.record_callee(module, call.callee_node, family)?;
        inference.slots.get_mut(&node).unwrap().installed_operation = Some(operation);
        self.establish(inference, node, result, false)
    }

    fn canonical_result(
        &mut self,
        module: &'g Module,
        node: NodeId,
        state: GoalState,
    ) -> Work<bool> {
        match state {
            GoalState::Proved => return Ok(true),
            GoalState::Disproved
            | GoalState::Unresolved(generics::GoalBlocker::MissingAssumption) => {
                self.finding(module, node, ScalarFindingKind::MissingRequirement)?
            }
            GoalState::Unresolved(generics::GoalBlocker::UnavailableProvider) => {
                self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            }
            GoalState::Unresolved(generics::GoalBlocker::OrdinaryAdmission) => {
                self.defer(module, node, ScalarUnsupported::CoreProtection)?;
            }
            _ => {
                self.defer(module, node, ScalarUnsupported::Requirements)?;
            }
        }
        Ok(false)
    }

    pub(super) fn check_template(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<()> {
        let module = inference.module;
        let children = module.syntax.nodes[node].children.len();
        self.elab.step(children)?;
        if !Self::template_holes(module, node).all(|(_, value)| {
            let slot = &inference.slots[&value];
            !slot.blocked && slot.ty.is_some() && slot.ordinary_context.is_none()
        }) {
            return Ok(());
        }
        let show = &self.show.unwrap().id;
        let header = &self
            .input
            .ordinary
            .unwrap()
            .captured_family(show)
            .unwrap()
            .header;
        let binder = &header.binders[0].declaration.id;
        self.elab.reserve(children)?;
        let mut calls = Vec::new();
        let mut valid = true;
        for (hole, value) in Self::template_holes(module, node) {
            let ty = inference.slots[&value].ty.as_ref().unwrap().clone();
            if primitive_name(&ty) == Some("never") {
                continue;
            }
            let borrowed = self.canonical_borrow(ty.clone())?;
            let (goal, state) = self.canonical_goal(
                Self::site(module, hole),
                show,
                vec![borrowed],
                Substitution {
                    binder,
                    position: 0,
                    argument: ty.clone(),
                },
                &[],
            )?;
            if !self.canonical_result(module, hole, state)? {
                valid = false;
                continue;
            }
            calls.push(FormattingCall {
                expression: value,
                family: show,
                primitive: self.primitive_show(&goal, &ty),
                goal,
            });
        }
        if !valid {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        inference.slots.get_mut(&node).unwrap().installed_operation =
            Some(data::DataExpression::Interpolation(calls.into()));
        let string = self.primitive("string")?;
        self.establish(inference, node, string, false)
    }
}
