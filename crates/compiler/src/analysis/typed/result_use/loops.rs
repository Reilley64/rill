//! Loop worklist and transfer targets. A jump leaves only its target's iteration scopes.
use super::*;
use std::collections::VecDeque;

pub(super) struct LoopExits<'g> {
    pub(super) node: NodeId,
    dead_depth: usize,
    breaks: Alternatives<'g>,
    continues: Alternatives<'g>,
}

impl<'a, 'g> Check<'a, 'g> {
    fn owned_literal_iteration(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        use crate::analysis::typed::ListTraversal;
        self.depth(depth)?;
        let Some(NodeFactKind::Iteration(iteration)) = self.facts(node)?.map(|f| &f.kind) else {
            self.pending(node, ResultUseBlocker::Control)?;
            return self.singleton(Residence::default());
        };
        let Some(NodeFactKind::Expression(source)) = self.facts(iteration.source)?.map(|f| &f.kind)
        else {
            self.pending(node, ResultUseBlocker::TypedInput)?;
            return self.singleton(Residence::default());
        };
        let Some(DataExpression::List { elements, .. }) = &source.data else {
            self.pending(node, ResultUseBlocker::Containment)?;
            return self.singleton(Residence::default());
        };
        if iteration.traversal != Some(ListTraversal::Owned) {
            self.pending(node, ResultUseBlocker::Control)?;
            return self.singleton(Residence::default());
        }
        let Some(NodeFactKind::Pattern(pattern)) = self.facts(iteration.pattern)?.map(|f| &f.kind)
        else {
            self.pending(node, ResultUseBlocker::Storage)?;
            return self.singleton(Residence::default());
        };
        let body = self.module.syntax.nodes[node].children[2];
        let iterator = Root::Temporary(node);
        let source = Root::Temporary(iteration.source);
        let mut running = self.expression(iteration.source, state, depth + 1)?;
        if elements.is_empty() || running.is_empty() {
            self.dead_statement(body, Residence::default(), depth + 1)?;
        }
        for state in &mut running {
            self.move_value(state, source, &[], iterator, &[])?;
        }
        let mut exits = Vec::new();
        for index in 0..elements.len() {
            self.budget.step(1)?;
            if running.is_empty() {
                break;
            }
            let mut next = Vec::new();
            for mut state in running {
                let yielded = [ResultComponent::ListElement(index)];
                self.commit_pattern(&mut state, iterator, &yielded, pattern)?;
                let remaining = state.take(iterator, &yielded, self.budget)?;
                self.discard(
                    remaining,
                    iterator,
                    iteration.pattern,
                    ResultDiscardKind::ScopeExit,
                )?;
                let mut bound = Vec::new();
                let jumped = self.with_loop(node, |check| {
                    bound = check.statement(body, state, depth + 1)?;
                    Ok(())
                })?;
                for state in &mut bound {
                    for binder in &pattern.binders {
                        let root = Root::Local(binder.declaration);
                        let values = state.take(root, &[], self.budget)?;
                        self.discard(values, root, body, ResultDiscardKind::ScopeExit)?;
                    }
                }
                next.extend(bound);
                next.extend(jumped.continues);
                exits.extend(jumped.breaks);
            }
            running = next;
        }
        // Each Some edge moved one known element. Only the fully consumed
        // paths reach this None edge; breaks were already recorded separately.
        exits.extend(running);
        Ok(exits)
    }

    pub(super) fn while_loop(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let children = &self.module.syntax.nodes[node].children;
        let (condition, body) = (children[0], children[1]);
        if self.expression_facts(condition)?.is_none() {
            self.pending(node, ResultUseBlocker::Control)?;
            self.dead_statement(body, Residence::default(), depth + 1)?;
            return self.singleton(Residence::default());
        }
        let entry = state.fork(self.budget)?;
        let mut queue = VecDeque::from(self.singleton(state)?);
        let mut seen = Vec::new();
        let mut exits = Vec::new();
        let mut entered = false;
        while let Some(mut state) = queue.pop_front() {
            if !self.first_visit(&mut state, &mut seen)? {
                continue;
            }
            // The condition can itself transfer to this loop.
            let jumped = self.with_loop(node, |check| {
                for (truth, state) in check.branch_condition(condition, state, depth + 1)? {
                    if truth {
                        entered = true;
                        queue.extend(check.statement(body, state, depth + 1)?);
                    } else {
                        exits.extend(check.singleton(state)?);
                    }
                }
                Ok(())
            })?;
            queue.extend(jumped.continues);
            exits.extend(jumped.breaks);
        }
        if !entered {
            self.dead_statement(body, entry, depth + 1)?;
        }
        Ok(exits)
    }

