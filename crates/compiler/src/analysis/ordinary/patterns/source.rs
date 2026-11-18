//! Compare only established columns, retaining the original alternative and binders.
use super::*;
use crate::analysis::ordinary::source::SourceCandidate;
use relations::{Site, Solution, chase, match_vector, unify};

pub(in crate::analysis::ordinary) fn project<'r, 'g>(
    family: &'r FamilyFacts<'g>,
    inputs: &[Option<Type<'g>>],
    filters: &[(usize, &Type<'g>)],
    keep_domain: bool,
    budget: &mut Budget,
) -> Work<Vec<SourceCandidate<'r, 'g>>> {
    let mut rigids = Vec::new();
    let mut rigid_indices = BTreeMap::new();
    let domain = flatten(
        inputs
            .iter()
            .filter_map(Option::as_ref)
            .chain(filters.iter().map(|(_, t)| *t)),
        |head, budget| {
            if let Constructor::Parameter(id, position) = head {
                if let Some(&index) = rigid_indices.get(&(id, position)) {
                    return Ok(Head::Variable(index));
                }
                budget.reserve(2)?;
                let index = rigids.len();
                rigid_indices.insert((id, position), index);
                rigids.push((id, position));
                return Ok(Head::Variable(index));
            }
            Ok(Head::Fixed(head))
        },
        budget,
    )?;
    let mut out = Vec::new();
    for body in &family.implementations {
        budget.step(1)?;
        let header = body
            .substitution
            .as_deref()
            .expect("eligible body has a substitution");
        let equations = flatten(
            filters.iter().map(|(i, _)| &header[*i].argument),
            |head, _| {
                Ok(if let Constructor::Parameter(_, position) = head {
                    Head::Variable(
                        body.input_variables[position]
                            .expect("explicit input filter uses input binder"),
                    )
                } else {
                    Head::Fixed(head)
                })
            },
            budget,
        )?;
        for (alternative, actual_pattern) in body.alternatives.iter().enumerate() {
            budget.step(1)?;
            let mut nodes = Vec::new();
            for (term, observed) in terms(&actual_pattern.nodes).zip(inputs) {
                budget.step(1)?;
                if observed.is_some() {
                    budget.reserve(term.len())?;
                    nodes.extend_from_slice(term);
                }
            }
            budget.reserve(equations.nodes.len())?;
            nodes.extend_from_slice(&equations.nodes);
            let projected = InputKey { nodes };
            let Some(solved) = unify(&projected, &domain, budget)? else {
                continue;
            };
            let covers = match_vector(
                &projected.nodes,
                &domain.nodes,
                &mut BTreeMap::new(),
                budget,
            )?;
            let observed_domain = if keep_domain {
                Some(restricted_domain(&domain, inputs.len(), &solved, budget)?)
            } else {
                None
            };
            let mut substitution = Vec::new();
            for (binder, variable) in body.header.binders.iter().zip(&body.input_variables) {
                budget.step(1)?;
                if let Some(source) = variable.and_then(|index| solved.get(&(false, index)))
                    && let Some(argument) = materialize(*source, &solved, &rigids, budget, 1)?
                {
                    budget.reserve(1)?;
                    substitution.push(Substitution {
                        binder: &binder.declaration.id,
                        position: binder.position,
                        argument,
                    });
                }
            }
            budget.reserve(1)?;
            out.push(SourceCandidate {
                body,
                alternative,
                covers,
                substitution,
                observed_domain,
            });
        }
    }
    Ok(out)
}

fn restricted_domain<'g>(
    domain: &InputKey<'g>,
    columns: usize,
    solved: &Solution<'_, 'g>,
    budget: &mut Budget,
) -> Work<InputKey<'g>> {
    let mut names = BTreeMap::new();
    let mut nodes = Vec::new();
    for term in terms(&domain.nodes).take(columns) {
        write_domain(
            Site { side: true, term },
            solved,
            &mut names,
            &mut nodes,
            budget,
            1,
        )?;
    }
    Ok(InputKey { nodes })
}

fn write_domain<'g>(
    site: Site<'_, 'g>,
    solved: &Solution<'_, 'g>,
    names: &mut BTreeMap<(bool, usize), usize>,
    out: &mut Vec<Node<'g>>,
    budget: &mut Budget,
    depth: usize,
) -> Work<usize> {
    if depth > budget.limits.max_depth.min(64) {
        return Err(());
    }
    budget.statistics.max_depth = budget.statistics.max_depth.max(depth);
    let root = chase(site, solved, budget)?;
    let mut term = root.term[0];
    if let Head::Variable(id) = term.head {
        let next = names.len();
        budget.reserve(1)?;
        let name = *names.entry((root.side, id)).or_insert(next);
        term.head = Head::Variable(name);
    }
    let start = out.len();
    budget.reserve(1)?;
    out.push(term);
    let mut height = 0;
    for child in root.children() {
        height = height.max(write_domain(child, solved, names, out, budget, depth + 1)?);
    }
    let size = out.len() - start;
    out[start].size = size;
    out[start].height = height + 1;
    Ok(height + 1)
}

pub(super) fn materialize<'g>(
    root: Site<'_, 'g>,
    solved: &Solution<'_, 'g>,
    rigids: &[(&'g DeclarationId, usize)],
    budget: &mut Budget,
    depth: usize,
) -> Work<Option<Type<'g>>> {
    budget.step(1)?;
    if depth > budget.limits.max_depth.min(64) {
        return Err(());
    }
    budget.statistics.max_depth = budget.statistics.max_depth.max(depth);
    let node = chase(root, solved, budget)?;
    let head = match node.term[0].head {
        Head::Fixed(head) => head,
        Head::Variable(i) if node.side => {
            let (declaration, position) = rigids[i];
            Constructor::Parameter(declaration, position)
        }
        Head::Variable(_) => return Ok(None),
    };
    budget.reserve(node.term[0].children + 1)?;
    let mut children = Vec::with_capacity(node.term[0].children);
    for child in node.children() {
        let Some(child) = materialize(child, solved, rigids, budget, depth + 1)? else {
            return Ok(None);
        };
        children.push(child);
    }
    let term = match head {
        Constructor::Primitive(id) => TypeTerm::Primitive(id),
        Constructor::Parameter(declaration, position) => TypeTerm::Parameter {
            declaration,
            position,
        },
        Constructor::Reference(mutable) => TypeTerm::Reference {
            mutable,
            referent: children.pop().unwrap(),
        },
        Constructor::Nominal(declaration) => TypeTerm::Nominal {
            declaration,
            arguments: children,
        },
        Constructor::Variant(declaration) => TypeTerm::Variant {
            declaration,
            parent: children.pop().unwrap(),
        },
        Constructor::Tuple => TypeTerm::Tuple(children),
        Constructor::Callable(mode, _) => TypeTerm::Callable {
            mode,
            result: children.pop().unwrap(),
            parameters: children,
        },
    };
    Ok(Some(Arc::new(term)))
}
