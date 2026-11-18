//! Check typed arrow parameters; unknown capture acquisition is not an empty environment.
use super::*;
use crate::analysis::InvocationMode;
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
            self.arrow_body(node, arrow, Residence::default(), depth + 1)?;
        }
        let root = Root::Temporary(node);
        state.store(root, &[], Content::Callable(node), self.budget)?;
        let mut acquired = BTreeSet::new();
        for capture in &arrow.captures {
            self.budget.step(1)?;
            if capture.form == CaptureForm::Value
                && matches!(capture.ty.as_ref(), TypeTerm::Primitive(_))
            {
                continue;
            }
            let storage = self.place_storage(&state, Some(&capture.path))?;
            if capture.form == CaptureForm::Value
                && capture.demand == Some(InvocationMode::Once)
                && capture.path.projections.is_empty()
                && matches!(storage, PlaceStorage::Exact((Root::Local(id), ref component)) if id == capture.path.root && component.is_empty())
            {
                self.budget.reserve(1)?;
                if acquired.insert(capture.path.root) {
                    // Keep the actual occurrences, not a type-only approximation of the environment.
                    let source = Root::Local(capture.path.root);
                    let values = state.take(source, &[], self.budget)?;
                    if values.is_empty() {
                        // A missing residence (for example, in a detached lexical
                        // entry) is not evidence that the environment is empty.
                        let probe = self.singleton(Residence::default())?;
                        for shape in self.seed(probe, source, node, &capture.ty)? {
                            if shape.may_owe_result(self.budget)? {
                                state.store(root, &[], Content::Unclassified(node), self.budget)?;
                                break;
                            }
                        }
                    }
                    state.install(
                        values,
                        root,
                        &[ResultComponent::Capture(capture.path.root)],
                        self.budget,
                    )?;
                }
            } else {
                // A projection or borrow does not establish an owned environment.
                // The explicit acknowledgement of an uninvoked callable can still consume it.
                state.store(root, &[], Content::Unclassified(node), self.budget)?;
                for address in storage.addresses() {
                    if capture.form != CaptureForm::Value {
                        let alias = state
                            .alias(address.0, &address.1, self.budget)?
                            .unwrap_or(address);
                        state.store(root, &[], Content::PossibleAlias(alias), self.budget)?;
                    } else if capture.demand != Some(InvocationMode::ReadOnly) {
                        // Do not claim a discard in the original place when acquisition may move it.
                        self.forget_address(&mut state, &address, node)?;
                    }
                }
            }
        }
        self.singleton(state)
    }

    pub(super) fn arrow_body(
        &mut self,
        node: NodeId,
        arrow: &ArrowFacts<'g>,
        captures: Residence<'g>,
        depth: usize,
    ) -> Work<()> {
        let children = &self.module.syntax.nodes[node].children;
        let (head, body) = (children[0], children[1]);
        let parameters =
            &self.module.syntax.nodes[self.module.syntax.nodes[head].children[0]].children;
        let previous = std::mem::take(&mut self.current_parameters);
        let mut entry = self.singleton(captures)?;
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
        self.current_parameters = previous;
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
