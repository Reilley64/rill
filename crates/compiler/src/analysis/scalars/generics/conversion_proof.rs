//! Apply the shared evidence engine to established conversion candidates.
use super::*;
use crate::analysis::conversions::{Applicability, ConversionFamily, PairMatch};
use ordinary::{InputPremise, RequirementTarget};

pub(in crate::analysis) fn check_pair_candidates<'g>(
    input: &OrdinaryAnalysis<'g>,
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
    checker.arrow_premise = Some(InputPremise::HypotheticalConversionRole);
    checker.elab.statistics = budget.statistics;
    let result = (|| {
        for candidate in matches {
            checker.elab.step(family.registrations.len())?;
            let entry = family
                .registrations
                .iter()
                .find(|r| r.source.declaration == *candidate.implementation)
                .unwrap();
            let declared = input.captured_family(&entry.source.family).unwrap();
            checker.elab.step(declared.implementations.len())?;
            let body = declared
                .implementations
                .iter()
                .find(|b| b.source.declaration == *candidate.implementation)
                .unwrap();
            let module = checker.evidence_module(&entry.source.declaration.module)?;
            let site = Checker::site(module, entry.source.signature_node);
            let mut pending = candidate.substitution.len() != body.header.binders.len();
            let mut disproved = false;
            let header = body.substitution.as_deref().unwrap();
            checker.elab.reserve(header.len())?;
            let mut family_substitution = Vec::with_capacity(header.len());
            for slot in header {
                family_substitution.push(Substitution {
                    argument: checker.substitute(&slot.argument, &candidate.substitution, 1)?,
                    ..slot.clone()
                });
            }
            for (owner, substitutions) in [
                (&entry.source.family, family_substitution.as_slice()),
                (&entry.source.declaration, candidate.substitution.as_slice()),
            ] {
                let Some(metadata) = input.headers().get(owner) else {
                    pending = true;
                    continue;
                };
                let origin = checker.evidence_module(&owner.module)?;
                for requirement in &metadata.requirements {
                    checker.elab.step(1)?;
                    // Family clauses need dispatch by role before entering the worklist.
                    // Do not prove a conversion requirement using an ordinary family key.
                    if !matches!(requirement.target, Some(RequirementTarget::Predicate(_))) {
                        pending = true;
                        continue;
                    }
                    let Some(judgment) = checker.requirement_term(requirement, substitutions)?
                    else {
                        pending = true;
                        continue;
                    };
                    let GoalJudgment::Requirement { arguments, .. } = &judgment else {
                        unreachable!()
                    };
                    let mut ready = true;
                    for arg in arguments {
                        ready &= checker.closed(arg, &body.header.binders, &|_| false, 1)?;
                    }
                    if !ready {
                        pending = true;
                        continue;
                    }
                    let id = checker.issue_goal(
                        judgment,
                        site,
                        Checker::site(origin, requirement.node),
                        &requirement.routes,
                        false,
                    )?;
                    checker.solve_goal(&id)?;
                    let facts = checker.evidence.get(&id).unwrap();
                    match (facts.hypothetical, facts.state) {
                        (Some(GoalVerdict::Disproved), _) | (_, GoalState::Disproved) => {
                            disproved = true
                        }
                        (Some(GoalVerdict::Proved), _) | (_, GoalState::Proved) => (),
                        _ => pending = true,
                    }
                    checker.elab.reserve(1)?;
                    candidate.requirements.push(id);
                }
            }
            candidate.applicability = if disproved {
                Applicability::Disproved
            } else if pending {
                Applicability::Unresolved
            } else {
                candidate.error = family_substitution.get(2).map(|s| s.argument.clone());
                Applicability::Proved
            };
        }
        Ok(())
    })();
    budget.statistics = checker.elab.statistics;
    (checker.evidence, result)
}
