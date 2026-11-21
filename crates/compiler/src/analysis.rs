//! M4.1 explicit declaration types. A receipt is not a checked program.
use crate::loader::{
    Binding, Declaration, DeclarationId, DeclarationKind, Graph, GraphId, Intrinsic, Module,
    ModuleKey, Namespace, Target,
};
use crate::parser::{NodeId, NodeKind};
use crate::source::Span;
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod conversions;
pub mod declarations;
pub mod ordinary;
pub mod ownership;
pub mod scalars;
pub mod typed;
pub use scalars::{data, generics};

#[derive(Clone, Copy, Debug)]
pub struct AnalysisLimits {
    pub max_work: usize,
    pub max_entries: usize,
    pub max_depth: usize,
}
impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_work: 1_000_000,
            max_entries: 100_000,
            max_depth: 64,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    pub work: usize,
    pub entries: usize,
    pub max_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvocationMode {
    ReadOnly,
    Mutable,
    Once,
}
pub type Type<'g> = Arc<TypeTerm<'g>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeTerm<'g> {
    Primitive(&'g Intrinsic),
    Parameter {
        declaration: &'g DeclarationId,
        position: usize,
    },
    Reference {
        mutable: bool,
        referent: Type<'g>,
    },
    Nominal {
        declaration: &'g DeclarationId,
        arguments: Vec<Type<'g>>,
    },
    Variant {
        parent: Type<'g>,
        declaration: &'g DeclarationId,
    },
    Tuple(Vec<Type<'g>>),
    Callable {
        mode: InvocationMode,
        parameters: Vec<Type<'g>>,
        result: Type<'g>,
    },
}
// The scoped retained-destination handoff recognizes only a caller's
// exclusive access to a shared bool descriptor, not general nested storage.
fn retained_bool_contents_type<'a, 'g>(ty: &'a TypeTerm<'g>) -> Option<&'a Type<'g>> {
    if let TypeTerm::Reference {
        mutable: true,
        referent,
    } = ty
        && matches!(referent.as_ref(), TypeTerm::Reference { mutable: false, referent: inner }
            if matches!(inner.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"))
    {
        Some(referent)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct Binder<'g> {
    pub declaration: &'g Declaration,
    pub position: usize,
}
#[derive(Debug)]
pub struct MemberType<'g> {
    pub declaration: &'g Declaration,
    pub ty: Type<'g>,
    /// Integrated body typing identified this formal as the direct source of
    /// an ordinary named string yield. No storage/provider proof is implied.
    pub(in crate::analysis) text_iteration_source: bool,
    /// The direct consuming loop reached with this whole incoming Option list fresh.
    pub(in crate::analysis) option_iteration_source: Option<NodeId>,
}
#[derive(Debug)]
pub enum Payload<'g> {
    Unit,
    Tuple(Vec<Type<'g>>),
    Record(Vec<MemberType<'g>>),
}
#[derive(Debug)]
pub struct VariantFacts<'g> {
    pub declaration: &'g Declaration,
    pub payload: Payload<'g>,
}
#[derive(Debug)]
pub enum DeclarationShape<'g> {
    Record {
        fields: Vec<MemberType<'g>>,
    },
    Enum {
        variants: Vec<VariantFacts<'g>>,
    },
    Function {
        parameters: Vec<MemberType<'g>>,
        result: Type<'g>,
    },
}
#[derive(Debug)]
pub struct DeclarationFacts<'g> {
    pub declaration: &'g Declaration,
    pub binders: Vec<Binder<'g>>,
    pub shape: DeclarationShape<'g>,
}
#[derive(Clone, Debug)]
pub struct Substitution<'g> {
    pub binder: &'g DeclarationId,
    pub position: usize,
    pub argument: Type<'g>,
}
#[derive(Clone, Debug)]
pub struct TypeUse<'g> {
    pub module: &'g ModuleKey,
    pub node: NodeId,
    pub span: &'g Span,
    pub ty: Type<'g>,
    pub bindings: Vec<&'g Binding>,
    pub substitution: Vec<Substitution<'g>>,
    pub defaults: Vec<DefaultArgumentUse<'g>>,
}
#[derive(Clone, Debug)]
pub struct DefaultArgumentUse<'g> {
    pub binder: &'g DeclarationId,
    pub node: NodeId,
    pub span: &'g Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    DeclarationTypes,
    DeclarationValidity,
    SignatureCompatibility,
    PositionValidity,
    Layout,
    Bodies,
    Startup,
    Expectations,
    Inference,
    Coverage,
    Evidence,
    CoreAdmission,
    ResultUse,
    Ownership,
    Execution,
}
#[derive(Debug)]
pub struct Completion {
    pub completed: Stage,
    pub not_performed: &'static [Stage],
}
const COMPLETION: Completion = Completion {
    completed: Stage::DeclarationTypes,
    not_performed: &[
        Stage::DeclarationValidity,
        Stage::SignatureCompatibility,
        Stage::PositionValidity,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingKind {
    MissingType,
    PrivateType,
    NotAType,
    ExcessArguments,
    DuplicateBinder,
    InvalidVariantParent,
    MissingVariant,
    AmbiguousVariant,
    NonTrailingDefault,
    InvalidDefaultDependency,
    DuplicateLocalDeclaration,
}
#[derive(Clone, Debug)]
pub struct Finding<'g> {
    pub kind: FindingKind,
    pub span: &'g Span,
    pub candidates: Vec<&'g Declaration>,
    pub bindings: Vec<&'g Binding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unsupported {
    GenericDefault,
    Requirements,
    Implementation,
    NeedsTypeArguments,
    RecursiveDefault,
}
#[derive(Clone, Debug)]
pub struct Obligation<'g> {
    pub reason: Unsupported,
    pub module: &'g ModuleKey,
    pub node: NodeId,
    pub span: &'g Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    TypeRejected,
    Incomplete,
    ResourceLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryError {
    InvalidRequest,
}

#[derive(Debug, Default)]
pub struct PartialFacts<'g> {
    declarations: BTreeMap<&'g DeclarationId, DeclarationFacts<'g>>,
    uses: BTreeMap<(&'g ModuleKey, usize, NodeId), TypeUse<'g>>,
    findings: BTreeMap<(&'g ModuleKey, usize, FindingKind), Finding<'g>>,
    obligations: BTreeMap<(&'g ModuleKey, usize, Unsupported), Obligation<'g>>,
}
impl<'g> PartialFacts<'g> {
    pub fn declarations(&self) -> impl Iterator<Item = &DeclarationFacts<'g>> {
        self.declarations.values()
    }
    pub fn type_uses(&self) -> impl Iterator<Item = &TypeUse<'g>> {
        self.uses.values()
    }
    pub fn findings(&self) -> impl Iterator<Item = &Finding<'g>> {
        self.findings.values()
    }
    pub fn obligations(&self) -> impl Iterator<Item = &Obligation<'g>> {
        self.obligations.values()
    }
}
#[derive(Debug)]
pub struct Failure<'g> {
    pub kind: FailureKind,
    pub partial: Box<PartialFacts<'g>>,
    pub statistics: Statistics,
    /// Secondary diagnostics can be incomplete, including after a source rejection.
    pub complete_diagnostics: bool,
}
#[derive(Debug)]
pub struct Receipt<'g> {
    graph: &'g Graph,
    facts: PartialFacts<'g>,
    statistics: Statistics,
}
impl<'g> Receipt<'g> {
    pub fn context(&self) -> GraphId {
        self.graph.id()
    }
    pub fn completion(&self) -> &Completion {
        &COMPLETION
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
    pub fn declarations(&self) -> impl Iterator<Item = &DeclarationFacts<'g>> {
        self.facts.declarations()
    }
    pub fn type_uses(&self) -> impl Iterator<Item = &TypeUse<'g>> {
        self.facts.type_uses()
    }
    pub fn declaration(
        &self,
        id: &DeclarationId,
    ) -> Result<Option<&DeclarationFacts<'g>>, QueryError> {
        if !self
            .graph
            .modules()
            .iter()
            .flat_map(|m| &m.declarations)
            .any(|d| d.id == *id)
        {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.facts.declarations.get(id))
    }
    pub fn type_use(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&TypeUse<'g>>, QueryError> {
        let Some(module) = self
            .graph
            .modules()
            .iter()
            .find(|m| std::ptr::eq(*m, module))
        else {
            return Err(QueryError::InvalidRequest);
        };
        let Some(syntax) = module.syntax.nodes.get(node) else {
            return Err(QueryError::InvalidRequest);
        };
        Ok(self
            .facts
            .uses
            .get(&(&module.key, syntax.span.start(), node)))
    }
}

