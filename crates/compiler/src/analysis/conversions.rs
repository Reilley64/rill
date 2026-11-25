//! Conditional conversion facts from a captured graph. A hypothesis grants no core admission.
use super::declarations::Budget;
use super::ordinary::{self, Compatibility, InputKey, OrdinaryAnalysis, patterns};
use super::*;
use crate::loader::{Implementation, Origin};

mod output;
pub(in crate::analysis) mod pairs;
pub use output::{DeterminationRoute, OutputDetermination};
pub use pairs::{
    Applicability, ErrorAgreement, PairBlocker, PairMatch, PairOutcome, PairReport, PairRequest,
    PairSignature,
};
pub use scalars::generics::ConversionUse;

#[derive(Clone, Copy, Debug)]
pub enum RoleHypothesis<'g> {
    Infallible(&'g Binding),
    Fallible {
        family: &'g Binding,
        result: &'g Binding,
    },
}

impl<'g> RoleHypothesis<'g> {
    pub(in crate::analysis) fn bindings(self) -> (&'g Binding, Option<&'g Binding>) {
        match self {
            Self::Infallible(family) => (family, None),
            Self::Fallible { family, result } => (family, Some(result)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationIssueKind {
    FamilySignature,
    MissingDestination,
    IncompatibleSignature,
    IncompleteSignature,
    DuplicatePair,
    OutsideEndpointOwners,
    ProtectedIdentity,
    ProtectedNumericPair,
    UnderdeterminedError,
}

impl RegistrationIssueKind {
    /// Code for the settled collection issue; hypotheses still do not emit source diagnostics.
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::DuplicatePair => Some("R0310"),
            Self::ProtectedIdentity | Self::ProtectedNumericPair => Some("R0311"),
            Self::OutsideEndpointOwners => Some("R0312"),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct RegistrationIssue<'g> {
    pub kind: RegistrationIssueKind,
    pub declaration: &'g DeclarationId,
    pub span: &'g Span,
    pub related: Option<&'g DeclarationId>,
}

#[derive(Debug)]
pub struct Registration<'g> {
    pub source: &'g Implementation,
    pub destination: Type<'g>,
    pub parameter: Type<'g>,
    pub signature: &'g Span,
    pub key: InputKey<'g>,
    pub error: Option<Type<'g>>,
    pub output: OutputDetermination,
    pub routes: Vec<DeterminationRoute<'g>>,
    pub(in crate::analysis) alternatives: Option<Vec<InputKey<'g>>>,
    variables: Vec<Option<usize>>,
}

#[derive(Debug)]
pub struct ConversionFamily<'g> {
    pub premise: RoleHypothesis<'g>,
    pub routes: Vec<&'g Binding>,
    pub registrations: Vec<Registration<'g>>,
    pub issues: Vec<RegistrationIssue<'g>>,
}

impl ConversionFamily<'_> {
    /// No registration issue (R0310 to R0312). Every field is named, so a new
    /// one fails to compile until it is decided here (metering rule 8).
    pub(in crate::analysis) fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            issues,
            // The premise, routes and registrations: facts, not findings.
            premise: _premise,
            routes: _routes,
            registrations: _registrations,
        } = self;
        issues.is_empty()
    }
}

#[derive(Debug)]
pub struct ConversionReport<'r, 'g> {
    input: &'r OrdinaryAnalysis<'g>,
    extended: Option<OrdinaryAnalysis<'g>>,
    pub(in crate::analysis) families: DeclarationTable<'g, ConversionFamily<'g>>,
    completion: ordinary::Completion,
    statistics: Statistics,
    /// What `type_facts` publishes at `ResourceLimit` (metering rule 8).
    withheld: PartialFacts<'g>,
}

