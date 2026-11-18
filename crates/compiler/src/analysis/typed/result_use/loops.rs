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
}

impl<'a, 'g> Check<'a, 'g> {
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
                    self.budget.reserve(1)?;
                    self.loops.push(LoopExits {
                        node,
                        body,
                        dead_depth: self.dead_depth,
                        breaks: Vec::new(),
                        continues: Vec::new(),
                    });
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
        let removed = state.take_scope(|root| matches!(root, Root::Local(id)
            if self.scopes.get(id).and_then(|scope| *scope).is_some_and(|scope| (start..end).contains(&scope))), self.budget)?;
        for (root, value) in removed {
            self.budget.reserve(1)?;
            self.discard(vec![value], root, node, ResultDiscardKind::ScopeExit)?;
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
