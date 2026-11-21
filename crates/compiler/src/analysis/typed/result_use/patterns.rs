//! Project committed typed patterns onto their original storage.
use super::*;
use crate::analysis::scalars::data::{
    Arm, Constructor, Match, Pattern, PatternFacts, PatternProjection,
};
use storage::{Address, BooleanValue};

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn commit_pattern(
        &mut self,
        state: &mut Residence<'g>,
        root: Root<'g>,
        prefix: &[ResultComponent<'g>],
        facts: &PatternFacts<'g>,
    ) -> Work<()> {
        if facts.access == Access::Owned {
            self.budget.reserve(prefix.len())?;
            self.acknowledge_pattern(state, root, &mut prefix.to_vec(), &facts.pattern, 1)?;
        }
        self.pattern_binders(state, root, prefix, facts, false)
    }

    fn pattern_binders(
        &mut self,
        state: &mut Residence<'g>,
        root: Root<'g>,
        prefix: &[ResultComponent<'g>],
        facts: &PatternFacts<'g>,
        restore: bool,
    ) -> Work<()> {
        for binder in &facts.binders {
            self.budget
                .reserve(prefix.len() + binder.projections.len() + 1)?;
            let mut source = (root, prefix.to_vec());
            let mut borrowed = facts.access != Access::Owned;
            let mut resolved = true;
            for projection in &binder.projections {
                source.1.push(match projection {
                    PatternProjection::Field(field) => ResultComponent::Field(field),
                    PatternProjection::TupleElement(index) => ResultComponent::TupleElement(*index),
                    PatternProjection::Payload { variant, index } => {
                        ResultComponent::VariantPayload {
                            variant,
                            index: *index,
                        }
                    }
                    PatternProjection::Dereference => {
                        borrowed = true;
                        let Some(address) = state.alias(source.0, &source.1, self.budget)? else {
                            resolved = false;
                            break;
                        };
                        source = address;
                        continue;
                    }
                });
            }
            let local = Root::Local(binder.declaration);
            if !resolved || borrowed {
                if restore {
                    state.take(local, &[], self.budget)?;
                } else if resolved && matches!(binder.ty.as_ref(), TypeTerm::Reference { .. }) {
                    state.store(local, &[], Content::Alias(source), self.budget)?;
                }
            } else if restore {
                self.move_value(state, local, &[], source.0, &source.1)?;
            } else {
                self.move_value(state, source.0, &source.1, local, &[])?;
            }
        }
        Ok(())
    }

    fn acknowledge_pattern(
        &mut self,
        state: &mut Residence<'g>,
        root: Root<'g>,
        path: &mut Vec<ResultComponent<'g>>,
        pattern: &Pattern<'g>,
        depth: usize,
    ) -> Work<()> {
        self.depth(depth)?;
        match pattern {
            Pattern::Wildcard => {
                state.take(root, path, self.budget)?;
            }
            Pattern::Constructor(tag, children, _) => {
                for (index, child) in children.iter().enumerate() {
                    let component = self.pattern_component(tag, index)?;
                    self.budget.reserve(1)?;
                    path.push(component);
                    self.acknowledge_pattern(state, root, path, child, depth + 1)?;
                    path.pop();
                }
            }
            Pattern::Any => {}
        }
        Ok(())
    }

    pub(super) fn split_pattern(
        &mut self,
        mut state: Residence<'g>,
        address: &Address<'g>,
        pattern: &Pattern<'g>,
        failed: &mut Alternatives<'g>,
        depth: usize,
    ) -> Work<Option<Residence<'g>>> {
        self.depth(depth)?;
        let Pattern::Constructor(tag, children, site) = pattern else {
            return Ok(Some(state));
        };
        let alias = state.alias(address.0, &address.1, self.budget)?;
        let (root, path) = alias.as_ref().unwrap_or(address);
        if !matches!(tag, Constructor::Record(_) | Constructor::Tuple(_)) {
            let mut copy = None;
            let outcome = if let Constructor::Boolean(expected) = tag {
                match state.boolean_value(*root, path, self.budget)? {
                    Some(BooleanValue::Known(actual)) => Some(actual == *expected),
                    Some(BooleanValue::Copy(value)) => {
                        copy = Some((value, *expected));
                        None
                    }
                    None => None,
                }
            } else {
                state.test(*root, path, tag, self.budget)?
            };
            if matches!(tag, Constructor::Variant(_)) {
                if alias.is_none()
                    && outcome.is_none()
                    && let Some(site) = site.or(match address.0 {
                        Root::Temporary(node) => Some(node),
                        Root::Local(_) => None,
                    })
                {
                    // A reference with only a possible referent cannot prove
                    // which occurrence a discriminant test observes.
                    for candidate in state.aliases_under(*root, path, self.budget)? {
                        self.forget_address(&mut state, &candidate, site)?;
                    }
                }
                state.observe(*root, path, self.budget)?;
            }
            match outcome {
                Some(true) => {}
                Some(false) => {
                    self.budget.reserve(1)?;
                    failed.push(state);
                    return Ok(None);
                }
                None => {
                    let mut no = state.fork(self.budget)?;
                    if let Some((copy, expected)) = copy {
                        no.refine_boolean_copy(&copy, !expected, self.budget)?;
                        state.refine_boolean_copy(&copy, expected, self.budget)?;
                    } else {
                        self.budget.reserve(
                            1 + match tag {
                                Constructor::Text(t) => t.len(),
                                _ => 0,
                            },
                        )?;
                        let tag = Arc::new(tag.clone());
                        no.store(*root, path, Content::Test(tag.clone(), false), self.budget)?;
                        state.store(*root, path, Content::Test(tag, true), self.budget)?;
                    }
                    self.budget.reserve(1)?;
                    failed.push(no);
                }
            }
        }
        for (index, child) in children.iter().enumerate() {
            self.budget.reserve(1 + path.len())?;
            let mut projection = path.clone();
            projection.push(self.pattern_component(tag, index)?);
            let Some(next) =
                self.split_pattern(state, &(*root, projection), child, failed, depth + 1)?
            else {
                return Ok(None);
            };
            state = next;
        }
        Ok(Some(state))
    }

    pub(super) fn match_expression(
        &mut self,
        node: NodeId,
        matched: &Match<'g>,
        state: Residence<'g>,
        depth: usize,
        unused_bool: bool,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let root = Root::Temporary(matched.scrutinee);
        let scrutinee = self.expression_facts(matched.scrutinee)?;
        let boolean = scrutinee.is_some_and(
            |fact| matches!(fact.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"),
        );
        let original = scrutinee
            .filter(|fact| {
                boolean
                    || (fact.access == Access::Owned
                        && !matches!(fact.ty.as_ref(), TypeTerm::Reference { .. }))
            })
            .and_then(|fact| fact.path.as_ref());
        let source = self.address(&state, original)?;
        if boolean {
            return self.boolean_match(node, matched, state, source, unused_bool, depth);
        }
        let remaining = self.expression(matched.scrutinee, state, depth + 1)?;
        if let Some(source) = source {
            self.budget.reserve(1)?;
            self.match_views.push((source, root));
            let result = self.match_arms(node, matched, remaining, false, unused_bool, depth);
            self.match_views.pop();
            result
        } else {
            self.match_arms(node, matched, remaining, false, unused_bool, depth)
        }
    }

    fn boolean_match(
        &mut self,
        node: NodeId,
        matched: &Match<'g>,
        state: Residence<'g>,
        source: Option<Address<'g>>,
        unused: bool,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let wildcard = matches!(matched.arms.as_slice(), [arm] if arm.guard.is_none()
                && matches!(&*arm.pattern.pattern, Pattern::Wildcard) && arm.pattern.binders.is_empty());
        let usage = if wildcard {
            ValueUse::Statement
        } else {
            ValueUse::Used
        };
        let mut remaining = self.expression_with_use(matched.scrutinee, state, depth + 1, usage)?;
        if wildcard {
            // The arm does not consume the truth of the scrutinee. Start
            // tracking equality only if a later read copies the source.
            if let Some(source) = source {
                self.budget.reserve(1)?;
                self.wildcard_sources.push(source);
                let result = self.match_arms(node, matched, remaining, true, unused, depth);
                self.wildcard_sources.pop();
                return result;
            }
            return self.match_arms(node, matched, remaining, true, unused, depth);
        }
        let root = Root::Temporary(matched.scrutinee);
        let Some(source) = source else {
            // For a computed scrutinee, arm reachability is determined by
            // all the outcomes of evaluating it, including no continuation.
            for state in &mut remaining {
                if state.boolean_value(root, &[], self.budget)?.is_none() {
                    state.new_boolean_copy(root, &[], self.budget)?;
                }
            }
            return self.match_arms(node, matched, remaining, true, unused, depth);
        };
        if self.boolean_match_preserves_state(matched, depth + 1, unused)? {
            for state in &mut remaining {
                self.budget.step(1)?;
                state.take(root, &[], self.budget)?;
            }
            return Ok(remaining);
        }
        let mut result = Vec::new();
        for mut state in remaining {
            // A snapshot and its copies carry the same value identity, not
            // aliases to the source. Branch only when truth is tested.
            let fresh = if state.boolean_value(root, &[], self.budget)?.is_none() {
                let copy = state.new_boolean_copy(source.0, &source.1, self.budget)?;
                state.store(root, &[], Content::BooleanCopy(copy.clone()), self.budget)?;
                Some(copy)
            } else {
                None
            };
            let snapshots = self.singleton(state)?;
            let mut completed = self.match_arms(node, matched, snapshots, true, unused, depth)?;
            for state in &mut completed {
                if unused {
                    let root = Root::Temporary(node);
                    let output = state.take(root, &[], self.budget)?;
                    self.discard(output, root, node, ResultDiscardKind::Expression)?;
                }
                if let Some(copy) = &fresh {
                    state.remove_single_boolean_copy(copy, self.budget)?;
                }
            }
            if let [first, second] = completed.as_mut_slice()
                && first.join_boolean(second, &source, self.budget)?
            {
                completed.pop();
            }
            if result.is_empty() {
                result = completed;
            } else {
                self.budget.reserve(completed.len())?;
                result.extend(completed);
            }
        }
        Ok(result)
    }

    /// An exhaustive match whose arms all fall through without observing or
    /// changing any residence does not distinguish the two Boolean outcomes.
    /// Still evaluate the scrutinee: its own transfers and effects are real.
    fn boolean_match_preserves_state(
        &mut self,
        matched: &Match<'g>,
        depth: usize,
        unused_bool: bool,
    ) -> Work<bool> {
        if !matched.coverage.exhaustive() {
            return Ok(false);
        }
        for arm in &matched.arms {
            self.budget.step(1)?;
            if arm.guard.is_some() {
                return Ok(false);
            }
            if unused_bool
                && self.module.syntax.nodes[arm.body].kind != NodeKind::Block
                && self.pure_scalar_value(arm.body, depth)?
            {
                continue;
            }
            if !self.discards_only_scalar_reads(arm.body, depth)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn wildcard_at(&mut self, mut node: NodeId, mut depth: usize) -> Work<bool> {
        while self.module.syntax.nodes[node].kind == NodeKind::GroupPattern {
            self.depth(depth)?;
            node = self.module.syntax.nodes[node].children[0];
            depth += 1;
        }
        self.depth(depth)?;
        Ok(self.module.syntax.nodes[node].kind == NodeKind::Wildcard)
    }

    fn discards_only_scalar_reads(&mut self, node: NodeId, depth: usize) -> Work<bool> {
        self.depth(depth)?;
        let syntax = &self.module.syntax.nodes[node];
        match syntax.kind {
            NodeKind::Block => {
                for &child in &syntax.children {
                    if !self.discards_only_scalar_reads(child, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            NodeKind::Binding { .. }
                if self.module.syntax.nodes[syntax.children[0]].kind == NodeKind::Wildcard
                    || (self.module.syntax.nodes[syntax.children[0]].kind
                        == NodeKind::GroupPattern
                        && self.wildcard_at(syntax.children[0], depth + 1)?) =>
            {
                let source = *syntax.children.last().unwrap();
                self.pure_scalar_value(source, depth + 1)
            }
            _ => Ok(false),
        }
    }

    // A discarded literal or whole-local primitive read has no effect on
    // Result residence. A projection of an effectful temporary is not a read
    // of this form. Groups keep their written depth even on the fast path.
    fn pure_scalar_value(&mut self, node: NodeId, depth: usize) -> Work<bool> {
        let (leaf, _) = self.value_root_at(node, depth)?;
        let kind = self.module.syntax.nodes[leaf].kind;
        Ok(matches!(kind, NodeKind::Literal | NodeKind::ValueReference)
            && self.facts(leaf)?.is_some_and(|fact| {
                fact.flow.normal && !fact.flow.pending && matches!(&fact.kind, NodeFactKind::Expression(v)
                    if v.ordinary_context.is_none() && v.access == Access::Owned
                    && matches!(v.ty.as_ref(), TypeTerm::Primitive(p) if p.name() != "never")
                    && (kind == NodeKind::Literal || v.path.as_ref().is_some_and(|p| p.projections.is_empty())))
            }))
    }

    fn match_arms(
        &mut self,
        node: NodeId,
        matched: &Match<'g>,
        mut remaining: Alternatives<'g>,
        copy_bool: bool,
        unused_bool: bool,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let root = Root::Temporary(matched.scrutinee);
        let mut result = Vec::new();
        for arm in &matched.arms {
            self.budget.step(1)?;
            let mut failed = Vec::new();
            let mut entered = false;
            for state in std::mem::take(&mut remaining) {
                if !arm.guard_reachable {
                    failed.extend(self.singleton(state)?);
                    continue;
                }
                let Some(mut state) = self.split_pattern(
                    state,
                    &(root, Vec::new()),
                    &arm.pattern.pattern,
                    &mut failed,
                    depth + 1,
                )?
                else {
                    continue;
                };
                if copy_bool {
                    if let Some(binder) = arm.pattern.binders.first() {
                        let local = Root::Local(binder.declaration);
                        if arm.guard.is_none() {
                            state.move_boolean(root, local, self.budget)?;
                        } else if let Some(value) = state.boolean_value(root, &[], self.budget)? {
                            self.install_boolean(&mut state, local, value)?;
                        }
                    }
                } else {
                    self.pattern_binders(&mut state, root, &[], &arm.pattern, false)?;
                }
                if copy_bool && arm.guard.is_none() {
                    if arm.body_reachable {
                        entered = true;
                        result.extend(self.match_body(
                            node,
                            root,
                            arm,
                            state,
                            unused_bool,
                            depth,
                        )?);
                    }
                    continue;
                }
                let states = if let Some(guard) = arm.guard {
                    self.branch_condition(guard, state, depth + 1)?
                } else {
                    self.budget.reserve(1)?;
                    vec![(true, state)]
                };
                for (truth, mut state) in states {
                    if !truth {
                        if copy_bool {
                            // Drop the binder copy, not the scrutinee snapshot.
                            if let Some(binder) = arm.pattern.binders.first() {
                                state.take(Root::Local(binder.declaration), &[], self.budget)?;
                            }
                        } else {
                            self.pattern_binders(&mut state, root, &[], &arm.pattern, true)?;
                        }
                        failed.extend(self.singleton(state)?);
                        continue;
                    }
                    if !arm.body_reachable {
                        continue;
                    }
                    entered = true;
                    result.extend(self.match_body(node, root, arm, state, unused_bool, depth)?);
                }
            }
            if !entered {
                self.dead_node(arm.body, Residence::default(), depth + 1)?;
            }
            remaining = failed;
        }
        Ok(result)
    }

    fn match_body(
        &mut self,
        node: NodeId,
        root: Root<'g>,
        arm: &Arm<'g>,
        mut state: Residence<'g>,
        unused_bool: bool,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        if arm.pattern.access == Access::Owned {
            self.acknowledge_pattern(&mut state, root, &mut Vec::new(), &arm.pattern.pattern, 1)?;
        }
        let block = self.module.syntax.nodes[arm.body].kind == NodeKind::Block;
        let mut next = if block {
            self.statement(arm.body, state, depth + 1)?
        } else {
            let usage = if unused_bool {
                ValueUse::Statement
            } else {
                ValueUse::Used
            };
            self.expression_with_use(arm.body, state, depth + 1, usage)?
        };
        let scope = arm.site.span.start();
        for state in &mut next {
            if !block {
                self.move_value(
                    state,
                    Root::Temporary(arm.body),
                    &[],
                    Root::Temporary(node),
                    &[],
                )?;
            }
            let unbound = state.take(root, &[], self.budget)?;
            self.discard(unbound, root, arm.site.node, ResultDiscardKind::ScopeExit)?;
            self.exit_scope(state, scope, arm.site.node)?;
        }
        Ok(next)
    }

    fn pattern_component(
        &mut self,
        tag: &Constructor<'g>,
        index: usize,
    ) -> Work<ResultComponent<'g>> {
        self.budget.step(1)?;
        Ok(match tag {
            Constructor::Tuple(_) => ResultComponent::TupleElement(index),
            Constructor::Record(id) => {
                let DeclarationShape::Record { fields } =
                    &self.inventory.type_facts().declarations[*id].shape
                else {
                    unreachable!()
                };
                ResultComponent::Field(&fields[index].declaration.id)
            }
            Constructor::Variant(variant) => match self.variants[variant] {
                Payload::Record(fields) => ResultComponent::Field(&fields[index].declaration.id),
                _ => ResultComponent::VariantPayload { variant, index },
            },
            _ => unreachable!("literal patterns have no children"),
        })
    }
}