impl<'g> ConversionReport<'_, 'g> {
    fn input(&self) -> &OrdinaryAnalysis<'g> {
        self.extended.as_ref().unwrap_or(self.input)
    }

    pub fn completion(&self) -> ordinary::Completion {
        self.completion
    }

    /// Whether the report publishes no finding through `type_facts`,
    /// `header_metadata` and `family` (metering rule 8). At `ResourceLimit`
    /// those return the withheld facts, no header and no family, so the stored
    /// issues of the families and the input are not published. Every field is
    /// named, so adding one fails to compile until it is decided here. Test
    /// support for `tests/support/limit.rs`; `bodies` is a `GenericReport`.
    #[doc(hidden)]
    pub fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            input: _input,
            extended: _extended,
            families,
            completion,
            // Stored findings and facts, published only through the accessors
            // read below; a counter.
            statistics: _statistics,
            withheld: _withheld,
        } = self;
        let limited = *completion == ordinary::Completion::ResourceLimit;
        self.type_facts().publishes_nothing_at_limit()
            && (limited
                || (self.input().publishes_nothing_at_limit()
                    && families
                        .values()
                        .all(ConversionFamily::publishes_nothing_at_limit)))
    }

    /// Inspect all supported captured bodies using this conversion inventory.
    pub fn bodies(&self, limits: AnalysisLimits) -> scalars::generics::GenericReport<'g> {
        scalars::generics::analyze_with_conversions(self.input(), Some(self), limits)
    }

    pub fn statistics(&self) -> Statistics {
        self.statistics
    }

    /// Canonical annotation terms and source-binding routes used by this inventory.
    /// A `ResourceLimit` report publishes none, as it publishes no finding.
    pub fn type_facts(&self) -> &PartialFacts<'g> {
        if self.completion == ordinary::Completion::ResourceLimit {
            return &self.withheld;
        }
        self.input().type_facts()
    }

    /// A `ResourceLimit` report publishes no header, as it publishes no finding.
    /// `Ok(None)` also means the declaration has no header: check `completion()` first.
    pub fn header_metadata(
        &self,
        id: &DeclarationId,
    ) -> Result<Option<&ordinary::HeaderMetadata<'g>>, QueryError> {
        let header = self.input().header_metadata(id)?;
        Ok(header.filter(|_| self.completion != ordinary::Completion::ResourceLimit))
    }

    /// A `ResourceLimit` report publishes no family and so no registration issue
    /// (metering rule 8): `Ok(None)` also means the declaration has no family,
    /// so check `completion()` first. The stored families keep their issues:
    /// `bodies` reads them.
    pub fn family(&self, id: &DeclarationId) -> Result<Option<&ConversionFamily<'g>>, QueryError> {
        self.input().family(id)?;
        Ok(self
            .families
            .get(id)
            .filter(|_| self.completion != ordinary::Completion::ResourceLimit))
    }
}

pub(super) fn family_signature(
    header: &DeclarationFacts<'_>,
    result_role: Option<&DeclarationId>,
) -> bool {
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return false;
    };
    let [parameter] = parameters.as_slice() else {
        return false;
    };
    match (result_role, header.binders.as_slice()) {
        (None, [to, from]) => is_parameter(result, to) && is_parameter(&parameter.ty, from),
        (Some(role), [to, from, error]) => {
            let TypeTerm::Nominal {
                declaration,
                arguments,
            } = result.as_ref()
            else {
                return false;
            };
            **declaration == *role
                && matches!(arguments.as_slice(), [success, failure]
                if is_parameter(success, to) && is_parameter(failure, error))
                && is_parameter(&parameter.ty, from)
        }
        _ => false,
    }
}

fn is_parameter(term: &Type<'_>, binder: &Binder<'_>) -> bool {
    matches!(term.as_ref(), TypeTerm::Parameter { declaration, position }
        if **declaration == binder.declaration.id && *position == binder.position)
}

