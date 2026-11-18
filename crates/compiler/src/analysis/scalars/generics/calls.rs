use super::*;
use ordinary::shape;

#[derive(Clone, Copy)]
enum Equation<'a, 'g> {
    Input(&'a [Substitution<'g>]),
    FreeOutput(&'a BTreeSet<usize>),
}

pub(in crate::analysis::scalars) struct Producer {
    reads: BTreeSet<usize>,
    writes: BTreeSet<usize>,
    unavailable: bool,
}

impl<'r, 'g> Checker<'r, 'g> {
    pub(in crate::analysis::scalars) fn advance_call_context(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<bool> {
        self.elab.reserve(inference.calls.len())?;
        let waiting: Vec<_> = inference
            .calls
            .iter()
            .filter_map(|(&node, call)| call.infer.then_some(node))
            .collect();
        for node in waiting {
            let callee = inference.calls[&node].fact.callee;
            if self.is_installed_into(callee) {
                if self.explicit_into_source_context(inference, node)? {
                    return Ok(true);
                }
                continue;
            }
            if self.is_installed_show(callee) || self.conversion_family(callee).is_some() {
                continue;
            }
            let advanced = self.with_node_premise(inference, node, |checker, inference| {
                if inference.calls[&node].family {
                    checker.ordinary_call_step(inference, node, true)
                } else {
                    checker.infer_standalone_call(inference, node, true)
                }
            })?;
            if advanced {
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
        if !self.source_equations(inference, &call, binders, &mut slots)? {
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(false);
        }
        self.apply_callable_equations(&[(call.fact.callee, &[])], binders, &mut slots)?;
        let pending = self.pending_conversion_outputs(call.fact.callee, binders, &slots)?;
        if let Some(expected) = &inference.slots[&node].expected
            && !self.closed(&call.result, binders, &|i| slots[i].is_some(), 1)?
        {
            self.elab.reserve(slots.len())?;
            let mut constrained = slots.clone();
            if pending.is_empty() {
                if self.input_equation(&call.result, expected, binders, &mut constrained, &[], 1)? {
                    slots = constrained;
                }
            } else if self.free_output_equation(
                &call.result,
                expected,
                binders,
                &mut constrained,
                &pending,
            )? {
                slots = constrained;
            }
        }
        let conversion_requirements =
            self.apply_conversion_outputs(inference, node, call.fact.callee, binders, &mut slots)?;
        let mut subs = self.slot_substitutions(binders, &slots)?;
        let applied = defaults_ready
            && self.try_call_default(inference, node, &mut call, binders, &slots, &subs, None)?;
        if applied {
            let chosen = call.fact.substitution.last().unwrap();
            slots[chosen.position] = Some(chosen.argument.clone());
            subs = self.slot_substitutions(binders, &slots)?;
            self.enqueue(inference, node)?;
        }
        for (pattern, &argument) in call.parameters.iter().zip(&call.fact.arguments) {
            self.project_standalone(inference, argument, pattern, signature, &slots, &subs)?;
        }
        if subs.len() == binders.len()
            && call
                .fact
                .arguments
                .iter()
                .all(|a| inference.slots[a].ty.is_some())
        {
            if conversion_requirements == Some(false) {
                self.defer(module, node, ScalarUnsupported::Requirements)?;
                inference.slots.get_mut(&node).unwrap().blocked = true;
                return Ok(false);
            }
            if !self.check_call_requirements(
                inference,
                node,
                &call,
                &subs,
                conversion_requirements.is_some(),
            )? {
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_call_default(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        call: &mut PreparedCall<'g>,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
        solved: &[Substitution<'g>],
        input_positions: Option<&[bool]>,
    ) -> Work<bool> {
        let Some(header) = self.input.ordinary.unwrap().headers().get(call.fact.callee) else {
            return Ok(false);
        };
        inference.calls_waiting_for_result.remove(&node);
        let pending = if call.family {
            BTreeSet::new()
        } else {
            self.pending_conversion_outputs(call.fact.callee, binders, slots)?
        };
        let mut producers = Vec::new();
        for (pattern, &argument) in call.parameters.iter().zip(&call.fact.arguments) {
            self.source_producers(
                inference,
                argument,
                pattern,
                (binders, Some(call.fact.callee)),
                slots,
                &mut producers,
            )?;
        }
        let mut defaulted = BTreeSet::new();
        for default in &header.defaults {
            for binder in binders {
                self.elab.step(1)?;
                if *default.binder == binder.declaration.id {
                    self.elab.reserve(1)?;
                    defaulted.insert(binder.position);
                }
            }
        }
        let necessary = self.necessary_reads(&producers, slots, &defaulted)?;
        for (binder, value) in binders.iter().zip(slots) {
            self.elab.step(header.defaults.len() + 1)?;
            if value.is_some()
                || pending.contains(&binder.position)
                || input_positions.is_some_and(|p| !p[binder.position])
            {
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
            if self.default_is_ready(binder.position, &producers, &necessary)? {
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
        // A child standalone frame can wait on an enclosing immediate result
        // contract. Do not mark an unknown operation or evidence as such a read.
        if !call.family
            && inference.slots[&node].expected.is_none()
            && !producers.iter().any(|p| p.unavailable)
        {
            let mut result_slots = BTreeSet::new();
            self.open_slots(&call.result, binders, slots, &mut result_slots, 1)?;
            for position in result_slots {
                if defaulted.contains(&position) || pending.contains(&position) {
                    continue;
                }
                if self.default_is_ready(position, &producers, &necessary)? {
                    self.elab.reserve(1)?;
                    inference.calls_waiting_for_result.insert(node);
                    break;
                }
            }
        }
        Ok(false)
    }

    pub(super) fn source_leaf(&mut self, module: &Module, mut node: NodeId) -> Work<NodeId> {
        while module.syntax.nodes[node].kind == NodeKind::Group {
            self.elab.step(1)?;
            node = module.syntax.nodes[node].children[0];
        }
        Ok(node)
    }

    pub(super) fn source_equations(
        &mut self,
        inference: &Inference<'g>,
        call: &PreparedCall<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
    ) -> Work<bool> {
        let module = inference.module;
        self.elab.reserve(call.parameters.len())?;
        let mut work: Vec<_> = call
            .fact
            .arguments
            .iter()
            .copied()
            .zip(&call.parameters)
            .rev()
            .map(|(node, pattern)| (node, pattern, true))
            .collect();
        while let Some((node, pattern, outer)) = work.pop() {
            let node = self.source_leaf(module, node)?;
            let slot = &inference.slots[&node];
            if slot.blocked {
                return Ok(false);
            }
            let compatible = if let Some(actual) = &slot.ty {
                if primitive_name(actual) == Some("never") {
                    true
                } else if outer {
                    self.accept_input(pattern, actual, binders, slots, &call.fact.substitution)?
                } else {
                    // This is a fresh element, not covariance of a typed tuple.
                    self.accept_input(pattern, actual, binders, slots, &[])?
                }
            } else {
                let syntax = &module.syntax.nodes[node];
                match (pattern.as_ref(), syntax.kind) {
                    (TypeTerm::Tuple(types), NodeKind::Bracket) => {
                        if types.len() != syntax.children.len() {
                            // The ordinary bracket pass owns the conditional arity finding.
                            if call.family {
                                continue;
                            }
                            self.finding(module, node, ScalarFindingKind::Arity)?;
                            return Ok(false);
                        }
                        self.elab.reserve(types.len())?;
                        work.extend(
                            syntax
                                .children
                                .iter()
                                .copied()
                                .zip(types)
                                .map(|(n, t)| (n, t, false)),
                        );
                    }
                    (
                        TypeTerm::Callable {
                            parameters, result, ..
                        },
                        NodeKind::Arrow,
                    ) if !call.family => {
                        let head = &inference.arrows[&node];
                        if parameters.len() != head.parameters.len() {
                            self.finding(module, node, ScalarFindingKind::Arity)?;
                            return Ok(false);
                        } else {
                            for ((_, written), parameter) in head.parameters.iter().zip(parameters)
                            {
                                if let Some(written) = written
                                    && !self.input_equation(
                                        parameter,
                                        written,
                                        binders,
                                        slots,
                                        &[],
                                        1,
                                    )?
                                {
                                    self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                                    return Ok(false);
                                }
                            }
                            if let Some(written) = &head.result
                                && !self.input_equation(result, written, binders, slots, &[], 1)?
                            {
                                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                                return Ok(false);
                            }
                        }
                    }
                    _ => (),
                }
                true
            };
            if !compatible {
                if !call.family {
                    self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
                }
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn project_standalone(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        pattern: &Type<'g>,
        signature: &'r DeclarationFacts<'g>,
        slots: &[Option<Type<'g>>],
        subs: &[Substitution<'g>],
    ) -> Work<()> {
        let mut work = Vec::new();
        self.elab.reserve(1)?;
        work.push((node, pattern));
        while let Some((node, pattern)) = work.pop() {
            if self.closed(pattern, &signature.binders, &|i| slots[i].is_some(), 1)? {
                let receiving = self.substitute(pattern, subs, 1)?;
                self.establish(inference, node, receiving, true)?;
                continue;
            }
            let node = self.source_leaf(inference.module, node)?;
            let syntax = &inference.module.syntax.nodes[node];
            match (pattern.as_ref(), syntax.kind) {
                (TypeTerm::Tuple(types), NodeKind::Bracket)
                    if types.len() == syntax.children.len() =>
                {
                    self.elab.reserve(types.len())?;
                    work.extend(syntax.children.iter().copied().zip(types));
                }
                (_, NodeKind::Arrow | NodeKind::ValueReference) => {
                    if let Some(context) =
                        self.written_callable_context(signature, pattern, subs, node)?
                    {
                        self.project_callable_context(
                            inference,
                            node,
                            CallableContext {
                                parameters: context.parameters,
                                result: context.result,
                            },
                        )?;
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }

    pub(in crate::analysis::scalars) fn source_producers(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        pattern: &Type<'g>,
        scope: (&[Binder<'g>], Option<&DeclarationId>),
        slots: &[Option<Type<'g>>],
        producers: &mut Vec<Producer>,
    ) -> Work<()> {
        let (binders, contract) = scope;
        let module = inference.module;
        let mut work = Vec::new();
        self.elab.reserve(1)?;
        work.push((node, pattern));
        while let Some((node, pattern)) = work.pop() {
            let node = self.source_leaf(module, node)?;
            let slot = &inference.slots[&node];
            if slot.ty.is_some() {
                continue;
            }
            let syntax = &module.syntax.nodes[node];
            let mut producer = Producer {
                reads: BTreeSet::new(),
                writes: BTreeSet::new(),
                unavailable: slot.blocked,
            };
            match (pattern.as_ref(), syntax.kind) {
                (TypeTerm::Tuple(types), NodeKind::Bracket)
                    if types.len() == syntax.children.len() =>
                {
                    self.elab.reserve(types.len())?;
                    work.extend(syntax.children.iter().copied().zip(types));
                    continue;
                }
                (
                    TypeTerm::Callable {
                        parameters, result, ..
                    },
                    NodeKind::Arrow,
                ) => {
                    let Some(head) = inference.arrows.get(&node) else {
                        self.open_slots(pattern, binders, slots, &mut producer.writes, 1)?;
                        producer.unavailable = true;
                        if !producer.writes.is_empty() {
                            self.elab.reserve(1)?;
                            producers.push(producer);
                        }
                        continue;
                    };
                    for (parameter, (_, written)) in parameters.iter().zip(&head.parameters) {
                        if written.is_none() {
                            self.open_slots(parameter, binders, slots, &mut producer.reads, 1)?;
                        }
                    }
                    if head.reads_result {
                        self.open_slots(result, binders, slots, &mut producer.reads, 1)?;
                    }
                    if head.result.is_none() {
                        self.open_slots(result, binders, slots, &mut producer.writes, 1)?;
                    }
                }
                _ => {
                    self.open_slots(pattern, binders, slots, &mut producer.writes, 1)?;
                    if let Some(head) = inference.arrows.get(&node)
                        && let Some(metadata) =
                            contract.and_then(|id| self.input.ordinary.unwrap().headers().get(id))
                    {
                        let mut written = false;
                        for clause in &metadata.requirements {
                            self.elab.step(1)?;
                            if !super::ordinary_callbacks::is_callable_clause(clause.target) {
                                continue;
                            }
                            let [Some(subject), Some(inputs), Some(output)] =
                                clause.arguments.as_slice()
                            else {
                                continue;
                            };
                            if !self.same(subject, pattern)? {
                                continue;
                            }
                            let TypeTerm::Tuple(inputs) = inputs.as_ref() else {
                                continue;
                            };
                            written = true;
                            producer.unavailable |= inputs.len() != head.parameters.len();
                            for (parameter, (_, explicit)) in inputs.iter().zip(&head.parameters) {
                                if explicit.is_none() {
                                    self.open_slots(
                                        parameter,
                                        binders,
                                        slots,
                                        &mut producer.reads,
                                        1,
                                    )?;
                                }
                                self.open_slots(
                                    parameter,
                                    binders,
                                    slots,
                                    &mut producer.writes,
                                    1,
                                )?;
                            }
                            if head.reads_result {
                                self.open_slots(output, binders, slots, &mut producer.reads, 1)?;
                            }
                            self.open_slots(output, binders, slots, &mut producer.writes, 1)?;
                        }
                        if written {
                            if !producer.writes.is_empty() {
                                self.elab.reserve(1)?;
                                producers.push(producer);
                            }
                            continue;
                        }
                    }
                    let waits_for_shape = inference.arrows.get(&node).is_some_and(|head| {
                        head.reads_result || head.parameters.iter().any(|(_, ty)| ty.is_none())
                    });
                    if self.needs_numeric_width(module, node, 1)?
                        || (waits_for_shape || inference.calls_waiting_for_result.contains(&node))
                            && self.call_slot(pattern, binders)?.is_some()
                    {
                        self.open_slots(pattern, binders, slots, &mut producer.reads, 1)?;
                    } else if let Some(frame) = inference.nominals.get(&node)
                        && let Some(arguments) = frame.receiving_arguments(pattern)
                    {
                        for &i in &frame.receiving_reads {
                            self.open_slots(&arguments[i], binders, slots, &mut producer.reads, 1)?;
                        }
                        producer.unavailable |= producer.reads.is_empty();
                    } else {
                        // Unknown source/evidence is not a proof of a mandatory read.
                        producer.unavailable = true;
                    }
                }
            }
            if !producer.writes.is_empty() {
                self.elab.reserve(1)?;
                producers.push(producer);
            }
        }
        Ok(())
    }

    pub(super) fn open_slots(
        &mut self,
        term: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &[Option<Type<'g>>],
        out: &mut BTreeSet<usize>,
        depth: usize,
    ) -> Work<()> {
        self.equation_depth(depth)?;
        if let Some(i) = self.call_slot(term, binders)? {
            if slots[i].is_none() && !out.contains(&i) {
                self.elab.reserve(1)?;
                out.insert(i);
            }
        } else {
            for child in shape(term).1 {
                self.open_slots(child, binders, slots, out, depth + 1)?;
            }
        }
        Ok(())
    }

    pub(in crate::analysis::scalars) fn default_is_ready(
        &mut self,
        position: usize,
        producers: &[Producer],
        necessary: &[BTreeSet<usize>],
    ) -> Work<bool> {
        for producer in producers {
            self.elab.step(1 + producer.reads.len())?;
            if producer.writes.contains(&position)
                && (producer.unavailable
                    || !producer
                        .reads
                        .iter()
                        .any(|&i| necessary[i].contains(&position)))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(in crate::analysis::scalars) fn necessary_reads(
        &mut self,
        producers: &[Producer],
        slots: &[Option<Type<'g>>],
        defaulted: &BTreeSet<usize>,
    ) -> Work<Vec<BTreeSet<usize>>> {
        self.elab.reserve(slots.len() * 2)?;
        let mut reads: Vec<_> = slots
            .iter()
            .enumerate()
            .map(|(i, v)| {
                if v.is_none() {
                    BTreeSet::from([i])
                } else {
                    BTreeSet::new()
                }
            })
            .collect();
        // A prerequisite may be filled by any producer, or by a written
        // default. Only a read common to every producer is mandatory.
        loop {
            let mut changed = false;
            for i in 0..slots.len() {
                if slots[i].is_some() || defaulted.contains(&i) {
                    continue;
                }
                for (required, slot) in slots.iter().enumerate() {
                    self.elab.step(1)?;
                    if reads[i].contains(&required) || slot.is_some() {
                        continue;
                    }
                    let mut some = false;
                    let mut all = true;
                    for producer in producers {
                        self.elab.step(1 + producer.reads.len())?;
                        if producer.writes.contains(&i) {
                            some = true;
                            all &= !producer.unavailable
                                && producer.reads.iter().any(|&r| reads[r].contains(&required));
                        }
                    }
                    if some && all {
                        self.elab.reserve(1)?;
                        reads[i].insert(required);
                        changed = true;
                    }
                }
            }
            if !changed {
                return Ok(reads);
            }
        }
    }

    pub(in crate::analysis::scalars) fn mode_admits(
        actual: InvocationMode,
        required: InvocationMode,
    ) -> bool {
        actual == required || actual == InvocationMode::ReadOnly || required == InvocationMode::Once
    }

    pub(in crate::analysis::scalars) fn slot_substitutions(
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

    pub(super) fn equation_depth(&mut self, depth: usize) -> Work<()> {
        if depth > self.elab.limits.max_depth.min(64) {
            return Err(());
        }
        self.elab.statistics.max_depth = self.elab.statistics.max_depth.max(depth);
        self.elab.step(1)
    }

    pub(in crate::analysis::scalars) fn closed(
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

    pub(in crate::analysis::scalars) fn accept_input(
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
            (
                TypeTerm::Callable {
                    mode: pm,
                    parameters: pp,
                    result: pr,
                },
                TypeTerm::Callable {
                    mode: am,
                    parameters: ap,
                    result: ar,
                },
            ) if Self::mode_admits(*am, *pm) && pp.len() == ap.len() => {
                for (p, a) in pp.iter().zip(ap) {
                    if !self.input_equation(p, a, binders, slots, fixed, 2)? {
                        return Ok(false);
                    }
                }
                self.input_equation(pr, ar, binders, slots, fixed, 2)
            }
            _ => self.input_equation(pattern, actual, binders, slots, fixed, 1),
        }
    }

    pub(super) fn input_equation(
        &mut self,
        pattern: &Type<'g>,
        actual: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
        fixed: &[Substitution<'g>],
        depth: usize,
    ) -> Work<bool> {
        self.equate(
            pattern,
            actual,
            binders,
            slots,
            Equation::Input(fixed),
            depth,
        )
    }

    pub(super) fn free_output_equation(
        &mut self,
        pattern: &Type<'g>,
        actual: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
        pending: &BTreeSet<usize>,
    ) -> Work<bool> {
        if self.closed(pattern, binders, &|i| slots[i].is_some(), 1)? {
            return Ok(false);
        }
        self.elab.reserve(slots.len())?;
        let mut proposed = slots.to_vec();
        if !self.equate(
            pattern,
            actual,
            binders,
            &mut proposed,
            Equation::FreeOutput(pending),
            1,
        )? {
            return Ok(false);
        }
        self.elab.step(slots.len())?;
        let changed = slots
            .iter()
            .zip(&proposed)
            .any(|(old, new)| old.is_none() && new.is_some());
        slots.clone_from_slice(&proposed);
        Ok(changed)
    }

    fn equate(
        &mut self,
        pattern: &Type<'g>,
        actual: &Type<'g>,
        binders: &[Binder<'g>],
        slots: &mut [Option<Type<'g>>],
        equation: Equation<'_, 'g>,
        depth: usize,
    ) -> Work<bool> {
        self.equation_depth(depth)?;
        if let Some(i) = self.call_slot(pattern, binders)? {
            if let Some(established) = &slots[i] {
                if let Equation::Input(fixed) = equation {
                    self.elab.step(fixed.len())?;
                    if depth == 1
                        && fixed.iter().any(|s| s.position == i)
                        && (self.data_admission(actual, established)?
                            || self.callable_admission(actual, established)?)
                    {
                        return Ok(true);
                    }
                }
                return self.same(established, actual);
            }
            // The source term has its caller's rigid binders. Leave pending
            // outputs unbound, even beside genuinely free leaves in a tuple.
            if !matches!(equation, Equation::FreeOutput(pending) if pending.contains(&i)) {
                slots[i] = Some(actual.clone());
            }
            return Ok(true);
        }
        let (p, ps) = shape(pattern);
        let (a, ac) = shape(actual);
        if p != a || ps.len() != ac.len() {
            return Ok(false);
        }
        for (p, a) in ps.zip(ac) {
            if !self.equate(p, a, binders, slots, equation, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
