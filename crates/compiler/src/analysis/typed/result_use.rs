//! Result-use analysis consumes typed control and place facts. It does not grant moves.
use super::{Budget, GenericReport, OrdinaryAnalysis, Work};
use crate::analysis::scalars::data::{Access, DataExpression, PathProjection, PlacePath};
use crate::analysis::scalars::{
    BodyOwner, ExpressionFacts, NodeFactKind, NodeFacts, Propagation, Site,
};
use crate::analysis::{
    Binder, DeclarationId, DeclarationShape, MemberType, Payload, Type, TypeTerm,
};
use crate::loader::{DeclarationKind, Module, ModuleKey};
use crate::parser::{NodeId, NodeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod loops;
mod storage;
use loops::LoopExits;
#[cfg(test)]
mod tests;
pub use storage::ResultComponent;
use storage::{Located, Occurrence, Residence, Root};

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
pub(super) struct Inputs<'g> {
    pub result: &'g DeclarationId,
    pub opaque_cause: Option<&'g DeclarationId>,
    pub opaque_list: Option<&'g DeclarationId>,
    pub outcome_observer: Option<&'g DeclarationId>,
}

pub(super) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    bodies: &GenericReport<'g>,
    core: Inputs<'g>,
    budget: &mut Budget,
    report: &mut ResultUseReport<'g>,
) -> Work<()> {
    let mut nodes = BTreeMap::new();
    let mut immutable_bindings = BTreeMap::new();
    for facts in bodies.typed_nodes() {
        if let NodeFactKind::Binding(binding) = &facts.kind
            && binding.ty.is_some()
            && !binding.mutable
        {
            budget.reserve(1)?;
            immutable_bindings.insert(binding.declaration, facts.site.node);
        }
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
            module,
            scopes,
            core,
            immutable_bindings: &immutable_bindings,
            budget,
            report,
            loops: Vec::new(),
            dead_depth: 0,
        };
        check.roots()?;
    }
    Ok(())
}

struct Arguments<'a, 'g> {
    binders: &'a [Binder<'g>],
    types: &'a [Type<'g>],
    outer: Option<&'a Arguments<'a, 'g>>,
}

