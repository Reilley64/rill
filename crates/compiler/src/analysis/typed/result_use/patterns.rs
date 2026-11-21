//! Project committed typed patterns onto their original storage.
use super::*;
use crate::analysis::scalars::data::{
    Constructor, Match, Pattern, PatternFacts, PatternProjection,
};
use storage::Address;

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
            let outcome = state.test(*root, path, tag, self.budget)?;
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
                    self.budget.reserve(
                        1 + match tag {
                            Constructor::Text(t) => t.len(),
                            _ => 0,
                        },
                    )?;
                    let tag = Arc::new(tag.clone());
                    let mut no = state.fork(self.budget)?;
                    no.store(*root, path, Content::Test(tag.clone(), false), self.budget)?;
                    self.budget.reserve(1)?;
                    failed.push(no);
                    state.store(*root, path, Content::Test(tag, true), self.budget)?;
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
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let root = Root::Temporary(matched.scrutinee);
        let scrutinee = self.expression_facts(matched.scrutinee)?;
        let original = scrutinee
            .filter(|fact| {
                fact.access == Access::Owned
                    && !matches!(fact.ty.as_ref(), TypeTerm::Reference { .. })
            })
            .and_then(|fact| fact.path.as_ref());
        let source = self.address(&state, original)?;
        let remaining = self.expression(matched.scrutinee, state, depth + 1)?;
        if let Some(source) = source {
            if scrutinee.is_some_and(
                |fact| matches!(fact.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"),
            ) {
                let mut snapshots = Vec::new();
                for mut state in remaining {
                    // Pattern tests use the evaluated copy, not a view into
                    // the caller's storage. Refine an unknown read at this
                    // point, before any guard or body can write to the source.
                    if state
                        .test(root, &[], &Constructor::Boolean(true), self.budget)?
                        .is_none()
                    {
                        let mut other = state.fork(self.budget)?;
                        for (branch, truth) in [(&mut other, false), (&mut state, true)] {
                            self.record_boolean(branch, root, &[], truth)?;
                            self.record_boolean(branch, source.0, &source.1, truth)?;
                        }
                        self.budget.reserve(1)?;
                        snapshots.push(other);
                    }
                    self.budget.reserve(1)?;
                    snapshots.push(state);
                }
                return self.match_arms(node, matched, snapshots, true, depth);
            }
            self.budget.reserve(1)?;
            self.match_views.push((source, root));
            let result = self.match_arms(node, matched, remaining, false, depth);
            self.match_views.pop();
            result
        } else {
            self.match_arms(node, matched, remaining, false, depth)
        }
    }

    fn match_arms(
        &mut self,
        node: NodeId,
        matched: &Match<'g>,
        mut remaining: Alternatives<'g>,
        copy_bool: bool,
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
                    if let Some(binder) = arm.pattern.binders.first()
                        && let Some(truth) =
                            state.test(root, &[], &Constructor::Boolean(true), self.budget)?
                    {
                        self.record_boolean(
                            &mut state,
                            Root::Local(binder.declaration),
                            &[],
                            truth,
                        )?;
                    }
                } else {
                    self.pattern_binders(&mut state, root, &[], &arm.pattern, false)?;
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
                    if arm.pattern.access == Access::Owned {
                        self.acknowledge_pattern(
                            &mut state,
                            root,
                            &mut Vec::new(),
                            &arm.pattern.pattern,
                            1,
                        )?;
                    }
                    let block = self.module.syntax.nodes[arm.body].kind == NodeKind::Block;
                    let mut next = if block {
                        self.statement(arm.body, state, depth + 1)?
                    } else {
                        self.expression(arm.body, state, depth + 1)?
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
                    result.extend(next);
                }
            }
            if !entered {
                self.dead_node(arm.body, Residence::default(), depth + 1)?;
            }
            remaining = failed;
        }
        Ok(result)
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
