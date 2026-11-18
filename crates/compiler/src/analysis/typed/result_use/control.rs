//! Apply canonical guard edges without treating a payload as its discriminant.
use super::*;
use crate::analysis::scalars::{
    BooleanSet,
    data::{Pattern, PatternFacts},
};

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn conditional(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        expression: bool,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let children = &self.module.syntax.nodes[node].children;
        let [condition, yes, rest @ ..] = children.as_slice() else {
            unreachable!()
        };
        if self.expression_facts(*condition)?.is_none() {
            self.pending(node, ResultUseBlocker::Control)?;
            for &child in &children[1..] {
                let entry = Residence::default();
                if expression {
                    self.dead_node(child, entry, depth + 1)?;
                } else {
                    self.dead_statement(child, entry, depth + 1)?;
                }
            }
            return self.unsettled(node, depth + 1);
        }
        let entry = state.fork(self.budget)?;
        let mut result = Vec::new();
        let mut taken = [false; 2];
        for (truth, state) in self.branch_condition(*condition, state, depth + 1)? {
            taken[usize::from(truth)] = true;
            let child = if truth {
                Some(*yes)
            } else {
                rest.first().copied()
            };
            let Some(child) = child else {
                result.extend(self.singleton(state)?);
                continue;
            };
            if expression {
                let mut next = self.expression(child, state, depth + 1)?;
                for state in &mut next {
                    self.move_value(
                        state,
                        Root::Temporary(child),
                        &[],
                        Root::Temporary(node),
                        &[],
                    )?;
                }
                result.extend(next);
            } else {
                result.extend(self.statement(child, state, depth + 1)?);
            }
        }
        for (truth, child) in [(true, Some(*yes)), (false, rest.first().copied())] {
            if !taken[usize::from(truth)]
                && let Some(child) = child
            {
                let entry = entry.fork(self.budget)?;
                if expression {
                    self.dead_node(child, entry, depth + 1)?;
                } else {
                    self.dead_statement(child, entry, depth + 1)?;
                }
            }
        }
        Ok(result)
    }

    pub(super) fn record_boolean(
        &mut self,
        state: &mut Residence<'g>,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        value: bool,
    ) -> Work<()> {
        self.budget.reserve(1)?;
        state.store(
            root,
            path,
            Content::Test(Arc::new(Constructor::Boolean(value)), true),
            self.budget,
        )
    }

    pub(super) fn branch_condition(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Vec<(bool, Residence<'g>)>> {
        let value = self.expression_facts(node)?;
        let truth = value
            .and_then(|value| value.boolean)
            .unwrap_or(BooleanSet::BOTH);
        let root = Root::Temporary(node);
        let mut result = Vec::new();
        for mut state in self.expression(node, state, depth)? {
            let known = state.test(root, &[], &Constructor::Boolean(true), self.budget)?;
            state.take(root, &[], self.budget)?;
            let address = self.address(&state, value.and_then(|value| value.path.as_ref()))?;
            for (outcome, feasible) in [(false, truth.can_be_false()), (true, truth.can_be_true())]
            {
                if !feasible || known.is_some_and(|known| known != outcome) {
                    continue;
                }
                let mut branch = state.fork(self.budget)?;
                if let Some((root, path)) = &address
                    && branch
                        .test(*root, path, &Constructor::Boolean(true), self.budget)?
                        .is_none()
                {
                    self.record_boolean(&mut branch, *root, path, outcome)?;
                }
                self.budget.reserve(1)?;
                result.push((outcome, branch));
            }
        }
        Ok(result)
    }

    pub(super) fn boolean_expression(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let syntax = &self.module.syntax.nodes[node];
        let root = Root::Temporary(node);
        let left = syntax.children[0];
        let right = syntax.children.get(1).copied();
        let entry = state.fork(self.budget)?;
        let mut evaluated_right = false;
        let mut result = Vec::new();
        for (truth, mut state) in self.branch_condition(left, state, depth)? {
            if let Some(right) = right
                && truth == (syntax.kind == NodeKind::Binary("&&"))
            {
                evaluated_right = true;
                for (truth, mut state) in self.branch_condition(right, state, depth)? {
                    self.record_boolean(&mut state, root, &[], truth)?;
                    result.extend(self.singleton(state)?);
                }
            } else {
                self.record_boolean(
                    &mut state,
                    root,
                    &[],
                    if right.is_none() { !truth } else { truth },
                )?;
                result.extend(self.singleton(state)?);
            }
        }
        if !evaluated_right && let Some(right) = right {
            self.dead_node(right, entry, depth)?;
        }
        Ok(result)
    }

    pub(super) fn primitive_operands(
        &mut self,
        operands: &[NodeId],
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut alternatives = self.singleton(state)?;
        for &child in operands {
            let root = Root::Temporary(child);
            let mut next = Vec::new();
            for state in alternatives {
                for mut state in self.expression(child, state, depth)? {
                    state.take(root, &[], self.budget)?;
                    next.extend(self.singleton(state)?);
                }
            }
            alternatives = next;
        }
        Ok(alternatives)
    }

    pub(super) fn guard(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let children = &self.module.syntax.nodes[node].children;
        let failure = *children.last().unwrap();
        let first = &self.module.syntax.nodes[children[0]];
        let is_binding = first.kind == NodeKind::GuardBinding;
        let operand = if is_binding {
            *first.children.last().unwrap()
        } else {
            children[0]
        };
        let value = self.facts(operand)?.and_then(|fact| match &fact.kind {
            NodeFactKind::Expression(value) => Some(value),
            _ => None,
        });
        if self.facts(node)?.is_none() || value.is_none() {
            self.pending(node, ResultUseBlocker::Control)?;
            return self.unsettled(node, depth + 1);
        }
        let value = value.unwrap();
        let pattern = if is_binding {
            self.facts(first.children[0])?
                .and_then(|fact| match &fact.kind {
                    NodeFactKind::Pattern(pattern) => Some(pattern),
                    _ => None,
                })
        } else {
            None
        };
        let Some(container) = self.container(&value.ty) else {
            let entry = state.fork(self.budget)?;
            let mut success = Vec::new();
            let mut reached = false;
            for (truth, state) in self.branch_condition(operand, state, depth + 1)? {
                if truth {
                    success.extend(self.singleton(state)?);
                } else {
                    self.statement(failure, state, depth + 1)?;
                    reached = true;
                }
            }
            if !reached {
                self.dead_statement(failure, entry, depth + 1)?;
            }
            return Ok(success);
        };
        let error = if children.len() == 3 {
            self.declaration_at(children[1], DeclarationKind::Binding)?
        } else {
            None
        };
        let root = Root::Temporary(operand);
        let mut success = Vec::new();
        let mut entered_failure = false;
        for state in self.expression(operand, state, depth + 1)? {
            let mut failed = Vec::new();
            let mut mismatch = Vec::new();
            if let Some(state) =
                self.split_variant(state, root, container.success, &mut failed, depth + 1)?
            {
                self.guard_success(
                    node,
                    root,
                    container.success,
                    state,
                    pattern,
                    &mut success,
                    &mut mismatch,
                    depth + 1,
                )?;
            }
            for mut state in failed {
                if let Some(error) = error {
                    let payload = [ResultComponent::VariantPayload {
                        variant: container.failure,
                        index: 0,
                    }];
                    if !matches!(value.ty.as_ref(), TypeTerm::Reference { .. })
                        && value.access == Access::Owned
                    {
                        self.move_value(&mut state, root, &payload, Root::Local(error), &[])?;
                    } else if let Some((owner, mut path)) = state.alias(root, &[], self.budget)? {
                        self.budget.reserve(1)?;
                        path.extend(payload);
                        state.store(
                            Root::Local(error),
                            &[],
                            Content::Alias((owner, path)),
                            self.budget,
                        )?;
                    }
                }
                self.guard_failure_edge(node, failure, root, state, depth + 1)?;
                entered_failure = true;
            }
            for state in mismatch {
                self.guard_failure_edge(node, failure, root, state, depth + 1)?;
                entered_failure = true;
            }
        }
        if !entered_failure {
            self.dead_statement(failure, Residence::default(), depth + 1)?;
        }
        Ok(success)
    }

    #[allow(clippy::too_many_arguments)]
    fn guard_success(
        &mut self,
        node: NodeId,
        root: Root<'g>,
        variant: &'g DeclarationId,
        mut state: Residence<'g>,
        pattern: Option<&PatternFacts<'g>>,
        success: &mut Alternatives<'g>,
        mismatch: &mut Alternatives<'g>,
        depth: usize,
    ) -> Work<()> {
        if let Some(pattern) = pattern {
            let (owner, mut path) = state
                .alias(root, &[], self.budget)?
                .unwrap_or((root, Vec::new()));
            self.budget.reserve(1)?;
            path.push(ResultComponent::VariantPayload { variant, index: 0 });
            let address = (owner, path);
            let Some(matched) =
                self.split_pattern(state, &address, &pattern.pattern, mismatch, depth)?
            else {
                return Ok(());
            };
            state = matched;
            self.commit_pattern(&mut state, address.0, &address.1, pattern)?;
        }
        let discarded = state.take(root, &[], self.budget)?;
        self.discard(discarded, root, node, ResultDiscardKind::Expression)?;
        success.extend(self.singleton(state)?);
        Ok(())
    }

    fn guard_failure_edge(
        &mut self,
        node: NodeId,
        failure: NodeId,
        root: Root<'g>,
        mut state: Residence<'g>,
        depth: usize,
    ) -> Work<()> {
        let discarded = state.take(root, &[], self.budget)?;
        self.discard(discarded, root, node, ResultDiscardKind::Expression)?;
        // A settled guard has no normal failure continuation.
        self.statement(failure, state, depth)?;
        Ok(())
    }

    pub(super) fn split_variant(
        &mut self,
        state: Residence<'g>,
        root: Root<'g>,
        variant: &'g DeclarationId,
        failed: &mut Alternatives<'g>,
        depth: usize,
    ) -> Work<Option<Residence<'g>>> {
        self.split_pattern(
            state,
            &(root, Vec::new()),
            &Pattern::Constructor(Constructor::Variant(variant), Vec::new(), None),
            failed,
            depth,
        )
    }

    fn container(&self, ty: &Type<'g>) -> Option<Container<'g>> {
        let ty = match ty.as_ref() {
            TypeTerm::Reference { referent, .. } => referent,
            _ => ty,
        };
        let ty = match ty.as_ref() {
            TypeTerm::Variant { parent, .. } => parent,
            _ => ty,
        };
        let TypeTerm::Nominal { declaration, .. } = ty.as_ref() else {
            return None;
        };
        [Some(self.core.result), self.core.option]
            .into_iter()
            .flatten()
            .find(|role| role.declaration == *declaration)
    }
}
