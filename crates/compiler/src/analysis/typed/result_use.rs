//! Result-use analysis consumes typed control and place facts. It does not grant moves.
use super::{Budget, GenericReport, OrdinaryAnalysis, Work};
use crate::analysis::scalars::data::{
    Access, Constructor, DataExpression, PathProjection, PlacePath,
};
use crate::analysis::scalars::{
    BodyOwner, ExpressionFacts, NodeFactKind, NodeFacts, Propagation, Site,
};
use crate::analysis::{
    Binder, DeclarationFacts, DeclarationId, DeclarationShape, MemberType, Payload, Type, TypeTerm,
};
use crate::loader::{DeclarationKind, Module, ModuleKey};
use crate::parser::{NodeId, NodeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod arrows;
mod calls;
mod control;
mod loops;
mod patterns;
mod releases;
mod shape;
mod storage;
mod values;
use loops::LoopExits;
#[cfg(test)]
mod tests;
pub use storage::ResultComponent;
use storage::{Address, Content, Located, Occurrence, Residence, Root};
use values::PlaceStorage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultDiscardKind {
    Expression,
    Replacement,
    ScopeExit,
}

#[derive(Debug)]
pub struct ResultDiscard<'g> {
    pub origin: Site<'g>,
    pub component: Arc<[ResultComponent<'g>]>,
    pub discard: Site<'g>,
    pub kind: ResultDiscardKind,
    /// Binding that contains the occurrence at the discard, if any.
    pub storage: Option<&'g DeclarationId>,
    pub storage_component: Vec<ResultComponent<'g>>,
}
impl ResultDiscard<'_> {
    pub fn code(&self) -> &'static str {
        "R0103"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultUseBlocker {
    TypedInput,
    Containment,
    Storage,
    Control,
    CallBoundary,
}

#[derive(Debug)]
pub struct PendingResultUse<'g> {
    pub site: Site<'g>,
    pub blocker: ResultUseBlocker,
}

#[derive(Debug, Default)]
pub struct ResultUseReport<'g> {
    discards: Vec<ResultDiscard<'g>>,
    pending: Vec<PendingResultUse<'g>>,
}
impl<'g> ResultUseReport<'g> {
    pub fn discards(&self) -> &[ResultDiscard<'g>] {
        &self.discards
    }
    pub fn pending(&self) -> &[PendingResultUse<'g>] {
        &self.pending
    }
}

type Nodes<'a, 'g> = BTreeMap<(&'g ModuleKey, NodeId), &'a NodeFacts<'g>>;
type Alternatives<'g> = Vec<Residence<'g>>;

#[derive(Clone, Copy)]
pub(super) struct Inputs<'a, 'g> {
    pub result: Container<'g>,
    pub option: Option<Container<'g>>,
    pub opaque_cause: Option<&'g DeclarationId>,
    pub owned_bytes: Option<&'g DeclarationId>,
    pub collections: super::CollectionCore<'g>,
    pub native: &'a super::ownership::Report<'g>,
    pub outcome_observer: Option<&'g DeclarationId>,
}

#[derive(Clone, Copy)]
pub(super) struct Container<'g> {
    pub declaration: &'g DeclarationId,
    pub success: &'g DeclarationId,
    pub failure: &'g DeclarationId,
}

