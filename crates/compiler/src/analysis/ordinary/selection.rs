use super::*;

#[derive(Clone, Debug)]
pub enum InputContract<'g> {
    Established(Type<'g>),
    Unresolved,
    BareNever,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HypotheticalOutcome<'g> {
    Winner(&'g DeclarationId),
    NoMatch,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionBlocker {
    DeclarationPrerequisite,
    InputShape,
    HeaderOrPlacement,
    Comparison,
}

#[derive(Debug)]
pub struct InputMatch<'g> {
    pub implementation: &'g DeclarationId,
    pub alternative: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPremise {
    HypotheticalOrdinaryRole,
    HypotheticalConversionRole,
    HypotheticalOrdinaryAndConversionRoles,
}

impl InputPremise {
    pub(in crate::analysis) fn has_conversion(self) -> bool {
        matches!(
            self,
            Self::HypotheticalConversionRole | Self::HypotheticalOrdinaryAndConversionRoles
        )
    }

    pub(in crate::analysis) fn combine(self, other: Self) -> Self {
        if self == other {
            self
        } else {
            Self::HypotheticalOrdinaryAndConversionRoles
        }
    }
}

#[derive(Debug)]
pub struct SelectionReport<'g> {
    /// Applies to all matches, comparisons and outcomes, including partial facts.
    pub premise: InputPremise,
    pub completion: Completion,
    pub outcome: Option<HypotheticalOutcome<'g>>,
    pub blocker: Option<SelectionBlocker>,
    pub matches: Vec<InputMatch<'g>>,
    /// Indices into the family's completed comparison witnesses.
    pub comparisons: Vec<usize>,
    /// Explicit family positions awaiting output agreement, not used in ranking.
    pub pending_output_arguments: Vec<usize>,
    pub statistics: Statistics,
}

pub(in crate::analysis) fn declaration<'g>(
    graph: &'g Graph,
    id: &DeclarationId,
    budget: &mut Budget,
) -> Work<Option<(&'g Module, &'g Declaration)>> {
    if id.context != graph.id() {
        return Ok(None);
    }
    for module in graph.modules() {
        budget.step(1)?;
        if module.key == id.module {
            budget.step(module.declarations.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
            return Ok(module
                .declarations
                .binary_search_by(|d| d.id.cmp(id))
                .ok()
                .map(|i| (module, &module.declarations[i])));
        }
    }
    Ok(None)
}

fn binder_count(module: &Module, header: NodeId, budget: &mut Budget) -> Work<usize> {
    for &child in &module.syntax.nodes[header].children {
        budget.step(1)?;
        let node = &module.syntax.nodes[child];
        if matches!(node.kind, NodeKind::GenericParameters { .. }) {
            return Ok(node.children.len());
        }
    }
    Ok(0)
}

#[derive(PartialEq, Eq)]
pub(in crate::analysis) enum RequestShape {
    Ground,
    Unestablished,
    Invalid,
}

pub(in crate::analysis) fn validate<'a, 'g>(
    graph: &'g Graph,
    types: impl Iterator<Item = &'a Type<'g>>,
    budget: &mut Budget,
) -> Work<RequestShape>
where
    'g: 'a,
{
    let mut pending = Vec::new();
    let mut unestablished = false;
    for root in types {
        budget.reserve(1)?;
        pending.push((root, 1));
        while let Some((ty, depth)) = pending.pop() {
            budget.step(1)?;
            if depth > budget.limits.max_depth.min(64) {
                return Err(());
            }
            budget.statistics.max_depth = budget.statistics.max_depth.max(depth);
            let valid = match ty.as_ref() {
                TypeTerm::Primitive(id) => {
                    let intrinsics = &graph.prelude().intrinsics;
                    budget.step(intrinsics.len())?;
                    id.namespace() == Namespace::TypeModule
                        && intrinsics.iter().any(|actual| actual == *id)
                }
                TypeTerm::Nominal {
                    declaration: id,
                    arguments,
                } => {
                    let Some((module, nominal)) = declaration(graph, id, budget)? else {
                        return Ok(RequestShape::Invalid);
                    };
                    unestablished |= nominal.scope.is_some();
                    matches!(
                        nominal.id.kind,
                        DeclarationKind::Enum | DeclarationKind::Record
                    ) && binder_count(module, nominal.node, budget)? == arguments.len()
                }
                TypeTerm::Variant {
                    parent,
                    declaration: id,
                } => {
                    let Some((_, variant)) = declaration(graph, id, budget)? else {
                        return Ok(RequestShape::Invalid);
                    };
                    variant.id.kind == DeclarationKind::Variant
                        && matches!(parent.as_ref(), TypeTerm::Nominal { declaration: parent, .. }
                            if parent.kind == DeclarationKind::Enum && parent.module == id.module && variant.scope == Some(parent.start))
                }
                TypeTerm::Parameter {
                    declaration: id,
                    position,
                } => {
                    unestablished = true;
                    let Some((module, binder)) = declaration(graph, id, budget)? else {
                        return Ok(RequestShape::Invalid);
                    };
                    if binder.id.kind != DeclarationKind::TypeParameter {
                        return Ok(RequestShape::Invalid);
                    }
                    let mut found = false;
                    for node in &module.syntax.nodes {
                        budget.step(1)?;
                        if matches!(node.kind, NodeKind::GenericParameters { .. })
                            && node.children.get(*position) == Some(&binder.node)
                        {
                            found = true;
                            break;
                        }
                    }
                    found
                }
                _ => true,
            };
            if !valid {
                return Ok(RequestShape::Invalid);
            }
            let (_, children) = shape(ty);
            budget.reserve(children.len())?;
            pending.extend(children.rev().map(|child| (child, depth + 1)));
        }
    }
    Ok(if unestablished {
        RequestShape::Unestablished
    } else {
        RequestShape::Ground
    })
}

fn match_and_rank<'g>(
    family: &FamilyFacts<'g>,
    actual: &InputKey<'g>,
    explicit: &[Option<InputKey<'g>>],
    output: &mut SelectionReport<'g>,
    budget: &mut Budget,
) -> Work<()> {
    for body in &family.implementations {
        budget.step(1)?;
        if let Some(alternative) = patterns::first_match(body, actual, explicit, budget)? {
            budget.reserve(1)?;
            output.matches.push(InputMatch {
                implementation: &body.source.declaration,
                alternative,
            });
        }
    }
    let matches = &output.matches;
    if matches.is_empty() {
        output.outcome = Some(HypotheticalOutcome::NoMatch);
        return Ok(());
    }
    budget.reserve(matches.len())?;
    let mut wins = vec![0; matches.len()];
    for (i, a) in matches.iter().enumerate() {
        for (j, b) in matches.iter().enumerate().skip(i + 1) {
            budget.step(1)?;
            let mut found = false;
            for (index, relation) in family.comparisons.iter().enumerate() {
                budget.step(1)?;
                if relation.left == a.implementation && relation.right == b.implementation {
                    found = true;
                    budget.reserve(1)?;
                    output.comparisons.push(index);
                    match (relation.left_in_right, relation.right_in_left) {
                        (true, false) => wins[i] += 1,
                        (false, true) => wins[j] += 1,
                        _ => {}
                    }
                    break;
                }
            }
            if !found {
                output.blocker = Some(SelectionBlocker::Comparison);
                return Ok(());
            }
        }
    }
    output.outcome = Some(
        wins.iter()
            .position(|wins| *wins == matches.len() - 1)
            .map_or(HypotheticalOutcome::Ambiguous, |i| {
                HypotheticalOutcome::Winner(matches[i].implementation)
            }),
    );
    Ok(())
}

fn query<'g>(
    analysis: &OrdinaryAnalysis<'g>,
    id: &DeclarationId,
    inputs: &[InputContract<'g>],
    explicit: &[Type<'g>],
    output: &mut SelectionReport<'g>,
    budget: &mut Budget,
) -> Work<Result<(), QueryError>> {
    let Some((module, decl)) = declaration(analysis.graph, id, budget)? else {
        return Ok(Err(QueryError::InvalidRequest));
    };
    if id.kind != DeclarationKind::Family || decl.scope.is_some() {
        return Ok(Err(QueryError::InvalidRequest));
    }
    let signature = module.syntax.nodes[decl.node].children[0];
    let mut arity = 0;
    for &child in &module.syntax.nodes[signature].children {
        budget.step(1)?;
        if module.syntax.nodes[child].kind == NodeKind::ValueParameters {
            arity = module.syntax.nodes[child].children.len();
        }
    }
    if inputs.len() != arity || explicit.len() > binder_count(module, signature, budget)? {
        return Ok(Err(QueryError::InvalidRequest));
    }
    budget.step(inputs.len())?;
    let established = inputs.iter().filter_map(|input| match input {
        InputContract::Established(ty) => Some(ty),
        _ => None,
    });
    let shape = validate(analysis.graph, established.clone(), budget)?;
    if shape == RequestShape::Invalid {
        return Ok(Err(QueryError::InvalidRequest));
    }
    budget.reserve(explicit.len())?;
    let mut explicit_shapes = Vec::with_capacity(explicit.len());
    for argument in explicit {
        let shape = validate(analysis.graph, std::iter::once(argument), budget)?;
        if shape == RequestShape::Invalid {
            return Ok(Err(QueryError::InvalidRequest));
        }
        explicit_shapes.push(shape);
    }
    if analysis.completion == Completion::ResourceLimit {
        return Err(());
    }
    budget.step(inputs.len())?;
    if shape == RequestShape::Unestablished || established.clone().count() != arity {
        output.blocker = Some(SelectionBlocker::InputShape);
        return Ok(Ok(()));
    }
    let family = match eligible_family(analysis, id, budget)? {
        Ok(family) => family,
        Err(blocker) => {
            output.blocker = Some(blocker);
            return Ok(Ok(()));
        }
    };
    let mut filter = Vec::new();
    if !explicit.is_empty() {
        let (_, input_positions) = patterns::input_key(&family.header, budget)?;
        budget.reserve(explicit.len())?;
        for (position, argument) in explicit.iter().enumerate() {
            budget.step(1)?;
            if input_positions[position].is_some() {
                if explicit_shapes[position] == RequestShape::Unestablished {
                    output.blocker = Some(SelectionBlocker::InputShape);
                    return Ok(Ok(()));
                }
                filter.push(Some(patterns::ground(std::iter::once(argument), budget)?));
            } else {
                budget.reserve(1)?;
                output.pending_output_arguments.push(position);
                filter.push(None);
            }
        }
    }
    budget.step(inputs.len())?;
    let key = patterns::ground(established, budget)?;
    match_and_rank(family, &key, &filter, output, budget)?;
    Ok(Ok(()))
}

pub(super) fn eligible_family<'a, 'g>(
    analysis: &'a OrdinaryAnalysis<'g>,
    id: &DeclarationId,
    budget: &mut Budget,
) -> Work<Result<&'a FamilyFacts<'g>, SelectionBlocker>> {
    budget.step(analysis.types.obligations.len())?;
    if !analysis.declaration_issues.is_empty()
        || !analysis.types.findings.is_empty()
        || analysis.types.obligations.values().any(|o| {
            !matches!(
                o.reason,
                Unsupported::Requirements | Unsupported::GenericDefault
            )
        })
    {
        return Ok(Err(SelectionBlocker::DeclarationPrerequisite));
    }
    eligible_captured_family(analysis, id, budget)
}

/// Integrated callers validate the canonical role and its dependent source inventory.
pub(super) fn eligible_captured_family<'a, 'g>(
    analysis: &'a OrdinaryAnalysis<'g>,
    id: &DeclarationId,
    budget: &mut Budget,
) -> Work<Result<&'a FamilyFacts<'g>, SelectionBlocker>> {
    budget.step(1)?;
    let Some(family) = analysis.families.get(id) else {
        return Ok(Err(SelectionBlocker::HeaderOrPlacement));
    };
    let mut count = 0;
    for source in analysis.graph.implementations() {
        budget.step(1)?;
        if source.family == *id {
            count += 1;
        }
    }
    budget.step(count)?;
    if family.implementations.len() != count
        || !family.duplicate_keys.is_empty()
        || family.implementations.iter().any(|body| {
            body.compatibility != Compatibility::Compatible
                || body.alternatives.is_empty()
                || body.placement == Placement::OutsideOwners
        })
    {
        return Ok(Err(SelectionBlocker::HeaderOrPlacement));
    }
    Ok(Ok(family))
}

/// Compare established input contracts, not source expressions. Since M3 has
/// no core role or protection record, every settled result is hypothetical.
pub fn select_inputs<'g>(
    analysis: &OrdinaryAnalysis<'g>,
    family: &DeclarationId,
    inputs: &[InputContract<'g>],
    explicit: &[Type<'g>],
    limits: AnalysisLimits,
) -> Result<SelectionReport<'g>, QueryError> {
    let mut budget = Budget {
        limits,
        statistics: Statistics::default(),
    };
    let mut output = SelectionReport {
        premise: InputPremise::HypotheticalOrdinaryRole,
        completion: Completion::Incomplete,
        outcome: None,
        blocker: None,
        matches: Vec::new(),
        comparisons: Vec::new(),
        pending_output_arguments: Vec::new(),
        statistics: Statistics::default(),
    };
    match query(analysis, family, inputs, explicit, &mut output, &mut budget) {
        Ok(result) => result?,
        Err(()) => output.completion = Completion::ResourceLimit,
    }
    output.statistics = budget.statistics;
    Ok(output)
}
