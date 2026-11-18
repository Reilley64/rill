use super::*;

mod conversions;
mod output;
pub(in crate::analysis::scalars) use conversions::ConversionWork;
pub(in crate::analysis) use conversions::check_pair_candidates;
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
    /// Discharge the selected contract at this use after source context or a
    /// ready default binds free positions. This is not a determination key.
    Instantiate {
        family: &'g DeclarationId,
        determination: GoalId,
        free: Vec<Substitution<'g>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalBlocker {
    MissingAssumption,
    OrdinaryAdmission,
    ConversionAdmission,
    ConversionPair(crate::analysis::conversions::PairBlocker),
    InputSelection,
    ConflictingAssumptions,
    SelectedOutput,
    UnavailableRequirement,
    UnavailableProvider,
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
    /// Exact originating arrow, when the source ties this subject to one value.
    pub arrow_origin: Option<Arc<ArrowFacts<'g>>>,
    /// A verdict only under the retained role and protection premise.
    pub hypothetical: Option<GoalVerdict>,
    pub selection: Option<GoalSelection<'g>>,
    pub conversion_outcome: Option<crate::analysis::conversions::PairOutcome<'g>>,
    pub conversion_parameter: Option<Type<'g>>,
    pub conversion_protection: Option<crate::analysis::typed::ConversionProtection>,
    pub conversion_provider: Option<crate::analysis::typed::ConversionProvider>,
    pub output: Option<GoalOutput<'g>>,
    /// Written output defaults actually taken at a source instantiation.
    pub defaults: Vec<DefaultArgumentUse<'g>>,
    pub equalities: Vec<GoalEquality<'g>>,
    pub assumptions: Vec<GoalId>,
    pub dependencies: Vec<GoalId>,
    pub cycle: Vec<GoalId>,
}

#[derive(Debug, Default)]
pub(in crate::analysis) struct Evidence<'g> {
    report: Arc<()>,
    goals: Vec<GoalFacts<'g>>,
    output_equations: BTreeMap<usize, output::OutputEquation<'g>>,
    // Check one written body at a time. Nested expressions inherit this set.
    assumptions: Vec<GoalId>,
}

impl<'g> Evidence<'g> {
    pub(in crate::analysis) fn goals(&self) -> &[GoalFacts<'g>] {
        &self.goals
    }

