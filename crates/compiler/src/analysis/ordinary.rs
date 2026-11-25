//! M4.5 ordinary headers and input facts over an immutable loaded graph.
use super::declarations::{
    Budget, CheckFinding, Contract, DeclarationSummaries, FieldClass, PreludeContracts, Schema,
    check_inventory,
};
use super::*;
use crate::loader::Implementation;
use std::collections::BTreeSet;

pub(super) mod metadata;
pub(super) mod patterns;
pub(super) mod selection;
pub(super) mod source;
pub use metadata::{DefaultMetadata, HeaderMetadata, RequirementMetadata, RequirementTarget};
pub use patterns::InputKey;
pub use selection::{
    HypotheticalOutcome, InputContract, InputMatch, InputPremise, SelectionBlocker,
    SelectionReport, select_inputs,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Incomplete,
    ResourceLimit,
}

#[derive(Debug)]
pub struct ImplementationFacts<'g> {
    pub source: &'g Implementation,
    pub header: DeclarationFacts<'g>,
    pub compatibility: Compatibility,
    pub substitution: Option<Vec<Substitution<'g>>>,
    pub input_key: InputKey<'g>,
    pub alternatives: Vec<InputKey<'g>>,
    pub placement: Placement<'g>,
    input_variables: Vec<Option<usize>>,
}

/// Placement computed under the ordinary-role premise, not core clearance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement<'g> {
    FamilyModule,
    ParameterOwner(&'g DeclarationId),
    OutsideOwners,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Compatibility {
    Compatible,
    Incompatible,
    Incomplete,
}

#[derive(Debug)]
pub struct FamilyFacts<'g> {
    pub header: DeclarationFacts<'g>,
    pub implementations: Vec<ImplementationFacts<'g>>,
    pub routes: Vec<&'g Binding>,
    /// Equal input keys under the hypothetical ordinary-role premise.
    pub duplicate_keys: Vec<Vec<&'g DeclarationId>>,
    /// Completed structural comparisons under the hypothetical ordinary-role premise.
    pub comparisons: Vec<Comparison<'g>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingAdmission {
    FamilyRole,
    RequiredWinners,
    ProhibitedInputs,
}

impl FamilyFacts<'_> {
    /// M3 supplies no role or protection inventory. Neither inventory may
    /// be treated as a validated empty catalogue.
    pub fn pending_admission(&self) -> &[PendingAdmission] {
        &[
            PendingAdmission::FamilyRole,
            PendingAdmission::RequiredWinners,
            PendingAdmission::ProhibitedInputs,
        ]
    }
}

#[derive(Debug)]
pub struct Comparison<'g> {
    pub left: &'g DeclarationId,
    pub right: &'g DeclarationId,
    pub left_in_right: bool,
    pub right_in_left: bool,
    /// Indices into the respective admitted alternatives; None proves disjointness.
    pub intersection: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
pub enum DeclarationIssue<'g> {
    DuplicateMembers {
        scope: &'g Declaration,
        members: Vec<&'g Declaration>,
    },
    PreludeSignature {
        schema: Schema,
        declaration: &'g Declaration,
    },
    InlineCycle {
        steps: Vec<LayoutStep<'g>>,
    },
    UnknownLayout {
        declaration: &'g DeclarationId,
        annotation: Span,
        unknown: Option<&'g DeclarationId>,
    },
}

#[derive(Clone, Debug)]
pub struct LayoutStep<'g> {
    pub origin: &'g DeclarationId,
    pub target: &'g DeclarationId,
    pub member: &'g Declaration,
    pub annotation: Span,
}

#[derive(Debug)]
pub struct OrdinaryAnalysis<'g> {
    graph: &'g Graph,
    lexical: Option<Arc<Lexical<'g>>>,
    families: BTreeMap<&'g DeclarationId, FamilyFacts<'g>>,
    types: PartialFacts<'g>,
    summaries: DeclarationSummaries,
    declaration_issues: Vec<DeclarationIssue<'g>>,
    /// Issues a `ResourceLimit` report no longer publishes. `analyze_generics`
    /// still meters them, so its statistics on this input do not move.
    withheld_issues: Vec<DeclarationIssue<'g>>,
    unusable: BTreeSet<&'g DeclarationId>,
    headers: metadata::Headers<'g>,
    statistics: Statistics,
    completion: Completion,
    reached_validation: bool,
}

