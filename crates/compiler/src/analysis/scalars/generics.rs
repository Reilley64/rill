//! M4.6 typed body analysis over the captured ordinary inventory.
use super::*;
use ordinary::{Completion, OrdinaryAnalysis};

pub mod callables;
mod calls;
mod conversion_uses;
use crate::analysis::conversions::ConversionReport;
pub use conversion_uses::ConversionUse;
pub(super) use evidence::ConversionWork;
pub(in crate::analysis) use evidence::check_pair_candidates;
mod callable_copy;
mod ordinary_callbacks;
mod ordinary_calls;
pub use callable_copy::{CallableCopy, CaptureCopy, CaptureCopyAction};
pub use ordinary_callbacks::OrdinaryCallbackContext;
pub use ordinary_calls::{
    ConditionalFinding, OrdinaryCall, OrdinaryCandidate, OrdinaryValue, OutputAgreement,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyCompletion {
    Checked,
    SourceRejected,
    Incomplete,
    ResourceLimit,
}

#[derive(Debug)]
pub struct BodyStatus<'g> {
    pub site: Site<'g>,
    pub completion: BodyCompletion,
    pub not_performed: &'static [Stage],
}
mod evidence;

pub(in crate::analysis) use evidence::Evidence;
pub use evidence::{
    GoalBlocker, GoalEquality, GoalFacts, GoalId, GoalJudgment, GoalOutput, GoalSelection,
    GoalState, GoalVerdict,
};

