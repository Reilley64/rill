//! Established pair queries. A conditional pair outcome is not a checked call.
use super::*;
use ordinary::selection::{self, RequestShape};
use scalars::generics::{self, GoalFacts, GoalId};

#[derive(Clone, Copy, Debug)]
pub struct PairRequest<'a, 'g> {
    pub destination: &'a Type<'g>,
    pub source: &'a Type<'g>,
    pub source_filter: Option<&'a Type<'g>>,
    pub expected_error: Option<&'a Type<'g>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairOutcome<'g> {
    Winner(&'g DeclarationId),
    NoMatch,
    Ambiguous,
    NumericGap,
    RequiredIdentity { filter_conflict: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairBlocker {
    FamilyRole,
    ResultRole,
    Protection,
    Representation,
    RequiredIdentityProvider,
    RequiredNumericProvider,
    PotentialProtectedPair,
    EndpointContext,
    Registration,
    SourceAdmission,
    Applicability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Applicability {
    Proved,
    Disproved,
    Unresolved,
}

#[derive(Debug)]
pub struct PairMatch<'g> {
    pub implementation: &'g DeclarationId,
    pub alternative: usize,
    pub covers: bool,
    pub substitution: Vec<Substitution<'g>>,
    pub applicability: Applicability,
    pub requirements: Vec<GoalId>,
    pub signature: Option<PairSignature<'g>>,
    pub error: Option<Type<'g>>,
}

#[derive(Clone, Debug)]
pub struct PairSignature<'g> {
    pub site: scalars::Site<'g>,
    pub parameter: Type<'g>,
    pub result: Type<'g>,
}

#[derive(Debug)]
pub enum ErrorAgreement<'g> {
    NotRequested,
    NotDetermined,
    Agrees,
    Disagrees {
        actual: Type<'g>,
        required: Type<'g>,
    },
}

#[derive(Debug)]
pub struct PairReport<'g> {
    /// Every conclusion about registrations or selection depends on this premise.
    pub premise: Option<RoleHypothesis<'g>>,
    pub completion: ordinary::Completion,
    pub outcome: Option<PairOutcome<'g>>,
    pub blockers: Vec<PairBlocker>,
    pub matches: Vec<PairMatch<'g>>,
    pub comparisons: Vec<ordinary::Comparison<'g>>,
    pub selected_signature: Option<PairSignature<'g>>,
    pub selected_error: Option<Type<'g>>,
    pub error_agreement: ErrorAgreement<'g>,
    pub statistics: Statistics,
    evidence: generics::Evidence<'g>,
}

impl<'g> PairReport<'g> {
    pub fn goals(&self) -> &[GoalFacts<'g>] {
        self.evidence.goals()
    }

    pub fn goal(&self, id: &GoalId) -> Result<&GoalFacts<'g>, QueryError> {
        self.evidence.get(id)
    }
}

