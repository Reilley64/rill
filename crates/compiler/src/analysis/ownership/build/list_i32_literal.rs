//! A `List<i32>` literal of integer literals, committed through the linked
//! `List::from_elements` (ri-e9t8).
use super::*;

impl<'a, 'g> Build<'a, 'g, '_> {
    /// Stage each element literal in written order into its own scratch
    /// `i32`, then commit them all to `into` in one step. No partial List is
    /// published, and no stage is live after the commitment.
    pub(super) fn list_i32_literal(
        &mut self,
        node: NodeId,
        facts: &ExpressionFacts<'g>,
        into: Option<Slot>,
        next: Pc,
        depth: usize,
    ) -> ResultOf<Pc> {
        let pending = || Stop::Pending(node);
        // Screen first (metering rule 1): only facts already paid for, no step,
        // reservation or allocation before the last early return on shape.
        let Some(DataExpression::List {
            declaration,
            elements,
        }) = &facts.data
        else {
            return Err(pending());
        };
        let nodes = &self.module.syntax.nodes;
        let into = into.ok_or_else(pending)?;
        // Only a literal whose context declares `List<i32>` (an annotated binding or
        // a typed argument): the unannotated `let values = [13i32, 29i32];` keeps its
        // published fixed caps (`legacy-caps/foreign-rc`).
        if facts.access != Access::Owned
            || facts.expected.as_ref() != Some(&facts.ty)
            || elements.is_empty()
            || elements.as_slice() != nodes[node].children.as_slice()
            || elements
                .iter()
                .any(|&element| nodes[element].kind != NodeKind::Literal)
            || !self.is_list_i32(&facts.ty)
            || !self
                .typed
                .list_role()
                .is_some_and(|role| std::ptr::eq(*declaration, &role.declaration.id))
        {
            return Err(pending());
        }
        let list = self.typed.list_role().ok_or_else(pending)?;
        let provider = self.typed.list_operations_artifact().ok_or_else(pending)?;
        let mut backing = None;
        for repr in self.typed.collection_representations() {
            self.budget.step(1)?;
            if repr.storage == CollectionStorage::Elements
                && std::ptr::eq(repr.declaration, list.declaration)
                && std::ptr::eq(repr.artifact.context(), provider.context())
            {
                backing = Some(repr);
                break;
            }
        }
        let backing = backing.ok_or_else(pending)?;
        self.budget.reserve(elements.len())?;
        let mut stages = Vec::with_capacity(elements.len());
        for &element in elements {
            let ty = &self.normal_value(element)?.ty;
            if scalar::scalar(ty) != Some(Primitive::I32) {
                return Err(pending());
            }
            stages.push(self.slot(element, ty)?);
        }
        // Commitment reads the stages as one run from the first.
        if stages
            .iter()
            .enumerate()
            .any(|(index, &stage)| stage != stages[0] + index)
        {
            return Err(pending());
        }
        let mut tail = self.emit(
            node,
            Action::CommitList {
                first: stages[0],
                count: stages.len(),
                into,
                backing,
                provider,
            },
            Edges::Next(next),
        )?;
        for (&element, &stage) in elements.iter().zip(&stages).rev() {
            self.budget.step(1)?;
            tail = self.expression(element, Some(stage), tail, depth + 1)?;
        }
        Ok(tail)
    }
}