impl<'g> OrdinaryAnalysis<'g> {
    /// Whether the analysis publishes no finding (metering rule 8): the type
    /// facts' findings, the declaration issues and every header's type
    /// findings. Every field is named, so adding one fails to compile until it
    /// is decided here. Test support for `tests/support/limit.rs`.
    #[doc(hidden)]
    pub fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            types,
            declaration_issues,
            headers,
            // Metered, not published (`withheld_issues`), or facts, identity
            // and counters.
            summaries: _summaries,
            graph: _graph,
            lexical: _lexical,
            families: _families,
            withheld_issues: _withheld_issues,
            unusable: _unusable,
            statistics: _statistics,
            completion: _completion,
            reached_validation: _reached_validation,
        } = self;
        types.publishes_nothing_at_limit()
            && declaration_issues.is_empty()
            && headers
                .values()
                .all(metadata::HeaderMetadata::publishes_nothing_at_limit)
    }

    /// Empties what the analysis publishes and says so: a withheld inventory
    /// reads `ResourceLimit`, so no consumer takes the emptied findings for a
    /// complete, clean inventory (`select_inputs`, `analyze_generics`,
    /// `analyze_conversions` check the completion).
    pub(super) fn withhold_findings(&mut self) {
        self.completion = Completion::ResourceLimit;
        self.types.withhold_findings();
        self.withheld_issues = std::mem::take(&mut self.declaration_issues);
        for header in self.headers.values_mut() {
            header.withhold_findings();
        }
    }

    /// Every issue the analysis found, published or withheld, for metering.
    pub(super) fn metered_issues(&self) -> impl Iterator<Item = &DeclarationIssue<'g>> {
        self.declaration_issues.iter().chain(&self.withheld_issues)
    }

    pub(super) fn graph(&self) -> &'g Graph {
        self.graph
    }

    pub(super) fn lexical(&self) -> Option<&Arc<Lexical<'g>>> {
        self.lexical.as_ref()
    }

    pub(super) fn reached_validation(&self) -> bool {
        self.reached_validation
    }

    pub(super) fn declaration_unusable(&self, id: &DeclarationId) -> bool {
        self.unusable.contains(id)
    }

    pub(super) fn headers(&self) -> &metadata::Headers<'g> {
        &self.headers
    }

    pub(super) fn field_class(&self, id: &DeclarationId) -> FieldClass {
        self.summaries.fields(id)
    }

    pub(super) fn custom_dispose(&self, id: &DeclarationId) -> bool {
        self.summaries.custom_dispose(id)
    }

    pub(super) fn family_probes(&self) -> usize {
        self.families.len()
    }

    pub(super) fn captured_family(&self, id: &DeclarationId) -> Option<&FamilyFacts<'g>> {
        self.families.get(id)
    }

    pub fn completion(&self) -> Completion {
        self.completion
    }

    pub fn context(&self) -> GraphId {
        self.graph.id()
    }

    pub fn statistics(&self) -> Statistics {
        self.statistics
    }

    pub(super) fn mark_text_iteration_sources(
        &mut self,
        mut marks: Vec<(&'g DeclarationId, usize)>,
        budget: &mut Budget,
    ) -> Work<()> {
        let n = marks.len();
        if n == 0 {
            return Ok(());
        }
        budget.step(n.checked_mul(n.ilog2() as usize + 3).ok_or(())?)?;
        marks.sort_unstable_by_key(|&(owner, i)| (std::ptr::from_ref(owner), i));
        marks.dedup_by(|a, b| std::ptr::eq(a.0, b.0) && a.1 == b.1);
        for group in marks.chunk_by(|a, b| std::ptr::eq(a.0, b.0)) {
            let (owner, first) = group[0];
            let width = owner.module.path.len().checked_add(4).ok_or(())?;
            budget.step(
                width
                    .checked_mul(self.types.declarations.len() + 1)
                    .ok_or(())?,
            )?;
            let header = self
                .types
                .declarations
                .get_mut(owner)
                .expect("checked named function");
            let DeclarationShape::Function { parameters, .. } = &mut header.shape else {
                unreachable!("checked formal")
            };
            // A partially marked signature would make the first parameter pay
            // for storage that an unrelated sibling cannot finish checking.
            let TypeTerm::Nominal {
                declaration: list, ..
            } = parameters[first].ty.as_ref()
            else {
                unreachable!("typed List source")
            };
            budget.step(parameters.len() + group.len())?;
            let required=parameters.iter().filter(|p| matches!(p.ty.as_ref(),TypeTerm::Nominal{declaration,arguments} if std::ptr::eq(*declaration,*list) && matches!(arguments.as_slice(),[t] if matches!(t.as_ref(),TypeTerm::Primitive(p) if p.name()=="string")))).count();
            if required == group.len() {
                for &(_, i) in group {
                    parameters[i].text_iteration_source = true;
                }
            }
        }
        Ok(())
    }

    pub(super) fn mark_option_iterator(
        &mut self,
        function: &'g DeclarationId,
        formal: &'g DeclarationId,
        site: NodeId,
        budget: &mut Budget,
    ) -> Work<()> {
        let width = function.module.path.len().checked_add(4).ok_or(())?;
        budget.step(
            width
                .checked_mul(self.types.declarations.len() + 1)
                .ok_or(())?,
        )?;
        let header = self
            .types
            .declarations
            .get_mut(function)
            .expect("checked function");
        let DeclarationShape::Function { parameters, .. } = &mut header.shape else {
            unreachable!("checked formal")
        };
        budget.step(parameters.len())?;
        let parameter = parameters
            .iter_mut()
            .find(|p| std::ptr::eq(&p.declaration.id, formal))
            .expect("source formal");
        parameter.option_iteration_source = Some(site);
        Ok(())
    }

    pub fn type_facts(&self) -> &PartialFacts<'g> {
        &self.types
    }

    pub fn header_metadata(
        &self,
        id: &DeclarationId,
    ) -> Result<Option<&HeaderMetadata<'g>>, QueryError> {
        if id.context != self.context()
            || !self
                .graph
                .modules()
                .iter()
                .flat_map(|m| &m.declarations)
                .any(|d| d.id == *id)
        {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.headers.get(id))
    }

    pub fn declaration_issues(&self) -> &[DeclarationIssue<'g>] {
        &self.declaration_issues
    }

    pub fn family(&self, id: &DeclarationId) -> Result<Option<&FamilyFacts<'g>>, QueryError> {
        if id.kind != DeclarationKind::Family
            || !self
                .graph
                .modules()
                .iter()
                .flat_map(|m| &m.declarations)
                .any(|d| d.id == *id)
        {
            return Err(QueryError::InvalidRequest);
        }
        Ok(self.families.get(id))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Constructor<'g> {
    Primitive(&'g Intrinsic),
    Parameter(&'g DeclarationId, usize),
    Reference(bool),
    Nominal(&'g DeclarationId),
    Variant(&'g DeclarationId),
    Tuple,
    Callable(InvocationMode, usize),
}

pub(super) fn shape<'a, 'g>(
    ty: &'a TypeTerm<'g>,
) -> (
    Constructor<'g>,
    impl ExactSizeIterator<Item = &'a Type<'g>> + DoubleEndedIterator + Clone,
) {
    let (head, children, last): (_, &[_], _) = match ty {
        TypeTerm::Primitive(id) => (Constructor::Primitive(id), &[], None),
        TypeTerm::Parameter {
            declaration,
            position,
        } => (Constructor::Parameter(declaration, *position), &[], None),
        TypeTerm::Reference { mutable, referent } => (
            Constructor::Reference(*mutable),
            std::slice::from_ref(referent),
            None,
        ),
        TypeTerm::Nominal {
            declaration,
            arguments,
        } => (Constructor::Nominal(declaration), arguments, None),
        TypeTerm::Variant {
            parent,
            declaration,
        } => (
            Constructor::Variant(declaration),
            std::slice::from_ref(parent),
            None,
        ),
        TypeTerm::Tuple(fields) => (Constructor::Tuple, fields, None),
        TypeTerm::Callable {
            mode,
            parameters,
            result,
        } => (
            Constructor::Callable(*mode, parameters.len()),
            parameters,
            Some(result),
        ),
    };
    // Store a callable result as its own last child, with arity retained in the head.
    (
        head,
        Children {
            parameters: children.iter(),
            result: last,
        },
    )
}

#[derive(Clone)]
struct Children<'a, 'g> {
    parameters: std::slice::Iter<'a, Type<'g>>,
    result: Option<&'a Type<'g>>,
}
impl<'a, 'g> Iterator for Children<'a, 'g> {
    type Item = &'a Type<'g>;
    fn next(&mut self) -> Option<Self::Item> {
        self.parameters.next().or_else(|| self.result.take())
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.parameters.len() + usize::from(self.result.is_some());
        (size, Some(size))
    }
}
impl DoubleEndedIterator for Children<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.result.take().or_else(|| self.parameters.next_back())
    }
}
impl ExactSizeIterator for Children<'_, '_> {}

fn compatibility<'g>(
    types: &PartialFacts<'g>,
    family: &DeclarationFacts<'g>,
    header: &DeclarationFacts<'g>,
    module: &'g Module,
    signature: NodeId,
    budget: &mut Budget,
) -> Work<(Compatibility, Option<Vec<Substitution<'g>>>)> {
    let DeclarationShape::Function {
        parameters: expected,
        result: expected_result,
    } = &family.shape
    else {
        unreachable!()
    };
    let DeclarationShape::Function {
        parameters: actual,
        result: actual_result,
    } = &header.shape
    else {
        unreachable!()
    };
    if expected.len() != actual.len() {
        return Ok((Compatibility::Incompatible, None));
    }
    budget.reserve(family.binders.len())?;
    let mut bindings = vec![None; family.binders.len()];
    for &child in &module.syntax.nodes[signature].children {
        budget.step(1)?;
        if module.syntax.nodes[child].kind != NodeKind::TypeArguments {
            continue;
        }
        let args = &module.syntax.nodes[child].children;
        if args.len() > bindings.len() {
            return Ok((Compatibility::Incompatible, None));
        }
        for (&arg, slot) in args.iter().zip(&mut bindings) {
            budget.step(1)?;
            let Some(use_) =
                types
                    .uses
                    .get(&(&module.key, module.syntax.nodes[arg].span.start(), arg))
            else {
                return Ok((Compatibility::Incomplete, None));
            };
            *slot = Some(&use_.ty);
        }
    }
    budget.reserve(expected.len().checked_add(1).ok_or(())?)?;
    let mut pairs = Vec::with_capacity(expected.len() + 1);
    pairs.push((expected_result, actual_result, true));
    pairs.extend(
        expected
            .iter()
            .zip(actual)
            .map(|(e, a)| (&e.ty, &a.ty, true)),
    );
    while let Some((mut e, a, flexible)) = pairs.pop() {
        budget.step(1)?;
        let mut flexible_children = flexible;
        if flexible && let TypeTerm::Parameter { position, .. } = e.as_ref() {
            let slot = &mut bindings[*position];
            if let Some(bound) = *slot {
                e = bound;
                flexible_children = false;
            } else {
                // Family and implementation binders are disjoint; actual terms
                // contain only rigid implementation binders, so no cycle can enter.
                *slot = Some(a);
                continue;
            }
        }
        let (eh, ec) = shape(e);
        let (ah, ac) = shape(a);
        if eh != ah || ec.len() != ac.len() {
            return Ok((Compatibility::Incompatible, None));
        }
        budget.reserve(ec.len())?;
        pairs.extend(ec.zip(ac).map(|(e, a)| (e, a, flexible_children)));
    }
    if bindings.iter().any(Option::is_none) {
        return Ok((Compatibility::Incomplete, None));
    }
    budget.reserve(bindings.len())?;
    Ok((
        Compatibility::Compatible,
        Some(
            family
                .binders
                .iter()
                .zip(bindings)
                .map(|(b, a)| Substitution {
                    binder: &b.declaration.id,
                    position: b.position,
                    argument: Arc::clone(a.unwrap()),
                })
                .collect(),
        ),
    ))
}

