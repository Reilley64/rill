//! Concrete Result-use for one fresh canonical result beside a scalar job.
use super::*;

pub(super) struct FreshChild<'g> {
    pub(super) expression: NodeId,
    pub(super) output: Vec<Located<'g>>,
    pub(super) construction: Option<Site<'g>>,
}

impl<'a, 'g> Check<'a, 'g> {
    fn bool_i32_result(&self, ty: &Type<'g>) -> bool {
        matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
            if *declaration == self.core.result.declaration
                && matches!(arguments.as_slice(), [ok, err]
                    if primitive(ok, "bool") && primitive(err, "i32")))
    }

    pub(super) fn fresh_result_slot(&self, ty: &Type<'g>) -> Option<usize> {
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

    pub(super) fn fresh_pair_child(
        &mut self,
        arrow_node: NodeId,
        arrow: &crate::analysis::generics::callables::ArrowFacts<'g>,
        (stored, slot): (&[Located<'g>], ResultComponent<'g>),
        returns_result: bool,
        depth: usize,
    ) -> Work<Option<FreshChild<'g>>> {
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
        let variant = if returns_result {
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
            Some(variant)
        } else {
            let scalar = match arrow.captures.as_slice() {
                [] => nodes[node].kind == NodeKind::Literal,
                [capture] => {
                    nodes[node].kind == NodeKind::ValueReference
                        && capture.site.node == node
                        && capture.reachable
                        && capture.through.is_none()
                        && capture.form == CaptureForm::Value
                        && capture.path.projections.is_empty()
                        && capture.storage.as_ref() == Some(&capture.ty)
                        && primitive(&capture.ty, "bool")
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
            None
        };
        if !self.pair_environment(arrow_node, (stored, slot), None)? {
            return Ok(None);
        }
        // Reuse the expression's own construction and outcome tag. Seeding
        // from the return type would invent a new occurrence at the join.
        let mut proof = self.callable_check(self.module)?;
        let mut alternatives = proof.expression(body, Residence::default(), depth + 1)?;
        let [state] = alternatives.as_mut_slice() else {
            return Ok(None);
        };
        let root = Root::Temporary(body);
        if let Some(variant) = variant
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
                    if variant.is_some()
                        && value.origin == node
                        && entry.component.is_empty()
                        && value.component.is_empty() =>
                {
                    occurrences += 1;
                }
                Content::Test(_, _) => (),
                _ => return Ok(None),
            }
        }
        if occurrences != usize::from(returns_result) {
            return Ok(None);
        }
        if returns_result {
            proof.budget.reserve(1)?;
        }
        Ok(Some(FreshChild {
            expression: body,
            output,
            construction: variant.map(|_| self.site(node)),
        }))
    }
}

fn primitive(ty: &Type<'_>, name: &str) -> bool {
    matches!(ty.as_ref(), TypeTerm::Primitive(id) if id.name() == name)
}
