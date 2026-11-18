//! Scoped M4.3 scalar/control facts. No complete typed-program or ownership receipt.
use super::*;
use crate::lexer::{LiteralMode, NumberForm, Token, TokenKind};
use declarations::DeclarationReceipt;
use std::collections::{BTreeSet, VecDeque};

pub mod data;
pub mod generics;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BodyOwner<'g> {
    Startup,
    Declaration(&'g DeclarationId),
}
#[derive(Clone, Copy, Debug)]
pub struct Site<'g> {
    pub module: &'g ModuleKey,
    pub node: NodeId,
    pub span: &'g Span,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flow {
    pub normal: bool,
    pub returns: bool,
    pub breaks: BTreeSet<NodeId>,
    pub continues: BTreeSet<NodeId>,
    pub diverges: bool,
}
impl Flow {
    fn normal() -> Self {
        Self {
            normal: true,
            ..Self::default()
        }
    }
}
#[derive(Debug)]
pub struct BodyFacts<'g> {
    pub site: Site<'g>,
    pub owner: BodyOwner<'g>,
    pub result: Option<Type<'g>>,
    pub flow: Flow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralValue {
    Integer { magnitude: u128, negative: bool },
    Float32 { bits: u32 },
    Float64 { bits: u64 },
    Boolean(bool),
    Decoded,
    StaticTemplate,
}
#[derive(Clone, Copy, Debug)]
pub struct LiteralEvidence<'g> {
    pub token: Option<&'g Token>,
    pub value: LiteralValue,
    /// A directly negated literal, ignoring grouping. Value includes this sign.
    pub negation: Option<NodeId>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BooleanSet(u8);
