use super::*;

#[derive(Clone, Copy)]
enum Admission<'g> {
    Exact,
    Variant {
        id: &'g DeclarationId,
        borrowed: bool,
    },
    Mode(InvocationMode),
}

impl<'g> Admission<'g> {
    fn append(self, term: &[Node<'g>], out: &mut Vec<Node<'g>>, budget: &mut Budget) -> Work<()> {
        let extra = usize::from(matches!(self, Self::Variant { .. }));
        budget.reserve(term.len().checked_add(extra).ok_or(())?)?;
        let mut root = term[0];
        match self {
            Self::Exact => out.extend_from_slice(term),
            Self::Variant { id, borrowed } => {
                let height = root.height + 1;
                if height > budget.limits.max_depth.min(64) {
                    return Err(());
                }
                budget.statistics.max_depth = budget.statistics.max_depth.max(height);
                let parent = if borrowed {
                    root.size += 1;
                    root.height = height;
                    out.push(root);
                    &term[1..]
                } else {
                    term
                };
                out.push(Node {
                    head: Head::Fixed(Constructor::Variant(id)),
                    children: 1,
                    size: parent.len() + 1,
                    height: parent[0].height + 1,
                });
                out.extend_from_slice(parent);
            }
            Self::Mode(mode) => {
                let Head::Fixed(Constructor::Callable(_, arity)) = root.head else {
                    unreachable!()
                };
                root.head = Head::Fixed(Constructor::Callable(mode, arity));
                out.push(root);
                out.extend_from_slice(&term[1..]);
            }
        }
        Ok(())
    }
}

pub(in crate::analysis::ordinary) fn alternatives<'g>(
    key: &InputKey<'g>,
    types: &PartialFacts<'g>,
    budget: &mut Budget,
) -> Work<Option<Vec<InputKey<'g>>>> {
    let mut dimensions = Vec::new();
    for term in terms(&key.nodes) {
        budget.reserve(2)?;
        let mut choices = vec![Admission::Exact];
        let nominal = match term[0].head {
            Head::Fixed(Constructor::Nominal(id)) => Some((id, false)),
            Head::Fixed(Constructor::Reference(false)) => match term[1].head {
                Head::Fixed(Constructor::Nominal(id)) => Some((id, true)),
                _ => None,
            },
            Head::Fixed(Constructor::Callable(mode, _)) => {
                let weaker = match mode {
                    InvocationMode::ReadOnly => &[][..],
                    InvocationMode::Mutable => &[InvocationMode::ReadOnly],
                    InvocationMode::Once => &[InvocationMode::ReadOnly, InvocationMode::Mutable],
                };
                budget.reserve(weaker.len())?;
                choices.extend(weaker.iter().copied().map(Admission::Mode));
                None
            }
            _ => None,
        };
        if let Some((id, borrowed)) = nominal
            && id.kind == DeclarationKind::Enum
        {
            let Some(DeclarationFacts {
                shape: DeclarationShape::Enum { variants },
                ..
            }) = types.declarations.get(id)
            else {
                return Ok(None);
            };
            budget.reserve(variants.len())?;
            choices.extend(variants.iter().map(|v| Admission::Variant {
                id: &v.declaration.id,
                borrowed,
            }));
        }
        dimensions.push((term, choices));
    }
    budget.reserve(dimensions.len())?;
    let mut indices = vec![0; dimensions.len()];
    let mut product = Vec::new();
    loop {
        budget.reserve(1)?;
        let mut nodes = Vec::new();
        for ((term, choices), index) in dimensions.iter().zip(&indices) {
            choices[*index].append(term, &mut nodes, budget)?;
        }
        product.push(InputKey { nodes });
        let mut carry = true;
        for ((_, choices), index) in dimensions.iter().zip(&mut indices).rev() {
            budget.step(1)?;
            *index += 1;
            if *index < choices.len() {
                carry = false;
                break;
            }
            *index = 0;
        }
        if carry {
            return Ok(Some(product));
        }
    }
}