#[derive(Debug)]
pub struct GenericReport<'g> {
    graph: &'g Graph,
    facts: ScalarFacts<'g>,
    evidence: Evidence<'g>,
    statistics: Statistics,
    completion: ScalarFailureKind,
    integrated: bool,
    not_reached: Vec<Stage>,
    ordinary_calls: BTreeMap<(&'g ModuleKey, NodeId), OrdinaryCall<'g>>,
    ordinary_values: BTreeMap<(&'g ModuleKey, NodeId), OrdinaryValue<'g>>,
    conditional_findings: Vec<ConditionalFinding<'g>>,
    conversion_uses: BTreeMap<(&'g ModuleKey, NodeId), ConversionUse<'g>>,
    conditional_conversion_nodes: BTreeMap<(&'g ModuleKey, NodeId), NodeFacts<'g>>,
    declaration_issues: Vec<ordinary::DeclarationIssue<'g>>,
}

impl<'g> GenericReport<'g> {
    pub fn context(&self) -> GraphId {
        self.graph.id()
    }

    pub fn completion(&self) -> ScalarFailureKind {
        self.completion
    }

    pub fn statistics(&self) -> Statistics {
        self.statistics
    }

    pub(in crate::analysis) fn not_reached(&self) -> &[Stage] {
        &self.not_reached
    }

    pub fn findings(&self) -> &[ScalarFinding<'g>] {
        &self.facts.findings
    }

    /// Whether the report publishes no finding (metering rule 8). Every field
    /// is named, so adding one fails to compile until it is decided here. Test
    /// support for `tests/support/limit.rs`.
    #[doc(hidden)]
    pub fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            facts,
            conditional_findings,
            declaration_issues,
            // Node facts: not findings (the lists above carry the diagnostics).
            // The rest is identity and counters.
            graph: _graph,
            evidence,
            statistics: _statistics,
            completion: _completion,
            integrated: _integrated,
            not_reached: _not_reached,
            ordinary_calls,
            ordinary_values,
            conversion_uses,
            // Facts about conditional nodes, under a premise: not findings.
            conditional_conversion_nodes: _conditional_conversion_nodes,
        } = self;
        let ScalarFacts {
            findings,
            annotations,
            // Body and node facts, obligations (blockers) and arrows: not findings.
            text_iteration_sources: _text_iteration_sources,
            bodies: _bodies,
            nodes: _nodes,
            obligations: _obligations,
            arrows: _arrows,
        } = facts;
        findings.is_empty()
            && annotations.publishes_nothing_at_limit()
            && conditional_findings.is_empty()
            && declaration_issues.is_empty()
            && evidence.publishes_nothing_at_limit(&self.conversion_use_goals())
            && ordinary_calls
                .values()
                .all(OrdinaryCall::publishes_nothing_at_limit)
            && ordinary_values
                .values()
                .all(OrdinaryValue::publishes_nothing_at_limit)
            && conversion_uses
                .values()
                .all(ConversionUse::publishes_nothing_at_limit)
    }

    /// The goals of the conversion uses with no premise, whose verdict is a
    /// finding when it says the pair is missing, rejected or disagrees.
    fn conversion_use_goals(&self) -> Vec<&GoalId> {
        self.conversion_uses
            .values()
            .filter(|use_| use_.premise.is_none())
            .filter_map(|use_| use_.goal.as_ref())
            .collect()
    }

    /// A `ResourceLimit` report publishes no finding (metering rule 8), and a
    /// report that withheld its findings reads `ResourceLimit` (the typed
    /// report's bodies can have another completion until here).
    pub(in crate::analysis) fn withhold_findings(&mut self) {
        self.completion = ScalarFailureKind::ResourceLimit;
        let goals = self
            .conversion_use_goals()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        self.evidence
            .withhold_findings(&goals.iter().collect::<Vec<_>>());
        for call in self.ordinary_calls.values_mut() {
            call.withhold_findings();
        }
        for value in self.ordinary_values.values_mut() {
            value.withhold_findings();
        }
        for use_ in self.conversion_uses.values_mut() {
            use_.withhold_findings();
        }
        self.facts.findings.clear();
        self.facts.annotations.withhold_findings();
        self.conditional_findings.clear();
        self.declaration_issues.clear();
    }

    pub fn conditional_findings(&self) -> &[ConditionalFinding<'g>] {
        &self.conditional_findings
    }

    pub fn obligations(&self) -> &[ScalarObligation<'g>] {
        &self.facts.obligations
    }

    pub fn declaration_issues(&self) -> &[ordinary::DeclarationIssue<'g>] {
        &self.declaration_issues
    }

    pub fn annotations(&self) -> &PartialFacts<'g> {
        &self.facts.annotations
    }

    pub fn body(
        &self,
        module: &Module,
        owner: BodyOwner<'_>,
    ) -> Result<Option<&BodyFacts<'g>>, QueryError> {
        self.check_module(module)?;
        if let BodyOwner::Declaration(id) = owner
            && !module.declarations.iter().any(|d| {
                d.id == *id
                    && (self.integrated || d.scope.is_none())
                    && matches!(
                        d.id.kind,
                        DeclarationKind::Function
                            | DeclarationKind::Implementation
                            | DeclarationKind::Expectation
                    )
            })
        {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.facts.bodies.iter().find(|b| b.owned_by(module, owner)))
    }

    pub(in crate::analysis) fn checked_body(
        &self,
        module: &Module,
        owner: BodyOwner<'_>,
        budget: &mut declarations::Budget,
    ) -> Work<Option<&BodyFacts<'g>>> {
        for body in &self.facts.bodies {
            budget.step(1)?;
            if body.owned_by(module, owner) {
                return Ok(Some(body));
            }
        }
        Ok(None)
    }

    /// Scope completeness is independent of role/protection clearance elsewhere.
    pub fn body_status(
        &self,
        module: &Module,
        owner: BodyOwner<'_>,
    ) -> Result<BodyStatus<'g>, QueryError> {
        let module = self.check_module(module)?;
        let checked = self.body(module, owner)?;
        let node = match owner {
            BodyOwner::Startup => module.syntax.root,
            BodyOwner::Declaration(id) => {
                module
                    .declarations
                    .iter()
                    .find(|d| d.id == *id)
                    .unwrap()
                    .node
            }
        };
        let site = Checker::site(module, node);
        let inside = |other: Site<'_>| {
            if other.module != &module.key {
                return false;
            }
            if owner == BodyOwner::Startup {
                // A top-level declaration belongs to its own body, not startup.
                return !module
                    .declarations
                    .iter()
                    .filter(|d| {
                        d.scope.is_none()
                            && matches!(
                                d.id.kind,
                                DeclarationKind::Function
                                    | DeclarationKind::Implementation
                                    | DeclarationKind::Expectation
                            )
                    })
                    .any(|d| {
                        let span = &module.syntax.nodes[d.node].span;
                        span.start() <= other.span.start() && other.span.end() <= span.end()
                    });
            }
            site.span.start() <= other.span.start() && other.span.end() <= site.span.end()
        };
        let completion = if checked.is_some() {
            BodyCompletion::Checked
        } else if self.completion == ScalarFailureKind::ResourceLimit {
            BodyCompletion::ResourceLimit
        } else if self.facts.obligations.iter().any(|o| inside(o.site)) {
            BodyCompletion::Incomplete
        } else if self.facts.findings.iter().any(|f| inside(f.site)) {
            BodyCompletion::SourceRejected
        } else {
            BodyCompletion::Incomplete
        };
        let not_performed = if completion == BodyCompletion::Checked {
            &[
                Stage::CoreAdmission,
                Stage::ResultUse,
                Stage::Ownership,
                Stage::Execution,
            ][..]
        } else {
            &[
                Stage::Inference,
                Stage::Evidence,
                Stage::CoreAdmission,
                Stage::ResultUse,
                Stage::Ownership,
                Stage::Execution,
            ][..]
        };
        Ok(BodyStatus {
            site,
            completion,
            not_performed,
        })
    }

    /// A node fact can be established even if the enclosing body is incomplete.
    pub fn node(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&NodeFacts<'g>>, QueryError> {
        self.check_module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self
            .facts
            .nodes
            .iter()
            .find(|n| n.site.module == &module.key && n.site.node == node))
    }

    pub(in crate::analysis) fn typed_nodes(&self) -> &[NodeFacts<'g>] {
        &self.facts.nodes
    }

    pub(in crate::analysis) fn take_text_iteration_sources(
        &mut self,
    ) -> Vec<(&'g DeclarationId, usize)> {
        std::mem::take(&mut self.facts.text_iteration_sources)
    }

    pub(in crate::analysis) fn typed_node_mut(&mut self, index: usize) -> &mut NodeFacts<'g> {
        &mut self.facts.nodes[index]
    }

    pub(in crate::analysis) fn checked_bodies(&self) -> &[BodyFacts<'g>] {
        &self.facts.bodies
    }

    pub fn goals(&self) -> &[GoalFacts<'g>] {
        self.evidence.goals()
    }

    pub fn goal(&self, id: &GoalId) -> Result<&GoalFacts<'g>, QueryError> {
        self.evidence.get(id)
    }

    /// Conditional context, selection and result facts, not a checked call.
    pub fn ordinary_call(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&OrdinaryCall<'g>>, QueryError> {
        let module = self.check_module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.ordinary_calls.get(&(&module.key, node)))
    }

    /// Conditional determination of a contextual overloaded name, not a checked value.
    pub fn ordinary_value(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&OrdinaryValue<'g>>, QueryError> {
        let module = self.check_module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.ordinary_values.get(&(&module.key, node)))
    }

    /// Conditional endpoints and selection for an original conversion call or function value.
    pub fn conversion_use(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&ConversionUse<'g>>, QueryError> {
        let module = self.check_module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.conversion_uses.get(&(&module.key, node)))
    }

    /// An expression fact available only under its retained conversion premise.
    pub fn conditional_conversion_node(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&NodeFacts<'g>>, QueryError> {
        let module = self.check_module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.conditional_conversion_nodes.get(&(&module.key, node)))
    }

    /// Formation facts may retain a signature and sources when mode is unresolved.
    pub fn arrow(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&callables::ArrowFacts<'g>>, QueryError> {
        let module = self.check_module(module)?;
        if node >= module.syntax.nodes.len() {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.facts.arrows.get(&(&module.key, node)).map(Arc::as_ref))
    }

    fn check_module(&self, module: &Module) -> Result<&'g Module, QueryError> {
        self.graph
            .modules()
            .iter()
            .find(|&m| std::ptr::eq(m, module))
            .ok_or(QueryError::InvalidRequest)
    }
}