impl BooleanSet {
    pub const EMPTY: Self = Self(0);
    pub const FALSE: Self = Self(1);
    pub const TRUE: Self = Self(2);
    pub const BOTH: Self = Self(3);
    pub fn can_be_false(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn can_be_true(self) -> bool {
        self.0 & 2 != 0
    }
    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    fn negate(self) -> Self {
        Self(((self.0 & 1) << 1) | ((self.0 & 2) >> 1))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    #[default]
    Value,
    ReadOnly,
    Writable,
}
#[derive(Debug)]
pub struct ExpressionFacts<'g> {
    pub ty: Type<'g>,
    pub expected: Option<Type<'g>>,
    pub literal: Option<LiteralEvidence<'g>>,
    pub binding: Option<&'g DeclarationId>,
    pub call: Option<CallFacts<'g>>,
    pub ordinary_context: Option<ordinary::InputPremise>,
    pub place: Place,
    pub boolean: Option<BooleanSet>,
    pub data: Option<data::DataExpression<'g>>,
    pub admission: Option<data::Admission<'g>>,
    pub path: Option<data::PlacePath<'g>>,
    pub refinement: Option<Arc<data::Refinement<'g>>>,
    pub read_only_projection: bool,
    pub access: data::Access,
}
#[derive(Debug)]
pub struct CallFacts<'g> {
    pub callee: &'g DeclarationId,
    pub bindings: Vec<&'g Binding>,
    pub substitution: Vec<Substitution<'g>>,
    pub arguments: Vec<NodeId>,
    pub defaults: Vec<DefaultArgumentUse<'g>>,
}
#[derive(Debug)]
pub struct DefaultArgumentUse<'g> {
    pub binder: &'g DeclarationId,
    pub node: NodeId,
    pub span: &'g Span,
}
#[derive(Debug)]
pub struct BindingFacts<'g> {
    pub declaration: &'g DeclarationId,
    pub ty: Option<Type<'g>>,
    pub mutable: bool,
    pub blocked_by: Option<Site<'g>>,
}
#[derive(Debug)]
pub enum NodeFactKind<'g> {
    Statement,
    Annotation(TypeUse<'g>),
    Expression(ExpressionFacts<'g>),
    Return {
        target: &'g DeclarationId,
    },
    Binding(BindingFacts<'g>),
    Pattern(data::PatternFacts<'g>),
    Coverage {
        ty: Type<'g>,
        coverage: data::Coverage<'g>,
    },
    PatternLiteral {
        ty: Type<'g>,
        evidence: LiteralEvidence<'g>,
    },
    Callee {
        declaration: &'g DeclarationId,
    },
    Break {
        target: NodeId,
    },
    Continue {
        target: NodeId,
    },
}
#[derive(Debug)]
pub struct NodeFacts<'g> {
    pub site: Site<'g>,
    pub kind: NodeFactKind<'g>,
    pub flow: Flow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScalarFindingKind {
    ReturnCoverage,
    TypeMismatch,
    LiteralRange,
    NumericContext,
    InsufficientContext,
    InvalidTarget,
    MissingValue,
    PrivateValue,
    StartupCapture,
    DuplicateBinding,
    Arity,
    ReadOnly,
    GuardFallthrough,
    ExpectationBoolean,
    MissingField,
    DuplicateField,
    NonExhaustive,
    RefutableBinding,
    TagChange,
}
impl ScalarFindingKind {
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::TypeMismatch => Some("R0100"),
            Self::LiteralRange => Some("R0002"),
            Self::NumericContext => Some("R0003"),
            Self::MissingValue => Some("R0400"),
            Self::PrivateValue => Some("R0401"),
            Self::StartupCapture => Some("R0502"),
            Self::ExpectationBoolean => Some("R0101"),
            _ => None,
        }
    }
}
#[derive(Debug)]
pub struct ScalarFinding<'g> {
    pub site: Site<'g>,
    pub kind: ScalarFindingKind,
    pub actual: Option<Type<'g>>,
    pub expected: Option<Type<'g>>,
    pub bindings: Vec<&'g Binding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarUnsupported {
    Syntax,
    FamilySelection,
    GenericInference,
    FunctionValue,
    CallableInvocation,
    TypeRelation,
    MutableTemporary,
    Formatting,
    Requirements,
    CoreAdmission,
    ImplicitCopy,
}
#[derive(Debug)]
pub struct ScalarObligation<'g> {
    pub site: Site<'g>,
    pub syntax: NodeKind,
    pub reason: ScalarUnsupported,
    pub declaration: Option<&'g DeclarationId>,
    pub bindings: Vec<&'g Binding>,
}
#[derive(Debug, Default)]
pub struct ScalarFacts<'g> {
    pub bodies: Vec<BodyFacts<'g>>,
    pub nodes: Vec<NodeFacts<'g>>,
    pub findings: Vec<ScalarFinding<'g>>,
    pub obligations: Vec<ScalarObligation<'g>>,
    pub annotations: PartialFacts<'g>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarFailureKind {
    SourceRejected,
    Incomplete,
    ResourceLimit,
}
#[derive(Debug)]
pub struct ScalarFailure<'g> {
    pub kind: ScalarFailureKind,
    pub partial: Box<ScalarFacts<'g>>,
    pub statistics: Statistics,
    pub complete_diagnostics: bool,
}
#[derive(Debug)]
pub struct ScalarReceipt<'r, 'g> {
    input: &'r Receipt<'g>,
    facts: ScalarFacts<'g>,
    statistics: Statistics,
}
impl<'g> ScalarReceipt<'_, 'g> {
    pub fn context(&self) -> GraphId {
        self.input.context()
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }
    pub fn not_performed(&self) -> &'static [Stage] {
        &[
            Stage::DeclarationValidity,
            Stage::SignatureCompatibility,
            Stage::PositionValidity,
            Stage::Inference,
            Stage::Layout,
            Stage::Bodies,
            Stage::Startup,
            Stage::Expectations,
            Stage::Coverage,
            Stage::Evidence,
            Stage::Ownership,
            Stage::Execution,
        ]
    }
    fn module(&self, module: &Module) -> Result<(), QueryError> {
        if self
            .input
            .graph
            .modules()
            .iter()
            .any(|m| std::ptr::eq(m, module))
        {
            Ok(())
        } else {
            Err(QueryError::InvalidRequest)
        }
    }
    pub fn body(
        &self,
        module: &Module,
        owner: BodyOwner<'_>,
    ) -> Result<Option<&BodyFacts<'g>>, QueryError> {
        self.module(module)?;
        if let BodyOwner::Declaration(id) = owner
            && !module.declarations.iter().any(|d| {
                d.id == *id
                    && d.scope.is_none()
                    && matches!(
                        d.id.kind,
                        DeclarationKind::Function | DeclarationKind::Expectation
                    )
            })
        {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self
            .facts
            .bodies
            .iter()
            .find(|b| b.site.module == &module.key && b.owner == owner))
    }
    pub fn node(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&NodeFacts<'g>>, QueryError> {
        self.module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self
            .facts
            .nodes
            .iter()
            .find(|n| n.site.module == &module.key && n.site.node == node))
    }
}
#[derive(Clone, Default)]
struct Slot<'g> {
    ty: Option<Type<'g>>,
    expected: Option<Type<'g>>,
    ordinary_context: Option<ordinary::InputPremise>,
    literal: Option<LiteralEvidence<'g>>,
    blocked: bool,
    binding: Option<&'g DeclarationId>,
    place: Place,
    normal: bool,
    diverges: bool,
    boolean: Option<BooleanSet>,
    admission: Option<data::Admission<'g>>,
    path: Option<data::PlacePath<'g>>,
    refinement: Option<Arc<data::Refinement<'g>>>,
    read_only_projection: bool,
    access: data::Access,
    flow: Flow,
}
struct ExpressionResult<'g> {
    ty: Type<'g>,
    place: Place,
    normal: bool,
    diverges: bool,
    boolean: Option<BooleanSet>,
    flow: Flow,
    access: data::Access,
    path: Option<data::PlacePath<'g>>,
}
#[derive(Clone)]
struct Local<'g> {
    declaration: &'g DeclarationId,
    ty: Option<Type<'g>>,
    mutable: bool,
    boolean: Option<BooleanSet>,
}
#[derive(Clone)]
struct Context<'r, 'g> {
    signature: Option<&'r DeclarationFacts<'g>>,
    scopes: Vec<BTreeMap<&'g str, Local<'g>>>,
    loops: Vec<NodeId>,
    startup: bool,
    views: Vec<Arc<data::Refinement<'g>>>,
}
enum Value<'g> {
    Local(Local<'g>),
    Declaration(&'g Declaration, Vec<&'g Binding>),
}
struct PreparedCall<'g> {
    callee_node: NodeId,
    fact: CallFacts<'g>,
    parameters: Vec<Type<'g>>,
    result: Type<'g>,
    infer: bool,
    family: bool,
}
struct Inference<'g> {
    module: &'g Module,
    slots: BTreeMap<NodeId, Slot<'g>>,
    parents: BTreeMap<NodeId, NodeId>,
    queue: VecDeque<NodeId>,
    calls: BTreeMap<NodeId, PreparedCall<'g>>,
    data: BTreeMap<NodeId, data::PreparedData<'g>>,
    data_nodes: BTreeSet<NodeId>,
    ordinary_tuples: BTreeSet<NodeId>,
}
struct BodyInput<'r, 'g> {
    graph: &'g Graph,
    facts: &'r PartialFacts<'g>,
    ordinary: Option<&'r ordinary::OrdinaryAnalysis<'g>>,
}
struct Checker<'r, 'g> {
    input: BodyInput<'r, 'g>,
    elab: Analyzer<'g>,
    facts: ScalarFacts<'g>,
    data: bool,
    data_depth: usize,
    prepared_calls: BTreeMap<(&'g ModuleKey, NodeId), PreparedCall<'g>>,
    match_inputs: BTreeMap<(&'g ModuleKey, NodeId), Option<ExpressionResult<'g>>>,
    match_hints: BTreeMap<(&'g ModuleKey, NodeId), Option<Type<'g>>>,
    match_blocks: BTreeMap<(&'g ModuleKey, NodeId), Flow>,
    ordinary_calls: BTreeMap<(&'g ModuleKey, NodeId), generics::OrdinaryCall<'g>>,
    conditional_findings: Vec<generics::ConditionalFinding<'g>>,
}
impl<'r, 'g> Checker<'r, 'g> {
    fn new(input: BodyInput<'r, 'g>, limits: AnalysisLimits, data: bool) -> Self {
        Self {
            elab: Analyzer {
                graph: input.graph,
                limits,
                statistics: Statistics::default(),
                facts: PartialFacts::default(),
                declarations: BTreeMap::new(),
            },
            input,
            facts: ScalarFacts::default(),
            data,
            data_depth: 0,
            prepared_calls: BTreeMap::new(),
            match_inputs: BTreeMap::new(),
            match_hints: BTreeMap::new(),
            match_blocks: BTreeMap::new(),
            ordinary_calls: BTreeMap::new(),
            conditional_findings: Vec::new(),
        }
    }

    fn site(module: &'g Module, node: NodeId) -> Site<'g> {
        Site {
            module: &module.key,
            node,
            span: &module.syntax.nodes[node].span,
        }
    }
    fn primitive(&mut self, name: &str) -> Work<Type<'g>> {
        for intrinsic in &self.input.graph.prelude().intrinsics {
            self.elab.step(1)?;
            if intrinsic.namespace() == Namespace::TypeModule && intrinsic.name() == name {
                self.elab.reserve(1)?;
                return Ok(Arc::new(TypeTerm::Primitive(intrinsic)));
            }
        }
        unreachable!("M3 supplies the fixed primitive identities")
    }
    fn defer(
        &mut self,
        module: &'g Module,
        node: NodeId,
        reason: ScalarUnsupported,
    ) -> Work<usize> {
        self.elab.step(self.facts.obligations.len())?;
        if let Some(index) =
            self.facts.obligations.iter().position(|o| {
                o.site.module == &module.key && o.site.node == node && o.reason == reason
            })
        {
            return Ok(index);
        }
        self.elab.reserve(1)?;
        let index = self.facts.obligations.len();
        self.facts.obligations.push(ScalarObligation {
            site: Self::site(module, node),
            syntax: module.syntax.nodes[node].kind,
            reason,
            declaration: None,
            bindings: Vec::new(),
        });
        Ok(index)
    }
    fn unsupported(&mut self, module: &'g Module, node: NodeId) -> Work<()> {
        self.defer(module, node, ScalarUnsupported::Syntax)
            .map(|_| ())
    }
    fn deferred_declaration(
        &mut self,
        module: &'g Module,
        node: NodeId,
        reason: ScalarUnsupported,
        declaration: &'g Declaration,
        bindings: Vec<&'g Binding>,
    ) -> Work<()> {
        let index = self.defer(module, node, reason)?;
        self.facts.obligations[index].declaration = Some(&declaration.id);
        self.facts.obligations[index].bindings = bindings;
        Ok(())
    }
    fn finding(&mut self, module: &'g Module, node: NodeId, kind: ScalarFindingKind) -> Work<()> {
        self.elab.step(self.facts.findings.len())?;
        if !self
            .facts
            .findings
            .iter()
            .any(|f| f.site.module == &module.key && f.site.node == node && f.kind == kind)
        {
            self.elab.reserve(1)?;
            self.facts.findings.push(ScalarFinding {
                site: Self::site(module, node),
                kind,
                actual: None,
                expected: None,
                bindings: Vec::new(),
            });
        }
        Ok(())
    }
    fn lookup_finding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        kind: ScalarFindingKind,
        bindings: Vec<&'g Binding>,
    ) -> Work<()> {
        self.finding(module, node, kind)?;
        self.elab.step(self.facts.findings.len())?;
        let finding = self
            .facts
            .findings
            .iter_mut()
            .find(|f| f.site.module == &module.key && f.site.node == node && f.kind == kind)
            .unwrap();
        finding.bindings = bindings;
        Ok(())
    }
    fn mismatch(
        &mut self,
        module: &'g Module,
        node: NodeId,
        actual: &Type<'g>,
        expected: &Type<'g>,
    ) -> Work<()> {
        self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
        self.elab.step(self.facts.findings.len())?;
        let finding = self
            .facts
            .findings
            .iter_mut()
            .find(|f| {
                f.site.module == &module.key
                    && f.site.node == node
                    && f.kind == ScalarFindingKind::TypeMismatch
            })
            .unwrap();
        finding.actual = Some(actual.clone());
        finding.expected = Some(expected.clone());
        Ok(())
    }
    fn type_depth(&mut self, ty: &Type<'g>, depth: usize) -> Work<()> {
        self.elab.step(1)?;
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        match ty.as_ref() {
            TypeTerm::Reference { referent, .. } => self.type_depth(referent, depth + 1)?,
            TypeTerm::Variant { parent, .. } => self.type_depth(parent, depth + 1)?,
            TypeTerm::Nominal { arguments, .. } | TypeTerm::Tuple(arguments) => {
                for ty in arguments {
                    self.type_depth(ty, depth + 1)?;
                }
            }
            TypeTerm::Callable {
                parameters, result, ..
            } => {
                for ty in parameters {
                    self.type_depth(ty, depth + 1)?;
                }
                self.type_depth(result, depth + 1)?;
            }
            _ => (),
        }
        Ok(())
    }
    fn same(&mut self, a: &Type<'g>, b: &Type<'g>) -> Work<bool> {
        self.type_depth(a, 1)?;
        self.type_depth(b, 1)?;
        Ok(a == b)
    }
    fn enqueue(&mut self, inference: &mut Inference<'g>, node: NodeId) -> Work<()> {
        self.elab.reserve(1)?;
        inference.queue.push_back(node);
        Ok(())
    }
    fn establish(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        ty: Type<'g>,
        expected: bool,
    ) -> Work<()> {
        self.elab
            .step(inference.slots.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        let previous = if expected {
            &inference.slots[&node].expected
        } else {
            &inference.slots[&node].ty
        };
        if let Some(previous) = previous {
            if !self.same(previous, &ty)? && primitive_name(&ty) != Some("never") {
                self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
            return Ok(());
        }
        let slot = inference.slots.get_mut(&node).unwrap();
        let target = if expected {
            &mut slot.expected
        } else {
            &mut slot.ty
        };
        if target.is_none() {
            *target = Some(ty);
            self.enqueue(inference, node)?;
            if let Some(&parent) = inference.parents.get(&node) {
                self.enqueue(inference, parent)?;
            }
        }
        Ok(())
    }
    fn token(&mut self, module: &'g Module, node: NodeId) -> Work<&'g Token> {
        let tokens = &module.syntax.lexed.tokens;
        self.elab
            .step(tokens.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        let start = module.syntax.nodes[node].span.start();
        let index = tokens.partition_point(|t| t.span.start() < start);
        Ok(&tokens[index])
    }
    fn literal_type(
        &mut self,
        module: &'g Module,
        node: NodeId,
        slot: &Slot<'g>,
    ) -> Work<Option<Type<'g>>> {
        let token = self.token(module, node)?;
        let name = match &token.kind {
            TokenKind::Number(number) => {
                if let Some(suffix) = number.suffix {
                    suffix
                } else if let Some(expected) = &slot.expected {
                    if numeric(expected) {
                        return Ok(Some(expected.clone()));
                    }
                    return Ok(None);
                } else {
                    return Ok(None);
                }
            }
            TokenKind::Keyword("true" | "false") => "bool",
            TokenKind::Literal(literal) => {
                if literal.mode == LiteralMode::Character {
                    "char"
                } else {
                    "string"
                }
            }
            _ => unreachable!("M2 ordinary literal"),
        };
        self.primitive(name).map(Some)
    }
    fn literal_value(
        &mut self,
        module: &'g Module,
        node: NodeId,
        ty: &Type<'g>,
        negation: Option<NodeId>,
    ) -> Work<Option<LiteralEvidence<'g>>> {
        let token = self.token(module, node)?;
        let negative = negation.is_some();
        let value = match &token.kind {
            TokenKind::Keyword("true") => LiteralValue::Boolean(true),
            TokenKind::Keyword("false") => LiteralValue::Boolean(false),
            TokenKind::Literal(_) => LiteralValue::Decoded,
            TokenKind::Number(number) => {
                self.elab.reserve(token.span.bytes().len())?;
                let raw = std::str::from_utf8(token.span.bytes()).unwrap();
                let raw = if let Some(suffix) = number.suffix {
                    &raw[..raw.len() - suffix.len()]
                } else {
                    raw
                };
                let text: String = raw.chars().filter(|&c| c != '_').collect();
                let name = primitive_name(ty).unwrap();
                let value = if integer(ty) && number.form != NumberForm::Float {
                    let digits = if number.base == 10 {
                        text.as_str()
                    } else {
                        &text[2..]
                    };
                    let magnitude = u128::from_str_radix(digits, number.base).ok();
                    let width: u32 = name[1..].parse().unwrap();
                    let max = if name.starts_with('i') {
                        let half = 1u128 << (width - 1);
                        if negative { half } else { half - 1 }
                    } else if negative {
                        0
                    } else if width == 128 {
                        u128::MAX
                    } else {
                        (1u128 << width) - 1
                    };
                    magnitude
                        .filter(|&m| m <= max && !(negative && name.starts_with('u')))
                        .map(|magnitude| LiteralValue::Integer {
                            magnitude,
                            negative,
                        })
                } else if number.base == 10 && matches!(name, "f32" | "f64") {
                    if name == "f32" {
                        text.parse::<f32>().ok().filter(|f| f.is_finite()).map(|f| {
                            LiteralValue::Float32 {
                                bits: (if negative { -f } else { f }).to_bits(),
                            }
                        })
                    } else {
                        text.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| {
                            LiteralValue::Float64 {
                                bits: (if negative { -f } else { f }).to_bits(),
                            }
                        })
                    }
                } else {
                    None
                };
                let Some(value) = value else {
                    self.finding(module, node, ScalarFindingKind::LiteralRange)?;
                    return Ok(None);
                };
                value
            }
            _ => unreachable!("validated literal token"),
        };
        Ok(Some(LiteralEvidence {
            token: Some(token),
            value,
            negation,
        }))
    }
    fn binary(&mut self, inference: &mut Inference<'g>, node: NodeId, operator: &str) -> Work<()> {
        let module = inference.module;
        let children = &module.syntax.nodes[node].children;
        let left = children[0];
        let right = children[1];
        let a = inference.slots[&left].ty.clone();
        let b = inference.slots[&right].ty.clone();
        let expected = inference.slots[&node].expected.clone();
        if matches!(operator, "&&" | "||") {
            let boolean = self.primitive("bool")?;
            self.establish(inference, node, boolean.clone(), false)?;
            self.establish(inference, left, boolean.clone(), true)?;
            self.establish(inference, right, boolean, true)?;
            return Ok(());
        }
        if matches!(operator, "==" | "!=" | "<" | "<=" | ">" | ">=") {
            let boolean = self.primitive("bool")?;
            self.establish(inference, node, boolean, false)?;
            let equality = matches!(operator, "==" | "!=");
            if a.as_ref()
                .is_some_and(|t| primitive_name(t) == Some("never"))
                && b.as_ref()
                    .is_some_and(|t| primitive_name(t) == Some("never"))
            {
                self.finding(module, node, ScalarFindingKind::InsufficientContext)?;
            }
            for (known, other) in [(&a, right), (&b, left)] {
                if let Some(ty) = known {
                    if primitive_name(ty) == Some("never") {
                        continue;
                    }
                    let allowed = numeric(ty)
                        || primitive_name(ty) == Some("char")
                        || (equality && primitive_name(ty) == Some("bool"))
                        || text(ty);
                    if !allowed {
                        self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                    }
                    if !text(ty) {
                        self.establish(inference, other, ty.clone(), true)?;
                    }
                }
            }
            if let (Some(a), Some(b)) = (&a, &b)
                && primitive_name(a) != Some("never")
                && primitive_name(b) != Some("never")
                && !(text(a) && text(b))
                && !self.same(a, b)?
            {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            }
            return Ok(());
        }
        let shift = matches!(operator, "<<" | ">>");
        let integral = shift || matches!(operator, "&" | "|" | "^");
        let mut anchor = None;
        for ty in [
            expected.as_ref(),
            a.as_ref(),
            if shift { None } else { b.as_ref() },
        ]
        .into_iter()
        .flatten()
        {
            if (if integral { integer(ty) } else { numeric(ty) }) && anchor.is_none() {
                anchor = Some(ty.clone());
            }
        }
        for ty in [a.as_ref(), if shift { None } else { b.as_ref() }]
            .into_iter()
            .flatten()
        {
            if primitive_name(ty) != Some("never")
                && !(if integral { integer(ty) } else { numeric(ty) })
            {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            }
        }
        if shift {
            let count = self.primitive("u32")?;
            self.establish(inference, right, count, true)?;
        }
        if let Some(ty) = anchor {
            self.establish(inference, node, ty.clone(), false)?;
            self.establish(inference, left, ty.clone(), true)?;
            if !shift {
                self.establish(inference, right, ty, true)?;
            }
        }
        Ok(())
    }
    fn conditional(&mut self, inference: &mut Inference<'g>, node: NodeId) -> Work<()> {
        let children = &inference.module.syntax.nodes[node].children;
        let boolean = self.primitive("bool")?;
        self.establish(inference, children[0], boolean, true)?;
        let a = inference.slots[&children[1]].ty.clone();
        let b = inference.slots[&children[2]].ty.clone();
        if let Some(expected) = inference.slots[&node].expected.clone() {
            self.establish(inference, children[1], expected.clone(), true)?;
            self.establish(inference, children[2], expected, true)?;
        } else {
            for (known, other) in [(&a, children[2]), (&b, children[1])] {
                if let Some(ty) = known
                    && primitive_name(ty) != Some("never")
                    && (self.data || primitive_name(ty).is_some())
                    && inference.slots[&other].ty.is_none()
                {
                    let context = if self.data
                        && let TypeTerm::Variant { parent, .. } = ty.as_ref()
                    {
                        parent.clone()
                    } else {
                        ty.clone()
                    };
                    self.establish(inference, other, context, true)?;
                }
            }
        }
        if let (Some(a), Some(b)) = (a, b) {
            let ty = if primitive_name(&a) == Some("never") {
                b
            } else if primitive_name(&b) == Some("never") || self.same(&a, &b)? {
                a
            } else if let Some(joined) = if self.data {
                self.data_join(&a, &b)?
            } else {
                None
            } {
                joined
            } else {
                if self.deferred_relation(&a, &b)?
                    || self.deferred_relation(&b, &a)?
                    || matches!((a.as_ref(), b.as_ref()), (TypeTerm::Variant { parent: pa, .. }, TypeTerm::Variant { parent: pb, .. }) if pa == pb)
                {
                    self.defer(inference.module, node, ScalarUnsupported::TypeRelation)?;
                } else {
                    self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
                }
                return Ok(());
            };
            self.establish(inference, node, ty, false)?;
        }
        Ok(())
    }
    fn deferred_relation(&mut self, actual: &Type<'g>, expected: &Type<'g>) -> Work<bool> {
        self.type_depth(actual, 1)?;
        self.type_depth(expected, 1)?;
        Ok(match (actual.as_ref(), expected.as_ref()) {
            (TypeTerm::Variant { parent, .. }, _) => parent == expected,
            (
                TypeTerm::Reference {
                    mutable: false,
                    referent: a,
                },
                TypeTerm::Reference {
                    mutable: false,
                    referent: b,
                },
            ) => matches!(a.as_ref(), TypeTerm::Variant { parent, .. } if parent == b),
            (
                TypeTerm::Callable {
                    mode: a,
                    parameters: pa,
                    result: ra,
                },
                TypeTerm::Callable {
                    mode: b,
                    parameters: pb,
                    result: rb,
                },
            ) => {
                pa == pb
                    && ra == rb
                    && matches!(
                        (a, b),
                        (
                            InvocationMode::ReadOnly,
                            InvocationMode::Mutable | InvocationMode::Once
                        ) | (InvocationMode::Mutable, InvocationMode::Once)
                    )
            }
            _ => false,
        })
    }
    fn prefix(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        operator: &str,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let child = inference.module.syntax.nodes[node].children[0];
        if operator == "!" {
            let ty = self.primitive("bool")?;
            self.establish(inference, child, ty.clone(), true)?;
            self.establish(inference, node, ty, false)?;
            return Ok(());
        }
        let actual = inference.slots[&child].ty.clone();
        if matches!(operator, "&" | "&mut") {
            let mutable = operator == "&mut";
            if !mutable
                && let Some(expected) = inference.slots[&node].expected.clone()
                && let TypeTerm::Reference {
                    mutable: false,
                    referent,
                } = expected.as_ref()
            {
                self.establish(inference, child, referent.clone(), true)?;
            }
            if let Some(actual) = actual {
                if mutable {
                    match inference.slots[&child].place {
                        Place::ReadOnly => {
                            self.finding(inference.module, node, ScalarFindingKind::ReadOnly)?
                        }
                        Place::Value => {
                            self.defer(
                                inference.module,
                                node,
                                ScalarUnsupported::MutableTemporary,
                            )?;
                        }
                        Place::Writable => (),
                    }
                }
                let referent = if !mutable
                    && !(self.data && matches!(actual.as_ref(), TypeTerm::Variant { .. }))
                {
                    inference.slots[&child].expected.clone().unwrap_or(actual)
                } else {
                    actual
                };
                self.type_depth(&referent, 2)?;
                self.elab.reserve(1)?;
                self.establish(
                    inference,
                    node,
                    Arc::new(TypeTerm::Reference { mutable, referent }),
                    false,
                )?;
            }
            return Ok(());
        }
        if operator == "*" {
            if let Some(actual) = actual {
                if let TypeTerm::Reference { mutable, referent } = actual.as_ref() {
                    let restricted = inference.slots[&child].read_only_projection;
                    let path = self.project_path(
                        inference.slots[&child].path.as_ref(),
                        data::PathProjection::Dereference,
                    )?;
                    let (ty, refinement) =
                        self.refined(path.as_ref(), referent.clone(), context)?;
                    let slot = inference.slots.get_mut(&node).unwrap();
                    slot.place = if *mutable && !restricted {
                        Place::Writable
                    } else {
                        Place::ReadOnly
                    };
                    slot.read_only_projection = restricted || !mutable;
                    slot.access = if *mutable && !restricted {
                        data::Access::Writable
                    } else {
                        data::Access::ReadOnly
                    };
                    slot.path = path;
                    slot.refinement = refinement;
                    self.establish(inference, node, ty, false)?;
                } else if primitive_name(&actual) != Some("never") {
                    self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
                }
            }
            return Ok(());
        }
        if !matches!(operator, "-" | "~") {
            self.unsupported(inference.module, node)?;
            return Ok(());
        }
        let expected = inference.slots[&node].expected.clone();
        let ty = expected
            .filter(|t| {
                if operator == "~" {
                    integer(t)
                } else {
                    numeric(t)
                }
            })
            .or_else(|| {
                actual.clone().filter(|t| {
                    if operator == "~" {
                        integer(t)
                    } else {
                        numeric(t)
                    }
                })
            });
        if let Some(actual) = &actual
            && primitive_name(actual) != Some("never")
            && (!(if operator == "~" {
                integer(actual)
            } else {
                numeric(actual)
            }) || (operator == "-"
                && primitive_name(actual).is_some_and(|n| n.starts_with('u'))))
        {
            self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
        }
        if let Some(ty) = ty {
            if operator == "-" && primitive_name(&ty).is_some_and(|n| n.starts_with('u')) {
                self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
            }
            self.establish(inference, child, ty.clone(), true)?;
            self.establish(inference, node, ty, false)?;
        }
        Ok(())
    }
    fn negation(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        ty: &Type<'g>,
    ) -> Work<Option<NodeId>> {
        if !numeric(ty) || primitive_name(ty).is_some_and(|n| n.starts_with('u')) {
            return Ok(None);
        }
        let mut child = node;
        while let Some(&parent) = inference.parents.get(&child) {
            self.elab.step(1)?;
            match inference.module.syntax.nodes[parent].kind {
                NodeKind::Group => child = parent,
                NodeKind::Prefix("-") => return Ok(Some(parent)),
                _ => break,
            }
        }
        Ok(None)
    }
    fn marks(&self) -> (usize, usize, usize, usize) {
        (
            self.facts.obligations.len(),
            self.facts.findings.len(),
            self.elab.facts.obligations.len(),
            self.elab.facts.findings.len(),
        )
    }
    fn local(&mut self, context: &Context<'r, 'g>, name: &str) -> Work<Option<Local<'g>>> {
        for scope in context.scopes.iter().rev() {
            self.elab
                .step(scope.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
            if let Some(local) = scope.get(name) {
                return Ok(Some(local.clone()));
            }
        }
        Ok(None)
    }
    fn value_binding(
        &mut self,
        key: &ModuleKey,
        name: &str,
        namespace: Namespace,
    ) -> Work<Option<&'g Binding>> {
        for binding in self.input.graph.bindings() {
            self.elab.step(1)?;
            if binding.module == *key && binding.name == name && binding.namespace == namespace {
                return Ok(Some(binding));
            }
        }
        Ok(None)
    }
    fn value(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<Value<'g>>> {
        let n = &module.syntax.nodes[node];
        let single = [node];
        let path = if n.kind == NodeKind::Name {
            &single[..]
        } else {
            &module.syntax.nodes[n.children[0]].children[..]
        };
        let first = std::str::from_utf8(module.syntax.nodes[path[0]].span.bytes()).unwrap();
        if path.len() == 1
            && let Some(local) = self.local(context, first)?
        {
            return Ok(Some(Value::Local(local)));
        }
        let mut key = &module.key;
        let mut bindings = Vec::new();
        for (index, &segment) in path.iter().enumerate() {
            let last = index + 1 == path.len();
            let name = std::str::from_utf8(module.syntax.nodes[segment].span.bytes()).unwrap();
            let namespace = if last {
                Namespace::ValueFunction
            } else {
                Namespace::TypeModule
            };
            let Some(binding) = self.value_binding(key, name, namespace)? else {
                if path.len() == 1 {
                    let id = match first {
                        "copy" => Some(&self.input.graph.prelude().copy),
                        "dispose" => Some(&self.input.graph.prelude().dispose),
                        _ => None,
                    };
                    if let Some(id) = id {
                        return Ok(Some(Value::Declaration(
                            self.elab.target_declaration(id)?,
                            bindings,
                        )));
                    }
                }
                if path.len() == 1 {
                    for intrinsic in &self.input.graph.prelude().intrinsics {
                        self.elab.step(1)?;
                        if intrinsic.namespace() == Namespace::ValueFunction
                            && intrinsic.name() == first
                        {
                            self.defer(module, node, ScalarUnsupported::Requirements)?;
                            return Ok(None);
                        }
                    }
                }
                self.finding(module, node, ScalarFindingKind::MissingValue)?;
                return Ok(None);
            };
            self.elab.reserve(1)?;
            bindings.push(binding);
            if index > 0 && !binding.public {
                self.lookup_finding(module, node, ScalarFindingKind::PrivateValue, bindings)?;
                return Ok(None);
            }
            match &binding.target {
                Target::Module(next) if !last => key = next,
                Target::Declaration(id) if last && id.kind == DeclarationKind::Binding => {
                    let kind = if !context.startup {
                        ScalarFindingKind::StartupCapture
                    } else {
                        ScalarFindingKind::MissingValue
                    };
                    self.lookup_finding(module, node, kind, bindings)?;
                    return Ok(None);
                }
                Target::Declaration(id) if last => {
                    return Ok(Some(Value::Declaration(
                        self.elab.target_declaration(id)?,
                        bindings,
                    )));
                }
                _ => {
                    self.unsupported(module, node)?;
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }
    fn substitute(
        &mut self,
        ty: &Type<'g>,
        substitutions: &[Substitution<'g>],
        depth: usize,
    ) -> Work<Type<'g>> {
        self.elab.step(1)?;
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        let term = match ty.as_ref() {
            TypeTerm::Parameter { declaration, .. } => {
                for substitution in substitutions {
                    self.elab.step(1)?;
                    if substitution.binder == *declaration {
                        self.type_depth(&substitution.argument, depth)?;
                        return Ok(substitution.argument.clone());
                    }
                }
                return Ok(ty.clone());
            }
            TypeTerm::Primitive(_) => return Ok(ty.clone()),
            TypeTerm::Reference { mutable, referent } => TypeTerm::Reference {
                mutable: *mutable,
                referent: self.substitute(referent, substitutions, depth + 1)?,
            },
            TypeTerm::Variant {
                parent,
                declaration,
            } => TypeTerm::Variant {
                parent: self.substitute(parent, substitutions, depth + 1)?,
                declaration,
            },
            TypeTerm::Nominal {
                declaration,
                arguments,
            } => {
                let arguments = self.substitute_all(arguments, substitutions, depth + 1)?;
                TypeTerm::Nominal {
                    declaration,
                    arguments,
                }
            }
            TypeTerm::Tuple(arguments) => {
                TypeTerm::Tuple(self.substitute_all(arguments, substitutions, depth + 1)?)
            }
            TypeTerm::Callable {
                mode,
                parameters,
                result,
            } => TypeTerm::Callable {
                mode: *mode,
                parameters: self.substitute_all(parameters, substitutions, depth + 1)?,
                result: self.substitute(result, substitutions, depth + 1)?,
            },
        };
        self.elab.reserve(1)?;
        Ok(Arc::new(term))
    }
    fn substitute_all(
        &mut self,
        types: &[Type<'g>],
        substitutions: &[Substitution<'g>],
        depth: usize,
    ) -> Work<Vec<Type<'g>>> {
        self.elab.reserve(types.len())?;
        types
            .iter()
            .map(|t| self.substitute(t, substitutions, depth))
            .collect()
    }
    fn prepare_call(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<PreparedCall<'g>>> {
        if let Some(call) = self.prepared_calls.remove(&(&module.key, node)) {
            return Ok(Some(call));
        }
        let n = &module.syntax.nodes[node];
        let mut callee = n.children[0];
        while module.syntax.nodes[callee].kind == NodeKind::Group {
            self.elab.step(1)?;
            callee = module.syntax.nodes[callee].children[0];
        }
        if module.syntax.nodes[callee].kind != NodeKind::ValueReference {
            self.unsupported(module, node)?;
            return Ok(None);
        }
        let Some(value) = self.value(module, callee, context)? else {
            return Ok(None);
        };
        let Value::Declaration(declaration, bindings) = value else {
            self.defer(module, node, ScalarUnsupported::CallableInvocation)?;
            return Ok(None);
        };
        let family =
            declaration.id.kind == DeclarationKind::Family && self.input.ordinary.is_some();
        if declaration.id.kind != DeclarationKind::Function && !family {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::FamilySelection,
                declaration,
                bindings,
            )?;
            return Ok(None);
        }
        let signature = if family {
            let Some(facts) = self
                .input
                .ordinary
                .unwrap()
                .captured_family(&declaration.id)
            else {
                self.deferred_declaration(
                    module,
                    node,
                    ScalarUnsupported::FamilySelection,
                    declaration,
                    bindings,
                )?;
                return Ok(None);
            };
            &facts.header
        } else {
            &self.input.facts.declarations[&declaration.id]
        };
        let callee_syntax = &module.syntax.nodes[callee];
        let argument_nodes = callee_syntax
            .children
            .get(1)
            .map(|&id| &module.syntax.nodes[id].children[..])
            .unwrap_or(&[]);
        if !family
            && self.input.ordinary.is_some_and(|ordinary| {
                ordinary
                    .headers()
                    .get(&declaration.id)
                    .is_some_and(|header| !header.requirements.is_empty())
            })
        {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::Requirements,
                declaration,
                bindings,
            )?;
            return Ok(None);
        }
        let infer = family || argument_nodes.len() < signature.binders.len();
        if infer && self.input.ordinary.is_none() {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::GenericInference,
                declaration,
                bindings,
            )?;
            return Ok(None);
        }
        if argument_nodes.len() > signature.binders.len() {
            if family {
                self.deferred_declaration(
                    module,
                    node,
                    ScalarUnsupported::FamilySelection,
                    declaration,
                    bindings,
                )?;
            } else {
                self.finding(module, node, ScalarFindingKind::Arity)?;
            }
            return Ok(None);
        }
        let mut substitution = Vec::new();
        for (binder, &argument) in signature.binders.iter().zip(argument_nodes) {
            let Some(argument) = self.elab.ty(
                module,
                argument,
                context.signature.map_or(&[], |s| &s.binders),
                1,
            )?
            else {
                return Ok(None);
            };
            self.elab.reserve(1)?;
            substitution.push(Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument,
            });
        }
        let DeclarationShape::Function { parameters, result } = &signature.shape else {
            unreachable!()
        };
        if parameters.len() != n.children.len() - 1 {
            if family {
                self.deferred_declaration(
                    module,
                    node,
                    ScalarUnsupported::FamilySelection,
                    declaration,
                    bindings,
                )?;
                return Ok(None);
            }
            self.finding(module, node, ScalarFindingKind::Arity)?;
        }
        self.elab.reserve(parameters.len())?;
        let (parameters, result) = if infer {
            (
                parameters.iter().map(|p| p.ty.clone()).collect(),
                result.clone(),
            )
        } else {
            let parameters = parameters
                .iter()
                .map(|p| self.substitute(&p.ty, &substitution, 1))
                .collect::<Work<Vec<_>>>()?;
            (parameters, self.substitute(result, &substitution, 1)?)
        };
        self.elab.reserve(n.children.len())?;
        let arguments = n.children[1..].to_vec();
        if !infer {
            self.record_callee(module, callee, &declaration.id)?;
        }
        Ok(Some(PreparedCall {
            callee_node: callee,
            fact: CallFacts {
                callee: &declaration.id,
                bindings,
                substitution,
                arguments,
                defaults: Vec::new(),
            },
            parameters,
            result,
            infer,
            family,
        }))
    }
    fn record_callee(
        &mut self,
        module: &'g Module,
        node: NodeId,
        declaration: &'g DeclarationId,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        self.facts.nodes.push(NodeFacts {
            site: Self::site(module, node),
            flow: Flow::normal(),
            kind: NodeFactKind::Callee { declaration },
        });
        Ok(())
    }
    fn context(&mut self, signature: Option<&'r DeclarationFacts<'g>>) -> Work<Context<'r, 'g>> {
        self.elab.reserve(1)?;
        let mut scope = BTreeMap::new();
        if let Some(signature) = signature {
            let DeclarationShape::Function { parameters, .. } = &signature.shape else {
                unreachable!()
            };
            for parameter in parameters {
                self.elab.step(1)?;
                if let Some(name) = parameter.declaration.name.as_deref() {
                    self.elab.reserve(1)?;
                    scope.insert(
                        name,
                        Local {
                            declaration: &parameter.declaration.id,
                            ty: Some(parameter.ty.clone()),
                            mutable: false,
                            boolean: None,
                        },
                    );
                }
            }
        }
        Ok(Context {
            signature,
            scopes: vec![scope],
            views: Vec::new(),
            loops: Vec::new(),
            startup: signature.is_none(),
        })
    }
    fn runtime_children(&mut self, inference: &Inference<'g>, node: NodeId) -> Work<Vec<NodeId>> {
        let n = &inference.module.syntax.nodes[node];
        let children = if let Some(prepared) = inference.data.get(&node) {
            &prepared.inputs[..]
        } else if n.kind == NodeKind::Field && self.data {
            &n.children[..1]
        } else if n.kind == NodeKind::Call {
            &n.children[1..]
        } else {
            &n.children[..]
        };
        self.elab.reserve(children.len())?;
        Ok(children
            .iter()
            .copied()
            .filter(|n| inference.slots.contains_key(n))
            .collect())
    }
    fn expression(
        &mut self,
        module: &'g Module,
        root: NodeId,
        expected: Option<Type<'g>>,
        context: &Context<'r, 'g>,
    ) -> Work<Option<ExpressionResult<'g>>> {
        let finding_start = self.facts.findings.len();
        let mut inference = Inference {
            module,
            slots: BTreeMap::new(),
            parents: BTreeMap::new(),
            queue: VecDeque::new(),
            calls: BTreeMap::new(),
            data: BTreeMap::new(),
            data_nodes: BTreeSet::new(),
            ordinary_tuples: BTreeSet::new(),
        };
        self.elab.reserve(1)?;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            self.elab.step(1)?;
            let syntax = &module.syntax.nodes[node];
            self.elab.reserve(1)?;
            inference.slots.insert(node, Slot::default());
            self.enqueue(&mut inference, node)?;
            match syntax.kind {
                NodeKind::Field if self.data => {
                    if self.enum_base(module, syntax.children[0], context)? {
                        self.elab.reserve(1)?;
                        inference.data_nodes.insert(node);
                    } else {
                        let base = syntax.children[0];
                        self.elab.reserve(2)?;
                        inference.parents.insert(base, node);
                        stack.push(base);
                    }
                }
                NodeKind::Construction if self.data => {
                    self.elab.reserve(1)?;
                    inference.data_nodes.insert(node);
                    for child in self.construction_inputs(module, node)? {
                        self.elab.reserve(2)?;
                        inference.parents.insert(child, node);
                        stack.push(child);
                    }
                }
                NodeKind::Bracket if self.data => {
                    self.elab.reserve(1)?;
                    inference.data_nodes.insert(node);
                    for &child in &syntax.children {
                        self.elab.reserve(2)?;
                        inference.parents.insert(child, node);
                        stack.push(child);
                    }
                }
                NodeKind::Index if self.data => {
                    self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                }
                NodeKind::Match if self.data => (),
                NodeKind::Literal => (),
                NodeKind::Template => {
                    self.elab.step(syntax.children.len())?;
                    if syntax
                        .children
                        .iter()
                        .any(|&child| module.syntax.nodes[child].kind != NodeKind::TemplateText)
                    {
                        self.defer(module, node, ScalarUnsupported::Formatting)?;
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                    } else {
                        let ty = self.primitive("string")?;
                        let slot = inference.slots.get_mut(&node).unwrap();
                        slot.ty = Some(ty);
                        slot.literal = Some(LiteralEvidence {
                            token: None,
                            value: LiteralValue::StaticTemplate,
                            negation: None,
                        });
                    }
                }
                NodeKind::ValueReference | NodeKind::Name => {
                    let value = self.value(module, node, context)?;
                    if let Some(Value::Declaration(declaration, bindings)) = &value {
                        self.elab.reserve(bindings.len())?;
                        self.deferred_declaration(
                            module,
                            node,
                            ScalarUnsupported::FunctionValue,
                            declaration,
                            bindings.clone(),
                        )?;
                    }
                    if syntax.children.len() > 1 && matches!(value, Some(Value::Local(_))) {
                        self.defer(module, node, ScalarUnsupported::FunctionValue)?;
                    }
                    let slot = inference.slots.get_mut(&node).unwrap();
                    slot.blocked = true;
                    if (syntax.children.len() == 1 || syntax.kind == NodeKind::Name)
                        && let Some(Value::Local(local)) = value
                    {
                        slot.blocked = local.ty.is_none();
                        slot.ty = local.ty;
                        if self.data {
                            slot.path = Some(data::PlacePath {
                                root: local.declaration,
                                projections: Vec::new(),
                            });
                            if let Some(ty) = slot.ty.take() {
                                let (ty, refinement) =
                                    self.refined(slot.path.as_ref(), ty, context)?;
                                slot.ty = Some(ty);
                                slot.refinement = refinement;
                            }
                        }
                        slot.binding = Some(local.declaration);
                        slot.place = if local.mutable {
                            Place::Writable
                        } else {
                            Place::ReadOnly
                        };
                        slot.boolean = if local.mutable { None } else { local.boolean };
                    }
                }
                NodeKind::Call => {
                    let callee = &module.syntax.nodes[syntax.children[0]];
                    if self.data
                        && callee.kind == NodeKind::Field
                        && self.enum_base(module, callee.children[0], context)?
                    {
                        self.elab.reserve(1)?;
                        inference.data_nodes.insert(node);
                        for &argument in &syntax.children[1..] {
                            self.elab.reserve(2)?;
                            inference.parents.insert(argument, node);
                            stack.push(argument);
                        }
                    } else if let Some(call) = self.prepare_call(module, node, context)? {
                        for &argument in &syntax.children[1..] {
                            self.elab.reserve(2)?;
                            inference.parents.insert(argument, node);
                            stack.push(argument);
                        }
                        self.elab.reserve(1)?;
                        inference.calls.insert(node, call);
                    } else {
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                    }
                }
                NodeKind::Group
                | NodeKind::Binary(_)
                | NodeKind::Prefix(_)
                | NodeKind::IfExpression => {
                    for &child in &syntax.children {
                        self.elab.reserve(2)?;
                        inference.parents.insert(child, node);
                        stack.push(child);
                    }
                }
                _ => {
                    self.unsupported(module, node)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                }
            }
        }
        // An unsupported or previously blocked operand cannot prove missing context in its siblings.
        if inference.slots.values().any(|s| s.blocked) {
            return Ok(None);
        }
        if let Some(expected) = expected {
            self.establish(&mut inference, root, expected, true)?;
        }
        loop {
            while let Some(node) = inference.queue.pop_front() {
                self.elab.step(1)?;
                let slot = inference.slots[&node].clone();
                let syntax = &module.syntax.nodes[node];
                match syntax.kind {
                    NodeKind::Field if self.data => {
                        if inference.data_nodes.contains(&node) {
                            self.variant_step(&mut inference, node, context)?;
                        } else {
                            self.field_projection(&mut inference, node, context)?;
                        }
                    }
                    NodeKind::Construction if self.data => {
                        self.construction_step(&mut inference, node, context)?
                    }
                    NodeKind::Match if self.data => {
                        if slot.expected.is_some() {
                            self.match_step(&mut inference, node, context)?;
                        }
                    }
                    NodeKind::Bracket if self.data => {
                        self.tuple_step(&mut inference, node)?;
                        if inference.ordinary_tuples.contains(&node)
                            && !inference.data.contains_key(&node)
                        {
                            self.gather_ordinary_tuple(&mut inference, node)?;
                        }
                    }
                    NodeKind::Literal if slot.ty.is_none() => {
                        if let Some(ty) = self.literal_type(module, node, &slot)? {
                            self.establish(&mut inference, node, ty, false)?;
                        }
                    }
                    NodeKind::Call => {
                        if inference.data_nodes.contains(&node) {
                            self.variant_step(&mut inference, node, context)?;
                        } else if inference.calls.get(&node).is_some_and(|c| c.family) {
                            self.ordinary_call_step(&mut inference, node)?;
                        } else if inference.calls.get(&node).is_some_and(|c| c.infer) {
                            self.infer_standalone_call(&mut inference, node, false)?;
                        } else if let Some(call) = inference.calls.get(&node) {
                            self.elab.reserve(call.parameters.len())?;
                            let contexts: Vec<_> = call
                                .fact
                                .arguments
                                .iter()
                                .copied()
                                .zip(call.parameters.iter().cloned())
                                .collect();
                            let result = call.result.clone();
                            self.establish(&mut inference, node, result, false)?;
                            for (argument, ty) in contexts {
                                self.establish(&mut inference, argument, ty, true)?;
                            }
                        }
                    }
                    NodeKind::IfExpression => self.conditional(&mut inference, node)?,
                    NodeKind::Prefix(operator) => {
                        self.prefix(&mut inference, node, operator, context)?
                    }
                    NodeKind::Binary(operator) => self.binary(&mut inference, node, operator)?,
                    NodeKind::Group => {
                        let child = syntax.children[0];
                        if let Some(ty) = slot.expected {
                            self.establish(&mut inference, child, ty, true)?;
                        }
                        let path = self.copy_path(inference.slots[&child].path.as_ref())?;
                        let refinement = inference.slots[&child].refinement.clone();
                        let place = inference.slots[&child].place;
                        let restricted = inference.slots[&child].read_only_projection;
                        let access = inference.slots[&child].access;
                        let target = inference.slots.get_mut(&node).unwrap();
                        target.place = place;
                        target.path = path;
                        target.refinement = refinement;
                        target.read_only_projection = restricted;
                        target.access = access;
                        if let Some(ty) = inference.slots[&child].ty.clone() {
                            self.establish(&mut inference, node, ty, false)?;
                        }
                    }
                    _ => (),
                }
            }
            if self.input.ordinary.is_some() && self.apply_ready_call_default(&mut inference)? {
                continue;
            }
            self.elab.reserve(inference.slots.len())?;
            let pending: Vec<_> = inference
                .slots
                .iter()
                .filter_map(|(&node, slot)| {
                    (self.data
                        && module.syntax.nodes[node].kind == NodeKind::Match
                        && !slot.blocked
                        && !inference.data.contains_key(&node))
                    .then_some(node)
                })
                .collect();
            if pending.is_empty() {
                break;
            }
            for node in pending {
                self.match_step(&mut inference, node, context)?;
            }
        }
        if self.input.ordinary.is_some() {
            self.defer_ordinary_calls(&mut inference)?;
        }
        if self.data {
            self.defer_pending_constructions(&mut inference)?;
        }
        // M2 allocates children before parents. No host-stack recursion along expression chains.
        let mut negations = BTreeMap::new();
        for (&node, slot) in &inference.slots {
            if module.syntax.nodes[node].kind == NodeKind::Literal
                && let Some(ty) = &slot.ty
            {
                let negation = self.negation(&inference, node, ty)?;
                self.elab.reserve(1)?;
                negations.insert(node, negation);
            }
        }
        for (&node, slot) in &mut inference.slots {
            self.elab.step(1)?;
            self.elab.step(self.facts.findings.len())?;
            slot.blocked |= self
                .facts
                .findings
                .iter()
                .any(|f| f.site.module == &module.key && f.site.node == node);
            self.elab.step(self.facts.obligations.len())?;
            slot.blocked |= self
                .facts
                .obligations
                .iter()
                .any(|o| o.site.module == &module.key && o.site.node == node);
            if slot.blocked {
                continue;
            }
            if let Some(ty) = &slot.ty {
                if module.syntax.nodes[node].kind == NodeKind::Literal {
                    slot.literal = self.literal_value(module, node, ty, negations[&node])?;
                    slot.blocked |= slot.literal.is_none();
                }
                if let Some(expected) = &slot.expected
                    && primitive_name(ty) != Some("never")
                    && !self.same(ty, expected)?
                {
                    if self.data && self.data_admission(ty, expected)? {
                        slot.admission = Some(data::Admission {
                            actual: ty.clone(),
                            expected: expected.clone(),
                        });
                    } else {
                        if self.deferred_relation(ty, expected)? {
                            self.defer(module, node, ScalarUnsupported::TypeRelation)?;
                        } else {
                            self.mismatch(module, node, ty, expected)?;
                        }
                        slot.blocked = true;
                    }
                }
            } else {
                if module.syntax.nodes[node].kind == NodeKind::Literal {
                    self.finding(module, node, ScalarFindingKind::NumericContext)?;
                } else {
                    self.finding(module, node, ScalarFindingKind::InsufficientContext)?;
                }
                slot.blocked = true;
            }
        }
        let ordered: Vec<_> = {
            self.elab.reserve(inference.slots.len())?;
            inference.slots.keys().copied().collect()
        };
        for &node in &ordered {
            let children = self.runtime_children(&inference, node)?;
            let blocked = children.iter().any(|child| inference.slots[child].blocked);
            let mut normal = inference.slots[&node]
                .ty
                .as_ref()
                .is_some_and(|ty| primitive_name(ty) != Some("never"))
                && children.iter().all(|child| inference.slots[child].normal);
            if module.syntax.nodes[node].kind == NodeKind::IfExpression {
                let children = &module.syntax.nodes[node].children;
                let condition = &inference.slots[&children[0]];
                let truth = if condition.normal {
                    condition.boolean.unwrap_or(BooleanSet::BOTH)
                } else {
                    BooleanSet::EMPTY
                };
                normal = (truth.can_be_true() && inference.slots[&children[1]].normal)
                    || (truth.can_be_false() && inference.slots[&children[2]].normal);
            }
            let kind = module.syntax.nodes[node].kind;
            if let Some(flow) = inference.data.get(&node).and_then(|p| p.flow.as_ref()) {
                normal = flow.normal;
            }
            let child_boolean = |child: NodeId| {
                let slot = &inference.slots[&child];
                if slot.normal {
                    slot.boolean.unwrap_or(BooleanSet::BOTH)
                } else {
                    BooleanSet::EMPTY
                }
            };
            let boolean = if inference.slots[&node]
                .ty
                .as_ref()
                .is_some_and(|t| primitive_name(t) == Some("bool"))
            {
                Some(match kind {
                    NodeKind::Literal => match inference.slots[&node].literal.map(|l| l.value) {
                        Some(LiteralValue::Boolean(true)) => BooleanSet::TRUE,
                        Some(LiteralValue::Boolean(false)) => BooleanSet::FALSE,
                        _ => BooleanSet::BOTH,
                    },
                    NodeKind::Group => child_boolean(children[0]),
                    NodeKind::IfExpression => {
                        let condition = child_boolean(children[0]);
                        (if condition.can_be_true() {
                            child_boolean(children[1])
                        } else {
                            BooleanSet::EMPTY
                        })
                        .union(if condition.can_be_false() {
                            child_boolean(children[2])
                        } else {
                            BooleanSet::EMPTY
                        })
                    }
                    NodeKind::Prefix("!") => child_boolean(children[0]).negate(),
                    NodeKind::Binary("&&") => {
                        let left = child_boolean(children[0]);
                        (if left.can_be_false() {
                            BooleanSet::FALSE
                        } else {
                            BooleanSet::EMPTY
                        })
                        .union(if left.can_be_true() {
                            child_boolean(children[1])
                        } else {
                            BooleanSet::EMPTY
                        })
                    }
                    NodeKind::Binary("||") => {
                        let left = child_boolean(children[0]);
                        (if left.can_be_true() {
                            BooleanSet::TRUE
                        } else {
                            BooleanSet::EMPTY
                        })
                        .union(if left.can_be_false() {
                            child_boolean(children[1])
                        } else {
                            BooleanSet::EMPTY
                        })
                    }
                    _ if !normal => BooleanSet::EMPTY,
                    _ => inference.slots[&node].boolean.unwrap_or(BooleanSet::BOTH),
                })
            } else {
                None
            };
            let flow = self.expression_flow(&inference, node, &children)?;
            let slot = inference.slots.get_mut(&node).unwrap();
            slot.blocked |= blocked;
            slot.normal = boolean.map_or(normal, |b| b != BooleanSet::EMPTY);
            slot.diverges = flow.diverges;
            slot.flow = flow;
            slot.boolean = boolean;
        }
        let mut result = None;
        for (&node, slot) in &inference.slots {
            self.elab.step(1)?;
            if slot.blocked
                || module.syntax.nodes[node]
                    .children
                    .iter()
                    .any(|child| inference.slots.get(child).is_some_and(|s| s.blocked))
            {
                continue;
            }
            if let Some(ty) = &slot.ty {
                self.elab
                    .reserve(1 + 2 * (slot.flow.breaks.len() + slot.flow.continues.len()))?;
                let path = self.copy_path(slot.path.as_ref())?;
                self.facts.nodes.push(NodeFacts {
                    site: Self::site(module, node),
                    flow: slot.flow.clone(),
                    kind: NodeFactKind::Expression(ExpressionFacts {
                        ty: ty.clone(),
                        expected: slot.expected.clone(),
                        literal: slot.literal,
                        binding: slot.binding,
                        call: inference.calls.remove(&node).map(|c| c.fact),
                        ordinary_context: slot.ordinary_context,
                        place: slot.place,
                        boolean: slot.boolean,
                        data: inference.data.remove(&node).map(|p| p.fact),
                        admission: slot.admission.clone(),
                        path,
                        refinement: slot.refinement.clone(),
                        read_only_projection: slot.read_only_projection,
                        access: slot.access,
                    }),
                });
                if node == root {
                    result = Some(ExpressionResult {
                        ty: ty.clone(),
                        place: slot.place,
                        normal: slot.normal,
                        diverges: slot.diverges,
                        boolean: slot.boolean,
                        flow: slot.flow.clone(),
                        access: slot.access,
                        path: self.copy_path(slot.path.as_ref())?,
                    });
                }
            }
        }
        if self.input.ordinary.is_some() {
            self.separate_ordinary_findings(&inference, finding_start)?;
        }
        Ok(result)
    }
    fn sequence(&mut self, mut prefix: Flow, next: &Flow) -> Work<Flow> {
        if prefix.normal {
            prefix.normal = false;
            self.union(prefix, next)
        } else {
            Ok(prefix)
        }
    }
    fn expression_flow(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        children: &[NodeId],
    ) -> Work<Flow> {
        if let Some(flow) = inference.data.get(&node).and_then(|p| p.flow.as_ref()) {
            self.elab
                .reserve(flow.breaks.len() + flow.continues.len())?;
            return Ok(flow.clone());
        }
        let n = &inference.module.syntax.nodes[node];
        let truth = |node| {
            let s = &inference.slots[&node];
            if s.normal {
                s.boolean.unwrap_or(BooleanSet::BOTH)
            } else {
                BooleanSet::EMPTY
            }
        };
        let mut flow = Flow::normal();
        if n.kind == NodeKind::IfExpression {
            let condition = &inference.slots[&children[0]].flow;
            flow = self.union(Flow::default(), condition)?;
            flow.normal = false;
            if truth(children[0]).can_be_true() {
                flow = self.union(flow, &inference.slots[&children[1]].flow)?;
            }
            if truth(children[0]).can_be_false() {
                flow = self.union(flow, &inference.slots[&children[2]].flow)?;
            }
        } else if matches!(n.kind, NodeKind::Binary("&&" | "||")) {
            flow = self.union(Flow::default(), &inference.slots[&children[0]].flow)?;
            let t = truth(children[0]);
            let conjunction = n.kind == NodeKind::Binary("&&");
            flow.normal = if conjunction {
                t.can_be_false()
            } else {
                t.can_be_true()
            };
            if if conjunction {
                t.can_be_true()
            } else {
                t.can_be_false()
            } {
                flow = self.union(flow, &inference.slots[&children[1]].flow)?;
            }
        } else {
            for child in children {
                flow = self.sequence(flow, &inference.slots[child].flow)?;
            }
            if flow.normal
                && inference.slots[&node]
                    .ty
                    .as_ref()
                    .is_some_and(|t| primitive_name(t) == Some("never"))
            {
                flow.normal = false;
                flow.diverges = true;
            }
        }
        Ok(flow)
    }
    fn union(&mut self, mut a: Flow, b: &Flow) -> Work<Flow> {
        self.elab.reserve(b.breaks.len() + b.continues.len())?;
        a.normal |= b.normal;
        a.returns |= b.returns;
        a.diverges |= b.diverges;
        a.breaks.extend(&b.breaks);
        a.continues.extend(&b.continues);
        Ok(a)
    }
    fn scope_may_be_missing_bindings(&self, module: &Module, node: NodeId) -> bool {
        let syntax = &module.syntax.nodes[node];
        self.input.ordinary.is_none()
            || (matches!(syntax.kind, NodeKind::Binding { .. })
                && simple_name(module, syntax.children[0]).is_none()
                && module.syntax.nodes[syntax.children[0]].kind != NodeKind::Wildcard)
    }
    fn source_scope_ready(&mut self, module: &'g Module, root: NodeId) -> Work<bool> {
        self.elab.reserve(1)?;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            self.elab.step(1)?;
            let n = &module.syntax.nodes[node];
            if matches!(
                n.kind,
                NodeKind::Function | NodeKind::Record | NodeKind::Enum
            ) {
                self.unsupported(module, node)?;
                return Ok(false);
            }
            self.elab.reserve(n.children.len())?;
            stack.extend(&n.children);
        }
        Ok(true)
    }
    fn statement(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        self.elab.step(1)?;
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        let n = &module.syntax.nodes[node];
        let before = self.marks();
        let mut flow = Flow::normal();
        let mut kind = NodeFactKind::Statement;
        match n.kind {
            NodeKind::Block => {
                self.elab.reserve(1)?;
                context.scopes.push(BTreeMap::new());
                for &child in &n.children {
                    let obligations = self.facts.obligations.len();
                    let next = self.statement(module, child, depth + 1, context)?;
                    if flow.normal {
                        flow.normal = false;
                        flow = self.union(flow, &next)?;
                    }
                    if obligations != self.facts.obligations.len()
                        && self.scope_may_be_missing_bindings(module, child)
                    {
                        break;
                    }
                }
                context.scopes.pop();
            }
            NodeKind::Binding { mutable } => {
                let pattern = n.children[0];
                let named = simple_name(module, pattern);
                if self.data
                    && named.is_none()
                    && module.syntax.nodes[pattern].kind != NodeKind::Wildcard
                {
                    flow = self.pattern_binding(module, node, mutable, depth + 1, context)?;
                } else if named.is_none() && module.syntax.nodes[pattern].kind != NodeKind::Wildcard
                {
                    self.unsupported(module, node)?;
                } else {
                    let annotation = if n.children.len() == 3 {
                        self.elab.ty(
                            module,
                            n.children[1],
                            context.signature.map_or(&[], |s| &s.binders),
                            1,
                        )?
                    } else {
                        None
                    };
                    let initializer = *n.children.last().unwrap();
                    let inferred =
                        self.expression(module, initializer, annotation.clone(), context)?;
                    flow = if let Some(e) = &inferred {
                        self.union(Flow::default(), &e.flow)?
                    } else {
                        Flow::normal()
                    };
                    let boolean = inferred.as_ref().and_then(|e| e.boolean);
                    let ty = if self.marks() == before {
                        annotation.or(inferred.map(|e| e.ty))
                    } else {
                        None
                    };
                    if let Some(name_node) = named {
                        let declaration = self.elab.declaration_at(module, name_node)?;
                        let name = declaration.name.as_deref().unwrap();
                        let scope = context.scopes.last_mut().unwrap();
                        if scope.contains_key(name) {
                            self.finding(module, node, ScalarFindingKind::DuplicateBinding)?;
                        }
                        self.elab.reserve(1)?;
                        context
                            .scopes
                            .last_mut()
                            .unwrap()
                            .entry(name)
                            .or_insert(Local {
                                declaration: &declaration.id,
                                ty: ty.clone(),
                                mutable,
                                boolean,
                            });
                        self.elab.reserve(1)?;
                        self.facts.nodes.push(NodeFacts {
                            site: Self::site(module, node),
                            flow: flow.clone(),
                            kind: NodeFactKind::Binding(BindingFacts {
                                declaration: &declaration.id,
                                mutable,
                                blocked_by: ty.is_none().then(|| Self::site(module, initializer)),
                                ty,
                            }),
                        });
                    }
                }
            }
            NodeKind::ExpressionStatement => {
                if let Some(expression) = self.expression(module, n.children[0], None, context)? {
                    flow = expression.flow;
                }
            }
            NodeKind::Assignment(operator) => {
                let left = self.expression(module, n.children[0], None, context)?;
                if left.as_ref().is_some_and(|e| e.place != Place::Writable) {
                    self.finding(module, node, ScalarFindingKind::ReadOnly)?;
                }
                let shift = matches!(operator, "<<=" | ">>=");
                let expected = if shift {
                    Some(self.primitive("u32")?)
                } else {
                    left.as_ref().map(|e| e.ty.clone())
                };
                let right = self.expression(module, n.children[1], expected, context)?;
                if self.data
                    && let Some(left) = &left
                {
                    self.tag_change(module, node, left, right.as_ref(), context)?;
                }
                if operator != "="
                    && let Some(left) = &left
                {
                    let valid = if matches!(operator, "&=" | "|=" | "^=" | "<<=" | ">>=") {
                        integer(&left.ty)
                    } else {
                        numeric(&left.ty)
                    };
                    if !valid {
                        self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                    }
                }
                let prefix = if let Some(left) = left {
                    left.flow
                } else {
                    Flow::normal()
                };
                flow = self.sequence(prefix, &right.map_or_else(Flow::normal, |e| e.flow))?;
            }
            NodeKind::IfStatement | NodeKind::While | NodeKind::Guard => {
                if self.data
                    && n.kind == NodeKind::Guard
                    && module.syntax.nodes[n.children[0]].kind == NodeKind::GuardBinding
                {
                    let binding = &module.syntax.nodes[n.children[0]];
                    let annotation = if binding.children.len() == 3 {
                        self.elab.ty(
                            module,
                            binding.children[1],
                            context.signature.map_or(&[], |s| &s.binders),
                            1,
                        )?
                    } else {
                        None
                    };
                    let input = self.expression(
                        module,
                        *binding.children.last().unwrap(),
                        annotation,
                        context,
                    )?;
                    flow = input.map_or_else(Flow::normal, |e| e.flow);
                    if n.children.len() == 2 {
                        self.statement(module, n.children[1], depth + 1, context)?;
                    }
                    self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
                } else if n.kind == NodeKind::Guard
                    && (n.children.len() != 2
                        || module.syntax.nodes[n.children[0]].kind == NodeKind::GuardBinding)
                {
                    self.unsupported(module, node)?;
                } else {
                    if n.kind == NodeKind::While {
                        self.elab.reserve(1)?;
                        context.loops.push(node);
                    }
                    let boolean = self.primitive("bool")?;
                    let condition =
                        self.expression(module, n.children[0], Some(boolean), context)?;
                    let truth = condition.as_ref().map_or(BooleanSet::BOTH, |e| {
                        if e.normal {
                            e.boolean.unwrap_or(BooleanSet::BOTH)
                        } else {
                            BooleanSet::EMPTY
                        }
                    });
                    let body = self.statement(module, n.children[1], depth + 1, context)?;
                    if n.kind == NodeKind::While {
                        context.loops.pop();
                    }
                    flow = condition.map_or_else(Flow::normal, |e| e.flow);
                    flow.normal = false;
                    match n.kind {
                        NodeKind::IfStatement => {
                            let otherwise = if let Some(&other) = n.children.get(2) {
                                self.statement(module, other, depth + 1, context)?
                            } else {
                                Flow::normal()
                            };
                            if truth.can_be_true() {
                                flow = self.union(flow, &body)?;
                            }
                            if truth.can_be_false() {
                                flow = self.union(flow, &otherwise)?;
                            }
                        }
                        NodeKind::Guard => {
                            if body.normal {
                                self.finding(module, node, ScalarFindingKind::GuardFallthrough)?;
                            }
                            if truth.can_be_true() {
                                flow.normal = true;
                            }
                            if truth.can_be_false() {
                                flow = self.union(flow, &body)?;
                            }
                        }
                        NodeKind::While => {
                            let condition_break = flow.breaks.remove(&node);
                            flow.normal = truth.can_be_false() || condition_break;
                            flow.diverges |= flow.continues.remove(&node);
                            if truth.can_be_true() {
                                let mut body = body;
                                flow.normal |= body.breaks.remove(&node);
                                let continued = body.continues.remove(&node);
                                let back = body.normal || continued;
                                body.normal = false;
                                body.diverges |= back;
                                flow = self.union(flow, &body)?;
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
            NodeKind::Break | NodeKind::Continue => {
                if let Some(&target) = context.loops.last() {
                    self.elab.reserve(1)?;
                    flow = Flow::default();
                    if n.kind == NodeKind::Break {
                        flow.breaks.insert(target);
                        kind = NodeFactKind::Break { target };
                    } else {
                        flow.continues.insert(target);
                        kind = NodeFactKind::Continue { target };
                    }
                } else {
                    self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
                }
            }
            NodeKind::Return => {
                if let Some(signature) = context.signature {
                    let DeclarationShape::Function { result, .. } = &signature.shape else {
                        unreachable!()
                    };
                    flow = if let Some(&child) = n.children.first() {
                        self.expression(module, child, Some(result.clone()), context)?
                            .map_or_else(Flow::normal, |e| e.flow)
                    } else {
                        if primitive_name(result) != Some("void") {
                            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                        }
                        Flow::normal()
                    };
                    kind = NodeFactKind::Return {
                        target: &signature.declaration.id,
                    };
                    flow.returns |= flow.normal;
                    flow.normal = false;
                } else {
                    self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
                }
            }
            _ => self.unsupported(module, node)?,
        }
        if before == self.marks() && !matches!(n.kind, NodeKind::Binding { .. }) {
            self.elab
                .reserve(1 + flow.breaks.len() + flow.continues.len())?;
            self.facts.nodes.push(NodeFacts {
                site: Self::site(module, node),
                kind,
                flow: flow.clone(),
            });
        }
        Ok(flow)
    }
    fn annotation_nodes(&mut self) -> Work<()> {
        self.elab.reserve(self.elab.facts.uses.len())?;
        let keys: Vec<_> = self.elab.facts.uses.keys().copied().collect();
        for key in keys {
            let use_ = &self.elab.facts.uses[&key];
            let size = 1 + use_.bindings.len() + use_.substitution.len();
            self.elab.reserve(size)?;
            let use_ = self.elab.facts.uses[&key].clone();
            self.facts.nodes.push(NodeFacts {
                site: Site {
                    module: use_.module,
                    node: use_.node,
                    span: use_.span,
                },
                flow: Flow::default(),
                kind: NodeFactKind::Annotation(use_),
            });
        }
        Ok(())
    }
    fn check_named_body(
        &mut self,
        module: &'g Module,
        typed: &'r DeclarationFacts<'g>,
        pending_role: bool,
    ) -> Work<()> {
        let declaration = typed.declaration;
        let DeclarationShape::Function { result, .. } = &typed.shape else {
            unreachable!()
        };
        let node = module.syntax.nodes[declaration.node].children[1];
        if !self.source_scope_ready(module, node)? {
            return Ok(());
        }
        let before = self.marks();
        if pending_role {
            self.defer(module, node, ScalarUnsupported::FamilySelection)?;
        }
        if let Some(ordinary) = self.input.ordinary
            && let Some(metadata) = ordinary.headers().get(&declaration.id)
        {
            if !metadata.requirements.is_empty() {
                self.defer(module, node, ScalarUnsupported::Requirements)?;
            }
            if metadata.defaults.iter().any(|d| d.ty.is_none()) {
                self.defer(module, node, ScalarUnsupported::GenericInference)?;
            }
        }
        let mut context = self.context(Some(typed))?;
        let flow = self.statement(module, node, 1, &mut context)?;
        if before == self.marks() {
            if flow.normal && primitive_name(result) != Some("void") {
                self.finding(module, node, ScalarFindingKind::ReturnCoverage)?;
            } else {
                self.elab.reserve(1)?;
                self.facts.bodies.push(BodyFacts {
                    site: Self::site(module, node),
                    owner: BodyOwner::Declaration(&declaration.id),
                    result: Some(result.clone()),
                    flow,
                });
            }
        }
        Ok(())
    }

    fn implementation_header(
        &mut self,
        id: &DeclarationId,
    ) -> Work<Option<&'r DeclarationFacts<'g>>> {
        let Some(ordinary) = self.input.ordinary else {
            return Ok(None);
        };
        for source in self.input.graph.implementations() {
            self.elab.step(1)?;
            if source.declaration != *id {
                continue;
            }
            if let Some(family) = ordinary.captured_family(&source.family) {
                self.elab.step(family.implementations.len())?;
                return Ok(family
                    .implementations
                    .iter()
                    .find(|b| b.source.declaration == *id)
                    .map(|b| &b.header));
            }
        }
        Ok(None)
    }

    fn run(&mut self) -> Work<()> {
        for module in self.input.graph.modules() {
            self.elab.step(1)?;
            for declaration in &module.declarations {
                self.elab.reserve(1)?;
                self.elab
                    .declarations
                    .insert((&module.key, declaration.node), declaration);
            }
        }
        for module in self.input.graph.modules() {
            for declaration in &module.declarations {
                self.elab.step(1)?;
                if declaration.scope.is_some() {
                    continue;
                }
                if declaration.id.kind == DeclarationKind::Function {
                    let typed = &self.input.facts.declarations[&declaration.id];
                    self.check_named_body(module, typed, false)?;
                } else if declaration.id.kind == DeclarationKind::Implementation
                    && self.input.ordinary.is_some()
                {
                    if let Some(typed) = self.implementation_header(&declaration.id)? {
                        self.check_named_body(module, typed, true)?;
                    } else {
                        self.defer(module, declaration.node, ScalarUnsupported::FamilySelection)?;
                    }
                } else if declaration.id.kind == DeclarationKind::Expectation {
                    let before = self.marks();
                    let mut context = self.context(None)?;
                    context.startup = false;
                    let boolean = self.primitive("bool")?;
                    let child = module.syntax.nodes[declaration.node].children[0];
                    let expression =
                        self.expression(module, child, Some(boolean.clone()), &context)?;
                    self.elab.step(self.facts.findings.len())?;
                    for finding in &mut self.facts.findings {
                        if finding.site.module == &module.key
                            && finding.site.node == child
                            && finding.kind == ScalarFindingKind::TypeMismatch
                            && finding.actual.is_some()
                        {
                            finding.kind = ScalarFindingKind::ExpectationBoolean;
                        }
                    }
                    if before == self.marks()
                        && let Some(expression) = expression
                    {
                        self.elab.reserve(1)?;
                        self.facts.bodies.push(BodyFacts {
                            site: Self::site(module, declaration.node),
                            owner: BodyOwner::Declaration(&declaration.id),
                            result: Some(boolean),
                            flow: Flow {
                                normal: expression.normal,
                                diverges: expression.diverges,
                                ..Flow::default()
                            },
                        });
                    }
                }
            }
            let before = self.marks();
            let mut context = self.context(None)?;
            let mut flow = Flow::normal();
            for &node in &module.syntax.nodes[module.syntax.root].children {
                self.elab.step(1)?;
                if matches!(
                    module.syntax.nodes[node].kind,
                    NodeKind::Export
                        | NodeKind::Function
                        | NodeKind::Import
                        | NodeKind::Record
                        | NodeKind::Enum
                        | NodeKind::Expect
                ) {
                    continue;
                }
                if self.input.ordinary.is_some()
                    && module.syntax.nodes[node].kind == NodeKind::Implementation
                {
                    continue;
                }
                if !self.source_scope_ready(module, node)? {
                    break;
                }
                let obligations = self.facts.obligations.len();
                let next = self.statement(module, node, 1, &mut context)?;
                if flow.normal {
                    flow.normal = false;
                    flow = self.union(flow, &next)?;
                }
                if obligations != self.facts.obligations.len()
                    && self.scope_may_be_missing_bindings(module, node)
                {
                    break;
                }
            }
            if before == self.marks() {
                self.elab.reserve(1)?;
                self.facts.bodies.push(BodyFacts {
                    site: Self::site(module, module.syntax.root),
                    owner: BodyOwner::Startup,
                    result: None,
                    flow,
                });
            }
        }
        Ok(())
    }
}
fn simple_name(module: &Module, node: NodeId) -> Option<NodeId> {
    let n = &module.syntax.nodes[node];
    if n.kind != NodeKind::NamePattern {
        return None;
    }
    let path = &module.syntax.nodes[n.children[0]];
    if path.children.len() != 1 {
        return None;
    }
    let qualified = &module.syntax.nodes[path.children[0]];
    (qualified.children.len() == 1).then_some(qualified.children[0])
}
fn integer(ty: &Type<'_>) -> bool {
    matches!(
        primitive_name(ty),
        Some("i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128")
    )
}
fn text(ty: &Type<'_>) -> bool {
    primitive_name(ty) == Some("string")
        || matches!(ty.as_ref(), TypeTerm::Reference { referent, .. } if primitive_name(referent) == Some("string"))
}
fn numeric(ty: &Type<'_>) -> bool {
    integer(ty) || matches!(primitive_name(ty), Some("f32" | "f64"))
}
fn primitive_name(ty: &Type<'_>) -> Option<&'static str> {
    if let TypeTerm::Primitive(p) = ty.as_ref() {
        Some(p.name())
    } else {
        None
    }
}

pub fn check_scalars<'r, 'g>(
    input: &DeclarationReceipt<'r, 'g>,
    limits: AnalysisLimits,
) -> Result<ScalarReceipt<'r, 'g>, ScalarFailure<'g>> {
    check(input, limits, false)
}
fn check<'r, 'g>(
    input: &DeclarationReceipt<'r, 'g>,
    limits: AnalysisLimits,
    data: bool,
) -> Result<ScalarReceipt<'r, 'g>, ScalarFailure<'g>> {
    let mut checker = Checker::new(
        BodyInput {
            graph: input.types.graph,
            facts: &input.types.facts,
            ordinary: None,
        },
        limits,
        data,
    );
    let exhausted = checker
        .run()
        .and_then(|_| checker.annotation_nodes())
        .is_err();
    checker.facts.annotations = checker.elab.facts;
    let statistics = checker.elab.statistics;
    let kind = if exhausted {
        Some(ScalarFailureKind::ResourceLimit)
    } else if !checker.facts.obligations.is_empty()
        || !checker.facts.annotations.obligations.is_empty()
    {
        Some(ScalarFailureKind::Incomplete)
    } else if !checker.facts.findings.is_empty() || !checker.facts.annotations.findings.is_empty() {
        Some(ScalarFailureKind::SourceRejected)
    } else {
        None
    };
    if let Some(kind) = kind {
        Err(ScalarFailure {
            kind,
            partial: Box::new(checker.facts),
            statistics,
            complete_diagnostics: false,
        })
    } else {
        Ok(ScalarReceipt {
            input: input.types,
            facts: checker.facts,
            statistics,
        })
    }
}