impl<'g> FamilyFacts<'g> {
    fn compare(&mut self, budget: &mut Budget) -> Work<()> {
        for (i, left) in self.implementations.iter().enumerate() {
            budget.step(1)?;
            for right in &self.implementations[i + 1..] {
                budget.step(1)?;
                if left.alternatives.is_empty() || right.alternatives.is_empty() {
                    continue;
                }
                let (a, b) = (&left.alternatives, &right.alternatives);
                let left_in_right = patterns::included(a, b, budget)?;
                let right_in_left = patterns::included(b, a, budget)?;
                let intersection = patterns::intersection(a, b, budget)?;
                budget.reserve(1)?;
                self.comparisons.push(Comparison {
                    left: &left.source.declaration,
                    right: &right.source.declaration,
                    left_in_right,
                    right_in_left,
                    intersection,
                });
            }
        }
        Ok(())
    }

    fn find_duplicates(&mut self, budget: &mut Budget) -> Work<()> {
        let bodies = &self.implementations;
        budget.reserve(bodies.len())?;
        let mut duplicate = vec![false; bodies.len()];
        for (i, body) in bodies.iter().enumerate() {
            budget.step(1)?;
            if duplicate[i] {
                continue;
            }
            let key = &body.input_key;
            let mut group = Vec::new();
            for (j, other) in bodies.iter().enumerate().skip(i + 1) {
                budget.step(1)?;
                if !duplicate[j] && key.same(&other.input_key, budget)? {
                    if group.is_empty() {
                        budget.reserve(1)?;
                        group.push(&body.source.declaration);
                    }
                    budget.reserve(1)?;
                    group.push(&other.source.declaration);
                    duplicate[j] = true;
                }
            }
            if !group.is_empty() {
                budget.reserve(1)?;
                self.duplicate_keys.push(group);
            }
        }
        Ok(())
    }
}