fn body_prerequisites(input: &OrdinaryAnalysis<'_>) -> bool {
    let types = input.type_facts();
    types.findings.is_empty()
        && !types.obligations().any(|o| {
            !matches!(
                o.reason,
                Unsupported::GenericDefault | Unsupported::Requirements
            )
        })
}

/// Check supported written bodies without treating a pending ordinary role as admission.
pub fn analyze_generics<'g>(
    input: &OrdinaryAnalysis<'g>,
    limits: AnalysisLimits,
) -> GenericReport<'g> {
    analyze_with_conversions(input, None, limits)
}

pub(in crate::analysis) fn analyze_with_conversions<'g>(
    input: &OrdinaryAnalysis<'g>,
    conversions: Option<&ConversionReport<'_, 'g>>,
    limits: AnalysisLimits,
) -> GenericReport<'g> {
    let budget = declarations::Budget {
        limits,
        statistics: Statistics::default(),
    };
    let mut report = analyze_bodies(input, conversions, budget, None);
    if report.completion == ScalarFailureKind::ResourceLimit {
        report.withhold_findings();
    }
    report
}

#[derive(Default)]
pub(in crate::analysis) struct IntegratedInputs<'r, 'g> {
    pub option: Option<crate::analysis::typed::OptionRole<'g>>,
    pub result: Option<crate::analysis::typed::ResultRole<'g>>,
    pub show: Option<&'g Declaration>,
    pub primitives: Option<&'r crate::analysis::typed::PrimitiveCore<'g>>,
    pub conversions: Option<&'r crate::analysis::typed::ConversionCore<'g>>,
    pub collections: crate::analysis::typed::CollectionCore<'g>,
    pub native_ownership: &'r [crate::analysis::typed::OwnershipProtection<'g>],
    /// Protected replacements that collection rejects (R0500).
    pub ownership_overlaps: &'r [crate::analysis::typed::OwnershipOverlap<'g>],
    pub ordinary_scope: Option<crate::analysis::typed::InstalledOrdinaryScope<'g>>,
    pub exhausted: bool,
}

