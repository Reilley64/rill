//! Loop worklist and transfer targets. A jump leaves only its target's iteration scopes.
use super::*;
use crate::analysis::{
    scalars::{Iteration, IterationBodyKind, data::Pattern, generics::GoalState},
    typed::{CollectionOperation, ListTraversal},
};
use std::collections::VecDeque;

enum OptionCursor<'g> {
    Empty,
    Unknown(&'g DeclarationId),
}

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
        let facts = self.facts(node)?;
        let iteration = facts.and_then(|facts| match &facts.kind {
            NodeFactKind::Iteration(iteration) => Some(iteration),
            _ => None,
        });
        let source_fact = if let Some(iteration) = iteration
            && iteration.traversal == Some(ListTraversal::Owned)
        {
            self.expression_facts(iteration.source)?
        } else {
            None
        };
        if source_fact
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
        if let Some(iteration) = iteration
            && facts.is_some_and(|facts| !facts.flow.pending)
            && let Some(source) = source_fact
            && let Some(cursor) =
                self.owned_option_iteration_facts(iteration, source, &state, depth)?
        {
            return match cursor {
                OptionCursor::Empty => {
                    self.empty_option_iteration(iteration.source, body, state, depth)
                }
                OptionCursor::Unknown(item) => {
                    if let Some((direct, candidate)) = &mut self.option_iterator
                        && *direct == depth
                        && match iteration.body {
                            Some(IterationBodyKind::YieldedTextPostfix) => true,
                            // Bare loop transfers need no other M5 operation on
                            // the owning carrier. Ordinary shape alone does not
                            // prove that (for example) a borrow can be lowered.
                            Some(IterationBodyKind::Ordinary) => {
                                match self.module.syntax.nodes[body].children.as_slice() {
                                    [] => true,
                                    [exit] => matches!(
                                        self.module.syntax.nodes[*exit].kind,
                                        NodeKind::Break | NodeKind::Continue
                                    ),
                                    _ => false,
                                }
                            }
                            _ => false,
                        }
                        && let Some(path) = source.path.as_ref()
                        && path.root.kind == DeclarationKind::Parameter
                    {
                        *candidate = Some((path.root, node));
                    }
                    self.owned_option_iteration(node, item, state, depth)
                }
            };
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

    fn owned_option_iteration_facts(
        &mut self,
        iteration: &Iteration<'g>,
        source: &ExpressionFacts<'g>,
        state: &Residence<'g>,
        depth: usize,
    ) -> Work<Option<OptionCursor<'g>>> {
        let Some(option) = self.core.option else {
            return Ok(None);
        };
        let syntax = &self.module.syntax.nodes;
        let Some(path) = source
            .path
            .as_ref()
            .filter(|path| path.projections.is_empty())
        else {
            return Ok(None);
        };
        // Keep the bounded handoff out of enclosing iteration worklists.
        // There is no cursor evidence for lexical (rather than live) visits.
        if self.dead_depth != 0
            || !self.loops.is_empty()
            || source.access != Access::Owned
            || !matches!(
                syntax[iteration.source].kind,
                NodeKind::ValueReference | NodeKind::Group
            )
            || !matches!(
                syntax[iteration.pattern].kind,
                NodeKind::NamePattern | NodeKind::GroupPattern
            )
            || !shape::text_option(&iteration.yielded, option.declaration)
            || !self.text_option_list(&source.ty)
        {
            return Ok(None);
        }
        let Some(availability) = state.option_list_source(path.root, self.budget)? else {
            return Ok(None);
        };
        let Some(NodeFactKind::Pattern(pattern)) = self.facts(iteration.pattern)?.map(|f| &f.kind)
        else {
            return Ok(None);
        };
        let [binder] = pattern.binders.as_slice() else {
            return Ok(None);
        };
        if pattern.access != Access::Owned
            || !matches!(pattern.pattern.as_ref(), Pattern::Any)
            || !binder.projections.is_empty()
            || self.address(state, Some(path))? != Some((Root::Local(path.root), Vec::new()))
        {
            return Ok(None);
        }

        let Some(cursor) = (match availability {
            OptionListSource::Fresh => Some(OptionCursor::Unknown(binder.declaration)),
            OptionListSource::GuardInspection { guard, borrow } => self
                .first_guard_proves_empty(guard, borrow, iteration.source)?
                .then_some(OptionCursor::Empty),
        }) else {
            return Ok(None);
        };

        let (source, _) = self.value_root_at(iteration.source, depth + 1)?;
        if syntax[source].kind != NodeKind::ValueReference {
            return Ok(None);
        }
        let mut binder = iteration.pattern;
        let mut binder_depth = depth + 1;
        loop {
            self.depth(binder_depth)?;
            match syntax[binder].kind {
                NodeKind::NamePattern => return Ok(Some(cursor)),
                NodeKind::GroupPattern => {
                    binder = syntax[binder].children[0];
                    binder_depth += 1;
                }
                _ => return Ok(None),
            }
        }
    }

    pub(super) fn option_list_inspection(&self, borrow: NodeId) -> Option<OptionListSource> {
        let guard = self.current_option_guard?;
        let syntax = &self.module.syntax.nodes;
        let [binding, _] = syntax[guard].children[..] else {
            return None;
        };
        if syntax[binding].kind != NodeKind::GuardBinding {
            return None;
        }
        let operand = *syntax[binding].children.last()?;
        (syntax[operand].kind == NodeKind::Call
            && matches!(syntax[operand].children.as_slice(), [_, argument] if *argument == borrow))
        .then_some(OptionListSource::GuardInspection { guard, borrow })
    }

    fn first_guard_proves_empty(
        &mut self,
        guard: NodeId,
        argument: NodeId,
        source: NodeId,
    ) -> Work<bool> {
        let syntax = &self.module.syntax.nodes;
        let [binding, failure] = syntax[guard].children[..] else {
            unreachable!("inspection is tied to a binding guard");
        };
        let failure = &syntax[failure].span;
        let source = &syntax[source].span;
        if failure.start() > source.start() || source.end() > failure.end() {
            return Ok(false);
        }
        let operand = *syntax[binding].children.last().unwrap();
        let Some(first) = self
            .core
            .collections
            .protection
            .iter()
            .flatten()
            .find(|p| p.operation == CollectionOperation::First)
            .copied()
        else {
            return Ok(false);
        };
        let Some(winner) = first.winner() else {
            return Ok(false);
        };
        let Some(call) = self
            .expression_facts(operand)?
            .and_then(|value| value.call.as_ref())
        else {
            return Ok(false);
        };
        if call.callee != &first.family.id || call.arguments.as_slice() != [argument] {
            return Ok(false);
        }
        self.budget.step(self.inventory.graph().modules().len())?;
        let Some(ordinary) = self
            .bodies
            .ordinary_call(self.module, operand)
            .ok()
            .flatten()
        else {
            return Ok(false);
        };
        self.budget.step(1)?;
        let Some(goal) = ordinary
            .goal
            .as_ref()
            .and_then(|id| self.bodies.goal(id).ok())
        else {
            return Ok(false);
        };
        // The ordinary comparison can retain a role premise. The proved
        // goal and the protected, Clear First winner discharge it here.
        if !ordinary.checked
            || ordinary
                .premise
                .is_some_and(crate::analysis::ordinary::InputPremise::has_conversion)
            || ordinary.instantiation.is_some()
            || ordinary.family != call.callee
            || goal.state != GoalState::Proved
            || goal.premise.is_some()
            || !goal
                .selection
                .as_ref()
                .is_some_and(|selected| selected.implementation == winner)
        {
            return Ok(false);
        }
        // An irrefutable payload cannot take the failure arm of a Some.
        Ok(self
            .facts(syntax[binding].children[0])?
            .is_some_and(|facts| {
                matches!(&facts.kind, NodeFactKind::Pattern(pattern)
                if matches!(pattern.pattern.as_ref(), Pattern::Any | Pattern::Wildcard)
                    && pattern.coverage.exhaustive())
            }))
    }

    fn empty_option_iteration(
        &mut self,
        source: NodeId,
        body: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut paths = self.expression(source, state, depth + 1)?;
        self.dead_statement(body, Residence::default(), depth + 1)?;
        for state in &mut paths {
            state.take(Root::Temporary(source), &[], self.budget)?;
        }
        Ok(paths)
    }

    fn owned_option_iteration(
        &mut self,
        node: NodeId,
        binder: &'g DeclarationId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let [_, source, body] = self.module.syntax.nodes[node].children[..] else {
            unreachable!()
        };
        let initial = self.expression(source, state, depth + 1)?;
        if initial.is_empty() {
            self.dead_statement(body, Residence::default(), depth + 1)?;
            return Ok(initial);
        }
        let option = self.core.option.unwrap();
        let item = Root::Local(binder);
        // The selected next result is Option<Option<string>>. The outer None
        // ends iteration; on outer Some the item has either inner tag. Split
        // those tags only if the body tests them: neither carries a Result.
        let mut queue = VecDeque::from(initial);
        let mut seen = Vec::new();
        let mut exits = Vec::new();
        while let Some(mut state) = queue.pop_front() {
            if !self.first_visit(&mut state, &mut seen)? {
                continue;
            }
            self.budget.reserve(1)?;
            exits.push(state.fork(self.budget)?);
            state.store(
                item,
                &[],
                Content::OptionItem {
                    some: option.success,
                    none: option.failure,
                },
                self.budget,
            )?;
            let jumped = self.with_loop(node, |check| {
                for mut state in check.statement(body, state, depth + 1)? {
                    let remaining = state.take(item, &[], check.budget)?;
                    check.discard(remaining, item, body, ResultDiscardKind::ScopeExit)?;
                    queue.extend(check.singleton(state)?);
                }
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