pub(super) fn collect<'g>(
    input: &OrdinaryAnalysis<'g>,
    ordinary: &ordinary::FamilyFacts<'g>,
    facts: &mut ConversionFamily<'g>,
    budget: &mut Budget,
) -> Work<()> {
    budget.reserve(ordinary.routes.len())?;
    facts.routes.extend_from_slice(&ordinary.routes);
    let result = facts.premise.bindings().1.map(|b| {
        let Target::Declaration(id) = &b.target else {
            unreachable!("validated role")
        };
        id
    });
    if !family_signature(&ordinary.header, result) {
        budget.reserve(1)?;
        facts.issues.push(RegistrationIssue {
            kind: RegistrationIssueKind::FamilySignature,
            declaration: &ordinary.header.declaration.id,
            span: &ordinary.header.declaration.span,
            related: None,
        });
        return Ok(());
    }
    let graph = input.graph();
    let mut retained = ordinary.implementations.iter().peekable();
    for source in graph.implementations() {
        budget.step(1)?;
        if source.family != ordinary.header.declaration.id {
            continue;
        }
        let body = retained.next_if(|b| b.source.declaration == source.declaration);
        budget.step(graph.modules().len())?;
        let module = graph
            .modules()
            .iter()
            .find(|m| m.key == source.declaration.module)
            .unwrap();
        let signature = &module.syntax.nodes[source.signature_node];
        budget.step(signature.children.len())?;
        let destination_written = signature.children.iter().any(|&n| {
            let child = &module.syntax.nodes[n];
            child.kind == NodeKind::TypeArguments && !child.children.is_empty()
        });
        let issue = if !destination_written {
            Some(RegistrationIssueKind::MissingDestination)
        } else {
            match body.map(|b| b.compatibility) {
                Some(Compatibility::Compatible) => None,
                Some(Compatibility::Incompatible) => {
                    Some(RegistrationIssueKind::IncompatibleSignature)
                }
                _ => Some(RegistrationIssueKind::IncompleteSignature),
            }
        };
        if let Some(kind) = issue {
            budget.reserve(1)?;
            facts.issues.push(RegistrationIssue {
                kind,
                declaration: &source.declaration,
                span: &signature.span,
                related: None,
            });
            continue;
        }
        let body = body.unwrap();
        let bindings = body.substitution.as_deref().unwrap();
        let (destination, parameter) = (&bindings[0].argument, &bindings[1].argument);
        let (key, variables) = patterns::normalized(
            [destination, parameter].into_iter(),
            &body.header.binders,
            budget,
        )?;
        let error = bindings.get(2).map(|sub| sub.argument.clone());
        let (output, routes) = if let Some(error) = &error {
            output::routes(input, body, &variables, error, budget)?
        } else {
            (OutputDetermination::Infallible, Vec::new())
        };
        let alternatives = patterns::conversion_alternatives(&key, input.type_facts(), budget)?;
        let registration = Registration {
            source: body.source,
            destination: destination.clone(),
            parameter: parameter.clone(),
            signature: &signature.span,
            key,
            error,
            output,
            routes,
            alternatives,
            variables,
        };
        check_registration(facts, &registration, budget)?;
        budget.reserve(1)?;
        facts.registrations.push(registration);
    }
    Ok(())
}

fn outer_owner<'g>(mut ty: &Type<'g>, budget: &mut Budget) -> Work<Option<&'g ModuleKey>> {
    loop {
        budget.step(1)?;
        match ty.as_ref() {
            TypeTerm::Reference { referent, .. } => ty = referent,
            TypeTerm::Variant { parent, .. } => ty = parent,
            TypeTerm::Nominal { declaration, .. } => return Ok(Some(&declaration.module)),
            _ => return Ok(None),
        }
    }
}

fn check_registration<'g>(
    family: &mut ConversionFamily<'g>,
    registration: &Registration<'g>,
    budget: &mut Budget,
) -> Work<()> {
    let declaration = &registration.source.declaration;
    let mut duplicate = None;
    for previous in &family.registrations {
        budget.step(1)?;
        if registration.key.same(&previous.key, budget)? {
            duplicate = Some(&previous.source.declaration);
            break;
        }
    }
    let placement = declaration.module == registration.source.family.module
        || outer_owner(&registration.destination, budget)? == Some(&declaration.module)
        || outer_owner(&registration.parameter, budget)? == Some(&declaration.module);
    // Primitive types are leaves. A root binder can be any numeric primitive.
    // Repeated roots can use the same numeric primitive; no distinctness is required.
    let numeric =
        |ty: &Type<'_>| matches!(ty.as_ref(), TypeTerm::Parameter { .. }) || scalars::numeric(ty);
    for (kind, applies, related) in [
        (
            RegistrationIssueKind::DuplicatePair,
            duplicate.is_some(),
            duplicate,
        ),
        (
            RegistrationIssueKind::OutsideEndpointOwners,
            !placement,
            None,
        ),
        (
            RegistrationIssueKind::ProtectedIdentity,
            matches!(family.premise, RoleHypothesis::Infallible(_))
                && registration.key.equal_pair_endpoints(budget)?,
            None,
        ),
        (
            RegistrationIssueKind::ProtectedNumericPair,
            numeric(&registration.destination) && numeric(&registration.parameter),
            None,
        ),
        (
            RegistrationIssueKind::UnderdeterminedError,
            registration.output == OutputDetermination::Underdetermined,
            None,
        ),
    ] {
        if applies {
            budget.reserve(1)?;
            family.issues.push(RegistrationIssue {
                kind,
                declaration,
                span: registration.signature,
                related,
            });
        }
    }
    Ok(())
}

