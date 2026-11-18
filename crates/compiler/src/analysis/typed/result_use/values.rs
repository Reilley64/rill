//! Expression evaluation retains original residences until transfer commits.
use super::*;
use crate::analysis::scalars::data::Construction;

pub(super) enum PlaceStorage<'g> {
    Exact(Address<'g>),
    Dynamic(Address<'g>),
    Indirect(Vec<Address<'g>>),
}
impl<'g> PlaceStorage<'g> {
    pub(super) fn addresses(self) -> impl Iterator<Item = Address<'g>> {
        let (one, many) = match self {
            Self::Exact(address) | Self::Dynamic(address) => (Some(address), Vec::new()),
            Self::Indirect(addresses) => (None, addresses),
        };
        one.into_iter().chain(many)
    }
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn address(
        &mut self,
        state: &Residence<'g>,
        path: Option<&PlacePath<'g>>,
    ) -> Work<Option<Address<'g>>> {
        Ok(match self.place_storage(state, path)? {
            PlaceStorage::Exact(address) => Some(address),
            _ => None,
        })
    }

    pub(super) fn place_storage(
        &mut self,
        state: &Residence<'g>,
        path: Option<&PlacePath<'g>>,
    ) -> Work<PlaceStorage<'g>> {
        let Some(path) = path else {
            return Ok(PlaceStorage::Indirect(Vec::new()));
        };
        self.budget.reserve(path.projections.len())?;
        let mut root = Root::Local(path.root);
        let mut component = Vec::with_capacity(path.projections.len());
        for projection in &path.projections {
            component.push(match projection {
                PathProjection::Field(field) => ResultComponent::Field(field),
                PathProjection::TupleElement(i) => ResultComponent::TupleElement(*i),
                PathProjection::Dereference => {
                    self.redirect_view(&mut root, &mut component)?;
                    let Some(alias) = state.alias(root, &component, self.budget)? else {
                        return Ok(PlaceStorage::Indirect(state.aliases_under(
                            root,
                            &component,
                            self.budget,
                        )?));
                    };
                    (root, component) = alias;
                    continue;
                }
                PathProjection::ListElement => {
                    self.redirect_view(&mut root, &mut component)?;
                    return Ok(PlaceStorage::Dynamic((root, component)));
                }
            });
        }
        self.redirect_view(&mut root, &mut component)?;
        Ok(PlaceStorage::Exact((root, component)))
    }

    fn redirect_view(
        &mut self,
        root: &mut Root<'g>,
        component: &mut Vec<ResultComponent<'g>>,
    ) -> Work<()> {
        for ((source, prefix), target) in &self.match_views {
            self.budget.step(1 + prefix.len())?;
            if *root == *source && component.starts_with(prefix) {
                self.budget.step(component.len())?;
                component.drain(..prefix.len());
                *root = *target;
            }
        }
        Ok(())
    }

    pub(super) fn list_literal(
        &mut self,
        node: NodeId,
        elements: &[NodeId],
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut states = self.singleton(state)?;
        for (index, &element) in elements.iter().enumerate() {
            states = self.component(
                states,
                element,
                Root::Temporary(node),
                &[ResultComponent::ListElement(index)],
                depth,
            )?;
        }
        Ok(states)
    }

    pub(super) fn dynamic_index(
        &mut self,
        node: NodeId,
        (base, selector): (NodeId, NodeId),
        value: &ExpressionFacts<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let owned = value.access == Access::Owned
            && !matches!(
                value.ty.as_ref(),
                TypeTerm::Primitive(_) | TypeTerm::Reference { .. }
            );
        if owned {
            self.pending(node, ResultUseBlocker::Storage)?;
        }
        let place = self
            .expression_facts(base)?
            .and_then(|value| value.path.as_ref());
        let sources = if self.address(&state, place)?.is_some() {
            self.singleton(state)?
        } else {
            self.expression(base, state, depth)?
        };
        let mut result = Vec::new();
        for state in sources {
            for mut state in self.expression(selector, state, depth)? {
                if owned {
                    for address in self.place_storage(&state, value.path.as_ref())?.addresses() {
                        self.forget_address(&mut state, &address, node)?;
                    }
                }
                state.take(Root::Temporary(selector), &[], self.budget)?;
                let removed = state.take(Root::Temporary(base), &[], self.budget)?;
                if !owned {
                    self.budget.step(removed.len())?;
                    if removed.iter().any(|entry| {
                        matches!(entry.content, Content::Result(_) | Content::Unclassified(_))
                    }) {
                        self.pending(node, ResultUseBlocker::Storage)?;
                    }
                }
                result.extend(self.singleton(state)?);
            }
        }
        Ok(result)
    }

    pub(super) fn forget_address(
        &mut self,
        state: &mut Residence<'g>,
        (root, path): &Address<'g>,
        site: NodeId,
    ) -> Work<()> {
        let affected = state.take(*root, path, self.budget)?;
        if !affected.is_empty() {
            self.pending(site, ResultUseBlocker::Storage)?;
        }
        Ok(())
    }

    pub(super) fn expression_facts(
        &mut self,
        node: NodeId,
    ) -> Work<Option<&'a ExpressionFacts<'g>>> {
        Ok(self.facts(node)?.and_then(|fact| match &fact.kind {
            NodeFactKind::Expression(value) => Some(value),
            _ => None,
        }))
    }

    pub(super) fn interpolate(
        &mut self,
        node: NodeId,
        holes: &[crate::analysis::scalars::FormattingCall<'g>],
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut states = self.singleton(state)?;
        for hole in holes {
            let root = Root::Temporary(hole.expression);
            let mut next = Vec::new();
            for state in states {
                for mut state in self.expression(hole.expression, state, depth)? {
                    let written = state.take(root, &[], self.budget)?;
                    self.discard(written, root, node, ResultDiscardKind::Expression)?;
                    next.extend(self.singleton(state)?);
                }
            }
            states = next;
        }
        Ok(states)
    }

    pub(super) fn rebuild(
        &mut self,
        node: NodeId,
        base: NodeId,
        rebuild: &Construction<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let place = self
            .expression_facts(base)?
            .and_then(|value| value.path.as_ref());
        let (address, mut states) = if let Some(address) = self.address(&state, place)? {
            // The old place is readable until installation of the replacements.
            (address, self.singleton(state)?)
        } else {
            (
                (Root::Temporary(base), Vec::new()),
                self.expression(base, state, depth + 1)?,
            )
        };
        let root = Root::Temporary(node);
        for field in &rebuild.fields {
            states = self.component(
                states,
                field.expression,
                root,
                &[ResultComponent::Field(&field.field.id)],
                depth + 1,
            )?;
        }
        for state in &mut states {
            self.budget.reserve(address.1.len() + 1)?;
            let mut path = address.1.clone();
            for field in &rebuild.fields {
                path.push(ResultComponent::Field(&field.field.id));
                let displaced = state.take(address.0, &path, self.budget)?;
                self.discard_at(
                    displaced,
                    &address,
                    &path[address.1.len()..],
                    field.expression,
                    ResultDiscardKind::Replacement,
                )?;
                path.pop();
            }
            self.move_value(state, address.0, &address.1, root, &[])?;
        }
        Ok(states)
    }

    pub(super) fn project(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        (base, component): (NodeId, ResultComponent<'g>),
        value: &ExpressionFacts<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let root = Root::Temporary(base);
        let mut alternatives = self.expression(base, state, depth)?;
        for state in &mut alternatives {
            let (owner, mut path) = state
                .alias(root, &[], self.budget)?
                .unwrap_or((root, Vec::new()));
            self.budget.reserve(1)?;
            path.push(component);
            if matches!(value.ty.as_ref(), TypeTerm::Reference { .. }) {
                if let Some(address) = state.alias(owner, &path, self.budget)? {
                    state.store(
                        Root::Temporary(node),
                        &[],
                        Content::Alias(address),
                        self.budget,
                    )?;
                }
            } else if value.access == Access::Owned && owner == root {
                self.move_value(state, owner, &path, Root::Temporary(node), &[])?;
            }
            let unselected = state.take(root, &[], self.budget)?;
            self.discard(unselected, root, node, ResultDiscardKind::Expression)?;
        }
        Ok(alternatives)
    }
}
