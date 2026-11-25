//! A source combinator can defer a callback boundary, not assume it checked.
use super::*;
use crate::analysis::InvocationMode;
use crate::analysis::scalars::CallFacts;
use crate::analysis::scalars::generics::callables::InvocationFacts;
use crate::loader::Origin;

// Each proved entry states which input variants require a checked actual callback.
pub(super) type CallbackContract<'g> =
    BTreeMap<&'g DeclarationId, (CallbackEntry<'g>, Option<[bool; 2]>)>;

#[derive(Clone, Copy)]
pub(super) struct CallbackEntry<'g> {
    container: Container<'g>,
    input: &'g crate::loader::Declaration,
    callback: &'g crate::loader::Declaration,
    binder: &'g DeclarationId,
    position: usize,
}

pub(super) struct Substitutions<'a, 'g> {
    pub(super) arguments: &'a [Substitution<'g>],
    pub(super) outer: Option<&'a Self>,
}

impl<'g> CallbackEntry<'g> {
    pub(super) fn from_signature(
        signature: &DeclarationFacts<'g>,
        core: Inputs<'_, 'g>,
    ) -> Option<Self> {
        let DeclarationShape::Function { parameters, .. } = &signature.shape else {
            return None;
        };
        let [input, callback] = parameters.as_slice() else {
            return None;
        };
        let TypeTerm::Nominal { declaration, .. } = input.ty.as_ref() else {
            return None;
        };
        let TypeTerm::Parameter {
            declaration: binder,
            position,
        } = callback.ty.as_ref()
        else {
            return None;
        };
        // Limit this proof procedure to the admitted container modules and this
        // signature shape. Neither identity nor signature proves the summary.
        let container = [Some(core.result), core.option]
            .into_iter()
            .flatten()
            .find(|role| {
                *declaration == role.declaration
                    && signature.declaration.id.module == role.declaration.module
            })?;
        (signature.binders.get(*position)?.declaration.id == **binder).then_some(Self {
            container,
            input: input.declaration,
            callback: callback.declaration,
            binder,
            position: *position,
        })
    }
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn signature(&self, id: &DeclarationId) -> Option<&'a DeclarationFacts<'g>> {
        self.inventory
            .type_facts()
            .declarations
            .get(id)
            .or_else(|| self.implementation_headers.get(id).copied())
    }

