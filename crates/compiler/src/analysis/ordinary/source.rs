//! Conditional source comparison. Inputs are elaborated by the shared body engine.
use super::*;

pub(in crate::analysis) struct SourceCandidate<'r, 'g> {
    pub body: &'r ImplementationFacts<'g>,
    pub alternative: usize,
    pub covers: bool,
    pub substitution: Vec<Substitution<'g>>,
}

pub(in crate::analysis) fn family<'r, 'g>(
    ordinary: &'r OrdinaryAnalysis<'g>,
    id: &DeclarationId,
    budget: &mut Budget,
) -> Work<Result<&'r FamilyFacts<'g>, SelectionBlocker>> {
    selection::eligible_family(ordinary, id, budget)
}

pub(in crate::analysis) fn input_positions(
    family: &FamilyFacts<'_>,
    budget: &mut Budget,
) -> Work<Vec<bool>> {
    let (_, input) = patterns::input_key(&family.header, budget)?;
    budget.reserve(input.len())?;
    Ok(input.iter().map(Option::is_some).collect())
}

pub(in crate::analysis) fn project<'r, 'g>(
    family: &'r FamilyFacts<'g>,
    inputs: &[Option<Type<'g>>],
    filters: &[(usize, &Type<'g>)],
    budget: &mut Budget,
) -> Work<Vec<SourceCandidate<'r, 'g>>> {
    patterns::project(family, inputs, filters, budget)
}

/// Demand a uniform winner when caller parameters are rigid. The augmented
/// alternatives establish viability and coverage; only original comparisons
/// can establish dominance. Do not count the same body's alternatives twice.
pub(in crate::analysis) fn rank<'g>(
    family: &FamilyFacts<'g>,
    viable: &[SourceCandidate<'_, 'g>],
    budget: &mut Budget,
) -> Work<Option<HypotheticalOutcome<'g>>> {
    if viable.is_empty() {
        return Ok(Some(HypotheticalOutcome::NoMatch));
    }
    let mut covers = BTreeMap::new();
    for candidate in viable {
        budget.step(1)?;
        let id = &candidate.body.source.declaration;
        if let Some(full) = covers.get_mut(id) {
            *full |= candidate.covers;
        } else {
            budget.reserve(1)?;
            covers.insert(id, candidate.covers);
        }
    }
    for (&body, &full) in &covers {
        budget.step(1)?;
        if !full {
            continue;
        }
        let mut wins = true;
        for &other in covers.keys() {
            budget.step(1)?;
            if other == body {
                continue;
            }
            let mut relation = None;
            for comparison in &family.comparisons {
                budget.step(1)?;
                if comparison.left == body && comparison.right == other {
                    relation = Some(comparison.left_in_right && !comparison.right_in_left);
                    break;
                }
                if comparison.left == other && comparison.right == body {
                    relation = Some(comparison.right_in_left && !comparison.left_in_right);
                    break;
                }
            }
            let Some(dominates) = relation else {
                return Ok(None);
            };
            if !dominates {
                wins = false;
                break;
            }
        }
        if wins {
            return Ok(Some(HypotheticalOutcome::Winner(body)));
        }
    }
    budget.step(covers.len())?;
    Ok(covers
        .values()
        .all(|full| *full)
        .then_some(HypotheticalOutcome::Ambiguous))
}
