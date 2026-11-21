//! Distinguish evaluation of an argument from commitment at invocation.
use super::*;

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn call(
        &mut self,
        node: NodeId,
        value: &'a ExpressionFacts<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let Some(call) = &value.call else {
            return self.invoke_value(node, value, state, depth);
        };
        self.budget.step(1)?;
        let settled = value.ordinary_context.is_none();
        let observer = settled && Some(call.callee) == self.core.outcome_observer;
        self.budget.step(self.core.collections.protection.len())?;
        let dispose = settled
            && (*call.callee == self.inventory.graph().prelude().dispose
                || self.core.collections.is_disposal_bridge(call.callee));
        let mut states = self.singleton(state)?;
        for argument in call.argument_evaluation_order() {
            states = self.stage_argument(argument, states, depth + 1)?;
        }
        if settled
            && let Some(DataExpression::CallableCopy(copy)) = &value.data
            && let [argument] = call.arguments.as_slice()
        {
            // The borrow is evaluated, but neither it nor a Result residence
            // enters the new owner: the checked fields are independent primitives.
            let borrowed = Root::Temporary(*argument);
            for state in &mut states {
                let contents = state.take(borrowed, &[], self.budget)?;
                self.discard(contents, borrowed, *argument, ResultDiscardKind::Expression)?;
                state.store(
                    Root::Temporary(node),
                    &[],
                    Content::Callable(copy.origin.node),
                    self.budget,
                )?;
            }
            return Ok(states);
        }
        self.budget.step(self.core.collections.protection.len())?;
        if settled
            && let [argument] = call.arguments.as_slice()
            && self
                .core
                .collections
                .forwards_list_to_owned_iterator(call.callee)
        {
            // This backing operation changes the owner, not the element residences.
            for state in &mut states {
                self.move_value(
                    state,
                    Root::Temporary(*argument),
                    &[],
                    Root::Temporary(node),
                    &[],
                )?;
            }
            return Ok(states);
        }
        // Only the designated winner can transfer work to children. A legal
        // application winner in this family keeps its ordinary call contract.
        let scoped = self.core.routine.is_some_and(|role| {
            role.needs_scoped_evidence(
                call.callee,
                call.selection.as_ref().map(|s| s.implementation),
            )
        });
        let boundary = if scoped {
            self.scoped
                .map(|(role, slots)| pairs::PairBoundary::Scoped(role, slots))
        } else {
            call.selection
                .as_ref()
                .and_then(|selection| self.pairs.get(selection.implementation))
                .copied()
                .map(pairs::PairBoundary::Serial)
        };
        if settled && let Some(boundary) = boundary {
            let mut forwarded = Vec::new();
            let mut pending = Vec::new();
            for mut state in states {
                self.budget.reserve(1)?;
                if self.transfer_pair(node, value, call, &mut state, boundary, depth + 1)? {
                    forwarded.push(state);
                } else {
                    self.pending(node, ResultUseBlocker::CallBoundary)?;
                    self.finish_call(
                        node,
                        value,
                        &mut state,
                        call.argument_evaluation_order(),
                        (false, false, false),
                    )?;
                    pending.push(state);
                }
            }
            forwarded.extend(self.call_output(node, value, pending)?);
            return Ok(forwarded);
        }
        if scoped {
            self.pending(node, ResultUseBlocker::CallBoundary)?;
        }
        for state in &mut states {
            let checked = settled && !scoped && self.ordinary_boundary(call, state, depth + 1)?;
            if settled && !checked && self.contracts.contains_key(call.callee) {
                self.pending(node, ResultUseBlocker::CallBoundary)?;
            }
            self.finish_call(
                node,
                value,
                state,
                call.argument_evaluation_order(),
                (checked, observer, dispose),
            )?;
        }
        if !settled {
            self.pending(node, ResultUseBlocker::CallBoundary)?;
            // Argument evaluation is separate from the conditional winner.
            // Its hypothetical body cannot prove an outgoing residence.
            return if states.is_empty() {
                Ok(states)
            } else {
                self.singleton(Residence::default())
            };
        }
        self.call_output(node, value, states)
    }

    fn stage_argument(
        &mut self,
        argument: NodeId,
        states: Alternatives<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.budget.step(1)?;
        let mut next = Vec::new();
        for state in states {
            next.extend(self.expression(argument, state, depth)?);
        }
        Ok(next)
    }

    fn finish_call(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        state: &mut Residence<'g>,
        arguments: impl Iterator<Item = NodeId>,
        (checked, observer, dispose): (bool, bool, bool),
    ) -> Work<()> {
        let borrows_can_escape = !matches!(value.ty.as_ref(), TypeTerm::Primitive(_));
        self.budget.step(1)?;
        let symbolic_parameter = self.release_parameters.get(&(&self.module.key, node));
        let native_release = match &value.data {
            Some(DataExpression::NativeOwnership(call)) => releases::release_blocker(call.action),
            _ => None,
        };
        for argument in arguments {
            let root = Root::Temporary(argument);
            let referents = state.aliases_under(root, &[], self.budget)?;
            let exact = state.alias(root, &[], self.budget)?;
            if observer && let Some((owner, path)) = &exact {
                let ok = Constructor::Variant(self.core.result.success);
                if let Some(outcome) = state.test(*owner, path, &ok, self.budget)? {
                    self.record_boolean(state, Root::Temporary(node), &[], outcome)?;
                }
                state.observe(*owner, path, self.budget)?;
            }
            let ty = self
                .expression_facts(argument)?
                .map(|value| value.ty.as_ref());
            let bool_descriptor = ty.is_some_and(|ty| retained_bool_contents_type(ty).is_some());
            let mutable_referent = ty.and_then(|ty| match ty {
                TypeTerm::Reference {
                    mutable: true,
                    referent,
                } => Some(referent.as_ref()),
                _ => None,
            });
            let primitive_tuple = if !referents.is_empty()
                && !observer
                && let Some(TypeTerm::Tuple(elements)) = mutable_referent
            {
                self.budget.step(elements.len())?;
                elements
                    .iter()
                    .all(|element| matches!(element.as_ref(), TypeTerm::Primitive(_)))
            } else {
                false
            };
            for address @ (owner, _) in referents {
                if observer && exact.is_none() || mutable_referent.is_some() && !observer {
                    let boolean_storage =
                        primitive_tuple || matches!(mutable_referent, Some(TypeTerm::Primitive(_)));
                    let exact_descriptor = if bool_descriptor && let Some(exact) = &exact {
                        self.budget.step(1 + exact.1.len().min(address.1.len()))?;
                        exact == &address
                    } else {
                        false
                    };
                    if boolean_storage || exact_descriptor {
                        // Mutation can invalidate Boolean refinements in
                        // primitive storage or an exact descriptor alias.
                        // Neither exception acknowledges a Result residence.
                        let affected = state.take(address.0, &address.1, self.budget)?;
                        self.budget.step(affected.len())?;
                        let stale = |entry: &Located<'g>| match &entry.content {
                            Content::Test(tag, _) if boolean_storage => {
                                matches!(tag.as_ref(), Constructor::Boolean(_))
                            }
                            Content::Alias(_) | Content::PossibleAlias(_) => exact_descriptor,
                            // Preserve ordinary hint invalidation on tuple storage.
                            Content::CallerBoolOrigin(_) | Content::ConsumedPair => primitive_tuple,
                            _ => false,
                        };
                        if affected.iter().any(|entry| !stale(entry)) {
                            self.pending(argument, ResultUseBlocker::Storage)?;
                        }
                    } else {
                        self.forget_address(state, &address, argument)?;
                    }
                } else if !observer && borrows_can_escape {
                    state.store(
                        Root::Temporary(node),
                        &[],
                        Content::PossibleAlias(address),
                        self.budget,
                    )?;
                    continue;
                }
                if let Root::Temporary(operand) = owner {
                    let span = self.site(argument).span;
                    if (span.start()..span.end()).contains(&self.site(operand).span.start()) {
                        let remaining = state.take(owner, &[], self.budget)?;
                        self.discard(remaining, owner, argument, ResultDiscardKind::Expression)?;
                    }
                }
            }
            let mut passed = state.take(root, &[], self.budget)?;
            self.budget.step(passed.len())?;
            if let Some(parameter) = symbolic_parameter {
                // Export the effect on the formal at its proved delegate.
                // Other origins, including any concrete occurrence, stay live.
                passed.retain(|value| value.content != Content::Unclassified(*parameter));
            }
            if let Some(blocker) = native_release {
                if passed
                    .iter()
                    .any(|v| matches!(v.content, Content::Result(_) | Content::Unclassified(_)))
                {
                    self.pending(node, blocker)?;
                }
            } else if dispose {
                self.discard(passed, root, node, ResultDiscardKind::Expression)?;
            } else if !checked
                && passed
                    .iter()
                    .any(|v| matches!(v.content, Content::Result(_) | Content::Unclassified(_)))
            {
                self.pending(node, ResultUseBlocker::CallBoundary)?;
            }
        }
        Ok(())
    }

    fn call_output(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        states: Alternatives<'g>,
    ) -> Work<Alternatives<'g>> {
        if let Some(DataExpression::Propagation(branch)) = &value.data {
            // The typed pipe call has already committed its arguments. Its Err
            // edge forwards the wrapper, not the caller's unrelated residences.
            if branch.can_return {
                for state in &states {
                    let state = state.fork(self.budget)?;
                    self.exit(state, node)?;
                }
            }
            if !branch.can_continue {
                return Ok(Vec::new());
            }
        }
        self.seed(states, Root::Temporary(node), node, &value.ty)
    }

    fn invoke_value(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let Some(invocation) = self
            .facts(node)?
            .and_then(|f| f.callable.as_ref())
            .and_then(|f| f.invocation.as_ref())
        else {
            self.pending(node, ResultUseBlocker::CallBoundary)?;
            return self.unsettled(node, depth);
        };
        let receiver = invocation.receiver;
        // The typed invocation identifies the receiver. Its contract alone does
        // not prove Result use for an erased callable or for owned captures.
        let syntax = &self.module.syntax.nodes[node];
        let pair_receiver = if syntax.kind == NodeKind::Call
            && syntax.children.as_slice() == [receiver]
            && invocation.mode == crate::analysis::InvocationMode::Once
            && invocation.access == Access::Owned
            && invocation.assumption.is_none()
            && let Some(path) = &invocation.path
            && path.root.kind == DeclarationKind::Binding
            && path.projections.is_empty()
            && let TypeTerm::Tuple(elements) = value.ty.as_ref()
            && elements.len() == 2
        {
            let name = self.ungroup_value(receiver)?;
            self.budget.step(3)?;
            (self.module.syntax.nodes[name].kind == NodeKind::ValueReference
                && elements.iter().all(|element| match element.as_ref() {
                    TypeTerm::Primitive(_) => true,
                    TypeTerm::Reference {
                        mutable: false,
                        referent,
                    } => matches!(referent.as_ref(), TypeTerm::Primitive(_)),
                    _ => false,
                }))
            .then_some(Root::Local(path.root))
        } else {
            None
        };
        let arguments = if syntax.kind == NodeKind::Call {
            &syntax.children[1..]
        } else {
            let [_, step] = syntax.children.as_slice() else {
                unreachable!()
            };
            self.module.syntax.nodes[*step]
                .children
                .get(1)
                .map_or(&[][..], |&list| {
                    self.module.syntax.nodes[list].children.as_slice()
                })
        };
        let pipe_input = (syntax.kind == NodeKind::Pipe).then_some(syntax.children[0]);
        let args = || {
            pipe_input.into_iter().chain(
                arguments
                    .iter()
                    .copied()
                    .filter(|&arg| self.module.syntax.nodes[arg].kind != NodeKind::PipePlaceholder),
            )
        };
        let initial = if let Some(input) = pipe_input {
            self.expression(input, state, depth + 1)?
        } else {
            self.singleton(state)?
        };
        let root = Root::Temporary(receiver);
        let mut states = Vec::new();
        let mut repeated = Vec::new();
        for mut state in initial {
            if let Some(local) = pair_receiver
                && self
                    .address(&state, invocation.path.as_ref())?
                    .is_some_and(|(exact, parts)| exact == local && parts.is_empty())
                && matches!(state.contents(local, &[], self.budget)?.as_slice(), [value]
                    if value.component.is_empty() && matches!(value.content, Content::ConsumedPair))
            {
                // No environment remains to evaluate and there are no
                // Result inputs or outputs. Keep checking unrelated
                // residences; M5 diagnoses the invalid ownership use.
                self.budget.reserve(1)?;
                repeated.push(state);
                continue;
            }
            if invocation.mode != crate::analysis::InvocationMode::Once
                && let Some(address) = self.address(&state, invocation.path.as_ref())?
            {
                state.store(root, &[], Content::Alias(address), self.budget)?;
                states.extend(self.singleton(state)?);
            } else {
                states.extend(self.expression(receiver, state, depth + 1)?);
            }
        }
        for argument in args().skip(usize::from(pipe_input.is_some())) {
            states = self.stage_argument(argument, states, depth + 1)?;
        }
        for state in &mut states {
            let address = state
                .alias(root, &[], self.budget)?
                .unwrap_or((root, Vec::new()));
            let values = state.contents(address.0, &address.1, self.budget)?;
            let mut consumed = None;
            if let Some(local) = pair_receiver
                && address.0 == root
                && address.1.is_empty()
                && let [value] = values.as_slice()
                && value.component.is_empty()
                && let Content::Callable(origin) = value.content
                && self
                    .address(state, invocation.path.as_ref())?
                    .is_some_and(|(exact, parts)| exact == local && parts.is_empty())
                && state.contents(local, &[], self.budget)?.is_empty()
                && let Some(arrow) = self
                    .facts(origin)?
                    .and_then(|f| f.callable.as_ref()?.arrow.as_deref())
                    .filter(|arrow| arrow.site.node == origin && arrow.premise.is_none())
                && self.primitive_capture_pair(origin, arrow)?
            {
                consumed = Some(local);
            }
            let checked = if invocation.assumption.is_some() {
                self.symbolic_callback(invocation, &values)?
            } else {
                self.prove_actual(values, invocation.mode, depth + 1)?
            };
            for referent in state.aliases_under(address.0, &address.1, self.budget)? {
                self.forget_address(state, &referent, node)?;
            }
            state.take(root, &[], self.budget)?;
            self.finish_call(node, value, state, args(), (checked, false, false))?;
            if !checked {
                self.pending(node, ResultUseBlocker::CallBoundary)?;
            } else if let Some(local) = consumed {
                state.store(local, &[], Content::ConsumedPair, self.budget)?;
            }
        }
        states.append(&mut repeated);
        self.call_output(node, value, states)
    }
}