impl<'g> ConversionReport<'_, 'g> {
    pub fn query_pair(
        &self,
        id: &DeclarationId,
        request: PairRequest<'_, 'g>,
        limits: AnalysisLimits,
    ) -> Result<PairReport<'g>, QueryError> {
        let mut output = PairReport {
            premise: None,
            completion: self.completion,
            outcome: None,
            blockers: Vec::new(),
            matches: Vec::new(),
            comparisons: Vec::new(),
            selected_signature: None,
            selected_error: None,
            error_agreement: if request.expected_error.is_some() {
                ErrorAgreement::NotDetermined
            } else {
                ErrorAgreement::NotRequested
            },
            statistics: Statistics::default(),
            evidence: generics::Evidence::default(),
        };
        let mut budget = Budget {
            limits,
            statistics: Statistics::default(),
        };
        match self.query(id, request, &mut output, &mut budget) {
            Ok(result) => result?,
            Err(()) => {
                output.completion = ordinary::Completion::ResourceLimit;
                output.outcome = None;
                output.selected_error = None;
                output.selected_signature = None;
            }
        }
        output.statistics = budget.statistics;
        Ok(output)
    }

    fn query(
        &self,
        id: &DeclarationId,
        request: PairRequest<'_, 'g>,
        output: &mut PairReport<'g>,
        budget: &mut Budget,
    ) -> Work<Result<(), QueryError>> {
        let graph = self.input().graph();
        if id.kind != DeclarationKind::Family
            || selection::declaration(graph, id, budget)?.is_none_or(|(_, d)| d.scope.is_some())
        {
            return Ok(Err(QueryError::InvalidRequest));
        }
        let endpoints = [request.destination, request.source]
            .into_iter()
            .chain(request.source_filter);
        let shape = selection::validate(graph, endpoints, budget)?;
        let error = selection::validate(graph, request.expected_error.into_iter(), budget)?;
        if shape == RequestShape::Invalid || error == RequestShape::Invalid {
            return Ok(Err(QueryError::InvalidRequest));
        }
        if self.completion == ordinary::Completion::ResourceLimit {
            return Err(());
        }
        let Some(family) = self.families.get(id) else {
            budget.reserve(1)?;
            output.blockers.push(PairBlocker::FamilyRole);
            return Ok(Ok(()));
        };
        output.premise = Some(family.premise);
        let infallible = matches!(family.premise, RoleHypothesis::Infallible(_));
        if infallible && request.expected_error.is_some() {
            return Ok(Err(QueryError::InvalidRequest));
        }
        let blockers = role_blockers(family.premise);
        budget.reserve(blockers.len())?;
        output.blockers.extend_from_slice(blockers);
        if shape != RequestShape::Ground {
            budget.reserve(1)?;
            output.blockers.push(PairBlocker::EndpointContext);
            return Ok(Ok(()));
        }
        if !family.issues.is_empty() {
            budget.reserve(1)?;
            output.blockers.push(PairBlocker::Registration);
            return Ok(Ok(()));
        }
        let pair = patterns::PairDomain::new(
            [request.destination, request.source].into_iter(),
            request.source_filter,
            budget,
        )?;
        if let Some(protected) = protected_pair(
            family.premise,
            request.destination,
            request.source,
            &pair,
            budget,
        )? {
            let blocker = match protected {
                ProtectedPair::Identity { filter_conflict } => {
                    output.outcome = Some(PairOutcome::RequiredIdentity { filter_conflict });
                    Some(PairBlocker::RequiredIdentityProvider)
                }
                ProtectedPair::NumericGap => {
                    output.outcome = Some(PairOutcome::NumericGap);
                    None
                }
                ProtectedPair::NumericProvider => Some(PairBlocker::RequiredNumericProvider),
            };
            if let Some(blocker) = blocker {
                budget.reserve(1)?;
                output.blockers.push(blocker);
            }
            return Ok(Ok(()));
        }
        compare_pair(self.input(), &self.families, family, &pair, output, budget)?;
        if let Some(PairOutcome::Winner(winner)) = output.outcome {
            budget.step(output.matches.len())?;
            let candidate = output
                .matches
                .iter()
                .find(|m| m.implementation == winner)
                .unwrap();
            if let Some(error) = &candidate.error {
                if let Some(expected) = request.expected_error {
                    let selected = patterns::ground(std::iter::once(error), budget)?;
                    let expected_key = patterns::ground(std::iter::once(expected), budget)?;
                    output.error_agreement = if selected.same(&expected_key, budget)? {
                        ErrorAgreement::Agrees
                    } else {
                        ErrorAgreement::Disagrees {
                            actual: error.clone(),
                            required: expected.clone(),
                        }
                    };
                }
                output.selected_error = Some(error.clone());
            }
            output.selected_signature = candidate.signature.clone();
        }
        Ok(Ok(()))
    }
}

pub(in crate::analysis) fn role_blockers(role: RoleHypothesis<'_>) -> &'static [PairBlocker] {
    match role {
        RoleHypothesis::Infallible(_) => &[PairBlocker::FamilyRole, PairBlocker::Protection],
        RoleHypothesis::Fallible { .. } => &[
            PairBlocker::FamilyRole,
            PairBlocker::ResultRole,
            PairBlocker::Protection,
            PairBlocker::Representation,
        ],
    }
}

pub(in crate::analysis) enum ProtectedPair {
    Identity { filter_conflict: bool },
    NumericProvider,
    NumericGap,
}

pub(in crate::analysis) fn protected_pair<'g>(
    role: RoleHypothesis<'g>,
    destination: &Type<'g>,
    source: &Type<'g>,
    pair: &patterns::PairDomain<'g>,
    budget: &mut Budget,
) -> Work<Option<ProtectedPair>> {
    let infallible = matches!(role, RoleHypothesis::Infallible(_));
    if infallible && pair.exact_endpoints(budget)? {
        return Ok(Some(ProtectedPair::Identity {
            filter_conflict: !pair.identity_filter_agrees(budget)?,
        }));
    }
    if scalars::numeric(destination) && scalars::numeric(source) {
        return Ok(Some(
            if infallible && !numeric_infallible(destination, source) {
                ProtectedPair::NumericGap
            } else {
                ProtectedPair::NumericProvider
            },
        ));
    }
    Ok(None)
}

// Numeric protection uses complete base ranges, not literal values.
fn numeric_infallible(destination: &Type<'_>, source: &Type<'_>) -> bool {
    fn integer(ty: &Type<'_>) -> Option<(bool, u16)> {
        let TypeTerm::Primitive(id) = ty.as_ref() else {
            unreachable!("numeric endpoint")
        };
        let (kind, width) = id.name().split_at(1);
        match kind {
            "i" => Some((true, width.parse().unwrap())),
            "u" => Some((false, width.parse().unwrap())),
            _ => None,
        }
    }
    match (integer(destination), integer(source)) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some((d_signed, d)), Some((s_signed, s))) => {
            if d_signed == s_signed {
                d >= s
            } else {
                d_signed && d > s
            }
        }
    }
}

