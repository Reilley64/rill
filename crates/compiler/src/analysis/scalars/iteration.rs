//! Iteration through the same selection and evidence engine as written calls.
use super::*;
use crate::analysis::typed::{CollectionOperation, ListTraversal};

/// Checked handoff at the original `for ... of` node. Goal queries retain the
/// selected bodies and exact substitutions; this is not an ownership judgment.
#[derive(Debug)]
pub struct Iteration<'g> {
    pub source: NodeId,
    pub pattern: NodeId,
    pub iterate: generics::GoalId,
    pub next: generics::GoalId,
    pub iterator: Type<'g>,
    pub yielded: Type<'g>,
    pub some: &'g DeclarationId,
    pub none: &'g DeclarationId,
    /// No storage handoff is implied by a user iterator's checked signatures.
    pub traversal: Option<ListTraversal>,
    /// Ordinary local control, bindings, place reads/borrows or disposal;
    /// excludes nested iteration, propagation and computed scalar expressions.
    /// This describes the written body, not ownership of its operands.
    pub(in crate::analysis) simple_body: bool,
    pub(super) text_input: Option<&'g DeclarationId>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn iteration_statement(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<(Flow, Option<Iteration<'g>>)> {
        let &[pattern, operand, block] = module.syntax.nodes[node].children.as_slice() else {
            unreachable!("parsed for-of")
        };
        let before = self.marks();
        let source = self.expression(module, operand, None, context)?;
        let mut handoff = if let Some(source) = &source
            && before == self.marks()
            && primitive_name(&source.ty) != Some("never")
        {
            self.iteration_handoff(module, node, source.ty.clone())?
        } else {
            None
        };
        self.elab.reserve(2)?;
        context.scopes.push(BTreeMap::new());
        context.loops.push(node);
        if let Some(handoff) = &handoff {
            self.iteration_pattern(module, handoff, depth + 1, context)?;
        } else {
            if before == self.marks() {
                self.defer(module, pattern, ScalarUnsupported::FamilySelection)?;
            }
            self.pending_pattern(module, pattern, context)?;
        }
        let enclosing = std::mem::replace(&mut self.simple_iteration_body, handoff.is_some());
        let body = self.statement(module, block, depth + 1, context);
        let simple = std::mem::replace(&mut self.simple_iteration_body, enclosing);
        if let Some(iteration) = &mut handoff {
            iteration.simple_body = simple;
        }
        let mut body = body?;
        if simple
            && context.scopes.last().is_some_and(|scope| scope.len() == 1)
            && let (Some(iteration), Some(value)) = (&mut handoff, &source)
            && iteration.traversal == Some(ListTraversal::Owned)
            && primitive_name(&iteration.yielded) == Some("string")
            && value.access == data::Access::Owned
            && let Some(path) = value.path.as_ref()
            && path.root.kind == DeclarationKind::Parameter
            && path.projections.is_empty()
        {
            // Written source only. The enclosing block separately knows whether
            // its prefix can actually reach this loop.
            iteration.text_input = Some(path.root);
        }
        context.loops.pop();
        context.scopes.pop();

        let mut flow = source.map_or_else(Flow::normal, |e| e.flow);
        if handoff.is_some() && flow.normal {
            body.breaks.remove(&node);
            body.continues.remove(&node);
            // The checked List traversal can end immediately (None), irrespective
            // of the body. The remaining body exits are possible Some edges.
            flow = self.union(flow, &body)?;
        }
        Ok((flow, handoff))
    }

    fn iteration_handoff(
        &mut self,
        module: &'g Module,
        node: NodeId,
        input: Type<'g>,
    ) -> Work<Option<Iteration<'g>>> {
        use generics::GoalState;
        let (Some(role), Some(option)) = (self.collections.iteration, self.option) else {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            return Ok(None);
        };
        let site = Self::site(module, node);
        let scope = self.collections.scope(&role.iterate.id, &[Some(&input)]);
        self.elab.reserve(1)?;
        let iterate = self.determine_source(site, &role.iterate.id, vec![input], &[], &[], None)?;
        let goal = self.evidence.get(&iterate).unwrap();
        let Some(output) = goal
            .output
            .as_ref()
            .filter(|_| goal.state == GoalState::Proved && goal.premise.is_none())
        else {
            self.defer(
                module,
                node,
                if scope.is_some() {
                    ScalarUnsupported::CoreProtection
                } else {
                    ScalarUnsupported::FamilySelection
                },
            )?;
            return Ok(None);
        };
        let iterator = output.result.clone();
        self.elab.reserve(2)?;
        let mutable_iterator = Arc::new(TypeTerm::Reference {
            mutable: true,
            referent: iterator.clone(),
        });
        let next_scope = self
            .collections
            .scope(&role.next.id, &[Some(&mutable_iterator)]);
        let next =
            self.determine_source(site, &role.next.id, vec![mutable_iterator], &[], &[], None)?;
        let goal = self.evidence.get(&next).unwrap();
        let Some(output) = goal
            .output
            .as_ref()
            .filter(|_| goal.state == GoalState::Proved && goal.premise.is_none())
        else {
            self.defer(
                module,
                node,
                if next_scope.is_some() {
                    ScalarUnsupported::CoreProtection
                } else {
                    ScalarUnsupported::FamilySelection
                },
            )?;
            return Ok(None);
        };
        let yielded = match output.result.as_ref() {
            TypeTerm::Nominal {
                declaration,
                arguments,
            } if **declaration == option.declaration.id => match arguments.as_slice() {
                [yielded] => Some(yielded),
                _ => None,
            },
            _ => None,
        };
        let Some(yielded) = yielded else {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            return Ok(None);
        };
        let [pattern, source, _] = module.syntax.nodes[node].children.as_slice() else {
            unreachable!()
        };
        let traversal = match (scope, next_scope) {
            (Some(iterate), Some(next)) => match (iterate.operation, next.operation) {
                (CollectionOperation::Iterate(input), CollectionOperation::Next(output))
                    if input == output =>
                {
                    Some(input)
                }
                _ => None,
            },
            _ => None,
        };
        self.elab.reserve(2)?;
        Ok(Some(Iteration {
            source: *source,
            pattern: *pattern,
            iterate,
            next,
            iterator,
            yielded: yielded.clone(),
            some: &option.some.id,
            none: &option.none.id,
            traversal,
            simple_body: false,
            text_input: None,
        }))
    }
}
