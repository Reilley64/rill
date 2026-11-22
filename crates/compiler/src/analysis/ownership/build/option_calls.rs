//! Ordinary postfix calls returning a canonical shared text carrier.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct SharedOptionCall<'a, 'g> {
    header: &'a DeclarationFacts<'g>,
    site: NodeId,
    argument: NodeId,
    ty: &'a Type<'g>,
    from: Slot,
}

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn check_shared_option_call(
        &mut self,
        postfix: NodeId,
        site: NodeId,
        result: &Type<'g>,
        depth: usize,
    ) -> ResultOf<SharedOptionCall<'a, 'g>> {
        let pending = Stop::Pending(postfix);
        let facts = self.normal_value(site)?;
        let call = facts.call.as_deref().ok_or_else(|| pending.clone())?;
        let [arg] = call.arguments.as_slice() else {
            return Err(pending);
        };
        // Screen on facts already paid for, so an unsupported sibling keeps
        // the caps it had before this route existed. A group carries its
        // child's place path, so a grouped whole carrier passes unwalked.
        let whole_carrier = self.value(*arg).is_ok_and(|input| {
            input.access == Access::Owned
                && input.path.as_ref().is_some_and(|path| {
                    matches!(
                        path.root.kind,
                        DeclarationKind::Binding | DeclarationKind::Parameter
                    ) && path.projections.is_empty()
                })
        });
        if !whole_carrier
            || !Self::direct_application_call(call)
            || call.pipe_input.is_some()
            || call.selection.is_some()
            || !call.defaults.is_empty()
            || !matches!(result.as_ref(), TypeTerm::Nominal { .. })
        {
            return Err(pending);
        }
        self.budget.step(1 + self.module.key.path.len())?;
        if call.callee.module != self.module.key {
            return Err(pending);
        }
        let headers = &self.typed.inventory().type_facts().declarations;
        graph::charge_key(&call.callee.module, headers.len(), self.budget)?;
        let header = headers
            .get(call.callee)
            .filter(|header| std::ptr::eq(call.callee, &header.declaration.id))
            .ok_or_else(|| pending.clone())?;
        let DeclarationShape::Function {
            parameters,
            result: declared,
        } = &header.shape
        else {
            return Err(pending);
        };
        let [formal] = parameters.as_slice() else {
            return Err(pending);
        };
        if header.declaration.public
            || header.declaration.scope.is_some()
            || !header.binders.is_empty()
            || declared != result
            || &formal.ty != result
        {
            return Err(pending);
        }
        let (argument, _) = self.ungroup_expression_at(*arg, depth + 1)?;
        let (actual, path) = self
            .named_owned_place(argument)?
            .ok_or_else(|| pending.clone())?;
        let parent = match actual.ty.as_ref() {
            TypeTerm::Variant { parent, .. } if self.admitted_option(&actual.ty).is_some() => {
                parent
            }
            _ => &actual.ty,
        };
        if parent != result {
            return Err(pending);
        }
        Ok(SharedOptionCall {
            header,
            site,
            argument,
            ty: &actual.ty,
            from: self.resolve_place(argument, path)?,
        })
    }

    pub(super) fn stage_shared_option_call(
        &mut self,
        call: SharedOptionCall<'a, 'g>,
        into: Slot,
        next: Pc,
    ) -> ResultOf<Pc> {
        let argument = self.slot(call.argument, call.ty)?;
        self.budget.reserve(2)?;
        let index = self.calls.len();
        self.calls.push(Call {
            callee: &call.header.declaration.id,
            direct_bool_parameter: false,
            args: vec![argument],
            into: Some(into),
            result: CallResult::RetainsSharedOption(call.header),
            destination: None,
        });
        let invoke = self.emit(call.site, Action::Call(index), Edges::Next(next))?;
        self.emit(
            call.argument,
            Action::Read {
                from: call.from,
                into: Some(argument),
            },
            Edges::Next(invoke),
        )
    }
}
