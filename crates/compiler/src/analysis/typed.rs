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
pub use collections::{
    CollectionOperation, CollectionOverlap, CollectionProtection, CollectionProvider,
    CollectionRepresentation, CollectionStorage, ListRole, ListTraversal,
};
pub(super) use core::PrimitiveCore;
pub use core::{
    DisplayProtection, ErrorOperation, ErrorProviders, ErrorRenderer, ErrorRepresentation,
    OptionRole, PrimitiveBinding, PrimitiveCoverage, PrimitiveOverlap, PrimitiveProtection,
    PrimitiveProvider, PrimitiveRole, ResultRole,
};
pub use interfaces::{PublicInterfaceFinding, PublicInterfaceKind};
pub(in crate::analysis) use numeric::ConversionCore;
pub use numeric::values as conversion_values;
pub use numeric::{
    ConversionProtection, ConversionProvider, DecimalParse, DecimalParseError,
    InfallibleNumericContract, NumericConversion, NumericConversionError, RegisteredConversion,
};
pub use ownership::{
    NativeOwnershipAction, NativeOwnershipCall, NativeOwnershipProvider, NativeRepresentation,
    NativeRepresentationKind, OwnershipCoverage, OwnershipOverlap, OwnershipProtection,
    OwnershipRule,
};
pub use primitive::{Primitive, PrimitiveOperation, PrimitiveValue};
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
    collection_overlaps: Vec<CollectionOverlap<'g>>,
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
        Some(self.error_representation()?.storage_source)
    }

    /// Capture-bound storage mapping. This establishes no Rill layout or ABI.
    pub fn error_representation(&self) -> Option<&ErrorRepresentation<'g>> {
        self.core.result.as_ref()?.representation.as_ref()
    }

    /// Bound construction, retention and rendering operations, independent of protection clearance.
    pub fn error_providers(&self) -> Option<&ErrorProviders<'g>> {
        self.core.result.as_ref()?.providers.as_ref()
    }

    /// The validated canonical formatting family, independent of any provider selection.
    pub fn show_role(&self) -> Option<&'g super::Declaration> {
        self.core.show
    }

    pub fn copy_role(&self) -> Option<&'g super::Declaration> {
        self.core.primitives.copy
    }

    pub fn dispose_role(&self) -> Option<&'g super::Declaration> {
        self.core.primitives.dispose
    }

    pub fn equality_role(&self) -> Option<&'g super::Declaration> {
        self.core.primitives.equality
    }

    /// Pinned implementations and specific compiler operation bindings, not selection or representation.
    pub fn primitive_providers(&self) -> &[PrimitiveProvider<'g>] {
        &self.core.primitives.providers
    }

    pub fn primitive_protection(&self) -> &[PrimitiveProtection<'g>] {
        &self.core.primitives.protection
    }

    /// The validated canonical infallible conversion family.
    pub fn into_role(&self) -> Option<&'g super::Declaration> {
        self.core.conversions.into
    }

    pub fn try_into_role(&self) -> Option<&'g super::Declaration> {
        self.core.conversions.try_into
    }

    /// Checks of the captured pair registrations, independent of selected providers.
    pub fn into_registration_issues(&self) -> Option<&[super::conversions::RegistrationIssue<'g>]> {
        Some(
            &self
                .core
                .conversions
                .families
                .get(&self.into_role()?.id)?
                .issues,
        )
    }

    pub fn try_into_registration_issues(
        &self,
    ) -> Option<&[super::conversions::RegistrationIssue<'g>]> {
        Some(
            &self
                .core
                .conversions
                .families
                .get(&self.try_into_role()?.id)?
                .issues,
        )
    }

    /// The captured registration and its separately validated linked provider.
    pub fn conversion_provider(
        &self,
        implementation: &super::DeclarationId,
    ) -> Result<Option<RegisteredConversion<'g>>, super::QueryError> {
        if implementation.kind != crate::loader::DeclarationKind::Implementation {
            return Err(super::QueryError::InvalidRequest);
        }
        self.inventory.header_metadata(implementation)?;
        Ok(self
            .core
            .conversions
            .registered
            .get(implementation)
            .copied())
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

    /// Attached storage contract, independent of the provider winner or protection coverage.
    pub fn native_representations(&self) -> &[NativeRepresentation<'g>] {
        &self.ownership.representations
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
        self.collections.lookup()
    }

    /// Owned element storage and retained source shapes, independent of ordinary call clearance.
    pub fn collection_representations(
        &self,
    ) -> impl Iterator<Item = CollectionRepresentation<'g>> + '_ {
        self.collections.representations()
    }

    /// Required-winner coverage and provider evidence for the captured List regions.
    pub fn collection_protection(&self) -> impl Iterator<Item = CollectionProtection<'g>> + '_ {
        self.collections.protection.iter().copied().flatten()
    }

    pub fn collection_overlaps(&self) -> &[CollectionOverlap<'g>] {
        &self.collection_overlaps
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
    let exhausted = show.is_err();
    let mut core = core::InstalledCore {
        option: roles.unwrap_or_default(),
        show: show.unwrap_or_default(),
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
    let authority_exhausted = authority_exhausted
        || core
            .primitives
            .check(&inventory, core.show, &mut budget)
            .is_err();
    let mut collections = CollectionCore::default();
    let mut collection_overlaps = Vec::new();
    let authority_exhausted = authority_exhausted
        || collections
            .check(
                &inventory,
                core.option,
                &mut budget,
                &mut collection_overlaps,
            )
            .is_err()
        || core
            .conversions
            .check(
                &inventory,
                core.result,
                core.option,
                collections.list,
                &mut budget,
            )
            .is_err();
    let bodies = analyze_integrated_bodies(
        &inventory,
        budget,
        IntegratedInputs {
            option: core.option,
            result: core.result,
            show: core.show,
            primitives: Some(&core.primitives),
            conversions: Some(&core.conversions),
            collections,
            native_ownership: &ownership.protection,
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
            result_use::Inputs {
                result: result_use::Container {
                    declaration: &role.declaration.id,
                    success: &role.ok.id,
                    failure: &role.err.id,
                },
                option: core.option.map(|option| result_use::Container {
                    declaration: &option.declaration.id,
                    success: &option.some.id,
                    failure: &option.none.id,
                }),
                opaque_cause: Some(&role.cause.id),
                collections,
                native: &ownership,
                outcome_observer: core.result_inspection.map(|d| &d.id),
            },
            &mut budget,
            &mut report,
        )
        .is_err()
        {
            completion = ScalarFailureKind::ResourceLimit;
        } else if !report.pending().is_empty() {
            completion = ScalarFailureKind::Incomplete;
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
        && core.primitives.copy.is_none()
        && core.primitives.dispose.is_none()
        && core.primitives.equality.is_none()
        && core.conversions.families.is_empty()
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
        collection_overlaps,
        completion,
        statistics: budget.statistics,
        not_performed,
        result_use,
    }
}
