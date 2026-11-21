//! Loop worklist and transfer targets. A jump leaves only its target's iteration scopes.
use super::*;
use crate::analysis::{
    scalars::{
        BooleanSet, Iteration, LiteralValue,
        data::{Pattern, PatternFacts},
        generics::GoalState,
    },
    typed::{CollectionOperation, ListTraversal},
};
use std::collections::{HashMap, VecDeque};
use std::ptr;

// Small summaries folded into the charged typed-node index pass. Local use
// facts carry the resolved declaration from the capture, even in a closure.
// Never hash/compare the structural ID or module path for each reference.
#[derive(Default)]
pub(super) struct OptionIterationEvidence<'a, 'g> {
    references: HashMap<*const DeclarationId, ParameterReferences<'g>>,
    binders: HashMap<(*const ModuleKey, NodeId), &'a PatternFacts<'g>>,
}

#[derive(Clone, Copy)]
enum ParameterReferences<'g> {
    Sole(Site<'g>),
    Pair(Site<'g>, Site<'g>),
    Multiple,
}

enum OptionCursor<'a, 'g> {
    Empty,
    Unknown(&'a PatternFacts<'g>, &'a Type<'g>),
}

fn covers(outer: Site<'_>, inner: Site<'_>) -> bool {
    ptr::eq(outer.module, inner.module)
        && outer.span.start() <= inner.span.start()
        && inner.span.end() <= outer.span.end()
}

fn text_option(ty: &Type<'_>, option: &DeclarationId) -> bool {
    matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
        if ptr::eq(*declaration, option)
            && matches!(arguments.as_slice(), [payload]
                if matches!(payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "string")))
}