impl<'g> OrdinaryAnalysis<'g> {
    fn validate_inventory(&mut self, budget: &mut Budget) -> Work<()> {
        let prelude = self.graph.prelude();
        let schemas = PreludeContracts {
            copy: Contract {
                declaration: &prelude.copy,
                schema: Schema::BorrowedCopy,
            },
            dispose: Contract {
                declaration: &prelude.dispose,
                schema: Schema::ConsumingDispose,
            },
        };
        let (result, mut facts) = check_inventory(self.graph, &self.types, schemas, budget);
        self.summaries = facts.take_summaries();
        if self.lexical.is_some() && result.is_ok() {
            for checked in facts.declarations() {
                budget.step(1)?;
                if !checked.members_unique {
                    budget.reserve(1)?;
                    self.unusable.insert(&checked.types.declaration.id);
                }
            }
        }
        for finding in facts.findings() {
            budget.reserve(1)?;
            let issue = match finding {
                CheckFinding::DuplicateMembers { scope, members } => {
                    budget.reserve(members.len())?;
                    DeclarationIssue::DuplicateMembers {
                        scope,
                        members: members.clone(),
                    }
                }
                CheckFinding::PreludeSignature { schema, actual } => {
                    if self.lexical.is_some() {
                        budget.reserve(1)?;
                        self.unusable.insert(&actual.declaration.id);
                    }
                    DeclarationIssue::PreludeSignature {
                        schema: *schema,
                        declaration: actual.declaration,
                    }
                }
                CheckFinding::InlineCycle { steps } => {
                    budget.reserve(steps.len())?;
                    if self.lexical.is_some() {
                        budget.reserve(steps.len())?;
                        self.unusable.extend(steps.iter().map(|step| step.origin));
                    }
                    DeclarationIssue::InlineCycle {
                        steps: steps
                            .iter()
                            .map(|step| LayoutStep {
                                origin: step.origin,
                                target: step.target,
                                member: step.source.member,
                                annotation: step.source.annotation.span.clone(),
                            })
                            .collect(),
                    }
                }
            };
            self.declaration_issues.push(issue);
        }
        for obligation in facts.obligations() {
            if self.lexical.is_some() {
                budget.reserve(1)?;
                self.unusable.insert(obligation.declaration);
            }
            budget.reserve(1)?;
            self.declaration_issues
                .push(DeclarationIssue::UnknownLayout {
                    declaration: obligation.declaration,
                    annotation: obligation.source.annotation.span.clone(),
                    unknown: obligation.unknown_declaration,
                });
        }
        result
    }

