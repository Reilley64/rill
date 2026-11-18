use super::*;

pub(super) type Captures<'a, 'g> = BTreeMap<usize, &'a [Node<'g>]>;

pub(super) fn match_vector<'a, 'g>(
    pattern: &[Node<'g>],
    actual: &'a [Node<'g>],
    env: &mut Captures<'a, 'g>,
    budget: &mut Budget,
) -> Work<bool> {
    let (mut p, mut a) = (0, 0);
    // The actual side's variables are rigid tokens. Only the pattern side binds.
    while let (Some(pn), Some(an)) = (pattern.get(p), actual.get(a)) {
        budget.step(1)?;
        match pn.head {
            Head::Variable(index) => {
                let term = &actual[a..a + an.size];
                if let Some(previous) = env.get(&index) {
                    budget.step(an.size)?;
                    if *previous != term {
                        return Ok(false);
                    }
                } else {
                    budget.reserve(1)?;
                    env.insert(index, term);
                }
                a += an.size;
            }
            Head::Fixed(_) if pn.head == an.head && pn.children == an.children => a += 1,
            Head::Fixed(_) => return Ok(false),
        }
        p += 1;
    }
    Ok(p == pattern.len() && a == actual.len())
}

pub(in crate::analysis) fn included(
    left: &[InputKey<'_>],
    right: &[InputKey<'_>],
    budget: &mut Budget,
) -> Work<bool> {
    for a in left {
        budget.step(1)?;
        let mut covered = false;
        for b in right {
            budget.step(1)?;
            if match_vector(&b.nodes, &a.nodes, &mut BTreeMap::new(), budget)? {
                covered = true;
                break;
            }
        }
        if !covered {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Clone, Copy)]
pub(super) struct Site<'a, 'g> {
    pub(super) side: bool,
    pub(super) term: &'a [Node<'g>],
}

impl Site<'_, '_> {
    fn variable(self) -> Option<(bool, usize)> {
        match self.term[0].head {
            Head::Variable(v) => Some((self.side, v)),
            Head::Fixed(_) => None,
        }
    }
    pub(super) fn children(self) -> impl Iterator<Item = Self> {
        terms(&self.term[1..]).map(move |term| Self {
            side: self.side,
            term,
        })
    }
}

pub(super) type Solution<'a, 'g> = BTreeMap<(bool, usize), Site<'a, 'g>>;

pub(super) fn chase<'a, 'g>(
    mut site: Site<'a, 'g>,
    solved: &BTreeMap<(bool, usize), Site<'a, 'g>>,
    budget: &mut Budget,
) -> Work<Site<'a, 'g>> {
    while let Some(next) = site.variable().and_then(|id| solved.get(&id)) {
        budget.step(1)?;
        site = *next;
    }
    Ok(site)
}

fn occurs<'a, 'g>(
    needle: (bool, usize),
    root: Site<'a, 'g>,
    solved: &BTreeMap<(bool, usize), Site<'a, 'g>>,
    budget: &mut Budget,
) -> Work<bool> {
    let mut pending = Vec::new();
    budget.reserve(1)?;
    pending.push(root);
    while let Some(site) = pending.pop() {
        budget.step(1)?;
        let site = chase(site, solved, budget)?;
        if site.variable() == Some(needle) {
            return Ok(true);
        }
        budget.reserve(site.term[0].children)?;
        pending.extend(site.children());
    }
    Ok(false)
}

pub(super) fn unify<'a, 'g>(
    left: &'a InputKey<'g>,
    right: &'a InputKey<'g>,
    budget: &mut Budget,
) -> Work<Option<Solution<'a, 'g>>> {
    let mut pending = Vec::new();
    let mut a = terms(&left.nodes);
    let mut b = terms(&right.nodes);
    loop {
        budget.step(1)?;
        match (a.next(), b.next()) {
            (None, None) => break,
            (Some(a), Some(b)) => {
                budget.reserve(1)?;
                pending.push((
                    Site {
                        side: false,
                        term: a,
                    },
                    Site {
                        side: true,
                        term: b,
                    },
                ));
            }
            _ => return Ok(None),
        }
    }
    let mut solved = BTreeMap::new();
    while let Some((left, right)) = pending.pop() {
        budget.step(1)?;
        let mut a = chase(left, &solved, budget)?;
        let mut b = chase(right, &solved, budget)?;
        if b.variable().is_some() && (a.variable().is_none() || (a.side && !b.side)) {
            std::mem::swap(&mut a, &mut b);
        }
        if let Some(variable) = a.variable() {
            if b.variable() == Some(variable) {
                continue;
            }
            if occurs(variable, b, &solved, budget)? {
                return Ok(None);
            }
            budget.reserve(1)?;
            solved.insert(variable, b);
        } else {
            let (a_node, b_node) = (a.term[0], b.term[0]);
            if a_node.head != b_node.head || a_node.children != b_node.children {
                return Ok(None);
            }
            budget.reserve(a_node.children)?;
            pending.extend(a.children().zip(b.children()));
        }
    }
    Ok(Some(solved))
}

pub(in crate::analysis) fn intersection<'g>(
    left: &[InputKey<'g>],
    right: &[InputKey<'g>],
    budget: &mut Budget,
) -> Work<Option<(usize, usize)>> {
    for (i, a) in left.iter().enumerate() {
        budget.step(1)?;
        for (j, b) in right.iter().enumerate() {
            if unify(a, b, budget)?.is_some() {
                return Ok(Some((i, j)));
            }
        }
    }
    Ok(None)
}
