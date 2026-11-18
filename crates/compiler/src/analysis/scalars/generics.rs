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
mod ordinary_callbacks;
mod ordinary_calls;
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
    analyze_bodies(input, conversions, budget, None)
}

pub(in crate::analysis) struct IntegratedInputs<'g> {
    pub option: Option<crate::analysis::typed::OptionRole<'g>>,
    pub result: Option<crate::analysis::typed::ResultRole<'g>>,
    pub exhausted: bool,
}

pub(in crate::analysis) fn analyze_integrated_bodies<'g>(
    input: &OrdinaryAnalysis<'g>,
    budget: declarations::Budget,
    integration: IntegratedInputs<'g>,
) -> GenericReport<'g> {
    analyze_bodies(input, None, budget, Some(integration))
}

fn analyze_bodies<'g>(
    input: &OrdinaryAnalysis<'g>,
    conversions: Option<&ConversionReport<'_, 'g>>,
    mut budget: declarations::Budget,
    integration: Option<IntegratedInputs<'g>>,
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
    checker.option = integration.as_ref().and_then(|inputs| inputs.option);
    checker.result = integration.as_ref().and_then(|inputs| inputs.result);
    checker.elab.lexical = input.lexical().cloned();
    checker.conversions = conversions.map(|r| &r.families);
    let mut declaration_issues = Vec::new();
    let exhausted = (|| -> Work<()> {
        use ordinary::DeclarationIssue;
        for issue in input.declaration_issues() {
            let extra = match issue {
                DeclarationIssue::DuplicateMembers { members, .. } => members.len(),
                DeclarationIssue::InlineCycle { steps } => steps.len(),
                _ => 0,
            };
            checker.elab.reserve(1 + extra)?;
            declaration_issues.push(issue.clone());
        }
        if preflight_exhausted
            || integration.is_some_and(|inputs| inputs.exhausted)
            || input.completion() == Completion::ResourceLimit
            || conversions.is_some_and(|r| r.completion() == Completion::ResourceLimit)
        {
            return Err(());
        }
        for (key, finding) in &types.findings {
            checker
                .elab
                .reserve(1 + finding.bindings.len() + finding.candidates.len())?;
            checker.elab.facts.findings.insert(*key, finding.clone());
        }
        for (key, obligation) in &types.obligations {
            if !matches!(
                obligation.reason,
                Unsupported::GenericDefault | Unsupported::Requirements
            ) {
                checker.elab.reserve(1)?;
                checker
                    .elab
                    .facts
                    .obligations
                    .insert(*key, obligation.clone());
            }
        }
        for (key, use_) in &types.uses {
            checker
                .elab
                .reserve(1 + use_.bindings.len() + use_.substitution.len() + use_.defaults.len())?;
            checker.elab.facts.uses.insert(*key, use_.clone());
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