pub(in crate::analysis) fn find_matches<'g>(
    declared: &ordinary::FamilyFacts<'g>,
    family: &ConversionFamily<'g>,
    pair: &patterns::PairDomain<'g>,
    matches: &mut Vec<PairMatch<'g>>,
    budget: &mut Budget,
) -> Work<bool> {
    let mut missing_admission = false;
    for entry in &family.registrations {
        budget.step(1)?;
        let Some(alternatives) = &entry.alternatives else {
            missing_admission = true;
            continue;
        };
        budget.step(declared.implementations.len())?;
        let body = declared
            .implementations
            .iter()
            .find(|b| b.source.declaration == entry.source.declaration)
            .unwrap();
        let Some(projected) = pair.project(
            &entry.key,
            alternatives,
            &entry.variables,
            &body.header.binders,
            budget,
        )?
        else {
            continue;
        };
        budget.reserve(1)?;
        matches.push(PairMatch {
            implementation: &entry.source.declaration,
            alternative: projected.alternative,
            covers: projected.covers,
            substitution: projected.substitution,
            applicability: Applicability::Unresolved,
            requirements: Vec::new(),
            signature: None,
            error: None,
        });
    }
    Ok(!missing_admission)
}

fn compare_pair<'g>(
    input: &OrdinaryAnalysis<'g>,
    families: &BTreeMap<&'g DeclarationId, ConversionFamily<'g>>,
    family: &ConversionFamily<'g>,
    pair: &patterns::PairDomain<'g>,
    output: &mut PairReport<'g>,
    budget: &mut Budget,
) -> Work<()> {
    let (binding, _) = family.premise.bindings();
    let Target::Declaration(id) = &binding.target else {
        unreachable!()
    };
    let declared = input.captured_family(id).unwrap();
    let complete = find_matches(declared, family, pair, &mut output.matches, budget)?;
    let (evidence, evaluated) =
        generics::check_pair_candidates(input, families, family, &mut output.matches, budget);
    output.evidence = evidence;
    evaluated?;
    if !complete {
        budget.reserve(1)?;
        output.blockers.push(PairBlocker::SourceAdmission);
    } else {
        output.outcome = rank(family, &output.matches, &mut output.comparisons, budget)?;
        if output.outcome.is_none() {
            budget.reserve(1)?;
            output.blockers.push(PairBlocker::Applicability);
        }
    }
    Ok(())
}

pub(in crate::analysis) fn rank<'g>(
    family: &ConversionFamily<'g>,
    matches: &[PairMatch<'g>],
    comparisons: &mut Vec<ordinary::Comparison<'g>>,
    budget: &mut Budget,
) -> Work<Option<PairOutcome<'g>>> {
    budget.reserve(matches.len())?;
    let mut alternatives = Vec::with_capacity(matches.len());
    for m in matches {
        budget.step(family.registrations.len())?;
        let registered = family
            .registrations
            .iter()
            .find(|r| r.source.declaration == *m.implementation)
            .unwrap();
        alternatives.push(registered.alternatives.as_ref().unwrap());
    }
    budget.reserve(matches.len())?;
    let mut dominated = vec![false; matches.len()];
    for (i, a) in alternatives.iter().enumerate() {
        for (j, b) in alternatives.iter().enumerate().skip(i + 1) {
            budget.step(1)?;
            let left_in_right = patterns::included(a, b, budget)?;
            let right_in_left = patterns::included(b, a, budget)?;
            let intersection = patterns::intersection(a, b, budget)?;
            budget.reserve(1)?;
            comparisons.push(ordinary::Comparison {
                left: matches[i].implementation,
                right: matches[j].implementation,
                left_in_right,
                right_in_left,
                intersection,
            });
            dominated[j] |= left_in_right
                && !right_in_left
                && matches[i].applicability == Applicability::Proved;
            dominated[i] |= right_in_left
                && !left_in_right
                && matches[j].applicability == Applicability::Proved;
        }
    }
    budget.step(matches.len())?;
    if matches
        .iter()
        .zip(&dominated)
        .any(|(m, d)| !d && m.applicability == Applicability::Unresolved)
    {
        return Ok(None);
    }
    budget.step(matches.len())?;
    let mut maxima = matches.iter().zip(dominated).filter_map(|(m, d)| {
        (!d && m.applicability == Applicability::Proved).then_some(m.implementation)
    });
    Ok(Some(match (maxima.next(), maxima.next()) {
        (None, _) => PairOutcome::NoMatch,
        (Some(id), None) => PairOutcome::Winner(id),
        (Some(_), Some(_)) => PairOutcome::Ambiguous,
    }))
}
