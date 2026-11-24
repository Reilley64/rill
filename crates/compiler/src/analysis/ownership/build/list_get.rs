//! `get(&users, 0u64)` on a named owned `List<&Record>`: the result's payload
//! refers to the List's element slot, which is destroyed with the List
//! (RFC 0007, ri-2s0.7.2). The stored `&Record` itself, extracted by value,
//! is the separate iteration route (ri-2s0.7).
use super::*;

/// The written shape of an admitted lookup, read from facts already paid for.
#[derive(Clone, Copy)]
pub(super) struct ListElementLookup<'a, 'g> {
    operand: NodeId,
    key: NodeId,
    list: &'a ExpressionFacts<'g>,
}

impl<'a, 'g> Build<'a, 'g, '_> {
    /// Screen on syntax and typed facts only (metering rule 1): no `step`,
    /// reservation or allocation. The callee is matched against the fixed
    /// array of List operations, which is constant-sized.
    pub(super) fn list_element_lookup(
        &self,
        facts: &ExpressionFacts<'g>,
    ) -> Option<ListElementLookup<'a, 'g>> {
        let call = facts.call.as_deref()?;
        let [receiver, key] = call.arguments.as_slice() else {
            return None;
        };
        let nodes = &self.module.syntax.nodes;
        if call.pipe_input.is_some()
            || !call.defaults.is_empty()
            || call.scoped_join.is_some()
            || call.list_arrows.is_some()
            || nodes[*receiver].kind != NodeKind::Prefix("&")
            || !matches!(
                nodes[*key].kind,
                NodeKind::Literal | NodeKind::ValueReference
            )
        {
            return None;
        }
        let [operand] = nodes[*receiver].children.as_slice() else {
            return None;
        };
        let TypeTerm::Nominal { arguments, .. } = facts.ty.as_ref() else {
            return None;
        };
        // `Option<&&Record>`: a reference to a stored shared record reference.
        let [payload] = arguments.as_slice() else {
            return None;
        };
        if !self.typed.option_role().is_some_and(|role| {
            matches!(facts.ty.as_ref(), TypeTerm::Nominal { declaration, .. }
                if *declaration == &role.declaration.id)
        }) || !matches!(payload.as_ref(), TypeTerm::Reference { mutable: false, referent }
            if self.is_shared_record_reference(referent))
        {
            return None;
        }
        let list = self.value(*operand).ok()?;
        if nodes[*operand].kind != NodeKind::ValueReference
            || list.access != Access::Owned
            || !self.is_list_shared_record_reference(&list.ty)
            || !list.path.as_ref().is_some_and(|path| {
                matches!(
                    path.root.kind,
                    DeclarationKind::Binding | DeclarationKind::Parameter
                ) && path.projections.is_empty()
            })
            || !self.typed.collection_protection().any(|scope| {
                scope.operation == CollectionOperation::Get && scope.family.id == *call.callee
            })
        {
            return None;
        }
        Some(ListElementLookup {
            operand: *operand,
            key: *key,
            list,
        })
    }

    /// The return storage of a lookup's result: a shared reference option,
    /// which holds the loan of the element slot and has no cleanup of its own.
    pub(super) fn list_element_option_slot(
        &mut self,
        node: NodeId,
        ty: &Type<'g>,
    ) -> ResultOf<Slot> {
        let role = *self.typed.option_role().ok_or(Stop::Pending(node))?;
        self.prove_no_variant_disposer(node, ty, [role.some, role.none])?;
        self.allocate_slot(
            node,
            false,
            StorageCleanup::End,
            Some(ReferenceOption::Shared),
        )
    }

    pub(super) fn call_list_element_reference(
        &mut self,
        node: NodeId,
        lookup: ListElementLookup<'a, 'g>,
        into: Option<Slot>,
        next: Pc,
    ) -> ResultOf<Pc> {
        let pending = || Stop::Pending(node);
        let into = into.ok_or_else(pending)?;
        let bridge = self.typed.list_lookup_bridge().ok_or_else(pending)?;
        let key = self.normal_value(lookup.key)?;
        if scalar::scalar(&key.ty) != Some(Primitive::U64) {
            return Err(pending());
        }
        let index = if self.module.syntax.nodes[lookup.key].kind == NodeKind::Literal {
            let Some(LiteralValue::Integer {
                magnitude,
                negative: false,
            }) = key.literal.map(|literal| literal.value)
            else {
                return Err(pending());
            };
            ElementIndex::Literal(u64::try_from(magnitude).map_err(|_| pending())?)
        } else {
            // A plain `u64` local or parameter, read once at the lookup.
            let path = key.path.as_ref().ok_or_else(pending)?;
            if key.access != Access::Owned
                || !matches!(
                    path.root.kind,
                    DeclarationKind::Binding | DeclarationKind::Parameter
                )
                || !path.projections.is_empty()
            {
                return Err(pending());
            }
            ElementIndex::Local(self.resolve_place(lookup.key, path)?)
        };
        let path = lookup.list.path.as_ref().ok_or_else(pending)?;
        let list = self.resolve_place(lookup.operand, path)?;
        self.emit(
            node,
            Action::LookupListElement {
                list,
                index,
                into,
                lookup: &bridge.id,
            },
            Edges::Next(next),
        )
    }
}
