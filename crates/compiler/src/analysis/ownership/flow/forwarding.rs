use super::*;

impl<'g> CancellationTrace<'_, 'g> {
    pub(in crate::analysis::ownership) fn forwarding_call(
        &self,
        mir: &Mir<'g>,
    ) -> Option<(usize, &'g DeclarationId)> {
        let block = self.retaining_call_block()?;
        let Operation::Call(callee) = mir.blocks[block].operation else {
            return None;
        };
        let Action::Call(call) = self.code[self.known.blocks[block].pc].action else {
            unreachable!("call block records a reached call");
        };
        (self.params.len() == 1
            && self.calls[call].forwarding_prefix.is_some()
            && self.calls[call].args.len() == 1
            && callee.kind == DeclarationKind::Function)
            .then_some((block, callee))
    }

    pub(in crate::analysis::ownership) fn finish_forwarding_caller(
        &self,
        mir: &Mir<'g>,
        block: usize,
        parameter: Place<'g>,
        budget: &mut Budget,
    ) -> ResultOf<Option<(Cancellation<'g>, SuspendedForwardingFrame<'g>)>> {
        budget.step(1)?;
        let Instruction {
            node,
            action: Action::Call(call),
            ..
        } = self.code[self.known.blocks[block].pc]
        else {
            unreachable!("forwarding block records a reached call");
        };
        let call = &self.calls[call];
        let prefix = call
            .forwarding_prefix
            .expect("admitted written forwarding call");
        // Only an eligible forwarding chain needs to inspect the unlowered
        // suffix. Dead bindings have no slots and do not guard an earlier call.
        let suffix = &self.module.syntax.nodes[self.body].children[prefix..];
        budget.step(suffix.len())?;
        if !suffix.iter().all(|&statement| {
            matches!(
                self.module.syntax.nodes[statement].kind,
                crate::parser::NodeKind::ExpressionStatement
                    | crate::parser::NodeKind::Binding { .. }
                    | crate::parser::NodeKind::Return
            )
        }) {
            return Ok(None);
        }
        let &[stage] = call.args.as_slice() else {
            unreachable!("forwarding call has one staged argument");
        };
        let Some(NodeFactKind::Expression(facts)) = self.fact_kind(node) else {
            return Ok(None);
        };
        let Some(facts) = facts.call.as_ref() else {
            return Ok(None);
        };
        if !std::ptr::eq(facts.callee, call.callee) || facts.pipe_input.is_some() {
            return Ok(None);
        }
        // Lowering recorded the actual straight-line call, not a call hidden
        // in a pruned branch. Check the formal against the transferred stage.
        if self.branch_parameter(self.slots[stage].node, budget)? != Some(self.params[0]) {
            return Ok(None);
        }
        let state = self.known.input(block);
        if !initialized(&state[stage], node)?.loans.is_empty() {
            return Ok(None);
        }
        budget.step(state.len())?;
        budget.reserve(state.len())?;
        let mut suspended = state.to_vec();
        suspended[stage] = State::Vacant;
        let frame = self.frame();
        if !frame.check(&suspended, node, budget)? {
            return Ok(None);
        }
        let Some(entry) = frame.entry(self.known.input(mir.entry), self.body, budget)? else {
            return Ok(None);
        };
        let exit = frame.exit(Cow::Owned(suspended), node, budget)?;
        budget.reserve(1)?;
        Ok(Some((
            entry,
            SuspendedForwardingFrame {
                argument: CancellationArgument {
                    staged: self.slots[stage].place(self.module),
                    parameter,
                },
                caller_cleanup: exit.cleanup,
            },
        )))
    }
}

/// The targeted worklist holds this one ordinary body until it has checked
/// the exact retaining leaf. It never enables an arbitrary transitive call.
pub(in crate::analysis::ownership) struct ForwardingCallee<'a, 'g> {
    pub(in crate::analysis::ownership) leaf: &'g DeclarationId,
    middle: &'g DeclarationId,
    retaining_block: usize,
    body: Mir<'g>,
    trace: CancellationTrace<'a, 'g>,
}

