//! Project exact destination, admitted source and an optional receiver equation together.
use super::*;
use relations::{match_vector, unify};
use source::{materialize, restricted_domain, rigid_domain};

pub(in crate::analysis) struct PairDomain<'g> {
    domain: InputKey<'g>,
    rigids: Vec<(&'g DeclarationId, usize)>,
    filtered: bool,
}

pub(in crate::analysis) struct PairProjection<'g> {
    pub alternative: usize,
    pub covers: bool,
    pub substitution: Vec<Substitution<'g>>,
}

impl<'g> PairDomain<'g> {
    pub(in crate::analysis) fn new<'a>(
        inputs: impl Iterator<Item = &'a Type<'g>>,
        filter: Option<&'a Type<'g>>,
        budget: &mut Budget,
    ) -> Work<Self>
    where
        'g: 'a,
    {
        let (domain, rigids) = rigid_domain(inputs.chain(filter), budget)?;
        Ok(Self {
            domain,
            rigids,
            filtered: filter.is_some(),
        })
    }

    pub(in crate::analysis) fn symbolic(&self) -> bool {
        !self.rigids.is_empty()
    }

    pub(in crate::analysis) fn exact_endpoints(&self, budget: &mut Budget) -> Work<bool> {
        self.domain.equal_pair_endpoints(budget)
    }

    fn columns(&self) -> usize {
        2 + usize::from(self.filtered)
    }

    fn append_filter(
        &self,
        pattern: &InputKey<'g>,
        key: &InputKey<'g>,
        apply_filter: bool,
        budget: &mut Budget,
    ) -> Work<InputKey<'g>> {
        let mut nodes = Vec::new();
        budget.reserve(pattern.nodes.len())?;
        nodes.extend_from_slice(&pattern.nodes);
        if self.filtered {
            if apply_filter {
                let term = terms(&key.nodes).nth(1).unwrap();
                budget.reserve(term.len())?;
                nodes.extend_from_slice(term);
            } else {
                // Indices are allocated by occurrence, so a key cannot already
                // use its node count as a variable index.
                budget.reserve(1)?;
                nodes.push(variable(key.nodes.len()));
            }
        }
        Ok(InputKey { nodes })
    }

    pub(in crate::analysis) fn project(
        &self,
        key: &InputKey<'g>,
        alternatives: &[InputKey<'g>],
        variables: &[Option<usize>],
        binders: &[Binder<'g>],
        budget: &mut Budget,
    ) -> Work<Option<PairProjection<'g>>> {
        let mut partial: Option<PairProjection<'g>> = None;
        for (alternative, pattern) in alternatives.iter().enumerate() {
            budget.step(1)?;
            let pattern = self.append_filter(pattern, key, true, budget)?;
            let Some(solved) = unify(&pattern, &self.domain, budget)? else {
                continue;
            };
            let covers = match_vector(
                &pattern.nodes,
                &self.domain.nodes,
                &mut BTreeMap::new(),
                budget,
            )?;
            let mut substitution = Vec::new();
            for (binder, variable) in binders.iter().zip(variables) {
                budget.step(1)?;
                if let Some(site) = variable.and_then(|i| solved.get(&(false, i)))
                    && let Some(argument) = materialize(*site, &solved, &self.rigids, budget, 1)?
                {
                    budget.reserve(1)?;
                    substitution.push(Substitution {
                        binder: &binder.declaration.id,
                        position: binder.position,
                        argument,
                    });
                }
            }
            let observed = PairProjection {
                alternative,
                covers,
                substitution,
            };
            if covers {
                return Ok(Some(observed));
            }
            if let Some(prior) = &mut partial {
                // Only use substitutions on which every intersecting
                // alternative agrees when disproving a partial match.
                let mut common = Vec::new();
                for binding in prior.substitution.drain(..) {
                    budget.step(observed.substitution.len())?;
                    if let Some(other) = observed
                        .substitution
                        .iter()
                        .find(|b| b.position == binding.position)
                    {
                        let left = ground(std::iter::once(&binding.argument), budget)?;
                        let right = ground(std::iter::once(&other.argument), budget)?;
                        if left.same(&right, budget)? {
                            budget.reserve(1)?;
                            common.push(binding);
                        }
                    }
                }
                prior.substitution = common;
            } else {
                partial = Some(observed);
            }
        }
        Ok(partial)
    }

    pub(in crate::analysis) fn filters_agree(
        &self,
        key: &InputKey<'g>,
        alternatives: &[InputKey<'g>],
        budget: &mut Budget,
    ) -> Work<bool> {
        if !self.filtered {
            return Ok(true);
        }
        let domains = |filtered, budget: &mut Budget| -> Work<Vec<_>> {
            let mut out = Vec::new();
            for alternative in alternatives {
                budget.step(1)?;
                let pattern = self.append_filter(alternative, key, filtered, budget)?;
                if let Some(solved) = unify(&pattern, &self.domain, budget)? {
                    budget.reserve(1)?;
                    out.push(restricted_domain(
                        &self.domain,
                        self.columns(),
                        &solved,
                        budget,
                    )?);
                }
            }
            Ok(out)
        };
        let full = domains(false, budget)?;
        let filtered = domains(true, budget)?;
        Ok(relations::included(&full, &filtered, budget)?
            && relations::included(&filtered, &full, budget)?)
    }

    pub(in crate::analysis) fn identity_intersects(&self, budget: &mut Budget) -> Work<bool> {
        budget.reserve(self.columns())?;
        let mut nodes = vec![variable(0), variable(0)];
        if self.filtered {
            nodes.push(variable(1));
        }
        Ok(unify(&InputKey { nodes }, &self.domain, budget)?.is_some())
    }

    pub(in crate::analysis) fn identity_filter_agrees(&self, budget: &mut Budget) -> Work<bool> {
        if !self.filtered {
            return Ok(true);
        }
        budget.reserve(6)?;
        let full = InputKey {
            nodes: vec![variable(0), variable(0), variable(1)],
        };
        let filtered = InputKey {
            nodes: vec![variable(0), variable(0), variable(0)],
        };
        let Some(base) = unify(&full, &self.domain, budget)? else {
            return Ok(true);
        };
        let Some(restricted) = unify(&filtered, &self.domain, budget)? else {
            return Ok(false);
        };
        let full = restricted_domain(&self.domain, self.columns(), &base, budget)?;
        let filtered = restricted_domain(&self.domain, self.columns(), &restricted, budget)?;
        relations::included(
            std::slice::from_ref(&full),
            std::slice::from_ref(&filtered),
            budget,
        )
    }
}

fn variable(index: usize) -> Node<'static> {
    Node {
        head: Head::Variable(index),
        children: 0,
        size: 1,
        height: 1,
    }
}