    pub(super) fn for_loop(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let &[_, source, body] = self.module.syntax.nodes[node].children.as_slice() else {
            unreachable!()
        };
        let iteration = self.facts(node)?.and_then(|facts| match &facts.kind {
            NodeFactKind::Iteration(iteration) => Some(iteration),
            _ => None,
        });
        if let Some(iteration) = iteration
            && iteration.traversal == Some(crate::analysis::typed::ListTraversal::Owned)
            && self
                .expression_facts(iteration.source)?
                .is_some_and(|source| matches!(source.data, Some(DataExpression::List { .. })))
        {
            return self.owned_literal_iteration(node, state, depth);
        }
        if let Some(iteration) = iteration
            && iteration.traversal == Some(crate::analysis::typed::ListTraversal::Owned)
            && matches!(iteration.yielded.as_ref(), TypeTerm::Primitive(p) if p.name() != "never")
            && self
                .facts(iteration.pattern)?
                .is_some_and(|fact| matches!(fact.kind, NodeFactKind::Pattern(_)))
        {
            return self.owned_scalar_iteration(node, iteration.source, body, state, depth);
        }
        self.pending(
            node,
            if iteration.is_some() {
                ResultUseBlocker::Storage
            } else {
                ResultUseBlocker::Control
            },
        )?;
        let source = iteration.map_or(source, |handoff| handoff.source);
        let source_paths = self.expression(source, state, depth + 1)?;
        self.dead_statement(body, Residence::default(), depth + 1)?;
        // A conditional iterate/next pair does not prove either Some or
        // None. Check the following lexical source, not an iterator exit.
        if source_paths.is_empty() {
            Ok(Vec::new())
        } else {
            self.singleton(Residence::default())
        }
    }

    fn owned_scalar_iteration(
        &mut self,
        node: NodeId,
        source: NodeId,
        body: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut initial = self.expression(source, state, depth + 1)?;
        if initial.is_empty() {
            self.dead_statement(body, Residence::default(), depth + 1)?;
        }
        for state in &mut initial {
            state.take(Root::Temporary(source), &[], self.budget)?;
        }
        let mut queue = VecDeque::from(initial);
        let mut seen = Vec::new();
        let mut exits = Vec::new();
        while let Some(mut state) = queue.pop_front() {
            if !self.first_visit(&mut state, &mut seen)? {
                continue;
            }
            // None has no retained Result, but any outer residence remains.
            self.budget.reserve(1)?;
            exits.push(state.fork(self.budget)?);
            let jumped = self.with_loop(node, |check| {
                queue.extend(check.statement(body, state, depth + 1)?);
                Ok(())
            })?;
            queue.extend(jumped.continues);
            exits.extend(jumped.breaks);
        }
        Ok(exits)
    }

    fn first_visit(
        &mut self,
        state: &mut Residence<'g>,
        seen: &mut Alternatives<'g>,
    ) -> Work<bool> {
        state.normalize(self.budget)?;
        for old in seen.iter() {
            if state.same_as(old, self.budget)? {
                return Ok(false);
            }
        }
        self.budget.reserve(1)?;
        seen.push(state.fork(self.budget)?);
        Ok(true)
    }

    fn with_loop(
        &mut self,
        node: NodeId,
        run: impl FnOnce(&mut Self) -> Work<()>,
    ) -> Work<LoopExits<'g>> {
        self.budget.reserve(1)?;
        self.loops.push(LoopExits {
            node,
            dead_depth: self.dead_depth,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        let result = run(self);
        let exits = self.loops.pop().unwrap();
        result?;
        Ok(exits)
    }

    pub(super) fn loop_jump(
        &mut self,
        node: NodeId,
        mut state: Residence<'g>,
    ) -> Work<Alternatives<'g>> {
        let Some(facts) = self.facts(node)? else {
            self.pending(node, ResultUseBlocker::Control)?;
            return Ok(Vec::new());
        };
        let (target, is_break) = match facts.kind {
            NodeFactKind::Break { target } => (target, true),
            NodeFactKind::Continue { target } => (target, false),
            _ => unreachable!("checked loop jump has a control target"),
        };
        let mut active = None;
        for (index, scope) in self.loops.iter().enumerate().rev() {
            self.budget.step(1)?;
            if scope.node == target && scope.dead_depth == self.dead_depth {
                active = Some(index);
                break;
            }
        }
        let Some(active) = active else {
            return Ok(Vec::new());
        };
        let span = &self.module.syntax.nodes[target].span;
        let range = span.start()..span.end();
        let removed = state.take_scope(
            |root| match root {
                Root::Local(id) => self
                    .scopes
                    .get(id)
                    .and_then(|scope| *scope)
                    .is_some_and(|scope| range.contains(&scope)),
                Root::Temporary(temporary) => {
                    // The iterator itself survives continue, but staged expression
                    // operands and inner-loop storage do not survive either jump.
                    temporary == target && is_break
                        || temporary != target
                            && range.contains(&self.module.syntax.nodes[temporary].span.start())
                }
            },
            self.budget,
        )?;
        for (root, value) in removed {
            self.budget.reserve(1)?;
            self.discard(vec![value], root, node, ResultDiscardKind::ScopeExit)?;
        }
        self.budget.reserve(1)?;
        let active = &mut self.loops[active];
        if is_break {
            active.breaks.push(state);
        } else {
            active.continues.push(state);
        }
        Ok(Vec::new())
    }
}
