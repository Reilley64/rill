//! Conversion pair applicability and output equations in the shared proof worklist.
use super::*;
use crate::analysis::conversions::{
    Applicability, ConversionFamily, PairBlocker, PairMatch, PairOutcome, PairSignature,
    RoleHypothesis, pairs,
};
use ordinary::patterns;

type Families<'g> = BTreeMap<&'g DeclarationId, ConversionFamily<'g>>;

pub(in crate::analysis) fn check_pair_candidates<'g>(
    input: &OrdinaryAnalysis<'g>,
    families: &Families<'g>,
    family: &ConversionFamily<'g>,
    matches: &mut [PairMatch<'g>],
    budget: &mut declarations::Budget,
) -> (Evidence<'g>, Work<()>) {
    let mut checker = Checker::new(
        BodyInput {
            graph: input.graph(),
            facts: input.type_facts(),
            ordinary: Some(input),
        },
        budget.limits,
        true,
    );
    checker.conversions = Some(families);
    checker.arrow_premise = Some(InputPremise::HypotheticalConversionRole);
    checker.elab.statistics = budget.statistics;
    let result = (|| {
        for candidate in matches {
            let mut work = checker.conversion_candidate(family, candidate)?;
            while let Some(id) = checker.schedule_conversion_requirement(&mut work, candidate)? {
                checker.solve_goal(&id)?;
            }
            checker.finish_conversion_candidate(work, candidate)?;
        }
        Ok(())
    })();
    budget.statistics = checker.elab.statistics;
    (checker.evidence, result)
}