    pub(super) fn function_body(
        &mut self,
        signature: &'a DeclarationFacts<'g>,
        depth: usize,
    ) -> Work<()> {
        self.depth(depth)?;
        let node = signature.declaration.node;
        let Some(&body) = self.module.syntax.nodes[node].children.get(1) else {
            return self.pending(node, ResultUseBlocker::CallBoundary);
        };
        let DeclarationShape::Function { parameters, .. } = &signature.shape else {
            return self.pending(node, ResultUseBlocker::TypedInput);
        };
        let previous = std::mem::replace(&mut self.current_parameters, parameters);
        self.budget.step(1)?;
        let checked = self.checked.contains(&signature.declaration.id);
        let previous_once = self.once_return.take();
        if checked
            && signature.declaration.id.kind == DeclarationKind::Function
            && signature.declaration.scope.is_none()
            && signature.binders.is_empty()
            && let Some(&statement) = self.module.syntax.nodes[body].children.last()
            && self.module.syntax.nodes[statement].kind == NodeKind::Return
        {
            // Exclude detached lexical arrow bodies and all other nested call
            // contexts. This route starts at a checked caller's direct return.
            self.once_return = self.module.syntax.nodes[statement]
                .children
                .first()
                .copied();
        }
        let nested_write_body = (signature.declaration.id.kind == DeclarationKind::Function
            && signature.binders.is_empty()
            && checked)
            .then_some(body);
        let previous_body = std::mem::replace(&mut self.nested_write_body, nested_write_body);
        let track_iterator = nested_write_body.is_some()
            && self.module.key.origin == Origin::Application
            && !self.proving_callable;
        let previous_iterator = std::mem::replace(
            &mut self.option_iterator,
            // Body block, direct statement, then the `for_loop` visitor.
            track_iterator.then_some((depth + 3, None)),
        );
        let (pending, discards) = (self.report.pending.len(), self.report.discards.len());
        let mut states = self.singleton(Residence::default())?;
        let mut option_formals = 0;
        for parameter in parameters {
            let id = &parameter.declaration.id;
            if track_iterator {
                if self.text_option_list(&parameter.ty) {
                    option_formals += 1;
                } else if let Some(list) = self.core.collections.list
                    && let TypeTerm::Nominal {
                        declaration,
                        arguments,
                    } = parameter.ty.as_ref()
                    && std::ptr::eq(*declaration, &list.declaration.id)
                    && !matches!(arguments.as_slice(), [element] if matches!(element.as_ref(), TypeTerm::Primitive(p)
                        if p.name() == "i32" || (p.name() == "string" && parameter.text_iteration_source)))
                {
                    // The function cannot open this unproved sibling backing.
                    // Do not grant earlier Option-list formals partial admission.
                    self.option_iterator = None;
                }
            }
            let root = Root::Local(id);
            states = self.seed_parameter(states, parameter, checked)?;
            if retained_bool_contents_type(&parameter.ty).is_some() {
                for state in &mut states {
                    state.store(root, &[], Content::CallerBoolOrigin(id), self.budget)?;
                }
            }
        }
        if option_formals != 1 {
            self.option_iterator = None;
        }
        if states.is_empty() {
            self.dead_statement(body, Residence::default(), depth + 1)?;
        }
        for state in states {
            if let Some(entry) = self.parametric {
                self.input_variant = state.test(
                    Root::Local(&entry.input.id),
                    &[],
                    &Constructor::Variant(entry.container.success),
                    self.budget,
                )?;
            }
            for state in self.statement(body, state, depth + 1)? {
                self.exit(state, body)?;
            }
        }
        let iterator = std::mem::replace(&mut self.option_iterator, previous_iterator);
        if let Some((_, Some((formal, site)))) = iterator
            && self.report.pending.len() == pending
            && self.report.discards.len() == discards
        {
            // This is a consumer of checked Result-use facts, not an extra
            // route for Result-use eligibility.
            self.budget.reserve(1)?;
            self.report
                .fresh_option_iterators
                .push((&signature.declaration.id, formal, site));
        }
        self.current_parameters = previous;
        self.nested_write_body = previous_body;
        self.once_return = previous_once;
        Ok(())
    }