pub(in crate::analysis) fn analyze_integrated_bodies<'g>(
    input: &OrdinaryAnalysis<'g>,
    budget: declarations::Budget,
    integration: IntegratedInputs<'_, 'g>,
) -> GenericReport<'g> {
    analyze_bodies(input, None, budget, Some(integration))
}

fn analyze_bodies<'g>(
    input: &OrdinaryAnalysis<'g>,
    conversions: Option<&ConversionReport<'_, 'g>>,
    mut budget: declarations::Budget,
    integration: Option<IntegratedInputs<'_, 'g>>,
) -> GenericReport<'g> {
    let graph = input.graph();
    let limits = budget.limits;
    let integrated = integration.is_some();
    let extension = if integrated {
        Ok(None)
    } else {
        ordinary::extend_nominal_types(input, &mut budget)
    };
    let preflight_exhausted = extension.is_err();
    let extended = extension.ok().flatten();
    let input = extended.as_ref().unwrap_or(input);
    let types = input.type_facts();
    let mut checker = Checker::new(
        BodyInput {
            graph,
            facts: types,
            ordinary: Some(input),
        },
        limits,
        true,
    );
    checker.elab.statistics =
        if input.completion() == Completion::ResourceLimit && extended.is_none() {
            input.statistics()
        } else {
            budget.statistics
        };
    checker.integrated = integrated;
    checker.option = integration.as_ref().and_then(|core| core.option);
    checker.result = integration.as_ref().and_then(|core| core.result);
    checker.show = integration.as_ref().and_then(|core| core.show);
    checker.primitives = integration.as_ref().and_then(|core| core.primitives);
    checker.installed_conversions = integration.as_ref().and_then(|core| core.conversions);
    checker.collections = integration
        .as_ref()
        .map_or_else(Default::default, |core| core.collections);
    checker.native_ownership = integration
        .as_ref()
        .map_or(&[], |core| core.native_ownership);
    checker.ownership_overlaps = integration
        .as_ref()
        .map_or(&[], |core| core.ownership_overlaps);
    checker.ordinary_scope = integration
        .as_ref()
        .and_then(|core| core.ordinary_scope.clone());
    checker.elab.lexical = input.lexical().cloned();
    checker.conversions = checker
        .installed_conversions
        .map(|core| &core.families)
        .or_else(|| conversions.map(|r| &r.families));
    let mut declaration_issues = Vec::new();
    let exhausted = (|| -> Work<()> {
        use ordinary::DeclarationIssue;
        for issue in input.metered_issues() {
            let extra = match issue {
                DeclarationIssue::DuplicateMembers { members, .. } => members.len(),
                DeclarationIssue::InlineCycle { steps } => steps.len(),
                _ => 0,
            };
            checker.elab.reserve(1 + extra)?;
            declaration_issues.push(issue.clone());
        }
        if preflight_exhausted
            || integration.is_some_and(|core| core.exhausted)
            || input.completion() == Completion::ResourceLimit
            || conversions.is_some_and(|r| r.completion() == Completion::ResourceLimit)
        {
            return Err(());
        }
        for (key, finding) in types.findings.iter() {
            checker
                .elab
                .reserve(1 + finding.bindings.len() + finding.candidates.len())?;
            let elab = &mut checker.elab;
            elab.facts.findings.insert(
                *key,
                finding.clone(),
                &mut Steps::of(&elab.limits, &mut elab.statistics),
            )?;
        }
        for (key, obligation) in types.obligations.iter() {
            if !matches!(
                obligation.reason,
                Unsupported::GenericDefault | Unsupported::Requirements
            ) {
                checker.elab.reserve(1)?;
                let elab = &mut checker.elab;
                elab.facts.obligations.insert(
                    *key,
                    obligation.clone(),
                    &mut Steps::of(&elab.limits, &mut elab.statistics),
                )?;
            }
        }
        checker.elab.size_for_modules()?;
        for (slot, key, use_) in types.uses.iter() {
            checker
                .elab
                .reserve(1 + use_.bindings.len() + use_.substitution.len() + use_.defaults.len())?;
            let elab = &mut checker.elab;
            elab.facts.uses.insert(
                slot,
                *key,
                use_.clone(),
                &mut Steps::of(&elab.limits, &mut elab.statistics),
            )?;
        }
        if integrated || (input.declaration_issues().is_empty() && body_prerequisites(input)) {
            checker.run()?;
        }
        checker.annotation_nodes()
    })()
    .is_err();
    checker.facts.annotations = checker.elab.facts;
    let not_reached = [
        (Stage::Bodies, checker.reached_bodies),
        (Stage::Startup, checker.reached_startup),
        (Stage::Expectations, checker.reached_expectations),
    ]
    .into_iter()
    .filter_map(|(stage, reached)| (!reached).then_some(stage))
    .collect();
    GenericReport {
        graph,
        integrated,
        not_reached,
        facts: checker.facts,
        evidence: checker.evidence,
        statistics: checker.elab.statistics,
        ordinary_calls: checker.ordinary_calls,
        ordinary_values: checker.ordinary_values,
        conditional_findings: checker.conditional_findings,
        conversion_uses: checker.conversion_uses,
        conditional_conversion_nodes: checker.conditional_conversion_nodes,
        declaration_issues,
        completion: if exhausted {
            ScalarFailureKind::ResourceLimit
        } else {
            // Core role and protection admission have not been established.
            ScalarFailureKind::Incomplete
        },
    }
}
