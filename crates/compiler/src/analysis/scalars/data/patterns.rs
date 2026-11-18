use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Constructor<'g> {
    Tuple(usize),
    Record(&'g DeclarationId),
    Variant(&'g DeclarationId),
    Boolean(bool),
    Integer { negative: bool, magnitude: u128 },
    Character(u32),
    Float32(u32),
    Float64(u64),
    Text(Vec<char>),
    Hole,
    Other,
}
#[derive(Clone, Debug)]
pub struct CoverageStep<'g> {
    pub path: Vec<usize>,
    pub constructor: Constructor<'g>,
}
#[derive(Clone, Debug)]
pub struct Coverage<'g> {
    pub witness: Option<Vec<CoverageStep<'g>>>,
}
impl Coverage<'_> {
    pub fn exhaustive(&self) -> bool {
        self.witness.is_none()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternProjection<'g> {
    Dereference,
    Field(&'g DeclarationId),
    TupleElement(usize),
    Payload {
        variant: &'g DeclarationId,
        index: usize,
    },
}
#[derive(Clone, Debug)]
pub struct PatternBinder<'g> {
    pub declaration: &'g DeclarationId,
    pub ty: Type<'g>,
    pub source_type: Type<'g>,
    pub projections: Vec<PatternProjection<'g>>,
    pub mutable: bool,
}
#[derive(Clone, Debug)]
pub struct PatternFacts<'g> {
    pub source: Site<'g>,
    pub access: Access,
    pub ty: Type<'g>,
    pub binders: Vec<PatternBinder<'g>>,
    pub coverage: Coverage<'g>,
    pub(in crate::analysis) pattern: Arc<Pattern<'g>>,
}
#[derive(Clone, Debug)]
pub(in crate::analysis) enum Pattern<'g> {
    Any,
    Wildcard,
    Constructor(Constructor<'g>, Vec<Arc<Pattern<'g>>>, Option<NodeId>),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Owned,
    ReadOnly,
    Writable,
}
#[derive(Clone, Copy)]
pub(super) struct PatternInput<'a, 'g> {
    pub ty: &'a Type<'g>,
    pub access: Access,
    pub mutable: bool,
    pub emit: bool,
}
struct Column<'g> {
    ty: Type<'g>,
    component: bool,
    path: Vec<usize>,
    restriction: Arc<Pattern<'g>>,
}
struct Matrix<'g> {
    columns: Vec<Column<'g>>,
    rows: Vec<Vec<Arc<Pattern<'g>>>>,
    trail: Vec<CoverageStep<'g>>,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn guard_payload_pattern(
        &mut self,
        module: &'g Module,
        pattern: NodeId,
        operand: NodeId,
        container: &super::super::core::GuardOperand<'g>,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Option<BooleanSet>> {
        let before = self.marks();
        let mut binders = Vec::new();
        let Some(prepared) = self.pattern(
            module,
            pattern,
            PatternInput {
                ty: &container.payload,
                access: container.access,
                mutable: false,
                emit: true,
            },
            depth,
            context,
            &mut binders,
        )?
        else {
            self.block_pattern_names(module, pattern, pattern, false, false, context)?;
            return Ok(None);
        };
        let Some(coverage) =
            self.coverage(module, pattern, &container.payload, vec![prepared.clone()])?
        else {
            return Ok(None);
        };
        let Some(reachable) =
            self.coverage_within(module, pattern, &container.payload, &[], prepared.clone())?
        else {
            return Ok(None);
        };
        if before != self.marks() {
            return Ok(None);
        }
        self.elab.reserve(1 + binders.len())?;
        let truth = (if reachable.exhaustive() {
            BooleanSet::EMPTY
        } else {
            BooleanSet::TRUE
        })
        .union(if coverage.exhaustive() {
            BooleanSet::EMPTY
        } else {
            BooleanSet::FALSE
        });
        self.record_node(NodeFacts {
            site: Self::site(module, pattern),
            flow: Flow::normal(),
            callable: None,
            kind: NodeFactKind::Pattern(PatternFacts {
                source: Self::site(module, operand),
                access: container.access,
                ty: container.payload.clone(),
                binders,
                coverage,
                pattern: prepared,
            }),
        });
        Ok(Some(truth))
    }

    pub(in crate::analysis::scalars) fn iteration_pattern(
        &mut self,
        module: &'g Module,
        iteration: &Iteration<'g>,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<()> {
        let before = self.marks();
        self.irrefutable_binding(
            module,
            (iteration.pattern, iteration.pattern, iteration.source),
            PatternInput {
                ty: &iteration.yielded,
                access: Access::Owned,
                mutable: false,
                emit: true,
            },
            depth,
            context,
            &Flow::normal(),
        )?;
        if before != self.marks() {
            self.block_pattern_names(
                module,
                iteration.pattern,
                iteration.pattern,
                false,
                false,
                context,
            )?;
        }
        Ok(())
    }

    pub(in crate::analysis::scalars) fn pattern_binding(
        &mut self,
        module: &'g Module,
        node: NodeId,
        mutable: bool,
        depth: usize,
        context: &mut Context<'r, 'g>,
    ) -> Work<Flow> {
        let n = &module.syntax.nodes[node];
        let before = self.marks();
        let annotation = if n.children.len() == 3 {
            self.elab.ty(
                module,
                n.children[1],
                context.signature.map_or(&[], |s| &s.binders),
                1,
            )?
        } else {
            None
        };
        let initializer = *n.children.last().unwrap();
        let value = self.expression(module, initializer, annotation.clone(), context)?;
        let flow = if let Some(value) = &value {
            self.union(Flow::default(), &value.flow)?
        } else {
            Flow::normal()
        };
        if value.is_none() || before != self.marks() {
            let cause = if n.children.len() == 3 && annotation.is_none() {
                n.children[1]
            } else {
                initializer
            };
            self.block_pattern_names(module, n.children[0], cause, mutable, false, context)?;
            return Ok(flow);
        }
        if let Some(value) = value {
            let ty = annotation.unwrap_or(value.ty);
            self.irrefutable_binding(
                module,
                (node, n.children[0], initializer),
                PatternInput {
                    ty: &ty,
                    access: value.access,
                    mutable,
                    emit: true,
                },
                depth,
                context,
                &flow,
            )?;
        }
        if before != self.marks() {
            self.block_pattern_names(
                module,
                n.children[0],
                n.children[0],
                mutable,
                false,
                context,
            )?;
        }
        Ok(flow)
    }
    fn irrefutable_binding(
        &mut self,
        module: &'g Module,
        (site, node, source): (NodeId, NodeId, NodeId),
        input: PatternInput<'_, 'g>,
        depth: usize,
        context: &mut Context<'r, 'g>,
        flow: &Flow,
    ) -> Work<()> {
        let before = self.marks();
        let (ty, access) = (input.ty, input.access);
        let mut binders = Vec::new();
        if let Some(pattern) = self.pattern(module, node, input, depth, context, &mut binders)?
            && before == self.marks()
            && let Some(coverage) = self.coverage(module, site, ty, vec![pattern.clone()])?
        {
            if !coverage.exhaustive() {
                self.finding(module, node, ScalarFindingKind::RefutableBinding)?;
                self.retain_coverage(module, node, ty, &coverage)?;
            } else if before == self.marks() {
                self.elab
                    .reserve(1 + binders.len() + flow.breaks.len() + flow.continues.len())?;
                self.record_node(NodeFacts {
                    site: Self::site(module, node),
                    flow: flow.clone(),
                    callable: None,
                    kind: NodeFactKind::Pattern(PatternFacts {
                        source: Self::site(module, source),
                        access,
                        ty: ty.clone(),
                        binders,
                        coverage,
                        pattern,
                    }),
                });
            }
        }
        Ok(())
    }

    pub(super) fn retain_coverage(
        &mut self,
        module: &'g Module,
        node: NodeId,
        ty: &Type<'g>,
        coverage: &Coverage<'g>,
    ) -> Work<()> {
        let size = coverage.witness.as_ref().map_or(0, |w| {
            w.iter()
                .map(|s| {
                    1 + s.path.len()
                        + if let Constructor::Text(text) = &s.constructor {
                            text.len()
                        } else {
                            0
                        }
                })
                .sum()
        });
        self.elab.reserve(1 + size)?;
        self.record_node(NodeFacts {
            site: Self::site(module, node),
            flow: Flow::default(),
            callable: None,
            kind: NodeFactKind::Coverage {
                ty: ty.clone(),
                coverage: coverage.clone(),
            },
        });
        Ok(())
    }
    pub(super) fn block_pattern_names(
        &mut self,
        module: &'g Module,
        pattern: NodeId,
        cause: NodeId,
        mutable: bool,
        preserve_existing: bool,
        context: &mut Context<'r, 'g>,
    ) -> Work<()> {
        self.elab.reserve(1)?;
        let mut stack = vec![pattern];
        while let Some(node) = stack.pop() {
            self.elab.step(1)?;
            let n = &module.syntax.nodes[node];
            let name = simple_name(module, node).or((n.kind == NodeKind::Name).then_some(node));
            if let Some(name) = name {
                let declaration = self.elab.declaration_at(module, name)?;
                let spelling = declaration.name.as_deref().unwrap();
                if let Some(local) = context.scopes.last().unwrap().get(spelling) {
                    if local.declaration != &declaration.id {
                        self.finding(module, name, ScalarFindingKind::DuplicateBinding)?;
                        continue;
                    }
                    if preserve_existing {
                        continue;
                    }
                }
                self.elab.reserve(1)?;
                context.scopes.last_mut().unwrap().insert(
                    spelling,
                    Local {
                        declaration: &declaration.id,
                        ty: None,
                        mutable,
                        boolean: None,
                    },
                );
                self.elab.step(self.facts.nodes.len())?;
                if let Some(b) = self.facts.nodes.iter_mut().find_map(|n| match &mut n.kind {
                    NodeFactKind::Binding(b) if b.declaration == &declaration.id => Some(b),
                    _ => None,
                }) {
                    b.ty = None;
                    b.blocked_by = Some(Self::site(module, cause));
                } else {
                    self.elab.reserve(1)?;
                    self.record_node(NodeFacts {
                        site: Self::site(module, name),
                        flow: Flow::default(),
                        callable: None,
                        kind: NodeFactKind::Binding(BindingFacts {
                            declaration: &declaration.id,
                            ty: None,
                            mutable,
                            blocked_by: Some(Self::site(module, cause)),
                        }),
                    });
                }
            } else {
                let children = match n.kind {
                    NodeKind::PatternField { shorthand: false } => &n.children[1..],
                    NodeKind::PatternField { shorthand: true }
                    | NodeKind::BracketPattern
                    | NodeKind::RecordPattern
                    | NodeKind::PayloadPattern
                    | NodeKind::GroupPattern => &n.children[..],
                    _ => &[],
                };
                self.elab.reserve(children.len())?;
                stack.extend(children);
            }
        }
        Ok(())
    }
    fn prefix_binders(
        &mut self,
        binders: &mut [PatternBinder<'g>],
        projection: PatternProjection<'g>,
    ) -> Work<()> {
        for binder in binders {
            if binder.projections.len() >= self.elab.limits.max_depth.min(64) {
                return Err(());
            }
            self.elab.step(1 + binder.projections.len())?;
            self.elab.reserve(1)?;
            binder.projections.insert(0, projection);
            self.elab.statistics.max_depth =
                self.elab.statistics.max_depth.max(binder.projections.len());
        }
        Ok(())
    }
    fn bind_pattern(
        &mut self,
        module: &'g Module,
        name: NodeId,
        input: PatternInput<'_, 'g>,
        context: &mut Context<'r, 'g>,
        binders: &mut Vec<PatternBinder<'g>>,
    ) -> Work<bool> {
        let PatternInput {
            ty,
            access,
            mutable,
            emit,
        } = input;
        let declaration = self.elab.declaration_at(module, name)?;
        let bound = if access == Access::Owned {
            Some(ty.clone())
        } else {
            let copying = self.copy_inputs();
            match implicit_copy(&mut self.elab, copying, ty)? {
                Some(true) => Some(ty.clone()),
                Some(false) => {
                    self.elab.reserve(1)?;
                    Some(Arc::new(TypeTerm::Reference {
                        mutable: access == Access::Writable,
                        referent: ty.clone(),
                    }))
                }
                None => {
                    self.defer(module, name, ScalarUnsupported::ImplicitCopy)?;
                    if !self.integrated {
                        return Ok(false);
                    }
                    // The binder still has an irrefutable shape. Its type is pending.
                    None
                }
            }
        };
        self.elab.step(binders.len())?;
        let spelling = declaration.name.as_deref().unwrap();
        if context.scopes.last().unwrap().contains_key(spelling) {
            self.finding(module, name, ScalarFindingKind::DuplicateBinding)?;
            return Ok(false);
        }
        self.elab.reserve(3)?;
        context.scopes.last_mut().unwrap().insert(
            spelling,
            Local {
                declaration: &declaration.id,
                ty: bound.clone(),
                mutable,
                boolean: None,
            },
        );
        if let Some(bound) = &bound {
            binders.push(PatternBinder {
                declaration: &declaration.id,
                ty: bound.clone(),
                source_type: ty.clone(),
                projections: Vec::new(),
                mutable,
            });
        }
        if emit {
            self.record_node(NodeFacts {
                site: Self::site(module, name),
                flow: Flow::normal(),
                callable: None,
                kind: NodeFactKind::Binding(BindingFacts {
                    declaration: &declaration.id,
                    blocked_by: bound.is_none().then(|| Self::site(module, name)),
                    ty: bound,
                    mutable,
                }),
            });
        }
        Ok(true)
    }
    pub(super) fn pattern(
        &mut self,
        module: &'g Module,
        node: NodeId,
        input: PatternInput<'_, 'g>,
        depth: usize,
        context: &mut Context<'r, 'g>,
        binders: &mut Vec<PatternBinder<'g>>,
    ) -> Work<Option<Arc<Pattern<'g>>>> {
        let PatternInput {
            ty: original,
            access: inherited,
            mutable,
            emit,
        } = input;
        self.elab.step(1)?;
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        let n = &module.syntax.nodes[node];
        if self.type_unavailable(original, 1)? {
            self.defer(module, node, ScalarUnsupported::DeclarationPrerequisite)?;
            return Ok(None);
        }
        if n.kind == NodeKind::Wildcard {
            self.elab.reserve(1)?;
            return Ok(Some(Arc::new(Pattern::Wildcard)));
        }
        if let Some(name) = simple_name(module, node).or((n.kind == NodeKind::Name).then_some(node))
        {
            if !self.bind_pattern(module, name, input, context, binders)? {
                return Ok(None);
            }
            self.elab.reserve(1)?;
            return Ok(Some(Arc::new(Pattern::Any)));
        }
        if n.kind == NodeKind::GroupPattern {
            return self.pattern(
                module,
                n.children[0],
                PatternInput {
                    ty: original,
                    access: inherited,
                    mutable,
                    emit,
                },
                depth + 1,
                context,
                binders,
            );
        }
        let first_binder = binders.len();
        let (ty, access) = if let TypeTerm::Reference { mutable, referent } = original.as_ref() {
            (
                referent,
                if inherited == Access::ReadOnly || !mutable {
                    Access::ReadOnly
                } else {
                    Access::Writable
                },
            )
        } else {
            (original, inherited)
        };
        if n.kind == NodeKind::BracketPattern {
            let TypeTerm::Tuple(types) = ty.as_ref() else {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            };
            if types.len() != n.children.len() {
                self.finding(module, node, ScalarFindingKind::Arity)?;
                return Ok(None);
            }
            let mut children = Vec::new();
            for (index, (&child, ty)) in n.children.iter().zip(types).enumerate() {
                let first = binders.len();
                let Some(pattern) = self.pattern(
                    module,
                    child,
                    PatternInput {
                        ty,
                        access,
                        mutable,
                        emit,
                    },
                    depth + 1,
                    context,
                    binders,
                )?
                else {
                    return Ok(None);
                };
                self.prefix_binders(
                    &mut binders[first..],
                    PatternProjection::TupleElement(index),
                )?;
                self.elab.reserve(1)?;
                children.push(pattern);
            }
            if matches!(original.as_ref(), TypeTerm::Reference { .. }) {
                self.prefix_binders(&mut binders[first_binder..], PatternProjection::Dereference)?;
            }
            self.elab.reserve(1)?;
            return Ok(Some(Arc::new(Pattern::Constructor(
                Constructor::Tuple(types.len()),
                children,
                Some(node),
            ))));
        }
        if n.kind == NodeKind::Template {
            if primitive_name(ty) != Some("string") {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            }
            let mut text = Vec::new();
            for &child in &n.children {
                self.elab.step(1)?;
                if module.syntax.nodes[child].kind != NodeKind::TemplateText {
                    self.defer(module, node, ScalarUnsupported::Formatting)?;
                    return Ok(None);
                }
                let token = self.token(module, child)?;
                let TokenKind::Literal(literal) = &token.kind else {
                    unreachable!()
                };
                self.elab.reserve(literal.decoded.len())?;
                text.extend(literal.decoded.iter().map(|c| c.value));
            }
            self.elab.reserve(2)?;
            if emit {
                self.record_node(NodeFacts {
                    site: Self::site(module, node),
                    flow: Flow::default(),
                    callable: None,
                    kind: NodeFactKind::PatternLiteral {
                        ty: ty.clone(),
                        evidence: LiteralEvidence {
                            token: None,
                            value: LiteralValue::StaticTemplate,
                            negation: None,
                        },
                    },
                });
            }
            return Ok(Some(Arc::new(Pattern::Constructor(
                Constructor::Text(text),
                Vec::new(),
                Some(node),
            ))));
        }
        if matches!(n.kind, NodeKind::Literal | NodeKind::NegatedPattern) {
            return self.literal_pattern(module, node, ty, emit);
        }
        if matches!(
            n.kind,
            NodeKind::RecordPattern | NodeKind::PayloadPattern | NodeKind::NamePattern
        ) {
            let pattern = self.constructor_pattern(
                module,
                node,
                PatternInput {
                    ty,
                    access,
                    mutable,
                    emit,
                },
                depth,
                context,
                binders,
            )?;
            if matches!(original.as_ref(), TypeTerm::Reference { .. }) {
                self.prefix_binders(&mut binders[first_binder..], PatternProjection::Dereference)?;
            }
            return Ok(pattern);
        }
        self.unsupported(module, node)?;
        Ok(None)
    }
    fn literal_pattern(
        &mut self,
        module: &'g Module,
        node: NodeId,
        ty: &Type<'g>,
        emit: bool,
    ) -> Work<Option<Arc<Pattern<'g>>>> {
        let n = &module.syntax.nodes[node];
        let mut literal = node;
        if n.kind == NodeKind::NegatedPattern {
            literal = n.children[0];
            while module.syntax.nodes[literal].kind == NodeKind::GroupPattern {
                literal = module.syntax.nodes[literal].children[0];
                self.elab.step(1)?;
            }
            if !primitive_name(ty)
                .is_some_and(|p| matches!(p, "i8" | "i16" | "i32" | "i64" | "i128" | "f32" | "f64"))
            {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            }
        }
        let slot = Slot {
            expected: Some(ty.clone()),
            ..Slot::default()
        };
        let Some(actual) = self.literal_type(module, literal, &slot)? else {
            self.finding(module, node, ScalarFindingKind::NumericContext)?;
            return Ok(None);
        };
        if !self.same(&actual, ty)? {
            self.mismatch(module, node, &actual, ty)?;
            return Ok(None);
        }
        let Some(evidence) = self.literal_value(
            module,
            literal,
            ty,
            (n.kind == NodeKind::NegatedPattern).then_some(node),
        )?
        else {
            return Ok(None);
        };
        let key = match evidence.value {
            LiteralValue::Integer {
                magnitude,
                negative,
            } => Constructor::Integer {
                magnitude,
                negative: negative && magnitude != 0,
            },
            LiteralValue::Boolean(b) => Constructor::Boolean(b),
            LiteralValue::Float32 { bits } => {
                Constructor::Float32(if bits << 1 == 0 { 0 } else { bits })
            }
            LiteralValue::Float64 { bits } => {
                Constructor::Float64(if bits << 1 == 0 { 0 } else { bits })
            }
            LiteralValue::Decoded => {
                let TokenKind::Literal(value) = &evidence.token.unwrap().kind else {
                    unreachable!()
                };
                if value.mode == LiteralMode::Character {
                    Constructor::Character(value.decoded[0].value as u32)
                } else {
                    self.elab.reserve(value.decoded.len())?;
                    Constructor::Text(value.decoded.iter().map(|c| c.value).collect())
                }
            }
            LiteralValue::StaticTemplate => unreachable!(),
        };
        self.elab.reserve(2)?;
        if emit {
            self.record_node(NodeFacts {
                site: Self::site(module, node),
                flow: Flow::default(),
                callable: None,
                kind: NodeFactKind::PatternLiteral {
                    ty: ty.clone(),
                    evidence,
                },
            });
        }
        Ok(Some(Arc::new(Pattern::Constructor(
            key,
            Vec::new(),
            Some(node),
        ))))
    }
    fn constructor_pattern(
        &mut self,
        module: &'g Module,
        node: NodeId,
        input: PatternInput<'_, 'g>,
        depth: usize,
        context: &mut Context<'r, 'g>,
        binders: &mut Vec<PatternBinder<'g>>,
    ) -> Work<Option<Arc<Pattern<'g>>>> {
        let PatternInput {
            ty: original,
            access,
            mutable,
            emit,
        } = input;
        let n = &module.syntax.nodes[node];
        let named = n
            .children
            .first()
            .filter(|&&id| module.syntax.nodes[id].kind == NodeKind::PatternPath)
            .copied();
        let before = self.marks();
        let ty = if let Some(name) = named {
            let Some(ty) = self.constructor_type(module, name, context, Some(original), None)?
            else {
                if before == self.marks() {
                    self.defer(module, node, ScalarUnsupported::GenericInference)?;
                }
                return Ok(None);
            };
            ty
        } else {
            original.clone()
        };
        let parent = |t: &Type<'g>| {
            if let TypeTerm::Variant { parent, .. } = t.as_ref() {
                parent.clone()
            } else {
                t.clone()
            }
        };
        if !self.same(&parent(&ty), &parent(original))? {
            self.mismatch(module, node, &ty, original)?;
            return Ok(None);
        }
        let p = parent(&ty);
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = p.as_ref()
        else {
            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            return Ok(None);
        };
        let facts = &self.input.facts.declarations[declaration];
        if !facts.declaration.public && declaration.module != module.key {
            self.finding(module, node, ScalarFindingKind::PrivateValue)?;
            return Ok(None);
        }
        let tag = if let TypeTerm::Variant { declaration, .. } = ty.as_ref() {
            Constructor::Variant(declaration)
        } else {
            Constructor::Record(declaration)
        };
        let mut children = Vec::new();
        if n.kind == NodeKind::RecordPattern {
            let Some(fields) = self.member_types(&ty)? else {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            };
            self.elab.reserve(fields.len() + 1)?;
            children.resize(fields.len(), Arc::new(Pattern::Any));
            let mut seen = BTreeSet::new();
            for &field in &n.children[usize::from(named.is_some())..] {
                let f = &module.syntax.nodes[field];
                let NodeKind::PatternField { shorthand } = f.kind else {
                    unreachable!()
                };
                self.elab.step(fields.len())?;
                let spelling = module.syntax.nodes[f.children[0]].span.bytes();
                let Some((index, (member, field_ty))) = fields
                    .iter()
                    .enumerate()
                    .find(|(_, (d, _))| d.name.as_ref().is_some_and(|s| s.as_bytes() == spelling))
                else {
                    self.finding(module, field, ScalarFindingKind::MissingField)?;
                    return Ok(None);
                };
                if !member.public && member.id.module != module.key {
                    self.finding(module, field, ScalarFindingKind::PrivateValue)?;
                    return Ok(None);
                }
                self.elab.reserve(1)?;
                if !seen.insert(index) {
                    self.finding(module, field, ScalarFindingKind::DuplicateField)?;
                    return Ok(None);
                }
                let first = binders.len();
                let Some(pattern) = self.pattern(
                    module,
                    f.children[usize::from(!shorthand)],
                    PatternInput {
                        ty: field_ty,
                        access,
                        mutable,
                        emit,
                    },
                    depth + 1,
                    context,
                    binders,
                )?
                else {
                    return Ok(None);
                };
                self.prefix_binders(&mut binders[first..], PatternProjection::Field(&member.id))?;
                children[index] = pattern;
            }
        } else {
            let TypeTerm::Variant { declaration, .. } = ty.as_ref() else {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            };
            let DeclarationShape::Enum { variants } = &facts.shape else {
                unreachable!()
            };
            self.elab.step(variants.len())?;
            let variant = variants
                .iter()
                .find(|v| v.declaration.id == **declaration)
                .unwrap();
            match (&variant.payload, n.kind) {
                (Payload::Unit, NodeKind::NamePattern) => (),
                (Payload::Tuple(types), NodeKind::PayloadPattern) => {
                    if types.len() != n.children.len() - 1 {
                        self.finding(module, node, ScalarFindingKind::Arity)?;
                        return Ok(None);
                    }
                    let substitution = self.nominal_substitution(&facts.binders, arguments)?;
                    for (index, (&child, ty)) in n.children[1..].iter().zip(types).enumerate() {
                        let first = binders.len();
                        let ty = self.substitute(ty, &substitution, 1)?;
                        let Some(pattern) = self.pattern(
                            module,
                            child,
                            PatternInput {
                                ty: &ty,
                                access,
                                mutable,
                                emit,
                            },
                            depth + 1,
                            context,
                            binders,
                        )?
                        else {
                            return Ok(None);
                        };
                        self.prefix_binders(
                            &mut binders[first..],
                            PatternProjection::Payload {
                                variant: declaration,
                                index,
                            },
                        )?;
                        self.elab.reserve(1)?;
                        children.push(pattern);
                    }
                }
                _ => {
                    self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                    return Ok(None);
                }
            }
        }
        self.elab.reserve(1)?;
        Ok(Some(Arc::new(Pattern::Constructor(
            tag,
            children,
            Some(node),
        ))))
    }
    fn scalar_domain(
        &mut self,
        ty: &Type<'g>,
        rows: &[Vec<Arc<Pattern<'g>>>],
        restriction: &Pattern<'g>,
    ) -> Work<Vec<(Constructor<'g>, Vec<Type<'g>>)>> {
        let mut keys = BTreeSet::new();
        for pattern in rows
            .iter()
            .map(|row| row[0].as_ref())
            .chain(std::iter::once(restriction))
        {
            self.elab.step(1)?;
            if let Pattern::Constructor(key, _, _) = pattern {
                self.elab.reserve(
                    1 + if let Constructor::Text(text) = key {
                        text.len()
                    } else {
                        0
                    },
                )?;
                keys.insert(key.clone());
            }
        }
        let missing = if integer(ty) {
            let name = primitive_name(ty).unwrap();
            let bits: u32 = name[1..].parse().unwrap();
            let signed = name.starts_with('i');
            let positive = if signed {
                (1u128 << (bits - 1)) - 1
            } else if bits == 128 {
                u128::MAX
            } else {
                (1u128 << bits) - 1
            };
            let mut missing = None;
            for ordinal in 0..=keys.len() as u128 {
                self.elab.step(1)?;
                let (negative, magnitude) = if ordinal <= positive {
                    (false, ordinal)
                } else {
                    (true, ordinal - positive)
                };
                if negative && (!signed || magnitude > (1u128 << (bits - 1))) {
                    break;
                }
                let key = Constructor::Integer {
                    negative,
                    magnitude,
                };
                if !keys.contains(&key) {
                    missing = Some(key);
                    break;
                }
            }
            missing
        } else if primitive_name(ty) == Some("char") {
            let mut scalar = 0;
            let mut missing = None;
            loop {
                self.elab.step(1)?;
                if scalar > 0x10ffff {
                    break;
                }
                if (0xd800..=0xdfff).contains(&scalar) {
                    scalar = 0xe000;
                }
                let key = Constructor::Character(scalar);
                if !keys.contains(&key) {
                    missing = Some(key);
                    break;
                }
                scalar += 1;
            }
            missing
        } else {
            Some(Constructor::Other)
        };
        if let Some(missing) = missing {
            self.elab.reserve(1)?;
            keys.insert(missing);
        }
        self.elab.reserve(keys.len())?;
        Ok(keys.into_iter().map(|key| (key, Vec::new())).collect())
    }
    fn domain(
        &mut self,
        module: &'g Module,
        column: &Column<'g>,
        rows: &[Vec<Arc<Pattern<'g>>>],
    ) -> Work<Vec<(Constructor<'g>, Vec<Type<'g>>)>> {
        let mut ty = &column.ty;
        let reference = if let TypeTerm::Reference { referent, .. } = ty.as_ref() {
            ty = referent;
            true
        } else {
            false
        };
        if primitive_name(ty) == Some("never") {
            return if column.component || reference {
                self.elab.reserve(1)?;
                Ok(vec![(Constructor::Hole, Vec::new())])
            } else {
                Ok(Vec::new())
            };
        }
        if primitive_name(ty) == Some("bool") {
            self.elab.reserve(2)?;
            return Ok(vec![
                (Constructor::Boolean(false), Vec::new()),
                (Constructor::Boolean(true), Vec::new()),
            ]);
        }
        if let TypeTerm::Tuple(types) = ty.as_ref() {
            self.elab.reserve(1 + types.len())?;
            return Ok(vec![(Constructor::Tuple(types.len()), types.clone())]);
        }
        let parent = if let TypeTerm::Variant { parent, .. } = ty.as_ref() {
            parent
        } else {
            ty
        };
        if let TypeTerm::Nominal {
            declaration,
            arguments,
        } = parent.as_ref()
        {
            let facts = &self.input.facts.declarations[declaration];
            if !facts.declaration.public && declaration.module != module.key {
                self.elab.reserve(1)?;
                return Ok(vec![(Constructor::Other, Vec::new())]);
            }
            match &facts.shape {
                DeclarationShape::Record { .. } => {
                    let fields = self.member_types(ty)?.unwrap();
                    self.elab.reserve(1 + fields.len())?;
                    return Ok(vec![(
                        Constructor::Record(declaration),
                        fields.into_iter().map(|(_, ty)| ty).collect(),
                    )]);
                }
                DeclarationShape::Enum { variants } => {
                    let mut result = Vec::new();
                    let substitution = self.nominal_substitution(&facts.binders, arguments)?;
                    for variant in variants {
                        self.elab.step(1)?;
                        if let TypeTerm::Variant { declaration, .. } = ty.as_ref()
                            && **declaration != variant.declaration.id
                        {
                            continue;
                        }
                        let mut fields = Vec::new();
                        match &variant.payload {
                            Payload::Unit => (),
                            Payload::Tuple(types) => {
                                for ty in types {
                                    let ty = self.substitute(ty, &substitution, 1)?;
                                    self.elab.reserve(1)?;
                                    fields.push(ty);
                                }
                            }
                            Payload::Record(members) => {
                                for member in members {
                                    let ty = self.substitute(&member.ty, &substitution, 1)?;
                                    self.elab.reserve(1)?;
                                    fields.push(ty);
                                }
                            }
                        }
                        self.elab.reserve(1)?;
                        result.push((Constructor::Variant(&variant.declaration.id), fields));
                    }
                    if result.is_empty() && (column.component || reference) {
                        self.elab.reserve(1)?;
                        result.push((Constructor::Hole, Vec::new()));
                    }
                    return Ok(result);
                }
                _ => (),
            }
        }
        if matches!(ty.as_ref(), TypeTerm::Primitive(_))
            && let Pattern::Constructor(key, children, _) = column.restriction.as_ref()
            && children.is_empty()
        {
            self.elab.reserve(
                1 + if let Constructor::Text(text) = key {
                    text.len()
                } else {
                    0
                },
            )?;
            return Ok(vec![(key.clone(), Vec::new())]);
        }
        self.scalar_domain(ty, rows, &column.restriction)
    }
    pub(super) fn coverage(
        &mut self,
        module: &'g Module,
        node: NodeId,
        ty: &Type<'g>,
        patterns: Vec<Arc<Pattern<'g>>>,
    ) -> Work<Option<Coverage<'g>>> {
        self.elab.reserve(1)?;
        self.coverage_within(module, node, ty, &patterns, Arc::new(Pattern::Any))
    }
    pub(super) fn coverage_within(
        &mut self,
        module: &'g Module,
        node: NodeId,
        ty: &Type<'g>,
        patterns: &[Arc<Pattern<'g>>],
        restriction: Arc<Pattern<'g>>,
    ) -> Work<Option<Coverage<'g>>> {
        self.elab.reserve(2 + patterns.len() * 2)?;
        let mut stack = vec![Matrix {
            columns: vec![Column {
                ty: ty.clone(),
                component: false,
                path: Vec::new(),
                restriction,
            }],
            rows: patterns.iter().map(|p| vec![p.clone()]).collect(),
            trail: Vec::new(),
        }];
        while let Some(state) = stack.pop() {
            self.elab
                .step(1 + state.rows.iter().map(|r| r.len()).sum::<usize>())?;
            if state.rows.iter().any(|r| {
                r.iter()
                    .all(|p| matches!(p.as_ref(), Pattern::Any | Pattern::Wildcard))
            }) {
                continue;
            }
            if state.columns.is_empty() {
                return Ok(Some(Coverage {
                    witness: Some(state.trail),
                }));
            }
            let column = &state.columns[0];
            if self.type_unavailable(&column.ty, 1)? {
                self.defer(module, node, ScalarUnsupported::DeclarationPrerequisite)?;
                return Ok(None);
            }
            for (constructor, fields) in self.domain(module, column, &state.rows)?.into_iter().rev()
            {
                self.elab.step(1 + state.rows.len())?;
                if let Pattern::Constructor(tag, _, _) = column.restriction.as_ref()
                    && *tag != constructor
                {
                    continue;
                }
                let mut rows = Vec::new();
                for row in &state.rows {
                    let mut selected = match row[0].as_ref() {
                        Pattern::Any | Pattern::Wildcard => {
                            self.elab.reserve(fields.len() + 1)?;
                            {
                                let any = Arc::new(Pattern::Any);
                                vec![any; fields.len()]
                            }
                        }
                        Pattern::Constructor(tag, children, _) if *tag == constructor => {
                            self.elab.reserve(children.len())?;
                            children.clone()
                        }
                        _ => continue,
                    };
                    self.elab.reserve(row.len())?;
                    selected.extend_from_slice(&row[1..]);
                    rows.push(selected);
                }
                self.elab.reserve(
                    state
                        .trail
                        .iter()
                        .map(|s| {
                            1 + s.path.len()
                                + if let Constructor::Text(text) = &s.constructor {
                                    text.len()
                                } else {
                                    0
                                }
                        })
                        .sum::<usize>()
                        + column.path.len()
                        + 1,
                )?;
                let mut trail = state.trail.clone();
                trail.push(CoverageStep {
                    path: column.path.clone(),
                    constructor,
                });
                let mut columns = Vec::new();
                for (index, ty) in fields.into_iter().enumerate() {
                    if column.path.len() + 1 > self.elab.limits.max_depth.min(64) {
                        return Err(());
                    }
                    self.elab.reserve(2 + column.path.len())?;
                    let mut path = column.path.clone();
                    path.push(index);
                    self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(path.len());
                    let restriction = match column.restriction.as_ref() {
                        Pattern::Any | Pattern::Wildcard => {
                            self.elab.reserve(1)?;
                            Arc::new(Pattern::Any)
                        }
                        Pattern::Constructor(_, children, _) => children[index].clone(),
                    };
                    columns.push(Column {
                        ty,
                        component: true,
                        path,
                        restriction,
                    });
                }
                for rest in &state.columns[1..] {
                    self.elab.reserve(1 + rest.path.len())?;
                    columns.push(Column {
                        ty: rest.ty.clone(),
                        component: rest.component,
                        path: rest.path.clone(),
                        restriction: rest.restriction.clone(),
                    });
                }
                if rows.is_empty()
                    && columns
                        .iter()
                        .all(|c| matches!(c.restriction.as_ref(), Pattern::Any | Pattern::Wildcard))
                {
                    return Ok(Some(Coverage {
                        witness: Some(trail),
                    }));
                }
                self.elab.reserve(1)?;
                stack.push(Matrix {
                    columns,
                    rows,
                    trail,
                });
            }
        }
        Ok(Some(Coverage { witness: None }))
    }
}
