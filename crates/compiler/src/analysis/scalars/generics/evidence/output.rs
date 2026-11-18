//! Output-producing clauses of the one selected ordinary header.
use super::*;

#[derive(Debug)]
pub(super) struct OutputEquation<'g> {
    child: GoalId,
    outputs: Vec<(usize, Type<'g>)>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars::generics) fn pending_output_slots(
        &mut self,
        contracts: &[(&DeclarationId, &[Substitution<'g>])],
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
    ) -> Work<Option<BTreeSet<usize>>> {
        let mut pending = BTreeSet::new();
        for &(owner, bindings) in contracts {
            let Some(header) = self.input.ordinary.unwrap().headers().get(owner) else {
                continue;
            };
            for requirement in &header.requirements {
                self.elab.step(1)?;
                let Some(target) = requirement.target else {
                    return Ok(None);
                };
                // Callable clauses can supply exact inputs as well as results.
                let positions = match target {
                    RequirementTarget::Family(target) => {
                        let Some(family) = self.evidence_family(target)? else {
                            return Ok(None);
                        };
                        let mut budget = declarations::Budget {
                            limits: self.elab.limits,
                            statistics: self.elab.statistics,
                        };
                        let positions = ordinary::source::input_positions(family, &mut budget);
                        self.elab.statistics = budget.statistics;
                        positions?
                    }
                    RequirementTarget::Predicate(id)
                        if matches!(Predicate::of(id), Predicate::Callable(_)) =>
                    {
                        self.elab.reserve(3)?;
                        vec![true, false, false]
                    }
                    RequirementTarget::Predicate(_) => continue,
                };
                if positions.len() != requirement.arguments.len() {
                    return Ok(None);
                }
                for (term, is_input) in requirement.arguments.iter().zip(positions) {
                    self.elab.step(1)?;
                    let Some(term) = term else { return Ok(None) };
                    if !is_input {
                        let term = self.substitute(term, bindings, 1)?;
                        self.open_slots(&term, binders, slots, &mut pending, 1)?;
                    }
                }
            }
        }
        Ok(Some(pending))
    }

    pub(super) fn try_source_output_default(
        &mut self,
        family: &'r ordinary::FamilyFacts<'g>,
        body: &'r ordinary::ImplementationFacts<'g>,
        slots: &mut [Option<Type<'g>>],
    ) -> Work<Option<DefaultArgumentUse<'g>>> {
        let ordinary = self.input.ordinary.unwrap();
        let callee = &family.header.declaration.id;
        let Some(written) = ordinary.headers().get(callee) else {
            return Ok(None);
        };
        if written.defaults.is_empty() {
            return Ok(None);
        }
        let binders = &body.header.binders;
        let header = body.substitution.as_deref().unwrap();
        let contracts = [(callee, header), (&body.source.declaration, &[][..])];
        let Some(pending) = self.pending_output_slots(&contracts, binders, slots)? else {
            return Ok(None);
        };
        for default in &written.defaults {
            self.elab.step(header.len())?;
            let Some(output) = header.iter().find(|s| s.binder == default.binder) else {
                continue;
            };
            // A written default supplies a complete term, not a projection
            // onto its currently free leaves.
            if !self.closed(&output.argument, binders, &|i| !pending.contains(&i), 1)?
                || self.closed(&output.argument, binders, &|i| slots[i].is_some(), 1)?
            {
                continue;
            }
            let Some(ty) = &default.ty else {
                continue;
            };
            let solved = self.slot_substitutions(binders, slots)?;
            let mut known = Vec::new();
            let mut positions = BTreeSet::new();
            for slot in &header[..output.position] {
                if self.closed(&slot.argument, binders, &|i| slots[i].is_some(), 1)? {
                    self.elab.reserve(2)?;
                    known.push(Substitution {
                        argument: self.substitute(&slot.argument, &solved, 1)?,
                        ..slot.clone()
                    });
                    positions.insert(slot.position);
                }
            }
            if self.closed(ty, &family.header.binders, &|i| positions.contains(&i), 1)? {
                let actual = self.substitute(ty, &known, 1)?;
                if self.free_output_equation(&output.argument, &actual, binders, slots, &pending)? {
                    return Ok(Some(DefaultArgumentUse {
                        binder: default.binder,
                        node: default.node,
                        span: default.span,
                    }));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn request_selected_output(
        &mut self,
        goal: usize,
        family: &'r ordinary::FamilyFacts<'g>,
        body: &'r ordinary::ImplementationFacts<'g>,
        selected: &[Substitution<'g>],
    ) -> Work<GoalState> {
        let binders = &body.header.binders;
        let mut solved = BTreeSet::new();
        for sub in selected {
            self.elab.reserve(1)?;
            solved.insert(sub.position);
        }
        let ordinary = self.input.ordinary.unwrap();
        for (owner, bindings) in [
            (
                &family.header.declaration.id,
                body.substitution.as_deref().unwrap(),
            ),
            (&body.source.declaration, &[][..]),
        ] {
            let Some(header) = ordinary.headers().get(owner) else {
                continue;
            };
            let module = self.evidence_module(&owner.module)?;
            for requirement in &header.requirements {
                self.elab.step(1)?;
                let Some(RequirementTarget::Family(target)) = requirement.target else {
                    continue;
                };
                let source = Self::site(module, requirement.node);
                self.elab
                    .step(self.evidence.goals[goal].dependencies.len())?;
                if self.evidence.goals[goal].dependencies.iter().any(|id| {
                    let child = &self.evidence.goals[id.index];
                    child.source.module == source.module && child.source.node == source.node
                }) {
                    continue;
                }
                let Some(target_facts) = self.evidence_family(target)? else {
                    continue;
                };
                if target_facts.header.binders.len() != requirement.arguments.len() {
                    continue;
                }
                let mut budget = declarations::Budget {
                    limits: self.elab.limits,
                    statistics: self.elab.statistics,
                };
                let positions = ordinary::source::input_positions(target_facts, &mut budget);
                self.elab.statistics = budget.statistics;
                let positions = positions?;
                self.elab
                    .reserve(positions.len().checked_mul(2).ok_or(())?)?;
                let mut filters = Vec::new();
                let mut outputs = Vec::new();
                let mut ready = true;
                for (i, term) in requirement.arguments.iter().enumerate() {
                    let Some(term) = term else {
                        ready = false;
                        break;
                    };
                    let term = self.substitute(term, bindings, 1)?;
                    let closed = self.closed(&term, binders, &|i| solved.contains(&i), 1)?;
                    if positions[i] {
                        if !closed {
                            ready = false;
                            break;
                        }
                        filters.push(Substitution {
                            binder: &target_facts.header.binders[i].declaration.id,
                            position: i,
                            argument: self.substitute(&term, selected, 1)?,
                        });
                    } else if !closed {
                        outputs.push((i, term));
                    }
                }
                if !ready || outputs.is_empty() {
                    continue;
                }
                let DeclarationShape::Function { parameters, .. } = &target_facts.header.shape
                else {
                    unreachable!()
                };
                self.elab.reserve(parameters.len())?;
                let inputs = parameters
                    .iter()
                    .map(|p| self.substitute(&p.ty, &filters, 1))
                    .collect::<Work<Vec<_>>>()?;
                let site = self.evidence.goals[goal].site;
                let child = self.issue_determination(
                    GoalJudgment::Determine {
                        family: target,
                        inputs,
                        filters,
                    },
                    site,
                    source,
                    &requirement.routes,
                    None,
                )?;
                self.link_goal(goal, child.clone())?;
                self.elab.reserve(1)?;
                self.evidence
                    .output_equations
                    .insert(goal, OutputEquation { child, outputs });
                return Ok(GoalState::Proved);
            }
        }
        Ok(GoalState::Unresolved(GoalBlocker::SelectedOutput))
    }

    pub(super) fn resume_selected_output(
        &mut self,
        goal: usize,
        equation: OutputEquation<'g>,
    ) -> Work<GoalState> {
        let family = match self.evidence.goals[goal].judgment {
            GoalJudgment::Determine { family, .. } | GoalJudgment::Instantiate { family, .. } => {
                family
            }
            _ => unreachable!(),
        };
        let family = self.evidence_family(family)?.unwrap();
        let selection = self.evidence.goals[goal].selection.as_ref().unwrap();
        let implementation = selection.implementation;
        self.elab.step(family.implementations.len())?;
        let body = family
            .implementations
            .iter()
            .find(|b| b.source.declaration == *implementation)
            .unwrap();
        if self.evidence.goals[equation.child.index].output.is_some() {
            let binders = &body.header.binders;
            self.elab.reserve(binders.len())?;
            let mut slots = vec![None; binders.len()];
            for sub in &selection.substitution {
                slots[sub.position] = Some(sub.argument.clone());
            }
            for (position, term) in equation.outputs {
                let actual = self.evidence.goals[equation.child.index]
                    .output
                    .as_ref()
                    .unwrap()
                    .arguments[position]
                    .clone();
                if !self.input_equation(&term, &actual, binders, &mut slots, &[], 1)? {
                    return Ok(GoalState::Disproved);
                }
            }
            self.update_selection_slots(goal, binders, &slots)?;
        }
        self.continue_selected_contract(goal, family)
    }
}
