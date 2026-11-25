//! Concrete Result-use for one fresh canonical result beside a scalar job.
use super::*;

pub(super) struct FreshPlan<'g> {
    pub(super) expression: NodeId,
    pub(super) construction: Option<Site<'g>>,
    variant: Option<&'g DeclarationId>,
    capture: Option<&'g DeclarationId>,
    depth: usize,
}

impl<'a, 'g> Check<'a, 'g> {
    fn bool_i32_result(&self, ty: &Type<'g>) -> bool {
        matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
            if *declaration == self.core.result.declaration
                && matches!(arguments.as_slice(), [ok, err]
                    if primitive(ok, "bool") && primitive(err, "i32")))
    }

    pub(in crate::analysis::typed::result_use) fn fresh_result_slot(
        &self,
        ty: &Type<'g>,
    ) -> Option<usize> {
        let TypeTerm::Tuple(items) = ty.as_ref() else {
            return None;
        };
        let [a, b] = items.as_slice() else {
            return None;
        };
        if primitive(b, "bool") && self.bool_i32_result(a) {
            Some(0)
        } else if primitive(a, "bool") && self.bool_i32_result(b) {
            Some(1)
        } else {
            None
        }
    }

    fn captured_flag_source(&mut self, declaration: &'g DeclarationId) -> Work<Root<'g>> {
        // A whole-local capture has no projections. Resolve the caller's
        // current storage; a copyable Boolean match does not redirect it.
        let mut source = Root::Local(declaration);
        self.redirect_view(&mut source, &mut Vec::new())?;
        Ok(source)
    }

    /// On the proved immediate handoff the sibling returns the value it just
    /// captured. An unknown flag has two possible *equal* input/output values,
    /// not four independent combinations. Future writes can change either slot.
    pub(in crate::analysis::typed::result_use) fn split_captured_flag(
        &mut self,
        call: NodeId,
        slot: usize,
        state: &mut Residence<'g>,
        forwarded: &mut Alternatives<'g>,
    ) -> Work<()> {
        self.budget.step(self.module.key.path.len() + 2)?;
        let index = self.table.get(&call).unwrap().0;
        let CheckedTransfer::Scoped(join) = &self.report.pair_calls[&index] else {
            unreachable!()
        };
        let [capture] = join.slots[slot].child.arrow.captures.as_slice() else {
            return Ok(());
        };
        let input = self.captured_flag_source(capture.path.root)?;
        if state
            .test(input, &[], &Constructor::Boolean(true), self.budget)?
            .is_none()
        {
            let output = Root::Temporary(call);
            let path = [ResultComponent::TupleElement(slot)];
            let mut other = state.fork(self.budget)?;
            for (branch, truth) in [(&mut other, false), (state, true)] {
                self.record_boolean(branch, input, &[], truth)?;
                self.record_boolean(branch, output, &path, truth)?;
            }
            self.budget.reserve(1)?;
            forwarded.push(other);
        }
        Ok(())
    }

    pub(super) fn fresh_pair_plan(
        &mut self,
        arrow_node: NodeId,
        arrow: &crate::analysis::generics::callables::ArrowFacts<'g>,
        (stored, slot): (&[Located<'g>], ResultComponent<'g>),
        returns_result: bool,
        depth: usize,
    ) -> Work<Option<FreshPlan<'g>>> {
        let nodes = &self.module.syntax.nodes;
        let body = nodes[arrow_node].children[1];
        let (node, expression_depth) = self.value_root_at(body, depth + 1)?;
        let Some(facts) = self.facts(node)? else {
            return Ok(None);
        };
        let NodeFactKind::Expression(value) = &facts.kind else {
            return Ok(None);
        };
        if !facts.flow.normal || value.ordinary_context.is_some() || arrow.mode.is_none() {
            return Ok(None);
        }
        let (variant, capture) = if returns_result {
            if !arrow.captures.is_empty() {
                return Ok(None);
            }
            let (payload, kind, variant) = match &value.data {
                Some(DataExpression::Construction(ok))
                    if ok.base.is_none()
                        && ok.fields.is_empty()
                        && matches!(ok.ty.as_ref(), TypeTerm::Variant { parent, declaration }
                            if *declaration == self.core.result.success && self.bool_i32_result(parent)) =>
                {
                    let [payload] = ok.arguments.as_slice() else {
                        return Ok(None);
                    };
                    (*payload, "bool", self.core.result.success)
                }
                Some(DataExpression::ErrorConstruction(err))
                    if err.operation == crate::analysis::typed::ErrorOperation::Construct
                        && err.retained.is_none()
                        && self.bool_i32_result(&err.result)
                        && err.failure == self.core.result.failure =>
                {
                    (err.value, "i32", self.core.result.failure)
                }
                _ => return Ok(None),
            };
            let (payload, _) = self.value_root_at(payload, expression_depth + 1)?;
            if nodes[payload].kind != NodeKind::Literal
                || !self
                    .expression_facts(payload)?
                    .is_some_and(|v| primitive(&v.ty, kind))
            {
                return Ok(None);
            }
            (Some(variant), None)
        } else {
            let scalar = match arrow.captures.as_slice() {
                [] => nodes[node].kind == NodeKind::Literal,
                [capture] => {
                    if nodes[node].kind != NodeKind::ValueReference
                        || !capture.reachable
                        || capture.through.is_some()
                        || capture.form != CaptureForm::Value
                        || !capture.path.projections.is_empty()
                        || !primitive(&capture.ty, "bool")
                        || !capture
                            .storage
                            .as_ref()
                            .is_some_and(|ty| primitive(ty, "bool"))
                    {
                        return Ok(None);
                    }
                    self.budget.step(2)?;
                    std::ptr::eq(capture.site.module, &self.module.key)
                        && self.value_root_at(capture.site.node, depth + 1)?.0 == node
                        && value.path.as_ref().is_some_and(|path| {
                            path.projections.is_empty()
                                && std::ptr::eq(path.root, capture.path.root)
                        })
                }
                _ => false,
            };
            if !scalar
                || value.call.is_some()
                || value.data.is_some()
                || !primitive(&value.ty, "bool")
            {
                return Ok(None);
            }
            (
                None,
                arrow.captures.first().map(|capture| capture.path.root),
            )
        };
        if !self.pair_environment(arrow_node, (stored, slot), None)? {
            return Ok(None);
        }
        self.budget.reserve(1)?;
        Ok(Some(FreshPlan {
            expression: body,
            construction: variant.map(|_| self.site(node)),
            variant,
            capture,
            depth: depth + 1,
        }))
    }

    pub(super) fn prove_fresh_pair_plan(
        &mut self,
        plan: FreshPlan<'g>,
        caller: &Residence<'g>,
    ) -> Work<Option<Vec<Located<'g>>>> {
        let mut environment = Residence::default();
        if let Some(capture) = plan.capture {
            // Neither acquisition writes this scalar. Read the captured value,
            // not a fresh unconstrained bool that could invent a discard path.
            let source = self.captured_flag_source(capture)?;
            if let Some(truth) =
                caller.test(source, &[], &Constructor::Boolean(true), self.budget)?
            {
                self.record_boolean(&mut environment, Root::Local(capture), &[], truth)?;
            }
        }
        // Reuse the expression's own construction and outcome tag. Seeding
        // from the return type would invent a new occurrence at the join.
        let mut proof = self.callable_check(self.module, self.table)?;
        let mut alternatives = proof.expression(plan.expression, environment, plan.depth)?;
        let [state] = alternatives.as_mut_slice() else {
            return Ok(None);
        };
        let root = Root::Temporary(plan.expression);
        if let Some(variant) = plan.variant
            && state.test(root, &[], &Constructor::Variant(variant), proof.budget)? != Some(true)
        {
            return Ok(None);
        }
        let output = state.take(root, &[], proof.budget)?;
        if proof.has_pending || state.may_owe_result(proof.budget)? {
            return Ok(None);
        }
        let mut occurrences = 0;
        for entry in &output {
            proof.budget.step(1)?;
            match &entry.content {
                Content::Result(value)
                    if plan
                        .construction
                        .is_some_and(|site| value.origin == site.node)
                        && entry.component.is_empty()
                        && value.component.is_empty() =>
                {
                    occurrences += 1;
                }
                Content::Test(_, _) => (),
                _ => return Ok(None),
            }
        }
        Ok((occurrences == usize::from(plan.construction.is_some())).then_some(output))
    }
}

fn primitive(ty: &Type<'_>, name: &str) -> bool {
    matches!(ty.as_ref(), TypeTerm::Primitive(id) if id.name() == name)
}
