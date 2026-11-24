//! The one place that says what `ResourceLimit` publishes (metering rule 8):
//! no MIR and no finding (a typed report: no receipt and no finding in any list
//! it publishes; an ordinary, generics or conversion report: no finding in any
//! list it publishes).
//! Every `ResourceLimit` assertion calls one of these, so a site cannot omit
//! part of it. The `*_flaws` forms list what is wrong, for tests that collect
//! failures instead of panicking.
//!
//! Every finding-like accessor of each report kind, in one place. Each must be
//! checked by that kind's `*_flaws` function; a new accessor that can carry a
//! finding or a verdict about one goes in both lists in the same change.
//! Backing the list, each report kind has a `publishes_nothing_at_limit`
//! method in the crate that destructures the report struct without `..`, so a
//! new field fails to compile until its author decides whether it can carry a
//! finding. Each `*_flaws` function calls it when the accessor list finds
//! nothing, and reports a field the list does not name (metering rule 8,
//! chief ruling 19). The body report, `GenericReport`, `OrdinaryAnalysis`,
//! `TypedReport`, `GraphReport`, `ConversionReport`, `SelectionReport` and
//! `PairReport` have one.
//!
//! - graph report (`graph_limit_flaws`): `findings()`; `mir()` and
//!   `ordinary_mir()` of every root and active disposer.
//! - body report (`body_limit_flaws`): `findings()`, `mir()`.
//! - typed report (`typed_limit_flaws`): `receipt()`;
//!   `bodies().findings()`, `.conditional_findings()`,
//!   `.annotations().findings()`, `.declaration_issues()`;
//!   `public_interface_findings()`; `inventory().type_facts().findings()`,
//!   `.declaration_issues()`; `result_use().discards()` (R0103);
//!   `into_registration_issues()` and `try_into_registration_issues()`
//!   (R0310 to R0312); `ownership_overlaps()` and `collection_overlaps()`
//!   (R0500); `primitive_protection()` `overlaps` (R0500); an `Overlap`
//!   `coverage` on `primitive_protection()`, `ownership_protection()` and
//!   `collection_protection()`.
//! - ordinary report (`ordinary_limit_flaws`): `type_facts().findings()`,
//!   `declaration_issues()`, `header_metadata(..)` type findings.
//! - generics report and `ConversionReport::bodies` (`generics_limit_flaws`):
//!   `findings()`, `conditional_findings()`, `declaration_issues()`,
//!   `annotations().findings()`.
//! - selection report (`selection_limit_flaws`): `outcome`.
//! - pair report (`pair_limit_flaws`): `outcome`, `selected_signature`,
//!   `selected_error`, `error_agreement` `Disagrees`, the `Registration`
//!   blocker and a `Registration` goal state.
//! - conversion report (`conversion_limit_flaws`): `type_facts()` findings,
//!   declarations, uses and obligations; `header_metadata(..)`; `family(..)`
//!   issues.
//!
//! Finding-disclosing variants (a verdict that cites a finding is a finding,
//! ruling 19 (1)), by what the accessor returns. At `ResourceLimit` each reads
//! as undecided; `conversion_use_flaws` and the goal check in
//! `generics_limit_flaws` name them:
//!
//! - `GenericReport::conversion_use(..)` of a use with no premise:
//!   `outcome` `NoMatch`, `NumericGap` or `Ambiguous` (R0311 to R0313 family
//!   findings at the use); `error_agreement` `Disagrees` (R0313);
//!   `result_agreement` and `descriptor_agreement` `Some(false)` (a type
//!   mismatch); `blockers` carrying `PairBlocker::Registration` (a rejected
//!   registration, R0310 to R0312).
//! - `GenericReport::goals()` and `goal(..)`, with no premise: state
//!   `Unresolved(ConversionPair(Registration))`; on the goal of a conversion
//!   use, state `Disproved` and `conversion_outcome` `NoMatch`, `NumericGap` or
//!   `Ambiguous`. (A `Disproved` goal that is not a use's own is a candidate's
//!   eliminated requirement: not withheld.)
//! - `GenericReport::ordinary_call(..)` and `ordinary_value(..)`:
//!   `output_agreement` `Disagrees` (a type mismatch, conditional or not).
//! - Not withheld, because they are facts and not findings: a family's
//!   `duplicate_keys` (`ri-q9lj`), the partial facts of an ordinary or typed
//!   report.
//!
//! Not finding-like, so not checked: `core_blockers()`, `result_use().pending()`
//! (blockers), and the partial facts an ordinary or typed report keeps
//! (declarations, headers, family facts: pinned M4 behaviour).
#![allow(dead_code)]

