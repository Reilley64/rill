//! Resolve primitive formatting against the installed canonical family.
use super::*;
use crate::analysis::typed::Primitive;

#[derive(Clone, Copy, Debug)]
pub struct FormattingCall<'g> {
    pub expression: NodeId,
    pub family: &'g DeclarationId,
    pub provider: Primitive,
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

    pub(super) fn is_installed_show(&self, id: &DeclarationId) -> bool {
        self.show.is_some_and(|show| show.id == *id)
    }

    // Do not infer ordinary clearance from an unavailable written registration.
    // Check disjointness against every captured registration before use.
    fn show_uncontested(&mut self, borrowed: &Type<'g>) -> Work<bool> {
        let show = &self.show.unwrap().id;
        let family = self.input.ordinary.unwrap().captured_family(show).unwrap();
        let mut count = 0;
        for implementation in self.input.graph.implementations() {
            self.elab.step(1)?;
            count += usize::from(implementation.family == *show);
        }
        if count != family.implementations.len() {
            return Ok(false);
        }
        if count == 0 {
            return Ok(true);
        }
        let mut budget = declarations::Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let result = (|| {
            let exact = ordinary::patterns::ground(std::iter::once(borrowed), &mut budget)?;
            for body in &family.implementations {
                budget.step(1)?;
                if body.compatibility != ordinary::Compatibility::Compatible
                    || body.alternatives.is_empty()
                    || ordinary::patterns::intersection(
                        &body.alternatives,
                        std::slice::from_ref(&exact),
                        &mut budget,
                    )?
                    .is_some()
                {
                    return Ok(false);
                }
            }
            Ok(true)
        })();
        self.elab.statistics = budget.statistics;
        result
    }

    pub(super) fn primitive_show_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
    ) -> Work<()> {
        let call = &inference.calls[&node];
        let [arg] = call.fact.arguments.as_slice() else {
            return Ok(());
        };
        let arg = *arg;
        let input = &inference.slots[&arg];
        if input.blocked {
            return Ok(());
        }
        let explicit = call.fact.substitution.first().map(|s| s.argument.clone());
        let Some(actual) = input.ty.clone() else {
            if let Some(referent) = explicit {
                let ty = self.show_borrow(referent)?;
                self.establish(inference, arg, ty, true)?;
            }
            return Ok(());
        };
        let TypeTerm::Reference {
            mutable: false,
            referent,
        } = actual.as_ref()
        else {
            return Ok(());
        };
        let Some(provider) = primitive_name(referent).and_then(Primitive::named) else {
            return Ok(());
        };
        if let Some(explicit) = explicit
            && !self.same(referent, &explicit)?
        {
            return Ok(());
        }
        if !self.show_uncontested(&actual)? {
            return Ok(());
        }
        let result = self.primitive("string")?;
        let call = inference.calls.get_mut(&node).unwrap();
        let family = self
            .input
            .ordinary
            .unwrap()
            .captured_family(call.fact.callee)
            .unwrap();
        let parameter = &family.header.binders[0];
        self.elab.reserve(2)?;
        call.fact.substitution = vec![Substitution {
            binder: &parameter.declaration.id,
            position: parameter.position,
            argument: referent.clone(),
        }];
        call.parameters[0] = actual;
        call.result = result.clone();
        call.family = false;
        call.infer = false;
        self.record_callee(inference.module, call.callee_node, call.fact.callee)?;
        inference.slots.get_mut(&node).unwrap().installed_operation =
            Some(data::DataExpression::PrimitiveShow(provider));
        self.establish(inference, node, result, false)
    }

    fn show_borrow(&mut self, referent: Type<'g>) -> Work<Type<'g>> {
        self.type_depth(&referent, 2)?;
        self.elab.reserve(1)?;
        Ok(Arc::new(TypeTerm::Reference {
            mutable: false,
            referent,
        }))
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
            !slot.blocked && slot.ty.is_some()
        }) {
            return Ok(());
        }
        let show = &self.show.unwrap().id;
        self.elab.reserve(children)?;
        let mut calls = Vec::new();
        for (hole, value) in Self::template_holes(module, node) {
            let ty = inference.slots[&value].ty.as_ref().unwrap();
            if primitive_name(ty) == Some("never") {
                continue;
            }
            let Some(operation) = primitive_name(ty).and_then(Primitive::named) else {
                self.defer(module, hole, ScalarUnsupported::Formatting)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            };
            let borrowed = self.show_borrow(ty.clone())?;
            if !self.show_uncontested(&borrowed)? {
                self.defer(module, hole, ScalarUnsupported::Formatting)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
            calls.push(FormattingCall {
                expression: value,
                family: show,
                provider: operation,
            });
        }
        inference.slots.get_mut(&node).unwrap().installed_operation =
            Some(data::DataExpression::Interpolation(calls.into()));
        let string = self.primitive("string")?;
        self.establish(inference, node, string, false)
    }
}