pub(super) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    bodies: &GenericReport<'g>,
    core: Inputs<'_, 'g>,
    budget: &mut Budget,
    report: &mut ResultUseReport<'g>,
) -> Work<()> {
    let mut nodes = BTreeMap::new();
    for facts in bodies.typed_nodes() {
        budget.reserve(1)?;
        nodes
            .entry((facts.site.module, facts.site.node))
            .or_insert(facts);
    }
    let mut checked = BTreeSet::new();
    for body in bodies.checked_bodies() {
        budget.step(1)?;
        if let BodyOwner::Declaration(id) = body.owner {
            budget.reserve(1)?;
            checked.insert(id);
        }
    }
    let mut variants = BTreeMap::new();
    for declaration in inventory.type_facts().declarations.values() {
        budget.step(1)?;
        if let DeclarationShape::Enum { variants: members } = &declaration.shape {
            for variant in members {
                budget.reserve(1)?;
                variants.insert(&variant.declaration.id, &variant.payload);
            }
        }
    }
    let mut implementation_headers = BTreeMap::new();
    let mut families = BTreeSet::new();
    for source in inventory.graph().implementations() {
        budget.reserve(1)?;
        if families.insert(&source.family)
            && let Some(family) = inventory.captured_family(&source.family)
        {
            for implementation in &family.implementations {
                budget.step(1)?;
                if implementation.compatibility
                    == crate::analysis::ordinary::Compatibility::Compatible
                {
                    budget.reserve(1)?;
                    implementation_headers
                        .insert(&implementation.source.declaration, &implementation.header);
                }
            }
        }
    }
    let release_parameters = releases::check(
        inventory,
        core,
        &nodes,
        &checked,
        &implementation_headers,
        budget,
    )?;
    for module in inventory.graph().modules() {
        budget.step(1)?;
        let mut scopes = BTreeMap::new();
        for declaration in &module.declarations {
            budget.reserve(1)?;
            scopes.insert(&declaration.id, declaration.scope);
        }
        let mut check = Check {
            inventory,
            nodes: &nodes,
            checked: &checked,
            variants: &variants,
            implementation_headers: &implementation_headers,
            release_parameters: &release_parameters,
            module,
            scopes,
            core,
            budget,
            report,
            loops: Vec::new(),
            match_views: Vec::new(),
            checked_arrows: BTreeSet::new(),
            dead_depth: 0,
            dead_start: None,
        };
        check.roots()?;
    }
    Ok(())
}

struct Check<'a, 'g> {
    inventory: &'a OrdinaryAnalysis<'g>,
    nodes: &'a Nodes<'a, 'g>,
    checked: &'a BTreeSet<&'g DeclarationId>,
    variants: &'a BTreeMap<&'g DeclarationId, &'a Payload<'g>>,
    implementation_headers: &'a BTreeMap<&'g DeclarationId, &'a DeclarationFacts<'g>>,
    release_parameters: &'a releases::Parameters<'g>,
    module: &'g Module,
    scopes: BTreeMap<&'g DeclarationId, Option<usize>>,
    core: Inputs<'a, 'g>,
    budget: &'a mut Budget,
    report: &'a mut ResultUseReport<'g>,
    loops: Vec<LoopExits<'g>>,
    match_views: Vec<(Address<'g>, Root<'g>)>,
    checked_arrows: BTreeSet<NodeId>,
    dead_depth: usize,
    dead_start: Option<usize>,
}

impl<'a, 'g> Check<'a, 'g> {
    fn depth(&mut self, depth: usize) -> Work<()> {
        self.budget.step(1)?;
        if depth > self.budget.limits.max_depth.min(64) {
            return Err(());
        }
        self.budget.statistics.max_depth = self.budget.statistics.max_depth.max(depth);
        Ok(())
    }

