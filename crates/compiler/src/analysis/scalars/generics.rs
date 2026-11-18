//! M4.6 typed body analysis over the captured ordinary inventory.
use super::*;
use ordinary::{Completion, OrdinaryAnalysis};

mod calls;

#[derive(Debug)]
pub struct GenericReport<'g> {
    graph: &'g Graph,
    facts: ScalarFacts<'g>,
    statistics: Statistics,
    completion: ScalarFailureKind,
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

    fn check_module(&self, module: &Module) -> Result<(), QueryError> {
        if self.graph.modules().iter().any(|m| std::ptr::eq(m, module)) {
            Ok(())
        } else {
            Err(QueryError::InvalidRequest)
        }
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
            && !types
                .obligations()
                .any(|o| o.reason != Unsupported::GenericDefault)
            && input.declaration_issues().is_empty()
            && checker
                .run()
                .and_then(|()| checker.annotation_nodes())
                .is_err());
    checker.facts.annotations = checker.elab.facts;
    GenericReport {
        graph,
        facts: checker.facts,
        statistics: checker.elab.statistics,
        completion: if exhausted {
            ScalarFailureKind::ResourceLimit
        } else {
            // Core role and protection admission have not been established.
            ScalarFailureKind::Incomplete
        },
    }
}
