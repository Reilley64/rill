use super::*;
use ordinary::{InputPremise, RequirementMetadata, RequirementTarget};

/// An identity issued by one generic analysis report, not a proof input.
#[derive(Clone, Debug)]
pub struct GoalId {
    report: Arc<()>,
    index: usize,
}
impl PartialEq for GoalId {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && Arc::ptr_eq(&self.report, &other.report)
    }
}
impl Eq for GoalId {}

#[derive(Clone, Debug)]
pub enum GoalJudgment<'g> {
    Requirement {
        target: RequirementTarget<'g>,
        arguments: Vec<Type<'g>>,
    },
    Determine {
        family: &'g DeclarationId,
        inputs: Vec<Type<'g>>,
        /// Only explicit input-position bindings, in declaration order.
        filters: Vec<Substitution<'g>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalBlocker {
    MissingAssumption,
    OrdinaryAdmission,
    InputSelection,
    ConflictingAssumptions,
    SelectedOutput,
    UnavailableRequirement,
    Representation,
    CallableEnvironment,
    Cycle,
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalState {
    Assumption,
    Proved,
    Disproved,
    Unresolved(GoalBlocker),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalVerdict {
    Proved,
    Disproved,
}

#[derive(Debug)]
pub struct GoalSelection<'g> {
    pub implementation: &'g DeclarationId,
    pub alternative: usize,
    pub substitution: Vec<Substitution<'g>>,
}

#[derive(Debug)]
pub struct GoalOutput<'g> {
    pub arguments: Vec<Type<'g>>,
    pub result: Type<'g>,
    pub site: Site<'g>,
}

#[derive(Debug)]
pub struct GoalEquality<'g> {
    /// A family argument position, or None for the complete result type.
    pub position: Option<usize>,
    pub requested: Type<'g>,
    pub actual: Type<'g>,
    pub requested_site: Site<'g>,
    pub actual_site: Site<'g>,
    pub agrees: bool,
}

#[derive(Debug)]
pub struct GoalFacts<'g> {
    pub id: GoalId,
    pub judgment: GoalJudgment<'g>,
    /// The expression or body that needs the goal.
    pub site: Site<'g>,
    /// The declaration of the requirement, or the member that needs it.
    pub source: Site<'g>,
    pub routes: Vec<&'g Binding>,
    pub state: GoalState,
    pub premise: Option<InputPremise>,
    /// A verdict only under the retained role and protection premise.
    pub hypothetical: Option<GoalVerdict>,
    pub selection: Option<GoalSelection<'g>>,
    pub output: Option<GoalOutput<'g>>,
    pub equalities: Vec<GoalEquality<'g>>,
    pub assumptions: Vec<GoalId>,
    pub dependencies: Vec<GoalId>,
    pub cycle: Vec<GoalId>,
}

#[derive(Debug, Default)]
pub(in crate::analysis::scalars) struct Evidence<'g> {
    report: Arc<()>,
    goals: Vec<GoalFacts<'g>>,
    // Check one written body at a time. Nested expressions inherit this set.
    assumptions: Vec<GoalId>,
}

impl<'g> Evidence<'g> {
    pub(super) fn goals(&self) -> &[GoalFacts<'g>] {
        &self.goals
    }

    pub(super) fn get(&self, id: &GoalId) -> Result<&GoalFacts<'g>, QueryError> {
        if !Arc::ptr_eq(&self.report, &id.report) {
            return Err(QueryError::InvalidRequest);
        }
        self.goals.get(id.index).ok_or(QueryError::InvalidRequest)
    }
}

