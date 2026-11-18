use super::*;

/// Source access carried by a capture. A bare value source may itself carry
/// borrows. This does not say how the environment acquires the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureForm {
    Value,
    ExplicitBorrow { mutable: bool },
    CarriedReference { mutable: bool },
}

#[derive(Debug)]
pub struct CaptureFact<'g> {
    pub site: Site<'g>,
    pub path: data::PlacePath<'g>,
    pub ty: Type<'g>,
    pub form: CaptureForm,
    pub access: data::Access,
    pub demand: Option<InvocationMode>,
    pub reachable: bool,
    /// The inner arrow that needs this source when the capture is transitive.
    pub through: Option<NodeId>,
    /// Known underlying arrow origin of a captured callable value.
    pub value_origin: Option<Site<'g>>,
}

#[derive(Debug)]
pub struct ArrowFacts<'g> {
    pub site: Site<'g>,
    /// Conditional when its signature used an unadmitted ordinary contract.
    pub premise: Option<ordinary::InputPremise>,
    pub parameters: Vec<Type<'g>>,
    pub result: Type<'g>,
    pub mode: Option<InvocationMode>,
    pub captures: Vec<CaptureFact<'g>>,
}

#[derive(Clone, Copy)]
enum Demand {
    Read,
    Mutate,
    Value,
    Invoke(InvocationMode),
    Pending,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn capture_facts(
        &mut self,
        module: &'g Module,
        arrow: NodeId,
        first_fact: usize,
    ) -> Work<(Option<InvocationMode>, Vec<CaptureFact<'g>>)> {
        let span = &module.syntax.nodes[arrow].span;
        let body = module.syntax.nodes[arrow].children[1];
        let local = |path: &data::PlacePath<'g>| {
            path.root.start >= span.start() && path.root.start < span.end()
        };
        let mut facts = BTreeMap::new();
        self.elab.reserve(self.facts.nodes.len() - first_fact)?;
        for (i, fact) in self.facts.nodes.iter().enumerate().skip(first_fact) {
            facts.insert(fact.site.node, i);
        }
        let mut captures = Vec::new();
        self.elab.reserve(1)?;
        let mut stack = vec![(body, Demand::Value, true)];
        while let Some((node, use_, reachable)) = stack.pop() {
            self.elab.step(1)?;
            let syntax = &module.syntax.nodes[node];
            let children = &syntax.children;
            let node_fact = |node| facts.get(&node).map(|&i| &self.facts.nodes[i]);
            let expression = |node| {
                node_fact(node).and_then(|fact| match &fact.kind {
                    NodeFactKind::Expression(expr) => Some(expr),
                    _ => None,
                })
            };
            if syntax.kind == NodeKind::Arrow {
                if let Some(inner) = self.facts.arrows.get(&(&module.key, node)) {
                    for capture in &inner.captures {
                        self.elab.step(1)?;
                        if !local(&capture.path) {
                            let demand = match capture.form {
                                CaptureForm::ExplicitBorrow { mutable: true } => {
                                    Some(InvocationMode::Mutable)
                                }
                                CaptureForm::ExplicitBorrow { mutable: false } => {
                                    Some(InvocationMode::ReadOnly)
                                }
                                _ => {
                                    Self::capture_demand(Demand::Value, &capture.ty, capture.access)
                                }
                            };
                            self.elab.reserve(1 + capture.path.projections.len())?;
                            captures.push(CaptureFact {
                                site: capture.site,
                                path: capture.path.clone(),
                                ty: capture.ty.clone(),
                                form: capture.form,
                                access: capture.access,
                                demand,
                                reachable: reachable && capture.reachable,
                                through: Some(node),
                                value_origin: capture.value_origin,
                            });
                        }
                    }
                }
                continue;
            }
            let explicit = match syntax.kind {
                NodeKind::Prefix("&") => Some(false),
                NodeKind::Prefix("&mut") => Some(true),
                _ => None,
            };
            let place = explicit
                .and_then(|_| expression(children[0]))
                .or_else(|| expression(node));
            if let Some(place) = place
                && let Some(path) = &place.path
            {
                if !local(path) {
                    let form = if let Some(mutable) = explicit {
                        CaptureForm::ExplicitBorrow { mutable }
                    } else if let TypeTerm::Reference { mutable, .. } = place.ty.as_ref() {
                        CaptureForm::CarriedReference { mutable: *mutable }
                    } else if place.access != data::Access::Owned {
                        CaptureForm::CarriedReference {
                            mutable: place.access == data::Access::Writable,
                        }
                    } else {
                        CaptureForm::Value
                    };
                    let demand = explicit.map_or_else(
                        || Self::capture_demand(use_, &place.ty, place.access),
                        |mutable| {
                            Some(if mutable {
                                InvocationMode::Mutable
                            } else {
                                InvocationMode::ReadOnly
                            })
                        },
                    );
                    self.elab.reserve(1 + path.projections.len())?;
                    captures.push(CaptureFact {
                        site: Self::site(module, node),
                        path: path.clone(),
                        ty: expression(node).unwrap_or(place).ty.clone(),
                        form,
                        access: explicit.map_or(place.access, |mutable| {
                            if mutable {
                                data::Access::Writable
                            } else {
                                data::Access::ReadOnly
                            }
                        }),
                        demand,
                        reachable,
                        through: None,
                        value_origin: node_fact(if explicit.is_some() {
                            children[0]
                        } else {
                            node
                        })
                        .and_then(|fact| fact.callable.as_ref()?.arrow.as_ref())
                        .map(|a| a.site),
                    });
                }
                // The typed stable path already accounts for its entire base.
                continue;
            }
            let normal = |id| {
                facts
                    .get(&id)
                    .is_none_or(|&i| self.facts.nodes[i].flow.normal)
            };
            let truth = |id| {
                if normal(id) {
                    expression(id)
                        .and_then(|e| e.boolean)
                        .unwrap_or(BooleanSet::BOTH)
                } else {
                    BooleanSet::EMPTY
                }
            };
            self.elab.reserve(children.len())?;
            match syntax.kind {
                NodeKind::IfExpression
                | NodeKind::IfStatement
                | NodeKind::While
                | NodeKind::Guard => {
                    let condition = truth(children[0]);
                    stack.push((children[0], Demand::Read, reachable));
                    if let Some(&yes) = children.get(1) {
                        stack.push((
                            yes,
                            use_,
                            reachable
                                && if syntax.kind == NodeKind::Guard {
                                    condition.can_be_false()
                                } else {
                                    condition.can_be_true()
                                },
                        ));
                    }
                    if let Some(&no) = children.get(2) {
                        stack.push((no, use_, reachable && condition.can_be_false()));
                    }
                }
                NodeKind::Binary(op) => {
                    stack.push((children[0], Demand::Read, reachable));
                    let right = match op {
                        "&&" => truth(children[0]).can_be_true(),
                        "||" => truth(children[0]).can_be_false(),
                        _ => normal(children[0]),
                    };
                    stack.push((children[1], Demand::Read, reachable && right));
                }
                NodeKind::Assignment(_) => {
                    stack.push((children[0], Demand::Mutate, reachable));
                    stack.push((children[1], Demand::Value, reachable && normal(children[0])));
                }
                NodeKind::Binding { .. } => {
                    stack.push((*children.last().unwrap(), Demand::Value, reachable));
                }
                NodeKind::Prefix("*") => {
                    // A computed reference is not an acquisition path. Its
                    // source expressions still run inside the arrow.
                    stack.push((children[0], Demand::Read, reachable));
                }
                NodeKind::Prefix("&" | "&mut") => {
                    stack.push((children[0], Demand::Pending, reachable));
                }
                NodeKind::Prefix(_) => {
                    stack.push((children[0], Demand::Read, reachable));
                }
                NodeKind::Call => {
                    let invocation = node_fact(node)
                        .and_then(|fact| fact.callable.as_ref()?.invocation.as_ref());
                    let mut active = reachable;
                    if let Some(invocation) = invocation {
                        stack.push((invocation.receiver, Demand::Invoke(invocation.mode), active));
                        active &= normal(invocation.receiver);
                    }
                    for &argument in &children[1..] {
                        stack.push((argument, Demand::Value, active));
                        active &= normal(argument);
                    }
                }
                NodeKind::Match => {
                    let Some(data::DataExpression::Match(matched)) =
                        expression(node).and_then(|e| e.data.as_ref())
                    else {
                        continue;
                    };
                    self.elab.step(matched.arms.len())?;
                    let needs_classification = matched
                        .arms
                        .iter()
                        .any(|arm| arm.body_reachable && !arm.pattern.binders.is_empty());
                    stack.push((
                        matched.scrutinee,
                        if needs_classification {
                            Demand::Pending
                        } else {
                            Demand::Read
                        },
                        reachable,
                    ));
                    self.elab.reserve(matched.arms.len() * 2)?;
                    for arm in &matched.arms {
                        if let Some(guard) = arm.guard {
                            stack.push((guard, Demand::Read, reachable && arm.guard_reachable));
                        }
                        stack.push((arm.body, use_, reachable && arm.body_reachable));
                    }
                }
                _ => {
                    let mut active = reachable;
                    for &child in children {
                        stack.push((child, use_, active));
                        active &= normal(child);
                    }
                }
            }
        }
        let mut mode = Some(InvocationMode::ReadOnly);
        self.elab.step(captures.len())?;
        for capture in captures.iter().filter(|c| c.reachable) {
            mode = match (mode, capture.demand) {
                (Some(a), Some(b)) => Some(if Self::mode_admits(a, b) { b } else { a }),
                _ => None,
            };
        }
        Ok((mode, captures))
    }

    fn capture_demand(use_: Demand, ty: &Type<'g>, access: data::Access) -> Option<InvocationMode> {
        match use_ {
            Demand::Read => Some(InvocationMode::ReadOnly),
            Demand::Mutate => Some(InvocationMode::Mutable),
            Demand::Invoke(mode) => Some(mode),
            Demand::Pending => None,
            Demand::Value => match implicit_copy_leaf(ty) {
                Some(true) => Some(InvocationMode::ReadOnly),
                Some(false) if access == data::Access::Owned => Some(InvocationMode::Once),
                _ => None,
            },
        }
    }
}
