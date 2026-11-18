use super::patterns::{Pattern, PatternInput};
use super::*;

#[derive(Clone, Debug)]
pub struct Arm<'g> {
    pub site: Site<'g>,
    pub pattern: PatternFacts<'g>,
    pub guard: Option<NodeId>,
    pub body: NodeId,
    pub views: Vec<Arc<Refinement<'g>>>,
    pub guard_reachable: bool,
    pub body_reachable: bool,
}
#[derive(Clone, Debug)]
pub struct Match<'g> {
    pub scrutinee: NodeId,
    pub arms: Vec<Arm<'g>>,
    pub coverage: Coverage<'g>,
}
impl<'r, 'g> Checker<'r, 'g> {
    fn arm_context(&mut self, context: &Context<'r, 'g>) -> Work<Context<'r, 'g>> {
        let size = context.scopes.iter().map(|s| s.len()).sum::<usize>()
            + context.scopes.len()
            + context.loops.len()
            + context.views.len()
            + 1;
        self.elab.reserve(size)?;
        let mut context = context.clone();
        context.scopes.push(BTreeMap::new());
        Ok(context)
    }
    // Collect declared result/primitive anchors without finalizing contextual literals.
    // Ordinary calls use their selected declared contract, never a desired output.
    fn nested_match_hint(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<Type<'g>>> {
        let key = (&module.key, node);
        self.elab
            .step(self.match_hints.len().checked_ilog2().unwrap_or(0) as usize + 1)?;
        if let Some(hint) = self.match_hints.get(&key) {
            return Ok(hint.clone());
        }
        self.data_depth += 1;
        if self.data_depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(self.data_depth);
        let hint = self.prepare_nested_hint(module, node, context);
        self.data_depth -= 1;
        let hint = hint?;
        self.elab.reserve(1)?;
        self.match_hints.insert(key, hint.clone());
        Ok(hint)
    }
    fn prepare_nested_hint(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<Type<'g>>> {
        let n = &module.syntax.nodes[node];
        // Scrutinee and block typing have no arm-result context. Check them once
        // and retain their results; do not retry finalized literals with new context.
        let input = self.expression(module, n.children[0], None, context)?;
        let mut hint = None;
        if let Some(value) = &input {
            for &arm in &n.children[1..] {
                let a = &module.syntax.nodes[arm];
                let mut scope = self.arm_context(context)?;
                let before = self.marks();
                let mut binders = Vec::new();
                let pattern = self.pattern(
                    module,
                    a.children[0],
                    PatternInput {
                        ty: &value.ty,
                        access: value.access,
                        mutable: false,
                        emit: false,
                    },
                    self.data_depth + 1,
                    &mut scope,
                    &mut binders,
                )?;
                let Some(pattern) = pattern else {
                    continue;
                };
                if before != self.marks() {
                    continue;
                }
                for view in self.pattern_views(module, n.children[0], value, pattern, &scope)? {
                    if view.path.is_some() {
                        self.elab.reserve(1)?;
                        scope.views.push(view);
                    }
                }
                let body = *a.children.last().unwrap();
                let ty = if module.syntax.nodes[body].kind == NodeKind::Block {
                    let flow = self.statement(module, body, self.data_depth + 1, &mut scope)?;
                    let ty = self.primitive(if flow.normal { "void" } else { "never" })?;
                    self.elab.reserve(1)?;
                    self.match_blocks.insert((&module.key, body), flow);
                    Some(ty)
                } else {
                    self.arm_hint(module, body, &scope)?
                };
                if let Some(ty) = ty {
                    hint = if let Some(previous) = hint {
                        self.join_match(module, node, &previous, &ty)?
                    } else {
                        Some(ty)
                    };
                }
            }
        }
        self.elab.reserve(1)?;
        self.match_inputs.insert((&module.key, node), input);
        Ok(hint)
    }
    fn arm_hint(
        &mut self,
        module: &'g Module,
        root: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<Type<'g>>> {
        struct Hint<'g> {
            ty: Option<Type<'g>>,
            path: Option<PlacePath<'g>>,
        }
        self.elab.reserve(1)?;
        let mut stack = vec![(root, false)];
        let mut hints = BTreeMap::<NodeId, Hint<'g>>::new();
        while let Some((node, done)) = stack.pop() {
            self.elab.step(1)?;
            let n = &module.syntax.nodes[node];
            let field =
                n.kind == NodeKind::Field && !self.enum_base(module, n.children[0], context)?;
            let structural = field
                || matches!(
                    n.kind,
                    NodeKind::Group
                        | NodeKind::Prefix(_)
                        | NodeKind::Binary(_)
                        | NodeKind::IfExpression
                );
            if !done && structural {
                let children = if field {
                    &n.children[..1]
                } else {
                    &n.children[..]
                };
                self.elab.reserve(1 + children.len())?;
                stack.push((node, true));
                stack.extend(children.iter().map(|&c| (c, false)));
                continue;
            }
            let mut path = None;
            let ty = match n.kind {
                NodeKind::Match => self.nested_match_hint(module, node, context)?,
                NodeKind::Literal => self.literal_type(module, node, &Slot::default())?,
                NodeKind::ValueReference => match self.value(module, node, context)? {
                    Some(Value::Local(local)) => {
                        path = Some(PlacePath {
                            root: local.declaration,
                            projections: Vec::new(),
                        });
                        local.ty
                    }
                    _ => None,
                },
                NodeKind::Construction => {
                    self.constructor_type(module, n.children[0], context, None, None)?
                }
                NodeKind::Call => {
                    let callee = &module.syntax.nodes[n.children[0]];
                    if callee.kind == NodeKind::Field
                        && self.enum_base(module, callee.children[0], context)?
                    {
                        self.constructor_type(module, n.children[0], context, None, None)?
                    } else if self.input.ordinary.is_some()
                        && self.value_callee(module, n.children[0], context)?
                    {
                        self.local_callable_result_hint(module, n.children[0], context)?
                    } else if let Some(call) = self.prepare_call(module, node, context)?
                        && !call.infer
                    {
                        let ty = call.result.clone();
                        self.elab.reserve(1)?;
                        self.prepared_calls.insert((&module.key, node), call);
                        Some(ty)
                    } else {
                        None
                    }
                }
                NodeKind::Field if !field => {
                    self.constructor_type(module, node, context, None, None)?
                }
                NodeKind::Field => {
                    let base = &hints[&n.children[0]];
                    path = self.copy_path(base.path.as_ref())?;
                    if let Some(mut ty) = base.ty.clone() {
                        if let TypeTerm::Reference { referent, .. } = ty.as_ref() {
                            path = self.project_path(path.as_ref(), PathProjection::Dereference)?;
                            ty = referent.clone();
                        }
                        let (ty, _) = self.refined(path.as_ref(), ty, context)?;
                        let parent = if let TypeTerm::Variant { parent, .. } = ty.as_ref() {
                            parent
                        } else {
                            &ty
                        };
                        let visible = match parent.as_ref() {
                            TypeTerm::Nominal { declaration, .. }
                                if !self.type_unavailable(&ty, 1)? =>
                            {
                                declaration.module == module.key
                                    || self.input.facts.declarations[declaration]
                                        .declaration
                                        .public
                            }
                            _ => false,
                        };
                        if visible && let Some(fields) = self.member_types(&ty)? {
                            self.elab.step(fields.len())?;
                            let spelling = module.syntax.nodes[n.children[1]].span.bytes();
                            if let Some((field, ty)) = fields.into_iter().find(|(f, _)| {
                                f.name.as_ref().is_some_and(|s| s.as_bytes() == spelling)
                                    && (f.public || f.id.module == module.key)
                            }) {
                                path = self.project_path(
                                    path.as_ref(),
                                    PathProjection::Field(&field.id),
                                )?;
                                Some(ty)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                NodeKind::Group => {
                    path = self.copy_path(hints[&n.children[0]].path.as_ref())?;
                    hints[&n.children[0]].ty.clone()
                }
                NodeKind::Prefix("*") => {
                    if let Some(ty) = &hints[&n.children[0]].ty
                        && let TypeTerm::Reference { referent, .. } = ty.as_ref()
                    {
                        path = self.project_path(
                            hints[&n.children[0]].path.as_ref(),
                            PathProjection::Dereference,
                        )?;
                        Some(referent.clone())
                    } else {
                        None
                    }
                }
                NodeKind::Prefix("!")
                | NodeKind::Binary("==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") => {
                    Some(self.primitive("bool")?)
                }
                NodeKind::Prefix("-" | "~") => hints[&n.children[0]].ty.clone().filter(numeric),
                NodeKind::Binary("<<" | ">>") => hints[&n.children[0]].ty.clone().filter(integer),
                NodeKind::Binary(_) => hints[&n.children[0]]
                    .ty
                    .clone()
                    .filter(numeric)
                    .or_else(|| hints[&n.children[1]].ty.clone().filter(numeric)),
                NodeKind::IfExpression => {
                    let a = hints[&n.children[1]].ty.clone();
                    let b = hints[&n.children[2]].ty.clone();
                    match (a, b) {
                        (Some(a), Some(b)) => self.join_match(module, node, &a, &b)?,
                        (a, b) => a.or(b),
                    }
                }
                NodeKind::Prefix("&" | "&mut") => {
                    if let Some(ty) = &hints[&n.children[0]].ty {
                        self.elab.reserve(1)?;
                        Some(Arc::new(TypeTerm::Reference {
                            mutable: n.kind == NodeKind::Prefix("&mut"),
                            referent: ty.clone(),
                        }))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let ty = if let Some(ty) = ty {
                Some(self.refined(path.as_ref(), ty, context)?.0)
            } else {
                None
            };
            self.elab.reserve(1)?;
            hints.insert(node, Hint { ty, path });
        }
        Ok(hints.remove(&root).and_then(|h| h.ty))
    }
    fn join_match(
        &mut self,
        module: &'g Module,
        node: NodeId,
        a: &Type<'g>,
        b: &Type<'g>,
    ) -> Work<Option<Type<'g>>> {
        if primitive_name(a) == Some("never") {
            return Ok(Some(b.clone()));
        }
        if primitive_name(b) == Some("never") || self.same(a, b)? {
            return Ok(Some(a.clone()));
        }
        if let Some(ty) = self.data_join(a, b)? {
            return Ok(Some(ty));
        }
        if self.deferred_relation(a, b)? || self.deferred_relation(b, a)? {
            self.defer(module, node, ScalarUnsupported::TypeRelation)?;
        } else {
            self.mismatch(module, node, b, a)?;
        }
        Ok(None)
    }
    pub(in crate::analysis::scalars) fn match_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        if !inference.data.contains_key(&node) && !inference.slots[&node].blocked {
            self.data_depth += 1;
            if self.data_depth > self.elab.limits.max_depth.min(64) {
                return Err(());
            }
            self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(self.data_depth);
            let prepared = self.check_match(
                inference.module,
                node,
                inference.slots[&node].expected.clone(),
                context,
            );
            self.data_depth -= 1;
            if let Some(prepared) = prepared? {
                self.elab.reserve(1)?;
                inference.data.insert(node, prepared);
            } else {
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
        }
        if let Some(prepared) = inference.data.get(&node) {
            let ty = prepared.ty.clone();
            self.establish(inference, node, ty, false)?;
        }
        Ok(())
    }
    fn arm_guard(module: &Module, arm: NodeId) -> Option<NodeId> {
        let children = &module.syntax.nodes[arm].children;
        (children.len() == 3).then(|| module.syntax.nodes[children[1]].children[0])
    }

    fn check_arm_without_flow(
        &mut self,
        module: &'g Module,
        arm: NodeId,
        expected: Option<Type<'g>>,
        scope: &mut Context<'r, 'g>,
    ) -> Work<()> {
        let children = &module.syntax.nodes[arm].children;
        if let Some(guard) = Self::arm_guard(module, arm) {
            let boolean = self.primitive("bool")?;
            self.expression(module, guard, Some(boolean), scope)?;
        }
        let body = *children.last().unwrap();
        if module.syntax.nodes[body].kind == NodeKind::Block {
            self.statement(module, body, self.data_depth + 1, scope)?;
        } else {
            let receiving_pending = expected.is_none();
            self.expression_with_receiving(module, body, expected, scope, receiving_pending)?;
        }
        Ok(())
    }

    fn check_match(
        &mut self,
        module: &'g Module,
        node: NodeId,
        expected: Option<Type<'g>>,
        context: &Context<'r, 'g>,
    ) -> Work<Option<PreparedData<'g>>> {
        let n = &module.syntax.nodes[node];
        let before = self.marks();
        let scrutinee = n.children[0];
        let input = if let Some(input) = self.match_inputs.remove(&(&module.key, node)) {
            input
        } else {
            self.expression(module, scrutinee, None, context)?
        };
        let Some(value) = input else {
            if self.integrated {
                self.defer(module, node, ScalarUnsupported::MatchCoverage)?;
                for &arm in &n.children[1..] {
                    self.elab.step(1)?;
                    let mut scope = self.arm_context(context)?;
                    self.block_pattern_names(
                        module,
                        module.syntax.nodes[arm].children[0],
                        scrutinee,
                        false,
                        true,
                        &mut scope,
                    )?;
                    self.check_arm_without_flow(module, arm, expected.clone(), &mut scope)?;
                }
            }
            return Ok(None);
        };
        let mut arms = Vec::new();
        let mut rows = Vec::new();
        let mut patterns_complete = true;
        let mut common = expected.clone();
        for &arm in &n.children[1..] {
            let a = &module.syntax.nodes[arm];
            let mut scope = self.arm_context(context)?;
            let mut binders = Vec::new();
            let start = self.marks();
            let pattern = self.pattern(
                module,
                a.children[0],
                PatternInput {
                    ty: &value.ty,
                    access: value.access,
                    mutable: false,
                    emit: true,
                },
                self.data_depth + 1,
                &mut scope,
                &mut binders,
            )?;
            let settled = start == self.marks();
            let views = if let Some(pattern) = &pattern {
                self.pattern_views(module, scrutinee, &value, pattern.clone(), &scope)?
            } else {
                Vec::new()
            };
            for view in &views {
                if view.path.is_some() {
                    self.elab.reserve(1)?;
                    scope.views.push(view.clone());
                }
            }
            let Some(pattern) = pattern.filter(|_| settled) else {
                patterns_complete = false;
                if self.integrated {
                    self.block_pattern_names(
                        module,
                        a.children[0],
                        a.children[0],
                        false,
                        true,
                        &mut scope,
                    )?;
                    self.check_arm_without_flow(module, arm, expected.clone(), &mut scope)?;
                }
                continue;
            };
            let guard = Self::arm_guard(module, arm);
            let Some(irrefutable) = self.coverage(module, arm, &value.ty, vec![pattern.clone()])?
            else {
                patterns_complete = false;
                self.check_arm_without_flow(module, arm, expected.clone(), &mut scope)?;
                continue;
            };
            if guard.is_none() {
                self.elab.reserve(1)?;
                rows.push(pattern.clone());
            }
            let body = *a.children.last().unwrap();
            if expected.is_none()
                && module.syntax.nodes[body].kind != NodeKind::Block
                && let Some(hint) = self.arm_hint(module, body, &scope)?
                && primitive_name(&hint) != Some("never")
            {
                let hint = if let TypeTerm::Variant { parent, .. } = hint.as_ref() {
                    parent.clone()
                } else {
                    hint
                };
                common = if let Some(previous) = common {
                    self.join_match(module, node, &previous, &hint)?
                } else {
                    Some(hint)
                };
            }
            self.elab.reserve(1)?;
            arms.push((
                Arm {
                    site: Self::site(module, arm),
                    pattern: PatternFacts {
                        source: Self::site(module, scrutinee),
                        access: value.access,
                        ty: value.ty.clone(),
                        binders,
                        coverage: irrefutable,
                    },
                    guard,
                    body,
                    views,
                    guard_reachable: false,
                    body_reachable: false,
                },
                scope,
                pattern,
            ));
        }
        let coverage = if patterns_complete {
            self.coverage(module, node, &value.ty, rows)?
        } else {
            None
        };
        if let Some(coverage) = &coverage
            && !coverage.exhaustive()
        {
            self.finding(module, node, ScalarFindingKind::NonExhaustive)?;
            self.retain_coverage(module, node, &value.ty, coverage)?;
        }
        let mut facts = Vec::new();
        let mut flow = Flow::default();
        let mut result = None;
        let mut handled = Vec::new();
        for (mut arm, mut scope, pattern) in arms {
            let mut restriction = pattern.clone();
            let mut possible = value.normal;
            if let Some(boolean) = value.boolean {
                match pattern.as_ref() {
                    Pattern::Constructor(Constructor::Boolean(b), _, _) => {
                        possible &= if *b {
                            boolean.can_be_true()
                        } else {
                            boolean.can_be_false()
                        }
                    }
                    Pattern::Any if boolean == BooleanSet::TRUE || boolean == BooleanSet::FALSE => {
                        self.elab.reserve(1)?;
                        restriction = Arc::new(Pattern::Constructor(
                            Constructor::Boolean(boolean == BooleanSet::TRUE),
                            Vec::new(),
                            None,
                        ));
                    }
                    _ => (),
                }
            }
            let available = if possible {
                self.coverage_within(module, node, &value.ty, &handled, restriction)?
                    .is_some_and(|remaining| !remaining.exhaustive())
            } else {
                false
            };
            let mut truth = BooleanSet::TRUE;
            if let Some(guard) = arm.guard {
                let boolean = self.primitive("bool")?;
                if let Some(g) = self.expression(module, guard, Some(boolean), &scope)? {
                    truth = if g.normal {
                        g.boolean.unwrap_or(BooleanSet::BOTH)
                    } else {
                        BooleanSet::EMPTY
                    };
                    if available {
                        let mut exits = g.flow;
                        exits.normal = false;
                        flow = self.union(flow, &exits)?;
                    }
                }
            }
            let block = module.syntax.nodes[arm.body].kind == NodeKind::Block;
            let (ty, branch) = if block {
                let f = if let Some(flow) = self.match_blocks.remove(&(&module.key, arm.body)) {
                    flow
                } else {
                    self.statement(module, arm.body, self.data_depth + 1, &mut scope)?
                };
                (self.primitive(if f.normal { "void" } else { "never" })?, f)
            } else if let Some(e) = self.expression(module, arm.body, common.clone(), &scope)? {
                (e.ty, e.flow)
            } else {
                continue;
            };
            result = if let Some(previous) = result {
                self.join_match(module, node, &previous, &ty)?
            } else {
                Some(ty)
            };
            arm.guard_reachable = available;
            arm.body_reachable = available && truth.can_be_true();
            if arm.body_reachable {
                flow = self.union(flow, &branch)?;
            }
            if !truth.can_be_false() {
                self.elab.reserve(1)?;
                handled.push(pattern);
            }
            self.elab.reserve(1)?;
            facts.push(arm);
        }
        flow = self.sequence(value.flow, &flow)?;
        let ty = if let Some(ty) = result {
            ty
        } else {
            self.primitive("never")?
        };
        self.elab.reserve(1)?;
        let Some(coverage) = coverage else {
            return Ok(None);
        };
        let prepared = PreparedData {
            ty,
            inputs: Vec::new(),
            contexts: Vec::new(),
            flow: Some(flow),
            fact: DataExpression::Match(Box::new(Match {
                scrutinee,
                arms: facts,
                coverage,
            })),
        };
        if before != self.marks() {
            return Ok(None);
        }
        Ok(Some(prepared))
    }
}