    pub(super) fn symbolic_callback(
        &mut self,
        invocation: &InvocationFacts<'g>,
        receiver: &[Located<'g>],
    ) -> Work<bool> {
        let Some(entry) = self.parametric else {
            return Ok(false);
        };
        if invocation.assumption.is_none()
            || invocation.mode != InvocationMode::Once
            || !invocation
                .path
                .as_ref()
                .is_some_and(|path| *path.root == entry.callback.id && path.projections.is_empty())
        {
            return Ok(false);
        }
        // A lexical check of a skipped arm is not an invocation of the entry value.
        if self.dead_depth != 0 {
            return Ok(true);
        }
        if !matches!(receiver, [value] if value.component.is_empty() && value.content == Content::Unclassified(entry.callback.node))
        {
            return Ok(false);
        }
        if let Some(success) = self.input_variant {
            self.invoked_variants[usize::from(success)] = true;
        } else {
            self.invoked_variants = [true; 2];
        }
        Ok(true)
    }

    pub(super) fn ordinary_boundary(
        &mut self,
        call: &CallFacts<'g>,
        state: &Residence<'g>,
        depth: usize,
    ) -> Work<bool> {
        if let Some(&(entry, proof)) = self.contracts.get(call.callee) {
            let Some(variants) = proof else {
                return Ok(false);
            };
            let [input, callback] = call.arguments.as_slice() else {
                return Ok(false);
            };
            let success = state.test(
                Root::Temporary(*input),
                &[],
                &Constructor::Variant(entry.container.success),
                self.budget,
            )?;
            if !success.map_or(variants[0] || variants[1], |tag| variants[usize::from(tag)]) {
                return Ok(true);
            }
            if !call.substitution.get(entry.position).is_some_and(|s| {
                s.binder == entry.binder
                    && s.position == entry.position
                    && matches!(s.argument.as_ref(), TypeTerm::Callable { .. })
            }) {
                return Ok(false);
            }
            return self.prove_at(
                state,
                &(Root::Temporary(*callback), Vec::new()),
                InvocationMode::Once,
                depth,
            );
        }
        let (target, substitution) = call
            .selection
            .as_ref()
            .map_or((call.callee, call.substitution.as_slice()), |selected| {
                (selected.implementation, selected.substitution.as_slice())
            });
        if !self.checked.contains(target) {
            return Ok(false);
        }
        if self.proving_callable || call.selection.is_some() {
            for argument in &call.arguments {
                self.budget.step(1)?;
                if state.owes_under(Root::Temporary(*argument), self.budget)? {
                    return self.prove_named(target, substitution, depth);
                }
            }
        }
        Ok(true)
    }

    pub(super) fn prove_at(
        &mut self,
        state: &Residence<'g>,
        address: &Address<'g>,
        mode: InvocationMode,
        depth: usize,
    ) -> Work<bool> {
        let values = state.contents(address.0, &address.1, self.budget)?;
        self.prove_actual(values, mode, depth)
    }

    pub(super) fn prove_closed_actual(
        &mut self,
        state: &Residence<'g>,
        actual: once::Actual,
        depth: usize,
    ) -> Work<bool> {
        let argument = match actual {
            once::Actual::Moved(argument) => argument,
            once::Actual::Lent(argument) => return self.prove_lent_arrow(state, argument, depth),
        };
        let values = state.contents(Root::Temporary(argument), &[], self.budget)?;
        // This route admits only the direct pair: don't substitute an empty
        // entry for an unknown, borrowed, or Result-bearing residence.
        if !matches!(values.as_slice(), [value] if value.component.is_empty()
            && value.content == Content::Callable(argument))
        {
            return Ok(false);
        }
        self.prove_actual(values, InvocationMode::Once, depth)
    }

    /// Read the referent residence of `&local` in place. Its owner and every
    /// capture occurrence stay with the caller.
    fn prove_lent_arrow(
        &mut self,
        state: &Residence<'g>,
        argument: NodeId,
        depth: usize,
    ) -> Work<bool> {
        match self.lent_descriptor(state, argument)? {
            Some(values) => self.prove_actual(values, InvocationMode::ReadOnly, depth),
            None => Ok(false),
        }
    }

    /// The bare read-only arrow behind `&local`, or `None`. A capture occurrence,
    /// unknown content or possible alias is not an empty environment.
    pub(super) fn lent_descriptor(
        &mut self,
        state: &Residence<'g>,
        argument: NodeId,
    ) -> Work<Option<Vec<Located<'g>>>> {
        let Some((owner @ Root::Local(id), component)) =
            state.alias(Root::Temporary(argument), &[], self.budget)?
        else {
            return Ok(None);
        };
        if id.kind != DeclarationKind::Binding || !component.is_empty() {
            return Ok(None);
        }
        let values = state.contents(owner, &[], self.budget)?;
        let [value] = values.as_slice() else {
            return Ok(None);
        };
        let Content::Callable(origin) = value.content else {
            return Ok(None);
        };
        let read_only = self
            .facts(origin)?
            .and_then(|f| f.callable.as_ref()?.arrow.as_deref())
            .is_some_and(|arrow| arrow.mode == Some(InvocationMode::ReadOnly));
        Ok((value.component.is_empty() && read_only).then_some(values))
    }

