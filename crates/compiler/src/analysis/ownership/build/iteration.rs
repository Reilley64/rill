//! Ordinary consuming `for` over the installed List<i32> backing.
use super::*;
use crate::analysis::generics::GoalId;
use crate::analysis::typed::{CollectionProtection, ListTraversal};

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn owned_list_iteration(
        &mut self,
        node: NodeId,
        next: Pc,
        exits: Exits,
        depth: usize,
    ) -> ResultOf<Pc> {
        let pending = || Stop::Pending(node);
        let Some(NodeFactKind::Iteration(iteration)) = self.fact(&node).map(|fact| &fact.kind)
        else {
            return Err(pending());
        };
        if iteration.traversal != Some(ListTraversal::Owned)
            || scalar::scalar(&iteration.yielded) != Some(Primitive::I32)
        {
            return Err(pending());
        }
        let syntax = &self.module.syntax.nodes[node];
        let [pattern, source, body] = syntax.children[..] else {
            return Err(pending());
        };
        if source != iteration.source || pattern != iteration.pattern {
            return Err(pending());
        }
        let Some(NodeFactKind::Pattern(binding)) = self.fact(&pattern).map(|fact| &fact.kind)
        else {
            return Err(pending());
        };
        // This slice checks a repeated named binding, or a single advance
        // with a discarded yield. Do not widen the discard case to traversal.
        if matches!(binding.pattern.as_ref(), Pattern::Wildcard)
            && !self.fact(&body).is_some_and(|fact| {
                !fact.flow.pending && !fact.flow.normal && fact.flow.continues.is_empty()
            })
        {
            return Err(pending());
        }
        // Computed sources have no admitted storage transfer in this slice.
        if self.value(source).map_err(|_| pending())?.path.is_none() {
            return Err(pending());
        }
        let (name, _) = self.ungroup_expression_at(source, depth + 1)?;
        let (named, path) = self.named_owned_place(name)?.ok_or_else(pending)?;
        let actual = self.normal_value(source)?;
        if !self.is_list_i32(&named.ty)
            || actual.ty != named.ty
            || actual.access != Access::Owned
            || !matches!(&actual.path, Some(p) if std::ptr::eq(p.root, path.root) && p.projections.is_empty())
            || !named.binding.is_some_and(|id| std::ptr::eq(id, path.root))
        {
            return Err(pending());
        }
        let source_slot = self.resolve_place(name, path)?;
        if !matches!(
            self.slots[source_slot].cleanup,
            StorageCleanup::DisposeListI32(_)
        ) {
            return Err(pending());
        }
        let TypeTerm::Nominal {
            declaration: list, ..
        } = actual.ty.as_ref()
        else {
            return Err(pending());
        };
        let TypeTerm::Nominal {
            declaration: cursor,
            arguments,
        } = iteration.iterator.as_ref()
        else {
            return Err(pending());
        };
        if !matches!(arguments.as_slice(), [element] if *element == iteration.yielded) {
            return Err(pending());
        }
        let option = self.typed.option_role().ok_or_else(pending)?;
        if !std::ptr::eq(iteration.some, &option.some.id)
            || !std::ptr::eq(iteration.none, &option.none.id)
        {
            return Err(pending());
        }
        use CollectionOperation::{DisposeIterator, Iterate, Next};
        use ListTraversal::Owned;
        let iterate = self.iteration_scope(node, list, Iterate(Owned))?;
        let open = self.selected_iteration_step(
            node,
            &iteration.iterate,
            iterate,
            [&actual.ty, &iteration.iterator],
            |output| *output == iteration.iterator,
        )?;
        let next_scope = self.iteration_scope(node, cursor, Next(Owned))?;
        let advance = self.selected_iteration_step(
            node,
            &iteration.next,
            next_scope,
            [&iteration.iterator, &iteration.yielded],
            |output| {
                matches!(output.as_ref(), TypeTerm::Nominal { declaration, arguments }
                if std::ptr::eq(*declaration, &option.declaration.id)
                    && matches!(arguments.as_slice(), [payload] if *payload == iteration.yielded))
            },
        )?;
        let dispose = self.iteration_scope(node, cursor, DisposeIterator(Owned))?;
        let release = dispose.provider.ok_or_else(pending)?;
        if !self
            .dispose
            .is_some_and(|id| std::ptr::eq(id, &dispose.family.id))
            || advance.artifact != open.artifact
            || release.artifact != open.artifact
        {
            return Err(pending());
        }
        let mut backing = None;
        for repr in self.typed.collection_representations() {
            self.budget.step(1)?;
            if std::ptr::eq(&repr.declaration.id, *cursor)
                && repr.storage == CollectionStorage::UnyieldedElements
                && std::ptr::eq(repr.artifact.context(), open.artifact.context())
            {
                backing = Some(repr);
                break;
            }
        }
        let backing = backing.ok_or_else(pending)?;
        self.check_i32_element_cleanup(node)?;
        // No composition with a propagating exit is proved by this ordinary loop.
        if self.contains_unadmitted_propagation(body, None, depth + 1)? {
            return Err(pending());
        }
        let into = self.iteration_binder(pattern, binding, source, &iteration.yielded)?;
        let cursor = self.allocate_slot(
            node,
            true,
            StorageCleanup::DisposeOwnedListIterI32(ListDisposal {
                implementation: &release.implementation.id,
                bridge: &release.bridge.id,
            }),
            None,
        )?;
        let retire = self.cleanup_scope(node, &[cursor], next)?;
        let return_retire = exits
            .returning
            .map(|pc| self.cleanup_scope(node, &[cursor], pc))
            .transpose()?;
        let head = self.emit(
            node,
            Action::AdvanceOwnedListI32 {
                cursor,
                into,
                some: iteration.some,
                none: iteration.none,
                provider: advance,
            },
            Edges::End,
        )?;
        let end_item = |this: &mut Self, pc| this.cleanup_scope(pattern, into.as_slice(), pc);
        let looping = Loop {
            breaks: end_item(self, retire)?,
            continues: end_item(self, head)?,
        };
        let returning = return_retire.map(|pc| end_item(self, pc)).transpose()?;
        let yes = self.statement(
            body,
            looping.continues,
            Exits {
                returning,
                looping: Some(looping),
            },
            depth + 1,
        )?;
        self.code[head].edges = Edges::Branch { yes, no: retire };
        self.emit(
            name,
            Action::OpenOwnedListI32 {
                source: source_slot,
                cursor,
                backing,
                provider: open,
            },
            Edges::Next(head),
        )
    }

    fn iteration_scope(
        &mut self,
        node: NodeId,
        owner: &DeclarationId,
        operation: CollectionOperation,
    ) -> ResultOf<CollectionProtection<'g>> {
        for scope in self.typed.collection_protection() {
            self.budget.step(1)?;
            if scope.operation == operation
                && std::ptr::eq(&scope.owner.id, owner)
                && scope.coverage == OwnershipCoverage::Clear
            {
                return Ok(scope);
            }
        }
        Err(Stop::Pending(node))
    }

    fn selected_iteration_step(
        &mut self,
        node: NodeId,
        id: &GoalId,
        scope: CollectionProtection<'g>,
        arguments: [&Type<'g>; 2],
        output_matches: impl Fn(&Type<'g>) -> bool,
    ) -> ResultOf<CollectionProvider<'g>> {
        let pending = || Stop::Pending(node);
        let input_matches = |input: &Type<'g>| {
            if matches!(scope.operation, CollectionOperation::Next(_)) {
                matches!(input.as_ref(), TypeTerm::Reference { mutable: true, referent } if referent == arguments[0])
            } else {
                input == arguments[0]
            }
        };
        self.budget.step(1)?;
        let provider = scope.provider.ok_or_else(pending)?;
        let goal = self.typed.bodies().goal(id).map_err(|_| pending())?;
        if goal.state != GoalState::Proved
            || goal.premise.is_some()
            || !matches!(&goal.judgment, GoalJudgment::Determine { family, inputs, filters }
                if *family == &scope.family.id && filters.is_empty()
                    && matches!(inputs.as_slice(), [input] if input_matches(input)))
            || !goal.selection.as_ref().is_some_and(|selection| {
                selection.implementation == &provider.implementation.id
                    && matches!(selection.substitution.as_slice(), [element]
                        if element.position == 0 && scalar::scalar(&element.argument) == Some(Primitive::I32))
            })
            || !goal.output.as_ref().is_some_and(|output| {
                matches!(output.arguments.as_slice(), [input, inferred]
                    if input == arguments[0] && inferred == arguments[1])
                    && output_matches(&output.result)
            })
        {
            return Err(pending());
        }
        Ok(provider)
    }

    fn check_i32_element_cleanup(&mut self, node: NodeId) -> ResultOf<()> {
        let scopes = self.typed.primitive_protection();
        self.budget.step(scopes.len())?;
        let winner = scopes
            .iter()
            .find_map(|scope| {
                (Some(scope.family) == self.dispose
                    && scope.role == PrimitiveRole::Dispose
                    && scope.primitive == Primitive::I32
                    && scope.coverage == PrimitiveCoverage::Clear)
                    .then_some(scope.required_winner)
                    .flatten()
            })
            .ok_or(Stop::Pending(node))?;
        let providers = self.typed.primitive_providers();
        self.budget.step(providers.len())?;
        if !providers
            .iter()
            .any(|p| std::ptr::eq(&p.implementation.id, winner) && p.binding.is_none())
        {
            return Err(Stop::Pending(node));
        }
        Ok(())
    }

    fn iteration_binder(
        &mut self,
        pattern: NodeId,
        facts: &PatternFacts<'g>,
        source: NodeId,
        yielded: &Type<'g>,
    ) -> ResultOf<Option<Slot>> {
        let pending = Stop::Pending(pattern);
        self.budget.step(1)?;
        if facts.source.node != source
            || facts.ty != *yielded
            || facts.access != Access::Owned
            || !facts.coverage.exhaustive()
        {
            return Err(pending);
        }
        match (facts.pattern.as_ref(), facts.binders.as_slice()) {
            (Pattern::Wildcard, []) => Ok(None),
            (Pattern::Any, [binder])
                if !binder.mutable
                    && binder.projections.is_empty()
                    && binder.ty == *yielded
                    && binder.source_type == *yielded =>
            {
                self.local(binder.declaration, pattern, yielded).map(Some)
            }
            _ => Err(pending),
        }
    }
}
