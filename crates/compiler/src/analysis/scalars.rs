//! Scoped M4.3 scalar/control facts. No complete typed-program or ownership receipt.
use super::typed::CollectionCore;
use super::*;
use crate::lexer::{LiteralMode, NumberForm, Token, TokenKind};
use declarations::DeclarationReceipt;
use std::collections::{BTreeSet, VecDeque};

mod copying;
mod core;
pub mod data;
mod formatting;
pub mod generics;
mod iteration;
mod numeric_conversion;

use generics::callables::{
    ArrowFacts, ArrowHead, ArrowReturn, CallableContext, CallableNodeFacts, FunctionValueFacts,
    InvocationFacts,
};

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
    /// A reachable prerequisite prevents a complete control-flow judgment.
    pub pending: bool,
    pub returns: bool,
    /// At least one feasible path evaluates a postfix `?` (including known Some).
    pub reaches_postfix: bool,
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
impl BodyFacts<'_> {
    fn owned_by(&self, module: &Module, owner: BodyOwner<'_>) -> bool {
        self.site.module == &module.key && self.owner == owner
    }
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
    pub call: Option<Box<CallFacts<'g>>>,
    pub ordinary_context: Option<ordinary::InputPremise>,
    pub place: Place,
    pub boolean: Option<BooleanSet>,
    pub data: Option<data::DataExpression<'g>>,
    pub admission: Option<data::Admission<'g>>,
    pub path: Option<data::PlacePath<'g>>,
    pub refinement: Option<Arc<data::Refinement<'g>>>,
    pub read_only_projection: bool,
    pub access: data::Access,
    /// The written shape is `local * literal`, or a local or that product
    /// plus a local. M5 still resolves the actual scalar slots.
    pub(in crate::analysis) bounded_i32_arithmetic: bool,
    /// The written shape is `left == right` on two `i32` operands. M5 still
    /// checks that each operand is normal and lowers it.
    pub(in crate::analysis) i32_equality: bool,
    /// The written shape is `left && right` where each operand, through
    /// groups, has the `i32_equality` shape (ri-e9t8).
    pub(in crate::analysis) i32_equality_conjunction: bool,
}
pub use core::{
    ErrorConstruction, Propagation, PropagationError, PropagationTarget, RetainedError, Wrapping,
};
pub use formatting::FormattingCall;
pub use iteration::Iteration;
pub(in crate::analysis) use iteration::IterationBodyKind;

