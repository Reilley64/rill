//! M4.6 typed body analysis over the captured ordinary inventory.
use super::*;
use ordinary::{Completion, OrdinaryAnalysis};

mod calls;
mod ordinary_calls;
pub use ordinary_calls::{ConditionalFinding, OrdinaryCall, OrdinaryCandidate, OutputAgreement};

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

pub(in crate::analysis::scalars) use evidence::Evidence;
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
    ordinary_calls: BTreeMap<(&'g ModuleKey, NodeId), OrdinaryCall<'g>>,
    conditional_findings: Vec<ConditionalFinding<'g>>,
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

    pub fn findings(&self) -> &[ScalarFinding<'g>] {
        &self.facts.findings
    }

    pub fn conditional_findings(&self) -> &[ConditionalFinding<'g>] {
        &self.conditional_findings
    }

    pub fn obligations(&self) -> &[ScalarObligation<'g>] {
        &self.facts.obligations
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
                    && d.scope.is_none()
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
        Ok(self
            .facts
            .bodies
            .iter()
            .find(|b| b.site.module == &module.key && b.owner == owner))
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
            &[Stage::Ownership, Stage::Execution][..]
        } else {
            &[
                Stage::Inference,
                Stage::Evidence,
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

    fn check_module(&self, module: &Module) -> Result<&'g Module, QueryError> {
        self.graph
            .modules()
            .iter()
            .find(|m| std::ptr::eq(*m, module))
            .ok_or(QueryError::InvalidRequest)
    }
}

/// Check supported written bodies without rerunning declaration analysis or
/// treating a pending ordinary role as admission.
pub fn analyze_generics<'g>(
    input: &OrdinaryAnalysis<'g>,
    limits: AnalysisLimits,
) -> GenericReport<'g> {
    let graph = input.graph();
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
    let exhausted = input.completion() == Completion::ResourceLimit
        || (types.findings().next().is_none()
            && !types.obligations().any(|o| {
                !matches!(
                    o.reason,
                    Unsupported::GenericDefault | Unsupported::Requirements
                )
            })
            && input.declaration_issues().is_empty()
            && checker
                .run()
                .and_then(|()| checker.annotation_nodes())
                .is_err());
    checker.facts.annotations = checker.elab.facts;
    GenericReport {
        graph,
        facts: checker.facts,
        evidence: checker.evidence,
        statistics: checker.elab.statistics,
        ordinary_calls: checker.ordinary_calls,
        conditional_findings: checker.conditional_findings,
        completion: if exhausted {
            ScalarFailureKind::ResourceLimit
        } else {
            // Core role and protection admission have not been established.
            ScalarFailureKind::Incomplete
        },
    }
}
