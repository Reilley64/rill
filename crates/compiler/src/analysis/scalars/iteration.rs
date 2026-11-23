//! Iteration through the same selection and evidence engine as written calls.
use super::*;
use crate::analysis::typed::{CollectionOperation, ListTraversal};

/// Checked handoff at the original `for ... of` node. Goal queries retain the
/// selected bodies and exact substitutions; this is not an ownership judgment.
#[derive(Debug)]
pub struct Iteration<'g> {
    pub source: NodeId,
    /// The typed List literal under transparent groups, not a computed List.
    pub(in crate::analysis) list_literal: Option<NodeId>,
    pub pattern: NodeId,
    pub iterate: generics::GoalId,
    pub next: generics::GoalId,
    pub iterator: Type<'g>,
    pub yielded: Type<'g>,
    pub some: &'g DeclarationId,
    pub none: &'g DeclarationId,
    /// No storage handoff is implied by a user iterator's checked signatures.
    pub traversal: Option<ListTraversal>,
    /// Checked shape of the written body, not ownership of its operands.
    pub(in crate::analysis) body: Option<IterationBodyKind>,
    pub(super) text_input: Option<&'g DeclarationId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum IterationBodyKind {
    Ordinary,
    NamedBoolPostfix,
    YieldedTextPostfix,
}

#[derive(Clone, Copy)]
pub(super) enum DirectExpression {
    BindingOrStatement(NodeId, usize),
    Postfix {
        root: NodeId,
        operand: NodeId,
        depth: usize,
        named: bool,
    },
    Return(NodeId, usize),
}

#[derive(Clone, Copy, Default)]
pub(super) enum IterationBody<'g> {
    #[default]
    Unsupported,
    Ordinary {
        option_text_item: bool,
        /// An owned yield of an application record; field reads are ordinary.
        record_item: bool,
    },
    TextOption {
        item: Option<&'g DeclarationId>,
        direct: Option<DirectExpression>,
        constructor: bool,
        postfix: bool,
    },
}