#[derive(Debug)]
pub struct CallFacts<'g> {
    /// Closed per-slot evidence from Result-use on this exact call, not a join contract.
    pub(in crate::analysis) list_arrows: Option<Box<super::typed::PairCall<'g>>>,
    pub(in crate::analysis) scoped_join: Option<Box<super::typed::ScopedJoinCall<'g>>>,
    pub callee: &'g DeclarationId,
    pub bindings: Vec<&'g Binding>,
    pub substitution: Vec<Substitution<'g>>,
    /// The selected ordinary body and its own binder substitution, when proved.
    pub selection: Option<generics::GoalSelection<'g>>,
    pub arguments: Vec<NodeId>,
    pub defaults: Vec<DefaultArgumentUse<'g>>,
    pub pipe_input: Option<NodeId>,
}
impl CallFacts<'_> {
    /// Argument evaluation order. A pipe evaluates its input before the written arguments.
    pub fn argument_evaluation_order(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.pipe_input.into_iter().chain(
            self.arguments
                .iter()
                .copied()
                .filter(|&node| self.pipe_input != Some(node)),
        )
    }
}
pub use super::DefaultArgumentUse;
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
    Iteration(Iteration<'g>),
    Annotation(TypeUse<'g>),
    Expression(ExpressionFacts<'g>),
    Return {
        target: &'g DeclarationId,
    },
    ArrowReturn {
        target: NodeId,
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
    pub callable: Option<Box<CallableNodeFacts<'g>>>,
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
    MissingRequirement,
    NoApplicableImplementation,
    AmbiguousImplementation,
    DuplicateImplementation,
    UnauthorizedImplementation,
    MissingConversionPair,
    ConversionErrorMismatch,
    AmbiguousConversionPair,
    ProtectedNumericGap,
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
    InvalidTupleIndex,
}
impl ScalarFindingKind {
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::TypeMismatch => Some("R0100"),
            Self::LiteralRange => Some("R0002"),
            Self::NumericContext => Some("R0003"),
            Self::MissingValue => Some("R0400"),
            Self::NoApplicableImplementation => Some("R0300"),
            Self::MissingRequirement => Some("R0302"),
            Self::MissingConversionPair | Self::ProtectedNumericGap => Some("R0314"),
            Self::ConversionErrorMismatch => Some("R0313"),
            Self::AmbiguousImplementation | Self::AmbiguousConversionPair => Some("R0303"),
            Self::DuplicateImplementation => Some("R0304"),
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
    pub related: Option<Site<'g>>,
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
    CaptureMode,
    TypeRelation,
    MutableTemporary,
    Formatting,
    Requirements,
    CoreAdmission,
    CoreProtection,
    ConversionAdmission,
    DeclarationPrerequisite,
    BindingType,
    MatchCoverage,
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
    /// Pending per-formal handoffs for the integrated report, by function and position.
    text_iteration_sources: Vec<(&'g DeclarationId, usize)>,
    pub bodies: Vec<BodyFacts<'g>>,
    pub nodes: Vec<NodeFacts<'g>>,
    pub findings: Vec<ScalarFinding<'g>>,
    pub obligations: Vec<ScalarObligation<'g>>,
    pub annotations: PartialFacts<'g>,
    pub arrows: BTreeMap<(&'g ModuleKey, NodeId), Arc<ArrowFacts<'g>>>,
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
            Stage::CoreAdmission,
            Stage::ResultUse,
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
        Ok(self.facts.bodies.iter().find(|b| b.owned_by(module, owner)))
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
    nominal_tuple: bool,
    binding: Option<&'g DeclarationId>,
    place: Place,
    normal: bool,
    diverges: bool,
    boolean: Option<BooleanSet>,
    propagation: Option<Propagation<'g>>,
    pending_core: bool,
    installed_operation: Option<data::DataExpression<'g>>,
    scoped_result_determination: bool,
    pipe_target: Option<(PropagationTarget<'g>, Option<Type<'g>>)>,
    pipe_expected: Option<Type<'g>>,
    admission: Option<data::Admission<'g>>,
    path: Option<data::PlacePath<'g>>,
    refinement: Option<Arc<data::Refinement<'g>>>,
    read_only_projection: bool,
    access: data::Access,
    flow: Flow,
    i32_operand: I32Operand,
    invocation: Option<Arc<InvocationFacts<'g>>>,
    function_value: Option<Arc<FunctionValueFacts<'g>>>,
    arrow: Option<Arc<ArrowFacts<'g>>>,
}
/// Operand shape of a normal `i32` expression, classified from its children
/// during the flow pass so that M5 admits arithmetic without a lookup.
#[derive(Clone, Copy, Default)]
enum I32Operand {
    #[default]
    Other,
    Local,
    Literal,
    Product,
    Sum,
    /// A read of one component of a local or parameter tuple, `pair[0]`.
    TupleElement,
}
struct ExpressionResult<'g> {
    ty: Type<'g>,
    list_literal: Option<NodeId>,
    place: Place,
    normal: bool,
    diverges: bool,
    boolean: Option<BooleanSet>,
    flow: Flow,
    access: data::Access,
    path: Option<data::PlacePath<'g>>,
    arrow: Option<Arc<ArrowFacts<'g>>>,
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
    inherited_scopes: usize,
    loops: Vec<NodeId>,
    startup: bool,
    arrow_return: Option<ArrowReturn<'g>>,
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
    closed_error: Option<core::ClosedErr<'g>>,
}
struct PipeInvocation {
    receiver: NodeId,
    arguments: Arc<[NodeId]>,
}
struct Inference<'g> {
    module: &'g Module,
    slots: BTreeMap<NodeId, Slot<'g>>,
    parents: BTreeMap<NodeId, NodeId>,
    queue: VecDeque<NodeId>,
    calls: BTreeMap<NodeId, PreparedCall<'g>>,
    calls_waiting_for_result: BTreeSet<NodeId>,
    conversion_requirements: BTreeMap<(NodeId, NodeId), generics::GoalId>,
    conversion_clause_calls: BTreeSet<NodeId>,
    arrows: BTreeMap<NodeId, ArrowHead<'g>>,
    function_values: BTreeMap<NodeId, FunctionValueFacts<'g>>,
    callable_contexts: BTreeMap<NodeId, Arc<CallableContext<'g>>>,
    value_calls: BTreeSet<NodeId>,
    pipe_invocations: BTreeMap<NodeId, PipeInvocation>,
    pending_invocations: BTreeSet<NodeId>,
    data: BTreeMap<NodeId, data::PreparedData<'g>>,
    data_nodes: BTreeSet<NodeId>,
    ordinary_tuples: BTreeSet<NodeId>,
    nominals: BTreeMap<NodeId, data::nominal::PreparedNominal<'g>>,
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
    reached_bodies: bool,
    reached_startup: bool,
    reached_expectations: bool,
    // Accumulate the written body shape while an iteration is being checked.
    iteration_body: iteration::IterationBody<'g>,
    data: bool,
    integrated: bool,
    option: Option<super::typed::OptionRole<'g>>,
    result: Option<super::typed::ResultRole<'g>>,
    show: Option<&'g Declaration>,
    primitives: Option<&'r super::typed::PrimitiveCore<'g>>,
    installed_conversions: Option<&'r super::typed::ConversionCore<'g>>,
    collections: CollectionCore<'g>,
    native_ownership: &'r [super::typed::OwnershipProtection<'g>],
    ownership_overlaps: &'r [super::typed::OwnershipOverlap<'g>],
    ordinary_scope: Option<super::typed::InstalledOrdinaryScope<'g>>,
    data_depth: usize,
    evidence: generics::Evidence<'g>,
    conversions: Option<&'r BTreeMap<&'g DeclarationId, super::conversions::ConversionFamily<'g>>>,
    conversion_work: BTreeMap<usize, generics::ConversionWork<'r, 'g>>,
    arrow_depth: usize,
    arrow_premise: Option<ordinary::InputPremise>,
    arrow_bindings: BTreeMap<&'g DeclarationId, Arc<ArrowFacts<'g>>>,
    prepared_calls: BTreeMap<(&'g ModuleKey, NodeId), PreparedCall<'g>>,
    match_inputs: BTreeMap<(&'g ModuleKey, NodeId), Option<ExpressionResult<'g>>>,
    match_hints: BTreeMap<(&'g ModuleKey, NodeId), Option<Type<'g>>>,
    match_blocks: BTreeMap<(&'g ModuleKey, NodeId), Flow>,
    ordinary_calls: BTreeMap<(&'g ModuleKey, NodeId), generics::OrdinaryCall<'g>>,
    ordinary_values: BTreeMap<(&'g ModuleKey, NodeId), generics::OrdinaryValue<'g>>,
    conditional_findings: Vec<generics::ConditionalFinding<'g>>,
    conversion_uses: BTreeMap<(&'g ModuleKey, NodeId), generics::ConversionUse<'g>>,
    conditional_conversion_nodes: BTreeMap<(&'g ModuleKey, NodeId), NodeFacts<'g>>,
}
impl<'r, 'g> Checker<'r, 'g> {
    fn new(input: BodyInput<'r, 'g>, limits: AnalysisLimits, data: bool) -> Self {
        Self {
            elab: Analyzer::new(input.graph, limits, input.ordinary.is_some()),
            input,
            facts: ScalarFacts::default(),
            reached_bodies: false,
            reached_startup: false,
            reached_expectations: false,
            iteration_body: iteration::IterationBody::default(),
            data,
            integrated: false,
            option: None,
            result: None,
            show: None,
            primitives: None,
            installed_conversions: None,
            collections: Default::default(),
            native_ownership: &[],
            ownership_overlaps: &[],
            ordinary_scope: None,
            data_depth: 0,
            evidence: generics::Evidence::default(),
            conversions: None,
            conversion_work: BTreeMap::new(),
            arrow_depth: 0,
            arrow_premise: None,
            arrow_bindings: BTreeMap::new(),
            prepared_calls: BTreeMap::new(),
            match_inputs: BTreeMap::new(),
            match_hints: BTreeMap::new(),
            match_blocks: BTreeMap::new(),
            ordinary_calls: BTreeMap::new(),
            ordinary_values: BTreeMap::new(),
            conditional_findings: Vec::new(),
            conversion_uses: BTreeMap::new(),
            conditional_conversion_nodes: BTreeMap::new(),
        }
    }

    fn record_node(&mut self, fact: NodeFacts<'g>) {
        if self.arrow_premise.is_some_and(|p| p.has_conversion()) {
            self.conditional_conversion_nodes
                .insert((fact.site.module, fact.site.node), fact);
        } else {
            self.facts.nodes.push(fact);
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
            // Prepay the recovery scan if an integrated expression runs out of budget.
            if self.integrated {
                self.elab.step(1)?;
            }
            self.elab.reserve(1)?;
            self.facts.findings.push(ScalarFinding {
                site: Self::site(module, node),
                kind,
                related: None,
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
    fn with_node_premise<T>(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        check: impl FnOnce(&mut Self, &mut Inference<'g>) -> Work<T>,
    ) -> Work<T> {
        let prior = self.arrow_premise;
        self.arrow_premise = self.node_premise(&inference.slots[&node]);
        let result = check(self, inference);
        self.arrow_premise = prior;
        result
    }

    fn node_premise(&self, slot: &Slot<'g>) -> Option<ordinary::InputPremise> {
        let premise = slot.ordinary_context.filter(|p| p.has_conversion());
        match (self.arrow_premise, premise) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (a, b) => a.or(b),
        }
    }

    fn mark_conversion_dependency(
        &mut self,
        inference: &mut Inference<'g>,
        mut node: NodeId,
        premise: ordinary::InputPremise,
        ancestors: bool,
    ) -> Work<()> {
        loop {
            self.elab.step(1)?;
            let slot = inference.slots.get_mut(&node).unwrap();
            slot.ordinary_context = Some(
                slot.ordinary_context
                    .map_or(premise, |p| p.combine(premise)),
            );
            if !ancestors {
                break;
            }
            let Some(&parent) = inference.parents.get(&node) else {
                break;
            };
            node = parent;
        }
        Ok(())
    }

    fn declaration_unusable(&mut self, declaration: &DeclarationId) -> Work<bool> {
        if !self.integrated {
            return Ok(false);
        }
        self.elab.step(1)?;
        Ok(self
            .input
            .ordinary
            .is_some_and(|o| o.declaration_unusable(declaration)))
    }

    fn type_unavailable(&mut self, ty: &Type<'g>, depth: usize) -> Work<bool> {
        if !self.integrated {
            return Ok(false);
        }
        self.elab.step(1)?;
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        if let TypeTerm::Nominal { declaration, .. } = ty.as_ref()
            && (self.declaration_unusable(declaration)?
                || !self.input.facts.declarations.contains_key(declaration))
        {
            return Ok(true);
        }
        for child in ordinary::shape(ty).1 {
            if self.type_unavailable(child, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn signature_unavailable(&mut self, signature: &DeclarationFacts<'g>) -> Work<bool> {
        if !self.integrated {
            return Ok(false);
        }
        if self.declaration_unusable(&signature.declaration.id)? {
            return Ok(true);
        }
        let DeclarationShape::Function { parameters, result } = &signature.shape else {
            unreachable!()
        };
        for ty in parameters
            .iter()
            .map(|p| &p.ty)
            .chain(std::iter::once(result))
        {
            if self.type_unavailable(ty, 1)? {
                return Ok(true);
            }
        }
        Ok(false)
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
        if self.type_unavailable(&ty, 1)? {
            self.defer(
                inference.module,
                node,
                ScalarUnsupported::DeclarationPrerequisite,
            )?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let pipe = self.integrated && Self::propagating_pipe(inference.module, node);
        let ty = if pipe && !expected {
            let Some(payload) = self.pipe_propagation_output(inference, node, &ty)? else {
                return Ok(());
            };
            payload
        } else {
            ty
        };
        let previous = if pipe && expected {
            &inference.slots[&node].pipe_expected
        } else if expected {
            &inference.slots[&node].expected
        } else {
            &inference.slots[&node].ty
        };
        if let Some(previous) = previous {
            if !self.same(previous, &ty)? && primitive_name(&ty) != Some("never") {
                if let Some(premise) = self.arrow_premise.filter(|p| p.has_conversion()) {
                    self.mark_conversion_dependency(inference, node, premise, false)?;
                }
                self.finding(inference.module, node, ScalarFindingKind::TypeMismatch)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
            return Ok(());
        }
        if let Some(premise) = self.arrow_premise.filter(|p| p.has_conversion()) {
            self.mark_conversion_dependency(inference, node, premise, !expected)?;
        }
        let slot = inference.slots.get_mut(&node).unwrap();
        let target = if pipe && expected {
            &mut slot.pipe_expected
        } else if expected {
            &mut slot.expected
        } else {
            &mut slot.ty
        };
        if target.is_none() {
            *target = Some(ty);
            if pipe && expected {
                // The receiving context belongs to the unwrapped result, never
                // to the underlying invocation or its candidate selection.
                return Ok(());
            }
            self.enqueue(inference, node)?;
            let mut child = node;
            while let Some(&parent) = inference.parents.get(&child) {
                self.enqueue(inference, parent)?;
                if self.input.ordinary.is_none()
                    || !matches!(
                        inference.module.syntax.nodes[parent].kind,
                        NodeKind::Group | NodeKind::Bracket
                    )
                {
                    break;
                }
                child = parent;
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
                inference.slots.get_mut(&node).unwrap().arrow =
                    inference.slots[&child].arrow.clone();
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
    fn marks(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.facts.obligations.len(),
            self.facts.findings.len(),
            self.elab.facts.obligations.len(),
            self.elab.facts.findings.len(),
            self.conditional_findings.len(),
        )
    }
    fn local(&mut self, context: &Context<'r, 'g>, name: &str) -> Work<Option<(usize, Local<'g>)>> {
        for (depth, scope) in context.scopes.iter().enumerate().rev() {
            self.elab
                .step(scope.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
            if let Some(local) = scope.get(name) {
                return Ok(Some((depth, local.clone())));
            }
        }
        Ok(None)
    }
    fn shadowing_local(
        &mut self,
        context: &Context<'r, 'g>,
        name: &str,
        lexical: Option<&'g Declaration>,
    ) -> Work<Option<(usize, Local<'g>)>> {
        let local = self.local(context, name)?;
        if let (Some(declaration), Some((_, binding))) = (lexical, &local)
            && declaration.scope > self.elab.target_declaration(binding.declaration)?.scope
        {
            return Ok(None);
        }
        Ok(local)
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
        if path.len() == 1 {
            let lexical =
                self.elab
                    .lexical_declaration(module, node, Namespace::ValueFunction, first)?;
            if let Some((depth, local)) = self.shadowing_local(context, first, lexical)? {
                if depth < context.inherited_scopes {
                    self.finding(module, node, ScalarFindingKind::StartupCapture)?;
                    return Ok(None);
                }
                return Ok(Some(Value::Local(local)));
            }
            if let Some(declaration) = lexical {
                return Ok(Some(Value::Declaration(declaration, Vec::new())));
            }
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
        self.prepare_named_call(module, node, n.children[0], &n.children[1..], context)
    }

    fn prepare_named_call(
        &mut self,
        module: &'g Module,
        node: NodeId,
        mut callee: NodeId,
        arguments: &[NodeId],
        context: &Context<'r, 'g>,
    ) -> Work<Option<PreparedCall<'g>>> {
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
        if let Some(wrapper) = self.routine_bridge(&declaration.id)
            && !context
                .signature
                .is_some_and(|signature| signature.declaration.id == wrapper.id)
        {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::FamilySelection,
                declaration,
                bindings,
            )?;
            return Ok(None);
        }
        let closed_error = self.closed_err(&declaration.id);
        let bridge = self.installed_bridge(&declaration.id)?;
        let family = declaration.id.kind == DeclarationKind::Family
            && self.input.ordinary.is_some()
            && !closed_error
            && !bridge;
        if declaration.id.kind != DeclarationKind::Function && !family && !closed_error && !bridge {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::FamilySelection,
                declaration,
                bindings,
            )?;
            return Ok(None);
        }
        let signature = if family || closed_error || bridge {
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
        } else if let Some(signature) = self.input.facts.declarations.get(&declaration.id) {
            signature
        } else {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::DeclarationPrerequisite,
                declaration,
                bindings,
            )?;
            return Ok(None);
        };
        if self.signature_unavailable(signature)? {
            self.deferred_declaration(
                module,
                node,
                ScalarUnsupported::DeclarationPrerequisite,
                declaration,
                bindings,
            )?;
            return Ok(None);
        }
        let argument_nodes = Self::written_call_arguments(module, callee);
        let closed = if closed_error {
            if !(1..=2).contains(&arguments.len()) || argument_nodes.len() > arguments.len() + 1 {
                self.finding(module, node, ScalarFindingKind::Arity)?;
                return Ok(None);
            }
            let cause = if let Some(&ty) = argument_nodes.get(2) {
                let binders = context.signature.map_or(&[][..], |s| s.binders.as_slice());
                let Some(ty) = self.elab.ty(module, ty, binders, 1)? else {
                    return Ok(None);
                };
                Some(ty)
            } else {
                None
            };
            Some(core::ClosedErr { cause })
        } else {
            None
        };
        let argument_nodes = if closed_error {
            &argument_nodes[..argument_nodes.len().min(2)]
        } else {
            argument_nodes
        };
        let requires_evidence = self.input.ordinary.is_some_and(|ordinary| {
            ordinary
                .headers()
                .get(&declaration.id)
                .is_some_and(|header| !header.requirements.is_empty())
        });
        let infer = !closed_error
            && (family || requires_evidence || argument_nodes.len() < signature.binders.len());
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
        let Some(substitution) =
            self.explicit_callable_substitution(module, node, argument_nodes, signature, context)?
        else {
            return Ok(None);
        };
        let DeclarationShape::Function { parameters, result } = &signature.shape else {
            unreachable!()
        };
        if !closed_error && parameters.len() != arguments.len() {
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
        let (parameters, result) = if infer || closed_error {
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
        self.elab.reserve(1 + arguments.len())?;
        let arguments = arguments.to_vec();
        if !infer && !closed_error {
            self.record_callee(module, callee, &declaration.id)?;
        }
        Ok(Some(PreparedCall {
            callee_node: callee,
            fact: CallFacts {
                list_arrows: None,
                scoped_join: None,
                callee: &declaration.id,
                bindings,
                substitution,
                selection: None,
                arguments,
                defaults: Vec::new(),
                pipe_input: None,
            },
            parameters,
            result,
            infer,
            family,
            closed_error: closed,
        }))
    }
    fn record_callee(
        &mut self,
        module: &'g Module,
        node: NodeId,
        declaration: &'g DeclarationId,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        self.record_node(NodeFacts {
            site: Self::site(module, node),
            flow: Flow::normal(),
            kind: NodeFactKind::Callee { declaration },
            callable: None,
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
            inherited_scopes: 0,
            views: Vec::new(),
            loops: Vec::new(),
            startup: signature.is_none(),
            arrow_return: None,
        })
    }
    fn runtime_children(&mut self, inference: &Inference<'g>, node: NodeId) -> Work<Vec<NodeId>> {
        let n = &inference.module.syntax.nodes[node];
        if n.kind == NodeKind::Template && self.integrated {
            self.elab.reserve(n.children.len())?;
            return Ok(Self::template_holes(inference.module, node)
                .map(|(_, value)| value)
                .collect());
        }
        if n.kind == NodeKind::Pipe {
            if let Some(call) = inference.calls.get(&node) {
                self.elab.reserve(call.fact.arguments.len())?;
                return Ok(call.fact.argument_evaluation_order().collect());
            }
            if let Some(call) = inference.pipe_invocations.get(&node) {
                self.elab.reserve(1 + call.arguments.len())?;
                let input = n.children[0];
                return Ok(std::iter::once(input)
                    .chain(std::iter::once(call.receiver))
                    .chain(call.arguments.iter().copied().filter(|&arg| arg != input))
                    .collect());
            }
        }
        let children = if let Some(prepared) = inference.data.get(&node) {
            &prepared.inputs[..]
        } else if n.kind == NodeKind::Field && self.data {
            &n.children[..1]
        } else if n.kind == NodeKind::Call && !inference.value_calls.contains(&node) {
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
        self.expression_with_receiving(module, root, expected, context, false)
    }

    fn expression_with_receiving(
        &mut self,
        module: &'g Module,
        root: NodeId,
        expected: Option<Type<'g>>,
        context: &Context<'r, 'g>,
        receiving_pending: bool,
    ) -> Work<Option<ExpressionResult<'g>>> {
        let finding_start = self.facts.findings.len();
        let premise = self.arrow_premise;
        let result = self.expression_work(
            module,
            root,
            expected,
            context,
            finding_start,
            receiving_pending,
        );
        self.arrow_premise = premise;
        if result.is_err() && self.input.ordinary.is_some() {
            // An interrupted expression has not classified context-dependent findings.
            // Keep diagnostics that do not depend on which type equations settle.
            let mut end = finding_start;
            if self.integrated {
                for read in finding_start..self.facts.findings.len() {
                    if matches!(
                        self.facts.findings[read].kind,
                        ScalarFindingKind::MissingValue
                            | ScalarFindingKind::PrivateValue
                            | ScalarFindingKind::StartupCapture
                            | ScalarFindingKind::InvalidTarget
                    ) {
                        self.facts.findings.swap(end, read);
                        end += 1;
                    }
                }
            }
            self.facts.findings.truncate(end);
        }
        result
    }

    fn expression_work(
        &mut self,
        module: &'g Module,
        root: NodeId,
        expected: Option<Type<'g>>,
        context: &Context<'r, 'g>,
        finding_start: usize,
        receiving_pending: bool,
    ) -> Work<Option<ExpressionResult<'g>>> {
        let inherited_conversion =
            self.arrow_depth > 0 && self.arrow_premise.is_some_and(|p| p.has_conversion());
        let earlier_uses = self.conversion_uses.len();
        let mut inference = Inference {
            module,
            slots: BTreeMap::new(),
            parents: BTreeMap::new(),
            queue: VecDeque::new(),
            calls: BTreeMap::new(),
            calls_waiting_for_result: BTreeSet::new(),
            conversion_requirements: BTreeMap::new(),
            conversion_clause_calls: BTreeSet::new(),
            arrows: BTreeMap::new(),
            function_values: BTreeMap::new(),
            callable_contexts: BTreeMap::new(),
            value_calls: BTreeSet::new(),
            pipe_invocations: BTreeMap::new(),
            pending_invocations: BTreeSet::new(),
            data: BTreeMap::new(),
            data_nodes: BTreeSet::new(),
            ordinary_tuples: BTreeSet::new(),
            nominals: BTreeMap::new(),
        };
        self.elab.reserve(1)?;
        let mut stack = vec![root];
        let mut root_leaf = root;
        while let Some(node) = stack.pop() {
            self.elab.step(1)?;
            let syntax = &module.syntax.nodes[node];
            if node == root_leaf && syntax.kind == NodeKind::Group {
                root_leaf = syntax.children[0];
            }
            self.iteration_body.visit(node, syntax);
            self.elab.reserve(1)?;
            inference.slots.insert(
                node,
                Slot {
                    ordinary_context: self.arrow_premise,
                    ..Slot::default()
                },
            );
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
                NodeKind::Index if self.integrated => {
                    let [base, selector] = [syntax.children[0], syntax.children[1]];
                    self.elab.reserve(2)?;
                    inference.parents.insert(base, node);
                    stack.push(base);
                    if !self.untyped_index_literal(module, selector)? {
                        self.elab.reserve(2)?;
                        inference.parents.insert(selector, node);
                        stack.push(selector);
                    }
                }
                NodeKind::Index if self.data => {
                    self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
                    inference.slots.get_mut(&node).unwrap().blocked = true;
                }
                NodeKind::Match if self.data => (),
                NodeKind::Arrow if self.input.ordinary.is_some() => {
                    self.register_arrow(&mut inference, node, context)?;
                }
                NodeKind::Literal => (),
                NodeKind::Propagate | NodeKind::Wrap if self.integrated => {
                    if !self.valid_propagation_target(module, node, context)? {
                        inference.slots.get_mut(&node).unwrap().blocked = true;
                    }
                    for &child in &syntax.children {
                        self.elab.reserve(2)?;
                        inference.parents.insert(child, node);
                        stack.push(child);
                    }
                }
                NodeKind::Template => {
                    self.elab.step(syntax.children.len())?;
                    if syntax
                        .children
                        .iter()
                        .any(|&child| module.syntax.nodes[child].kind != NodeKind::TemplateText)
                    {
                        if self.show.is_none() {
                            self.defer(module, node, ScalarUnsupported::Formatting)?;
                            inference.slots.get_mut(&node).unwrap().blocked = true;
                        }
                        if self.integrated {
                            for (_, expression) in Self::template_holes(module, node) {
                                self.elab.reserve(2)?;
                                inference.parents.insert(expression, node);
                                stack.push(expression);
                            }
                        }
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
                    if let Some(Value::Declaration(declaration, bindings)) = value {
                        if self.closed_err(&declaration.id) {
                            self.finding(module, node, ScalarFindingKind::InvalidTarget)?;
                            inference.slots.get_mut(&node).unwrap().blocked = true;
                        } else if self.input.ordinary.is_some() {
                            self.register_function_value(
                                &mut inference,
                                node,
                                context,
                                declaration,
                                bindings,
                            )?;
                        } else {
                            self.deferred_declaration(
                                module,
                                node,
                                ScalarUnsupported::FunctionValue,
                                declaration,
                                bindings,
                            )?;
                            inference.slots.get_mut(&node).unwrap().blocked = true;
                        }
                        continue;
                    }
                    if syntax.children.len() > 1 && matches!(value, Some(Value::Local(_))) {
                        if self.input.ordinary.is_some() {
                            self.finding(module, node, ScalarFindingKind::Arity)?;
                        } else {
                            self.defer(module, node, ScalarUnsupported::FunctionValue)?;
                        }
                    }
                    let slot = inference.slots.get_mut(&node).unwrap();
                    slot.blocked = true;
                    if (syntax.children.len() == 1 || syntax.kind == NodeKind::Name)
                        && let Some(Value::Local(local)) = value
                    {
                        slot.blocked = local.ty.is_none();
                        if self.integrated && slot.blocked {
                            let index = self.defer(module, node, ScalarUnsupported::BindingType)?;
                            self.facts.obligations[index].declaration = Some(local.declaration);
                        }
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
                        slot.arrow = self.arrow_bindings.get(local.declaration).cloned();
                        slot.place = if local.mutable {
                            Place::Writable
                        } else {
                            Place::ReadOnly
                        };
                        slot.boolean = if local.mutable { None } else { local.boolean };
                    }
                }
                NodeKind::Pipe if self.integrated => {
                    self.prepare_pipe(&mut inference, node, context, &mut stack)?;
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
                    } else if self.input.ordinary.is_some()
                        && self.value_callee(module, syntax.children[0], context)?
                    {
                        self.elab.reserve(1)?;
                        inference.value_calls.insert(node);
                        for &child in &syntax.children {
                            self.elab.reserve(2)?;
                            inference.parents.insert(child, node);
                            stack.push(child);
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
                        if self.input.ordinary.is_some() {
                            for &argument in &syntax.children[1..] {
                                self.elab.reserve(2)?;
                                inference.parents.insert(argument, node);
                                stack.push(argument);
                            }
                        }
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
        if self.input.ordinary.is_none() && inference.slots.values().any(|s| s.blocked) {
            return Ok(None);
        }
        if let Some(expected) = expected {
            self.establish(&mut inference, root, expected, true)?;
        }
        loop {
            while let Some(node) = inference.queue.pop_front() {
                self.elab.step(1)?;
                let slot = inference.slots[&node].clone();
                if self.input.ordinary.is_some() && slot.blocked {
                    continue;
                }
                let syntax = &module.syntax.nodes[node];
                let premise = self.arrow_premise;
                self.arrow_premise = self.node_premise(&slot);
                match syntax.kind {
                    NodeKind::Field if self.data => {
                        if inference.data_nodes.contains(&node) {
                            self.variant_step(&mut inference, node, context)?;
                        } else {
                            self.field_projection(&mut inference, node, context)?;
                        }
                    }
                    NodeKind::Index if self.integrated => {
                        self.aggregate_index(&mut inference, node, context)?;
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
                        if inference.ordinary_tuples.contains(&node) {
                            if !inference.data.contains_key(&node) {
                                self.gather_ordinary_tuple(&mut inference, node)?;
                            }
                        } else {
                            self.tuple_step(&mut inference, node)?;
                            if slot.expected.is_some() {
                                self.list_step(&mut inference, node, false)?;
                            }
                        }
                    }
                    NodeKind::Literal if slot.ty.is_none() => {
                        if let Some(ty) = self.literal_type(module, node, &slot)? {
                            self.establish(&mut inference, node, ty, false)?;
                        }
                    }
                    NodeKind::Call | NodeKind::Pipe => {
                        if inference.data_nodes.contains(&node) {
                            self.variant_step(&mut inference, node, context)?;
                        } else if inference
                            .calls
                            .get(&node)
                            .is_some_and(|c| c.closed_error.is_some())
                        {
                            self.closed_err_step(&mut inference, node, false)?;
                        } else if inference.calls.get(&node).is_some_and(|c| c.family) {
                            let id = inference.calls[&node].fact.callee;
                            if self.canonical_scalar_call(&inference, node) {
                                self.canonical_call_step(&mut inference, node)?;
                            } else if self.conversion_family(id).is_some() {
                                self.conversion_call_step(&mut inference, node, false)?;
                            } else {
                                self.ordinary_call_step(&mut inference, node, false)?;
                            }
                        } else if inference.value_calls.contains(&node) {
                            self.invocation_step(&mut inference, node)?;
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
                            if let Some(operation) = self.primitive_bridge(call.fact.callee) {
                                inference.slots.get_mut(&node).unwrap().installed_operation =
                                    Some(data::DataExpression::PrimitiveOperation(operation));
                            }
                            self.establish(&mut inference, node, result, false)?;
                            for (argument, ty) in contexts {
                                self.establish(&mut inference, argument, ty, true)?;
                            }
                        }
                    }
                    NodeKind::Template if self.show.is_some() && slot.ty.is_none() => {
                        self.check_template(&mut inference, node)?;
                    }
                    NodeKind::Propagate if self.option.is_some() || self.result.is_some() => {
                        self.postfix_propagation(&mut inference, node, context)?;
                    }
                    NodeKind::Wrap if self.integrated => {
                        self.wrapping_step(&mut inference, node, context)?;
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
                        let arrow = inference.slots[&child].arrow.clone();
                        let premise = inference.slots[&child].ordinary_context;
                        let target = inference.slots.get_mut(&node).unwrap();
                        target.place = place;
                        target.path = path;
                        target.refinement = refinement;
                        target.read_only_projection = restricted;
                        target.access = access;
                        target.arrow = arrow;
                        target.ordinary_context = target.ordinary_context.or(premise);
                        if let Some(ty) = inference.slots[&child].ty.clone() {
                            self.establish(&mut inference, node, ty, false)?;
                        }
                    }
                    _ => (),
                }
                self.arrow_premise = premise;
            }
            if self.input.ordinary.is_some()
                && (self.advance_function_values(&mut inference, false)?
                    || self.advance_arrows(&mut inference, context, false)?
                    || self.advance_function_values(&mut inference, true)?
                    || self.apply_ready_nominal_default(&mut inference)?
                    || self.advance_call_context(&mut inference)?)
            {
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
                if self.input.ordinary.is_some()
                    && self.advance_arrows(&mut inference, context, true)?
                {
                    continue;
                }
                if self.advance_list_defaults(&mut inference)?
                    || self.advance_closed_errors(&mut inference)?
                {
                    continue;
                }
                break;
            }
            for node in pending {
                self.with_node_premise(&mut inference, node, |checker, inference| {
                    checker.match_step(inference, node, context)
                })?;
            }
        }
        for &node in &inference.pending_invocations {
            self.elab.step(1)?;
            let slot = inference.slots.get_mut(&node).unwrap();
            if !slot.blocked {
                self.defer(module, node, ScalarUnsupported::CallableInvocation)?;
                slot.blocked = true;
            }
        }
        if self.integrated {
            for (&node, slot) in &mut inference.slots {
                self.elab.step(1)?;
                if Self::propagating_pipe(module, node) {
                    if slot.propagation.is_none() {
                        slot.pending_core = !slot.blocked && slot.pipe_target.is_some();
                        slot.blocked = true;
                    } else {
                        slot.expected = slot.pipe_expected.take();
                    }
                } else if !slot.blocked
                    && ((slot.propagation.is_none()
                        && module.syntax.nodes[node].kind == NodeKind::Propagate)
                        || (slot.ty.is_none() && module.syntax.nodes[node].kind == NodeKind::Wrap))
                {
                    slot.pending_core = true;
                    slot.blocked = true;
                }
            }
        }
        let mut unavailable = BTreeSet::new();
        if self.input.ordinary.is_some() {
            for &node in inference.slots.keys().rev() {
                let ancestor_blocked = inference.parents.get(&node).is_some_and(|&parent| {
                    // Only ancestors blocked before finalization can prevent a
                    // context; missing context itself is not such a premise.
                    inference.slots[&parent].blocked || unavailable.contains(&parent)
                });
                self.elab.step(1)?;
                if ancestor_blocked {
                    self.elab.reserve(1)?;
                    unavailable.insert(node);
                }
            }
        }
        if self.input.ordinary.is_some() {
            self.finish_conversion_uses(&mut inference)?;
            self.defer_ordinary_calls(&mut inference)?;
        }
        if self.data {
            self.defer_pending_constructions(&mut inference)?;
        }
        let mut blocked_inputs = BTreeSet::new();
        if self.integrated {
            for (&node, slot) in &inference.slots {
                self.elab.step(1)?;
                if (slot.blocked || blocked_inputs.contains(&node))
                    && let Some(&parent) = inference.parents.get(&node)
                {
                    self.elab.reserve(1)?;
                    blocked_inputs.insert(parent);
                }
            }
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
                    if self.data && self.data_admission(ty, expected)?
                        || self.input.ordinary.is_some() && self.callable_admission(ty, expected)?
                    {
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
                if !receiving_pending
                    && !unavailable.contains(&node)
                    && !blocked_inputs.contains(&node)
                {
                    if module.syntax.nodes[node].kind == NodeKind::Literal {
                        self.finding(module, node, ScalarFindingKind::NumericContext)?;
                    } else {
                        self.finding(module, node, ScalarFindingKind::InsufficientContext)?;
                    }
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
            if !blocked && inference.slots[&node].pending_core {
                // An invalid or pending operand has its own finding or prerequisite.
                let site = if Self::propagating_pipe(module, node) {
                    module.syntax.nodes[node].children[1]
                } else {
                    node
                };
                let index = self.defer(module, site, ScalarUnsupported::CoreAdmission)?;
                if let Some(call) = inference
                    .calls
                    .get(&node)
                    .filter(|c| c.closed_error.is_some())
                {
                    self.elab.reserve(call.fact.bindings.len())?;
                    self.facts.obligations[index].declaration = Some(call.fact.callee);
                    self.facts.obligations[index].bindings = call.fact.bindings.clone();
                }
            }
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
            let i32_operand = {
                use I32Operand::*;
                let slot = &inference.slots[&node];
                let operand = |child: &NodeId| inference.slots[child].i32_operand;
                let normal_i32 = slot.ty.as_ref().and_then(primitive_name) == Some("i32")
                    && !blocked
                    && !slot.blocked
                    && slot.ordinary_context.is_none()
                    && flow.normal
                    && !flow.pending
                    && !flow.returns
                    && flow.breaks.is_empty()
                    && flow.continues.is_empty();
                match (kind, children.as_slice()) {
                    _ if !normal_i32 => Other,
                    (NodeKind::Group, [child]) => operand(child),
                    (NodeKind::ValueReference, _)
                        if slot.access == data::Access::Owned
                            && slot.path.as_ref().is_some_and(|path| {
                                path.projections.is_empty()
                                    && slot.binding.is_some_and(|id| std::ptr::eq(id, path.root))
                                    && matches!(
                                        path.root.kind,
                                        DeclarationKind::Parameter | DeclarationKind::Binding
                                    )
                            }) =>
                    {
                        Local
                    }
                    (NodeKind::Literal, _)
                        if matches!(
                            slot.literal.map(|literal| literal.value),
                            Some(LiteralValue::Integer { magnitude, negative: false })
                                if i32::try_from(magnitude).is_ok()
                        ) =>
                    {
                        Literal
                    }
                    // `slot.path` is rooted at the tuple and projects one component, so the
                    // read cannot be a List index, which can trap on its bound.
                    (NodeKind::Index, _)
                        if slot.access == data::Access::Owned
                            && matches!(
                                inference.data.get(&node).map(|prepared| &prepared.fact),
                                Some(data::DataExpression::TupleIndex { .. })
                            )
                            && slot.path.as_ref().is_some_and(|path| {
                                matches!(
                                    path.projections.as_slice(),
                                    [data::PathProjection::TupleElement(_)]
                                ) && matches!(
                                    path.root.kind,
                                    DeclarationKind::Parameter | DeclarationKind::Binding
                                )
                            }) =>
                    {
                        TupleElement
                    }
                    (NodeKind::Binary("*"), [left, right])
                        if matches!((operand(left), operand(right)), (Local, Literal)) =>
                    {
                        Product
                    }
                    (NodeKind::Binary("+"), [left, right])
                        if matches!((operand(left), operand(right)), (Local | Product, Local)) =>
                    {
                        Sum
                    }
                    _ => Other,
                }
            };
            let slot = inference.slots.get_mut(&node).unwrap();
            slot.i32_operand = i32_operand;
            let (normal, boolean) = if matches!(
                slot.installed_operation,
                Some(data::DataExpression::Wrapping(_))
            ) {
                (
                    flow.normal,
                    boolean.map(|_| {
                        if flow.normal {
                            BooleanSet::BOTH
                        } else {
                            BooleanSet::EMPTY
                        }
                    }),
                )
            } else if slot.propagation.is_some() && !flow.normal {
                (false, boolean.map(|_| BooleanSet::EMPTY))
            } else {
                (normal, boolean)
            };
            slot.blocked |= blocked;
            slot.normal = boolean.map_or(normal, |b| b != BooleanSet::EMPTY);
            slot.diverges = flow.diverges;
            slot.flow = flow;
            slot.boolean = boolean;
        }
        let mut result = None;
        let mut list_literal = None;
        for (&node, slot) in &inference.slots {
            self.elab.step(1)?;
            if self.conversions.is_some()
                && !self.integrated
                && (self.conversion_uses.contains_key(&(&module.key, node))
                    || inference.conversion_clause_calls.contains(&node))
            {
                self.defer(module, node, ScalarUnsupported::ConversionAdmission)?;
            }
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
                let callable = if slot.invocation.is_some()
                    || slot.function_value.is_some()
                    || slot.arrow.is_some()
                {
                    self.elab.reserve(1)?;
                    Some(Box::new(CallableNodeFacts {
                        invocation: slot.invocation.clone(),
                        function_value: slot.function_value.clone(),
                        arrow: slot.arrow.clone(),
                    }))
                } else {
                    None
                };
                let premise = match (
                    self.arrow_premise.filter(|p| p.has_conversion()),
                    slot.ordinary_context,
                ) {
                    (Some(a), Some(b)) => Some(a.combine(b)),
                    (a, b) => a.or(b),
                };
                let data = if let Some(propagation) = &slot.propagation {
                    self.elab.reserve(1)?;
                    let propagation = Box::new(propagation.clone());
                    Some(match &slot.installed_operation {
                        Some(data::DataExpression::ErrorConstruction(construction)) => {
                            data::DataExpression::ErrorPipe {
                                construction: construction.clone(),
                                propagation,
                            }
                        }
                        _ => data::DataExpression::Propagation(propagation),
                    })
                } else if let Some(operation) = &slot.installed_operation {
                    Some(operation.clone())
                } else {
                    inference.data.remove(&node).map(|p| p.fact)
                };
                let call = if let Some(mut call) = inference.calls.remove(&node).map(|c| c.fact) {
                    self.record_checked_ordinary_call(module, node, &mut call)?;
                    self.elab.reserve(1)?;
                    Some(Box::new(call))
                } else {
                    None
                };
                if node == root_leaf
                    && premise.is_none()
                    && matches!(data, Some(data::DataExpression::List { .. }))
                {
                    list_literal = Some(node);
                }
                self.iteration_expression(module, node, slot, data.as_ref(), call.as_deref())?;
                // Classified from the operand classes and types already inferred, so M5
                // admits equality without a lookup of its own. Each operand, through
                // groups, is a call or an operand that M5's checked arithmetic or tuple
                // component read reads.
                let i32_comparison = |node: NodeId| {
                    module.syntax.nodes[node].kind == NodeKind::Binary("==")
                        && inference
                            .slots
                            .get(&node)
                            .and_then(|slot| slot.ty.as_ref())
                            .and_then(primitive_name)
                            == Some("bool")
                        && module.syntax.nodes[node].children.iter().all(|&child| {
                            let mut operand = child;
                            while module.syntax.nodes[operand].kind == NodeKind::Group {
                                operand = module.syntax.nodes[operand].children[0];
                            }
                            inference.slots.get(&operand).is_some_and(|slot| {
                                slot.ty.as_ref().and_then(primitive_name) == Some("i32")
                                    && (module.syntax.nodes[operand].kind == NodeKind::Call
                                        || !matches!(slot.i32_operand, I32Operand::Other))
                            })
                        })
                };
                let facts = NodeFacts {
                    site: Self::site(module, node),
                    flow: slot.flow.clone(),
                    callable,
                    kind: NodeFactKind::Expression(ExpressionFacts {
                        ty: ty.clone(),
                        expected: slot.expected.clone(),
                        literal: slot.literal,
                        binding: slot.binding,
                        call,
                        ordinary_context: premise,
                        place: slot.place,
                        boolean: slot.boolean,
                        data,
                        admission: slot.admission.clone(),
                        path,
                        refinement: slot.refinement.clone(),
                        read_only_projection: slot.read_only_projection,
                        access: slot.access,
                        bounded_i32_arithmetic: matches!(
                            slot.i32_operand,
                            I32Operand::Product | I32Operand::Sum
                        ),
                        // Classified from the operand classes and types already
                        // inferred, so M5 admits equality without a lookup of its
                        // own. Each operand, through groups, is a call or an
                        // operand that M5's checked arithmetic reads.
                        i32_equality: i32_comparison(node),
                        // Classified from the same inferred facts: both operands, through
                        // groups, are i32 equalities, so the check adds no lookup.
                        i32_equality_conjunction: module.syntax.nodes[node].kind
                            == NodeKind::Binary("&&")
                            && primitive_name(ty) == Some("bool")
                            && module.syntax.nodes[node].children.len() == 2
                            && module.syntax.nodes[node].children.iter().all(|&child| {
                                let mut operand = child;
                                while module.syntax.nodes[operand].kind == NodeKind::Group {
                                    operand = module.syntax.nodes[operand].children[0];
                                }
                                inference.slots.get(&operand).is_some_and(|slot| {
                                    slot.ty.as_ref().and_then(primitive_name) == Some("bool")
                                }) && i32_comparison(operand)
                            }),
                    }),
                };
                if premise.is_some_and(|p| p.has_conversion()) {
                    self.conditional_conversion_nodes
                        .insert((&module.key, node), facts);
                    // A source argument may be checked inside an already conditional
                    // arrow. Only its local equations can use these provisional facts.
                    if !inherited_conversion || earlier_uses != self.conversion_uses.len() {
                        continue;
                    }
                } else {
                    self.facts.nodes.push(facts);
                }
                if node == root {
                    result = Some(ExpressionResult {
                        ty: ty.clone(),
                        list_literal,
                        place: slot.place,
                        normal: slot.normal,
                        diverges: slot.diverges,
                        boolean: slot.boolean,
                        flow: slot.flow.clone(),
                        access: slot.access,
                        path: self.copy_path(slot.path.as_ref())?,
                        arrow: slot.arrow.clone(),
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
        if let Some(data::DataExpression::Wrapping(wrapping)) =
            &inference.slots[&node].installed_operation
        {
            let left = &inference.slots[&children[0]].flow;
            flow = self.union(Flow::default(), left)?;
            flow.normal = left.normal && wrapping.branch.can_continue;
            if left.normal && wrapping.branch.can_return {
                let right = &inference.slots[&children[1]].flow;
                flow.returns |= right.normal;
                let can_continue = flow.normal;
                flow = self.union(flow, right)?;
                flow.normal = can_continue;
            }
        } else if n.kind == NodeKind::IfExpression {
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
                && let Some(propagation) = &inference.slots[&node].propagation
            {
                flow.reaches_postfix |= n.kind == NodeKind::Propagate;
                flow.normal = propagation.can_continue;
                flow.returns |= propagation.can_return;
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
        a.pending |= b.pending;
        a.returns |= b.returns;
        a.reaches_postfix |= b.reaches_postfix;
        a.diverges |= b.diverges;
        a.breaks.extend(&b.breaks);
        a.continues.extend(&b.continues);
        Ok(a)
    }
    fn scope_may_be_missing_bindings(&self, module: &Module, node: NodeId) -> bool {
        let syntax = &module.syntax.nodes[node];
        !self.integrated
            && (self.input.ordinary.is_none()
                || (matches!(syntax.kind, NodeKind::Binding { .. })
                    && simple_name(module, syntax.children[0]).is_none()
                    && module.syntax.nodes[syntax.children[0]].kind != NodeKind::Wildcard))
    }
    fn source_scope_ready(&mut self, module: &'g Module, root: NodeId) -> Work<bool> {
        if self.integrated {
            return Ok(true);
        }
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
        self.iteration_body.statement(n, depth);
        let before = self.marks();
        let mut flow = Flow::normal();
        let mut kind = NodeFactKind::Statement;
        let mut combined_flow = false;
        match n.kind {
            NodeKind::Block => {
                combined_flow = true;
                self.elab.reserve(1)?;
                context.scopes.push(BTreeMap::new());
                for &child in &n.children {
                    let obligations = self.facts.obligations.len();
                    let next = self.statement(module, child, depth + 1, context)?;
                    if flow.normal
                        && !flow.pending
                        && module.syntax.nodes[child].kind == NodeKind::For
                        && context.arrow_return.is_none()
                        && let Some(header) = context.signature
                        && header.declaration.id.kind == DeclarationKind::Function
                        && module.syntax.nodes[header.declaration.node].children.get(1)
                            == Some(&node)
                        && let Some(NodeFacts {
                            site,
                            kind: NodeFactKind::Iteration(iteration),
                            ..
                        }) = self.facts.nodes.last()
                        && site.node == child
                        && let Some(source) = iteration.text_input
                        && let DeclarationShape::Function { parameters, .. } = &header.shape
                    {
                        self.elab.step(parameters.len())?;
                        if let Some(i) = parameters
                            .iter()
                            .position(|p| std::ptr::eq(&p.declaration.id, source))
                        {
                            self.elab.reserve(1)?;
                            self.facts
                                .text_iteration_sources
                                .push((&header.declaration.id, i));
                        }
                    }
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
            NodeKind::Function if self.integrated => {
                // A nested declaration's body is static work, not an executed branch.
                combined_flow = true;
                let declaration = self.elab.declaration_at(module, node)?;
                if let Some(typed) = self.input.facts.declarations.get(&declaration.id) {
                    self.nested_body(module, typed, context, depth + 1)?;
                } else {
                    self.defer(module, node, ScalarUnsupported::DeclarationPrerequisite)?;
                }
            }
            NodeKind::Record | NodeKind::Enum if self.integrated => (),
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
                    let arrow = inferred.as_ref().and_then(|e| e.arrow.clone());
                    let ty = if self.marks() == before
                        && (self.input.ordinary.is_none() || inferred.is_some())
                    {
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
                        if !mutable
                            && ty.is_some()
                            && let Some(arrow) = arrow
                        {
                            self.elab.reserve(1)?;
                            self.arrow_bindings.insert(&declaration.id, arrow);
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
                        self.record_node(NodeFacts {
                            site: Self::site(module, node),
                            flow: flow.clone(),
                            callable: None,
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
            NodeKind::For if self.integrated => {
                let (iteration_flow, handoff) =
                    self.iteration_statement(module, node, depth, context)?;
                flow = iteration_flow;
                if let Some(handoff) = handoff {
                    kind = NodeFactKind::Iteration(handoff);
                }
            }
            NodeKind::Guard
                if self.integrated
                    && module.syntax.nodes[n.children[0]].kind == NodeKind::GuardBinding =>
            {
                flow = self.integrated_guard_binding(module, node, depth, context)?;
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
                    let direct_guard = self.integrated && n.kind == NodeKind::Guard;
                    let condition = self.expression(
                        module,
                        n.children[0],
                        (!direct_guard).then_some(boolean.clone()),
                        context,
                    )?;
                    let container_truth = if direct_guard {
                        condition
                            .as_ref()
                            .and_then(|e| self.guard_operand(e))
                            .map(|container| container.truth)
                    } else {
                        None
                    };
                    if direct_guard
                        && container_truth.is_none()
                        && let Some(value) = &condition
                        && primitive_name(&value.ty) != Some("never")
                        && !self.same(&value.ty, &boolean)?
                    {
                        let ty = match value.ty.as_ref() {
                            TypeTerm::Reference { referent, .. } => referent.as_ref(),
                            ty => ty,
                        };
                        if matches!(
                            ty,
                            TypeTerm::Nominal { .. }
                                | TypeTerm::Variant { .. }
                                | TypeTerm::Parameter { .. }
                        ) {
                            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
                        } else {
                            self.mismatch(module, n.children[0], &value.ty, &boolean)?;
                        }
                    }
                    let condition_pending =
                        self.integrated && (condition.is_none() || before != self.marks());
                    let truth = container_truth.unwrap_or_else(|| {
                        condition.as_ref().map_or(BooleanSet::BOTH, |e| {
                            if e.normal {
                                e.boolean.unwrap_or(BooleanSet::BOTH)
                            } else {
                                BooleanSet::EMPTY
                            }
                        })
                    });
                    let body = if n.kind == NodeKind::Guard {
                        self.guard_failure(module, node, n.children[1], depth + 1, context)?
                    } else {
                        self.statement(module, n.children[1], depth + 1, context)?
                    };
                    if n.kind == NodeKind::While {
                        context.loops.pop();
                    }
                    flow = condition.map_or_else(Flow::normal, |e| e.flow);
                    flow.pending |= condition_pending;
                    flow.normal = false;
                    combined_flow = true;
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
                if let Some(target) = &context.arrow_return {
                    flow = self.arrow_return_statement(module, node, context, target)?;
                    kind = NodeFactKind::ArrowReturn {
                        target: target.node,
                    };
                } else if let Some(signature) = context.signature {
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
        let unchanged = before == self.marks();
        flow.pending |= self.integrated && !combined_flow && !unchanged;
        if (unchanged && !matches!(n.kind, NodeKind::Binding { .. }))
            || matches!(kind, NodeFactKind::Iteration(_))
        {
            self.elab
                .reserve(1 + flow.breaks.len() + flow.continues.len())?;
            self.record_node(NodeFacts {
                site: Self::site(module, node),
                kind,
                flow: flow.clone(),
                callable: None,
            });
        }
        Ok(flow)
    }
    fn annotation_nodes(&mut self) -> Work<()> {
        self.elab.reserve(self.elab.facts.uses.len())?;
        let keys: Vec<_> = self.elab.facts.uses.keys().copied().collect();
        for key in keys {
            let use_ = &self.elab.facts.uses[&key];
            let size = 1 + use_.bindings.len() + use_.substitution.len() + use_.defaults.len();
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
                callable: None,
            });
        }
        Ok(())
    }
    fn check_named_body(
        &mut self,
        module: &'g Module,
        typed: &'r DeclarationFacts<'g>,
        pending_role: bool,
        rejected: bool,
        enclosing: Option<(&Context<'r, 'g>, usize)>,
    ) -> Work<()> {
        let declaration = typed.declaration;
        let DeclarationShape::Function { result, .. } = &typed.shape else {
            unreachable!()
        };
        let node = module.syntax.nodes[declaration.node].children[1];
        if self.signature_unavailable(typed)? {
            self.defer(
                module,
                declaration.node,
                ScalarUnsupported::DeclarationPrerequisite,
            )?;
            return Ok(());
        }
        if !self.source_scope_ready(module, node)? {
            return Ok(());
        }
        if pending_role && !rejected {
            self.defer(module, node, ScalarUnsupported::FamilySelection)?;
        }
        let before = self.marks();
        if let Some(ordinary) = self.input.ordinary
            && let Some(metadata) = ordinary.headers().get(&declaration.id)
        {
            if !pending_role && declaration.id.kind != DeclarationKind::Implementation {
                self.add_body_assumptions(module, node, &declaration.id, &[])?;
            }
            if metadata.defaults.iter().any(|d| d.ty.is_none()) {
                self.defer(module, node, ScalarUnsupported::GenericInference)?;
            }
        }
        let mut context = self.context(Some(typed))?;
        let depth = if let Some((enclosing, depth)) = enclosing {
            let mut count = enclosing.scopes.len();
            for scope in &enclosing.scopes {
                self.elab.step(1)?;
                count = count.checked_add(scope.len()).ok_or(())?;
            }
            // reserve charges the work and entries of each cloned binding.
            self.elab.reserve(count)?;
            let mut scopes = enclosing.scopes.clone();
            context.inherited_scopes = scopes.len();
            scopes.append(&mut context.scopes);
            context.scopes = scopes;
            depth
        } else {
            1
        };
        self.reached_bodies = true;
        let flow = self.statement(module, node, depth, &mut context)?;
        self.clear_body_assumptions();
        if before == self.marks() {
            if flow.normal && primitive_name(result) != Some("void") {
                self.finding(module, node, ScalarFindingKind::ReturnCoverage)?;
            } else if !pending_role {
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
                    if let Some(typed) = self.input.facts.declarations.get(&declaration.id) {
                        self.check_named_body(module, typed, false, false, None)?;
                    } else {
                        self.defer(
                            module,
                            declaration.node,
                            ScalarUnsupported::DeclarationPrerequisite,
                        )?;
                    }
                } else if declaration.id.kind == DeclarationKind::Implementation
                    && self.input.ordinary.is_some()
                {
                    if let Some((typed, admitted)) =
                        self.implementation_contract(module, declaration)?
                    {
                        self.elab.step(self.native_ownership.len())?;
                        let delivered = self
                            .native_ownership
                            .iter()
                            .filter_map(|scope| scope.provider.as_ref())
                            .any(|provider| provider.implementation.id == declaration.id);
                        let pending = !admitted
                            && !delivered
                            && !self.collections.is_wrapper(&declaration.id);
                        // Collection rejects a protected replacement (R0500), so
                        // its family role is decided rather than pending.
                        let mut rejected = false;
                        if pending && !self.ownership_overlaps.is_empty() {
                            self.elab.step(self.ownership_overlaps.len())?;
                            rejected = self
                                .ownership_overlaps
                                .iter()
                                .any(|overlap| overlap.implementation.id == declaration.id);
                        }
                        self.check_named_body(module, typed, pending, rejected, None)?;
                        self.clear_body_assumptions();
                    } else {
                        self.defer(module, declaration.node, ScalarUnsupported::FamilySelection)?;
                    }
                } else if declaration.id.kind == DeclarationKind::Expectation {
                    let before = self.marks();
                    let mut context = self.context(None)?;
                    context.startup = false;
                    let boolean = self.primitive("bool")?;
                    let child = module.syntax.nodes[declaration.node].children[0];
                    self.reached_expectations = true;
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
            self.reached_startup = true;
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
pub(super) fn numeric(ty: &Type<'_>) -> bool {
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
