//! Finish the integrated checkpoint without promoting local or hypothetical facts.
use super::*;
use crate::analysis::conversions::RegistrationIssueKind;
use crate::analysis::ordinary::DeclarationIssue;
use crate::analysis::scalars::{BodyOwner, Site};
use crate::loader::{DeclarationKind, Module};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypedCompletion {
    Checked,
    SourceRejected,
    Incomplete,
    ResourceLimit,
}

pub(super) fn finish(report: &TypedReport<'_>, budget: &mut Budget) -> Work<TypedCompletion> {
    use TypedCompletion::*;
    let bodies = &report.bodies;
    let Some(uses) = &report.result_use else {
        return Ok(Incomplete);
    };
    if !report.core_blockers.is_empty()
        || !bodies.obligations().is_empty()
        || !bodies.conditional_findings().is_empty()
        || bodies.annotations().obligations().next().is_some()
    {
        return Ok(Incomplete);
    }
    for pending in uses.pending() {
        budget.step(1)?;
        if pending.blocker == ResultUseBlocker::TypedInput {
            budget.step(bodies.findings().len())?;
            if bodies.findings().iter().any(|finding| {
                finding.site.module == pending.site.module && finding.site.node == pending.site.node
            }) {
                // The dependent fact cannot exist because this same node was
                // rejected. This is not an independent unresolved judgment.
                continue;
            }
        }
        return Ok(Incomplete);
    }
    let mut rejected = report.protected_violation
        || !bodies.findings().is_empty()
        || bodies.annotations().findings().next().is_some()
        || !report.public_interface_findings.is_empty()
        || !uses.discards().is_empty();
    for issue in report.inventory.declaration_issues() {
        budget.step(1)?;
        if matches!(issue, DeclarationIssue::UnknownLayout { .. }) {
            return Ok(Incomplete);
        }
        rejected = true;
    }
    for family in report.core.conversions.families.values() {
        budget.step(1)?;
        for issue in &family.issues {
            budget.step(1)?;
            if issue.kind == RegistrationIssueKind::IncompleteSignature {
                return Ok(Incomplete);
            }
            rejected = true;
        }
    }
    if !accounted_for(report.inventory.graph(), bodies, budget)? {
        return Ok(Incomplete);
    }
    Ok(if rejected { SourceRejected } else { Checked })
}

fn body_root(kind: DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::Function | DeclarationKind::Implementation | DeclarationKind::Expectation
    )
}

fn accounted_for(graph: &Graph, bodies: &GenericReport<'_>, budget: &mut Budget) -> Work<bool> {
    let mut checked = BTreeSet::new();
    for body in bodies.checked_bodies() {
        budget.reserve(1)?;
        if body.flow.pending || !checked.insert((body.site.module, body.owner)) {
            return Ok(false);
        }
    }
    for module in graph.modules() {
        budget.step(1)?;
        if !checked.contains(&(&module.key, BodyOwner::Startup))
            && !has_finding(bodies, module, None, budget)?
        {
            return Ok(false);
        }
        for declaration in &module.declarations {
            budget.step(1)?;
            if body_root(declaration.id.kind)
                && !checked.contains(&(&module.key, BodyOwner::Declaration(&declaration.id)))
                && !has_finding(bodies, module, Some(declaration.node), budget)?
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn has_finding(
    bodies: &GenericReport<'_>,
    module: &Module,
    root: Option<crate::parser::NodeId>,
    budget: &mut Budget,
) -> Work<bool> {
    let enclosed = |node: crate::parser::NodeId, site: Site<'_>| {
        let span = &module.syntax.nodes[node].span;
        span.start() <= site.span.start() && site.span.end() <= span.end()
    };
    'findings: for finding in bodies.findings() {
        budget.step(1)?;
        if finding.site.module != &module.key {
            continue;
        }
        if let Some(root) = root {
            if enclosed(root, finding.site) {
                return Ok(true);
            }
            continue;
        }
        for declaration in &module.declarations {
            budget.step(1)?;
            if declaration.scope.is_none()
                && body_root(declaration.id.kind)
                && enclosed(declaration.node, finding.site)
            {
                continue 'findings;
            }
        }
        return Ok(true);
    }
    Ok(false)
}