    fn site(&self, node: NodeId) -> Site<'g> {
        Site {
            module: &self.module.key,
            node,
            span: &self.module.syntax.nodes[node].span,
        }
    }

    fn facts(&mut self, node: NodeId) -> Work<Option<&'a NodeFacts<'g>>> {
        self.budget.step(1)?;
        Ok(self.nodes.get(&(&self.module.key, node)).copied())
    }

    fn pending(&mut self, node: NodeId, blocker: ResultUseBlocker) -> Work<()> {
        let site = self.site(node);
        self.budget.step(self.report.pending.len())?;
        if !self
            .report
            .pending
            .iter()
            .any(|p| p.site.module == site.module && p.site.node == node && p.blocker == blocker)
        {
            self.budget.reserve(1)?;
            self.report.pending.push(PendingResultUse { site, blocker });
        }
        Ok(())
    }

    fn discard(
        &mut self,
        values: Vec<Located<'g>>,
        root: Root<'g>,
        node: NodeId,
        kind: ResultDiscardKind,
    ) -> Work<()> {
        let storage = match root {
            Root::Local(id) => Some(id),
            Root::Temporary(_) => None,
        };
        let discard = self.site(node);
        if kind == ResultDiscardKind::ScopeExit
            && self.dead_start.is_some_and(|start| match root {
                Root::Local(id) => id.start < start,
                Root::Temporary(node) => self.site(node).span.start() < start,
            })
        {
            return Ok(());
        }
        for value in values {
            self.budget.step(1)?;
            let occurrence = match value.content {
                Content::Result(occurrence) => occurrence,
                Content::Unclassified(origin) => {
                    self.pending(origin, ResultUseBlocker::Containment)?;
                    continue;
                }
                _ => continue,
            };
            let origin = self.site(occurrence.origin);
            let mut duplicate = false;
            for old in &self.report.discards {
                self.budget.step(1)?;
                if old.discard.module == discard.module
                    && old.discard.node == node
                    && old.origin.node == origin.node
                    && old.kind == kind
                    && old.storage == storage
                {
                    self.budget
                        .step(occurrence.component.len() + value.component.len())?;
                    if old.component == occurrence.component
                        && old.storage_component == value.component
                    {
                        duplicate = true;
                        break;
                    }
                }
            }
            if !duplicate {
                self.budget.reserve(1)?;
                self.report.discards.push(ResultDiscard {
                    origin,
                    component: occurrence.component,
                    discard,
                    kind,
                    storage,
                    storage_component: value.component,
                });
            }
        }
        Ok(())
    }

    fn discard_at(
        &mut self,
        mut values: Vec<Located<'g>>,
        (root, prefix): &Address<'g>,
        suffix: &[ResultComponent<'g>],
        site: NodeId,
        kind: ResultDiscardKind,
    ) -> Work<()> {
        for value in &mut values {
            self.budget.reserve(prefix.len() + suffix.len())?;
            self.budget.step(1 + value.component.len())?;
            value
                .component
                .splice(..0, prefix.iter().chain(suffix).copied());
        }
        self.discard(values, *root, site, kind)
    }

    fn singleton(&mut self, state: Residence<'g>) -> Work<Alternatives<'g>> {
        self.budget.reserve(1)?;
        Ok(vec![state])
    }

    fn roots(&mut self) -> Work<()> {
        for declaration in &self.module.declarations {
            self.budget.step(1)?;
            let node = declaration.node;
            let syntax = &self.module.syntax.nodes[node];
            match declaration.id.kind {
                DeclarationKind::Function | DeclarationKind::Implementation => {
                    let Some(&body) = syntax.children.get(1) else {
                        continue;
                    };
                    self.budget.step(1)?;
                    let mut states = self.singleton(Residence::default())?;
                    if let Some(signature) = self
                        .inventory
                        .type_facts()
                        .declarations
                        .get(&declaration.id)
                        .or_else(|| self.implementation_headers.get(&declaration.id).copied())
                        && let DeclarationShape::Function { parameters, .. } = &signature.shape
                    {
                        for parameter in parameters {
                            states = self.seed(
                                states,
                                Root::Local(&parameter.declaration.id),
                                parameter.declaration.node,
                                &parameter.ty,
                            )?;
                        }
                    } else {
                        self.pending(node, ResultUseBlocker::TypedInput)?;
                    }
                    if states.is_empty() {
                        self.dead_statement(body, Residence::default(), 1)?;
                    }
                    for state in states {
                        for state in self.statement(body, state, 1)? {
                            self.exit(state, body)?;
                        }
                    }
                }
                DeclarationKind::Expectation => {
                    let value = syntax.children[0];
                    for mut state in self.expression(value, Residence::default(), 1)? {
                        let root = Root::Temporary(value);
                        let values = state.take(root, &[], self.budget)?;
                        self.discard(values, root, node, ResultDiscardKind::Expression)?;
                    }
                }
                _ => {}
            }
        }
        let node = self.module.syntax.root;
        for state in self.block(node, Residence::default(), 1)? {
            self.exit(state, node)?;
        }
        Ok(())
    }

    fn exit(&mut self, mut state: Residence<'g>, node: NodeId) -> Work<()> {
        for (root, value) in state.take_scope(|_| true, self.budget)? {
            self.budget.reserve(1)?;
            self.discard(vec![value], root, node, ResultDiscardKind::ScopeExit)?;
        }
        Ok(())
    }

    fn dead_statement(&mut self, node: NodeId, state: Residence<'g>, depth: usize) -> Work<()> {
        self.in_dead(node, |check| check.statement(node, state, depth))
            .map(|_| ())
    }

    fn dead_node(&mut self, node: NodeId, state: Residence<'g>, depth: usize) -> Work<()> {
        if self.module.syntax.nodes[node].kind == NodeKind::Block {
            self.dead_statement(node, state, depth)
        } else {
            self.in_dead(node, |check| check.expression(node, state, depth))
                .map(|_| ())
        }
    }

    fn in_dead<T>(&mut self, node: NodeId, check: impl FnOnce(&mut Self) -> Work<T>) -> Work<T> {
        let previous = self.dead_start;
        self.dead_start = previous.or(Some(self.site(node).span.start()));
        self.dead_depth += 1;
        let result = check(self);
        self.dead_depth -= 1;
        self.dead_start = previous;
        result
    }

    fn exit_scope(&mut self, state: &mut Residence<'g>, scope: usize, site: NodeId) -> Work<()> {
        for (root, value) in state.take_scope(|root| matches!(root, Root::Local(id) if self.scopes.get(id).is_some_and(|s| *s == Some(scope))), self.budget)? {
            self.budget.reserve(1)?;
            self.discard(vec![value], root, site, ResultDiscardKind::ScopeExit)?;
        }
        Ok(())
    }

    fn block(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut running = self.singleton(state)?;
        let mut dead: Alternatives<'g> = Vec::new();
        let mut entry = None;
        for &child in &self.module.syntax.nodes[node].children {
            let reachable = entry.is_none() && !running.is_empty();
            let mut lexical = Vec::new();
            let (input, output) = if reachable {
                (std::mem::take(&mut running), &mut running)
            } else {
                let entry = *entry.get_or_insert(child);
                for state in std::mem::take(&mut dead) {
                    let before = state.fork(self.budget)?;
                    let next =
                        self.in_dead(entry, |check| check.statement(child, state, depth + 1))?;
                    if next.is_empty() {
                        lexical.extend(self.singleton(before)?);
                    }
                    dead.extend(next);
                }
                if dead.is_empty() {
                    dead = lexical;
                }
                continue;
            };
            for state in input {
                let before = state.fork(self.budget)?;
                let next = self.statement(child, state, depth + 1)?;
                if next.is_empty() {
                    lexical.extend(self.singleton(before)?);
                }
                output.extend(next);
            }
            if running.is_empty() {
                dead = lexical;
            }
        }
        let scope = self.module.syntax.nodes[node].span.start();
        for state in &mut running {
            self.exit_scope(state, scope, node)?;
        }
        if let Some(entry) = entry {
            self.in_dead(entry, |check| {
                for state in &mut dead {
                    check.exit_scope(state, scope, node)?;
                }
                Ok(())
            })?;
        }
        Ok(running)
    }

    fn statement(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let syntax = &self.module.syntax.nodes[node];
        let children = &syntax.children;
        match syntax.kind {
            NodeKind::Block | NodeKind::Module => self.block(node, state, depth),
            NodeKind::Export
            | NodeKind::Function
            | NodeKind::Implementation
            | NodeKind::Record
            | NodeKind::Enum
            | NodeKind::Expect
            | NodeKind::Import => self.singleton(state),
            NodeKind::Binding { .. } => {
                let source = *children.last().unwrap();
                let alternatives = self.expression(source, state, depth + 1)?;
                let facts = self.facts(node)?;
                let mut running = Vec::new();
                for mut state in alternatives {
                    let root = Root::Temporary(source);
                    if let Some(NodeFactKind::Binding(binding)) = facts.map(|f| &f.kind)
                        && binding.ty.is_some()
                    {
                        self.move_value(
                            &mut state,
                            root,
                            &[],
                            Root::Local(binding.declaration),
                            &[],
                        )?;
                    } else if let Some(NodeFactKind::Pattern(pattern)) =
                        self.facts(children[0])?.map(|f| &f.kind)
                    {
                        self.commit_pattern(&mut state, root, &[], pattern)?;
                        let remaining = state.take(root, &[], self.budget)?;
                        self.discard(remaining, root, node, ResultDiscardKind::Expression)?;
                    } else {
                        state.take(root, &[], self.budget)?;
                        if self.module.syntax.nodes[children[0]].kind != NodeKind::Wildcard {
                            self.pending(node, ResultUseBlocker::Storage)?;
                        }
                    }
                    self.budget.reserve(1)?;
                    running.push(state);
                }
                Ok(running)
            }
            NodeKind::Assignment(_) => {
                let (left, right) = (children[0], children[1]);
                let path = self.facts(left)?.and_then(|f| match &f.kind {
                    NodeFactKind::Expression(e) => e.path.as_ref(),
                    _ => None,
                });
                let checked = self.facts(node)?.is_some();
                let mut alternatives = self.expression(right, state, depth + 1)?;
                for state in &mut alternatives {
                    let incoming = state.take(Root::Temporary(right), &[], self.budget)?;
                    if let Some(target @ (root, path)) = &self.address(state, path)?
                        && checked
                    {
                        let displaced = state.take(*root, path, self.budget)?;
                        self.discard_at(
                            displaced,
                            target,
                            &[],
                            node,
                            ResultDiscardKind::Replacement,
                        )?;
                        state.install(incoming, *root, path, self.budget)?;
                    } else {
                        self.pending(node, ResultUseBlocker::Storage)?;
                        for address in self.place_storage(state, path)?.addresses() {
                            self.forget_address(state, &address, node)?;
                        }
                    }
                }
                Ok(alternatives)
            }
            NodeKind::ExpressionStatement => {
                let value = children[0];
                let root = Root::Temporary(value);
                let mut alternatives = self.expression(value, state, depth + 1)?;
                for state in &mut alternatives {
                    let values = state.take(root, &[], self.budget)?;
                    self.discard(values, root, node, ResultDiscardKind::Expression)?;
                }
                Ok(alternatives)
            }
            NodeKind::Return => {
                let mut alternatives = if let Some(&value) = children.first() {
                    let mut alternatives = self.expression(value, state, depth + 1)?;
                    for state in &mut alternatives {
                        state.take(Root::Temporary(value), &[], self.budget)?;
                    }
                    alternatives
                } else {
                    self.singleton(state)?
                };
                if self.facts(node)?.is_some() {
                    for state in alternatives {
                        self.exit(state, node)?;
                    }
                } else {
                    self.pending(node, ResultUseBlocker::TypedInput)?;
                    alternatives.clear();
                }
                Ok(Vec::new())
            }
            NodeKind::IfStatement => self.conditional(node, state, false, depth + 1),
            NodeKind::Guard => self.guard(node, state, depth + 1),
            NodeKind::While => self.while_loop(node, state, depth + 1),
            NodeKind::For => self.for_loop(node, state, depth + 1),
            NodeKind::Break | NodeKind::Continue => self.loop_jump(node, state),
            _ => {
                self.pending(node, ResultUseBlocker::Control)?;
                self.unsettled(node, depth + 1)
            }
        }
    }

    fn expression(
        &mut self,
        node: NodeId,
        mut state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let Some(facts) = self.facts(node)? else {
            self.pending(node, ResultUseBlocker::TypedInput)?;
            return self.unsettled(node, depth + 1);
        };
        let NodeFactKind::Expression(value) = &facts.kind else {
            self.pending(node, ResultUseBlocker::TypedInput)?;
            return self.unsettled(node, depth + 1);
        };
        let syntax = &self.module.syntax.nodes[node];
        let root = Root::Temporary(node);
        if let Some((from, component)) = self.address(&state, value.path.as_ref())? {
            if matches!(value.ty.as_ref(), TypeTerm::Reference { .. }) {
                if let Some(alias) = state.alias(from, &component, self.budget)? {
                    state.store(root, &[], Content::Alias(alias), self.budget)?;
                } else {
                    for address in state.aliases_under(from, &component, self.budget)? {
                        state.store(root, &[], Content::PossibleAlias(address), self.budget)?;
                    }
                }
                return self.singleton(state);
            }
            if matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool") {
                if let Some(truth) =
                    state.test(from, &component, &Constructor::Boolean(true), self.budget)?
                {
                    self.record_boolean(&mut state, root, &[], truth)?;
                }
                return self.singleton(state);
            }
            if value.access == Access::Owned {
                self.move_value(&mut state, from, &component, root, &[])?;
            } else {
                state.store(root, &[], Content::Alias((from, component)), self.budget)?;
            }
            return self.singleton(state);
        }
        if let Some(data) = &value.data {
            match data {
                DataExpression::Propagation(branch) if syntax.kind == NodeKind::Propagate => {
                    return self.propagate(node, branch, None, state, depth + 1);
                }
                DataExpression::Wrapping(wrap) => {
                    return self.propagate(
                        node,
                        &wrap.branch,
                        Some(wrap.construction.value),
                        state,
                        depth + 1,
                    );
                }
                DataExpression::ErrorConstruction(error)
                | DataExpression::ErrorPipe {
                    construction: error,
                    ..
                } => {
                    let failure = ResultComponent::VariantPayload {
                        variant: error.failure,
                        index: 0,
                    };
                    let repr = error.representation;
                    let payload = [failure, ResultComponent::Field(&repr.payload_field.id)];
                    let retained = [
                        failure,
                        ResultComponent::Field(&repr.retained_field.id),
                        ResultComponent::VariantPayload {
                            variant: &repr.retained_some.id,
                            index: 0,
                        },
                    ];
                    let mut alternatives = self.singleton(state)?;
                    for arg in value.call.as_ref().unwrap().argument_evaluation_order() {
                        let path = if arg == error.value {
                            &payload[..]
                        } else {
                            &retained[..]
                        };
                        alternatives = self.component(alternatives, arg, root, path, depth + 1)?;
                    }
                    if let DataExpression::ErrorPipe { propagation, .. } = data {
                        debug_assert!(propagation.can_return && !propagation.can_continue);
                        for mut state in alternatives {
                            // The newly built failure is immediately forwarded.
                            state.take(root, &[], self.budget)?;
                            self.exit(state, node)?;
                        }
                        return Ok(Vec::new());
                    }
                    for state in &mut alternatives {
                        self.add_source(state, root, node, &[])?;
                        self.budget.reserve(1)?;
                        state.store(
                            root,
                            &[],
                            Content::Test(Arc::new(Constructor::Variant(error.failure)), true),
                            self.budget,
                        )?;
                    }
                    return Ok(alternatives);
                }
                DataExpression::List { elements, .. } => {
                    return self.list_literal(node, elements, state, depth + 1);
                }
                DataExpression::ListIndex { base, selector, .. } => {
                    return self.dynamic_index(node, (*base, *selector), value, state, depth + 1);
                }
                DataExpression::Tuple { elements } => {
                    let mut alternatives = self.singleton(state)?;
                    for (i, &element) in elements.iter().enumerate() {
                        alternatives = self.component(
                            alternatives,
                            element,
                            root,
                            &[ResultComponent::TupleElement(i)],
                            depth + 1,
                        )?;
                    }
                    return Ok(alternatives);
                }
                DataExpression::Construction(construction) => {
                    if let Some(base) = construction.base {
                        return self.rebuild(node, base, construction, state, depth + 1);
                    }
                    let variant = match construction.ty.as_ref() {
                        TypeTerm::Variant { declaration, .. } => Some(*declaration),
                        _ => None,
                    };
                    let mut alternatives = self.singleton(state)?;
                    for field in &construction.fields {
                        alternatives = self.component(
                            alternatives,
                            field.expression,
                            root,
                            &[ResultComponent::Field(&field.field.id)],
                            depth + 1,
                        )?;
                    }
                    for (index, &argument) in construction.arguments.iter().enumerate() {
                        let Some(variant) = variant else {
                            self.pending(node, ResultUseBlocker::Containment)?;
                            break;
                        };
                        alternatives = self.component(
                            alternatives,
                            argument,
                            root,
                            &[ResultComponent::VariantPayload { variant, index }],
                            depth + 1,
                        )?;
                    }
                    for state in &mut alternatives {
                        if self.is_result(&construction.ty) {
                            self.add_source(state, root, node, &[])?;
                        }
                        if let Some(variant) = variant {
                            self.budget.reserve(1)?;
                            state.store(
                                root,
                                &[],
                                Content::Test(Arc::new(Constructor::Variant(variant)), true),
                                self.budget,
                            )?;
                        }
                    }
                    return Ok(alternatives);
                }
                DataExpression::Projection(projection) => {
                    return self.project(
                        node,
                        state,
                        (
                            projection.base,
                            ResultComponent::Field(&projection.field.id),
                        ),
                        value,
                        depth + 1,
                    );
                }
                DataExpression::TupleIndex {
                    base, component, ..
                } => {
                    return self.project(
                        node,
                        state,
                        (*base, ResultComponent::TupleElement(*component)),
                        value,
                        depth + 1,
                    );
                }
                DataExpression::Interpolation(holes) => {
                    return self.interpolate(node, holes, state, depth + 1);
                }
                DataExpression::Match(matched) => {
                    return self.match_expression(node, matched, state, depth + 1);
                }
                _ => {}
            }
        }
        match syntax.kind {
            NodeKind::Arrow => self.arrow(node, facts, state, depth + 1),
            NodeKind::Group => {
                let alternatives = self.singleton(state)?;
                self.component(alternatives, syntax.children[0], root, &[], depth + 1)
            }
            NodeKind::IfExpression => self.conditional(node, state, true, depth + 1),
            NodeKind::Template
                if syntax
                    .children
                    .iter()
                    .all(|&c| self.module.syntax.nodes[c].kind == NodeKind::TemplateText) =>
            {
                self.singleton(state)
            }
            NodeKind::Literal | NodeKind::ValueReference | NodeKind::Prefix("*") => {
                use crate::analysis::scalars::BooleanSet;
                if let Some(truth @ (BooleanSet::TRUE | BooleanSet::FALSE)) = value.boolean {
                    self.record_boolean(&mut state, root, &[], truth == BooleanSet::TRUE)?;
                }
                self.singleton(state)
            }
            NodeKind::Binary("&&" | "||") | NodeKind::Prefix("!") => {
                self.boolean_expression(node, state, depth + 1)
            }
            NodeKind::Binary(_) | NodeKind::Prefix("+" | "-" | "~") => {
                self.primitive_operands(syntax.children.as_slice(), state, depth + 1)
            }
            NodeKind::Prefix("&" | "&mut") => {
                let operand = syntax.children[0];
                let operand_facts = self.expression_facts(operand)?;
                let path = operand_facts.and_then(|e| e.path.as_ref());
                let place = self.place_storage(&state, path)?;
                if let PlaceStorage::Exact(address) = place {
                    state.store(root, &[], Content::Alias(address), self.budget)?;
                    return self.singleton(state);
                }
                let mut alternatives = if matches!(place, PlaceStorage::Dynamic(_))
                    && let Some(DataExpression::ListIndex { base, selector, .. }) =
                        operand_facts.and_then(|e| e.data.as_ref())
                    && let Some(base) = self.expression_facts(*base)?
                    && self.address(&state, base.path.as_ref())?.is_some()
                {
                    // Computing this borrowed address evaluates the selector, not a move from the List.
                    let mut states = self.expression(*selector, state, depth + 1)?;
                    for state in &mut states {
                        state.take(Root::Temporary(*selector), &[], self.budget)?;
                    }
                    states
                } else {
                    self.expression(operand, state, depth + 1)?
                };
                for state in &mut alternatives {
                    if path.is_none() {
                        state.store(
                            root,
                            &[],
                            Content::Alias((Root::Temporary(operand), Vec::new())),
                            self.budget,
                        )?;
                    } else {
                        for address in self.place_storage(state, path)?.addresses() {
                            state.store(root, &[], Content::PossibleAlias(address), self.budget)?;
                        }
                    }
                }
                Ok(alternatives)
            }
            NodeKind::Call | NodeKind::Pipe => self.call(node, value, state, depth + 1),
            _ => {
                self.pending(node, ResultUseBlocker::Control)?;
                let lexical = self.unsettled(node, depth + 1)?;
                Ok(if facts.flow.normal {
                    lexical
                } else {
                    Vec::new()
                })
            }
        }
    }

    fn unsettled(&mut self, node: NodeId, depth: usize) -> Work<Alternatives<'g>> {
        // An unsettled edge cannot settle the exit of any incoming residence.
        // Still check typed islands in the written source as lexical entries.
        self.unsettled_children(node, depth)?;
        self.singleton(Residence::default())
    }

    fn unsettled_children(&mut self, node: NodeId, depth: usize) -> Work<()> {
        self.depth(depth)?;
        for &child in &self.module.syntax.nodes[node].children {
            if self.module.syntax.nodes[child].kind == NodeKind::Block {
                self.dead_statement(child, Residence::default(), depth + 1)?;
            } else if self.expression_facts(child)?.is_some() {
                self.dead_node(child, Residence::default(), depth + 1)?;
            } else {
                self.unsettled_children(child, depth + 1)?;
            }
        }
        Ok(())
    }

    fn propagate(
        &mut self,
        node: NodeId,
        branch: &Propagation<'g>,
        outer: Option<NodeId>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let operand = self.module.syntax.nodes[node].children[0];
        let source = Root::Temporary(operand);
        let mut normal = Vec::new();
        for state in self.expression(operand, state, depth)? {
            let mut failed = Vec::new();
            if let Some(mut success) =
                self.split_variant(state, source, branch.success, &mut failed, depth)?
                && branch.can_continue
            {
                self.move_value(
                    &mut success,
                    source,
                    &[ResultComponent::VariantPayload {
                        variant: branch.success,
                        index: 0,
                    }],
                    Root::Temporary(node),
                    &[],
                )?;
                success.take(source, &[], self.budget)?;
                normal.extend(self.singleton(success)?);
            }
            if branch.can_return {
                for mut returned in failed {
                    // The existing failure wrapper is forwarded; unrelated storage leaves scope.
                    returned.take(source, &[], self.budget)?;
                    if let Some(outer) = outer {
                        for mut completed in self.expression(outer, returned, depth + 1)? {
                            completed.take(Root::Temporary(outer), &[], self.budget)?;
                            self.exit(completed, node)?;
                        }
                    } else {
                        self.exit(returned, node)?;
                    }
                }
            }
        }
        Ok(normal)
    }

    fn component(
        &mut self,
        alternatives: Alternatives<'g>,
        child: NodeId,
        root: Root<'g>,
        path: &[ResultComponent<'g>],
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut next = Vec::new();
        for state in alternatives {
            let mut evaluated = self.expression(child, state, depth)?;
            for state in &mut evaluated {
                self.move_value(state, Root::Temporary(child), &[], root, path)?;
            }
            next.extend(evaluated);
        }
        Ok(next)
    }

    fn move_value(
        &mut self,
        state: &mut Residence<'g>,
        from: Root<'g>,
        prefix: &[ResultComponent<'g>],
        to: Root<'g>,
        target: &[ResultComponent<'g>],
    ) -> Work<()> {
        let values = state.take(from, prefix, self.budget)?;
        state.install(values, to, target, self.budget)
    }

    fn is_result(&self, ty: &Type<'g>) -> bool {
        let ty = if let TypeTerm::Variant { parent, .. } = ty.as_ref() {
            parent
        } else {
            ty
        };
        matches!(ty.as_ref(), TypeTerm::Nominal { declaration, .. } if *declaration == self.core.result.declaration)
    }

    fn add_source(
        &mut self,
        state: &mut Residence<'g>,
        root: Root<'g>,
        origin: NodeId,
        path: &[ResultComponent<'g>],
    ) -> Work<()> {
        self.budget.reserve(1 + path.len())?;
        state.place(
            Occurrence {
                origin,
                component: Arc::from(path),
            },
            root,
            path,
            self.budget,
        )
    }
}