type Work<T> = Result<T, ()>;
enum NameTarget<'g> {
    Parameter(&'g DeclarationId, usize),
    Primitive(&'g Intrinsic),
    Declaration(&'g Declaration),
}
mod defaults;
mod lexical;
use defaults::ActiveDefault;
use lexical::Lexical;

struct Analyzer<'g> {
    graph: &'g Graph,
    complete_defaults: bool,
    active_defaults: Vec<ActiveDefault<'g>>,
    lexical: Option<Arc<Lexical<'g>>>,
    limits: AnalysisLimits,
    statistics: Statistics,
    facts: PartialFacts<'g>,
    declarations: BTreeMap<(&'g ModuleKey, NodeId), &'g Declaration>,
}
impl<'g> Analyzer<'g> {
    fn new(graph: &'g Graph, limits: AnalysisLimits, complete_defaults: bool) -> Self {
        Self {
            graph,
            limits,
            complete_defaults,
            active_defaults: Vec::new(),
            lexical: None,
            statistics: Statistics::default(),
            facts: PartialFacts::default(),
            declarations: BTreeMap::new(),
        }
    }
    fn step(&mut self, amount: usize) -> Work<()> {
        let next = self.statistics.work.checked_add(amount).ok_or(())?;
        if next > self.limits.max_work {
            return Err(());
        }
        self.statistics.work = next;
        Ok(())
    }
    fn reserve(&mut self, amount: usize) -> Work<()> {
        let next = self.statistics.entries.checked_add(amount).ok_or(())?;
        if next > self.limits.max_entries {
            return Err(());
        }
        self.step(amount)?;
        self.statistics.entries = next;
        Ok(())
    }
    fn declaration_at(&mut self, module: &'g Module, node: NodeId) -> Work<&'g Declaration> {
        self.step(self.declarations.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        Ok(self.declarations[&(&module.key, node)])
    }
    fn unsupported(&mut self, module: &'g Module, node: NodeId, reason: Unsupported) -> Work<()> {
        self.reserve(1)?;
        let span = &module.syntax.nodes[node].span;
        self.facts.obligations.insert(
            (&module.key, span.start(), reason),
            Obligation {
                reason,
                module: &module.key,
                node,
                span,
            },
        );
        Ok(())
    }
    fn finding(&mut self, module: &'g Module, node: NodeId, kind: FindingKind) -> Work<()> {
        self.reserve(1)?;
        let span = &module.syntax.nodes[node].span;
        self.facts.findings.insert(
            (&module.key, span.start(), kind),
            Finding {
                kind,
                span,
                candidates: Vec::new(),
                bindings: Vec::new(),
            },
        );
        Ok(())
    }
    fn binding_finding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        kind: FindingKind,
        binding: &'g Binding,
    ) -> Work<()> {
        self.reserve(2)?; // Finding plus retained binding provenance.
        let mut candidates = Vec::new();
        if let Target::Declaration(id) = &binding.target {
            let declaration = self.target_declaration(id)?;
            self.reserve(1)?;
            candidates.push(declaration);
        }
        let span = &module.syntax.nodes[node].span;
        self.facts.findings.insert(
            (&module.key, span.start(), kind),
            Finding {
                kind,
                span,
                candidates,
                bindings: vec![binding],
            },
        );
        Ok(())
    }
    fn declaration_finding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        kind: FindingKind,
        declaration: &'g Declaration,
        bindings: Vec<&'g Binding>,
    ) -> Work<()> {
        self.reserve(2)?; // Finding plus canonical declaration candidate.
        let span = &module.syntax.nodes[node].span;
        self.facts.findings.insert(
            (&module.key, span.start(), kind),
            Finding {
                kind,
                span,
                candidates: vec![declaration],
                bindings,
            },
        );
        Ok(())
    }
    fn owner(&mut self, key: &ModuleKey) -> Work<&'g Module> {
        for module in self.graph.modules() {
            self.step(1)?;
            if module.key == *key {
                return Ok(module);
            }
        }
        unreachable!("a resolved M3 declaration has an actual module")
    }
    fn binders(&mut self, module: &'g Module, header: NodeId) -> Work<Vec<Binder<'g>>> {
        let mut binders: Vec<Binder<'g>> = Vec::new();
        for &child in &module.syntax.nodes[header].children {
            self.step(1)?;
            if !matches!(
                module.syntax.nodes[child].kind,
                NodeKind::GenericParameters { .. }
            ) {
                continue;
            }
            for &node in &module.syntax.nodes[child].children {
                self.step(1)?;
                let declaration = self.declaration_at(module, node)?;
                for previous in &binders {
                    self.step(1)?;
                    if previous.declaration.name == declaration.name {
                        self.finding(module, node, FindingKind::DuplicateBinder)?;
                        break;
                    }
                }
                if !self.complete_defaults && module.syntax.nodes[node].children.len() > 1 {
                    self.unsupported(module, node, Unsupported::GenericDefault)?;
                }
                self.reserve(1)?;
                binders.push(Binder {
                    declaration,
                    position: binders.len(),
                });
            }
        }
        Ok(binders)
    }
    fn ty(
        &mut self,
        module: &'g Module,
        node: NodeId,
        binders: &[Binder<'g>],
        depth: usize,
    ) -> Work<Option<Type<'g>>> {
        self.step(1)?;
        if depth > self.limits.max_depth.min(64) {
            return Err(());
        }
        self.statistics.max_depth = self.statistics.max_depth.max(depth);
        let n = &module.syntax.nodes[node];
        let term = match n.kind {
            NodeKind::ReferenceType { mutable } => {
                let Some(referent) = self.ty(module, n.children[0], binders, depth + 1)? else {
                    return Ok(None);
                };
                TypeTerm::Reference { mutable, referent }
            }
            NodeKind::NamedType => return self.named_type(module, node, binders, depth),
            NodeKind::GroupType => {
                let Some(ty) = self.ty(module, n.children[0], binders, depth + 1)? else {
                    return Ok(None);
                };
                if !self.active_defaults.is_empty() {
                    return Ok(Some(ty));
                }
                self.reserve(1)?;
                self.facts.uses.insert(
                    (&module.key, n.span.start(), node),
                    TypeUse {
                        module: &module.key,
                        node,
                        span: &n.span,
                        ty: ty.clone(),
                        bindings: Vec::new(),
                        substitution: Vec::new(),
                        defaults: Vec::new(),
                    },
                );
                return Ok(Some(ty));
            }
            NodeKind::TupleType | NodeKind::CallableType { .. } => {
                let mut types = Vec::new();
                let mut valid = true;
                for &child in &n.children {
                    if let Some(ty) = self.ty(module, child, binders, depth + 1)? {
                        self.reserve(1)?;
                        types.push(ty);
                    } else {
                        valid = false;
                    }
                }
                if !valid {
                    return Ok(None);
                }
                if let NodeKind::CallableType { mode } = n.kind {
                    let result = types.pop().unwrap();
                    let mode = match mode {
                        None => InvocationMode::ReadOnly,
                        Some("mut") => InvocationMode::Mutable,
                        Some("once") => InvocationMode::Once,
                        _ => unreachable!("M2 callable modes"),
                    };
                    TypeTerm::Callable {
                        mode,
                        parameters: types,
                        result,
                    }
                } else {
                    TypeTerm::Tuple(types)
                }
            }
            _ => unreachable!("M2 annotation type node"),
        };
        self.retain_type(module, node, term, Vec::new(), Vec::new())
            .map(Some)
    }
    fn retain_type(
        &mut self,
        module: &'g Module,
        node: NodeId,
        term: TypeTerm<'g>,
        substitution: Vec<Substitution<'g>>,
        bindings: Vec<&'g Binding>,
    ) -> Work<Type<'g>> {
        self.reserve(2)?; // One immutable term and one written-use record.
        let ty = Arc::new(term);
        if !self.active_defaults.is_empty() {
            return Ok(ty);
        }
        let n = &module.syntax.nodes[node];
        self.facts.uses.insert(
            (&module.key, n.span.start(), node),
            TypeUse {
                module: &module.key,
                node,
                span: &n.span,
                ty: ty.clone(),
                bindings,
                substitution,
                defaults: Vec::new(),
            },
        );
        Ok(ty)
    }
    fn binding(&mut self, module: &ModuleKey, name: &str) -> Work<Option<&'g Binding>> {
        self.namespace_binding(module, Namespace::TypeModule, name)
    }
    fn namespace_binding(
        &mut self,
        module: &ModuleKey,
        namespace: Namespace,
        name: &str,
    ) -> Work<Option<&'g Binding>> {
        for binding in self.graph.bindings() {
            self.step(1)?;
            if binding.module == *module && binding.namespace == namespace && binding.name == name {
                return Ok(Some(binding));
            }
        }
        Ok(None)
    }
    fn target_declaration(&mut self, id: &DeclarationId) -> Work<&'g Declaration> {
        for module in self.graph.modules() {
            self.step(1)?;
            if module.key != id.module {
                continue;
            }
            for declaration in &module.declarations {
                self.step(1)?;
                if declaration.id == *id {
                    return Ok(declaration);
                }
            }
        }
        unreachable!("a resolved M3 binding has an actual declaration")
    }
    fn named_target(
        &mut self,
        module: &'g Module,
        node: NodeId,
        binders: &[Binder<'g>],
        bindings: &mut Vec<&'g Binding>,
    ) -> Work<Option<NameTarget<'g>>> {
        let n = &module.syntax.nodes[node];
        let path = &module.syntax.nodes[n.children[0]];
        let name = std::str::from_utf8(module.syntax.nodes[path.children[0]].span.bytes()).unwrap();
        if self.hidden_default_binder(name)? {
            self.finding(module, node, FindingKind::InvalidDefaultDependency)?;
            return Ok(None);
        }
        let mut target = None;
        for binder in binders {
            self.step(1)?;
            if binder.declaration.name.as_deref() == Some(name) {
                target = Some(NameTarget::Parameter(
                    &binder.declaration.id,
                    binder.position,
                ));
                break;
            }
        }
        if target.is_none()
            && let Some(local) =
                self.lexical_declaration(module, node, Namespace::TypeModule, name)?
        {
            if path.children.len() != 1 {
                self.finding(module, node, FindingKind::NotAType)?;
                return Ok(None);
            }
            target = Some(NameTarget::Declaration(local));
        }
        if target.is_none() {
            let mut binding = self.binding(&module.key, name)?;
            for position in 0..path.children.len() {
                self.step(1)?;
                let Some(found) = binding else {
                    break;
                };
                if position > 0 && !found.public {
                    self.binding_finding(module, node, FindingKind::PrivateType, found)?;
                    return Ok(None);
                }
                self.reserve(1)?;
                bindings.push(found);
                let last = position + 1 == path.children.len();
                match &found.target {
                    Target::Declaration(id) if last => {
                        target = Some(NameTarget::Declaration(self.target_declaration(id)?));
                        break;
                    }
                    Target::Module(key) if !last => {
                        let next = path.children[position + 1];
                        binding = self.binding(
                            key,
                            std::str::from_utf8(module.syntax.nodes[next].span.bytes()).unwrap(),
                        )?;
                    }
                    _ => {
                        self.binding_finding(module, node, FindingKind::NotAType, found)?;
                        return Ok(None);
                    }
                }
            }
            if target.is_none() && path.children.len() == 1 {
                for intrinsic in &self.graph.prelude().intrinsics {
                    self.step(1)?;
                    if intrinsic.namespace() == Namespace::TypeModule && intrinsic.name() == name {
                        target = Some(NameTarget::Primitive(intrinsic));
                        break;
                    }
                }
            }
        }
        let Some(target) = target else {
            self.finding(module, node, FindingKind::MissingType)?;
            return Ok(None);
        };
        if path.children.len() > 1 && matches!(target, NameTarget::Parameter(..)) {
            self.finding(module, node, FindingKind::NotAType)?;
            return Ok(None);
        }
        Ok(Some(target))
    }
    fn written_arguments(
        &mut self,
        module: &'g Module,
        node: NodeId,
        binders: &[Binder<'g>],
        depth: usize,
    ) -> Work<Option<Vec<Type<'g>>>> {
        let n = &module.syntax.nodes[node];
        let mut arguments = Vec::new();
        let mut valid = true;
        for &child in &n.children[1..] {
            if module.syntax.nodes[child].kind == NodeKind::Name {
                continue;
            }
            for &argument in &module.syntax.nodes[child].children {
                if let Some(ty) = self.ty(module, argument, binders, depth + 1)? {
                    self.reserve(1)?;
                    arguments.push(ty);
                } else {
                    valid = false;
                }
            }
        }
        Ok(valid.then_some(arguments))
    }
    fn named_type(
        &mut self,
        module: &'g Module,
        node: NodeId,
        binders: &[Binder<'g>],
        depth: usize,
    ) -> Work<Option<Type<'g>>> {
        let n = &module.syntax.nodes[node];
        let mut bindings = Vec::new();
        let Some(target) = self.named_target(module, node, binders, &mut bindings)? else {
            return Ok(None);
        };
        let Some(arguments) = self.written_arguments(module, node, binders, depth)? else {
            return Ok(None);
        };
        let variant = n
            .children
            .last()
            .copied()
            .filter(|&child| module.syntax.nodes[child].kind == NodeKind::Name);
        let mut arguments = arguments;
        let mut substitution = Vec::new();
        let mut defaults = Vec::new();
        let term = match target {
            NameTarget::Primitive(primitive) => {
                if !arguments.is_empty() {
                    self.finding(module, node, FindingKind::ExcessArguments)?;
                    return Ok(None);
                }
                TypeTerm::Primitive(primitive)
            }
            NameTarget::Parameter(declaration, position) => {
                if !arguments.is_empty() {
                    self.finding(module, node, FindingKind::ExcessArguments)?;
                    return Ok(None);
                }
                if let Some(ty) = self.default_argument(declaration, position) {
                    return Ok(Some(ty));
                }
                TypeTerm::Parameter {
                    declaration,
                    position,
                }
            }
            NameTarget::Declaration(declaration) => {
                if !matches!(
                    declaration.id.kind,
                    DeclarationKind::Record | DeclarationKind::Enum
                ) {
                    self.finding(module, node, FindingKind::NotAType)?;
                    return Ok(None);
                }
                let owner = self.owner(&declaration.id.module)?;
                let mut count = 0;
                for &child in &owner.syntax.nodes[declaration.node].children {
                    self.step(1)?;
                    if !matches!(
                        owner.syntax.nodes[child].kind,
                        NodeKind::GenericParameters { .. }
                    ) {
                        continue;
                    }
                    count = owner.syntax.nodes[child].children.len();
                    if self.complete_defaults && arguments.len() < count {
                        let nominal = self.binders(owner, declaration.node)?;
                        for binder in nominal.iter().skip(arguments.len()) {
                            let Some(default) = self.written_default(owner, binder) else {
                                self.unsupported(module, node, Unsupported::NeedsTypeArguments)?;
                                return Ok(None);
                            };
                            self.reserve(arguments.len() + 2)?;
                            let established: Vec<_> = arguments.iter().cloned().map(Some).collect();
                            let Some(ty) = self.elaborate_default(
                                owner,
                                &nominal,
                                &default,
                                &established,
                                depth + 1,
                            )?
                            else {
                                return Ok(None);
                            };
                            defaults.push(default);
                            arguments.push(ty);
                        }
                    }
                    if count == arguments.len() {
                        for (position, &parameter) in
                            owner.syntax.nodes[child].children.iter().enumerate()
                        {
                            let binder = self.declaration_at(owner, parameter)?;
                            self.reserve(1)?;
                            substitution.push(Substitution {
                                binder: &binder.id,
                                position,
                                argument: arguments[position].clone(),
                            });
                        }
                    }
                }
                if arguments.len() > count {
                    self.declaration_finding(
                        module,
                        node,
                        FindingKind::ExcessArguments,
                        declaration,
                        bindings,
                    )?;
                    return Ok(None);
                }
                if arguments.len() < count {
                    self.unsupported(module, node, Unsupported::NeedsTypeArguments)?;
                    return Ok(None);
                }
                TypeTerm::Nominal {
                    declaration: &declaration.id,
                    arguments,
                }
            }
        };
        let term = if let Some(variant) = variant {
            let TypeTerm::Nominal { declaration, .. } = &term else {
                self.finding(module, node, FindingKind::InvalidVariantParent)?;
                return Ok(None);
            };
            if declaration.kind != DeclarationKind::Enum {
                let parent = self.target_declaration(declaration)?;
                self.declaration_finding(
                    module,
                    node,
                    FindingKind::InvalidVariantParent,
                    parent,
                    bindings,
                )?;
                return Ok(None);
            }
            let owner = self.owner(&declaration.module)?;
            let parent = self.target_declaration(declaration)?;
            let name = module.syntax.nodes[variant].span.bytes();
            let mut candidates = Vec::new();
            for &member in &owner.syntax.nodes[parent.node].children {
                self.step(1)?;
                let syntax = &owner.syntax.nodes[member];
                if syntax.kind == NodeKind::EnumVariant
                    && owner.syntax.nodes[syntax.children[0]].span.bytes() == name
                {
                    let candidate = self.declaration_at(owner, member)?;
                    self.reserve(1)?;
                    candidates.push(candidate);
                }
            }
            let selected = match candidates.as_slice() {
                [] => {
                    self.declaration_finding(
                        module,
                        node,
                        FindingKind::MissingVariant,
                        parent,
                        bindings,
                    )?;
                    return Ok(None);
                }
                [selected] => *selected,
                _ => {
                    self.reserve(1)?;
                    self.facts.findings.insert(
                        (&module.key, n.span.start(), FindingKind::AmbiguousVariant),
                        Finding {
                            kind: FindingKind::AmbiguousVariant,
                            span: &n.span,
                            candidates,
                            bindings,
                        },
                    );
                    return Ok(None);
                }
            };
            self.reserve(1)?;
            TypeTerm::Variant {
                parent: Arc::new(term),
                declaration: &selected.id,
            }
        } else {
            term
        };
        let ty = self.retain_type(module, node, term, substitution, bindings)?;
        if !defaults.is_empty() && self.active_defaults.is_empty() {
            self.facts
                .uses
                .get_mut(&(&module.key, n.span.start(), node))
                .unwrap()
                .defaults = defaults;
        }
        Ok(Some(ty))
    }
    fn member_types(
        &mut self,
        module: &'g Module,
        node: NodeId,
        binders: &[Binder<'g>],
    ) -> Work<Option<Vec<MemberType<'g>>>> {
        let mut members = Vec::new();
        let mut valid = true;
        for &member in &module.syntax.nodes[node].children {
            self.step(1)?;
            let declaration = self.declaration_at(module, member)?;
            if let Some(ty) =
                self.ty(module, module.syntax.nodes[member].children[1], binders, 1)?
            {
                self.reserve(1)?;
                members.push(MemberType {
                    declaration,
                    ty,
                    text_iteration_source: false,
                    option_iteration_source: None,
                });
            } else {
                valid = false;
            }
        }
        Ok(valid.then_some(members))
    }
    fn nominal_binders(
        &mut self,
        module: &'g Module,
        declaration: &'g Declaration,
        metadata: Option<&mut ordinary::metadata::Headers<'g>>,
    ) -> Work<Vec<Binder<'g>>> {
        let binders = self.binders(module, declaration.node)?;
        self.validate_nominal_defaults(module, &binders)?;
        if let Some(metadata) = metadata {
            ordinary::metadata::capture(
                metadata,
                self,
                module,
                declaration,
                declaration.node,
                &binders,
            )?;
        }
        Ok(binders)
    }

    fn analyze(&mut self, metadata: Option<&mut ordinary::metadata::Headers<'g>>) -> Work<()> {
        self.analyze_inventory(metadata, false)
    }

    fn analyze_inventory(
        &mut self,
        mut metadata: Option<&mut ordinary::metadata::Headers<'g>>,
        integrated: bool,
    ) -> Work<()> {
        if integrated {
            self.index_lexical()?;
        }
        for module in self.graph.modules() {
            self.step(1)?;
            for declaration in &module.declarations {
                self.reserve(1)?;
                self.declarations
                    .insert((&module.key, declaration.node), declaration);
            }
        }
        for module in self.graph.modules() {
            for declaration in &module.declarations {
                self.step(1)?;
                if declaration.scope.is_some() && !integrated {
                    continue;
                }
                if declaration.id.kind == DeclarationKind::Implementation && metadata.is_none() {
                    self.unsupported(module, declaration.node, Unsupported::Implementation)?;
                    continue;
                }
                if declaration.id.kind == DeclarationKind::Record {
                    let headers = if integrated {
                        metadata.as_deref_mut()
                    } else {
                        None
                    };
                    let binders = self.nominal_binders(module, declaration, headers)?;
                    let shape = *module.syntax.nodes[declaration.node]
                        .children
                        .last()
                        .unwrap();
                    if let Some(fields) = self.member_types(module, shape, &binders)? {
                        self.reserve(1)?;
                        self.facts.declarations.insert(
                            &declaration.id,
                            DeclarationFacts {
                                declaration,
                                binders,
                                shape: DeclarationShape::Record { fields },
                            },
                        );
                    }
                    continue;
                }
                if declaration.id.kind == DeclarationKind::Enum {
                    let headers = if integrated {
                        metadata.as_deref_mut()
                    } else {
                        None
                    };
                    let binders = self.nominal_binders(module, declaration, headers)?;
                    let mut variants = Vec::new();
                    let mut valid = true;
                    for &child in &module.syntax.nodes[declaration.node].children {
                        self.step(1)?;
                        if module.syntax.nodes[child].kind != NodeKind::EnumVariant {
                            continue;
                        }
                        let member = self.declaration_at(module, child)?;
                        let children = &module.syntax.nodes[child].children;
                        let payload = if children.len() == 1 {
                            Some(Payload::Unit)
                        } else {
                            let payload = children[1];
                            if module.syntax.nodes[payload].kind == NodeKind::RecordPayload {
                                self.member_types(module, payload, &binders)?
                                    .map(Payload::Record)
                            } else {
                                let mut types = Vec::new();
                                let mut complete = true;
                                for &ty in &module.syntax.nodes[payload].children {
                                    if let Some(ty) = self.ty(module, ty, &binders, 1)? {
                                        self.reserve(1)?;
                                        types.push(ty);
                                    } else {
                                        complete = false;
                                    }
                                }
                                complete.then_some(Payload::Tuple(types))
                            }
                        };
                        if let Some(payload) = payload {
                            self.reserve(1)?;
                            variants.push(VariantFacts {
                                declaration: member,
                                payload,
                            });
                        } else {
                            valid = false;
                        }
                    }
                    if valid {
                        self.reserve(1)?;
                        self.facts.declarations.insert(
                            &declaration.id,
                            DeclarationFacts {
                                declaration,
                                binders,
                                shape: DeclarationShape::Enum { variants },
                            },
                        );
                    }
                    continue;
                }
                if !matches!(
                    declaration.id.kind,
                    DeclarationKind::Function
                        | DeclarationKind::Family
                        | DeclarationKind::Implementation
                ) {
                    continue;
                }
                let header = module.syntax.nodes[declaration.node].children[0];
                let binders = self.binders(module, header)?;
                let mut parameters = None;
                let mut result = None;
                for &child in &module.syntax.nodes[header].children {
                    self.step(1)?;
                    match module.syntax.nodes[child].kind {
                        NodeKind::ValueParameters => {
                            parameters = self.member_types(module, child, &binders)?;
                        }
                        NodeKind::Requirements => {
                            self.unsupported(module, child, Unsupported::Requirements)?
                        }
                        NodeKind::Name
                        | NodeKind::QualifiedName
                        | NodeKind::GenericParameters { .. } => {}
                        NodeKind::TypeArguments => {
                            for &argument in &module.syntax.nodes[child].children {
                                self.ty(module, argument, &binders, 1)?;
                            }
                        }
                        _ => {
                            result = self.ty(module, child, &binders, 1)?;
                        }
                    }
                }
                if let Some(metadata) = metadata.as_deref_mut() {
                    ordinary::metadata::capture(
                        metadata,
                        self,
                        module,
                        declaration,
                        header,
                        &binders,
                    )?;
                }
                if let (Some(parameters), Some(result)) = (parameters, result) {
                    self.reserve(1)?;
                    self.facts.declarations.insert(
                        &declaration.id,
                        DeclarationFacts {
                            declaration,
                            binders,
                            shape: DeclarationShape::Function { parameters, result },
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

pub fn analyze_declaration_types(
    graph: &Graph,
    limits: AnalysisLimits,
) -> Result<Receipt<'_>, Failure<'_>> {
    let mut analyzer = Analyzer::new(graph, limits, false);
    let kind = if analyzer.analyze(None).is_err() {
        Some(FailureKind::ResourceLimit)
    } else if !analyzer.facts.obligations.is_empty() {
        Some(FailureKind::Incomplete)
    } else if !analyzer.facts.findings.is_empty() {
        Some(FailureKind::TypeRejected)
    } else {
        None
    };
    if let Some(kind) = kind {
        Err(Failure {
            kind,
            partial: Box::new(analyzer.facts),
            statistics: analyzer.statistics,
            complete_diagnostics: false,
        })
    } else {
        Ok(Receipt {
            graph,
            facts: analyzer.facts,
            statistics: analyzer.statistics,
        })
    }
}