use compiler::analysis::AnalysisLimits;
use compiler::analysis::conversions::{
    ConversionReport, ErrorAgreement, PairBlocker, PairOutcome, PairReport,
};
use compiler::analysis::generics::{GenericReport, GoalBlocker, GoalState, OutputAgreement};
use compiler::analysis::ordinary::{
    Completion as OrdinaryCompletion, OrdinaryAnalysis, SelectionReport,
};
use compiler::analysis::ownership::{Completion, GraphCompletion, GraphReport, Report};
use compiler::analysis::scalars::ScalarFailureKind;
use compiler::analysis::typed::{
    OwnershipCoverage, PrimitiveCoverage, TypedCompletion, TypedReport, analyze_typed,
};
use compiler::loader::{DeclarationKind, Graph};

/// What a limit helper reports when its accessor list names nothing wrong but the
/// report's `publishes_nothing_at_limit` says it publishes a finding, a verdict
/// that cites one or MIR: a field the list does not cover. Fix the list and the
/// function together.
const UNNAMED: &str =
    "publishes something its accessor list does not name (publishes_nothing_at_limit)";

/// A whole-graph report: no finding, and no ordinary MIR in any root or active disposer.
pub(crate) fn graph_limit_flaws(report: &GraphReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != GraphCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if !report.findings().is_empty() {
        flaws.push("publishes a finding");
    }
    if report
        .roots()
        .iter()
        .chain(report.active_disposers())
        .any(|root| root.mir().is_some() || root.ordinary_mir().is_some())
    {
        flaws.push("publishes MIR");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// A targeted `lower_body` or `lower_root` report: no finding and no MIR.
pub(crate) fn body_limit_flaws(report: &Report<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != Completion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if !report.findings().is_empty() {
        flaws.push("publishes a finding");
    }
    if report.mir().is_some() {
        flaws.push("publishes MIR");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// A typed report: no receipt, and no finding in any list it publishes: body,
/// conditional, annotation, public-interface, declaration issue, the
/// inventory's type findings and declaration issues, Result discards, the
/// `into` and `try_into` registration issues, ownership, collection and
/// primitive overlaps, and an `Overlap` coverage verdict on any protection.
pub(crate) fn typed_limit_flaws(report: &TypedReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != TypedCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.receipt().is_some() {
        flaws.push("grants a receipt");
    }
    let bodies = report.bodies();
    if !bodies.findings().is_empty() {
        flaws.push("publishes a body finding");
    }
    if !bodies.conditional_findings().is_empty() {
        flaws.push("publishes a conditional finding");
    }
    if registration_goal(bodies) {
        flaws.push("publishes a Registration goal state");
    }
    if bodies.annotations().findings().next().is_some() {
        flaws.push("publishes an annotation finding");
    }
    if !bodies.declaration_issues().is_empty() {
        flaws.push("publishes a declaration issue");
    }
    if !report.public_interface_findings().is_empty() {
        flaws.push("publishes a public-interface finding");
    }
    let inventory = report.inventory();
    if inventory.type_facts().findings().next().is_some() {
        flaws.push("publishes an inventory type finding");
    }
    if !inventory.declaration_issues().is_empty() {
        flaws.push("publishes an inventory declaration issue");
    }
    if report
        .result_use()
        .is_some_and(|uses| !uses.discards().is_empty())
    {
        flaws.push("publishes a Result discard");
    }
    if report
        .into_registration_issues()
        .into_iter()
        .chain(report.try_into_registration_issues())
        .any(|issues| !issues.is_empty())
    {
        flaws.push("publishes a registration issue");
    }
    if !report.ownership_overlaps().is_empty() {
        flaws.push("publishes an ownership overlap");
    }
    if !report.collection_overlaps().is_empty() {
        flaws.push("publishes a collection overlap");
    }
    if report
        .primitive_protection()
        .iter()
        .any(|scope| !scope.overlaps.is_empty() || scope.coverage == PrimitiveCoverage::Overlap)
    {
        flaws.push("publishes a primitive overlap");
    }
    if report
        .ownership_protection()
        .iter()
        .any(|scope| scope.coverage == OwnershipCoverage::Overlap)
        || report
            .collection_protection()
            .any(|scope| scope.coverage == OwnershipCoverage::Overlap)
    {
        flaws.push("publishes an overlap verdict");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// Whether a goal with no premise reads `Unresolved(ConversionPair(Registration))`.
fn registration_goal(report: &GenericReport<'_>) -> bool {
    report.goals().iter().any(|goal| {
        goal.premise.is_none()
            && goal.state
                == GoalState::Unresolved(GoalBlocker::ConversionPair(PairBlocker::Registration))
    })
}

/// The finding-disclosing states of every conversion use and ordinary call of a
/// body report (see the list at the head of this file). The graph names the
/// nodes. A report that publishes any is not a `ResourceLimit` report.
pub(crate) fn conversion_use_flaws(graph: &Graph, report: &GenericReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    for module in graph.modules() {
        for node in 0..module.syntax.nodes.len() {
            if let Some(use_) = report.conversion_use(module, node).unwrap()
                && use_.premise.is_none()
            {
                if matches!(
                    use_.outcome,
                    Some(PairOutcome::NoMatch | PairOutcome::NumericGap | PairOutcome::Ambiguous)
                ) {
                    flaws.push("publishes a conversion use outcome that is a finding");
                }
                if matches!(use_.error_agreement, ErrorAgreement::Disagrees { .. }) {
                    flaws.push("publishes a conversion use error disagreement");
                }
                if use_.result_agreement == Some(false) || use_.descriptor_agreement == Some(false)
                {
                    flaws.push("publishes a conversion use agreement that is a mismatch");
                }
                if use_.blockers.contains(&PairBlocker::Registration) {
                    flaws.push("publishes a Registration blocker");
                }
                if let Some(goal) = &use_.goal {
                    let goal = report.goal(goal).unwrap();
                    if goal.state == GoalState::Disproved
                        || matches!(
                            goal.conversion_outcome,
                            Some(
                                PairOutcome::NoMatch
                                    | PairOutcome::NumericGap
                                    | PairOutcome::Ambiguous
                            )
                        )
                    {
                        flaws.push("publishes a conversion use goal verdict that is a finding");
                    }
                }
            }
            let disagrees = |agreement: &OutputAgreement<'_>| {
                matches!(agreement, OutputAgreement::Disagrees { .. })
            };
            if report
                .ordinary_call(module, node)
                .unwrap()
                .is_some_and(|call| disagrees(&call.output_agreement))
                || report
                    .ordinary_value(module, node)
                    .unwrap()
                    .is_some_and(|value| disagrees(&value.output_agreement))
            {
                flaws.push("publishes an output disagreement");
            }
        }
    }
    flaws.dedup();
    flaws
}

/// An `analyze_ordinary` report: no finding in the type facts, the declaration
/// issues or any declaration header's metadata. The graph names the headers.
pub(crate) fn ordinary_limit_flaws(
    graph: &Graph,
    report: &OrdinaryAnalysis<'_>,
) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != OrdinaryCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.type_facts().findings().next().is_some() {
        flaws.push("publishes a type finding");
    }
    if !report.declaration_issues().is_empty() {
        flaws.push("publishes a declaration issue");
    }
    if header_finding(graph, report) {
        flaws.push("publishes a header metadata finding");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// Whether any declaration header of the report publishes a metadata finding.
pub(crate) fn header_finding(graph: &Graph, report: &OrdinaryAnalysis<'_>) -> bool {
    graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .any(|declaration| {
            report
                .header_metadata(&declaration.id)
                .unwrap()
                .is_some_and(|header| header.type_metadata().findings().next().is_some())
        })
}

/// An `analyze_generics` report (or `ConversionReport::bodies`): no finding in
/// any list it publishes: body, conditional, declaration issue and annotation.
pub(crate) fn generics_limit_flaws(report: &GenericReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != ScalarFailureKind::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if !report.findings().is_empty() {
        flaws.push("publishes a body finding");
    }
    if !report.conditional_findings().is_empty() {
        flaws.push("publishes a conditional finding");
    }
    if !report.declaration_issues().is_empty() {
        flaws.push("publishes a declaration issue");
    }
    if report.annotations().findings().next().is_some() {
        flaws.push("publishes an annotation finding");
    }
    if registration_goal(report) {
        flaws.push("publishes a Registration goal state");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// A `ConversionReport`: no finding, no declaration, use or obligation through
/// `type_facts`, no header through `header_metadata` and no registration issue
/// through `family` (metering rule 8). Its bodies are `assert_generics_limit`'s.
/// The graph names the headers and the families.
pub(crate) fn conversion_limit_flaws(
    graph: &Graph,
    report: &ConversionReport<'_, '_>,
) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion() != OrdinaryCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.type_facts().findings().next().is_some() {
        flaws.push("publishes a type finding");
    }
    let facts = report.type_facts();
    if facts.declarations().next().is_some()
        || facts.type_uses().next().is_some()
        || facts.obligations().next().is_some()
    {
        flaws.push("publishes type facts");
    }
    if conversion_header_finding(graph, report) {
        flaws.push("publishes a header metadata finding");
    }
    if conversion_headers(graph, report) != 0 {
        flaws.push("publishes a header");
    }
    if graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .filter(|declaration| declaration.id.kind == DeclarationKind::Family)
        .any(|declaration| {
            report
                .family(&declaration.id)
                .unwrap()
                .is_some_and(|family| !family.issues.is_empty())
        })
    {
        flaws.push("publishes a registration issue");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// A `query_pair` report: no outcome, selected signature or selected error, no
/// disagreeing error, no `Registration` blocker and no goal that cites a finding.
pub(crate) fn pair_limit_flaws(report: &PairReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion != OrdinaryCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.outcome.is_some() {
        flaws.push("publishes an outcome");
    }
    if report.selected_signature.is_some() || report.selected_error.is_some() {
        flaws.push("publishes a selected signature or error");
    }
    if matches!(report.error_agreement, ErrorAgreement::Disagrees { .. }) {
        flaws.push("publishes an error disagreement");
    }
    if report.blockers.contains(&PairBlocker::Registration) {
        flaws.push("publishes a Registration blocker");
    }
    if report.goals().iter().any(|goal| {
        goal.premise.is_none()
            && goal.state
                == GoalState::Unresolved(GoalBlocker::ConversionPair(PairBlocker::Registration))
    }) {
        flaws.push("publishes a Registration goal state");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// A `select_inputs` report: no outcome (a winner, `NoMatch` or `Ambiguous`).
pub(crate) fn selection_limit_flaws(report: &SelectionReport<'_>) -> Vec<&'static str> {
    let mut flaws = Vec::new();
    if report.completion != OrdinaryCompletion::ResourceLimit {
        flaws.push("completion is not ResourceLimit");
    }
    if report.outcome.is_some() {
        flaws.push("publishes an outcome");
    }
    if flaws.is_empty() && !report.publishes_nothing_at_limit() {
        flaws.push(UNNAMED);
    }
    flaws
}

/// How many declarations of the graph have a header through the conversion
/// report's `header_metadata`. `Ok(None)` is both "no header" and, at
/// `ResourceLimit`, "withheld".
pub(crate) fn conversion_headers(graph: &Graph, report: &ConversionReport<'_, '_>) -> usize {
    graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .filter(|declaration| report.header_metadata(&declaration.id).unwrap().is_some())
        .count()
}

/// Whether any declaration header of the conversion report publishes a metadata finding.
pub(crate) fn conversion_header_finding(graph: &Graph, report: &ConversionReport<'_, '_>) -> bool {
    graph
        .modules()
        .iter()
        .flat_map(|module| &module.declarations)
        .any(|declaration| {
            report
                .header_metadata(&declaration.id)
                .unwrap()
                .is_some_and(|header| header.type_metadata().findings().next().is_some())
        })
}

/// Every cap below a typed run's own: one short on each axis, 64 evenly spaced
/// work caps from zero, and the last 64 work caps. Whichever list a stop falls
/// inside, `assert_typed_limit` holds. `full` is the report of the default run
/// of `graph`.
#[track_caller]
pub(crate) fn assert_typed_limits_below(graph: &Graph, full: &TypedReport<'_>) {
    let stats = full.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert_eq!(
        analyze_typed(graph, exact).statistics(),
        stats,
        "the exact cap replays the run"
    );
    let work = (0..64)
        .map(|step| stats.work * step / 64)
        .chain(stats.work.saturating_sub(64)..stats.work);
    for max_work in work {
        assert_typed_limit_uses(
            graph,
            &analyze_typed(graph, AnalysisLimits { max_work, ..exact }),
        );
    }
    assert_typed_limit_uses(
        graph,
        &analyze_typed(
            graph,
            AnalysisLimits {
                max_entries: exact.max_entries - 1,
                ..exact
            },
        ),
    );
    assert_typed_limit_uses(
        graph,
        &analyze_typed(
            graph,
            AnalysisLimits {
                max_depth: exact.max_depth - 1,
                ..exact
            },
        ),
    );
}

/// `assert_typed_limit`, and no finding-disclosing conversion use or output
/// verdict in the bodies (`conversion_use_flaws`).
#[track_caller]
pub(crate) fn assert_typed_limit_uses(graph: &Graph, report: &TypedReport<'_>) {
    let flaws = conversion_use_flaws(graph, report.bodies());
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
    assert_typed_limit(report);
}

#[track_caller]
pub(crate) fn assert_graph_limit(report: &GraphReport<'_>) {
    let flaws = graph_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_body_limit(report: &Report<'_>) {
    let flaws = body_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_typed_limit(report: &TypedReport<'_>) {
    let flaws = typed_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_ordinary_limit(graph: &Graph, report: &OrdinaryAnalysis<'_>) {
    let flaws = ordinary_limit_flaws(graph, report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_conversion_limit(graph: &Graph, report: &ConversionReport<'_, '_>) {
    let flaws = conversion_limit_flaws(graph, report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}

#[track_caller]
pub(crate) fn assert_pair_limit(report: &PairReport<'_>) {
    let flaws = pair_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics);
}

#[track_caller]
pub(crate) fn assert_selection_limit(report: &SelectionReport<'_>) {
    let flaws = selection_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics);
}

#[track_caller]
pub(crate) fn assert_generics_limit(report: &GenericReport<'_>) {
    let flaws = generics_limit_flaws(report);
    assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
}
