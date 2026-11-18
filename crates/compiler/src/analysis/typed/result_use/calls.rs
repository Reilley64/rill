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
        let checked = settled && self.checked.contains(call.callee);
        let observer = settled && Some(call.callee) == self.core.outcome_observer;
        self.budget.step(self.core.collections.protection.len())?;
        let dispose = settled
            && (*call.callee == self.inventory.graph().prelude().dispose
                || self.core.collections.is_disposal_bridge(call.callee));
        let mut states = self.singleton(state)?;
        for argument in call.argument_evaluation_order() {
            states = self.stage_argument(argument, states, depth + 1)?;
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
        for state in &mut states {
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
            let mutable_reference = self.expression_facts(argument)?.is_some_and(|value| {
                matches!(value.ty.as_ref(), TypeTerm::Reference { mutable: true, .. })
            });
            for address @ (owner, _) in referents {
                if observer && exact.is_none() || mutable_reference && !observer {
                    self.forget_address(state, &address, argument)?;
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
            let passed = state.take(root, &[], self.budget)?;
            if dispose {
                self.discard(passed, root, node, ResultDiscardKind::Expression)?;
            } else if !checked {
                self.budget.step(passed.len())?;
                if passed
                    .iter()
                    .any(|v| matches!(v.content, Content::Result(_) | Content::Unclassified(_)))
                {
                    self.pending(node, ResultUseBlocker::CallBoundary)?;
                }
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
        let named = self
            .facts(receiver)?
            .and_then(|f| f.callable.as_ref())
            .and_then(|f| f.function_value.as_ref());
        let arrow = self
            .facts(receiver)?
            .and_then(|f| f.callable.as_ref())
            .and_then(|f| f.arrow.as_ref());
        let checked = invocation.assumption.is_none()
            && (named.is_some_and(|value| self.checked.contains(value.declaration))
                || arrow.is_some_and(|arrow| arrow.premise.is_none()));
        // The shared checker has attached the invocation to this receiver. It has
        // not proved a source contract for an erased callable.
        let syntax = &self.module.syntax.nodes[node];
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
        for mut state in initial {
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
            for referent in state.aliases_under(address.0, &address.1, self.budget)? {
                self.forget_address(state, &referent, node)?;
            }
            state.take(root, &[], self.budget)?;
            self.finish_call(node, value, state, args(), (checked, false, false))?;
        }
        if !checked {
            self.pending(node, ResultUseBlocker::CallBoundary)?;
        }
        self.call_output(node, value, states)
    }
}
