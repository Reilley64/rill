//! Stage the written prefix of a consuming text List literal before commitment.
use super::*;

pub(super) struct ListTextSource<'a, 'g> {
    pub(super) literal: NodeId,
    pub(super) provider: CollectionArtifact<'g>,
    elements: &'a [NodeId],
    literal_depth: usize,
    position: usize,
    postfix: NodeId,
    postfix_depth: usize,
    checked: CheckedPropagation<'a, 'g>,
    returning: Pc,
}

impl ListTextSource<'_, '_> {
    pub(super) fn can_commit(&self) -> bool {
        self.checked.exit.can_continue
    }
}

impl<'a, 'g> Build<'a, 'g, '_> {
    pub(super) fn checked_list_text_source(
        &mut self,
        node: NodeId,
        (source, expected_literal): (NodeId, NodeId),
        actual: &ExpressionFacts<'g>,
        exits: Exits,
        depth: usize,
    ) -> ResultOf<ListTextSource<'a, 'g>> {
        let pending = || Stop::Pending(node);
        let returning = exits.returning.ok_or_else(pending)?;
        if !matches!(
            self.module.syntax.nodes[source].kind,
            NodeKind::Bracket | NodeKind::Group
        ) || actual.access != Access::Owned
            || !self.is_list_text(&actual.ty)
        {
            return Err(pending());
        }
        let flow = &self.fact(&source).ok_or_else(pending)?.flow;
        // Preserve ordinary literals and computed sources outside this judgment.
        if flow.pending || !flow.returns || !flow.breaks.is_empty() || !flow.continues.is_empty() {
            return Err(pending());
        }
        let (literal, literal_depth) = self.ungroup_expression_at(source, depth)?;
        if literal != expected_literal {
            return Err(pending());
        }
        self.budget.step(1)?;
        let bracket = self.value(literal).map_err(|_| pending())?;
        let Some(DataExpression::List {
            declaration,
            elements,
        }) = &bracket.data
        else {
            return Err(pending());
        };
        if bracket.ty != actual.ty
            || bracket.access != Access::Owned
            || self.module.syntax.nodes[literal].kind != NodeKind::Bracket
            || !self
                .typed
                .list_role()
                .is_some_and(|role| std::ptr::eq(*declaration, &role.declaration.id))
        {
            return Err(pending());
        }
        self.budget.step(elements.len())?;
        if elements.as_slice() != self.module.syntax.nodes[literal].children.as_slice() {
            return Err(pending());
        }
        let mut propagation = None;
        for (position, &element) in elements.iter().enumerate() {
            self.budget.step(1)?;
            let (root, root_depth) = self.ungroup_expression_at(element, literal_depth + 1)?;
            if self.module.syntax.nodes[root].kind == NodeKind::Propagate {
                if propagation.replace((position, root, root_depth)).is_some()
                    || !owns_text(&self.value(root)?.ty)
                {
                    return Err(pending());
                }
            } else if !matches!(
                self.module.syntax.nodes[root].kind,
                NodeKind::ValueReference | NodeKind::Literal
            ) {
                return Err(pending());
            } else {
                let value = self.normal_value(element)?;
                if value.access != Access::Owned || !owns_text(&value.ty) {
                    return Err(pending());
                }
            }
        }
        let (position, postfix, postfix_depth) = propagation.ok_or_else(pending)?;
        self.budget.step(2)?;
        let checked = self.checked_propagation(postfix, self.value(postfix)?, postfix_depth)?;
        if !matches!(checked.option_take, Some((_, OptionTakeSource::Move(_))))
            || !checked.exit.can_return
            || checked.exit.can_continue != flow.normal
        {
            return Err(pending());
        }
        let provider = self.typed.list_operations_artifact().ok_or_else(pending)?;
        Ok(ListTextSource {
            literal,
            provider,
            elements,
            literal_depth,
            position,
            postfix,
            postfix_depth,
            checked,
            returning,
        })
    }

    pub(super) fn stage_list_text_source(
        &mut self,
        source: ListTextSource<'a, 'g>,
        commitment: Option<(Slot, CollectionRepresentation<'g>, Pc)>,
    ) -> ResultOf<Pc> {
        let count = if source.can_commit() {
            source.elements.len()
        } else {
            source.position
        };
        let stages = &source.elements[..count];
        self.budget.reserve(count)?;
        let mut slots = Vec::with_capacity(count);
        for &element in stages {
            self.budget.step(1)?;
            slots.push(self.slot(element, &self.value(element)?.ty)?);
        }
        // A failed postfix exports first, then retires the preceding string
        // residences in reverse order. No List backing exists on this edge.
        let failure = self.cleanup(source.postfix, &slots[..source.position], source.returning)?;
        let mut tail = source.returning;
        if let Some((into, backing, open)) = commitment {
            tail = self.emit(
                source.literal,
                Action::CommitListText {
                    first: slots[0],
                    count,
                    into,
                    backing,
                    provider: source.provider,
                },
                Edges::Next(open),
            )?;
            for position in (source.position + 1..count).rev() {
                self.budget.step(1)?;
                tail = self.expression(
                    stages[position],
                    Some(slots[position]),
                    tail,
                    source.literal_depth + 1,
                )?;
            }
        }
        tail = self.lower_checked_propagation(
            source.postfix,
            slots.get(source.position).copied(),
            tail,
            failure,
            source.postfix_depth,
            source.checked,
        )?;
        for position in (0..source.position).rev() {
            self.budget.step(1)?;
            tail = self.expression(
                stages[position],
                Some(slots[position]),
                tail,
                source.literal_depth + 1,
            )?;
        }
        Ok(tail)
    }
}