impl<'g> GoalJudgment<'g> {
    fn target(&self) -> RequirementTarget<'g> {
        match self {
            Self::Requirement { target, .. } => *target,
            Self::Determine { family, .. } => RequirementTarget::Family(family),
        }
    }

    fn key_term(&self, index: usize) -> Option<(Option<usize>, &Type<'g>)> {
        match self {
            Self::Requirement { arguments, .. } => arguments.get(index).map(|ty| (None, ty)),
            Self::Determine {
                inputs, filters, ..
            } => inputs.get(index).map(|ty| (None, ty)).or_else(|| {
                filters
                    .get(index - inputs.len())
                    .map(|s| (Some(s.position), &s.argument))
            }),
        }
    }

    fn key_len(&self) -> usize {
        match self {
            Self::Requirement { arguments, .. } => arguments.len(),
            Self::Determine {
                inputs, filters, ..
            } => inputs.len() + filters.len(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Predicate {
    NoBorrow,
    Sendable,
    Shareable,
    Callable(InvocationMode),
}

impl Predicate {
    // These identities come exclusively from the captured prelude's lookup in
    // RequirementMetadata, never by looking at a user declaration's name.
    fn of(intrinsic: &Intrinsic) -> Self {
        match intrinsic.name() {
            "noBorrow" => Self::NoBorrow,
            "sendable" => Self::Sendable,
            "shareable" => Self::Shareable,
            "callable" => Self::Callable(InvocationMode::ReadOnly),
            "callableMut" => Self::Callable(InvocationMode::Mutable),
            "callableOnce" => Self::Callable(InvocationMode::Once),
            _ => unreachable!("captured M3 predicate"),
        }
    }
}

fn satisfies_mode(actual: InvocationMode, required: InvocationMode) -> bool {
    actual == required || actual == InvocationMode::ReadOnly || required == InvocationMode::Once
}

enum GoalChoice<'g> {
    Blocked,
    NoWinner,
    Selected(GoalSelection<'g>),
}

struct Frame {
    goal: usize,
    next: usize,
    state: GoalState,
}

impl<'r, 'g> Checker<'r, 'g> {
    fn issue_goal(
        &mut self,
        judgment: GoalJudgment<'g>,
        site: Site<'g>,
        source: Site<'g>,
        routes: &[&'g Binding],
        assumption: bool,
    ) -> Work<GoalId> {
        self.elab.reserve(1)?;
        self.elab.reserve(routes.len())?;
        let evidence = &mut self.evidence;
        let id = GoalId {
            report: evidence.report.clone(),
            index: evidence.goals.len(),
        };
        let (state, assumptions) = if assumption {
            (GoalState::Assumption, Vec::new())
        } else {
            self.elab.reserve(evidence.assumptions.len())?;
            (
                GoalState::Unresolved(GoalBlocker::ResourceLimit),
                evidence.assumptions.clone(),
            )
        };
        let target = judgment.target();
        evidence.goals.push(GoalFacts {
            id: id.clone(),
            judgment,
            site,
            source,
            routes: routes.to_vec(),
            state,
            premise: matches!(target, RequirementTarget::Family(_))
                .then_some(InputPremise::HypotheticalOrdinaryRole),
            hypothetical: None,
            selection: None,
            output: None,
            equalities: Vec::new(),
            assumptions,
            dependencies: Vec::new(),
            cycle: Vec::new(),
        });
        Ok(id)
    }

    fn requirement_term(
        &mut self,
        requirement: &RequirementMetadata<'g>,
        substitutions: &[Substitution<'g>],
    ) -> Work<Option<GoalJudgment<'g>>> {
        let Some(target) = requirement.target else {
            return Ok(None);
        };
        let arity = match target {
            RequirementTarget::Predicate(id) => match Predicate::of(id) {
                Predicate::Callable(_) => 3,
                _ => 1,
            },
            RequirementTarget::Family(id) => {
                let Some(family) = self.evidence_family(id)? else {
                    return Ok(None);
                };
                family.header.binders.len()
            }
        };
        if requirement.arguments.len() != arity {
            return Ok(None);
        }
        self.elab.reserve(arity)?;
        let mut arguments = Vec::with_capacity(arity);
        for argument in &requirement.arguments {
            let Some(argument) = argument else {
                return Ok(None);
            };
            arguments.push(self.substitute(argument, substitutions, 1)?);
        }
        if matches!(target, RequirementTarget::Predicate(id) if matches!(Predicate::of(id), Predicate::Callable(_)))
            && !matches!(arguments[1].as_ref(), TypeTerm::Tuple(_))
        {
            return Ok(None);
        }
        Ok(Some(GoalJudgment::Requirement { target, arguments }))
    }

    fn evidence_family(
        &mut self,
        id: &DeclarationId,
    ) -> Work<Option<&'r ordinary::FamilyFacts<'g>>> {
        // All IDs at this seam already came from the captured inventory.
        let input = self.input.ordinary.unwrap();
        // Account for the public lookup's capture validation scan.
        for module in self.input.graph.modules() {
            self.elab.step(module.declarations.len() + 1)?;
        }
        Ok(input.family(id).expect("captured requirement target"))
    }

    fn evidence_module(&mut self, key: &ModuleKey) -> Work<&'g Module> {
        for module in self.input.graph.modules() {
            self.elab.step(1)?;
            if module.key == *key {
                return Ok(module);
            }
        }
        unreachable!("captured declaration module")
    }

    pub(in crate::analysis::scalars) fn implementation_contract(
        &mut self,
        module: &'g Module,
        declaration: &'g Declaration,
    ) -> Work<Option<(&'r DeclarationFacts<'g>, NodeId)>> {
        if self.input.ordinary.is_none() {
            return Ok(None);
        }
        self.elab.step(self.input.graph.implementations().len())?;
        let Some(source) = self
            .input
            .graph
            .implementations()
            .iter()
            .find(|source| source.declaration == declaration.id)
        else {
            return Ok(None);
        };
        let Some(family) = self.evidence_family(&source.family)? else {
            return Ok(None);
        };
        self.elab.step(family.implementations.len())?;
        let Some(body) = family
            .implementations
            .iter()
            .find(|body| body.source.declaration == declaration.id)
        else {
            return Ok(None);
        };
        let Some(substitutions) = &body.substitution else {
            return Ok(None);
        };
        if body.placement == ordinary::Placement::OutsideOwners {
            return Ok(None);
        }
        self.add_body_assumptions(module, source.body_node, &source.family, substitutions)?;
        self.add_body_assumptions(module, source.body_node, &source.declaration, &[])?;
        Ok(Some((&body.header, source.body_node)))
    }

    pub(in crate::analysis::scalars) fn clear_body_assumptions(&mut self) {
        self.evidence.assumptions.clear();
    }

    /// Install only written, canonical assumptions before checking a body.
    pub(in crate::analysis::scalars) fn add_body_assumptions(
        &mut self,
        module: &'g Module,
        node: NodeId,
        owner: &DeclarationId,
        substitutions: &[Substitution<'g>],
    ) -> Work<()> {
        let Some(header) = self
            .input
            .ordinary
            .and_then(|input| input.headers().get(owner))
        else {
            return Ok(());
        };
        for requirement in &header.requirements {
            self.elab.step(1)?;
            let Some(judgment) = self.requirement_term(requirement, substitutions)? else {
                self.defer(module, node, ScalarUnsupported::Requirements)?;
                continue;
            };
            let source = Self::site(self.evidence_module(&owner.module)?, requirement.node);
            self.elab.reserve(1)?;
            let id = self.issue_goal(
                judgment,
                Self::site(module, node),
                source,
                &requirement.routes,
                true,
            )?;
            self.evidence.assumptions.push(id);
        }
        Ok(())
    }

    pub(super) fn check_call_requirements(
        &mut self,
        module: &'g Module,
        node: NodeId,
        owner: &DeclarationId,
        substitutions: &[Substitution<'g>],
    ) -> Work<bool> {
        let Some(header) = self
            .input
            .ordinary
            .and_then(|input| input.headers().get(owner))
        else {
            return Ok(true);
        };
        let mut unresolved = false;
        let mut rejected = false;
        for requirement in &header.requirements {
            self.elab.step(1)?;
            let Some(judgment) = self.requirement_term(requirement, substitutions)? else {
                unresolved = true;
                continue;
            };
            let source = Self::site(self.evidence_module(&owner.module)?, requirement.node);
            let goal = self.issue_goal(
                judgment,
                Self::site(module, node),
                source,
                &requirement.routes,
                false,
            )?;
            match self.solve_goal(&goal)? {
                GoalState::Proved => (),
                GoalState::Disproved | GoalState::Unresolved(GoalBlocker::MissingAssumption)
                    if self.evidence.goals[goal.index].premise.is_none() =>
                {
                    rejected = true
                }
                _ => unresolved = true,
            }
        }
        if unresolved {
            self.defer(module, node, ScalarUnsupported::Requirements)?;
        } else if rejected {
            self.finding(module, node, ScalarFindingKind::MissingRequirement)?;
        }
        Ok(!unresolved && !rejected)
    }

    fn same_goal(&mut self, left: usize, right: usize, weakening: bool) -> Work<bool> {
        let l = &self.evidence.goals[left].judgment;
        let r = &self.evidence.goals[right].judgment;
        if std::mem::discriminant(l) != std::mem::discriminant(r) {
            return Ok(false);
        }
        let (lt, rt) = (l.target(), r.target());
        let matches = lt == rt
            || (weakening
                && matches!((lt, rt), (RequirementTarget::Predicate(l), RequirementTarget::Predicate(r))
                    if matches!((Predicate::of(l), Predicate::of(r)), (Predicate::Callable(a), Predicate::Callable(b)) if satisfies_mode(a, b))));
        if !matches || l.key_len() != r.key_len() {
            return Ok(false);
        }
        for index in 0..l.key_len() {
            let (lp, l) = self.evidence.goals[left].judgment.key_term(index).unwrap();
            let (rp, r) = self.evidence.goals[right].judgment.key_term(index).unwrap();
            if lp != rp || !self.same(&l.clone(), &r.clone())? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn link_goal(&mut self, parent: usize, child: GoalId) -> Work<()> {
        self.elab.reserve(1)?;
        self.evidence.goals[parent].dependencies.push(child);
        Ok(())
    }

    pub(super) fn solve_goal(&mut self, goal: &GoalId) -> Work<GoalState> {
        debug_assert!(self.evidence.get(goal).is_ok());
        let root = goal.index;
        let state = self.evidence.goals[root].state;
        if state != GoalState::Unresolved(GoalBlocker::ResourceLimit) {
            return Ok(state);
        }
        let mut stack = Vec::<Frame>::new();
        let mut completed = Vec::<usize>::new();
        self.enter_goal(root, &mut stack, &completed)?;
        while let Some(frame) = stack.last_mut() {
            self.elab.step(1)?;
            let goal = frame.goal;
            if let Some(child) = self.evidence.goals[goal].dependencies.get(frame.next) {
                let child = child.index;
                if self.evidence.goals[child].state
                    == GoalState::Unresolved(GoalBlocker::ResourceLimit)
                {
                    self.enter_goal(child, &mut stack, &completed)?;
                    continue;
                }
                let child = &self.evidence.goals[child];
                let state = match child.hypothetical {
                    Some(GoalVerdict::Proved) => GoalState::Proved,
                    Some(GoalVerdict::Disproved) => GoalState::Disproved,
                    None => child.state,
                };
                let premise = child.premise;
                if premise.is_some() {
                    self.evidence.goals[goal].premise = premise;
                }
                frame.state = join(frame.state, state);
                frame.next += 1;
            } else {
                let mut state = stack.pop().unwrap().state;
                if let Some(equal) = self.finish_family_requirement(goal)?
                    && !equal
                {
                    state = join(state, GoalState::Disproved);
                }
                if self.evidence.goals[goal].premise.is_some() {
                    self.evidence.goals[goal].hypothetical = match state {
                        GoalState::Proved => Some(GoalVerdict::Proved),
                        GoalState::Disproved => Some(GoalVerdict::Disproved),
                        _ => None,
                    };
                    if self.evidence.goals[goal].hypothetical.is_some() {
                        state = GoalState::Unresolved(GoalBlocker::OrdinaryAdmission);
                    }
                }
                if matches!(state, GoalState::Proved | GoalState::Disproved) {
                    self.elab.reserve(1)?;
                    completed.push(goal);
                }
                self.evidence.goals[goal].state = state;
            }
        }
        Ok(self.evidence.goals[root].state)
    }

    fn enter_goal(&mut self, goal: usize, stack: &mut Vec<Frame>, completed: &[usize]) -> Work<()> {
        self.elab.step(1)?;
        for index in 0..self.evidence.goals[goal].assumptions.len() {
            self.elab.step(1)?;
            let assumption = self.evidence.goals[goal].assumptions[index].clone();
            if self.same_goal(assumption.index, goal, true)? {
                self.link_goal(goal, assumption)?;
                let fact = &mut self.evidence.goals[goal];
                fact.state = if fact.premise.is_some() {
                    let prelude = self.input.graph.prelude();
                    if !matches!(fact.judgment.target(), RequirementTarget::Family(id) if *id == prelude.copy || *id == prelude.dispose)
                    {
                        fact.hypothetical = Some(GoalVerdict::Proved);
                    }
                    GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
                } else {
                    GoalState::Proved
                };
                return Ok(());
            }
        }
        for &prior in completed {
            self.elab.step(1)?;
            if self.same_goal(prior, goal, false)? {
                self.link_goal(goal, self.evidence.goals[prior].id.clone())?;
                self.evidence.goals[goal].state = self.evidence.goals[prior].state;
                return Ok(());
            }
        }
        for (offset, frame) in stack.iter().enumerate() {
            self.elab.step(1)?;
            if self.same_goal(frame.goal, goal, false)? {
                self.elab.reserve(stack.len() - offset + 1)?;
                self.evidence.goals[goal].cycle = stack[offset..]
                    .iter()
                    .map(|frame| self.evidence.goals[frame.goal].id.clone())
                    .chain(std::iter::once(self.evidence.goals[goal].id.clone()))
                    .collect();
                self.evidence.goals[goal].state = GoalState::Unresolved(GoalBlocker::Cycle);
                return Ok(());
            }
        }
        let depth = stack.len() + 1;
        if depth > self.elab.limits.max_depth {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        self.elab.reserve(1)?;
        let state = self.expand_goal(goal)?;
        stack.push(Frame {
            goal,
            next: 0,
            state,
        });
        Ok(())
    }

    fn expand_goal(&mut self, goal: usize) -> Work<GoalState> {
        let judgment = &self.evidence.goals[goal].judgment;
        match judgment {
            GoalJudgment::Determine { .. } => self.expand_determination(goal),
            GoalJudgment::Requirement {
                target: RequirementTarget::Family(family),
                arguments,
            } => {
                let family = *family;
                self.elab.reserve(arguments.len())?;
                self.expand_family_requirement(goal, family, arguments.clone())
            }
            GoalJudgment::Requirement {
                target: RequirementTarget::Predicate(intrinsic),
                arguments,
            } => self.expand_predicate(goal, intrinsic, arguments[0].clone()),
        }
    }

    fn family_substitutions(
        &mut self,
        header: &DeclarationFacts<'g>,
        arguments: &[Type<'g>],
    ) -> Work<Vec<Substitution<'g>>> {
        self.elab.reserve(arguments.len())?;
        Ok(header
            .binders
            .iter()
            .zip(arguments)
            .map(|(binder, argument)| Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument: argument.clone(),
            })
            .collect())
    }

    fn family_inputs(
        &mut self,
        header: &DeclarationFacts<'g>,
        substitutions: &[Substitution<'g>],
    ) -> Work<(Vec<Type<'g>>, Vec<Substitution<'g>>)> {
        let DeclarationShape::Function { parameters, .. } = &header.shape else {
            unreachable!("family signature")
        };
        self.elab.reserve(parameters.len())?;
        let mut inputs = Vec::with_capacity(parameters.len());
        self.elab.reserve(substitutions.len())?;
        let mut filters = Vec::new();
        for binding in substitutions {
            for parameter in parameters {
                if !self.closed(
                    &parameter.ty,
                    &header.binders,
                    &|i| i != binding.position,
                    1,
                )? {
                    filters.push(binding.clone());
                    break;
                }
            }
        }
        for parameter in parameters {
            inputs.push(self.substitute(&parameter.ty, substitutions, 1)?);
        }
        Ok((inputs, filters))
    }

    fn expand_family_requirement(
        &mut self,
        goal: usize,
        family: &'g DeclarationId,
        arguments: Vec<Type<'g>>,
    ) -> Work<GoalState> {
        let header = &self.evidence_family(family)?.unwrap().header;
        let substitutions = self.family_substitutions(header, &arguments)?;
        let (inputs, filters) = self.family_inputs(header, &substitutions)?;
        let parent = &self.evidence.goals[goal];
        let child = self.issue_determination(
            GoalJudgment::Determine {
                family,
                inputs,
                filters,
            },
            parent.site,
            parent.source,
            None,
        )?;
        self.link_goal(goal, child)?;
        Ok(GoalState::Proved)
    }

    /// Source calls can attach their already selected header/substitution instead
    /// of running selection again. Only `solve_goal` can complete the judgment.
    pub(super) fn issue_determination(
        &mut self,
        judgment: GoalJudgment<'g>,
        site: Site<'g>,
        source: Site<'g>,
        selection: Option<GoalSelection<'g>>,
    ) -> Work<GoalId> {
        debug_assert!(matches!(judgment, GoalJudgment::Determine { .. }));
        let id = self.issue_goal(judgment, site, source, &[], false)?;
        self.evidence.goals[id.index].selection = selection;
        Ok(id)
    }

    fn expand_determination(&mut self, goal: usize) -> Work<GoalState> {
        self.elab
            .reserve(self.evidence.goals[goal].judgment.key_len())?;
        let GoalJudgment::Determine {
            family: id,
            inputs,
            filters,
        } = self.evidence.goals[goal].judgment.clone()
        else {
            unreachable!("determination judgment")
        };
        let selection = self.evidence.goals[goal].selection.take();
        let prelude = self.input.graph.prelude();
        if *id == prelude.copy || *id == prelude.dispose {
            return Ok(GoalState::Unresolved(GoalBlocker::OrdinaryAdmission));
        }
        let family = self.evidence_family(id)?.unwrap();
        if let Some(state) = self.assumption_output(goal, &family.header, &inputs, &filters)? {
            return Ok(state);
        }
        let selection = if let Some(selection) = selection {
            selection
        } else {
            match self.select_goal_contract(id, &inputs, &filters)? {
                GoalChoice::Blocked => {
                    return Ok(GoalState::Unresolved(GoalBlocker::InputSelection));
                }
                GoalChoice::NoWinner => return Ok(GoalState::Disproved),
                GoalChoice::Selected(selection) => selection,
            }
        };
        self.expand_selected_contract(goal, family, selection)
    }

    fn assumption_output(
        &mut self,
        goal: usize,
        header: &DeclarationFacts<'g>,
        inputs: &[Type<'g>],
        filters: &[Substitution<'g>],
    ) -> Work<Option<GoalState>> {
        let DeclarationShape::Function { result, .. } = &header.shape else {
            unreachable!("family signature")
        };
        let mut output: Option<GoalOutput<'g>> = None;
        let mut conflict = false;
        for i in 0..self.evidence.goals[goal].assumptions.len() {
            self.elab.step(1)?;
            let id = self.evidence.goals[goal].assumptions[i].clone();
            let assume = &self.evidence.goals[id.index];
            let GoalJudgment::Requirement {
                target: RequirementTarget::Family(family),
                arguments,
            } = &assume.judgment
            else {
                continue;
            };
            if *family != &header.declaration.id {
                continue;
            }
            self.elab.reserve(arguments.len())?;
            let arguments = arguments.clone();
            let site = assume.source;
            let substitutions = self.family_substitutions(header, &arguments)?;
            let (actual_inputs, actual_filters) = self.family_inputs(header, &substitutions)?;
            if !self.same_terms(inputs, &actual_inputs)? || filters.len() != actual_filters.len() {
                continue;
            }
            let mut agrees = true;
            for (requested, actual) in filters.iter().zip(&actual_filters) {
                if requested.position != actual.position
                    || !self.same(&requested.argument, &actual.argument)?
                {
                    agrees = false;
                    break;
                }
            }
            if !agrees {
                continue;
            }
            self.link_goal(goal, id)?;
            let result = self.substitute(result, &substitutions, 1)?;
            if let Some(first) = &output {
                conflict |= !self.same_terms(&first.arguments, &arguments)?
                    || !self.same(&first.result, &result)?;
            } else {
                output = Some(GoalOutput {
                    arguments,
                    result,
                    site,
                });
            }
        }
        if conflict {
            return Ok(Some(GoalState::Unresolved(
                GoalBlocker::ConflictingAssumptions,
            )));
        }
        let established = output.is_some();
        self.evidence.goals[goal].output = output;
        Ok(established.then_some(GoalState::Proved))
    }

    fn same_terms(&mut self, left: &[Type<'g>], right: &[Type<'g>]) -> Work<bool> {
        self.elab.step(1)?;
        if left.len() != right.len() {
            return Ok(false);
        }
        for (l, r) in left.iter().zip(right) {
            if !self.same(l, r)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // Reuse M4.5's established-input selector for ground goal dependencies.
    // The M4.6 source selector can replace this adapter with uniform selection
    // and pass the same implementation witness to expand_selected_contract.
    fn select_goal_contract(
        &mut self,
        family: &'g DeclarationId,
        inputs: &[Type<'g>],
        filters: &[Substitution<'g>],
    ) -> Work<GoalChoice<'g>> {
        for (i, binding) in filters.iter().enumerate() {
            self.elab.step(1)?;
            if binding.position != i {
                return Ok(GoalChoice::Blocked);
            }
        }
        self.elab.reserve(inputs.len())?;
        self.elab.reserve(filters.len())?;
        let contracts: Vec<_> = inputs
            .iter()
            .cloned()
            .map(ordinary::InputContract::Established)
            .collect();
        let explicit: Vec<_> = filters.iter().map(|s| s.argument.clone()).collect();
        let remaining = AnalysisLimits {
            max_work: self.elab.limits.max_work - self.elab.statistics.work,
            max_entries: self.elab.limits.max_entries - self.elab.statistics.entries,
            max_depth: self.elab.limits.max_depth,
        };
        let analysis = self.input.ordinary.unwrap();
        let Ok(report) =
            ordinary::select_inputs(analysis, family, &contracts, &explicit, remaining)
        else {
            return Ok(GoalChoice::Blocked);
        };
        self.elab.step(report.statistics.work)?;
        self.elab.reserve(report.statistics.entries)?;
        self.elab.statistics.max_depth = self
            .elab
            .statistics
            .max_depth
            .max(report.statistics.max_depth);
        if report.completion == ordinary::Completion::ResourceLimit {
            return Err(());
        }
        let (Some(outcome), None) = (report.outcome, report.blocker) else {
            return Ok(GoalChoice::Blocked);
        };
        let ordinary::HypotheticalOutcome::Winner(implementation) = outcome else {
            return Ok(GoalChoice::NoWinner);
        };
        self.elab.step(report.matches.len())?;
        let alternative = report
            .matches
            .iter()
            .find(|m| m.implementation == implementation)
            .unwrap()
            .alternative;
        let family = self.evidence_family(family)?.unwrap();
        self.elab.step(family.implementations.len())?;
        let body = family
            .implementations
            .iter()
            .find(|b| b.source.declaration == *implementation)
            .unwrap();
        let DeclarationShape::Function { parameters, .. } = &body.header.shape else {
            unreachable!("implementation signature")
        };
        let binders = &body.header.binders;
        self.elab.reserve(binders.len())?;
        let mut slots = vec![None; binders.len()];
        for (parameter, actual) in parameters.iter().zip(inputs) {
            if !self.accept_input(&parameter.ty, actual, binders, &mut slots, &[])? {
                return Ok(GoalChoice::Blocked);
            }
        }
        Ok(GoalChoice::Selected(GoalSelection {
            implementation,
            alternative,
            substitution: self.slot_substitutions(binders, &slots)?,
        }))
    }

    fn expand_selected_contract(
        &mut self,
        goal: usize,
        family: &ordinary::FamilyFacts<'g>,
        selection: GoalSelection<'g>,
    ) -> Work<GoalState> {
        self.elab.step(family.implementations.len())?;
        let body = family
            .implementations
            .iter()
            .find(|b| b.source.declaration == *selection.implementation)
            .unwrap();
        let DeclarationShape::Function { result, .. } = &body.header.shape else {
            unreachable!("implementation signature")
        };
        let source = Self::site(
            self.evidence_module(&body.source.declaration.module)?,
            body.source.signature_node,
        );
        let site = self.evidence.goals[goal].site;
        let implementation = selection.implementation;
        self.evidence.goals[goal].selection = Some(selection);
        let selection = self.evidence.goals[goal].selection.as_ref().unwrap();
        for (index, binder) in body.header.binders.iter().enumerate() {
            self.elab.step(1)?;
            if selection
                .substitution
                .get(index)
                .is_none_or(|sub| sub.binder != &binder.declaration.id)
            {
                return Ok(GoalState::Unresolved(GoalBlocker::SelectedOutput));
            }
        }
        self.elab.reserve(selection.substitution.len())?;
        let selected_substitutions = selection.substitution.clone();
        let result = self.substitute(result, &selected_substitutions, 1)?;
        let header_substitution = body.substitution.as_ref().unwrap();
        self.elab.reserve(header_substitution.len())?;
        let mut substitutions = Vec::with_capacity(header_substitution.len());
        for family_slot in header_substitution {
            substitutions.push(Substitution {
                binder: family_slot.binder,
                position: family_slot.position,
                argument: self.substitute(&family_slot.argument, &selected_substitutions, 1)?,
            });
        }
        self.elab.reserve(substitutions.len())?;
        self.evidence.goals[goal].output = Some(GoalOutput {
            arguments: substitutions
                .iter()
                .map(|sub| sub.argument.clone())
                .collect(),
            result,
            site: source,
        });
        let declared =
            self.append_header_goals(goal, site, &family.header.declaration.id, &substitutions)?;
        let selected =
            self.append_header_goals(goal, site, implementation, &selected_substitutions)?;
        Ok(join(declared, selected))
    }

    fn append_header_goals(
        &mut self,
        goal: usize,
        site: Site<'g>,
        owner: &DeclarationId,
        substitutions: &[Substitution<'g>],
    ) -> Work<GoalState> {
        let Some(header) = self.input.ordinary.unwrap().headers().get(owner) else {
            return Ok(GoalState::Proved);
        };
        let module = self.evidence_module(&owner.module)?;
        let mut state = GoalState::Proved;
        for requirement in &header.requirements {
            self.elab.step(1)?;
            let Some(judgment) = self.requirement_term(requirement, substitutions)? else {
                state = GoalState::Unresolved(GoalBlocker::UnavailableRequirement);
                continue;
            };
            let child = self.issue_goal(
                judgment,
                site,
                Self::site(module, requirement.node),
                &requirement.routes,
                false,
            )?;
            self.link_goal(goal, child)?;
        }
        Ok(state)
    }

    fn finish_family_requirement(&mut self, goal: usize) -> Work<Option<bool>> {
        let fact = &self.evidence.goals[goal];
        let GoalJudgment::Requirement {
            target: RequirementTarget::Family(id),
            arguments,
        } = &fact.judgment
        else {
            return Ok(None);
        };
        let Some(child) = fact.dependencies.first() else {
            return Ok(None);
        };
        let child = child.index;
        let Some(output) = &self.evidence.goals[child].output else {
            return Ok(None);
        };
        let id = *id;
        let actual_site = output.site;
        let actual_result = output.result.clone();
        let requested_site = fact.source;
        self.elab.reserve(arguments.len())?;
        let arguments = arguments.clone();
        let module = self.evidence_module(requested_site.module)?;
        let header = &self.evidence_family(id)?.unwrap().header;
        let DeclarationShape::Function { result, .. } = &header.shape else {
            unreachable!("family signature")
        };
        let substitutions = self.family_substitutions(header, &arguments)?;
        let requested_result = self.substitute(result, &substitutions, 1)?;
        let mut agrees = true;
        for (position, requested) in arguments.into_iter().enumerate() {
            let actual = self.evidence.goals[child]
                .output
                .as_ref()
                .unwrap()
                .arguments[position]
                .clone();
            let requested_site = Self::site(
                module,
                module.syntax.nodes[requested_site.node].children[position + 1],
            );
            agrees &= self.goal_equality(
                goal,
                Some(position),
                requested,
                actual,
                requested_site,
                actual_site,
            )?;
        }
        agrees &= self.goal_equality(
            goal,
            None,
            requested_result,
            actual_result,
            requested_site,
            actual_site,
        )?;
        Ok(Some(agrees))
    }

    fn goal_equality(
        &mut self,
        goal: usize,
        position: Option<usize>,
        requested: Type<'g>,
        actual: Type<'g>,
        requested_site: Site<'g>,
        actual_site: Site<'g>,
    ) -> Work<bool> {
        let agrees = self.same(&requested, &actual)?;
        self.elab.reserve(1)?;
        self.evidence.goals[goal].equalities.push(GoalEquality {
            position,
            requested,
            actual,
            requested_site,
            actual_site,
            agrees,
        });
        Ok(agrees)
    }

    fn expand_predicate(
        &mut self,
        goal: usize,
        intrinsic: &'g Intrinsic,
        receiver: Type<'g>,
    ) -> Work<GoalState> {
        let predicate = Predicate::of(intrinsic);
        if let Predicate::Callable(required) = predicate {
            let GoalJudgment::Requirement { arguments, .. } = &self.evidence.goals[goal].judgment
            else {
                unreachable!("predicate judgment")
            };
            let parameters = arguments[1].clone();
            let result = arguments[2].clone();
            return self.callable_evidence(&receiver, &parameters, &result, required);
        }
        match receiver.as_ref() {
            TypeTerm::Primitive(id) if !matches!(id.name(), "void" | "never") => {
                Ok(GoalState::Proved)
            }
            TypeTerm::Parameter { .. } => Ok(GoalState::Unresolved(GoalBlocker::MissingAssumption)),
            TypeTerm::Reference { .. } if predicate == Predicate::NoBorrow => {
                Ok(GoalState::Disproved)
            }
            TypeTerm::Reference { mutable, referent } => {
                let property = if predicate == Predicate::Sendable && !mutable {
                    let intrinsics = &self.input.graph.prelude().intrinsics;
                    self.elab.step(intrinsics.len())?;
                    intrinsics
                        .iter()
                        .find(|i| Predicate::of_value(i) == Some(Predicate::Shareable))
                        .unwrap()
                } else {
                    intrinsic
                };
                self.property_dependency(
                    goal,
                    property,
                    referent.clone(),
                    self.evidence.goals[goal].source,
                )?;
                Ok(GoalState::Proved)
            }
            TypeTerm::Tuple(fields) => {
                for field in fields {
                    self.property_dependency(
                        goal,
                        intrinsic,
                        field.clone(),
                        self.evidence.goals[goal].source,
                    )?;
                }
                Ok(GoalState::Proved)
            }
            TypeTerm::Callable { .. } => {
                Ok(GoalState::Unresolved(GoalBlocker::CallableEnvironment))
            }
            TypeTerm::Nominal { .. } => self.nominal_property(goal, intrinsic, &receiver, None),
            TypeTerm::Variant {
                parent,
                declaration,
            } => self.nominal_property(goal, intrinsic, parent, Some(declaration)),
            _ => Ok(GoalState::Unresolved(GoalBlocker::Representation)),
        }
    }

    fn nominal_property(
        &mut self,
        goal: usize,
        intrinsic: &'g Intrinsic,
        ty: &Type<'g>,
        variant: Option<&DeclarationId>,
    ) -> Work<GoalState> {
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = ty.as_ref()
        else {
            unreachable!("canonical variant parent")
        };
        if declaration.module.origin != crate::loader::Origin::Application {
            return Ok(GoalState::Unresolved(GoalBlocker::Representation));
        }
        let Some(facts) = self.input.facts.declarations.get(declaration) else {
            return Ok(GoalState::Unresolved(GoalBlocker::Representation));
        };
        if facts.binders.len() != arguments.len() || facts.declaration.scope.is_some() {
            return Ok(GoalState::Unresolved(GoalBlocker::Representation));
        }
        let module = self.evidence_module(&declaration.module)?;
        self.elab.reserve(arguments.len())?;
        let substitutions: Vec<_> = facts
            .binders
            .iter()
            .zip(arguments)
            .map(|(binder, argument)| Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument: argument.clone(),
            })
            .collect();
        match &facts.shape {
            DeclarationShape::Record { fields } => {
                for field in fields {
                    let ty = self.substitute(&field.ty, &substitutions, 1)?;
                    self.property_dependency(
                        goal,
                        intrinsic,
                        ty,
                        Self::site(module, field.declaration.node),
                    )?;
                }
            }
            DeclarationShape::Enum { variants } => {
                for item in variants {
                    self.elab.step(1)?;
                    if variant.is_some_and(|id| *id != item.declaration.id) {
                        continue;
                    }
                    match &item.payload {
                        Payload::Unit => (),
                        Payload::Tuple(fields) => {
                            for field in fields {
                                let ty = self.substitute(field, &substitutions, 1)?;
                                self.property_dependency(
                                    goal,
                                    intrinsic,
                                    ty,
                                    Self::site(module, item.declaration.node),
                                )?;
                            }
                        }
                        Payload::Record(fields) => {
                            for field in fields {
                                let ty = self.substitute(&field.ty, &substitutions, 1)?;
                                self.property_dependency(
                                    goal,
                                    intrinsic,
                                    ty,
                                    Self::site(module, field.declaration.node),
                                )?;
                            }
                        }
                    }
                }
            }
            DeclarationShape::Function { .. } => unreachable!("nominal representation"),
        }
        Ok(GoalState::Proved)
    }

    fn property_dependency(
        &mut self,
        goal: usize,
        property: &'g Intrinsic,
        subject: Type<'g>,
        source: Site<'g>,
    ) -> Work<()> {
        self.elab.step(1)?;
        self.elab.reserve(1)?;
        let child = self.issue_goal(
            GoalJudgment::Requirement {
                target: RequirementTarget::Predicate(property),
                arguments: vec![subject],
            },
            self.evidence.goals[goal].site,
            source,
            &[],
            false,
        )?;
        self.link_goal(goal, child)
    }

    fn callable_evidence(
        &mut self,
        receiver: &Type<'g>,
        parameters: &Type<'g>,
        result: &Type<'g>,
        required: InvocationMode,
    ) -> Work<GoalState> {
        let (callable, reference) = match receiver.as_ref() {
            TypeTerm::Reference { referent, mutable } => (referent, Some(*mutable)),
            _ => (receiver, None),
        };
        let TypeTerm::Callable {
            mode,
            parameters: actual,
            result: actual_result,
        } = callable.as_ref()
        else {
            return Ok(if matches!(callable.as_ref(), TypeTerm::Parameter { .. }) {
                GoalState::Unresolved(GoalBlocker::MissingAssumption)
            } else {
                GoalState::Disproved
            });
        };
        let mode = match (reference, mode) {
            (Some(_), InvocationMode::ReadOnly) => InvocationMode::ReadOnly,
            (Some(true), InvocationMode::Mutable) => InvocationMode::Mutable,
            (Some(_), _) => return Ok(GoalState::Disproved),
            (None, mode) => *mode,
        };
        let TypeTerm::Tuple(parameters) = parameters.as_ref() else {
            unreachable!("validated requirement term")
        };
        if !satisfies_mode(mode, required)
            || actual.len() != parameters.len()
            || !self.same(actual_result, result)?
        {
            return Ok(GoalState::Disproved);
        }
        for (actual, required) in actual.iter().zip(parameters) {
            if !self.same(actual, required)? {
                return Ok(GoalState::Disproved);
            }
        }
        Ok(GoalState::Proved)
    }
}

impl Predicate {
    fn of_value(id: &Intrinsic) -> Option<Self> {
        (id.namespace() == Namespace::ValueFunction).then(|| Self::of(id))
    }
}

fn join(left: GoalState, right: GoalState) -> GoalState {
    match (left, right) {
        (GoalState::Unresolved(GoalBlocker::MissingAssumption), other)
        | (other, GoalState::Unresolved(GoalBlocker::MissingAssumption)) => match other {
            GoalState::Proved | GoalState::Assumption => {
                GoalState::Unresolved(GoalBlocker::MissingAssumption)
            }
            _ => other,
        },
        (GoalState::Unresolved(_), _) => left,
        (_, GoalState::Unresolved(_)) => right,
        (GoalState::Disproved, _) | (_, GoalState::Disproved) => GoalState::Disproved,
        _ => GoalState::Proved,
    }
}
