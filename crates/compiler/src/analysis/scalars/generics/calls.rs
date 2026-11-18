use super::*;
use ordinary::shape;

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn apply_ready_call_default(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<bool> {
        self.elab.reserve(inference.calls.len())?;
        let waiting: Vec<_> = inference
            .calls
            .iter()
            .filter_map(|(&node, call)| (call.infer && !call.family).then_some(node))
            .collect();
        for node in waiting {
            if self.infer_standalone_call(inference, node, true)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(in crate::analysis::scalars) fn infer_standalone_call(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        defaults_ready: bool,
    ) -> Work<bool> {
        let mut call = inference.calls.remove(&node).unwrap();
        let signature = &self.input.facts.declarations[call.fact.callee];
        let module = inference.module;
        if call.parameters.len() != call.fact.arguments.len() {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(false);
        }
        let binders = &signature.binders;
        self.elab.reserve(binders.len())?;
        let mut slots = vec![None; binders.len()];
        for sub in &call.fact.substitution {
            slots[sub.position] = Some(sub.argument.clone());
        }
        for (pattern, argument) in call.parameters.iter().zip(&call.fact.arguments) {
            self.elab.step(1)?;
            if let Some(actual) = inference.slots[argument].ty.as_ref()
                && primitive_name(actual) != Some("never")
                && !self.accept_input(
                    pattern,
                    actual,
                    binders,
                    &mut slots,
                    &call.fact.substitution,
                )?
            {
                self.finding(module, *argument, ScalarFindingKind::TypeMismatch)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(false);
            }
        }
        if let Some(expected) = &inference.slots[&node].expected
            && !self.closed(&call.result, binders, &|i| slots[i].is_some(), 1)?
        {
            self.elab.reserve(slots.len())?;
            let mut constrained = slots.clone();
            if self.input_equation(&call.result, expected, binders, &mut constrained, &[], 1)? {
                slots = constrained;
            }
        }
        let mut subs = self.slot_substitutions(binders, &slots)?;
        let applied = defaults_ready
            && self.try_call_default(inference, &mut call, binders, &slots, &subs)?;
        if applied {
            let chosen = call.fact.substitution.last().unwrap();
            slots[chosen.position] = Some(chosen.argument.clone());
            subs = self.slot_substitutions(binders, &slots)?;
            self.enqueue(inference, node)?;
        }
        for (pattern, &argument) in call.parameters.iter().zip(&call.fact.arguments) {
            if self.closed(pattern, binders, &|i| slots[i].is_some(), 1)? {
                let receiving = self.substitute(pattern, &subs, 1)?;
                self.establish(inference, argument, receiving, true)?;
            }
        }
        if subs.len() == binders.len()
            && call
                .fact
                .arguments
                .iter()
                .all(|a| inference.slots[a].ty.is_some())
        {
            if !self.check_call_requirements(module, node, call.fact.callee, &subs)? {
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(false);
            }
            let result = self.substitute(&call.result, &subs, 1)?;
            self.establish(inference, node, result.clone(), false)?;
            call.parameters = self.substitute_all(&call.parameters, &subs, 1)?;
            call.result = result;
            call.fact.substitution = subs;
            call.infer = false;
            self.record_callee(module, call.callee_node, call.fact.callee)?
        }
        self.elab.reserve(1)?;
        inference.calls.insert(node, call);
        Ok(applied)
    }

    // This entry currently completes defaults for simple receiving literals.
    // Defer more complex unfinished source rather than treating it as no producer.
    fn try_call_default(
        &mut self,
        inference: &Inference<'g>,
        call: &mut PreparedCall<'g>,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
        solved: &[Substitution<'g>],
    ) -> Work<bool> {
        let Some(header) = self.input.ordinary.unwrap().headers().get(call.fact.callee) else {
            return Ok(false);
        };
        for (binder, value) in binders.iter().zip(slots) {
            self.elab.step(header.defaults.len() + 1)?;
            if value.is_some() {
                continue;
            }
            let Some(default) = header
                .defaults
                .iter()
                .find(|d| *d.binder == binder.declaration.id)
            else {
                continue;
            };
            let Some(term) = default.ty.as_ref() else {
                continue;
            };
            if !self.closed(term, binders, &|i| slots[i].is_some(), 1)? {
                continue;
            }
            let mut blocked = false;
            for (parameter, &argument) in call.parameters.iter().zip(&call.fact.arguments) {
                if inference.slots[&argument].ty.is_none()
                    && !self.closed(parameter, binders, &|i| i != binder.position, 1)?
                    && !(self.call_slot(parameter, binders)? == Some(binder.position)
                        && inference.module.syntax.nodes[argument].kind == NodeKind::Literal)
                {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                let argument = self.substitute(term, solved, 1)?;
                self.elab.reserve(2)?;
                call.fact.substitution.push(Substitution {
                    binder: default.binder,
                    position: binder.position,
                    argument,
                });
                call.fact.defaults.push(DefaultArgumentUse {
                    binder: default.binder,
                    node: default.node,
                    span: default.span,
                });
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn slot_substitutions(
        &mut self,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
    ) -> Work<Vec<Substitution<'g>>> {
        self.elab.reserve(binders.len())?;
        Ok(binders
            .iter()
            .zip(slots)
            .filter_map(|(binder, argument)| {
                argument.as_ref().map(|ty| Substitution {
                    binder: &binder.declaration.id,
                    position: binder.position,
                    argument: ty.clone(),
                })
            })
            .collect())
    }

    fn call_slot(&mut self, ty: &Type<'g>, binders: &[Binder<'g>]) -> Work<Option<usize>> {
        self.elab.step(1)?;
        if let TypeTerm::Parameter {
            declaration,
            position,
        } = ty.as_ref()
            && binders
                .get(*position)
                .is_some_and(|b| b.declaration.id == **declaration)
        {
            return Ok(Some(*position));
        }
        Ok(None)
    }

    fn equation_depth(&mut self, depth: usize) -> Work<()> {
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        self.elab.step(1)
    }

    pub(super) fn closed(
        &mut self,
        term: &Type<'g>,
        binders: &[Binder<'g>],
        established: &impl Fn(usize) -> bool,
        depth: usize,
    ) -> Work<bool> {
        self.equation_depth(depth)?;
        if let Some(i) = self.call_slot(term, binders)? {
            return Ok(established(i));
        }
        for child in shape(term).1 {
            if !self.closed(child, binders, established, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn accept_input(
        &mut self,
        pattern: &Type<'g>,
        actual: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
        fixed: &[Substitution<'g>],
    ) -> Work<bool> {
        match (pattern.as_ref(), actual.as_ref()) {
            (TypeTerm::Nominal { .. }, TypeTerm::Variant { parent, .. }) => {
                self.input_equation(pattern, parent, binders, slots, fixed, 1)
            }
            (
                TypeTerm::Reference {
                    mutable: false,
                    referent: receiving,
                },
                TypeTerm::Reference {
                    mutable: false,
                    referent: actual,
                },
            ) if matches!(receiving.as_ref(), TypeTerm::Nominal { .. }) => {
                let actual = match actual.as_ref() {
                    TypeTerm::Variant { parent, .. } => parent,
                    _ => actual,
                };
                self.input_equation(receiving, actual, binders, slots, fixed, 2)
            }
            _ => self.input_equation(pattern, actual, binders, slots, fixed, 1),
        }
    }

    fn input_equation(
        &mut self,
        pattern: &Type<'g>,
        actual: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
        fixed: &[Substitution<'g>],
        depth: usize,
    ) -> Work<bool> {
        self.equation_depth(depth)?;
        if let Some(i) = self.call_slot(pattern, binders)? {
            if let Some(established) = &slots[i] {
                self.elab.step(fixed.len())?;
                return if depth == 1
                    && fixed.iter().any(|s| s.position == i)
                    && self.data_admission(actual, established)?
                {
                    Ok(true)
                } else {
                    self.same(established, actual)
                };
            }
            // Actual arguments come from the caller, where its binders are rigid.
            slots[i] = Some(actual.clone());
            return Ok(true);
        }
        let (p, ps) = shape(pattern);
        let (a, ac) = shape(actual);
        if p != a || ps.len() != ac.len() {
            return Ok(false);
        }
        for (p, a) in ps.zip(ac) {
            if !self.input_equation(p, a, binders, slots, fixed, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