impl<'a, 'g> OptionIterationEvidence<'a, 'g> {
    pub(super) fn record(&mut self, facts: &'a NodeFacts<'g>, core: Inputs<'_, 'g>, first: bool) {
        let (Some(option), Some(_)) = (core.option, core.collections.list) else {
            return;
        };
        let node = facts.site.node;
        match &facts.kind {
            NodeFactKind::Expression(value) => {
                if let Some(id) = value.binding
                    && id.kind == DeclarationKind::Parameter
                {
                    self.references
                        .entry(ptr::from_ref(id))
                        .and_modify(|references| {
                            let same = |site: Site<'_>| {
                                ptr::eq(site.module, facts.site.module) && site.node == node
                            };
                            *references = match *references {
                                ParameterReferences::Sole(first) if !same(first) => {
                                    ParameterReferences::Pair(first, facts.site)
                                }
                                ParameterReferences::Pair(a, b) if !same(a) && !same(b) => {
                                    ParameterReferences::Multiple
                                }
                                _ => *references,
                            };
                        })
                        .or_insert(ParameterReferences::Sole(facts.site));
                }
            }
            NodeFactKind::Pattern(pattern)
                if first
                    && pattern.access == Access::Owned
                    && matches!(pattern.pattern.as_ref(), Pattern::Any)
                    && matches!(pattern.binders.as_slice(), [binder] if binder.projections.is_empty())
                    && text_option(&pattern.ty, option.declaration) =>
            {
                self.binders
                    .entry((ptr::from_ref(facts.site.module), node))
                    .or_insert(pattern);
            }
            _ => {}
        }
    }
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
                OptionCursor::Unknown(pattern, next) => {
                    self.owned_option_iteration(node, pattern, next, state, depth)
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
    ) -> Work<Option<OptionCursor<'a, 'g>>> {
        let (Some(option), Some(list)) = (self.core.option, self.core.collections.list) else {
            return Ok(None);
        };
        let syntax = &self.module.syntax.nodes;
        let Some(path) = source.path.as_ref().filter(|path| {
            path.root.kind == DeclarationKind::Parameter && path.projections.is_empty()
        }) else {
            return Ok(None);
        };
        // Use only a checked formal whose cursor cannot be revisited on an
        // enclosing backedge. Its references must leave it unconstrained or
        // establish the exact empty edge below.
        if !self.current_body_checked
            || self.dead_depth != 0
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
            || !text_option(&iteration.yielded, option.declaration)
            || !matches!(source.ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
                if ptr::eq(*declaration, &list.declaration.id)
                    && matches!(arguments.as_slice(), [element] if text_option(element, option.declaration)))
        {
            return Ok(None);
        }
        let Some(&pattern) = self
            .option_iterations
            .binders
            .get(&(ptr::from_ref(&self.module.key), iteration.pattern))
        else {
            return Ok(None);
        };
        let Some((reference, other)) = self.option_source_references(path.root, iteration.source)
        else {
            return Ok(None);
        };
        let empty = if let Some(other) = other {
            if !self.first_guard_proves_empty(other, iteration.source)? {
                return Ok(None);
            }
            true
        } else {
            false
        };
        self.budget.step(self.current_parameters.len())?;
        if !self
            .current_parameters
            .iter()
            .any(|p| std::ptr::eq(&p.declaration.id, path.root))
            || self.address(state, Some(path))? != Some((Root::Local(path.root), Vec::new()))
            || state.owes_under(Root::Local(path.root), self.budget)?
        {
            return Ok(None);
        }
        if reference != self.value_root_at(iteration.source, depth + 1)?.0 {
            return Ok(None);
        }
        if empty {
            return Ok(Some(OptionCursor::Empty));
        }
        let mut binder = iteration.pattern;
        let mut binder_depth = depth + 1;
        loop {
            self.depth(binder_depth)?;
            match syntax[binder].kind {
                NodeKind::NamePattern => break,
                NodeKind::GroupPattern => {
                    binder = syntax[binder].children[0];
                    binder_depth += 1;
                }
                _ => return Ok(None),
            }
        }
        // Processing the binder and looking up the selected next output.
        self.budget.step(2)?;
        Ok(self.bodies.goal(&iteration.next).ok().and_then(|goal| {
            Some(OptionCursor::Unknown(
                pattern,
                &goal.output.as_ref()?.result,
            ))
        }))
    }

    fn option_source_references(
        &self,
        parameter: &DeclarationId,
        source: NodeId,
    ) -> Option<(NodeId, Option<NodeId>)> {
        let source = self.site(source);
        let within = |reference| covers(source, reference);
        let same_module = |reference: Site<'_>| ptr::eq(reference.module, &self.module.key);
        let (reference, other) = match *self
            .option_iterations
            .references
            .get(&ptr::from_ref(parameter))?
        {
            ParameterReferences::Sole(reference) if within(reference) => (reference, None),
            ParameterReferences::Pair(a, b) if within(a) && same_module(b) => (a, Some(b.node)),
            ParameterReferences::Pair(a, b) if within(b) && same_module(a) => (b, Some(a.node)),
            _ => return None,
        };
        Some((
            reference.node,
            other.filter(|node| !self.dead_option_references.contains(node)),
        ))
    }

    pub(super) fn note_dead_option_reference(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
    ) -> Work<()> {
        // Only literal-dead branches are invariant across alternative visits.
        if let Some((condition, skipped)) = self.literal_skipped_branch
            && covers(self.site(skipped), self.site(node))
            && let Some(id) = value.binding
            && let Some(ParameterReferences::Pair(..)) = self.option_iterations.references.get(&ptr::from_ref(id))
            && let (Some(option), Some(list)) = (self.core.option, self.core.collections.list)
            && matches!(value.ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
                if ptr::eq(*declaration, &list.declaration.id)
                    && matches!(arguments.as_slice(), [element] if text_option(element, option.declaration)))
            && !self.dead_option_references.contains(&node)
            && self.facts(condition)?.is_some_and(|facts| {
                facts.flow.normal && !facts.flow.pending
                    && matches!(&facts.kind, NodeFactKind::Expression(value)
                        if value.boolean == Some(BooleanSet::FALSE)
                            && value.literal.is_some_and(|literal| literal.value == LiteralValue::Boolean(false)))
            })
        {
            self.budget.reserve(1)?;
            self.dead_option_references.insert(node);
        }
        Ok(())
    }

    fn first_guard_proves_empty(&mut self, reference: NodeId, source: NodeId) -> Work<bool> {
        let Some(guard) = self.current_option_guard else {
            return Ok(false);
        };
        let syntax = &self.module.syntax.nodes;
        let [binding, failure] = syntax[guard].children[..] else {
            return Ok(false);
        };
        if syntax[binding].kind != NodeKind::GuardBinding
            || !covers(self.site(failure), self.site(source))
        {
            return Ok(false);
        }
        let Some(&operand) = syntax[binding].children.last() else {
            return Ok(false);
        };
        let [_, argument] = syntax[operand].children[..] else {
            return Ok(false);
        };
        if syntax[operand].kind != NodeKind::Call
            || syntax[argument].kind != NodeKind::Prefix("&")
            || syntax[argument].children.as_slice() != [reference]
        {
            return Ok(false);
        }
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
        pattern: &PatternFacts<'g>,
        next: &Type<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let [_, source, body] = self.module.syntax.nodes[node].children[..] else {
            unreachable!()
        };
        let mut initial = self.expression(source, state, depth + 1)?;
        if initial.is_empty() {
            self.dead_statement(body, Residence::default(), depth + 1)?;
        }
        for state in &mut initial {
            // Only Result-free backing reaches this worklist. Its length and
            // element tags are not known, so forget any positional hints.
            state.take(Root::Temporary(source), &[], self.budget)?;
        }
        let option = self.core.option.unwrap();
        let next_value = Root::Temporary(node);
        let payload = [ResultComponent::VariantPayload {
            variant: option.success,
            index: 0,
        }];
        let item = Root::Local(pattern.binders[0].declaration);
        let mut queue = VecDeque::from(initial);
        let mut seen = Vec::new();
        let mut exits = Vec::new();
        while let Some(mut state) = queue.pop_front() {
            if !self.first_visit(&mut state, &mut seen)? {
                continue;
            }
            // next produces an outer Option. Only outer Some installs a fresh
            // item, whose inner Some and None remain separate in the body.
            let input = self.singleton(state)?;
            for mut state in self.seed(input, next_value, node, next)? {
                let yielded = state.test(
                    next_value,
                    &[],
                    &Constructor::Variant(option.success),
                    self.budget,
                )?;
                if yielded == Some(true) {
                    self.commit_pattern(&mut state, next_value, &payload, pattern)?;
                    state.take(next_value, &[], self.budget)?;
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
                } else if yielded == Some(false) {
                    state.take(next_value, &[], self.budget)?;
                    exits.extend(self.singleton(state)?);
                } else {
                    self.pending(node, ResultUseBlocker::Containment)?;
                }
            }
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