impl<'a, 'g> ForwardingCallee<'a, 'g> {
    pub(in crate::analysis::ownership) fn new(
        middle: &'g DeclarationId,
        body: Mir<'g>,
        trace: CancellationTrace<'a, 'g>,
    ) -> Option<Self> {
        if trace.params.len() != 1 {
            return None;
        }
        let retaining_block = trace.retaining_call_block()?;
        let Operation::CallRetainingWithBool {
            callee: leaf,
            ref arguments,
            second_input: None,
            ..
        } = body.blocks[retaining_block].operation
        else {
            return None;
        };
        (arguments.len() == 3 && !std::ptr::eq(middle, leaf)).then_some(Self {
            leaf,
            middle,
            retaining_block,
            body,
            trace,
        })
    }

    pub(in crate::analysis::ownership) fn finish(
        mut self,
        (caller, mir, block): (&CancellationTrace<'_, 'g>, &Mir<'g>, usize),
        (leaf_trace, leaf): (CancellationTrace<'_, 'g>, Mir<'g>),
        rejected_calls: &mut Option<[Site<'g>; 2]>,
        budget: &mut Budget,
    ) -> Result<Option<Cancellation<'g>>, (&'g Module, Stop)> {
        let middle_error = |stop| (self.trace.module, stop);
        if !leaf_trace.forwarding_backedge || leaf_trace.retaining_parameters().is_none() {
            return Ok(None);
        }
        let call = match self.trace.prepare_retaining_call(&self.body, budget) {
            Ok(Some(call)) => call,
            Err(Stop::Limit) => return Err(middle_error(Stop::Limit)),
            _ => return Ok(None),
        };
        let Some(parameter) = call.forwarded_parameter() else {
            return Ok(None);
        };
        let (mut entry, suspended) =
            match caller.finish_forwarding_caller(mir, block, parameter, budget) {
                Ok(Some(proof)) => proof,
                Err(Stop::Limit) => return Err((caller.module, Stop::Limit)),
                _ => return Ok(None),
            };
        let proof = match call.finish(leaf_trace, leaf, budget) {
            Ok(Some(proof)) => proof,
            Err(
                error @ (
                    _,
                    Stop::Rejected {
                        kind: Conflict::CancellationEscape { .. },
                        ..
                    },
                ),
            ) => {
                budget.reserve(1).map_err(|()| middle_error(Stop::Limit))?;
                *rejected_calls = Some([
                    mir.blocks[block].site,
                    self.body.blocks[self.retaining_block].site,
                ]);
                return Err(error);
            }
            Err(error @ (_, Stop::Limit)) => return Err(error),
            _ => return Ok(None),
        };
        let Some(CancellationSuccessor::RetainingCall(call)) =
            proof.successor(self.retaining_block)
        else {
            unreachable!("checked middle proof links the retaining call");
        };
        budget.step(1).map_err(|()| middle_error(Stop::Limit))?;
        let Some(leaf_checkpoint) = call
            .body
            .cancellation()
            .and_then(|leaf| leaf.sole_backedge_site(&call.body))
        else {
            return Ok(None);
        };
        budget.reserve(2).map_err(|()| middle_error(Stop::Limit))?;
        self.body.cancellation = Some(proof);
        entry.successors.insert(
            block,
            CancellationSuccessor::ForwardedRetainingCall(Box::new(
                ForwardedRetainingCallCancellation {
                    callee: self.middle,
                    body: self.body,
                    leaf_checkpoint,
                    suspended,
                },
            )),
        );
        Ok(Some(entry))
    }
}

impl<'g> Cancellation<'g> {
    pub(in crate::analysis::ownership) fn sole_backedge_site(
        &self,
        mir: &Mir<'g>,
    ) -> Option<Site<'g>> {
        // The admitted retaining leaf must have a real feasible body delivery.
        // Its entry proof is insufficient, as is an ordinary reachable block.
        let (&block, CancellationSuccessor::LoopBackedge { .. }) =
            self.successors.first_key_value()?
        else {
            return None;
        };
        (self.successors.len() == 1).then(|| mir.blocks[block].site)
    }
}
