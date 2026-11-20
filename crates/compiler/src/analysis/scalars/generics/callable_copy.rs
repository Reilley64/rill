//! Admission of the compiler's known-environment copy operation. This is not
//! an ordinary implementation or evidence for an arbitrary `copy(T)` goal.
use super::*;
use crate::analysis::typed::{Primitive, PrimitiveOperation, PrimitiveProvider, PrimitiveRole};
use crate::analysis::{declarations::Budget, ordinary::source};
use callables::{ArrowFacts, CaptureFact, CaptureForm};

#[derive(Debug)]
pub struct CallableCopy<'g> {
    pub family: &'g DeclarationId,
    pub origin: Site<'g>,
    /// Checked operations in stored field order. Ownership and initialization
    /// of both environments are checked separately by M5.
    pub captures: Vec<CaptureCopy<'g>>,
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureCopy<'g> {
    pub site: Site<'g>,
    pub action: CaptureCopyAction<'g>,
}

#[derive(Clone, Copy, Debug)]
pub enum CaptureCopyAction<'g> {
    Scalar(PrimitiveProvider<'g>),
    Text {
        copy: PrimitiveProvider<'g>,
        dispose: PrimitiveProvider<'g>,
    },
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(super) fn known_callable_copy(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        call: &mut PreparedCall<'g>,
        inputs: &[Option<Type<'g>>],
    ) -> Work<bool> {
        let Some(core) = self.primitives else {
            return Ok(false);
        };
        let family = call.fact.callee;
        let module = inference.module;
        let syntax = &module.syntax.nodes;
        let ([Some(input)], [argument]) = (inputs, call.fact.arguments.as_slice()) else {
            return Ok(false);
        };
        let TypeTerm::Reference {
            mutable: false,
            referent,
        } = input.as_ref()
        else {
            return Ok(false);
        };
        if !self.integrated
            || !core.copy.is_some_and(|copy| std::ptr::eq(&copy.id, family))
            || !matches!(referent.as_ref(), TypeTerm::Callable { .. })
            || syntax[node].kind != NodeKind::Call
            || call.fact.pipe_input.is_some()
            || self.arrow_premise.is_some()
            || inference.slots[argument].ordinary_context.is_some()
            || inference.slots[argument].blocked
        {
            return Ok(false);
        }
        for written in &call.fact.substitution {
            if !self.same(referent, &written.argument)? {
                return Ok(false);
            }
        }
        let borrow = self.source_leaf(module, *argument)?;
        if syntax[borrow].kind != NodeKind::Prefix("&") {
            return Ok(false);
        }
        let owner = self.source_leaf(module, syntax[borrow].children[0])?;
        let slot = &inference.slots[&owner];
        let Some(arrow) = slot.arrow.clone() else {
            return Ok(false);
        };
        if syntax[owner].kind != NodeKind::ValueReference
            || slot.blocked
            || slot.ordinary_context.is_some()
            || slot.access != data::Access::Owned
            || !matches!(&slot.path, Some(path) if path.root.kind == DeclarationKind::Binding
                && path.projections.is_empty())
            || !matches!(&slot.ty, Some(ty) if self.same(ty, referent)?)
        {
            return Ok(false);
        }
        // Use the captured, closed family, and do not filter by explicit
        // arguments here: no ordinary contender may overlap this input.
        // Neither origins nor requirement truth participate in ordinary ranking.
        let mut budget = Budget {
            limits: self.elab.limits,
            statistics: self.elab.statistics,
        };
        let clear = (|| {
            let Ok(family) =
                source::admitted_family(self.input.ordinary.unwrap(), family, &mut budget)?
            else {
                return Ok(false);
            };
            Ok(source::project(family, inputs, &[], &mut budget)?.is_empty())
        })();
        self.elab.statistics = budget.statistics;
        if !clear? {
            return Ok(false);
        }

        // One definite exclusive capture disproves duplication, regardless of
        // whether we can lower the rest of the environment as a positive layout.
        if !std::ptr::eq(arrow.site.module, &module.key)
            || arrow.premise.is_some()
            || arrow.mode.is_none()
        {
            return Ok(false);
        }
        self.elab.step(arrow.captures.len())?;
        if let Some(capture) = arrow.captures.iter().find(|capture| {
            capture.reachable
                && capture.through.is_none()
                && capture.path.projections.is_empty()
                && std::ptr::eq(capture.site.module, &module.key)
                && matches!(
                    capture.ty.as_ref(),
                    TypeTerm::Reference { mutable: true, .. }
                )
                && matches!(
                    (capture.form, capture.access),
                    (
                        CaptureForm::ExplicitBorrow { mutable: true },
                        data::Access::Writable
                    ) | (
                        CaptureForm::CarriedReference { mutable: true },
                        data::Access::Owned
                    )
                )
        }) {
            self.finding(module, node, ScalarFindingKind::MissingRequirement)?;
            self.elab.step(self.facts.findings.len())?;
            self.facts
                .findings
                .iter_mut()
                .rev()
                .find(|finding| {
                    std::ptr::eq(finding.site.module, &module.key)
                        && finding.site.node == node
                        && finding.kind == ScalarFindingKind::MissingRequirement
                })
                .unwrap()
                .related = Some(capture.site);
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(true);
        }

        let Some(captures) = self.direct_copy_layout(module, &arrow)? else {
            return Ok(false);
        };
        self.elab.reserve(1 + captures.len())?;
        let mut actions = Vec::new();
        for capture in captures.into_iter().flatten() {
            let Some(copy) = self.capture_provider(PrimitiveRole::Copy, &capture.ty)? else {
                return Ok(false);
            };
            let action = if copy.primitive == Primitive::String {
                let Some(dispose) = self.capture_provider(PrimitiveRole::Dispose, &capture.ty)?
                else {
                    return Ok(false);
                };
                if !matches!(copy.binding, Some(binding) if binding.operation == PrimitiveOperation::CopyText)
                    || !matches!(dispose.binding, Some(binding) if binding.operation == PrimitiveOperation::DisposeText)
                {
                    return Ok(false);
                }
                CaptureCopyAction::Text { copy, dispose }
            } else {
                CaptureCopyAction::Scalar(copy)
            };
            actions.push(CaptureCopy {
                site: capture.site,
                action,
            });
        }
        let binder = &self
            .input
            .ordinary
            .unwrap()
            .captured_family(family)
            .unwrap()
            .header
            .binders[0];
        self.elab.reserve(1)?;
        call.fact.substitution = vec![Substitution {
            binder: &binder.declaration.id,
            position: 0,
            argument: referent.clone(),
        }];
        self.record_callee(module, call.callee_node, family)?;
        let result = inference.slots.get_mut(&node).unwrap();
        result.installed_operation =
            Some(data::DataExpression::CallableCopy(Arc::new(CallableCopy {
                family,
                origin: arrow.site,
                captures: actions,
            })));
        result.arrow = Some(arrow);
        self.establish(inference, node, referent.clone(), false)?;
        Ok(true)
    }

    fn capture_provider(
        &mut self,
        role: PrimitiveRole,
        ty: &Type<'g>,
    ) -> Work<Option<PrimitiveProvider<'g>>> {
        let core = self.primitives.unwrap();
        let family = match role {
            PrimitiveRole::Copy => core.copy,
            PrimitiveRole::Dispose => core.dispose,
            _ => unreachable!(),
        };
        let Some(family) = family else {
            return Ok(None);
        };
        self.elab
            .step(core.protection.len() + core.providers.len())?;
        let Some(winner) = core.winner(&family.id, ty) else {
            return Ok(None);
        };
        Ok(core
            .providers
            .iter()
            .find(|p| p.role == role && std::ptr::eq(&p.implementation.id, winner))
            .copied())
    }

    fn direct_copy_layout<'a>(
        &mut self,
        module: &'g Module,
        arrow: &'a ArrowFacts<'g>,
    ) -> Work<Option<[Option<&'a CaptureFact<'g>>; 2]>> {
        if !std::ptr::eq(arrow.site.module, &module.key)
            || arrow.premise.is_some()
            || arrow.mode.is_none()
            || !arrow.parameters.is_empty()
        {
            return Ok(None);
        }
        let syntax = &module.syntax.nodes;
        let body = self.source_leaf(module, syntax[arrow.site.node].children[1])?;
        let layout = match arrow.captures.as_slice() {
            [single] if self.source_leaf(module, single.site.node)? == body => [Some(single), None],
            [first, second]
                if syntax[body].kind == NodeKind::Bracket
                    && matches!(arrow.result.as_ref(), TypeTerm::Tuple(fields) if fields.len() == 2) =>
            {
                let [left, right] = syntax[body].children.as_slice() else {
                    return Ok(None);
                };
                let written = [
                    self.source_leaf(module, *left)?,
                    self.source_leaf(module, *right)?,
                ];
                let sites = [
                    self.source_leaf(module, first.site.node)?,
                    self.source_leaf(module, second.site.node)?,
                ];
                if written == sites {
                    [Some(first), Some(second)]
                } else if written == [sites[1], sites[0]] {
                    [Some(second), Some(first)]
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };
        for capture in layout.into_iter().flatten() {
            self.elab.step(1)?;
            if !std::ptr::eq(capture.site.module, &module.key)
                || !capture.reachable
                || capture.through.is_some()
                || !capture.path.projections.is_empty()
                || !matches!(&capture.storage, Some(storage) if self.same(&capture.ty, storage)?)
            {
                return Ok(None);
            }
            let source = self.source_leaf(module, capture.site.node)?;
            match (capture.form, syntax[source].kind, capture.ty.as_ref()) {
                (CaptureForm::Value, NodeKind::ValueReference, TypeTerm::Primitive(p))
                    if capture.access == data::Access::Owned
                        && Primitive::named(p.name()).is_some() => {}
                _ => return Ok(None),
            }
        }
        if let [Some(a), Some(b)] = layout
            && std::ptr::eq(a.path.root, b.path.root)
            && primitive_name(&a.ty) == Some("string")
        {
            return Ok(None);
        }
        Ok(Some(layout))
    }
}
