//! Check typed arrow parameters; unknown capture acquisition is not an empty environment.
use super::*;
use crate::analysis::scalars::generics::callables::{ArrowFacts, CaptureForm};

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn arrow(
        &mut self,
        node: NodeId,
        facts: &NodeFacts<'g>,
        mut state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let Some(arrow) = facts
            .callable
            .as_ref()
            .and_then(|facts| facts.arrow.as_ref())
            .filter(|arrow| arrow.premise.is_none())
        else {
            self.pending(node, ResultUseBlocker::Containment)?;
            return self.unsettled(node, depth);
        };
        self.budget.reserve(1)?;
        if self.checked_arrows.insert(node) {
            self.arrow_body(node, arrow, depth + 1)?;
        }
        let root = Root::Temporary(node);
        for capture in &arrow.captures {
            self.budget.step(1)?;
            for address in self.place_storage(&state, Some(&capture.path))?.addresses() {
                if capture.form == CaptureForm::Value {
                    let affected = state.take(address.0, &address.1, self.budget)?;
                    self.budget.step(affected.len())?;
                    if affected.into_iter().any(|value| {
                        matches!(value.content, Content::Result(_) | Content::Unclassified(_))
                    }) {
                        self.pending(node, ResultUseBlocker::Storage)?;
                        state.store(root, &[], Content::Unclassified(node), self.budget)?;
                    }
                } else {
                    // Formation does not observe the owner's result. A later
                    // invocation may, but the layout/body effect is not proved.
                    let alias = state
                        .alias(address.0, &address.1, self.budget)?
                        .unwrap_or(address);
                    state.store(root, &[], Content::PossibleAlias(alias), self.budget)?;
                }
            }
        }
        self.singleton(state)
    }

    fn arrow_body(&mut self, node: NodeId, arrow: &ArrowFacts<'g>, depth: usize) -> Work<()> {
        let children = &self.module.syntax.nodes[node].children;
        let (head, body) = (children[0], children[1]);
        let parameters =
            &self.module.syntax.nodes[self.module.syntax.nodes[head].children[0]].children;
        let mut entry = self.singleton(Residence::default())?;
        for (&parameter, ty) in parameters.iter().zip(&arrow.parameters) {
            if let Some(declaration) = self.declaration_at(parameter, DeclarationKind::Parameter)? {
                entry = self.seed(entry, Root::Local(declaration), parameter, ty)?;
            } else {
                self.pending(parameter, ResultUseBlocker::TypedInput)?;
            }
        }
        if self.module.syntax.nodes[body].kind == NodeKind::Block {
            for state in entry {
                for state in self.statement(body, state, depth)? {
                    self.exit(state, body)?;
                }
            }
        } else {
            for state in entry {
                for mut state in self.expression(body, state, depth)? {
                    state.take(Root::Temporary(body), &[], self.budget)?;
                    self.exit(state, body)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn declaration_at(
        &mut self,
        node: NodeId,
        kind: DeclarationKind,
    ) -> Work<Option<&'g DeclarationId>> {
        let start = self.site(node).span.start();
        let declarations = &self.module.declarations;
        self.budget
            .step(1 + declarations.len().checked_ilog2().unwrap_or(0) as usize)?;
        for declaration in &declarations[declarations.partition_point(|d| d.id.start < start)..] {
            self.budget.step(1)?;
            if declaration.id.start != start {
                break;
            }
            if declaration.node == node && declaration.id.kind == kind {
                return Ok(Some(&declaration.id));
            }
        }
        Ok(None)
    }
}
