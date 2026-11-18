use super::patterns::Pattern;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathProjection<'g> {
    Dereference,
    Field(&'g DeclarationId),
    TupleElement(usize),
    /// A dynamic selection retains the source but proves no element identity.
    ListElement,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacePath<'g> {
    pub root: &'g DeclarationId,
    pub projections: Vec<PathProjection<'g>>,
}
#[derive(Clone, Debug)]
pub struct Refinement<'g> {
    pub original: Type<'g>,
    pub ty: Type<'g>,
    pub path: Option<PlacePath<'g>>,
    pub tag_path: Option<PlacePath<'g>>,
    pub scrutinee: Site<'g>,
    pub pattern: Site<'g>,
    pub requires_tag_stability: bool,
}
impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn copy_path(
        &mut self,
        path: Option<&PlacePath<'g>>,
    ) -> Work<Option<PlacePath<'g>>> {
        if let Some(path) = path {
            self.elab.reserve(path.projections.len())?;
        }
        Ok(path.cloned())
    }
    pub(in crate::analysis::scalars) fn project_path(
        &mut self,
        path: Option<&PlacePath<'g>>,
        projection: PathProjection<'g>,
    ) -> Work<Option<PlacePath<'g>>> {
        let mut path = self.copy_path(path)?;
        if let Some(path) = &mut path {
            if path.projections.len() >= self.elab.limits.max_depth.min(64) {
                return Err(());
            }
            self.elab.reserve(1)?;
            path.projections.push(projection);
            self.elab.statistics.max_depth =
                self.elab.statistics.max_depth.max(path.projections.len());
        }
        Ok(path)
    }
    pub(in crate::analysis::scalars) fn refined(
        &mut self,
        path: Option<&PlacePath<'g>>,
        original: Type<'g>,
        context: &Context<'r, 'g>,
    ) -> Work<(Type<'g>, Option<Arc<Refinement<'g>>>)> {
        if let Some(path) = path {
            self.elab.step(path.projections.len())?;
            if path.projections.contains(&PathProjection::ListElement) {
                return Ok((original, None));
            }
            for view in context.views.iter().rev() {
                self.elab.step(1 + path.projections.len())?;
                if view.path.as_ref() == Some(path) {
                    return Ok((view.ty.clone(), Some(view.clone())));
                }
            }
        }
        Ok((original, None))
    }
    pub(super) fn pattern_views(
        &mut self,
        module: &'g Module,
        scrutinee: NodeId,
        value: &ExpressionResult<'g>,
        pattern: Arc<Pattern<'g>>,
        context: &Context<'r, 'g>,
    ) -> Work<Vec<Arc<Refinement<'g>>>> {
        self.elab.reserve(1)?;
        let path = self.copy_path(value.path.as_ref())?;
        let mut stack = vec![(pattern, value.ty.clone(), path)];
        let mut views = Vec::new();
        while let Some((pattern, original, path)) = stack.pop() {
            self.elab.step(1)?;
            let Pattern::Constructor(tag, children, site) = pattern.as_ref() else {
                continue;
            };
            let mut subject = original.clone();
            let mut projected = self.copy_path(path.as_ref())?;
            let mutable = if let TypeTerm::Reference { mutable, referent } = subject.as_ref() {
                projected = self.project_path(path.as_ref(), PathProjection::Dereference)?;
                let mutable = *mutable;
                subject = referent.clone();
                Some(mutable)
            } else {
                None
            };
            let parent = if let TypeTerm::Variant { parent, .. } = subject.as_ref() {
                parent.clone()
            } else {
                subject.clone()
            };
            let mut selected = subject.clone();
            if let Constructor::Variant(declaration) = tag {
                self.elab.reserve(1)?;
                selected = Arc::new(TypeTerm::Variant {
                    parent: parent.clone(),
                    declaration,
                });
                let mut ty = selected.clone();
                if let Some(mutable) = mutable {
                    self.elab.reserve(1)?;
                    ty = Arc::new(TypeTerm::Reference {
                        mutable,
                        referent: ty,
                    });
                }
                let mut declared = original.clone();
                for old in context.views.iter().rev() {
                    self.elab
                        .step(1 + path.as_ref().map_or(0, |p| p.projections.len()))?;
                    if path.is_some() && old.path == path {
                        declared = old.original.clone();
                        break;
                    }
                }
                self.elab.reserve(2)?;
                let view_path = self.copy_path(path.as_ref())?;
                let tag_path = self.copy_path(projected.as_ref())?;
                views.push(Arc::new(Refinement {
                    original: declared,
                    ty,
                    path: view_path,
                    tag_path,
                    scrutinee: Self::site(module, scrutinee),
                    pattern: Self::site(module, site.unwrap()),
                    requires_tag_stability: true,
                }));
            }
            if children.is_empty() {
                continue;
            }
            if let Some(fields) = self.member_types(&selected)? {
                for ((field, ty), child) in fields.into_iter().zip(children).rev() {
                    let path =
                        self.project_path(projected.as_ref(), PathProjection::Field(&field.id))?;
                    self.elab.reserve(1)?;
                    stack.push((child.clone(), ty, path));
                }
            } else if let TypeTerm::Tuple(types) = selected.as_ref() {
                for (ty, child) in types.iter().zip(children).rev() {
                    self.elab.reserve(1)?;
                    stack.push((child.clone(), ty.clone(), None));
                }
            } else if let TypeTerm::Variant { declaration, .. } = selected.as_ref() {
                let TypeTerm::Nominal {
                    declaration: owner,
                    arguments,
                } = parent.as_ref()
                else {
                    unreachable!()
                };
                let facts = &self.input.facts.declarations[owner];
                let DeclarationShape::Enum { variants } = &facts.shape else {
                    unreachable!()
                };
                self.elab.step(variants.len())?;
                let variant = variants
                    .iter()
                    .find(|v| v.declaration.id == **declaration)
                    .unwrap();
                let Payload::Tuple(types) = &variant.payload else {
                    unreachable!()
                };
                let substitution = self.nominal_substitution(&facts.binders, arguments)?;
                for (ty, child) in types.iter().zip(children).rev() {
                    let ty = self.substitute(ty, &substitution, 1)?;
                    self.elab.reserve(1)?;
                    stack.push((child.clone(), ty, None));
                }
            }
        }
        Ok(views)
    }
    pub(in crate::analysis::scalars) fn tag_change(
        &mut self,
        module: &'g Module,
        node: NodeId,
        left: &ExpressionResult<'g>,
        right: Option<&ExpressionResult<'g>>,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        let Some(path) = &left.path else {
            return Ok(());
        };
        for view in &context.views {
            let Some(tag) = &view.tag_path else {
                continue;
            };
            self.elab.step(1 + path.projections.len())?;
            if path.root != tag.root || !tag.projections.starts_with(&path.projections) {
                continue;
            }
            let expected = if let TypeTerm::Reference { referent, .. } = view.ty.as_ref() {
                referent
            } else {
                &view.ty
            };
            if path == tag
                && let Some(right) = right
                && self.same(&right.ty, expected)?
            {
                continue;
            }
            self.finding(module, node, ScalarFindingKind::TagChange)?;
        }
        Ok(())
    }
}
