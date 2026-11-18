//! Worklist for typed while edges. A break or continue exits the loop body's scopes.
use super::*;
use crate::analysis::scalars::BooleanSet;
use std::collections::VecDeque;

pub(super) struct LoopExits<'g> {
    pub(super) node: NodeId,
    body: NodeId,
    dead_depth: usize,
    breaks: Alternatives<'g>,
    continues: Alternatives<'g>,
    owned_iteration: Option<OwnedIteration<'g>>,
}

#[derive(Clone, Copy)]
struct OwnedIteration<'g> {
    iterator: Root<'g>,
    binder: Option<&'g DeclarationId>,
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn owned_literal_iteration(
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
        let binder = match pattern.binders.as_slice() {
            [binder] if binder.projections.is_empty() => Some(binder.declaration),
            [] if self.module.syntax.nodes[iteration.pattern].kind == NodeKind::Wildcard => None,
            _ => {
                self.pending(node, ResultUseBlocker::Storage)?;
                return self.singleton(Residence::default());
            }
        };
        let body = self.module.syntax.nodes[node].children[2];
        let iterator = Root::Temporary(iteration.source);
        let ownership = OwnedIteration { iterator, binder };
        let mut running = self.expression(iteration.source, state, depth + 1)?;
        let mut exits = Vec::new();
        for index in 0..elements.len() {
            self.budget.step(1)?;
            if running.is_empty() {
                break;
            }
            let mut next = Vec::new();
            for mut state in running {
                let yielded = [ResultComponent::ListElement(index)];
                if let Some(binder) = binder {
                    self.move_value(&mut state, iterator, &yielded, Root::Local(binder), &[])?;
                } else {
                    state.take(iterator, &yielded, self.budget)?;
                }
                self.start_loop(node, body, Some(ownership))?;
                let mut bound = self.statement(body, state, depth + 1)?;
                let jumped = self.loops.pop().unwrap();
                if let Some(binder) = binder {
                    for state in &mut bound {
                        let root = Root::Local(binder);
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

    fn start_loop(
        &mut self,
        node: NodeId,
        body: NodeId,
        owned_iteration: Option<OwnedIteration<'g>>,
    ) -> Work<()> {
        self.budget.reserve(1)?;
        self.loops.push(LoopExits {
            node,
            body,
            dead_depth: self.dead_depth,
            breaks: Vec::new(),
            continues: Vec::new(),
            owned_iteration,
        });
        Ok(())
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
        let Some(NodeFactKind::Expression(fact)) = self.facts(condition)?.map(|f| &f.kind) else {
            self.pending(node, ResultUseBlocker::Control)?;
            return self.singleton(Residence::default());
        };
        let truth = fact.boolean.unwrap_or(BooleanSet::BOTH);
        let mut queue = VecDeque::from(self.singleton(state)?);
        let mut seen = Vec::new();
        let mut exits = Vec::new();
        'work: while let Some(mut state) = queue.pop_front() {
            state.normalize(self.budget)?;
            for old in &seen {
                if state.same_as(old, self.budget)? {
                    continue 'work;
                }
            }
            self.budget.reserve(1)?;
            seen.push(state.fork(self.budget)?);
            for state in self.expression(condition, state, depth + 1)? {
                if truth.can_be_false() {
                    self.budget.reserve(1)?;
                    exits.push(state.fork(self.budget)?);
                }
                if truth.can_be_true() {
                    self.start_loop(node, body, None)?;
                    queue.extend(self.statement(body, state, depth + 1)?);
                    let jumped = self.loops.pop().unwrap();
                    queue.extend(jumped.continues);
                    exits.extend(jumped.breaks);
                }
            }
        }
        if !truth.can_be_true() {
            self.dead_depth += 1;
            let checked = self.statement(body, Residence::default(), depth + 1);
            self.dead_depth -= 1;
            checked?;
        }
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
        let Some(active) = self.loops.last() else {
            return Ok(Vec::new());
        };
        if active.node != target || active.dead_depth != self.dead_depth {
            return Ok(Vec::new());
        }
        let body = &self.module.syntax.nodes[active.body].span;
        let start = body.start();
        let end = body.end();
        let owned = active.owned_iteration;
        let removed = state.take_scope(|root| matches!(root, Root::Local(id)
            if owned.and_then(|owned| owned.binder) == Some(id)
                || self.scopes.get(id).and_then(|scope| *scope).is_some_and(|scope| (start..end).contains(&scope))), self.budget)?;
        for (root, value) in removed {
            self.budget.reserve(1)?;
            self.discard(vec![value], root, node, ResultDiscardKind::ScopeExit)?;
        }
        if is_break && let Some(owned) = owned {
            let values = state.take(owned.iterator, &[], self.budget)?;
            self.discard(values, owned.iterator, node, ResultDiscardKind::ScopeExit)?;
        }
        self.budget.reserve(1)?;
        let active = self.loops.last_mut().unwrap();
        if is_break {
            active.breaks.push(state);
        } else {
            active.continues.push(state);
        }
        Ok(Vec::new())
    }
}