pub(in crate::analysis::scalars) struct ConversionWork<'r, 'g> {
    family: &'r ConversionFamily<'g>,
    complete: bool,
    required_provider: bool,
    candidates: Vec<(PairMatch<'g>, CandidateWork<'r, 'g>)>,
    settled: Vec<PairMatch<'g>>,
}

struct CandidateWork<'r, 'g> {
    body: &'r ordinary::ImplementationFacts<'g>,
    site: Site<'g>,
    requirements: Vec<(Site<'g>, bool, &'r RequirementMetadata<'g>)>,
    taken: Vec<bool>,
    slots: Vec<Option<Type<'g>>>,
    waiting: Option<(GoalId, Option<Type<'g>>)>,
    disproved: bool,
    pending: bool,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars::generics) fn fallible_conversion(
        &self,
        target: Option<RequirementTarget<'g>>,
    ) -> bool {
        matches!(target, Some(RequirementTarget::Family(id)) if matches!(self.conversion_family(id).map(|role| role.premise), Some(RoleHypothesis::Fallible { .. })))
    }

    pub(in crate::analysis::scalars::generics) fn pending_conversion_outputs(
        &mut self,
        owner: &DeclarationId,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
    ) -> Work<BTreeSet<usize>> {
        let mut pending = BTreeSet::new();
        if self.conversions.is_some()
            && let Some(metadata) = self.input.ordinary.unwrap().headers().get(owner)
        {
            for clause in &metadata.requirements {
                self.elab.step(1)?;
                if self.fallible_conversion(clause.target)
                    && let [_, _, Some(error)] = clause.arguments.as_slice()
                {
                    self.open_slots(error, binders, slots, &mut pending, 1)?;
                }
            }
        }
        Ok(pending)
    }

    /// A standalone call has no overload determination of its own. Its ready
    /// conversion clauses issue pair goals and use their exact output equations.
    pub(in crate::analysis::scalars::generics) fn apply_conversion_outputs(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        owner: &DeclarationId,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
    ) -> Work<Option<bool>> {
        if self.conversions.is_none() {
            return Ok(None);
        }
        let Some(metadata) = self.input.ordinary.unwrap().headers().get(owner) else {
            return Ok(None);
        };
        let module = inference.module;
        let mut relevant = false;
        loop {
            let mut progressed = false;
            let mut settled = true;
            for clause in &metadata.requirements {
                self.elab.step(1)?;
                if !self.fallible_conversion(clause.target) {
                    continue;
                }
                relevant = true;
                let Some(RequirementTarget::Family(family)) = clause.target else {
                    unreachable!()
                };
                let [Some(to), Some(from), Some(expected_error)] = clause.arguments.as_slice()
                else {
                    settled = false;
                    continue;
                };
                if !self.closed(to, binders, &|i| slots[i].is_some(), 1)?
                    || !self.closed(from, binders, &|i| slots[i].is_some(), 1)?
                {
                    settled = false;
                    continue;
                }
                let key = (node, clause.node);
                let goal = if let Some(prior) = inference.conversion_requirements.get(&key) {
                    prior.clone()
                } else {
                    let substitution = self.slot_substitutions(binders, slots)?;
                    self.elab.reserve(3)?;
                    let inputs = vec![
                        self.substitute(to, &substitution, 1)?,
                        self.substitute(from, &substitution, 1)?,
                    ];
                    let site = Self::site(module, node);
                    let origin = self.evidence_module(&owner.module)?;
                    let goal = self.issue_determination(
                        GoalJudgment::Determine {
                            family,
                            inputs,
                            filters: Vec::new(),
                        },
                        site,
                        Self::site(origin, clause.node),
                        &clause.routes,
                        None,
                    )?;
                    self.solve_goal(&goal)?;
                    inference.conversion_requirements.insert(key, goal.clone());
                    goal
                };
                let proof = self.evidence.get(&goal).unwrap();
                let Some(output) = &proof.output else {
                    settled = false;
                    continue;
                };
                let actual = output.arguments[2].clone();
                let actual_site = output.site;
                let origin = proof.source;
                let premise = proof.premise;
                let record = proof.equalities.is_empty();
                let already_bound =
                    self.closed(expected_error, binders, &|i| slots[i].is_some(), 1)?;
                self.elab.reserve(slots.len())?;
                let mut proposed = slots.to_vec();
                let agrees =
                    self.input_equation(expected_error, &actual, binders, &mut proposed, &[], 1)?;
                if agrees {
                    slots.clone_from_slice(&proposed);
                    progressed |= !already_bound;
                    if let Some(premise) = premise {
                        self.mark_conversion_dependency(inference, node, premise, true)?;
                        self.arrow_premise =
                            Some(self.arrow_premise.map_or(premise, |p| p.combine(premise)));
                    }
                } else {
                    settled = false;
                }
                if record {
                    let substitution = self.slot_substitutions(binders, slots)?;
                    let requested = self.substitute(expected_error, &substitution, 1)?;
                    self.goal_equality(
                        goal.index,
                        Some(2),
                        requested,
                        actual,
                        origin,
                        actual_site,
                    )?;
                }
            }
            if relevant && !inference.conversion_clause_calls.contains(&node) {
                self.elab.reserve(1)?;
                inference.conversion_clause_calls.insert(node);
            }
            if !progressed {
                return Ok(relevant.then_some(settled));
            }
        }
    }

    pub(in crate::analysis::scalars) fn conversion_family(
        &self,
        id: &DeclarationId,
    ) -> Option<&'r ConversionFamily<'g>> {
        self.conversions?.get(id)
    }

    fn conversion_candidate(
        &mut self,
        family: &ConversionFamily<'g>,
        candidate: &PairMatch<'g>,
    ) -> Work<CandidateWork<'r, 'g>> {
        let input = self.input.ordinary.unwrap();
        self.elab.step(family.registrations.len())?;
        let entry = family
            .registrations
            .iter()
            .find(|r| &r.source.declaration == candidate.implementation)
            .unwrap();
        let declared = input.captured_family(&entry.source.family).unwrap();
        self.elab.step(declared.implementations.len())?;
        let body = declared
            .implementations
            .iter()
            .find(|b| &b.source.declaration == candidate.implementation)
            .unwrap();
        let module = self.evidence_module(&entry.source.declaration.module)?;
        let site = Self::site(module, entry.source.signature_node);
        self.elab.reserve(body.header.binders.len())?;
        let mut slots = vec![None; body.header.binders.len()];
        self.elab.step(candidate.substitution.len())?;
        for s in &candidate.substitution {
            slots[s.position] = Some(s.argument.clone());
        }
        let mut requirements = Vec::new();
        let mut pending = false;
        for (owner, family) in [
            (&entry.source.family, true),
            (&entry.source.declaration, false),
        ] {
            let Some(metadata) = input.headers().get(owner) else {
                pending = true;
                continue;
            };
            let module = self.evidence_module(&owner.module)?;
            self.elab.reserve(metadata.requirements.len() * 2)?;
            requirements.extend(
                metadata
                    .requirements
                    .iter()
                    .map(|r| (Self::site(module, r.node), family, r)),
            );
        }
        Ok(CandidateWork {
            body,
            site,
            taken: vec![false; requirements.len()],
            requirements,
            slots,
            waiting: None,
            disproved: false,
            pending,
        })
    }

    fn schedule_conversion_requirement(
        &mut self,
        work: &mut CandidateWork<'r, 'g>,
        candidate: &mut PairMatch<'g>,
    ) -> Work<Option<GoalId>> {
        let binders = &work.body.header.binders;
        if let Some((id, expected_error)) = work.waiting.take() {
            let facts = self.evidence.get(&id).unwrap();
            match (facts.hypothetical, facts.state) {
                (Some(GoalVerdict::Disproved), _) | (_, GoalState::Disproved) => {
                    work.disproved = true
                }
                (Some(GoalVerdict::Proved), _) | (_, GoalState::Proved) => {
                    if let Some(expected) = expected_error {
                        if let Some(actual) = facts
                            .output
                            .as_ref()
                            .and_then(|output| output.arguments.get(2))
                            .cloned()
                        {
                            let site = facts.output.as_ref().unwrap().site;
                            let origin = facts.source;
                            // Equation against the selected output, not a selection filter.
                            self.elab.reserve(work.slots.len())?;
                            let mut proposed = work.slots.clone();
                            if self.input_equation(
                                &expected,
                                &actual,
                                binders,
                                &mut proposed,
                                &[],
                                1,
                            )? {
                                work.slots = proposed;
                                let substitution = self.conversion_substitution(work)?;
                                let requested = self.substitute(&expected, &substitution, 1)?;
                                self.goal_equality(
                                    id.index,
                                    Some(2),
                                    requested,
                                    actual,
                                    origin,
                                    site,
                                )?;
                            } else {
                                let substitution = self.conversion_substitution(work)?;
                                let requested = self.substitute(&expected, &substitution, 1)?;
                                let mut budget = self.conversion_budget();
                                let intersects = patterns::PairDomain::new(
                                    [&requested, &actual].into_iter(),
                                    None,
                                    &mut budget,
                                )
                                .and_then(|pair| pair.identity_intersects(&mut budget));
                                self.elab.statistics = budget.statistics;
                                if intersects? {
                                    work.pending = true;
                                } else {
                                    work.disproved = true;
                                }
                                self.goal_equality(
                                    id.index,
                                    Some(2),
                                    requested,
                                    actual,
                                    origin,
                                    site,
                                )?;
                            }
                        } else {
                            work.pending = true;
                        }
                    }
                }
                _ => work.pending = true,
            }
        }
        if work.disproved {
            return Ok(None);
        }
        let substitutions = self.conversion_substitution(work)?;
        for (i, &(source, from_family, clause)) in work.requirements.iter().enumerate() {
            self.elab.step(1)?;
            if work.taken[i] {
                continue;
            }
            let Some(target) = clause.target else {
                continue;
            };
            let fallible = matches!(target, RequirementTarget::Family(id) if matches!(self.conversion_family(id).map(|r| r.premise), Some(RoleHypothesis::Fallible { .. })));
            let Some(arity) = self.requirement_arity(target)? else {
                continue;
            };
            if clause.arguments.len() != arity || (fallible && arity != 3) {
                continue;
            }
            let inputs = arity - usize::from(fallible);
            self.elab.reserve(inputs)?;
            let mut arguments = Vec::with_capacity(inputs);
            let mut output = None;
            for (position, ty) in clause.arguments.iter().enumerate() {
                let Some(ty) = ty else {
                    break;
                };
                let template = if from_family {
                    self.substitute(ty, work.body.substitution.as_deref().unwrap(), 1)?
                } else {
                    ty.clone()
                };
                if fallible && position == 2 {
                    // Keep the candidate binder occurrences for the waiting equation.
                    output = Some(template);
                } else if self.closed(&template, binders, &|i| work.slots[i].is_some(), 1)? {
                    arguments.push(self.substitute(&template, &substitutions, 1)?);
                } else {
                    break;
                }
            }
            if arguments.len() != inputs || (fallible && output.is_none()) {
                continue;
            }
            let judgment = if let RequirementTarget::Family(id) = target
                && fallible
            {
                GoalJudgment::Determine {
                    family: id,
                    inputs: arguments,
                    filters: Vec::new(),
                }
            } else if Self::requirement_shape(target, &arguments) {
                GoalJudgment::Requirement { target, arguments }
            } else {
                continue;
            };
            let id = self.issue_goal(judgment, work.site, source, &clause.routes, false)?;
            let owner = &work.body.source.family;
            if !self.installed_conversion(owner) {
                self.add_goal_premise(&id, InputPremise::HypotheticalConversionRole);
            }
            self.elab.reserve(1)?;
            candidate.requirements.push(id.clone());
            work.taken[i] = true;
            work.waiting = Some((id.clone(), output));
            return Ok(Some(id));
        }
        Ok(None)
    }

    fn conversion_substitution(
        &mut self,
        work: &CandidateWork<'r, 'g>,
    ) -> Work<Vec<Substitution<'g>>> {
        self.elab.reserve(work.slots.len())?;
        Ok(work
            .body
            .header
            .binders
            .iter()
            .zip(&work.slots)
            .filter_map(|(binder, slot)| {
                slot.as_ref().map(|argument| Substitution {
                    binder: &binder.declaration.id,
                    position: binder.position,
                    argument: argument.clone(),
                })
            })
            .collect())
    }

