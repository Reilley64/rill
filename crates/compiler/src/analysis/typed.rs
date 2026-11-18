//! Integrated but incomplete typed analysis. The capture carries no core admission.
use super::Work;
use super::declarations::Budget;
use super::generics::{GenericReport, analyze_integrated_bodies};
use super::ordinary::{OrdinaryAnalysis, analyze_typed_inventory};
use super::scalars::ScalarFailureKind;
use super::{AnalysisLimits, Graph, Stage, Statistics};

mod interfaces;
pub use interfaces::{PublicInterfaceFinding, PublicInterfaceKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreBlocker {
    Roles,
    Providers,
    Protection,
    Representation,
}

#[derive(Debug)]
pub struct TypedReport<'g> {
    inventory: OrdinaryAnalysis<'g>,
    bodies: GenericReport<'g>,
    public_interface_findings: Vec<PublicInterfaceFinding<'g>>,
    completion: ScalarFailureKind,
    statistics: Statistics,
    not_performed: Vec<Stage>,
}

impl<'g> TypedReport<'g> {
    /// No typed-stage receipt can follow from the current capture format.
    pub fn completion(&self) -> ScalarFailureKind {
        self.completion
    }

    pub fn statistics(&self) -> Statistics {
        self.statistics
    }

    pub fn public_interface_findings(&self) -> &[PublicInterfaceFinding<'g>] {
        &self.public_interface_findings
    }

    /// Stages not reached or outside this increment. An omitted stage is not a pass.
    pub fn not_performed(&self) -> &[Stage] {
        &self.not_performed
    }

    pub fn core_blockers(&self) -> &'static [CoreBlocker] {
        &[
            CoreBlocker::Roles,
            CoreBlocker::Providers,
            CoreBlocker::Protection,
            CoreBlocker::Representation,
        ]
    }

    /// Collected headers and hypothetical input comparisons. Not admission.
    pub fn inventory(&self) -> &OrdinaryAnalysis<'g> {
        &self.inventory
    }

    /// Local facts and scoped blockers. A checked local body is not typed-stage completion.
    pub fn bodies(&self) -> &GenericReport<'g> {
        &self.bodies
    }
}

pub fn analyze_typed(graph: &Graph, limits: AnalysisLimits) -> TypedReport<'_> {
    let inventory = analyze_typed_inventory(graph, limits);
    let bodies = analyze_integrated_bodies(&inventory, limits);
    let mut budget = Budget {
        limits,
        statistics: bodies.statistics(),
    };
    let mut completion = bodies.completion();
    let mut public_interface_findings = Vec::new();
    if completion != ScalarFailureKind::ResourceLimit
        && interfaces::check(&inventory, &mut budget, &mut public_interface_findings).is_err()
    {
        completion = ScalarFailureKind::ResourceLimit;
    }
    let mut not_performed = vec![
        Stage::CoreAdmission,
        Stage::ResultUse,
        Stage::Ownership,
        Stage::Execution,
    ];
    if inventory.lexical().is_none() {
        not_performed.push(Stage::DeclarationTypes);
    }
    if !inventory.reached_validation() {
        not_performed.extend([
            Stage::DeclarationValidity,
            Stage::SignatureCompatibility,
            Stage::PositionValidity,
            Stage::Layout,
        ]);
    }
    let not_reached = bodies.not_reached();
    not_performed.extend_from_slice(not_reached);
    if not_reached.len() == 3 {
        not_performed.extend([Stage::Inference, Stage::Coverage, Stage::Evidence]);
    }
    TypedReport {
        inventory,
        bodies,
        public_interface_findings,
        completion,
        statistics: budget.statistics,
        not_performed,
    }
}
