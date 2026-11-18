//! Registration-time routes are exact type dependencies, not applicability proofs.
use super::*;
use ordinary::{ImplementationFacts, RequirementTarget, shape};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputDetermination {
    Infallible,
    /// Every error slot has a structural route. Required goals may still fail.
    Determined,
    Underdetermined,
    Pending,
}

#[derive(Debug)]
pub struct DeterminationRoute<'g> {
    pub owner: &'g DeclarationId,
    pub node: NodeId,
    pub span: &'g Span,
    pub reads: BTreeSet<usize>,
    pub writes: BTreeSet<usize>,
}

fn positions(
    term: &Type<'_>,
    bindings: &[Substitution<'_>],
    into: &mut BTreeSet<usize>,
    budget: &mut Budget,
    depth: usize,
) -> Work<()> {
    budget.step(1)?;
    if depth > budget.limits.max_depth.min(64) {
        return Err(());
    }
    budget.statistics.max_depth = budget.statistics.max_depth.max(depth);
    if let TypeTerm::Parameter { position, .. } = term.as_ref() {
        if let Some(binding) = bindings.get(*position) {
            return positions(&binding.argument, &[], into, budget, depth + 1);
        }
        if !into.contains(position) {
            budget.reserve(1)?;
            into.insert(*position);
        }
        return Ok(());
    }
    for child in shape(term).1 {
        positions(child, bindings, into, budget, depth + 1)?;
    }
    Ok(())
}

pub(super) fn routes<'g>(
    input: &OrdinaryAnalysis<'g>,
    body: &ImplementationFacts<'g>,
    variables: &[Option<usize>],
    error: &Type<'g>,
    budget: &mut Budget,
) -> Work<(OutputDetermination, Vec<DeterminationRoute<'g>>)> {
    let mut needed = BTreeSet::new();
    positions(error, &[], &mut needed, budget, 1)?;
    budget.step(needed.len())?;
    if needed.iter().all(|&i| variables[i].is_some()) {
        return Ok((OutputDetermination::Determined, Vec::new()));
    }
    let mut pending = Vec::<Option<DeterminationRoute<'g>>>::new();
    let mut unclassified = false;
    for (owner, bindings) in [
        (&body.source.family, body.substitution.as_deref().unwrap()),
        (&body.source.declaration, &[][..]),
    ] {
        let Some(header) = input.headers().get(owner) else {
            unclassified = true;
            continue;
        };
        for clause in &header.requirements {
            budget.step(1)?;
            let canonical = match clause.target {
                Some(RequirementTarget::Family(family)) => family == &body.source.family,
                None if clause.arguments.len() == 3 => {
                    unclassified = true;
                    false
                }
                _ => false,
            };
            if !canonical {
                continue;
            }
            let [Some(to), Some(from), Some(error)] = clause.arguments.as_slice() else {
                unclassified = true;
                continue;
            };
            let (mut reads, mut writes) = (BTreeSet::new(), BTreeSet::new());
            positions(to, bindings, &mut reads, budget, 1)?;
            positions(from, bindings, &mut reads, budget, 1)?;
            positions(error, bindings, &mut writes, budget, 1)?;
            budget.reserve(1)?;
            pending.push(Some(DeterminationRoute {
                owner,
                node: clause.node,
                span: clause.span,
                reads,
                writes,
            }));
        }
    }
    budget.reserve(variables.len())?;
    let mut known: Vec<_> = variables.iter().map(Option::is_some).collect();
    let mut reached = Vec::new();
    loop {
        let mut changed = false;
        for route in &mut pending {
            budget.step(1)?;
            let Some(candidate) = route else {
                continue;
            };
            budget.step(candidate.reads.len())?;
            if !candidate.reads.iter().all(|&i| known[i]) {
                continue;
            }
            let ready = route.take().unwrap();
            budget.step(ready.writes.len())?;
            let mut new = false;
            for &i in &ready.writes {
                new |= !std::mem::replace(&mut known[i], true);
            }
            if new {
                changed = true;
                budget.reserve(1)?;
                reached.push(ready);
            }
        }
        if !changed {
            break;
        }
    }
    budget.step(needed.len())?;
    let output = if needed.iter().all(|&i| known[i]) {
        OutputDetermination::Determined
    } else if unclassified {
        OutputDetermination::Pending
    } else {
        OutputDetermination::Underdetermined
    };
    Ok((output, reached))
}