    pub(in crate::analysis) fn get(&self, id: &GoalId) -> Result<&GoalFacts<'g>, QueryError> {
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
            Self::Determine { family, .. } | Self::Instantiate { family, .. } => {
                RequirementTarget::Family(family)
            }
        }
    }

    fn key_term(&self, index: usize) -> Option<(Option<usize>, &Type<'g>)> {
        match self {
            Self::Instantiate { free, .. } => {
                free.get(index).map(|s| (Some(s.position), &s.argument))
            }
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
            Self::Instantiate { free, .. } => free.len(),
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

pub(super) struct SymbolicCallable<'g> {
    pub receiving: Vec<Option<Type<'g>>>,
    pub selected: Option<(Type<'g>, GoalId)>,
    pub rejection: Option<ScalarFindingKind>,
}

enum GoalChoice<'g> {
    Blocked,
    NoWinner,
    NonUniform,
    Selected(GoalSelection<'g>),
}

struct Frame {
    goal: usize,
    next: usize,
    state: GoalState,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn prove_error_property(
        &mut self,
        site: Site<'g>,
        intrinsic: &'g Intrinsic,
        subject: Type<'g>,
    ) -> Work<GoalId> {
        self.elab.reserve(1)?;
        let goal = self.issue_goal(
            GoalJudgment::Requirement {
                target: RequirementTarget::Predicate(intrinsic),
                arguments: vec![subject],
            },
            site,
            site,
            &[],
            false,
        )?;
        self.solve_goal(&goal)?;
        Ok(goal)
    }

    pub(super) fn issue_goal(
        &mut self,
        judgment: GoalJudgment<'g>,
        site: Site<'g>,
        source: Site<'g>,
        routes: &[&'g Binding],
        assumption: bool,
    ) -> Work<GoalId> {
        self.elab.reserve(1)?;
        self.elab.reserve(routes.len())?;
        let role = match judgment.target() {
            RequirementTarget::Family(id) if self.installed_conversion(id) => None,
            RequirementTarget::Family(id) if self.conversion_family(id).is_some() => {
                Some(InputPremise::HypotheticalConversionRole)
            }
            RequirementTarget::Family(id) if self.ordinary_scalar_role(id).is_some() => None,
            RequirementTarget::Family(id) if self.clear_primitive_goal(id, &judgment) => None,
            RequirementTarget::Family(id)
                if self
                    .result
                    .and_then(|role| role.display_protection)
                    .is_some_and(|protection| &protection.family.id == id) =>
            {
                None
            }
            RequirementTarget::Family(id) => match &judgment {
                GoalJudgment::Determine { inputs, .. }
                    if self.collection_winner(id, inputs).is_some() =>
                {
                    None
                }
                _ => Some(InputPremise::HypotheticalOrdinaryRole),
            },
            _ => None,
        };
        let premise = match (self.arrow_premise, role) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (a, b) => a.or(b),
        };
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
        evidence.goals.push(GoalFacts {
            id: id.clone(),
            judgment,
            site,
            source,
            routes: routes.to_vec(),
            state,
            premise,
            arrow_origin: None,
            hypothetical: None,
            selection: None,
            conversion_outcome: None,
            conversion_parameter: None,
            conversion_protection: None,
            conversion_provider: None,
            output: None,
            defaults: Vec::new(),
            equalities: Vec::new(),
            assumptions,
            dependencies: Vec::new(),
            cycle: Vec::new(),
        });
        Ok(id)
    }

    pub(super) fn requirement_term(
        &mut self,
        requirement: &RequirementMetadata<'g>,
        substitutions: &[Substitution<'g>],
    ) -> Work<Option<GoalJudgment<'g>>> {
        let Some(target) = requirement.target else {
            return Ok(None);
        };
        let Some(arity) = self.requirement_arity(target)? else {
            return Ok(None);
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
        Ok(Self::requirement_shape(target, &arguments)
            .then_some(GoalJudgment::Requirement { target, arguments }))
    }

    fn requirement_arity(&mut self, target: RequirementTarget<'g>) -> Work<Option<usize>> {
        Ok(Some(match target {
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
        }))
    }

    fn requirement_shape(target: RequirementTarget<'g>, arguments: &[Type<'g>]) -> bool {
        !matches!(target, RequirementTarget::Predicate(id) if matches!(Predicate::of(id), Predicate::Callable(_)))
            || matches!(arguments[1].as_ref(), TypeTerm::Tuple(_))
    }

    fn evidence_family(
        &mut self,
        id: &DeclarationId,
    ) -> Work<Option<&'r ordinary::FamilyFacts<'g>>> {
        // All IDs at this seam already came from the captured inventory.
        self.elab.step(1)?;
        Ok(self.input.ordinary.unwrap().captured_family(id))
    }

    pub(in crate::analysis::scalars) fn evidence_module(
        &mut self,
        key: &ModuleKey,
    ) -> Work<&'g Module> {
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
    ) -> Work<Option<(&'r DeclarationFacts<'g>, bool)>> {
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
        if self.closed_err(&source.family) {
            self.finding(
                module,
                source.signature_node,
                ScalarFindingKind::InvalidTarget,
            )?;
        }
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
        if body.placement == ordinary::Placement::OutsideOwners
            && self.conversion_family(&source.family).is_none()
        {
            return Ok(None);
        }
        self.add_body_assumptions(module, source.body_node, &source.family, substitutions)?;
        self.add_body_assumptions(module, source.body_node, &source.declaration, &[])?;
        let primitive = if let Some(core) = self
            .primitives
            .filter(|p| p.prelude(&source.family).is_some())
        {
            self.elab.step(core.providers.len())?;
            core.providers
                .iter()
                .any(|p| p.implementation.id == declaration.id)
        } else {
            false
        };
        let mut conversion = self.installed_conversion(&source.family);
        if conversion && let Some(family) = self.conversion_family(&source.family) {
            self.elab.step(family.issues.len())?;
            conversion = family
                .issues
                .iter()
                .all(|issue| issue.declaration != &declaration.id);
        }
        let admitted = conversion
            || (self.ordinary_scalar_role(&source.family).is_some() || primitive)
                && body.compatibility == ordinary::Compatibility::Compatible;
        Ok(Some((&body.header, admitted)))
    }

    pub(in crate::analysis::scalars) fn nested_body(
        &mut self,
        module: &'g Module,
        typed: &'r DeclarationFacts<'g>,
        context: &Context<'r, 'g>,
        depth: usize,
    ) -> Work<()> {
        let assumptions = std::mem::take(&mut self.evidence.assumptions);
        let result = self.check_named_body(module, typed, false, Some((context, depth)));
        self.evidence.assumptions = assumptions;
        result
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
        inference: &Inference<'g>,
        node: NodeId,
        call: &PreparedCall<'g>,
        substitutions: &[Substitution<'g>],
        checked_conversion_outputs: bool,
    ) -> Work<bool> {
        let module = inference.module;
        let owner = call.fact.callee;
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
            if checked_conversion_outputs && self.fallible_conversion(requirement.target) {
                continue;
            }
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
            let origin = self.arrow_subject(inference, call, requirement)?;
            let fact = &mut self.evidence.goals[goal.index];
            if let Some(premise) = origin.as_ref().and_then(|arrow| arrow.premise) {
                fact.premise = Some(fact.premise.map_or(premise, |p| p.combine(premise)));
            }
            fact.arrow_origin = origin;
            match self.solve_goal(&goal)? {
                GoalState::Proved => (),
                _ if self.arrow_premise.is_some_and(|p| p.has_conversion())
                    && matches!(requirement.target, Some(RequirementTarget::Predicate(_)))
                    && self.evidence.get(&goal).unwrap().hypothetical
                        == Some(GoalVerdict::Proved) => {}
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

    pub(super) fn symbolic_callable(
        &mut self,
        receiver: &Type<'g>,
        actual: &[Option<Type<'g>>],
    ) -> Work<SymbolicCallable<'g>> {
        let mut matching: Option<(InvocationMode, Type<'g>, Type<'g>, GoalId)> = None;
        let mut receiving: Vec<Option<Type<'g>>> = Vec::new();
        let mut conflicting = false;
        let mut has_contract = false;
        let mut has_arity = false;
        for i in 0..self.evidence.assumptions.len() {
            self.elab.step(1)?;
            let id = self.evidence.assumptions[i].clone();
            let GoalJudgment::Requirement {
                target: RequirementTarget::Predicate(predicate),
                arguments,
            } = &self.evidence.goals[id.index].judgment
            else {
                continue;
            };
            let Predicate::Callable(mode) = Predicate::of(predicate) else {
                continue;
            };
            let [subject, inputs, output] = arguments.as_slice() else {
                continue;
            };
            let (subject, inputs, output) = (subject.clone(), inputs.clone(), output.clone());
            let TypeTerm::Tuple(fields) = inputs.as_ref() else {
                continue;
            };
            if !self.same(&subject, receiver)? {
                continue;
            }
            has_contract = true;
            if fields.len() != actual.len() {
                continue;
            }
            has_arity = true;
            let mut fits = true;
            for (argument, parameter) in actual.iter().zip(fields) {
                self.elab.step(1)?;
                if let Some(argument) = argument
                    && primitive_name(argument) != Some("never")
                    && !self.same(argument, parameter)?
                    && !self.data_admission(argument, parameter)?
                    && !self.callable_admission(argument, parameter)?
                {
                    fits = false;
                    break;
                }
            }
            if !fits {
                continue;
            }
            if let Some((other, before, result, _)) = &matching {
                for (agreed, field) in receiving.iter_mut().zip(fields) {
                    self.elab.step(1)?;
                    if let Some(ty) = agreed
                        && !self.same(ty, field)?
                    {
                        *agreed = None;
                    }
                }
                conflicting |= !self.same(before, &inputs)? || !self.same(result, &output)?;
                if Self::mode_admits(*other, mode) {
                    continue;
                }
            } else {
                self.elab.reserve(fields.len())?;
                receiving.extend(fields.iter().cloned().map(Some));
            }
            matching = Some((mode, inputs, output, id));
        }
        self.elab.step(actual.len())?;
        let rejection = if has_contract && matching.is_none() {
            Some(if has_arity {
                ScalarFindingKind::TypeMismatch
            } else {
                ScalarFindingKind::Arity
            })
        } else {
            None
        };
        let selected = if let Some((mode, inputs, result, id)) = matching
            && !conflicting
            && actual.iter().all(Option::is_some)
        {
            let TypeTerm::Tuple(parameters) = inputs.as_ref() else {
                unreachable!()
            };
            self.elab.reserve(1 + parameters.len())?;
            Some((
                Arc::new(TypeTerm::Callable {
                    mode,
                    parameters: parameters.clone(),
                    result,
                }),
                id,
            ))
        } else {
            None
        };
        Ok(SymbolicCallable {
            receiving,
            selected,
            rejection,
        })
    }

    fn arrow_subject(
        &mut self,
        inference: &Inference<'g>,
        call: &PreparedCall<'g>,
        requirement: &RequirementMetadata<'g>,
    ) -> Work<Option<Arc<ArrowFacts<'g>>>> {
        if !matches!(requirement.target, Some(RequirementTarget::Predicate(id)) if !matches!(Predicate::of(id), Predicate::Callable(_)))
        {
            return Ok(None);
        }
        let [Some(subject)] = requirement.arguments.as_slice() else {
            return Ok(None);
        };
        let mut origin: Option<Arc<ArrowFacts<'g>>> = None;
        for (pattern, argument) in call.parameters.iter().zip(&call.fact.arguments) {
            if !self.same(pattern, subject)? {
                continue;
            }
            let Some(arrow) = inference.slots[argument].arrow.as_ref() else {
                return Ok(None);
            };
            if origin
                .as_ref()
                .is_some_and(|other| !Arc::ptr_eq(other, arrow))
            {
                return Ok(None);
            }
            origin = Some(arrow.clone());
        }
        Ok(origin)
    }

    fn same_goal(&mut self, left: usize, right: usize, weakening: bool) -> Work<bool> {
        let l_fact = &self.evidence.goals[left];
        let r_fact = &self.evidence.goals[right];
        if !match (&l_fact.arrow_origin, &r_fact.arrow_origin) {
            (None, None) => true,
            (Some(l), Some(r)) => Arc::ptr_eq(l, r),
            _ => false,
        } {
            return Ok(false);
        }
        let l = &l_fact.judgment;
        let r = &self.evidence.goals[right].judgment;
        if std::mem::discriminant(l) != std::mem::discriminant(r) {
            return Ok(false);
        }
        if let (
            GoalJudgment::Instantiate {
                determination: l, ..
            },
            GoalJudgment::Instantiate {
                determination: r, ..
            },
        ) = (l, r)
            && l != r
        {
            return Ok(false);
        }
        let (lt, rt) = (l.target(), r.target());
        let matches = lt == rt
            || (weakening
                && matches!((lt, rt), (RequirementTarget::Predicate(l), RequirementTarget::Predicate(r))
                    if matches!((Predicate::of(l), Predicate::of(r)), (Predicate::Callable(a), Predicate::Callable(b)) if Self::mode_admits(a, b))));
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

    pub(in crate::analysis::scalars) fn solve_goal(&mut self, goal: &GoalId) -> Work<GoalState> {
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
                if let Some(premise) = child.premise {
                    let parent = &mut self.evidence.goals[goal].premise;
                    *parent = Some(parent.map_or(premise, |p| p.combine(premise)));
                }
                if !self.conversion_work.contains_key(&goal) {
                    frame.state = join(frame.state, state);
                }
                frame.next += 1;
            } else {
                if let Some(work) = self.conversion_work.remove(&goal) {
                    let Some(state) = self.advance_conversion_goal(goal, work)? else {
                        continue;
                    };
                    frame.state = state;
                }
                if let Some(equation) = self.evidence.output_equations.remove(&goal) {
                    frame.state = join(frame.state, self.resume_selected_output(goal, equation)?);
                    if frame.next < self.evidence.goals[goal].dependencies.len() {
                        continue;
                    }
                }
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
                        state = GoalState::Unresolved(match self.evidence.goals[goal].premise {
                            Some(
                                InputPremise::HypotheticalConversionRole
                                | InputPremise::HypotheticalOrdinaryAndConversionRoles,
                            ) => GoalBlocker::ConversionAdmission,
                            _ => GoalBlocker::OrdinaryAdmission,
                        });
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
        let conversion_relation = matches!(self.evidence.goals[goal].judgment,
            GoalJudgment::Requirement { target: RequirementTarget::Family(id), .. } if self.conversion_family(id).is_some());
        for index in 0..self.evidence.goals[goal].assumptions.len() {
            if conversion_relation {
                break;
            } // Check the pair and conflict before using a relation.
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
                    GoalState::Unresolved(match fact.premise {
                        Some(
                            InputPremise::HypotheticalConversionRole
                            | InputPremise::HypotheticalOrdinaryAndConversionRoles,
                        ) => GoalBlocker::ConversionAdmission,
                        _ => GoalBlocker::OrdinaryAdmission,
                    })
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
            GoalJudgment::Instantiate { family, .. } => {
                let family = self.evidence_family(family)?.unwrap();
                self.continue_selected_contract(goal, family)
            }
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
        let (inputs, filters) = if let Some(conversion) = self.conversion_family(family) {
            if arguments.len() < 2 || !conversion.issues.is_empty() {
                return Ok(GoalState::Unresolved(GoalBlocker::ConversionPair(
                    crate::analysis::conversions::PairBlocker::Registration,
                )));
            }
            self.elab.reserve(2)?;
            (arguments[..2].to_vec(), Vec::new())
        } else {
            let substitutions = self.family_substitutions(header, &arguments)?;
            self.family_inputs(header, &substitutions)?
        };
        let parent = &self.evidence.goals[goal];
        let child = self.issue_determination(
            GoalJudgment::Determine {
                family,
                inputs,
                filters,
            },
            parent.site,
            parent.source,
            &[],
            None,
        )?;
        self.link_goal(goal, child)?;
        Ok(GoalState::Proved)
    }

    /// Source calls can attach their already selected header/substitution instead
    /// of running selection again. Only `solve_goal` can complete the judgment.
    pub(in crate::analysis::scalars) fn issue_determination(
        &mut self,
        judgment: GoalJudgment<'g>,
        site: Site<'g>,
        source: Site<'g>,
        routes: &[&'g Binding],
        selection: Option<GoalSelection<'g>>,
    ) -> Work<GoalId> {
        debug_assert!(matches!(judgment, GoalJudgment::Determine { .. }));
        let id = self.issue_goal(judgment, site, source, routes, false)?;
        self.evidence.goals[id.index].selection = selection;
        Ok(id)
    }

    fn collection_winner(
        &self,
        id: &DeclarationId,
        inputs: &[Type<'g>],
    ) -> Option<&'g DeclarationId> {
        let scope = match inputs {
            [source] => self.collections.scope(id, &[Some(source)]),
            [source, key] => self.collections.scope(id, &[Some(source), Some(key)]),
            _ => None,
        }?;
        scope.winner()
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
        if let Some(family) = self.conversion_family(id) {
            return self.begin_conversion_goal(goal, family, &inputs, &filters);
        }
        let prelude = self.input.graph.prelude();
        let family = self.evidence_family(id)?.unwrap();
        if *id == prelude.copy || *id == prelude.dispose {
            let Some((implementation, subject)) = self
                .primitives
                .and_then(|core| core.winner_for_inputs(id, &inputs))
            else {
                return Ok(GoalState::Unresolved(GoalBlocker::OrdinaryAdmission));
            };
            for filter in &filters {
                self.elab.step(1)?;
                if filter.position != 0 || !self.same(&filter.argument, subject)? {
                    return Ok(GoalState::Disproved);
                }
            }
            // This ground region has already passed the required-winner comparison
            // against every captured registration, even those not eligible to call.
            return self.expand_selected_contract(
                goal,
                family,
                GoalSelection {
                    implementation,
                    alternative: 0,
                    substitution: Vec::new(),
                },
            );
        }
        let required = self.collection_winner(id, &inputs);
        if required.is_none()
            && let Some(state) = self.assumption_output(goal, &family.header, &inputs, &filters)?
        {
            return Ok(state);
        }
        if let (Some(role), Some(primitives)) = (self.ordinary_scalar_role(id), self.primitives) {
            self.elab.step(primitives.providers.len())?;
            if primitives.missing_provider(id, role, &inputs) {
                return Ok(GoalState::Unresolved(GoalBlocker::UnavailableProvider));
            }
        }
        let selection = if let Some(selection) = self.evidence.goals[goal].selection.take() {
            selection
        } else {
            match self.select_goal_contract(id, &inputs, &filters)? {
                GoalChoice::Blocked => {
                    return Ok(GoalState::Unresolved(GoalBlocker::InputSelection));
                }
                GoalChoice::NoWinner => return Ok(GoalState::Disproved),
                GoalChoice::NonUniform => {
                    return Ok(GoalState::Unresolved(GoalBlocker::MissingAssumption));
                }
                GoalChoice::Selected(selection) => selection,
            }
        };
        if required.is_some_and(|id| id != selection.implementation) {
            return Ok(GoalState::Unresolved(GoalBlocker::OrdinaryAdmission));
        }
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
            if !self.same_terms(inputs, &actual_inputs)? {
                continue;
            }
            let mut agrees = filters.len() == actual_filters.len();
            for (requested, actual) in filters.iter().zip(&actual_filters) {
                if requested.position != actual.position
                    || !self.same(&requested.argument, &actual.argument)?
                {
                    agrees = false;
                    break;
                }
            }
            if !agrees
                && !self.equal_filter_domains(
                    &header.declaration.id,
                    inputs,
                    filters,
                    &actual_filters,
                )?
            {
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

    fn equal_filter_domains(
        &mut self,
        family: &DeclarationId,
        inputs: &[Type<'g>],
        left: &[Substitution<'g>],
        right: &[Substitution<'g>],
    ) -> Work<bool> {
        let mut budget = declarations::Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let same = ordinary::source::family(self.input.ordinary.unwrap(), family, &mut budget)
            .and_then(|facts| match facts {
                Ok(facts) => {
                    ordinary::source::filters_agree(facts, inputs, left, right, &mut budget)
                }
                Err(_) => Ok(false),
            });
        self.elab.statistics = budget.statistics;
        same
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

    fn select_goal_contract(
        &mut self,
        family: &'g DeclarationId,
        inputs: &[Type<'g>],
        filters: &[Substitution<'g>],
    ) -> Work<GoalChoice<'g>> {
        let mut budget = declarations::Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let selected = (|| {
            let admitted = self.ordinary_scalar_role(family).is_some();
            let inventory = self.input.ordinary.unwrap();
            let facts = if admitted {
                ordinary::source::admitted_family(inventory, family, &mut budget)?
            } else {
                ordinary::source::family(inventory, family, &mut budget)?
            };
            let Ok(facts) = facts else {
                return Ok(GoalChoice::Blocked);
            };
            budget.reserve(inputs.len() + filters.len())?;
            let inputs: Vec<_> = inputs.iter().map(|ty| Some(ty.clone())).collect();
            let filters: Vec<_> = filters.iter().map(|s| (s.position, &s.argument)).collect();
            let candidates = ordinary::source::project(facts, &inputs, &filters, &mut budget)?;
            match ordinary::source::rank(facts, &candidates, &mut budget)? {
                Some(ordinary::HypotheticalOutcome::Winner(implementation)) => {
                    budget.step(candidates.len())?;
                    let found = candidates
                        .into_iter()
                        .find(|c| c.covers && c.body.source.declaration == *implementation)
                        .unwrap();
                    Ok(GoalChoice::Selected(GoalSelection {
                        implementation,
                        alternative: found.alternative,
                        substitution: found.substitution,
                    }))
                }
                Some(_) => Ok(GoalChoice::NoWinner),
                None if admitted => Ok(GoalChoice::NonUniform),
                None => Ok(GoalChoice::Blocked),
            }
        })();
        self.elab.statistics = budget.statistics;
        selected
    }

    fn expand_selected_contract(
        &mut self,
        goal: usize,
        family: &'r ordinary::FamilyFacts<'g>,
        selection: GoalSelection<'g>,
    ) -> Work<GoalState> {
        self.evidence.goals[goal].selection = Some(selection);
        self.continue_selected_contract(goal, family)
    }

    fn continue_selected_contract(
        &mut self,
        goal: usize,
        family: &'r ordinary::FamilyFacts<'g>,
    ) -> Work<GoalState> {
        let selection = self.evidence.goals[goal].selection.as_ref().unwrap();
        let implementation = selection.implementation;
        self.elab.step(family.implementations.len())?;
        let body = family
            .implementations
            .iter()
            .find(|b| b.source.declaration == *implementation)
            .unwrap();
        let DeclarationShape::Function { result, .. } = &body.header.shape else {
            unreachable!("implementation signature")
        };
        self.elab.reserve(selection.substitution.len())?;
        let mut selected_substitutions = selection.substitution.clone();
        let binders = &body.header.binders;
        while selected_substitutions.len() != binders.len() {
            self.elab.reserve(binders.len())?;
            let mut slots = vec![None; binders.len()];
            for sub in &selected_substitutions {
                slots[sub.position] = Some(sub.argument.clone());
            }
            self.apply_callable_equations(
                &[
                    (
                        &family.header.declaration.id,
                        body.substitution.as_deref().unwrap(),
                    ),
                    (implementation, &[]),
                ],
                binders,
                &mut slots,
            )?;
            selected_substitutions = self.update_selection_slots(goal, binders, &slots)?;
            if selected_substitutions.len() == binders.len() {
                break;
            }

            let state =
                self.request_selected_output(goal, family, body, &selected_substitutions)?;
            if state != GoalState::Unresolved(GoalBlocker::SelectedOutput)
                || !matches!(
                    self.evidence.goals[goal].judgment,
                    GoalJudgment::Instantiate { .. }
                )
            {
                return Ok(state);
            }
            let Some(default) = self.try_source_output_default(family, body, &mut slots)? else {
                return Ok(state);
            };
            self.elab.reserve(1)?;
            self.evidence.goals[goal].defaults.push(default);
            selected_substitutions = self.update_selection_slots(goal, binders, &slots)?;
        }
        let source = Self::site(
            self.evidence_module(&body.source.declaration.module)?,
            body.source.signature_node,
        );
        let site = self.evidence.goals[goal].site;
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

    fn update_selection_slots(
        &mut self,
        goal: usize,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
    ) -> Work<Vec<Substitution<'g>>> {
        let substitutions = self.slot_substitutions(binders, slots)?;
        self.elab.reserve(substitutions.len())?;
        self.evidence.goals[goal]
            .selection
            .as_mut()
            .unwrap()
            .substitution = substitutions.clone();
        Ok(substitutions)
    }

    pub(super) fn instantiate_source_contract(
        &mut self,
        determination: &GoalId,
        prior: Option<&GoalId>,
        selection: GoalSelection<'g>,
        free: Vec<Substitution<'g>>,
    ) -> Work<GoalId> {
        if let Some(prior) = prior {
            let GoalJudgment::Instantiate {
                determination: previous,
                free: bindings,
                ..
            } = &self.evidence.get(prior).unwrap().judgment
            else {
                unreachable!()
            };
            if previous == determination && bindings.len() == free.len() {
                let mut agrees = true;
                for (i, binding) in free.iter().enumerate() {
                    self.elab.step(1)?;
                    let GoalJudgment::Instantiate { free: bindings, .. } =
                        &self.evidence.get(prior).unwrap().judgment
                    else {
                        unreachable!()
                    };
                    let previous = &bindings[i];
                    if previous.position != binding.position
                        || !self.same(&previous.argument.clone(), &binding.argument)?
                    {
                        agrees = false;
                        break;
                    }
                }
                if agrees {
                    return Ok(prior.clone());
                }
            }
        }
        let original = self.evidence.get(determination).unwrap();
        let GoalJudgment::Determine { family, .. } = original.judgment else {
            unreachable!()
        };
        let (site, source) = (original.site, original.source);
        self.elab.reserve(original.routes.len())?;
        let routes = original.routes.clone();
        let id = self.issue_goal(
            GoalJudgment::Instantiate {
                family,
                determination: determination.clone(),
                free,
            },
            site,
            source,
            &routes,
            false,
        )?;
        self.evidence.goals[id.index].selection = Some(selection);
        self.solve_goal(&id)?;
        Ok(id)
    }

    fn issue_header_requirement(
        &mut self,
        site: Site<'g>,
        module: &'g Module,
        requirement: &RequirementMetadata<'g>,
        substitutions: &[Substitution<'g>],
    ) -> Work<Option<GoalId>> {
        let Some(judgment) = self.requirement_term(requirement, substitutions)? else {
            return Ok(None);
        };
        self.issue_goal(
            judgment,
            site,
            Self::site(module, requirement.node),
            &requirement.routes,
            false,
        )
        .map(Some)
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
            let Some(child) =
                self.issue_header_requirement(site, module, requirement, substitutions)?
            else {
                state = GoalState::Unresolved(GoalBlocker::UnavailableRequirement);
                continue;
            };
            if matches!(
                self.evidence.goals[goal].judgment,
                GoalJudgment::Instantiate { .. }
            ) {
                self.add_goal_premise(&child, InputPremise::HypotheticalOrdinaryRole);
            }
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

    pub(super) fn add_goal_premise(&mut self, goal: &GoalId, premise: InputPremise) {
        let fact = &mut self.evidence.goals[goal.index];
        fact.premise = Some(fact.premise.map_or(premise, |p| p.combine(premise)));
    }

    pub(super) fn record_conversion_equality(
        &mut self,
        goal: &GoalId,
        position: Option<usize>,
        requested: Type<'g>,
        actual: Type<'g>,
        requested_site: Site<'g>,
        actual_site: Site<'g>,
    ) -> Work<bool> {
        self.goal_equality(
            goal.index,
            position,
            requested,
            actual,
            requested_site,
            actual_site,
        )
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
                let Some(origin) = self.evidence.goals[goal].arrow_origin.clone() else {
                    return Ok(GoalState::Unresolved(GoalBlocker::CallableEnvironment));
                };
                let mut state = GoalState::Proved;
                for capture in &origin.captures {
                    self.elab.step(1)?;
                    let Some(storage) = &capture.storage else {
                        let blocked = if predicate == Predicate::NoBorrow
                            && matches!(
                                capture.form,
                                callables::CaptureForm::CarriedReference { .. }
                            ) {
                            GoalState::Disproved
                        } else {
                            GoalState::Unresolved(GoalBlocker::CallableEnvironment)
                        };
                        state = join(state, blocked);
                        continue;
                    };
                    self.property_dependency(goal, intrinsic, storage.clone(), capture.site)?;
                    if let Some(source) = capture.value_origin
                        && let Some(arrow) = self.facts.arrows.get(&(source.module, source.node))
                    {
                        let child = self.evidence.goals[goal].dependencies.last().unwrap().index;
                        self.evidence.goals[child].arrow_origin = Some(arrow.clone());
                    }
                }
                Ok(state)
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
        if let Some((representation, element)) = self.collections.representation(ty) {
            use crate::analysis::typed::CollectionStorage;
            let stored = match representation.storage {
                CollectionStorage::Elements | CollectionStorage::UnyieldedElements => element,
                CollectionStorage::BorrowedList { .. }
                    if Predicate::of(intrinsic) == Predicate::NoBorrow =>
                {
                    return Ok(GoalState::Disproved);
                }
                CollectionStorage::BorrowedList { mutable } => {
                    let Some(list) = self.collections.list else {
                        return Ok(GoalState::Unresolved(GoalBlocker::Representation));
                    };
                    self.elab.reserve(3)?;
                    Arc::new(TypeTerm::Reference {
                        mutable,
                        referent: Arc::new(TypeTerm::Nominal {
                            declaration: &list.declaration.id,
                            arguments: vec![element],
                        }),
                    })
                }
            };
            let module = self.evidence_module(&representation.declaration.id.module)?;
            self.property_dependency(
                goal,
                intrinsic,
                stored,
                Self::site(module, representation.element.node),
            )?;
            return Ok(GoalState::Proved);
        }
        if let Some(repr) = self.result.and_then(|r| r.representation) {
            if **declaration == repr.cause.id
                || **declaration == repr.source_location.id
                || **declaration == repr.message.id
            {
                return Ok(GoalState::Proved);
            }
            if **declaration == repr.wrapper.id
                && let [payload] = arguments.as_slice()
            {
                let source = self.evidence.goals[goal].source;
                self.property_dependency(goal, intrinsic, payload.clone(), source)?;
                return Ok(GoalState::Proved);
            }
        }
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
        if !Self::mode_admits(mode, required)
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
