//! M4.2 scoped member/signature and source-nominal containment checking.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Schema {
    BorrowedCopy,
    ConsumingDispose,
}
#[derive(Clone, Copy, Debug)]
pub struct Contract<'g> {
    pub declaration: &'g DeclarationId,
    pub schema: Schema,
}
#[derive(Clone, Copy, Debug)]
pub struct PreludeContracts<'g> {
    pub copy: Contract<'g>,
    pub dispose: Contract<'g>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    MemberUniqueness,
    PreludeSignatures,
    ExistingTypePositions,
    SourceNominalInlineContainment,
}
#[derive(Debug)]
pub struct CheckCompletion {
    pub completed: &'static [Check],
    pub not_performed: &'static [Stage],
}
const COMPLETION: CheckCompletion = CheckCompletion {
    completed: &[
        Check::MemberUniqueness,
        Check::PreludeSignatures,
        Check::ExistingTypePositions,
        Check::SourceNominalInlineContainment,
    ],
    not_performed: &[
        Stage::DeclarationValidity,
        Stage::SignatureCompatibility,
        Stage::Layout,
        Stage::Bodies,
        Stage::Startup,
        Stage::Expectations,
        Stage::Inference,
        Stage::Coverage,
        Stage::Evidence,
        Stage::CoreAdmission,
        Stage::ResultUse,
        Stage::Ownership,
        Stage::Execution,
    ],
};
#[derive(Debug)]
pub struct CheckedDeclaration<'r, 'g> {
    pub types: &'r DeclarationFacts<'g>,
    pub members_unique: bool,
    pub prelude: Option<Schema>,
}
#[derive(Clone, Copy, Debug)]
pub struct StorageSource<'r, 'g> {
    pub member: &'g Declaration,
    pub annotation: &'r TypeUse<'g>,
}
#[derive(Clone, Copy, Debug)]
pub struct Forwarding<'r, 'g> {
    pub declaration: &'g DeclarationId,
    pub parameter: usize,
    pub argument: &'r Type<'g>,
    pub summary_generation: usize,
}
#[derive(Debug)]
pub struct InlineParameter<'r, 'g> {
    pub source: StorageSource<'r, 'g>,
    pub forwarding: Vec<Forwarding<'r, 'g>>,
    pub generation: usize,
}
#[derive(Clone, Debug)]
pub struct ContainmentEdge<'r, 'g> {
    pub origin: &'g DeclarationId,
    pub target: &'g DeclarationId,
    pub source: StorageSource<'r, 'g>,
    pub forwarding: Vec<Forwarding<'r, 'g>>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FieldClass {
    #[default]
    Unclassified,
    TextFreeScalars,
    Text,
    Other,
}
#[derive(Clone, Copy, Debug, Default)]
struct Summary {
    fields: FieldClass,
    custom_dispose: bool,
}
#[derive(Debug, Default)]
pub(super) struct DeclarationSummaries(Vec<Summary>);
impl DeclarationSummaries {
    pub(super) fn fields(&self, id: &DeclarationId) -> FieldClass {
        self.0
            .get(id.ordinal.index())
            .map_or_else(FieldClass::default, |summary| summary.fields)
    }
    pub(super) fn custom_dispose(&self, id: &DeclarationId) -> bool {
        self.0
            .get(id.ordinal.index())
            .is_some_and(|summary| summary.custom_dispose)
    }
}
#[derive(Debug)]
pub struct LayoutFacts<'r, 'g> {
    pub declaration: &'g Declaration,
    pub inline_parameters: BTreeMap<usize, InlineParameter<'r, 'g>>,
    pub edges: BTreeMap<&'g DeclarationId, ContainmentEdge<'r, 'g>>,
    /// Storage-summary knowledge only, not acyclicity, constructibility or target validity.
    pub complete: bool,
}
#[derive(Debug)]
pub struct LayoutObligation<'r, 'g> {
    pub declaration: &'g DeclarationId,
    pub source: StorageSource<'r, 'g>,
    pub unknown_declaration: Option<&'g DeclarationId>,
}
#[derive(Debug)]
pub enum CheckFinding<'r, 'g> {
    DuplicateMembers {
        scope: &'g Declaration,
        members: Vec<&'g Declaration>,
    },
    PreludeSignature {
        schema: Schema,
        actual: &'r DeclarationFacts<'g>,
    },
    InlineCycle {
        steps: Vec<ContainmentEdge<'r, 'g>>,
    },
}
impl CheckFinding<'_, '_> {
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::InlineCycle { .. } => Some("R0406"),
            _ => None,
        }
    }
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FindingOrder {
    Duplicate,
    Prelude(Schema),
    Cycle,
}
#[derive(Debug, Default)]
pub struct CheckFacts<'r, 'g> {
    declarations: BTreeMap<&'g DeclarationId, CheckedDeclaration<'r, 'g>>,
    layouts: BTreeMap<&'g DeclarationId, LayoutFacts<'r, 'g>>,
    summaries: DeclarationSummaries,
    obligations: BTreeMap<(&'g DeclarationId, NodeId), LayoutObligation<'r, 'g>>,
    findings: BTreeMap<(&'g DeclarationId, usize, FindingOrder), CheckFinding<'r, 'g>>,
}
impl<'r, 'g> CheckFacts<'r, 'g> {
    pub(super) fn take_summaries(&mut self) -> DeclarationSummaries {
        std::mem::take(&mut self.summaries)
    }
    pub fn declarations(&self) -> impl Iterator<Item = &CheckedDeclaration<'r, 'g>> {
        self.declarations.values()
    }
    pub fn findings(&self) -> impl Iterator<Item = &CheckFinding<'r, 'g>> {
        self.findings.values()
    }
    pub fn layouts(&self) -> impl Iterator<Item = &LayoutFacts<'r, 'g>> {
        self.layouts.values()
    }
    pub fn obligations(&self) -> impl Iterator<Item = &LayoutObligation<'r, 'g>> {
        self.obligations.values()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckFailureKind {
    InvalidRequest,
    SourceRejected,
    Incomplete,
    ResourceLimit,
}
#[derive(Debug)]
pub struct CheckFailure<'r, 'g> {
    pub kind: CheckFailureKind,
    pub partial: Box<CheckFacts<'r, 'g>>,
    pub statistics: Statistics,
    pub complete_diagnostics: bool,
}
#[derive(Debug)]
pub struct DeclarationReceipt<'r, 'g> {
    pub(super) types: &'r Receipt<'g>,
    facts: CheckFacts<'r, 'g>,
    statistics: Statistics,
}
impl<'r, 'g> DeclarationReceipt<'r, 'g> {
    pub fn context(&self) -> GraphId {
        self.types.context()
    }
    pub fn completion(&self) -> &CheckCompletion {
        &COMPLETION
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
    pub fn declarations(&self) -> impl Iterator<Item = &CheckedDeclaration<'r, 'g>> {
        self.facts.declarations()
    }
    pub fn layouts(&self) -> impl Iterator<Item = &LayoutFacts<'r, 'g>> {
        self.facts.layouts()
    }
    pub fn layout(&self, id: &DeclarationId) -> Result<Option<&LayoutFacts<'r, 'g>>, QueryError> {
        self.types.declaration(id)?;
        Ok(self.facts.layouts.get(id))
    }
    pub fn declaration(
        &self,
        id: &DeclarationId,
    ) -> Result<Option<&CheckedDeclaration<'r, 'g>>, QueryError> {
        self.types.declaration(id)?;
        Ok(self.facts.declarations.get(id))
    }
}
pub(super) struct Budget {
    pub(super) limits: AnalysisLimits,
    pub(super) statistics: Statistics,
}
impl Budget {
    pub(super) fn step(&mut self, n: usize) -> Work<()> {
        let next = self.statistics.work.checked_add(n).ok_or(())?;
        if next > self.limits.max_work {
            return Err(());
        }
        self.statistics.work = next;
        Ok(())
    }
    // Materializing entries charges both storage and per-entry work.
    pub(super) fn reserve(&mut self, n: usize) -> Work<()> {
        let next = self.statistics.entries.checked_add(n).ok_or(())?;
        if next > self.limits.max_entries {
            return Err(());
        }
        self.step(n)?;
        self.statistics.entries = next;
        Ok(())
    }
}
impl crate::loader::keyed::Meter for Budget {
    fn step(&mut self, n: usize) -> Work<()> {
        Budget::step(self, n)
    }
}
#[derive(Clone, Copy)]
struct Stored<'r, 'g> {
    origin: &'g DeclarationId,
    source: StorageSource<'r, 'g>,
    ty: &'r Type<'g>,
}
struct Checker<'r, 'g> {
    graph: &'g Graph,
    types: &'r PartialFacts<'g>,
    budget: Budget,
    facts: CheckFacts<'r, 'g>,
    inputs: Vec<Stored<'r, 'g>>,
    changed: bool,
}
impl<'r, 'g> Checker<'r, 'g> {
    fn names(
        &mut self,
        scope: &'g Declaration,
        members: impl Iterator<Item = &'g Declaration>,
    ) -> Work<bool> {
        let mut names: BTreeMap<&str, Vec<&Declaration>> = BTreeMap::new();
        for member in members {
            self.budget.step(1)?;
            let Some(name) = member.name.as_deref() else {
                continue;
            };
            if !names.contains_key(name) {
                self.budget.reserve(1)?;
            }
            self.budget.reserve(1)?;
            names.entry(name).or_default().push(member);
        }
        let mut unique = true;
        for members in names.into_values() {
            self.budget.step(1)?;
            if members.len() > 1 {
                self.budget.reserve(1)?;
                self.facts.findings.insert(
                    (&scope.id, members[1].span.start(), FindingOrder::Duplicate),
                    CheckFinding::DuplicateMembers { scope, members },
                );
                unique = false;
            }
        }
        Ok(unique)
    }
    fn names_without_types(&mut self) -> Work<()> {
        // In a partial inventory, a bad annotation can hide the typed shape.
        // Use actual captured declarations to check its independent names.
        for module in self.graph.modules() {
            let nodes = &module.syntax.nodes;
            let mut by_node = BTreeMap::new();
            for scope in &module.declarations {
                self.budget.step(1)?;
                if scope.scope.is_some() || self.types.declarations.contains_key(&scope.id) {
                    continue;
                }
                if !matches!(
                    scope.id.kind,
                    DeclarationKind::Record
                        | DeclarationKind::Enum
                        | DeclarationKind::Function
                        | DeclarationKind::Family
                        | DeclarationKind::Implementation
                ) {
                    continue;
                }
                if by_node.is_empty() {
                    self.budget.reserve(module.declarations.len())?;
                    by_node.extend(module.declarations.iter().map(|d| (d.node, d)));
                }
                let members =
                    |container: NodeId| nodes[container].children.iter().map(|node| by_node[node]);
                let children = &nodes[scope.node].children;
                match scope.id.kind {
                    DeclarationKind::Record => {
                        self.names(scope, members(*children.last().unwrap()))?;
                    }
                    DeclarationKind::Enum => {
                        let variants = || {
                            children
                                .iter()
                                .copied()
                                .filter(|&node| nodes[node].kind == NodeKind::EnumVariant)
                        };
                        self.budget.step(children.len())?;
                        self.names(scope, variants().map(|node| by_node[&node]))?;
                        self.budget.step(children.len())?;
                        for variant in variants() {
                            if let Some(&payload) = nodes[variant].children.get(1)
                                && nodes[payload].kind == NodeKind::RecordPayload
                            {
                                self.names(by_node[&variant], members(payload))?;
                            }
                        }
                    }
                    _ => {
                        let signature = &nodes[children[0]].children;
                        self.budget.step(signature.len())?;
                        let parameters = *signature
                            .iter()
                            .find(|&&node| nodes[node].kind == NodeKind::ValueParameters)
                            .unwrap();
                        self.names(scope, members(parameters))?;
                    }
                }
            }
        }
        Ok(())
    }
    fn prelude(&mut self, facts: &'r DeclarationFacts<'g>, schema: Schema) -> Work<bool> {
        self.budget.step(1)?;
        let DeclarationShape::Function { parameters, result } = &facts.shape else {
            unreachable!("M3 canonical prelude families");
        };
        let valid = if facts.binders.len() == 1 && parameters.len() == 1 {
            let binder = &facts.binders[0].declaration.id;
            let parameter = |ty: &Type<'g>| matches!(ty.as_ref(), TypeTerm::Parameter { declaration, position: 0 } if *declaration == binder);
            match schema {
                Schema::BorrowedCopy => {
                    matches!(parameters[0].ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if parameter(referent))
                        && parameter(result)
                }
                Schema::ConsumingDispose => {
                    let mut void = None;
                    for primitive in &self.graph.prelude().intrinsics {
                        self.budget.step(1)?;
                        if primitive.namespace() == Namespace::TypeModule
                            && primitive.name() == "void"
                        {
                            void = Some(primitive);
                            break;
                        }
                    }
                    parameter(&parameters[0].ty)
                        && matches!(result.as_ref(), TypeTerm::Primitive(p) if Some(*p) == void)
                }
            }
        } else {
            false
        };
        if !valid {
            self.budget.reserve(1)?;
            self.facts.findings.insert(
                (
                    &facts.declaration.id,
                    facts.declaration.span.start(),
                    FindingOrder::Prelude(schema),
                ),
                CheckFinding::PreludeSignature {
                    schema,
                    actual: facts,
                },
            );
        }
        Ok(valid)
    }
    fn owner(&mut self, key: &ModuleKey) -> Work<&'g Module> {
        for module in self.graph.modules() {
            self.budget.step(1)?;
            if module.key == *key {
                return Ok(module);
            }
        }
        unreachable!("M4.1 declaration module")
    }
    fn storage(
        &mut self,
        origin: &'g DeclarationId,
        member: &'g Declaration,
        node: NodeId,
        ty: &'r Type<'g>,
    ) -> Work<()> {
        let module = self.owner(&member.id.module)?;
        self.budget
            .step(self.types.uses.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        let annotation =
            &self.types.uses[&(&module.key, module.syntax.nodes[node].span.start(), node)];
        self.budget.reserve(1)?;
        self.inputs.push(Stored {
            origin,
            source: StorageSource { member, annotation },
            ty,
        });
        Ok(())
    }
    fn member_storage(
        &mut self,
        origin: &'g DeclarationId,
        member: &'r MemberType<'g>,
    ) -> Work<()> {
        let module = self.owner(&member.declaration.id.module)?;
        self.storage(
            origin,
            member.declaration,
            module.syntax.nodes[member.declaration.node].children[1],
            &member.ty,
        )
    }
    fn layout_inputs(&mut self) -> Work<()> {
        let count = self.graph.declaration_count();
        self.budget.reserve(count)?;
        self.facts.summaries = DeclarationSummaries(vec![Summary::default(); count]);
        for facts in self.types.declarations() {
            self.budget.step(1)?;
            if let DeclarationShape::Function { parameters, .. } = &facts.shape {
                if let [formal] = parameters.as_slice()
                    && let TypeTerm::Nominal {
                        declaration,
                        arguments,
                    } = formal.ty.as_ref()
                    && arguments.is_empty()
                    && facts.binders.is_empty()
                    && facts.declaration.id.module.origin == crate::loader::Origin::Application
                    && self
                        .graph
                        .implementation(&facts.declaration.id)
                        .is_some_and(|implementation| {
                            implementation.family.ordinal == self.graph.prelude().dispose.ordinal
                        })
                {
                    self.facts.summaries.0[declaration.ordinal.index()].custom_dispose = true;
                }
                continue;
            }
            let origin = &facts.declaration.id;
            self.budget.reserve(1)?;
            self.facts.layouts.insert(
                origin,
                LayoutFacts {
                    declaration: facts.declaration,
                    inline_parameters: BTreeMap::new(),
                    edges: BTreeMap::new(),
                    complete: true,
                },
            );
            match &facts.shape {
                DeclarationShape::Record { fields } => {
                    let mut scalars = true;
                    let mut text = !fields.is_empty();
                    for field in fields {
                        self.member_storage(origin, field)?;
                        let string = matches!(field.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "string");
                        scalars &= matches!(field.ty.as_ref(), TypeTerm::Primitive(_)) && !string;
                        text &= string;
                    }
                    if origin.module.origin == crate::loader::Origin::Application
                        && facts.binders.is_empty()
                    {
                        self.facts.summaries.0[origin.ordinal.index()].fields = if scalars {
                            FieldClass::TextFreeScalars
                        } else if text {
                            FieldClass::Text
                        } else {
                            FieldClass::Other
                        };
                    }
                }
                DeclarationShape::Enum { variants } => {
                    for variant in variants {
                        self.budget.step(1)?;
                        match &variant.payload {
                            Payload::Unit => {}
                            Payload::Record(fields) => {
                                for field in fields {
                                    self.member_storage(origin, field)?;
                                }
                            }
                            Payload::Tuple(types) => {
                                let module = self.owner(&origin.module)?;
                                let payload =
                                    module.syntax.nodes[variant.declaration.node].children[1];
                                for (index, ty) in types.iter().enumerate() {
                                    self.storage(
                                        origin,
                                        variant.declaration,
                                        module.syntax.nodes[payload].children[index],
                                        ty,
                                    )?;
                                }
                            }
                        }
                    }
                }
                DeclarationShape::Function { .. } => unreachable!(),
            }
        }
        Ok(())
    }
    fn unknown(
        &mut self,
        input: Stored<'r, 'g>,
        declaration: Option<&'g DeclarationId>,
    ) -> Work<()> {
        let layout = self.facts.layouts.get_mut(input.origin).unwrap();
        if layout.complete {
            layout.complete = false;
            self.changed = true;
        }
        let key = (input.origin, input.source.annotation.node);
        if !self.facts.obligations.contains_key(&key) {
            self.budget.reserve(1)?;
            self.facts.obligations.insert(
                key,
                LayoutObligation {
                    declaration: input.origin,
                    source: input.source,
                    unknown_declaration: declaration,
                },
            );
        }
        Ok(())
    }
    fn walk(
        &mut self,
        input: Stored<'r, 'g>,
        ty: &'r Type<'g>,
        route: &mut Vec<Forwarding<'r, 'g>>,
        depth: usize,
        summary: bool,
    ) -> Work<()> {
        self.budget.step(1)?;
        if depth > self.budget.limits.max_depth.min(64) {
            return Err(());
        }
        self.budget.statistics.max_depth = self.budget.statistics.max_depth.max(depth);
        match ty.as_ref() {
            TypeTerm::Primitive(_) | TypeTerm::Reference { .. } => {}
            TypeTerm::Parameter { position, .. } if summary => {
                if !self.facts.layouts[input.origin]
                    .inline_parameters
                    .contains_key(position)
                {
                    self.budget.reserve(route.len().checked_add(1).ok_or(())?)?;
                    let generation = self.budget.statistics.entries;
                    self.facts
                        .layouts
                        .get_mut(input.origin)
                        .unwrap()
                        .inline_parameters
                        .insert(
                            *position,
                            InlineParameter {
                                source: input.source,
                                forwarding: route.clone(),
                                generation,
                            },
                        );
                    self.changed = true;
                }
            }
            TypeTerm::Parameter { .. } => {}
            TypeTerm::Tuple(elements) => {
                for element in elements {
                    self.walk(input, element, route, depth + 1, summary)?;
                }
            }
            TypeTerm::Callable { .. } => self.unknown(input, None)?,
            TypeTerm::Variant { parent, .. } => {
                self.walk(input, parent, route, depth + 1, summary)?
            }
            TypeTerm::Nominal {
                declaration,
                arguments,
            } => {
                let target = *declaration;
                if !summary {
                    let replace =
                        if let Some(old) = self.facts.layouts[input.origin].edges.get(target) {
                            self.budget.step(
                                route
                                    .len()
                                    .checked_add(old.forwarding.len())
                                    .and_then(|n| n.checked_add(1))
                                    .ok_or(())?,
                            )?;
                            (
                                &input.source.member.id,
                                input.source.annotation.span.start(),
                            )
                                .cmp(&(&old.source.member.id, old.source.annotation.span.start()))
                                .then_with(|| {
                                    route
                                        .iter()
                                        .map(|s| s.parameter)
                                        .cmp(old.forwarding.iter().map(|s| s.parameter))
                                })
                                .is_lt()
                        } else {
                            true
                        };
                    if replace {
                        self.budget.reserve(route.len().checked_add(1).ok_or(())?)?;
                        self.facts
                            .layouts
                            .get_mut(input.origin)
                            .unwrap()
                            .edges
                            .insert(
                                target,
                                ContainmentEdge {
                                    origin: input.origin,
                                    target,
                                    source: input.source,
                                    forwarding: route.clone(),
                                },
                            );
                    }
                }
                if !self.facts.layouts[target].complete {
                    self.unknown(input, Some(target))?;
                }
                let mut after = None;
                loop {
                    let parameters = &self.facts.layouts[target].inline_parameters;
                    self.budget
                        .step(parameters.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
                    let next = if let Some(index) = after {
                        parameters
                            .range((std::ops::Bound::Excluded(index), std::ops::Bound::Unbounded))
                            .next()
                    } else {
                        parameters.iter().next()
                    }
                    .map(|(&index, proof)| (index, proof.generation));
                    let Some((parameter, summary_generation)) = next else {
                        break;
                    };
                    self.budget.reserve(1)?;
                    route.push(Forwarding {
                        declaration: target,
                        parameter,
                        argument: &arguments[parameter],
                        summary_generation,
                    });
                    self.walk(input, &arguments[parameter], route, depth + 1, summary)?;
                    route.pop();
                    after = Some(parameter);
                }
            }
        }
        Ok(())
    }
    fn cycles(&mut self) -> Work<()> {
        let n = self.facts.layouts.len();
        self.budget.reserve(n.checked_mul(5).ok_or(())?)?;
        let vertices: Vec<_> = self.facts.layouts.keys().copied().collect();
        let indices: BTreeMap<_, _> = vertices
            .iter()
            .copied()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        let mut forward = vec![Vec::new(); n];
        let mut reverse = vec![Vec::new(); n];
        let mut visited = vec![false; n];
        for (origin, layout) in &self.facts.layouts {
            for target in layout.edges.keys() {
                self.budget
                    .step(n.checked_ilog2().unwrap_or(0) as usize + 1)?;
                self.budget.reserve(2)?;
                let (a, b) = (indices[origin], indices[target]);
                forward[a].push(b);
                reverse[b].push(a);
            }
        }
        let mut finish = Vec::new();
        for root in 0..n {
            self.budget.step(1)?;
            if visited[root] {
                continue;
            }
            self.budget.reserve(1)?;
            let mut stack = vec![(root, 0)];
            visited[root] = true;
            while let Some(&(at, next)) = stack.last() {
                self.budget.step(1)?;
                if let Some(&child) = forward[at].get(next) {
                    stack.last_mut().unwrap().1 += 1;
                    if !visited[child] {
                        self.budget.reserve(1)?;
                        visited[child] = true;
                        stack.push((child, 0));
                    }
                } else {
                    self.budget.reserve(1)?;
                    finish.push(at);
                    stack.pop();
                }
            }
        }
        self.budget.reserve(n.checked_mul(2).ok_or(())?)?;
        let mut component_of = vec![None; n];
        let mut components = BTreeMap::new();
        for &root in finish.iter().rev() {
            self.budget.step(1)?;
            if component_of[root].is_some() {
                continue;
            }
            let number = components.len();
            self.budget.reserve(1)?;
            let mut stack = vec![root];
            component_of[root] = Some(number);
            let mut minimum = root;
            let mut count = 0;
            while let Some(at) = stack.pop() {
                self.budget.step(1)?;
                count += 1;
                minimum = minimum.min(at);
                for &child in &reverse[at] {
                    self.budget.step(1)?;
                    if component_of[child].is_none() {
                        self.budget.reserve(1)?;
                        component_of[child] = Some(number);
                        stack.push(child);
                    }
                }
            }
            self.budget.reserve(1)?;
            components.insert(minimum, (number, count));
        }
        let mut color = vec![0u8; n];
        for (root, (number, count)) in components {
            self.budget.step(1)?;
            if count == 1 {
                self.budget.step(forward[root].len())?;
                if !forward[root].contains(&root) {
                    continue;
                }
            }
            self.budget.reserve(1)?;
            let mut stack = vec![(root, 0)];
            color[root] = 1;
            while let Some(&(at, next)) = stack.last() {
                self.budget.step(1)?;
                let Some(&child) = forward[at].get(next) else {
                    color[at] = 2;
                    stack.pop();
                    continue;
                };
                stack.last_mut().unwrap().1 += 1;
                if component_of[child] != Some(number) {
                    continue;
                }
                if color[child] == 0 {
                    self.budget.reserve(1)?;
                    color[child] = 1;
                    stack.push((child, 0));
                } else if color[child] == 1 {
                    self.budget.step(stack.len())?;
                    let start = stack.iter().position(|&(v, _)| v == child).unwrap();
                    let mut steps = Vec::new();
                    for index in start..stack.len() {
                        let origin = vertices[stack[index].0];
                        let target = vertices[if index + 1 < stack.len() {
                            stack[index + 1].0
                        } else {
                            child
                        }];
                        let edge = &self.facts.layouts[origin].edges[target];
                        self.budget
                            .reserve(edge.forwarding.len().checked_add(1).ok_or(())?)?;
                        steps.push(edge.clone());
                    }
                    self.budget.reserve(1)?;
                    self.facts.findings.insert(
                        (vertices[root], 0, FindingOrder::Cycle),
                        CheckFinding::InlineCycle { steps },
                    );
                    break;
                }
            }
        }
        Ok(())
    }
    fn run(&mut self, contracts: PreludeContracts<'g>) -> Work<()> {
        for facts in self.types.declarations() {
            self.budget.step(1)?;
            let members_unique = match &facts.shape {
                DeclarationShape::Function { parameters, .. } => {
                    self.names(facts.declaration, parameters.iter().map(|m| m.declaration))?
                }
                DeclarationShape::Record { fields } => {
                    self.names(facts.declaration, fields.iter().map(|m| m.declaration))?
                }
                DeclarationShape::Enum { variants } => {
                    let mut unique =
                        self.names(facts.declaration, variants.iter().map(|v| v.declaration))?;
                    for variant in variants {
                        if let Payload::Record(fields) = &variant.payload {
                            unique &= self
                                .names(variant.declaration, fields.iter().map(|m| m.declaration))?;
                        }
                    }
                    unique
                }
            };
            let mut prelude = None;
            for contract in [contracts.copy, contracts.dispose] {
                if facts.declaration.id == *contract.declaration
                    && self.prelude(facts, contract.schema)?
                {
                    prelude = Some(contract.schema);
                }
            }
            self.budget.reserve(1)?;
            self.facts.declarations.insert(
                &facts.declaration.id,
                CheckedDeclaration {
                    types: facts,
                    members_unique,
                    prelude,
                },
            );
        }
        if !self.types.findings.is_empty() || !self.types.obligations.is_empty() {
            self.names_without_types()?;
        }
        self.layout_inputs()?;
        loop {
            self.changed = false;
            for index in 0..self.inputs.len() {
                let input = self.inputs[index];
                self.walk(input, input.ty, &mut Vec::new(), 1, true)?;
            }
            if !self.changed {
                break;
            }
        }
        for index in 0..self.inputs.len() {
            let input = self.inputs[index];
            self.walk(input, input.ty, &mut Vec::new(), 1, false)?;
        }
        self.cycles()?;
        Ok(())
    }
}

pub(super) fn check_inventory<'r, 'g>(
    graph: &'g Graph,
    types: &'r PartialFacts<'g>,
    contracts: PreludeContracts<'g>,
    budget: &mut Budget,
) -> (Work<()>, CheckFacts<'r, 'g>) {
    let mut checker = Checker {
        graph,
        types,
        budget: Budget {
            limits: budget.limits,
            statistics: budget.statistics,
        },
        facts: CheckFacts::default(),
        inputs: Vec::new(),
        changed: false,
    };
    let result = checker.run(contracts);
    budget.statistics = checker.budget.statistics;
    (result, checker.facts)
}

pub fn check_declarations<'r, 'g>(
    types: &'r Receipt<'g>,
    contracts: PreludeContracts<'g>,
    limits: AnalysisLimits,
) -> Result<DeclarationReceipt<'r, 'g>, CheckFailure<'r, 'g>> {
    if contracts.copy.declaration != &types.graph.prelude().copy
        || contracts.dispose.declaration != &types.graph.prelude().dispose
        || contracts.copy.schema != Schema::BorrowedCopy
        || contracts.dispose.schema != Schema::ConsumingDispose
    {
        return Err(CheckFailure {
            kind: CheckFailureKind::InvalidRequest,
            partial: Box::default(),
            statistics: Statistics::default(),
            complete_diagnostics: false,
        });
    }
    let mut budget = Budget {
        limits,
        statistics: Statistics::default(),
    };
    let (result, facts) = check_inventory(types.graph, &types.facts, contracts, &mut budget);
    let kind = if result.is_err() {
        Some(CheckFailureKind::ResourceLimit)
    } else if !facts.obligations.is_empty() {
        Some(CheckFailureKind::Incomplete)
    } else if !facts.findings.is_empty() {
        Some(CheckFailureKind::SourceRejected)
    } else {
        None
    };
    if let Some(kind) = kind {
        Err(CheckFailure {
            kind,
            partial: Box::new(facts),
            statistics: budget.statistics,
            complete_diagnostics: false,
        })
    } else {
        Ok(DeclarationReceipt {
            types,
            facts,
            statistics: budget.statistics,
        })
    }
}