    pub(super) fn prove_actual(
        &mut self,
        values: Vec<Located<'g>>,
        mode: InvocationMode,
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
        let mut origin = None;
        let mut unknown_environment = false;
        let mut entry = Residence::default();
        for mut value in values {
            self.budget.step(1)?;
            match value.component.as_slice() {
                [] => match value.content {
                    Content::Callable(node) if origin.is_none() => origin = Some(node),
                    Content::Callable(_) => return Ok(false),
                    _ => unknown_environment = true,
                },
                [ResultComponent::Capture(source), ..] if mode == InvocationMode::Once => {
                    let source = *source;
                    self.budget.step(value.component.len())?;
                    value.component.remove(0);
                    self.budget.reserve(1)?;
                    entry.install(vec![value], Root::Local(source), &[], self.budget)?;
                }
                _ => unknown_environment = true,
            }
        }
        let Some(origin) = origin else {
            return Ok(false);
        };
        let Some(callable) = self.facts(origin)?.and_then(|f| f.callable.as_ref()) else {
            return Ok(false);
        };
        if let Some(named) = &callable.function_value {
            let (target, substitution) = named.selection.as_ref().map_or(
                (named.declaration, named.substitution.as_slice()),
                |selected| (selected.implementation, selected.substitution.as_slice()),
            );
            return Ok(!unknown_environment && self.prove_named(target, substitution, depth + 1)?);
        }
        let Some(arrow) = callable
            .arrow
            .as_ref()
            .filter(|a| a.premise.is_none() && a.site.node == origin)
        else {
            return Ok(false);
        };
        let mut proof = self.callable_check(self.module, self.table)?;
        proof.arrow_body(origin, arrow, entry, depth + 1)?;
        Ok(!unknown_environment && arrow.mode.is_some() && !proof.has_pending)
    }

    fn prove_named(
        &mut self,
        declaration: &'g DeclarationId,
        arguments: &[Substitution<'g>],
        depth: usize,
    ) -> Work<bool> {
        self.depth(depth)?;
        if !self.checked.contains(declaration)
            || self.contracts.contains_key(declaration)
            || self.pairs.contains_key(declaration)
        {
            return Ok(false);
        }
        let Some(signature) = self.signature(declaration) else {
            return Ok(false);
        };
        self.budget.step(signature.binders.len())?;
        if signature.binders.len() != arguments.len()
            || !signature
                .binders
                .iter()
                .zip(arguments)
                .all(|(b, s)| b.position == s.position && b.declaration.id == *s.binder)
        {
            return Ok(false);
        }
        let modules = self.inventory.graph().modules();
        self.budget.step(modules.len())?;
        let Some(index) = modules.iter().position(|m| m.key == declaration.module) else {
            return Ok(false);
        };
        let (module, table) = (&modules[index], &self.nodes.tables[index]);
        let substitution = Substitutions {
            arguments,
            outer: self.substitution,
        };
        let mut proof = self.callable_check(module, table)?;
        proof.substitution = Some(&substitution);
        proof.function_body(signature, depth + 1)?;
        Ok(!proof.has_pending)
    }

    pub(super) fn callable_check(
        &mut self,
        module: &'g Module,
        table: &'a NodeTable<'a, 'g>,
    ) -> Work<Check<'_, 'g>> {
        let sources = Sources {
            inventory: self.inventory,
            bodies: self.bodies,
            pairs: self.pairs,
            scoped: self.scoped,
            nodes: self.nodes,
            checked: self.checked,
            variants: self.variants,
            implementation_headers: self.implementation_headers,
            release_parameters: self.release_parameters,
            core: self.core,
        };
        let mut check = Check::new(
            sources,
            module,
            table,
            self.contracts,
            self.budget,
            self.report,
        )?;
        check.substitution = self.substitution;
        check.proving_callable = true;
        Ok(check)
    }
}
