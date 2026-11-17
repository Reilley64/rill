use super::*;

mod admissions;
mod relations;
pub(super) use admissions::alternatives;
pub(super) use relations::{included, intersection};

fn terms<'a, 'g>(mut vector: &'a [Node<'g>]) -> impl Iterator<Item = &'a [Node<'g>]> {
    std::iter::from_fn(move || {
        let (term, rest) = vector.split_at(vector.first()?.size);
        vector = rest;
        Some(term)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Head<'g> {
    Variable(usize),
    Fixed(Constructor<'g>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Node<'g> {
    head: Head<'g>,
    children: usize,
    /// Number of consecutive prefix nodes in this subtree, including this node.
    size: usize,
    height: usize,
}

/// Canonical input vector. Indices are allocated by first occurrence, across
/// parameter boundaries, so equality preserves (and requires) correlation.
#[derive(Debug, Eq, PartialEq)]
pub struct InputKey<'g> {
    nodes: Vec<Node<'g>>,
}

impl<'g> InputKey<'g> {
    pub(super) fn parameter_owner(
        &self,
        module: &ModuleKey,
        budget: &mut Budget,
    ) -> Work<Option<&'g DeclarationId>> {
        for node in &self.nodes {
            budget.step(1)?;
            if let Head::Fixed(Constructor::Nominal(id) | Constructor::Variant(id)) = node.head
                && id.module == *module
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    pub(super) fn same(&self, other: &Self, budget: &mut Budget) -> Work<bool> {
        budget.step(
            self.nodes
                .len()
                .min(other.nodes.len())
                .checked_add(1)
                .ok_or(())?,
        )?;
        Ok(self == other)
    }
}

pub(super) fn ground<'a, 'g>(
    types: impl Iterator<Item = &'a Type<'g>>,
    budget: &mut Budget,
) -> Work<InputKey<'g>>
where
    'g: 'a,
{
    flatten(types, |constructor, _| Ok(Head::Fixed(constructor)), budget)
}

pub(super) fn first_match<'g>(
    body: &ImplementationFacts<'g>,
    actual: &InputKey<'g>,
    explicit: &[Option<InputKey<'g>>],
    budget: &mut Budget,
) -> Work<Option<usize>> {
    let mut equations = Vec::new();
    if let Some(substitution) = &body.substitution {
        for (argument, solved) in explicit.iter().zip(substitution) {
            budget.step(1)?;
            if let Some(argument) = argument {
                let left = flatten(
                    std::iter::once(&solved.argument),
                    |constructor, _| {
                        Ok(match constructor {
                        Constructor::Parameter(_, position) => Head::Variable(body.input_variables[position]
                            .expect("family input positions only refer to input-occurring implementation binders")),
                        _ => Head::Fixed(constructor),
                    })
                    },
                    budget,
                )?;
                budget.reserve(1)?;
                equations.push((left, argument));
            }
        }
    }
    for (index, alternative) in body.alternatives.iter().enumerate() {
        budget.step(1)?;
        let mut bindings = BTreeMap::new();
        if !relations::match_vector(&alternative.nodes, &actual.nodes, &mut bindings, budget)? {
            continue;
        }
        let mut agrees = true;
        for (left, right) in &equations {
            budget.step(1)?;
            if !relations::match_vector(&left.nodes, &right.nodes, &mut bindings, budget)? {
                agrees = false;
                break;
            }
        }
        if agrees {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

enum Visit<'a, 'g> {
    Enter(&'a Type<'g>, usize),
    Leave(usize),
}

fn flatten<'a, 'g>(
    types: impl Iterator<Item = &'a Type<'g>>,
    mut head_for: impl FnMut(Constructor<'g>, &mut Budget) -> Work<Head<'g>>,
    budget: &mut Budget,
) -> Work<InputKey<'g>>
where
    'g: 'a,
{
    let mut nodes = Vec::<Node<'g>>::new();
    let mut stack = Vec::new();
    for root in types {
        budget.reserve(1)?;
        stack.push(Visit::Enter(root, 1));
        while let Some(visit) = stack.pop() {
            budget.step(1)?;
            match visit {
                Visit::Enter(ty, depth) => {
                    if depth > budget.limits.max_depth.min(64) {
                        return Err(());
                    }
                    budget.statistics.max_depth = budget.statistics.max_depth.max(depth);
                    let (constructor, children) = shape(ty);
                    let head = head_for(constructor, budget)?;
                    budget.reserve(1)?;
                    let start = nodes.len();
                    nodes.push(Node {
                        head,
                        children: children.len(),
                        size: 1,
                        height: 1,
                    });
                    if children.len() != 0 {
                        budget.reserve(children.len().checked_add(1).ok_or(())?)?;
                        stack.push(Visit::Leave(start));
                        stack.extend(children.rev().map(|child| Visit::Enter(child, depth + 1)));
                    }
                }
                Visit::Leave(start) => {
                    let size = nodes.len() - start;
                    let mut height = 0;
                    for child in terms(&nodes[start + 1..]) {
                        budget.step(1)?;
                        height = height.max(child[0].height);
                    }
                    nodes[start].size = size;
                    nodes[start].height = height + 1;
                }
            }
        }
    }
    Ok(InputKey { nodes })
}

pub(super) fn input_key<'g>(
    header: &DeclarationFacts<'g>,
    budget: &mut Budget,
) -> Work<(InputKey<'g>, Vec<Option<usize>>)> {
    let DeclarationShape::Function { parameters, .. } = &header.shape else {
        unreachable!()
    };
    budget.reserve(header.binders.len())?;
    let mut indices = vec![None; header.binders.len()];
    let mut next = 0;
    let key = flatten(
        parameters.iter().map(|member| &member.ty),
        |constructor, _| {
            let Constructor::Parameter(declaration, position) = constructor else {
                return Ok(Head::Fixed(constructor));
            };
            debug_assert_eq!(&header.binders[position].declaration.id, declaration);
            let index = indices[position].get_or_insert_with(|| {
                let previous = next;
                next += 1;
                previous
            });
            Ok(Head::Variable(*index))
        },
        budget,
    )?;
    Ok((key, indices))
}
