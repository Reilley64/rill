//! Expression evaluation retains original residences until transfer commits.
use super::*;

pub(super) fn ungroup(module: &Module, mut node: NodeId, budget: &mut Budget) -> Work<NodeId> {
    loop {
        budget.step(1)?;
        let syntax = &module.syntax.nodes[node];
        if syntax.kind != NodeKind::Group {
            return Ok(node);
        }
        node = syntax.children[0];
    }
}
use crate::analysis::scalars::data::Construction;
use storage::BooleanValue;

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
    pub(super) fn read_place(
        &mut self,
        node: NodeId,
        value: &ExpressionFacts<'g>,
        state: &mut Residence<'g>,
        (from, component): Address<'g>,
        usage: ValueUse,
    ) -> Work<()> {
        let root = Root::Temporary(node);
        if matches!(value.ty.as_ref(), TypeTerm::Reference { .. }) {
            if let Some(alias) = state.alias(from, &component, self.budget)? {
                state.store(root, &[], Content::Alias(alias), self.budget)?;
            } else {
                // Track the external origin, not another ownership permission.
                if component.is_empty()
                    && retained_bool_contents_type(&value.ty).is_some()
                    && let Some(parameter) = state.caller_bool_origin(from, self.budget)?
                {
                    state.store(root, &[], Content::CallerBoolOrigin(parameter), self.budget)?;
                }
                for alias in state.aliases_under(from, &component, self.budget)? {
                    state.store(root, &[], Content::PossibleAlias(alias), self.budget)?;
                }
            }
        } else if let TypeTerm::Primitive(p) = value.ty.as_ref() {
            if p.name() == "bool" {
                if let Some(value) = state.boolean_value(from, &component, self.budget)? {
                    self.install_boolean(state, root, value)?;
                } else if matches!(usage, ValueUse::Scrutinee) {
                    // A computed Boolean scrutinee reads its source through an alias. Guards
                    // that read the source must see the same value.
                    let copy = state.new_boolean_copy(from, &component, self.budget)?;
                    self.install_boolean(state, root, BooleanValue::Copy(copy))?;
                } else if !self.wildcard_sources.is_empty() && !self.discarded_value(usage)? {
                    let mut copied = false;
                    for ((source, path), _) in self.wildcard_sources.iter().rev() {
                        self.budget.step(1 + component.len())?;
                        if from == *source && component == *path {
                            let copy = state.new_boolean_copy(from, &component, self.budget)?;
                            self.install_boolean(state, root, BooleanValue::Copy(copy))?;
                            copied = true;
                            break;
                        }
                    }
                    if !copied
                        && let Some(source) = self.parameter_copy_source(value, from, &component)?
                    {
                        match state.boolean_value(source.0, &source.1, self.budget)? {
                            Some(value) => self.install_boolean(state, root, value)?,
                            None => {
                                let copy =
                                    state.new_boolean_copy(source.0, &source.1, self.budget)?;
                                self.install_boolean(state, root, BooleanValue::Copy(copy))?;
                            }
                        }
                    }
                }
            }
        } else if value.access == Access::Owned {
            self.move_value(state, from, &component, root, &[])?;
        } else {
            state.store(root, &[], Content::Alias((from, component)), self.budget)?;
        }
        Ok(())
    }

    fn parameter_copy_source(
        &mut self,
        value: &ExpressionFacts<'g>,
        from: Root<'g>,
        component: &[ResultComponent<'g>],
    ) -> Work<Option<Address<'g>>> {
        let Root::Local(id) = from else {
            return Ok(None);
        };
        let read = value
            .binding
            .is_some_and(|binding| std::ptr::eq(binding, id));
        if !component.is_empty()
            || id.kind != DeclarationKind::Binding
            || (read && (value.written_before || matches!(value.copy_of, CopySource::Not)))
            || !self.wildcard_sources.iter().any(|((root, path), written)| {
                path.is_empty()
                    && matches!(root, Root::Local(p) if match p.kind {
                        DeclarationKind::Parameter => true,
                        DeclarationKind::Binding => !written,
                        _ => false,
                    } && match value.copy_of {
                        CopySource::Of(copy) if read => std::ptr::eq(copy, *p),
                        _ => true,
                    })
            })
        {
            return Ok(None);
        }
        let Some((name, parent)) = self.written_binding(id)? else {
            return Ok(None);
        };
        let nodes = &self.module.syntax.nodes;
        let span = &nodes[name].span;
        let NodeKind::Binding { mutable } = nodes[parent].kind else {
            return Ok(None);
        };
        let (pattern, init) = (
            nodes[parent].children[0],
            *nodes[parent].children.last().unwrap(),
        );
        let mut pattern = pattern;
        while nodes[pattern].kind == NodeKind::GroupPattern {
            self.budget.step(1)?;
            pattern = nodes[pattern].children[0];
        }
        if !matches!(
            nodes[pattern].kind,
            NodeKind::PatternPath | NodeKind::NamePattern
        ) || nodes[pattern].span != *span
        {
            return Ok(None);
        }
        let init = self.ungroup_value(init)?;
        if nodes[init].kind != NodeKind::ValueReference {
            return Ok(None);
        }
        let Some(read) = self.expression_facts(init)? else {
            return Ok(None);
        };
        let Some(path) = read
            .path
            .as_ref()
            .filter(|path| path.projections.is_empty())
        else {
            return Ok(None);
        };
        if !matches!(read.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool") {
            return Ok(None);
        }
        let mut found = None;
        for ((source, path_at), _) in &self.wildcard_sources {
            self.budget.step(1)?;
            if path_at.is_empty() && matches!(source, Root::Local(p) if std::ptr::eq(*p, path.root))
            {
                found = Some(*source);
                break;
            }
        }
        let Some(Root::Local(source)) = found else {
            return Ok(None);
        };
        if mutable && !self.never_written(id, parent)? {
            return Ok(None);
        }
        if source.kind == DeclarationKind::Binding {
            let Some((_, binding)) = self.written_binding(source)? else {
                return Ok(None);
            };
            match self.module.syntax.nodes[binding].kind {
                NodeKind::Binding { mutable: false } => {}
                NodeKind::Binding { mutable: true } if self.never_written(source, binding)? => {}
                _ => return Ok(None),
            }
        }
        Ok(Some((Root::Local(source), Vec::new())))
    }

    /// The name node of the local `id` and the node that binds it, found on demand.
    fn written_binding(&mut self, id: &DeclarationId) -> Work<Option<(NodeId, NodeId)>> {
        let nodes = &self.module.syntax.nodes;
        let declarations = &self.module.declarations;
        self.budget
            .step(1 + declarations.len().checked_ilog2().unwrap_or(0) as usize)?;
        let mut name = None;
        for declaration in &declarations[declarations.partition_point(|d| d.id.start < id.start)..]
        {
            self.budget.step(1)?;
            if declaration.id.start != id.start {
                break;
            }
            if declaration.id.kind == DeclarationKind::Binding {
                name = Some(declaration.node);
                break;
            }
        }
        let Some(name) = name else {
            return Ok(None);
        };
        // Nodes follow their children, so the first later node that spans the name and is
        // not part of its pattern is the binding.
        let span = &nodes[name].span;
        let mut parent = name + 1;
        loop {
            self.budget.step(1)?;
            let Some(node) = nodes.get(parent) else {
                return Ok(None);
            };
            if node.span.start() <= span.start()
                && span.end() <= node.span.end()
                && !matches!(
                    node.kind,
                    NodeKind::QualifiedName
                        | NodeKind::PatternPath
                        | NodeKind::NamePattern
                        | NodeKind::GroupPattern
                )
            {
                break;
            }
            parent += 1;
        }
        Ok(Some((name, parent)))
    }

    /// Whether no assignment and no `&mut` in the scope of the mutable local `id`, bound
    /// at `binding`, names it. A scalar capture copies, so an arrow that only reads the
    /// local needs no screen of its own; an assignment or `&mut` inside one is still seen.
    /// A node whose place is unknown counts as a write.
    fn never_written(&mut self, id: &DeclarationId, binding: NodeId) -> Work<bool> {
        let nodes = &self.module.syntax.nodes;
        let span = &nodes[binding].span;
        for node in nodes.iter().skip(binding + 1) {
            self.budget.step(1)?;
            let written = match node.kind {
                NodeKind::Block
                    if node.span.start() <= span.start() && span.end() <= node.span.end() =>
                {
                    return Ok(true);
                }
                NodeKind::Assignment(_) => node.children[0],
                NodeKind::Prefix("&mut") => node.children[0],
                _ => continue,
            };
            let place = self.ungroup_value(written)?;
            let Some(facts) = self.expression_facts(place)? else {
                return Ok(false);
            };
            if facts
                .path
                .as_ref()
                .is_none_or(|path| std::ptr::eq(path.root, id))
            {
                return Ok(false);
            }
        }
        Ok(false)
    }

    pub(super) fn discarded_value(&mut self, usage: ValueUse) -> Work<bool> {
        match usage {
            ValueUse::Used | ValueUse::Scrutinee => Ok(false),
            ValueUse::Statement => Ok(true),
            ValueUse::Binding { pattern, depth } => match self.module.syntax.nodes[pattern].kind {
                NodeKind::Wildcard => Ok(true),
                NodeKind::GroupPattern => self.wildcard_at(pattern, depth),
                _ => Ok(false),
            },
        }
    }

    pub(super) fn dereference(
        &mut self,
        node: NodeId,
        operand: NodeId,
        value: &ExpressionFacts<'g>,
        state: Residence<'g>,
        depth: usize,
        usage: ValueUse,
    ) -> Work<Alternatives<'g>> {
        let mut states = self.expression(operand, state, depth)?;
        let descriptor = Root::Temporary(operand);
        let scalar = matches!(value.ty.as_ref(), TypeTerm::Primitive(_));
        for state in &mut states {
            let aliases = state.aliases_under(descriptor, &[], self.budget)?;
            if let Some(address) = state.alias(descriptor, &[], self.budget)? {
                self.read_place(node, value, state, address, usage)?;
            } else if !scalar && !aliases.is_empty() {
                // A borrowed parameter with no tracked local referent does
                // not own a local occurrence. Possible local aliases do matter.
                self.pending(node, ResultUseBlocker::Storage)?;
                for address in &aliases {
                    self.budget.reserve(1 + address.1.len())?;
                    state.store(
                        Root::Temporary(node),
                        &[],
                        Content::PossibleAlias(address.clone()),
                        self.budget,
                    )?;
                }
            }
            state.take(descriptor, &[], self.budget)?;
            if scalar {
                for (referent, _) in aliases {
                    self.budget.step(1)?;
                    // A scalar read cannot carry an operand-local source.
                    if let Root::Temporary(origin) = referent {
                        let span = self.site(operand).span;
                        if (span.start()..span.end()).contains(&self.site(origin).span.start()) {
                            let discarded = state.take(referent, &[], self.budget)?;
                            self.discard(discarded, referent, node, ResultDiscardKind::Expression)?;
                        }
                    }
                }
            }
        }
        Ok(states)
    }

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

    pub(super) fn redirect_view(
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
        // Invalidating a location can discard a hint without losing a
        // tracked residence or handing a later store an incoming origin.
        self.budget.step(affected.len())?;
        if affected.iter().any(|entry| match &entry.content {
            Content::BooleanCopy(_) | Content::CallerBoolOrigin(_) | Content::ConsumedPair => false,
            Content::Test(tag, _) if matches!(tag.as_ref(), Constructor::Boolean(_)) => {
                // A Boolean fact at a named field describes that scalar only.
                // Keep the existing storage boundary for nested tuple elements.
                !matches!(
                    entry.component.last().or_else(|| path.last()),
                    Some(ResultComponent::Field(_))
                )
            }
            _ => true,
        }) {
            self.pending(site, ResultUseBlocker::Storage)?;
        }
        Ok(())
    }

    pub(super) fn value_root_at(
        &mut self,
        mut node: NodeId,
        mut depth: usize,
    ) -> Work<(NodeId, usize)> {
        loop {
            self.depth(depth)?;
            let syntax = &self.module.syntax.nodes[node];
            if syntax.kind != NodeKind::Group {
                return Ok((node, depth));
            }
            node = syntax.children[0];
            depth += 1;
        }
    }

    pub(super) fn ungroup_value(&mut self, node: NodeId) -> Work<NodeId> {
        ungroup(self.module, node, self.budget)
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
