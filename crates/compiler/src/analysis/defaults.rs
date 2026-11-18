//! Elaboration of written nominal defaults in the defining module.
use super::*;

pub(super) struct ActiveDefault<'g> {
    binder: &'g DeclarationId,
    binders: Vec<Binder<'g>>,
    arguments: Vec<Option<Type<'g>>>,
}

impl<'g> Analyzer<'g> {
    pub(super) fn written_default(
        &self,
        module: &'g Module,
        binder: &Binder<'g>,
    ) -> Option<DefaultArgumentUse<'g>> {
        let node = *module.syntax.nodes[binder.declaration.node]
            .children
            .get(1)?;
        Some(DefaultArgumentUse {
            binder: &binder.declaration.id,
            node,
            span: &module.syntax.nodes[node].span,
        })
    }

    pub(super) fn hidden_default_binder(&mut self, name: &str) -> Work<bool> {
        let Some(active) = self.active_defaults.last() else {
            return Ok(false);
        };
        let hidden = &active.binders[active.arguments.len()..];
        let count = hidden.len();
        let matched = hidden
            .iter()
            .any(|b| b.declaration.name.as_deref() == Some(name));
        self.step(count)?;
        Ok(matched)
    }

    pub(super) fn default_argument(
        &self,
        binder: &DeclarationId,
        position: usize,
    ) -> Option<Type<'g>> {
        let active = self.active_defaults.last()?;
        (active.binders.get(position)?.declaration.id == *binder)
            .then(|| active.arguments.get(position)?.clone())
            .flatten()
    }

    pub(super) fn elaborate_default(
        &mut self,
        module: &'g Module,
        binders: &[Binder<'g>],
        default: &DefaultArgumentUse<'g>,
        arguments: &[Option<Type<'g>>],
        depth: usize,
    ) -> Work<Option<Type<'g>>> {
        // Compare applications, not field definitions or declaration names alone.
        for i in 0..self.active_defaults.len() {
            self.step(1)?;
            if *self.active_defaults[i].binder != *default.binder {
                continue;
            }
            let mut same = true;
            for (position, actual) in arguments.iter().enumerate() {
                self.step(1)?;
                let other = self.active_defaults[i].arguments[position].clone();
                same &= match (actual, &other) {
                    (Some(a), Some(b)) => self.default_equality(a, b, 1)?,
                    (None, None) => true,
                    _ => false,
                };
                if !same {
                    break;
                }
            }
            if same {
                self.unsupported(module, default.node, Unsupported::RecursiveDefault)?;
                return Ok(None);
            }
        }
        self.reserve(1 + binders.len() + arguments.len())?;
        self.active_defaults.push(ActiveDefault {
            binder: default.binder,
            binders: binders.to_vec(),
            arguments: arguments.to_vec(),
        });
        let result = self.ty(module, default.node, &binders[..arguments.len()], depth);
        self.active_defaults.pop();
        result
    }

    fn default_equality(&mut self, a: &Type<'g>, b: &Type<'g>, depth: usize) -> Work<bool> {
        self.step(1)?;
        if depth > self.limits.max_depth.min(64) {
            return Err(());
        }
        self.statistics.max_depth = self.statistics.max_depth.max(depth);
        if Arc::ptr_eq(a, b) {
            return Ok(true);
        }
        let (ah, ac) = ordinary::shape(a);
        let (bh, bc) = ordinary::shape(b);
        if ah != bh || ac.len() != bc.len() {
            return Ok(false);
        }
        for (a, b) in ac.zip(bc) {
            if !self.default_equality(a, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn default_dependencies_ready(
        &mut self,
        module: &'g Module,
        node: NodeId,
        earlier: &[Binder<'g>],
        arguments: &[Option<Type<'g>>],
    ) -> Work<bool> {
        self.reserve(1)?;
        let mut stack = vec![node];
        while let Some(node) = stack.pop() {
            self.step(1)?;
            let n = &module.syntax.nodes[node];
            if n.kind == NodeKind::NamedType {
                let first = module.syntax.nodes[n.children[0]].children[0];
                let spelling = module.syntax.nodes[first].span.bytes();
                self.step(earlier.len())?;
                if earlier.iter().zip(arguments).any(|(b, value)| {
                    value.is_none()
                        && b.declaration
                            .name
                            .as_ref()
                            .is_some_and(|s| s.as_bytes() == spelling)
                }) {
                    return Ok(false);
                }
            }
            self.reserve(n.children.len())?;
            stack.extend(&n.children);
        }
        Ok(true)
    }

    pub(super) fn validate_nominal_defaults(
        &mut self,
        module: &'g Module,
        binders: &[Binder<'g>],
    ) -> Work<()> {
        if !self.complete_defaults {
            return Ok(());
        }
        let mut saw_default = false;
        for binder in binders {
            self.step(1)?;
            if let Some(default) = self.written_default(module, binder) {
                saw_default = true;
                self.reserve(1)?;
                let mut stack = vec![default.node];
                while let Some(node) = stack.pop() {
                    self.step(1)?;
                    let n = &module.syntax.nodes[node];
                    if n.kind == NodeKind::NamedType {
                        let first = module.syntax.nodes[n.children[0]].children[0];
                        let spelling = module.syntax.nodes[first].span.bytes();
                        let hidden = &binders[binder.position..];
                        self.step(hidden.len())?;
                        if hidden.iter().any(|b| {
                            b.declaration
                                .name
                                .as_ref()
                                .is_some_and(|s| s.as_bytes() == spelling)
                        }) {
                            self.finding(module, node, FindingKind::InvalidDefaultDependency)?;
                        }
                    }
                    self.reserve(n.children.len())?;
                    stack.extend(&n.children);
                }
            } else if saw_default {
                self.finding(
                    module,
                    binder.declaration.node,
                    FindingKind::NonTrailingDefault,
                )?;
            }
        }
        Ok(())
    }
}