struct Check<'a, 'g> {
    inventory: &'a OrdinaryAnalysis<'g>,
    nodes: &'a Nodes<'a, 'g>,
    checked: &'a BTreeSet<&'g DeclarationId>,
    module: &'g Module,
    scopes: BTreeMap<&'g DeclarationId, Option<usize>>,
    core: Inputs<'g>,
    immutable_bindings: &'a BTreeMap<&'g DeclarationId, NodeId>,
    budget: &'a mut Budget,
    report: &'a mut ResultUseReport<'g>,
    loops: Vec<LoopExits<'g>>,
    dead_depth: usize,
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
        for value in values {
            let origin = self.site(value.occurrence.origin);
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
                        .step(value.occurrence.component.len() + value.component.len())?;
                    if old.component == value.occurrence.component
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
                    component: value.occurrence.component,
                    discard,
                    kind,
                    storage,
                    storage_component: value.component,
                });
            }
        }
        Ok(())
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
                    let mut state = Residence::default();
                    if let Some(signature) = self
                        .inventory
                        .type_facts()
                        .declarations
                        .get(&declaration.id)
                        && let DeclarationShape::Function { parameters, .. } = &signature.shape
                    {
                        for parameter in parameters {
                            self.seed(
                                &mut state,
                                Root::Local(&parameter.declaration.id),
                                parameter.declaration.node,
                                &parameter.ty,
                            )?;
                        }
                    } else {
                        self.pending(node, ResultUseBlocker::TypedInput)?;
                    }
                    for state in self.statement(body, state, 1)? {
                        self.exit(state, body)?;
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

    fn block(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let mut running = self.singleton(state)?;
        let mut dead = Vec::new();
        for &child in &self.module.syntax.nodes[node].children {
            let reachable = !running.is_empty();
            let alternatives = if reachable {
                std::mem::take(&mut running)
            } else if dead.is_empty() {
                self.singleton(Residence::default())?
            } else {
                std::mem::take(&mut dead)
            };
            for state in alternatives {
                if reachable {
                    running.extend(self.statement(child, state, depth + 1)?);
                } else {
                    self.dead_depth += 1;
                    let next = self.statement(child, state, depth + 1);
                    self.dead_depth -= 1;
                    dead.extend(next?);
                }
            }
        }
        let scope = self.module.syntax.nodes[node].span.start();
        for state in running.iter_mut().chain(dead.iter_mut()) {
            for (root, value) in state.take_scope(|root| matches!(root, Root::Local(id) if self.scopes.get(id).is_some_and(|s| *s == Some(scope))), self.budget)? {
                self.budget.reserve(1)?;
                self.discard(vec![value], root, node, ResultDiscardKind::ScopeExit)?;
            }
        }
        Ok(running)
    }

    fn statement(
        &mut self,
        node: NodeId,
        mut state: Residence<'g>,
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
                    let values = state.take(Root::Temporary(source), &[], self.budget)?;
                    if let Some(NodeFactKind::Binding(binding)) = facts.map(|f| &f.kind)
                        && binding.ty.is_some()
                    {
                        state.install(
                            values,
                            Root::Local(binding.declaration),
                            &[],
                            self.budget,
                        )?;
                    } else if self.module.syntax.nodes[children[0]].kind != NodeKind::Wildcard {
                        self.pending(node, ResultUseBlocker::Storage)?;
                    }
                    self.budget.reserve(1)?;
                    running.push(state);
                }
                Ok(running)
            }
            NodeKind::Assignment("=") => {
                let (left, right) = (children[0], children[1]);
                let path = self.facts(left)?.and_then(|f| match &f.kind {
                    NodeFactKind::Expression(e) => e.path.as_ref(),
                    _ => None,
                });
                let target = self.exact_path(path)?;
                let checked = self.facts(node)?.is_some();
                let mut alternatives = self.expression(right, state, depth + 1)?;
                for state in &mut alternatives {
                    let incoming = state.take(Root::Temporary(right), &[], self.budget)?;
                    if let Some((root, path)) = &target
                        && checked
                    {
                        let displaced = state.take(*root, path, self.budget)?;
                        self.discard(displaced, *root, node, ResultDiscardKind::Replacement)?;
                        state.install(incoming, *root, path, self.budget)?;
                    } else {
                        self.pending(node, ResultUseBlocker::Storage)?;
                        *state = Residence::default();
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
            NodeKind::While => self.while_loop(node, state, depth + 1),
            NodeKind::Break | NodeKind::Continue => self.loop_jump(node, state),
            _ => {
                self.pending(node, ResultUseBlocker::Control)?;
                state = Residence::default();
                self.singleton(state)
            }
        }
    }

    fn conditional(
        &mut self,
        node: NodeId,
        state: Residence<'g>,
        expression: bool,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let children = &self.module.syntax.nodes[node].children;
        let [condition, yes, rest @ ..] = children.as_slice() else {
            unreachable!()
        };
        let Some(NodeFactKind::Expression(fact)) = self.facts(*condition)?.map(|f| &f.kind) else {
            self.pending(node, ResultUseBlocker::Control)?;
            return self.singleton(Residence::default());
        };
        let truth = fact
            .boolean
            .unwrap_or(crate::analysis::scalars::BooleanSet::BOTH);
        let mut result = Vec::new();
        for state in self.expression(*condition, state, depth + 1)? {
            for (reachable, child) in [
                (truth.can_be_true(), Some(*yes)),
                (truth.can_be_false(), rest.first().copied()),
            ] {
                if !reachable {
                    if let Some(child) = child {
                        self.dead_depth += 1;
                        let checked = if expression {
                            self.expression(child, Residence::default(), depth + 1)
                        } else {
                            self.statement(child, Residence::default(), depth + 1)
                        };
                        self.dead_depth -= 1;
                        checked?;
                    }
                    continue;
                }
                let alternative = state.fork(self.budget)?;
                let Some(child) = child else {
                    result.extend(self.singleton(alternative)?);
                    continue;
                };
                if expression {
                    let mut next = self.expression(child, alternative, depth + 1)?;
                    for state in &mut next {
                        self.move_value(
                            state,
                            Root::Temporary(child),
                            &[],
                            Root::Temporary(node),
                            &[],
                        )?;
                    }
                    result.extend(next);
                } else {
                    result.extend(self.statement(child, alternative, depth + 1)?);
                }
            }
        }
        Ok(result)
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
            return self.singleton(Residence::default());
        };
        let NodeFactKind::Expression(value) = &facts.kind else {
            self.pending(node, ResultUseBlocker::TypedInput)?;
            return self.singleton(Residence::default());
        };
        let syntax = &self.module.syntax.nodes[node];
        let root = Root::Temporary(node);
        if value.access == Access::Owned
            && let Some((from, component)) = self.exact_path(value.path.as_ref())?
        {
            self.move_value(&mut state, from, &component, root, &[])?;
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
                DataExpression::ErrorConstruction(error) => {
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
                    for state in &mut alternatives {
                        self.add_source(state, root, node, &[])?;
                    }
                    return Ok(alternatives);
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
                DataExpression::Construction(construction) if construction.base.is_none() => {
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
                    if self.is_result(&value.ty) {
                        for state in &mut alternatives {
                            self.add_source(state, root, node, &[])?;
                        }
                    }
                    return Ok(alternatives);
                }
                DataExpression::Match(matched)
                    if self.is_result(
                        &matched
                            .arms
                            .first()
                            .map(|arm| arm.pattern.ty.clone())
                            .unwrap_or_else(|| value.ty.clone()),
                    ) && matched.arms.iter().all(|arm| arm.guard.is_none()) =>
                {
                    return self.match_result(node, matched, state, depth + 1);
                }
                _ => {}
            }
        }
        match syntax.kind {
            NodeKind::Group => {
                let alternatives = self.singleton(state)?;
                self.component(alternatives, syntax.children[0], root, &[], depth + 1)
            }
            NodeKind::IfExpression => self.conditional(node, state, true, depth + 1),
            NodeKind::Literal | NodeKind::ValueReference => self.singleton(state),
            NodeKind::Prefix("&" | "&mut") => {
                let operand = syntax.children[0];
                if self.facts(operand)?.is_some_and(
                    |f| matches!(&f.kind, NodeFactKind::Expression(e) if e.path.is_some()),
                ) {
                    return self.singleton(state);
                }
                let root = Root::Temporary(operand);
                let mut alternatives = self.expression(operand, state, depth + 1)?;
                for state in &mut alternatives {
                    let values = state.take(root, &[], self.budget)?;
                    self.discard(values, root, node, ResultDiscardKind::Expression)?;
                }
                Ok(alternatives)
            }
            NodeKind::Call | NodeKind::Pipe => self.call(node, value, state, depth + 1),
            _ => {
                if state.has_values()
                    || !matches!(
                        value.ty.as_ref(),
                        TypeTerm::Primitive(_) | TypeTerm::Reference { .. }
                    )
                {
                    self.pending(node, ResultUseBlocker::Control)?;
                    state = Residence::default();
                }
                if facts.flow.normal {
                    self.singleton(state)
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    fn inspect_argument(
        &mut self,
        mut state: Residence<'g>,
        node: NodeId,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let syntax = &self.module.syntax.nodes[node];
        if matches!(syntax.kind, NodeKind::Prefix("&" | "&mut")) {
            let operand = syntax.children[0];
            if let Some(NodeFactKind::Expression(value)) = self.facts(operand)?.map(|f| &f.kind) {
                if let Some((root, component)) = self.exact_path(value.path.as_ref())? {
                    state.observe(root, &component, self.budget)?;
                    return self.singleton(state);
                }
                if value.path.is_none() {
                    let root = Root::Temporary(operand);
                    let mut alternatives = self.expression(operand, state, depth)?;
                    for state in &mut alternatives {
                        state.observe(root, &[], self.budget)?;
                        let remaining = state.take(root, &[], self.budget)?;
                        self.discard(remaining, root, node, ResultDiscardKind::Expression)?;
                    }
                    return Ok(alternatives);
                }
            }
        }
        let limit = self.immutable_borrow_target(node)?;
        let mut alternatives = self.expression(node, state, depth)?;
        for state in &mut alternatives {
            if let Some((root, component)) = &limit {
                // The written immutable reference limits which occurrence it can
                // observe. This does not assert alias validity or a legal borrow.
                state.observe(*root, component, self.budget)?;
            } else if state.has_values() {
                self.pending(node, ResultUseBlocker::Storage)?;
                *state = Residence::default();
            }
        }
        Ok(alternatives)
    }

    fn immutable_borrow_target(
        &mut self,
        node: NodeId,
    ) -> Work<Option<(Root<'g>, Vec<ResultComponent<'g>>)>> {
        let Some(NodeFactKind::Expression(alias)) = self.facts(node)?.map(|f| &f.kind) else {
            return Ok(None);
        };
        let Some(alias) = alias
            .path
            .as_ref()
            .filter(|path| path.root.module == self.module.key && path.projections.is_empty())
        else {
            return Ok(None);
        };
        self.budget.step(1)?;
        let Some(&binding) = self.immutable_bindings.get(alias.root) else {
            return Ok(None);
        };
        let syntax = &self.module.syntax.nodes[binding];
        if !matches!(syntax.kind, NodeKind::Binding { .. }) {
            return Ok(None);
        }
        let source = &self.module.syntax.nodes[*syntax.children.last().unwrap()];
        if source.kind != NodeKind::Prefix("&") {
            return Ok(None);
        }
        let Some(NodeFactKind::Expression(referent)) =
            self.facts(source.children[0])?.map(|f| &f.kind)
        else {
            return Ok(None);
        };
        self.exact_path(referent.path.as_ref())
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
        for mut state in self.expression(operand, state, depth)? {
            state.observe(source, &[], self.budget)?;
            if branch.can_return {
                let mut returned = state.fork(self.budget)?;
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
            if branch.can_continue {
                self.move_value(
                    &mut state,
                    source,
                    &[ResultComponent::VariantPayload {
                        variant: branch.success,
                        index: 0,
                    }],
                    Root::Temporary(node),
                    &[],
                )?;
                state.take(source, &[], self.budget)?;
                self.budget.reserve(1)?;
                normal.push(state);
            }
        }
        Ok(normal)
    }

    fn match_result(
        &mut self,
        node: NodeId,
        matched: &'a crate::analysis::scalars::data::Match<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let source = Root::Temporary(matched.scrutinee);
        let mut result = Vec::new();
        for incoming in self.expression(matched.scrutinee, state, depth + 1)? {
            for arm in &matched.arms {
                self.budget.step(1)?;
                if !arm.body_reachable {
                    continue;
                }
                let mut state = incoming.fork(self.budget)?;
                let pattern_id = self.module.syntax.nodes[arm.site.node].children[0];
                let pattern = &self.module.syntax.nodes[pattern_id];
                if pattern.kind == NodeKind::Wildcard {
                    state.take(source, &[], self.budget)?;
                } else if matches!(
                    pattern.kind,
                    NodeKind::PayloadPattern | NodeKind::RecordPattern | NodeKind::NamePattern
                ) && !arm.views.is_empty()
                {
                    state.observe(source, &[], self.budget)?;
                    let payload = state.take(source, &[], self.budget)?;
                    if !payload.is_empty() || !arm.pattern.binders.is_empty() {
                        self.pending(arm.site.node, ResultUseBlocker::Storage)?;
                    }
                } else {
                    self.pending(arm.site.node, ResultUseBlocker::Storage)?;
                    state.take(source, &[], self.budget)?;
                }
                if self.module.syntax.nodes[arm.body].kind == NodeKind::Block {
                    result.extend(self.statement(arm.body, state, depth + 1)?);
                } else {
                    let mut next = self.expression(arm.body, state, depth + 1)?;
                    for state in &mut next {
                        self.move_value(
                            state,
                            Root::Temporary(arm.body),
                            &[],
                            Root::Temporary(node),
                            &[],
                        )?;
                    }
                    result.extend(next);
                }
            }
        }
        Ok(result)
    }

    fn call(
        &mut self,
        node: NodeId,
        value: &'a ExpressionFacts<'g>,
        state: Residence<'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        let Some(call) = &value.call else {
            self.pending(node, ResultUseBlocker::CallBoundary)?;
            return self.singleton(Residence::default());
        };
        self.budget.step(1)?;
        let checked_boundary =
            value.ordinary_context.is_none() && self.checked.contains(call.callee);
        let mut alternatives = self.singleton(state)?;
        for arg in call.argument_evaluation_order() {
            let mut next = Vec::new();
            for state in alternatives {
                if Some(call.callee) == self.core.outcome_observer {
                    next.extend(self.inspect_argument(state, arg, depth + 1)?);
                    continue;
                }
                let mut evaluated = self.expression(arg, state, depth + 1)?;
                for state in &mut evaluated {
                    let root = Root::Temporary(arg);
                    let passed = state.take(root, &[], self.budget)?;
                    if !passed.is_empty() {
                        if *call.callee == self.inventory.graph().prelude().dispose {
                            self.discard(passed, root, node, ResultDiscardKind::Expression)?;
                        } else if !checked_boundary {
                            self.pending(node, ResultUseBlocker::CallBoundary)?;
                        }
                    }
                }
                next.extend(evaluated);
            }
            alternatives = next;
        }
        if value.ordinary_context.is_some() {
            self.pending(node, ResultUseBlocker::CallBoundary)?;
            return Ok(alternatives);
        }
        if let Some(DataExpression::Propagation(branch)) = &value.data {
            // The checked call creates the container, and this pipe immediately
            // observes it. Only its success payload can become a new residence.
            if branch.can_return {
                for state in &alternatives {
                    let returned = state.fork(self.budget)?;
                    self.exit(returned, node)?;
                }
            }
            if !branch.can_continue {
                return Ok(Vec::new());
            }
        } else if matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "never") {
            return Ok(Vec::new());
        }
        for state in &mut alternatives {
            self.seed(state, Root::Temporary(node), node, &value.ty)?;
        }
        Ok(alternatives)
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

    fn exact_path(
        &mut self,
        path: Option<&PlacePath<'g>>,
    ) -> Work<Option<(Root<'g>, Vec<ResultComponent<'g>>)>> {
        let Some(path) = path else {
            return Ok(None);
        };
        self.budget.reserve(path.projections.len())?;
        let mut component = Vec::with_capacity(path.projections.len());
        for projection in &path.projections {
            component.push(match projection {
                PathProjection::Field(field) => ResultComponent::Field(field),
                PathProjection::TupleElement(i) => ResultComponent::TupleElement(*i),
                // Includes indirection and any more storage paths not yet admitted here.
                _ => return Ok(None),
            });
        }
        Ok(Some((Root::Local(path.root), component)))
    }

    fn is_result(&self, ty: &Type<'g>) -> bool {
        let ty = if let TypeTerm::Variant { parent, .. } = ty.as_ref() {
            parent
        } else {
            ty
        };
        matches!(ty.as_ref(), TypeTerm::Nominal { declaration, .. } if *declaration == self.core.result)
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

    fn seed(
        &mut self,
        state: &mut Residence<'g>,
        root: Root<'g>,
        origin: NodeId,
        ty: &Type<'g>,
    ) -> Work<()> {
        let mut path = Vec::new();
        let mut components = Vec::new();
        if self.containment(ty, None, &mut path, &mut components, 1)? {
            self.pending(origin, ResultUseBlocker::Containment)?;
        }
        for path in components {
            self.add_source(state, root, origin, &path)?;
        }
        Ok(())
    }

    fn containment(
        &mut self,
        ty: &Type<'g>,
        env: Option<&Arguments<'_, 'g>>,
        path: &mut Vec<ResultComponent<'g>>,
        out: &mut Vec<Vec<ResultComponent<'g>>>,
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
        let (ty, variant) = match ty.as_ref() {
            TypeTerm::Variant {
                parent,
                declaration,
            } => (parent, Some(*declaration)),
            _ => (ty, None),
        };
        match ty.as_ref() {
            TypeTerm::Primitive(_) | TypeTerm::Reference { .. } => Ok(false),
            TypeTerm::Callable { .. } => Ok(true),
            TypeTerm::Parameter {
                declaration,
                position,
            } => {
                let mut env = env;
                while let Some(frame) = env {
                    self.budget.step(1)?;
                    if frame
                        .binders
                        .get(*position)
                        .is_some_and(|b| b.declaration.id == **declaration)
                    {
                        return self.containment(
                            &frame.types[*position],
                            frame.outer,
                            path,
                            out,
                            depth + 1,
                        );
                    }
                    env = frame.outer;
                }
                Ok(true)
            }
            TypeTerm::Tuple(types) => {
                let mut pending = false;
                for (i, ty) in types.iter().enumerate() {
                    self.budget.reserve(1)?;
                    path.push(ResultComponent::TupleElement(i));
                    pending |= self.containment(ty, env, path, out, depth + 1)?;
                    path.pop();
                }
                Ok(pending)
            }
            TypeTerm::Nominal {
                declaration,
                arguments,
            } => {
                if Some(*declaration) == self.core.opaque_cause
                    || Some(*declaration) == self.core.opaque_list
                {
                    // A sealed source shape does not classify opaque owned storage.
                    return Ok(true);
                }
                let is_result = *declaration == self.core.result;
                if is_result {
                    self.budget.reserve(1 + path.len())?;
                    out.push(path.clone());
                }
                self.budget.step(1)?;
                let Some(shape) = self.inventory.type_facts().declarations.get(*declaration) else {
                    return Ok(true);
                };
                let inner = Arguments {
                    binders: &shape.binders,
                    types: arguments,
                    outer: env,
                };
                match &shape.shape {
                    DeclarationShape::Record { fields } => {
                        self.fields(fields, &inner, path, out, depth)
                    }
                    DeclarationShape::Enum { variants } => {
                        let mut pending = false;
                        for member in variants {
                            self.budget.step(1)?;
                            if variant.is_some_and(|id| *id != member.declaration.id) {
                                continue;
                            }
                            let mut possible = Vec::new();
                            let components = if variant.is_some() {
                                &mut *out
                            } else {
                                &mut possible
                            };
                            match &member.payload {
                                Payload::Unit => {}
                                Payload::Record(fields) => {
                                    pending |=
                                        self.fields(fields, &inner, path, components, depth)?;
                                }
                                Payload::Tuple(types) => {
                                    for (index, ty) in types.iter().enumerate() {
                                        self.budget.reserve(1)?;
                                        path.push(ResultComponent::VariantPayload {
                                            variant: &member.declaration.id,
                                            index,
                                        });
                                        pending |= self.containment(
                                            ty,
                                            Some(&inner),
                                            path,
                                            components,
                                            depth + 1,
                                        )?;
                                        path.pop();
                                    }
                                }
                            }
                            // Without an exact variant, these payload occurrences are alternatives.
                            pending |= !possible.is_empty();
                        }
                        Ok(pending)
                    }
                    DeclarationShape::Function { .. } => Ok(true),
                }
            }
            TypeTerm::Variant { .. } => unreachable!("variant parents are nominal"),
        }
    }

    fn fields(
        &mut self,
        fields: &[MemberType<'g>],
        inner: &Arguments<'_, 'g>,
        path: &mut Vec<ResultComponent<'g>>,
        out: &mut Vec<Vec<ResultComponent<'g>>>,
        depth: usize,
    ) -> Work<bool> {
        let mut pending = false;
        for field in fields {
            self.budget.reserve(1)?;
            path.push(ResultComponent::Field(&field.declaration.id));
            pending |= self.containment(&field.ty, Some(inner), path, out, depth + 1)?;
            path.pop();
        }
        Ok(pending)
    }
}
