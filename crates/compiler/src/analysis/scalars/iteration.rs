//! Conditional iterator selection for the shared typed checker.
use super::*;

/// A conditional iteration handoff; the goals retain selection, substitution,
/// required arguments and the ordinary-role/protection premise.
#[derive(Debug)]
pub struct Iteration<'g> {
    pub source: NodeId,
    pub pattern: NodeId,
    pub iterate: generics::GoalId,
    pub next: generics::GoalId,
    pub iterator: Type<'g>,
    pub yielded: Type<'g>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn iteration_statement(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        let children = &module.syntax.nodes[node].children;
        let source = self.expression(module, children[1], None, context)?;
        if self.collections.iteration.is_none() || self.option.is_none() {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
        } else if let Some(source) = &source {
            let handoff = self.iteration_handoff(module, node, source.ty.clone())?;
            self.defer(
                module,
                node,
                if handoff.is_some() {
                    ScalarUnsupported::CoreProtection
                } else {
                    ScalarUnsupported::FamilySelection
                },
            )?;
            if let Some(handoff) = handoff {
                self.elab.reserve(1)?;
                self.record_node(NodeFacts {
                    site: Self::site(module, node),
                    kind: NodeFactKind::Iteration(handoff),
                    flow: Flow {
                        pending: true,
                        ..Flow::default()
                    },
                    callable: None,
                });
            }
        }
        self.elab.reserve(2)?;
        context.scopes.push(BTreeMap::new());
        context.loops.push(node);
        // A conditional winner cannot bind a real pattern or prove control flow.
        self.pending_pattern(module, children[0], context)?;
        self.statement(module, children[2], depth + 1, context)?;
        context.loops.pop();
        context.scopes.pop();
        Ok(source.map_or_else(Flow::normal, |e| e.flow))
    }

    fn iteration_handoff(
        &mut self,
        module: &'g Module,
        node: NodeId,
        input: Type<'g>,
    ) -> Work<Option<Iteration<'g>>> {
        use generics::GoalVerdict;
        if primitive_name(&input) == Some("never") {
            return Ok(None);
        }
        let role = self.collections.iteration.unwrap();
        let option = self.option.unwrap();
        let site = Self::site(module, node);
        self.elab.reserve(1)?;
        let iterate = self.determine_source(site, &role.iterate.id, vec![input], &[], &[], None)?;
        let goal = self.evidence.get(&iterate).unwrap();
        let Some(output) = &goal.output else {
            return Ok(None);
        };
        if goal.selection.is_none() || goal.hypothetical != Some(GoalVerdict::Proved) {
            return Ok(None);
        }
        let iterator = output.result.clone();
        self.elab.reserve(2)?;
        let mutable_iterator = Arc::new(TypeTerm::Reference {
            mutable: true,
            referent: iterator.clone(),
        });
        let next =
            self.determine_source(site, &role.next.id, vec![mutable_iterator], &[], &[], None)?;
        let goal = self.evidence.get(&next).unwrap();
        let Some(output) = &goal.output else {
            return Ok(None);
        };
        if goal.selection.is_none() || goal.hypothetical != Some(GoalVerdict::Proved) {
            return Ok(None);
        }
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = output.result.as_ref()
        else {
            return Ok(None);
        };
        let [yielded] = arguments.as_slice() else {
            return Ok(None);
        };
        if **declaration != option.declaration.id {
            return Ok(None);
        }
        let [pattern, source, _] = module.syntax.nodes[node].children.as_slice() else {
            unreachable!()
        };
        Ok(Some(Iteration {
            source: *source,
            pattern: *pattern,
            iterate,
            next,
            iterator,
            yielded: yielded.clone(),
        }))
    }
}