    fn collect(&mut self, budget: &mut Budget) -> Work<()> {
        for module in self.graph.modules() {
            for declaration in &module.declarations {
                budget.step(1)?;
                if declaration.id.kind != DeclarationKind::Family || declaration.scope.is_some() {
                    continue;
                }
                if self.types.declarations.contains_key(&declaration.id) {
                    budget.reserve(1)?;
                    let header = self.types.declarations.remove(&declaration.id).unwrap();
                    self.families.insert(
                        &declaration.id,
                        FamilyFacts {
                            header,
                            implementations: Vec::new(),
                            routes: Vec::new(),
                            duplicate_keys: Vec::new(),
                            comparisons: Vec::new(),
                        },
                    );
                }
            }
        }
        for binding in self.graph.bindings() {
            budget.step(1)?;
            if let Target::Declaration(id) = &binding.target
                && let Some(family) = self.families.get_mut(id)
            {
                budget.reserve(1)?;
                family.routes.push(binding);
            }
        }
        for source in self.graph.implementations() {
            budget.step(1)?;
            let Some(family) = self.families.get_mut(&source.family) else {
                continue;
            };
            let Some(header) = self.types.declarations.get(&source.declaration) else {
                continue;
            };
            let mut owner = None;
            for module in self.graph.modules() {
                budget.step(1)?;
                if module.key == source.declaration.module {
                    owner = Some(module);
                    break;
                }
            }
            let module = owner.expect("captured implementation has a captured module");
            let (compatibility, substitution) = compatibility(
                &self.types,
                &family.header,
                header,
                module,
                source.signature_node,
                budget,
            )?;
            let (key, input_variables) = patterns::input_key(header, budget)?;
            let placement = if source.family.module == source.declaration.module {
                Placement::FamilyModule
            } else if let Some(owner) = key.parameter_owner(&source.declaration.module, budget)? {
                Placement::ParameterOwner(owner)
            } else {
                Placement::OutsideOwners
            };
            budget.reserve(1)?;
            let header = self.types.declarations.remove(&source.declaration).unwrap();
            family.implementations.push(ImplementationFacts {
                source,
                header,
                compatibility,
                substitution,
                input_key: key,
                alternatives: Vec::new(),
                placement,
                input_variables,
            });
            let body = family.implementations.last_mut().unwrap();
            if let Some(alternatives) =
                patterns::alternatives(&body.input_key, &self.types, budget)?
            {
                body.alternatives = alternatives;
            }
        }
        for family in self.families.values_mut() {
            family.find_duplicates(budget)?;
            family.compare(budget)?;
        }
        Ok(())
    }
}

