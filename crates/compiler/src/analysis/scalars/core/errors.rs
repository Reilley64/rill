//! Independent static checks for closed error construction. No runtime value is issued.
use super::*;

pub(in crate::analysis::scalars) struct ClosedErr<'g> {
    pub cause: Option<Type<'g>>,
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
    ) -> Work<()> {
        let role = self.result.unwrap();
        let module = inference.module;
        let call = &inference.calls[&node];
        let expected = inference.slots[&node]
            .expected
            .as_ref()
            .and_then(|ty| nominal_args(ty, &role.declaration.id));
        let mut payload_type = expected.map(|args| args[1].clone());
        for written in &call.fact.substitution {
            if let Some(args) = expected
                && !self.same(&written.argument, &args[written.position])?
            {
                self.mismatch(module, node, &written.argument, &args[written.position])?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
            if written.position == 1 {
                payload_type = Some(written.argument.clone());
            }
        }
        let value = call.fact.arguments[0];
        if let Some(&cause) = call.fact.arguments.get(1) {
            let supplied = &inference.slots[&cause];
            if let Some(ty) = &supplied.ty {
                let inner = nominal_args(ty, &role.wrapper.id);
                if primitive_name(ty) != Some("never")
                    && (inner.is_none() || supplied.access != data::Access::Owned)
                {
                    self.finding(module, cause, ScalarFindingKind::InvalidTarget)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                    return Ok(());
                }
                if let (Some(args), Some(explicit)) =
                    (inner, &call.closed_error.as_ref().unwrap().cause)
                    && !self.same(&args[0], explicit)?
                {
                    self.mismatch(module, cause, &args[0], explicit)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                    return Ok(());
                }
            }
        }
        if let Some(payload) = payload_type {
            self.establish(inference, value, payload, true)?;
        }
        // Display selection is still conditional. In particular, this is not a
        // successful ordinary call even when its written arguments are well typed.
        Ok(())
    }

    pub(in crate::analysis::scalars) fn wrapping_prerequisites(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let Some(role) = self.result else {
            return Ok(());
        };
        let module = inference.module;
        let [left, right] = module.syntax.nodes[node].children[..] else {
            unreachable!()
        };
        let Some((_, Some(returned))) = self.propagation_target(context) else {
            return Ok(());
        };
        let Some(args) = nominal_args(returned, &role.declaration.id) else {
            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        let left_slot = &inference.slots[&left];
        if let Some(ty) = &left_slot.ty {
            let container = match ty.as_ref() {
                TypeTerm::Variant { parent, .. } => parent,
                _ => ty,
            };
            if primitive_name(ty) != Some("never")
                && (nominal_args(container, &role.declaration.id).is_none()
                    || left_slot.access != data::Access::Owned)
            {
                self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(());
            }
        }
        self.establish(inference, right, args[1].clone(), true)
    }
}