fn validate_export<'g>(
    graph: &Graph,
    binding: &'g Binding,
    kind: DeclarationKind,
) -> Result<&'g DeclarationId, QueryError> {
    let Target::Declaration(id) = &binding.target else {
        return Err(QueryError::InvalidRequest);
    };
    if !binding.public
        || binding.module.origin != Origin::Standard
        || id.module.origin != Origin::Standard
        || id.context != graph.id()
        || id.kind != kind
    {
        return Err(QueryError::InvalidRequest);
    }
    Ok(id)
}

/// Inspect the designated captured exports under an explicit, unadmitted role premise.
pub fn analyze_conversions<'r, 'g>(
    input: &'r OrdinaryAnalysis<'g>,
    roles: &[RoleHypothesis<'g>],
    limits: AnalysisLimits,
) -> Result<ConversionReport<'r, 'g>, QueryError> {
    if roles.len() > 2
        || matches!(roles, [a, b] if a.bindings().1.is_some() == b.bindings().1.is_some())
    {
        return Err(QueryError::InvalidRequest);
    }
    let graph = input.graph();
    let mut budget = Budget {
        limits,
        statistics: Statistics::default(),
    };
    let extension = ordinary::extend_nominal_types(input, &mut budget);
    let exhausted = extension.is_err() || budget.reserve(graph.declaration_count()).is_err();
    let extended = extension.ok().flatten();
    let complete = extended.as_ref().unwrap_or(input);
    let mut report = ConversionReport {
        input,
        extended: None,
        families: DeclarationTable::new(),
        completion: if exhausted {
            ordinary::Completion::ResourceLimit
        } else {
            complete.completion()
        },
        statistics: Statistics::default(),
        withheld: PartialFacts::default(),
    };
    if !exhausted {
        report
            .families
            .size_for(graph.id(), graph.declaration_count());
    }
    for &premise in roles {
        let (binding, result) = premise.bindings();
        let id = validate_export(graph, binding, DeclarationKind::Family)?;
        if report.families.contains_key(id) {
            return Err(QueryError::InvalidRequest);
        }
        if let Some(result) = result {
            validate_export(graph, result, DeclarationKind::Enum)?;
        }
        if exhausted {
            continue;
        }
        let count = graph
            .bindings()
            .len()
            .checked_mul(1 + usize::from(result.is_some()));
        if count.is_none_or(|n| budget.step(n).is_err()) {
            report.completion = ordinary::Completion::ResourceLimit;
            break;
        }
        if !graph.bindings().iter().any(|b| std::ptr::eq(b, binding))
            || result.is_some_and(|r| !graph.bindings().iter().any(|b| std::ptr::eq(b, r)))
        {
            return Err(QueryError::InvalidRequest);
        }
        let Some(ordinary) = complete.captured_family(id) else {
            continue;
        };
        let mut family = ConversionFamily {
            premise,
            routes: Vec::new(),
            registrations: Vec::new(),
            issues: Vec::new(),
        };
        if budget.reserve(1).is_err() {
            report.completion = ordinary::Completion::ResourceLimit;
            break;
        }
        let collected = collect(complete, ordinary, &mut family, &mut budget);
        report.families.insert(id, family);
        if collected.is_err() {
            report.completion = ordinary::Completion::ResourceLimit;
            break;
        }
    }
    report.families.follow_order(
        graph
            .modules()
            .iter()
            .flat_map(|module| module.declarations.iter().map(|d| &d.id)),
    );
    report.statistics = budget.statistics;
    report.extended = extended;
    Ok(report)
}