/// Analyze an actual M3 capture. M3 does not carry core role/protection admission.
/// Results remain partial, never a checked ordinary collection or source call.
/// A `ResourceLimit` report publishes no finding (metering rule 8).
pub fn analyze_ordinary(graph: &Graph, limits: AnalysisLimits) -> OrdinaryAnalysis<'_> {
    let mut report = analyze_ordinary_types(graph, limits, Statistics::default(), false, false);
    if report.completion == Completion::ResourceLimit {
        report.withhold_findings();
    }
    report
}

pub(super) fn analyze_typed_inventory(
    graph: &Graph,
    limits: AnalysisLimits,
) -> OrdinaryAnalysis<'_> {
    analyze_ordinary_types(graph, limits, Statistics::default(), true, true)
}

// Complete M4.6 nominal type uses on the same immutable graph. This
// does not take defaults for a family argument or an implementation equation.
pub(super) fn extend_nominal_types<'g>(
    input: &OrdinaryAnalysis<'g>,
    budget: &mut Budget,
) -> Work<Option<OrdinaryAnalysis<'g>>> {
    if input.completion() == Completion::ResourceLimit {
        return Ok(None);
    }
    for obligation in input.type_facts().obligations() {
        budget.step(1)?;
        if matches!(
            obligation.reason,
            Unsupported::GenericDefault | Unsupported::NeedsTypeArguments
        ) {
            let extended = analyze_ordinary_types(
                input.graph(),
                budget.limits,
                budget.statistics,
                true,
                false,
            );
            budget.statistics = extended.statistics();
            return Ok(Some(extended));
        }
    }
    Ok(None)
}

fn analyze_ordinary_types(
    graph: &Graph,
    limits: AnalysisLimits,
    statistics: Statistics,
    complete_defaults: bool,
    integrated: bool,
) -> OrdinaryAnalysis<'_> {
    let mut analyzer = Analyzer::new(graph, limits, complete_defaults);
    analyzer.statistics = statistics;
    let mut headers = BTreeMap::new();
    let elaborated = analyzer
        .analyze_inventory(Some(&mut headers), integrated)
        .is_ok();
    let mut report = OrdinaryAnalysis {
        graph,
        families: BTreeMap::new(),
        lexical: analyzer.lexical,
        types: analyzer.facts,
        summaries: DeclarationSummaries::default(),
        declaration_issues: Vec::new(),
        withheld_issues: Vec::new(),
        unusable: BTreeSet::new(),
        headers,
        statistics: analyzer.statistics,
        completion: Completion::Incomplete,
        reached_validation: elaborated,
    };
    let mut budget = Budget {
        limits,
        statistics: report.statistics,
    };
    if !elaborated
        || report
            .validate_inventory(&mut budget)
            .and_then(|()| report.collect(&mut budget))
            .is_err()
    {
        report.completion = Completion::ResourceLimit;
    }
    report.statistics = budget.statistics;
    report
}
