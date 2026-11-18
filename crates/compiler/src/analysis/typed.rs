//! Integrated typed analysis with separately validated first-party contracts.
use super::Work;
use super::declarations::Budget;
use super::generics::{GenericReport, IntegratedInputs, analyze_integrated_bodies};
use super::ordinary::{OrdinaryAnalysis, analyze_typed_inventory};
use super::scalars::ScalarFailureKind;
use super::{AnalysisLimits, Graph, Stage, Statistics};

mod collections;
mod core;
mod interfaces;
mod numeric;
mod ownership;
mod primitive;
mod result_use;
pub(in crate::analysis) use collections::CollectionCore;
pub use collections::ListRole;
pub(super) use core::IntoAdmission;
pub use core::{OptionRole, ResultRole};
pub use interfaces::{PublicInterfaceFinding, PublicInterfaceKind};
pub use numeric::{InfallibleNumericContract, NumericConversion, NumericConversionError};
pub use ownership::{
    NativeOwnershipAction, NativeOwnershipProvider, OwnershipCoverage, OwnershipOverlap,
    OwnershipProtection, OwnershipRule,
};
pub use primitive::{Primitive, PrimitiveValue};
pub use result_use::{
    PendingResultUse, ResultComponent, ResultDiscard, ResultDiscardKind, ResultUseBlocker,
    ResultUseReport,
};

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
    core: core::InstalledCore<'g>,
    ownership: ownership::Report<'g>,
    collections: CollectionCore<'g>,
    completion: ScalarFailureKind,
    statistics: Statistics,
    not_performed: Vec<Stage>,
    result_use: Option<ResultUseReport<'g>>,
}

impl<'g> TypedReport<'g> {
    /// The remaining unchecked contracts still prevent a typed-stage receipt.
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

    pub fn option_role(&self) -> Option<&OptionRole<'g>> {
        self.core.option.as_ref()
    }

    /// Nominal Result schema, independent of wrapper representation and protection.
    pub fn result_role(&self) -> Option<&ResultRole<'g>> {
        self.core.result.as_ref()
    }

    /// Pinned, delivered Rust storage source for the admitted schema. This is not a
    /// lowered representation, a native provider binding, or a protection inventory.
    pub fn error_storage_source(&self) -> Option<&'g std::path::Path> {
        self.core.result?;
        self.inventory.graph().error_storage_source()
    }

    /// The validated canonical formatting family, independent of any provider selection.
    pub fn show_role(&self) -> Option<&'g super::Declaration> {
        self.core.show
    }

    /// The validated canonical infallible conversion family.
    pub fn into_role(&self) -> Option<&'g super::Declaration> {
        self.core.into.as_ref().map(|into| into.declaration)
    }

    /// Checks of the captured pair registrations, independent of selected providers.
    pub fn into_registration_issues(&self) -> Option<&[super::conversions::RegistrationIssue<'g>]> {
        self.core.into.as_ref().map(|into| into.issues.as_slice())
    }

    /// The checked delivered inspection implementation, separate from the enum schema.
    pub fn option_inspection(&self) -> Option<&'g super::Declaration> {
        self.core.option_inspection
    }

    /// The admitted and checked source body for canonical `isOk`.
    pub fn result_inspection(&self) -> Option<&'g super::Declaration> {
        self.core.result_inspection
    }

    /// Per-owner static evidence, not clearance for an entire family or Rill call.
    pub fn ownership_protection(&self) -> &[OwnershipProtection<'g>] {
        &self.ownership.protection
    }

    pub fn ownership_overlaps(&self) -> &[OwnershipOverlap<'g>] {
        &self.ownership.overlaps
    }

    /// A partial pass over owned occurrences. `None` means no admitted role or no work reached.
    pub fn result_use(&self) -> Option<&ResultUseReport<'g>> {
        self.result_use.as_ref()
    }

    /// List designation validated against the linked construction and projection contract.
    pub fn list_role(&self) -> Option<ListRole<'g>> {
        self.collections.list
    }

    /// The private `get` implementation bridge, not a selected ordinary winner.
    pub fn list_lookup_bridge(&self) -> Option<&'g super::Declaration> {
        self.collections.lookup
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
    let mut budget = Budget {
        limits,
        statistics: inventory.statistics(),
    };
    let roles = if inventory.completion() == super::ordinary::Completion::ResourceLimit {
        Err(())
    } else {
        core::check_roles(&inventory, &mut budget)
    };
    let show = if roles.is_ok() {
        core::check_show(&inventory, &mut budget)
    } else {
        Err(())
    };
    let into = if show.is_ok() {
        core::check_into(&inventory, &mut budget)
    } else {
        Err(())
    };
    let exhausted = into.is_err();
    let mut core = core::InstalledCore {
        option: roles.unwrap_or_default(),
        show: show.unwrap_or_default(),
        into: into.unwrap_or_default(),
        ..Default::default()
    };
    let result_roles = match core.option {
        Some(option) if !exhausted => core::check_result(&inventory, option, &mut budget),
        _ => Ok(None),
    };
    core.result = result_roles.unwrap_or_default();
    let mut ownership = ownership::Report::default();
    let authority_exhausted = exhausted
        || result_roles.is_err()
        || ownership::check(&inventory, &mut budget, &mut ownership).is_err();
    let mut collections = CollectionCore::default();
    let authority_exhausted = authority_exhausted
        || collections
            .check(&inventory, core.option, &mut budget)
            .is_err();
    let bodies = analyze_integrated_bodies(
        &inventory,
        budget,
        IntegratedInputs {
            option: core.option,
            result: core.result,
            show: core.show,
            into: core.into.as_ref(),
            collections,
            exhausted: authority_exhausted,
        },
    );
    let mut budget = Budget {
        limits,
        statistics: bodies.statistics(),
    };
    let mut completion = bodies.completion();
    if completion != ScalarFailureKind::ResourceLimit
        && core::check_providers(&inventory, &bodies, &mut budget, &mut core).is_err()
    {
        completion = ScalarFailureKind::ResourceLimit;
    }
    let mut public_interface_findings = Vec::new();
    if completion != ScalarFailureKind::ResourceLimit
        && interfaces::check(&inventory, &mut budget, &mut public_interface_findings).is_err()
    {
        completion = ScalarFailureKind::ResourceLimit;
    }
    let result_use = if let Some(role) = core.result
        && completion != ScalarFailureKind::ResourceLimit
    {
        let mut report = ResultUseReport::default();
        if result_use::check(
            &inventory,
            &bodies,
            &role.declaration.id,
            Some(&role.cause.id),
            core.result_inspection.map(|d| &d.id),
            &mut budget,
            &mut report,
        )
        .is_err()
        {
            completion = ScalarFailureKind::ResourceLimit;
        }
        Some(report)
    } else {
        None
    };
    let mut not_performed = vec![Stage::Ownership, Stage::Execution];
    if result_use.is_none() {
        not_performed.push(Stage::ResultUse);
    }
    if core.option.is_none()
        && core.show.is_none()
        && core.into.is_none()
        && ownership.protection.iter().all(|p| p.provider.is_none())
    {
        not_performed.push(Stage::CoreAdmission);
    }
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
        core,
        ownership,
        collections,
        completion,
        statistics: budget.statistics,
        not_performed,
        result_use,
    }
}