    fn finish_conversion_candidate(
        &mut self,
        work: CandidateWork<'r, 'g>,
        candidate: &mut PairMatch<'g>,
    ) -> Work<()> {
        self.elab.step(work.taken.len())?;
        if work.disproved {
            candidate.applicability = Applicability::Disproved;
        } else if candidate.covers && !work.pending && work.taken.iter().all(|taken| *taken) {
            self.elab.step(work.slots.len())?;
            if work.slots.iter().any(Option::is_none) {
                return Ok(());
            }
            let substitution = self.conversion_substitution(&work)?;
            let DeclarationShape::Function { parameters, result } = &work.body.header.shape else {
                unreachable!()
            };
            let parameter = self.substitute(&parameters[0].ty, &substitution, 1)?;
            let result = self.substitute(result, &substitution, 1)?;
            if let Some(slot) = work.body.substitution.as_ref().unwrap().get(2) {
                candidate.error = Some(self.substitute(&slot.argument, &substitution, 1)?);
            }
            candidate.signature = Some(PairSignature {
                site: work.site,
                parameter,
                result,
            });
            candidate.substitution = substitution;
            candidate.applicability = Applicability::Proved;
        }
        Ok(())
    }

    pub(super) fn begin_conversion_goal(
        &mut self,
        goal: usize,
        family: &'r ConversionFamily<'g>,
        inputs: &[Type<'g>],
        filters: &[Substitution<'g>],
    ) -> Work<GoalState> {
        let [destination, source] = inputs else {
            return Ok(blocked(PairBlocker::EndpointContext));
        };
        let infallible = matches!(family.premise, RoleHypothesis::Infallible(_));
        let GoalJudgment::Determine { family: id, .. } = self.evidence.goals[goal].judgment else {
            unreachable!()
        };
        let header = self.evidence_family(id)?.unwrap();
        let filter = filters.first().map(|s| &s.argument);
        let installed = self.installed_conversion(id);
        if installed
            && let Some(state) = self.installed_protected_goal(
                goal,
                &header.header,
                !infallible,
                destination,
                source,
                filter,
                family.issues.is_empty(),
            )?
        {
            return Ok(state);
        }
        if !family.issues.is_empty() {
            return Ok(blocked(PairBlocker::Registration));
        }
        let mut budget = self.conversion_budget();
        let pair = patterns::PairDomain::new(inputs.iter(), filter, &mut budget);
        self.elab.statistics = budget.statistics;
        let pair = pair?;
        let mut budget = self.conversion_budget();
        let protected = if installed {
            Ok(None)
        } else {
            pairs::protected_pair(family.premise, destination, source, &pair, &mut budget)
        };
        self.elab.statistics = budget.statistics;
        if let Some(protected) = protected? {
            return Ok(match protected {
                pairs::ProtectedPair::Identity { filter_conflict } => {
                    if pair.symbolic() && filter_conflict {
                        blocked(PairBlocker::PotentialProtectedPair)
                    } else {
                        self.evidence.goals[goal].conversion_outcome =
                            Some(PairOutcome::RequiredIdentity { filter_conflict });
                        if filter_conflict {
                            GoalState::Disproved
                        } else {
                            blocked(PairBlocker::RequiredIdentityProvider)
                        }
                    }
                }
                pairs::ProtectedPair::NumericGap => {
                    self.evidence.goals[goal].conversion_outcome = Some(PairOutcome::NumericGap);
                    GoalState::Disproved
                }
                pairs::ProtectedPair::NumericProvider => {
                    blocked(PairBlocker::RequiredNumericProvider)
                }
            });
        }
        let maybe_numeric =
            |ty: &Type<'_>| numeric(ty) || matches!(ty.as_ref(), TypeTerm::Parameter { .. });
        let maybe_numeric = maybe_numeric(destination) && maybe_numeric(source);
        let mut budget = self.conversion_budget();
        let maybe_identity = if infallible {
            pair.identity_intersects(&mut budget)
        } else {
            Ok(false)
        };
        self.elab.statistics = budget.statistics;
        let maybe_identity = maybe_identity?;
        if let Some(state) = self.conversion_assumption(
            goal,
            family,
            &header.header,
            inputs,
            &pair,
            filter,
            maybe_numeric,
        )? {
            return Ok(state);
        }
        if maybe_numeric || maybe_identity {
            return Ok(blocked(PairBlocker::PotentialProtectedPair));
        }
        let mut budget = self.conversion_budget();
        let mut matches = Vec::new();
        let complete = pairs::find_matches(header, family, &pair, &mut matches, &mut budget);
        self.elab.statistics = budget.statistics;
        let complete = complete?;
        self.elab.reserve(matches.len() * 2 + 1)?;
        let mut candidates = Vec::with_capacity(matches.len());
        for m in matches.into_iter().rev() {
            let work = self.conversion_candidate(family, &m)?;
            candidates.push((m, work));
        }
        let required_provider = installed
            && self
                .installed_conversions
                .unwrap()
                .promises_registered_pair(destination, source, !infallible)
            && match filter {
                Some(filter) => self.same(filter, source)?,
                None => true,
            };
        let work = ConversionWork {
            family,
            complete,
            required_provider,
            candidates,
            settled: Vec::new(),
        };
        Ok(self
            .advance_conversion_goal(goal, work)?
            .unwrap_or(GoalState::Proved))
    }

    #[allow(clippy::too_many_arguments)]
    fn installed_protected_goal(
        &mut self,
        goal: usize,
        header: &DeclarationFacts<'g>,
        fallible: bool,
        destination: &Type<'g>,
        source: &Type<'g>,
        filter: Option<&Type<'g>>,
        registration_valid: bool,
    ) -> Work<Option<GoalState>> {
        use crate::analysis::typed::ConversionProtection;
        let Some(protected) = self.protected_conversion(fallible, destination, source)? else {
            return Ok(None);
        };
        self.evidence.goals[goal].conversion_protection = Some(protected.protection);
        let numeric_error = self.installed_conversions.unwrap().numeric_error;
        if !registration_valid {
            // A linked operation may be known without proving it is the call's winner.
            if !fallible || numeric_error.is_some() {
                self.evidence.goals[goal].conversion_provider = protected.provider;
            }
            return Ok(Some(blocked(PairBlocker::Registration)));
        }
        if protected.protection == ConversionProtection::ProhibitedNumeric {
            self.evidence.goals[goal].conversion_outcome = Some(PairOutcome::NumericGap);
            return Ok(Some(GoalState::Disproved));
        }
        if let Some(filter) = filter
            && !self.same(filter, source)?
        {
            self.evidence.goals[goal].conversion_outcome = Some(PairOutcome::NoMatch);
            return Ok(Some(GoalState::Disproved));
        }
        let Some(provider) = protected.provider else {
            return Ok(Some(blocked(PairBlocker::RequiredNumericProvider)));
        };
        self.elab.reserve(2 + usize::from(fallible) * 2)?;
        let mut arguments = vec![destination.clone(), source.clone()];
        let result = if fallible {
            let Some(error) = numeric_error else {
                return Ok(Some(blocked(PairBlocker::RequiredNumericProvider)));
            };
            let error = Arc::new(TypeTerm::Nominal {
                declaration: &error.id,
                arguments: Vec::new(),
            });
            arguments.push(error.clone());
            Arc::new(TypeTerm::Nominal {
                declaration: &self.result.unwrap().declaration.id,
                arguments: vec![destination.clone(), error],
            })
        } else {
            destination.clone()
        };
        let module = self.evidence_module(&header.declaration.id.module)?;
        let site = Self::site(module, header.declaration.node);
        let facts = &mut self.evidence.goals[goal];
        facts.conversion_provider = Some(provider);
        facts.conversion_parameter = Some(source.clone());
        facts.output = Some(GoalOutput {
            arguments,
            result,
            site,
        });
        Ok(Some(GoalState::Proved))
    }

    fn conversion_budget(&self) -> declarations::Budget {
        declarations::Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn conversion_assumption(
        &mut self,
        goal: usize,
        family: &ConversionFamily<'g>,
        header: &DeclarationFacts<'g>,
        inputs: &[Type<'g>],
        pair: &patterns::PairDomain<'g>,
        filter: Option<&Type<'g>>,
        maybe_numeric: bool,
    ) -> Work<Option<GoalState>> {
        let mut output: Option<GoalOutput<'g>> = None;
        let DeclarationShape::Function { result, .. } = &header.shape else {
            unreachable!()
        };
        for i in 0..self.evidence.goals[goal].assumptions.len() {
            self.elab.step(1)?;
            let id = self.evidence.goals[goal].assumptions[i].clone();
            let fact = &self.evidence.goals[id.index];
            let GoalJudgment::Requirement {
                target: RequirementTarget::Family(owner),
                arguments,
            } = &fact.judgment
            else {
                continue;
            };
            if **owner != header.declaration.id || arguments.len() != header.binders.len() {
                continue;
            }
            self.elab.reserve(arguments.len())?;
            let arguments = arguments.clone();
            let site = fact.source;
            if !self.same_terms(&arguments[..2], inputs)? {
                continue;
            }
            if let Some(filter) = filter {
                if maybe_numeric && !self.same(filter, &inputs[1])? {
                    continue;
                }
                let mut budget = self.conversion_budget();
                let agrees = (|| {
                    if matches!(family.premise, RoleHypothesis::Infallible(_))
                        && !pair.identity_filter_agrees(&mut budget)?
                    {
                        return Ok(false);
                    }
                    for registered in &family.registrations {
                        budget.step(1)?;
                        let Some(alternatives) = &registered.alternatives else {
                            return Ok(false);
                        };
                        if !pair.filters_agree(&registered.key, alternatives, &mut budget)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                })();
                self.elab.statistics = budget.statistics;
                if !agrees? {
                    continue;
                }
            }
            self.link_goal(goal, id)?;
            if let Some(previous) = &output {
                if !self.same_terms(&previous.arguments, &arguments)? {
                    return Ok(Some(GoalState::Unresolved(
                        GoalBlocker::ConflictingAssumptions,
                    )));
                }
            } else {
                let substitutions = self.family_substitutions(header, &arguments)?;
                let result = self.substitute(result, &substitutions, 1)?;
                output = Some(GoalOutput {
                    arguments,
                    result,
                    site,
                });
            }
        }
        let facts = &mut self.evidence.goals[goal];
        facts.output = output;
        Ok(facts.output.is_some().then_some(GoalState::Proved))
    }

    pub(super) fn advance_conversion_goal(
        &mut self,
        goal: usize,
        mut work: ConversionWork<'r, 'g>,
    ) -> Work<Option<GoalState>> {
        while let Some((candidate, stage)) = work.candidates.last_mut() {
            if let Some(id) = self.schedule_conversion_requirement(stage, candidate)? {
                self.link_goal(goal, id)?;
                self.conversion_work.insert(goal, work);
                return Ok(None);
            }
            let (mut candidate, stage) = work.candidates.pop().unwrap();
            self.finish_conversion_candidate(stage, &mut candidate)?;
            work.settled.push(candidate);
        }
        if !work.complete {
            return Ok(Some(blocked(PairBlocker::SourceAdmission)));
        }
        let mut budget = declarations::Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let ranked = pairs::rank(work.family, &work.settled, &mut Vec::new(), &mut budget);
        self.elab.statistics = budget.statistics;
        let ranked = ranked?;
        if work.required_provider
            && let Some(core) = self.installed_conversions
            && matches!(ranked, Some(PairOutcome::NoMatch | PairOutcome::Winner(_)))
            && !matches!(ranked, Some(PairOutcome::Winner(id)) if core.registered.contains_key(id))
        {
            // A missing required provider does not license another winner or a gap.
            return Ok(Some(blocked(PairBlocker::RequiredCoreProvider)));
        }
        let winner = match ranked {
            Some(PairOutcome::Winner(id)) => id,
            Some(PairOutcome::NoMatch | PairOutcome::Ambiguous) => {
                self.evidence.goals[goal].conversion_outcome = ranked;
                return Ok(Some(GoalState::Disproved));
            }
            _ => return Ok(Some(blocked(PairBlocker::Applicability))),
        };
        self.elab.step(work.settled.len())?;
        let candidate = work
            .settled
            .into_iter()
            .find(|c| c.implementation == winner)
            .unwrap();
        let signature = candidate.signature.unwrap();
        let facts = &mut self.evidence.goals[goal];
        let GoalJudgment::Determine { inputs, .. } = &facts.judgment else {
            unreachable!()
        };
        self.elab
            .reserve(inputs.len() + usize::from(candidate.error.is_some()))?;
        let mut arguments = inputs.clone();
        arguments.extend(candidate.error);
        facts.conversion_outcome = ranked;
        facts.conversion_provider = self
            .installed_conversions
            .and_then(|core| core.registered.get(winner))
            .map(|r| r.operation);
        facts.conversion_parameter = Some(signature.parameter);
        facts.output = Some(GoalOutput {
            arguments,
            result: signature.result,
            site: signature.site,
        });
        facts.selection = Some(GoalSelection {
            implementation: winner,
            alternative: candidate.alternative,
            substitution: candidate.substitution,
        });
        Ok(Some(GoalState::Proved))
    }
}

fn blocked(reason: PairBlocker) -> GoalState {
    GoalState::Unresolved(GoalBlocker::ConversionPair(reason))
}