impl IterationBody<'_> {
    fn finish(self, reached_postfix: bool) -> Option<IterationBodyKind> {
        match self {
            Self::TextOption {
                item,
                postfix: true,
                ..
            } if reached_postfix => Some(if item.is_some() {
                IterationBodyKind::YieldedTextPostfix
            } else {
                IterationBodyKind::NamedBoolPostfix
            }),
            Self::Ordinary { .. }
            | Self::TextOption {
                constructor: false,
                postfix: false,
                ..
            } => Some(IterationBodyKind::Ordinary),
            _ => None,
        }
    }

    // Follow only the direct statement result and its postfix operand during
    // the existing top-down expression visit. Groups add depth, not new uses.
    pub(super) fn visit(&mut self, node: NodeId, syntax: &crate::parser::Node) {
        let Self::TextOption {
            direct: Some(d), ..
        } = self
        else {
            return;
        };
        use DirectExpression::*;
        match (*d, syntax.kind) {
            (BindingOrStatement(root, depth), NodeKind::Group) if root == node => {
                *d = BindingOrStatement(syntax.children[0], depth + 1)
            }
            (Return(root, depth), NodeKind::Group) if root == node => {
                *d = Return(syntax.children[0], depth + 1)
            }
            (BindingOrStatement(root, depth), NodeKind::Propagate) if root == node => {
                *d = Postfix {
                    root,
                    operand: syntax.children[0],
                    depth: depth + 1,
                    named: false,
                }
            }
            (
                Postfix {
                    root,
                    operand,
                    depth,
                    ..
                },
                NodeKind::Group,
            ) if operand == node => {
                *d = Postfix {
                    root,
                    operand: syntax.children[0],
                    depth: depth + 1,
                    named: false,
                }
            }
            _ => {}
        }
    }

    pub(super) fn statement(&mut self, node: &crate::parser::Node, depth: usize) {
        if !matches!(
            node.kind,
            NodeKind::Block
                | NodeKind::Binding { .. }
                | NodeKind::Return
                | NodeKind::Break
                | NodeKind::Continue
                | NodeKind::IfStatement
                | NodeKind::ExpressionStatement
                | NodeKind::Assignment("=")
        ) {
            *self = Self::Unsupported;
        } else if let Self::TextOption { direct, .. } = self {
            *direct = match node.kind {
                NodeKind::Binding { .. } | NodeKind::ExpressionStatement => node
                    .children
                    .last()
                    .copied()
                    .map(|root| DirectExpression::BindingOrStatement(root, depth + 1)),
                NodeKind::Return => node
                    .children
                    .first()
                    .copied()
                    .map(|root| DirectExpression::Return(root, depth + 1)),
                _ => None,
            };
        }
    }
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn iteration_statement(
        &mut self,
        module: &'g Module,
        node: NodeId,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<(Flow, Option<Iteration<'g>>)> {
        let &[pattern, operand, block] = module.syntax.nodes[node].children.as_slice() else {
            unreachable!("parsed for-of")
        };
        let before = self.marks();
        let source = self.expression(module, operand, None, context)?;
        let mut handoff = if let Some(source) = &source
            && before == self.marks()
            && primitive_name(&source.ty) != Some("never")
        {
            self.iteration_handoff(module, node, source.ty.clone())?
        } else {
            None
        };
        self.elab.reserve(2)?;
        context.scopes.push(BTreeMap::new());
        context.loops.push(node);
        if let Some(handoff) = &handoff {
            self.iteration_pattern(module, handoff, depth + 1, context)?;
        } else {
            if before == self.marks() {
                self.defer(module, pattern, ScalarUnsupported::FamilySelection)?;
            }
            self.pending_pattern(module, pattern, context)?;
        }
        let shape = match (&handoff, &source) {
            (Some(iteration), Some(source))
                if iteration.traversal == Some(ListTraversal::Owned)
                    && primitive_name(&iteration.yielded) == Some("string")
                    && source.access == data::Access::Owned
                    && source.path.as_ref().is_some_and(|p| {
                        p.root.kind == DeclarationKind::Parameter && p.projections.is_empty()
                    })
                    && self
                        .propagation_target(context)
                        .is_some_and(|(target, result)| {
                            matches!(target, PropagationTarget::Callable(_))
                                && result.is_some_and(|ty| {
                                    Self::is_iteration_option(self.option, ty, "bool")
                                })
                        }) =>
            {
                IterationBody::TextOption {
                    item: None,
                    direct: None,
                    constructor: false,
                    postfix: false,
                }
            }
            (Some(iteration), Some(source))
                if iteration.traversal == Some(ListTraversal::Owned)
                    && Self::is_iteration_option(self.option, &iteration.yielded, "string")
                    && self
                        .propagation_target(context)
                        .is_some_and(|(target, result)| {
                            matches!(target, PropagationTarget::Callable(_))
                                && result.is_some_and(|ty| {
                                    Self::is_iteration_option(self.option, ty, "bool")
                                })
                        })
                    && source.access == data::Access::Owned
                    && source.path.as_ref().is_some_and(|p| {
                        p.root.kind == DeclarationKind::Parameter && p.projections.is_empty()
                    })
                    && let Some(NodeFacts {
                        kind: NodeFactKind::Pattern(pattern),
                        ..
                    }) = self.facts.nodes.last()
                    && let [binder] = pattern.binders.as_slice()
                    && !binder.mutable
                    && binder.projections.is_empty() =>
            {
                IterationBody::TextOption {
                    item: Some(binder.declaration),
                    direct: None,
                    constructor: false,
                    postfix: false,
                }
            }
            (Some(iteration), _) => IterationBody::Ordinary {
                option_text_item: iteration.traversal == Some(ListTraversal::Owned)
                    && Self::is_iteration_option(self.option, &iteration.yielded, "string"),
                record_item: iteration.traversal == Some(ListTraversal::Owned)
                    && matches!(iteration.yielded.as_ref(), TypeTerm::Nominal { declaration, arguments }
                        if declaration.kind == DeclarationKind::Record
                            && declaration.module.origin == crate::loader::Origin::Application
                            && arguments.is_empty()),
            },
            _ => IterationBody::Unsupported,
        };
        let enclosing = std::mem::replace(&mut self.iteration_body, shape);
        let body = self.statement(module, block, depth + 1, context);
        let shape = std::mem::replace(&mut self.iteration_body, enclosing);
        let mut body = body?;
        if let Some(iteration) = &mut handoff {
            iteration.body = shape.finish(body.reaches_postfix);
            iteration.list_literal = source.as_ref().and_then(|value| value.list_literal);
        }
        if context.scopes.last().is_some_and(|scope| scope.len() == 1)
            && let (Some(iteration), Some(value)) = (&mut handoff, &source)
            && iteration.body.is_some()
            && iteration.traversal == Some(ListTraversal::Owned)
            && primitive_name(&iteration.yielded) == Some("string")
            && value.access == data::Access::Owned
            && let Some(path) = value.path.as_ref()
            && path.root.kind == DeclarationKind::Parameter
            && path.projections.is_empty()
        {
            // This hint uses the same body classification as M5. Only the
            // enclosing block can establish that its prefix reaches the loop.
            iteration.text_input = Some(path.root);
        }
        context.loops.pop();
        context.scopes.pop();

        let mut flow = source.map_or_else(Flow::normal, |e| e.flow);
        if handoff.is_some() && flow.normal {
            body.breaks.remove(&node);
            body.continues.remove(&node);
            // The checked List traversal can end immediately (None), irrespective
            // of the body. The remaining body exits are possible Some edges.
            flow = self.union(flow, &body)?;
        }
        Ok((flow, handoff))
    }

    fn is_iteration_option(
        role: Option<crate::analysis::typed::OptionRole<'g>>,
        ty: &Type<'g>,
        primitive: &str,
    ) -> bool {
        let Some(role) = role else {
            return false;
        };
        let ty = match ty.as_ref() {
            TypeTerm::Variant {
                parent,
                declaration,
            } if std::ptr::eq(*declaration, &role.some.id)
                || std::ptr::eq(*declaration, &role.none.id) =>
            {
                parent
            }
            _ => ty,
        };
        matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
            if std::ptr::eq(*declaration, &role.declaration.id)
                && matches!(arguments.as_slice(), [payload] if primitive_name(payload) == Some(primitive)))
    }

    // Use the existing charged statement/expression visits. No second body walk
    // or recursive admission of expressions containing propagation is needed.
    pub(super) fn iteration_expression(
        &mut self,
        module: &Module,
        node: NodeId,
        slot: &Slot<'g>,
        data: Option<&data::DataExpression<'g>>,
        call: Option<&CallFacts<'g>>,
    ) -> Work<()> {
        if matches!(self.iteration_body, IterationBody::Unsupported) {
            return Ok(());
        }
        let syntax = &module.syntax.nodes;
        // The admitted view is of the extracted string, not the whole
        // Option carrier. Reject the written borrow before granting storage.
        if matches!(
            self.iteration_body,
            IterationBody::TextOption { item: Some(_), .. }
                | IterationBody::Ordinary {
                    option_text_item: true,
                    ..
                }
        ) && matches!(syntax[node].kind, NodeKind::Prefix("&" | "&mut"))
            && matches!(slot.ty.as_deref(), Some(TypeTerm::Reference { referent, .. })
                if Self::is_iteration_option(self.option, referent, "string"))
        {
            self.iteration_body = IterationBody::Unsupported;
            return Ok(());
        }
        if let IterationBody::TextOption {
            item,
            direct: Some(DirectExpression::Postfix { operand, named, .. }),
            ..
        } = &mut self.iteration_body
            && *operand == node
            && syntax[node].kind == NodeKind::ValueReference
            && slot.access == data::Access::Owned
            && slot.path.as_ref().is_some_and(|p| {
                p.projections.is_empty()
                    && match *item {
                        Some(item) => std::ptr::eq(item, p.root),
                        None => slot
                            .ty
                            .as_ref()
                            .is_some_and(|ty| Self::is_iteration_option(self.option, ty, "bool")),
                    }
            })
        {
            *named = true;
        }
        match syntax[node].kind {
            NodeKind::Literal
            | NodeKind::ValueReference
            | NodeKind::Group
            | NodeKind::Prefix("&" | "&mut" | "*") => return Ok(()),
            NodeKind::Field
                if matches!(
                    self.iteration_body,
                    IterationBody::Ordinary {
                        record_item: true,
                        ..
                    }
                ) =>
            {
                return Ok(());
            }
            NodeKind::Call
                if call.is_some_and(|call| call.callee == &self.input.graph.prelude().dispose) =>
            {
                return Ok(());
            }
            _ => (),
        }
        let IterationBody::TextOption {
            item,
            direct,
            mut constructor,
            mut postfix,
        } = self.iteration_body
        else {
            self.iteration_body = IterationBody::Unsupported;
            return Ok(());
        };
        let depth = match (direct, syntax[node].kind, data) {
            (
                Some(DirectExpression::Postfix {
                    root,
                    depth,
                    named: true,
                    ..
                }),
                NodeKind::Propagate,
                Some(data::DataExpression::Propagation(exit)),
            ) if root == node && exit.error.is_none() => {
                postfix = true;
                depth
            }
            (
                Some(DirectExpression::Return(root, depth)),
                NodeKind::Call,
                Some(data::DataExpression::Construction(value)),
            ) if root == node && Self::is_iteration_option(self.option, &value.ty, "bool") => {
                constructor = true;
                depth
            }
            _ => {
                self.iteration_body = IterationBody::Unsupported;
                return Ok(());
            }
        };
        if item.is_none() {
            if depth > self.elab.limits.max_depth.min(64) {
                return Err(());
            }
            self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        }
        self.iteration_body = IterationBody::TextOption {
            item,
            direct,
            constructor,
            postfix,
        };
        Ok(())
    }

    fn iteration_handoff(
        &mut self,
        module: &'g Module,
        node: NodeId,
        input: Type<'g>,
    ) -> Work<Option<Iteration<'g>>> {
        use generics::GoalState;
        let (Some(role), Some(option)) = (self.collections.iteration, self.option) else {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            return Ok(None);
        };
        let site = Self::site(module, node);
        let scope = self.collections.scope(&role.iterate.id, &[Some(&input)]);
        self.elab.reserve(1)?;
        let iterate = self.determine_source(site, &role.iterate.id, vec![input], &[], &[], None)?;
        let goal = self.evidence.get(&iterate).unwrap();
        let Some(output) = goal
            .output
            .as_ref()
            .filter(|_| goal.state == GoalState::Proved && goal.premise.is_none())
        else {
            self.defer(
                module,
                node,
                if scope.is_some() {
                    ScalarUnsupported::CoreProtection
                } else {
                    ScalarUnsupported::FamilySelection
                },
            )?;
            return Ok(None);
        };
        let iterator = output.result.clone();
        self.elab.reserve(2)?;
        let mutable_iterator = Arc::new(TypeTerm::Reference {
            mutable: true,
            referent: iterator.clone(),
        });
        let next_scope = self
            .collections
            .scope(&role.next.id, &[Some(&mutable_iterator)]);
        let next =
            self.determine_source(site, &role.next.id, vec![mutable_iterator], &[], &[], None)?;
        let goal = self.evidence.get(&next).unwrap();
        let Some(output) = goal
            .output
            .as_ref()
            .filter(|_| goal.state == GoalState::Proved && goal.premise.is_none())
        else {
            self.defer(
                module,
                node,
                if next_scope.is_some() {
                    ScalarUnsupported::CoreProtection
                } else {
                    ScalarUnsupported::FamilySelection
                },
            )?;
            return Ok(None);
        };
        let yielded = match output.result.as_ref() {
            TypeTerm::Nominal {
                declaration,
                arguments,
            } if **declaration == option.declaration.id => match arguments.as_slice() {
                [yielded] => Some(yielded),
                _ => None,
            },
            _ => None,
        };
        let Some(yielded) = yielded else {
            self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            return Ok(None);
        };
        let [pattern, source, _] = module.syntax.nodes[node].children.as_slice() else {
            unreachable!()
        };
        let traversal = match (scope, next_scope) {
            (Some(iterate), Some(next)) => match (iterate.operation, next.operation) {
                (CollectionOperation::Iterate(input), CollectionOperation::Next(output))
                    if input == output =>
                {
                    Some(input)
                }
                _ => None,
            },
            _ => None,
        };
        self.elab.reserve(2)?;
        Ok(Some(Iteration {
            source: *source,
            list_literal: None,
            pattern: *pattern,
            iterate,
            next,
            iterator,
            yielded: yielded.clone(),
            some: &option.some.id,
            none: &option.none.id,
            traversal,
            body: None,
            text_input: None,
        }))
    }
}
