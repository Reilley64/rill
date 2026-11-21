//! Consuming `for` over the admitted installed List backings.
use super::*;
use crate::analysis::generics::GoalId;
use crate::analysis::typed::{CollectionProtection, ListTraversal};

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn option_list_parameter(
        &mut self,
        parameter: &MemberType<'g>,
        iteration: NodeId,
    ) -> ResultOf<Slot> {
        let node = parameter.declaration.node;
        let pending = || Stop::Pending(node);
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = parameter.ty.as_ref()
        else {
            return Err(pending());
        };
        let [element] = arguments.as_slice() else {
            return Err(pending());
        };
        let list = self.typed.list_role().ok_or_else(pending)?;
        let (role, OptionPayload::Text(_)) = self.admitted_option(element).ok_or_else(pending)?
        else {
            return Err(pending());
        };
        if !std::ptr::eq(*declaration, &list.declaration.id)
            || !matches!(element.as_ref(), TypeTerm::Nominal { .. })
        {
            return Err(pending());
        }
        // The installed Option role proves Some(T) and payloadless None.
        // Rule out custom disposal of the carrier and both variants before
        // composing the structural tag test with the protected string leaf.
        self.prove_no_variant_disposer(node, element, [role.some, role.none])?;
        let element = OptionTextDisposal {
            some: &role.some.id,
            none: &role.none.id,
            payload: self.text_element_disposal(node)?,
        };
        let disposal = OptionTextListDisposal {
            list: self.list_disposal(node)?,
            element,
        };
        let slot = self.allocate_slot(
            node,
            true,
            StorageCleanup::DisposeListOptionText {
                disposal,
                iteration,
            },
            None,
        )?;
        self.budget.reserve(1)?;
        self.locals.insert(&parameter.declaration.id, slot);
        Ok(slot)
    }

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
        let text = owns_text(&iteration.yielded);
        let option_text = matches!(iteration.yielded.as_ref(), TypeTerm::Nominal { .. })
            && matches!(
                self.admitted_option(&iteration.yielded),
                Some((_, OptionPayload::Text(_)))
            );
        use crate::analysis::scalars::IterationBodyKind::{
            NamedBoolPostfix, Ordinary, YieldedTextPostfix,
        };
        let supported = match iteration.body {
            Some(Ordinary) => true,
            Some(NamedBoolPostfix) => text && exits.returning.is_some(),
            Some(YieldedTextPostfix) => option_text && exits.returning.is_some(),
            None => false,
        };
        if !supported
            || iteration.traversal != Some(ListTraversal::Owned)
            || (!text && !option_text && scalar::scalar(&iteration.yielded) != Some(Primitive::I32))
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
        // The scalar discard shortcut cannot retire an owning yield.
        if matches!(binding.pattern.as_ref(), Pattern::Wildcard)
            && (text
                || option_text
                || !self.fact(&body).is_some_and(|fact| {
                    !fact.flow.pending && !fact.flow.normal && fact.flow.continues.is_empty()
                }))
        {
            return Err(pending());
        }
        let actual = self.value(source).map_err(|_| pending())?;
        let (name, source_slot, staged) = if actual.path.is_none() {
            if !text {
                return Err(pending());
            }
            let staged = self.checked_list_text_source(node, source, actual, exits, depth + 1)?;
            let element = self.text_element_disposal(node)?;
            if !staged.can_commit() {
                return self.stage_list_text_source(staged, None);
            }
            let disposal = TextListDisposal {
                list: self.list_disposal(node)?,
                element,
            };
            let slot = self.allocate_slot(
                staged.literal,
                true,
                StorageCleanup::DisposeListText(disposal),
                None,
            )?;
            (staged.literal, slot, Some(staged))
        } else {
            let (name, _) = self.ungroup_expression_at(source, depth + 1)?;
            let (named, path) = self.named_owned_place(name)?.ok_or_else(pending)?;
            let actual = self.normal_value(source)?;
            let admitted_source = if text {
                self.is_list_text(&named.ty)
            } else if option_text {
                path.root.kind == DeclarationKind::Parameter
                    && matches!(named.ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
                        if self.typed.list_role().is_some_and(|role| std::ptr::eq(*declaration, &role.declaration.id))
                            && matches!(arguments.as_slice(), [element] if *element == iteration.yielded))
            } else {
                self.is_list_i32(&named.ty)
            };
            if !admitted_source
                || actual.ty != named.ty
                || actual.access != Access::Owned
                || !matches!(&actual.path, Some(p) if std::ptr::eq(p.root, path.root) && p.projections.is_empty())
                || !named.binding.is_some_and(|id| std::ptr::eq(id, path.root))
            {
                return Err(pending());
            }
            (name, self.resolve_place(name, path)?, None)
        };
        let source_cleanup = self.slots[source_slot].cleanup;
        let list_disposal = match source_cleanup {
            StorageCleanup::DisposeListI32(_) if !text && !option_text => None,
            StorageCleanup::DisposeListText(disposal) if text => Some(disposal.list),
            StorageCleanup::DisposeListOptionText {
                disposal,
                iteration: site,
            } if option_text && site == node => Some(disposal.list),
            _ => return Err(pending()),
        };
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
            &iteration.yielded,
            |output| *output == iteration.iterator,
        )?;
        let next_scope = self.iteration_scope(node, cursor, Next(Owned))?;
        let advance = self.selected_iteration_step(
            node,
            &iteration.next,
            next_scope,
            [&iteration.iterator, &iteration.yielded],
            &iteration.yielded,
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
            || !advance.artifact.same_link(open.artifact)
            || !release.artifact.same_link(open.artifact)
            || staged
                .as_ref()
                .is_some_and(|source| !source.provider.same_link(open.artifact))
        {
            return Err(pending());
        }
        let backing = self.iteration_backing(
            node,
            cursor,
            CollectionStorage::UnyieldedElements,
            open.artifact,
        )?;
        let release = ListDisposal {
            implementation: &release.implementation.id,
            bridge: &release.bridge.id,
        };
        let (source_backing, cursor_cleanup) = if let Some(disposal) = list_disposal {
            let source_release = self
                .iteration_scope(node, list, CollectionOperation::DisposeList)?
                .provider
                .ok_or_else(pending)?;
            if !source_release.artifact.same_link(open.artifact)
                || disposal.implementation != &source_release.implementation.id
                || disposal.bridge != &source_release.bridge.id
            {
                return Err(pending());
            }
            let list_backing =
                self.iteration_backing(node, list, CollectionStorage::Elements, open.artifact)?;
            let cleanup = match source_cleanup {
                StorageCleanup::DisposeListOptionText { disposal, .. } => {
                    StorageCleanup::DisposeOwnedListIterOptionText(OptionTextListDisposal {
                        list: release,
                        ..disposal
                    })
                }
                StorageCleanup::DisposeListText(disposal) => {
                    StorageCleanup::DisposeOwnedListIterText(TextListDisposal {
                        list: release,
                        ..disposal
                    })
                }
                _ => return Err(pending()),
            };
            (Some(list_backing), cleanup)
        } else {
            self.check_i32_element_cleanup(node)?;
            (None, StorageCleanup::DisposeOwnedListIterI32(release))
        };
        let into =
            self.iteration_binder(pattern, binding, source, &iteration.yielded, depth + 1)?;
        if let StorageCleanup::DisposeOwnedListIterOptionText(disposal) = cursor_cleanup {
            let element = disposal.element;
            let item = into.ok_or_else(pending)?;
            if self.slots[item].cleanup != StorageCleanup::DropOptionPayload(element.some) {
                return Err(pending());
            }
            self.slots[item].cleanup = StorageCleanup::DropYieldedOptionText(element);
        }
        let cursor = self.allocate_slot(node, true, cursor_cleanup, None)?;
        let retire = self.cleanup_scope(node, &[cursor], next)?;
        let return_retire = exits
            .returning
            .map(|pc| self.cleanup_scope(node, &[cursor], pc))
            .transpose()?;
        let head = self.emit(
            node,
            Action::AdvanceOwnedList {
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
        // Both explicit return and checked postfix failure export before taking
        // this edge: body scope, initialized item, unyielded suffix, parent.
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
        let entry = self.emit(
            name,
            Action::OpenOwnedList {
                source: source_slot,
                cursor,
                backing,
                source_backing,
                provider: open,
            },
            Edges::Next(head),
        )?;
        if let Some(staged) = staged {
            self.stage_list_text_source(
                staged,
                Some((source_slot, source_backing.ok_or_else(pending)?, entry)),
            )
        } else {
            Ok(entry)
        }
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

    fn iteration_backing(
        &mut self,
        node: NodeId,
        owner: &DeclarationId,
        storage: CollectionStorage,
        artifact: CollectionArtifact<'g>,
    ) -> ResultOf<CollectionRepresentation<'g>> {
        for repr in self.typed.collection_representations() {
            self.budget.step(1)?;
            if std::ptr::eq(&repr.declaration.id, owner)
                && repr.storage == storage
                && repr.artifact.same_capture(artifact)
            {
                return Ok(repr);
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
        yielded: &Type<'g>,
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
                        if element.position == 0 && &element.argument == yielded)
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

    pub(super) fn text_element_disposal(
        &mut self,
        node: NodeId,
    ) -> ResultOf<TextElementDisposal<'g>> {
        let pending = || Stop::Pending(node);
        let scopes = self.typed.primitive_protection();
        self.budget.step(scopes.len())?;
        let winner = scopes
            .iter()
            .find(|scope| {
                Some(scope.family) == self.dispose
                    && scope.role == PrimitiveRole::Dispose
                    && scope.primitive == Primitive::String
                    && scope.coverage == PrimitiveCoverage::Clear
            })
            .and_then(|scope| scope.required_winner)
            .ok_or_else(pending)?;
        let providers = self.typed.primitive_providers();
        self.budget.step(providers.len())?;
        for provider in providers {
            if Some(provider.family) == self.dispose
                && provider.role == PrimitiveRole::Dispose
                && provider.primitive == Primitive::String
                && std::ptr::eq(&provider.implementation.id, winner)
                && let Some(binding) = provider.binding
                && binding.operation == PrimitiveOperation::DisposeText
            {
                return Ok(TextElementDisposal {
                    implementation: winner,
                    bridge: &binding.declaration.id,
                });
            }
        }
        Err(pending())
    }

    fn iteration_binder(
        &mut self,
        pattern: NodeId,
        facts: &PatternFacts<'g>,
        source: NodeId,
        yielded: &Type<'g>,
        mut depth: usize,
    ) -> ResultOf<Option<Slot>> {
        let pending = Stop::Pending(pattern);
        let mut leaf = pattern;
        loop {
            self.depth(depth)?;
            let syntax = &self.module.syntax.nodes[leaf];
            if syntax.kind != NodeKind::GroupPattern {
                break;
            }
            leaf = syntax.children[0];
            depth += 1;
        }
        if facts.source.node != source
            || facts.ty != *yielded
            || facts.access != Access::Owned
            || !facts.coverage.exhaustive()
        {
            return Err(pending);
        }
        match (facts.pattern.as_ref(), facts.binders.as_slice()) {
            (Pattern::Wildcard, [])
                if self.module.syntax.nodes[leaf].kind == NodeKind::Wildcard =>
            {
                Ok(None)
            }
            (Pattern::Any, [binder])
                if self.module.syntax.nodes[leaf].kind == NodeKind::NamePattern
                    && !binder.mutable
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
