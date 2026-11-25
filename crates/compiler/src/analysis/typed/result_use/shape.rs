//! Instantiate real stored members, separating inhabited enum alternatives.
use super::*;
use crate::analysis::scalars::data::Constructor;

struct Seed<'a, 'g> {
    root: Root<'g>,
    origin: NodeId,
    path: Vec<ResultComponent<'g>>,
    substitution: Option<&'a Substitutions<'a, 'g>>,
}

struct Arguments<'a, 'g> {
    nominal: &'g DeclarationId,
    binders: &'a [Binder<'g>],
    types: &'a [Type<'g>],
    outer: Option<&'a Arguments<'a, 'g>>,
}

pub(super) fn text_option(ty: &Type<'_>, option: &DeclarationId) -> bool {
    matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
        if std::ptr::eq(*declaration, option)
            && matches!(arguments.as_slice(), [payload]
                if matches!(payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "string")))
}

impl<'a, 'g> Check<'a, 'g> {
    pub(super) fn text_option_list(&self, ty: &Type<'g>) -> bool {
        let (Some(option), Some(list)) = (self.core.option, self.core.collections.list) else {
            return false;
        };
        matches!(ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
            if std::ptr::eq(*declaration, &list.declaration.id)
                && matches!(arguments.as_slice(), [element] if text_option(element, option.declaration)))
            && self.core.collections.representations().next().is_some()
    }

    pub(super) fn seed_parameter(
        &mut self,
        mut states: Alternatives<'g>,
        parameter: &MemberType<'g>,
        checked: bool,
    ) -> Work<Alternatives<'g>> {
        let id = &parameter.declaration.id;
        if checked && self.text_option_list(&parameter.ty) {
            // The admitted List representation stores only Option<string>.
            // No containment probe or item-tag split is needed at entry.
            self.depth(1)?;
            for state in &mut states {
                state.add_option_list(id, self.budget)?;
            }
            return Ok(states);
        }
        self.seed(
            states,
            Root::Local(id),
            parameter.declaration.node,
            &parameter.ty,
        )
    }

    pub(super) fn seed(
        &mut self,
        states: Alternatives<'g>,
        root: Root<'g>,
        origin: NodeId,
        ty: &Type<'g>,
    ) -> Work<Alternatives<'g>> {
        self.shape(
            states,
            ty,
            None,
            &mut Seed {
                root,
                origin,
                path: Vec::new(),
                substitution: self.substitution,
            },
            1,
        )
    }

    // Opaque backing owns zero or more occurrences; the element type can
    // rule out Result, but does not establish a count or a specific origin.
    fn opaque_elements(
        &mut self,
        states: Alternatives<'g>,
        element: &Type<'g>,
        env: Option<&Arguments<'_, 'g>>,
        seed: &mut Seed<'_, 'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        let probe = self.singleton(Residence::default())?;
        for state in self.shape(probe, element, env, seed, depth + 1)? {
            if state.may_owe_result(self.budget)? {
                return self.unknown_shape(states, seed);
            }
        }
        Ok(states)
    }

    fn shape(
        &mut self,
        mut states: Alternatives<'g>,
        ty: &Type<'g>,
        env: Option<&Arguments<'_, 'g>>,
        seed: &mut Seed<'_, 'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        self.depth(depth)?;
        if states.is_empty() {
            return Ok(states);
        }
        let (ty, selected) = match ty.as_ref() {
            TypeTerm::Variant {
                parent,
                declaration,
            } => (parent, Some(*declaration)),
            _ => (ty, None),
        };
        match ty.as_ref() {
            TypeTerm::Primitive(p) if p.name() == "never" => return Ok(Vec::new()),
            TypeTerm::Primitive(_) | TypeTerm::Reference { .. } => return Ok(states),
            TypeTerm::Parameter {
                declaration,
                position,
            } => {
                let mut frame = env;
                while let Some(arguments) = frame {
                    self.budget.step(1)?;
                    if arguments
                        .binders
                        .get(*position)
                        .is_some_and(|b| b.declaration.id == **declaration)
                    {
                        return self.shape(
                            states,
                            &arguments.types[*position],
                            arguments.outer,
                            seed,
                            depth + 1,
                        );
                    }
                    frame = arguments.outer;
                }
                let mut frame = seed.substitution;
                while let Some(arguments) = frame {
                    self.budget.step(1)?;
                    if let Some(argument) = arguments.arguments.get(*position)
                        && argument.binder == *declaration
                        && argument.position == *position
                    {
                        let previous = seed.substitution;
                        seed.substitution = arguments.outer;
                        let result = self.shape(states, &argument.argument, env, seed, depth + 1);
                        seed.substitution = previous;
                        return result;
                    }
                    frame = arguments.outer;
                }
            }
            TypeTerm::Tuple(types) => {
                for (index, ty) in types.iter().enumerate() {
                    self.budget.reserve(1)?;
                    seed.path.push(ResultComponent::TupleElement(index));
                    states = self.shape(states, ty, env, seed, depth + 1)?;
                    seed.path.pop();
                }
                return Ok(states);
            }
            TypeTerm::Nominal { declaration, .. }
                if Some(*declaration) == self.core.owned_bytes =>
            {
                // The linked buffer stores u8 values; the empty source seal
                // neither contains Results nor makes Bytes uninhabited.
                return Ok(states);
            }
            TypeTerm::Nominal {
                declaration,
                arguments,
            } if Some(*declaration) != self.core.opaque_cause => {
                use crate::analysis::typed::CollectionStorage;
                for repr in self.core.collections.representations() {
                    self.budget.step(1)?;
                    if repr.declaration.id == **declaration {
                        return match (repr.storage, arguments.as_slice()) {
                            (CollectionStorage::BorrowedList { .. }, [_]) => Ok(states),
                            (_, [element]) => {
                                self.opaque_elements(states, element, env, seed, depth)
                            }
                            _ => self.unknown_shape(states, seed),
                        };
                    }
                }
                for repr in &self.core.native.representations {
                    self.budget.step(1)?;
                    if repr.owner.id == **declaration {
                        return match arguments.as_slice() {
                            [payload] => self.opaque_elements(states, payload, env, seed, depth),
                            _ => self.unknown_shape(states, seed),
                        };
                    }
                }
                for scope in &self.core.native.protection {
                    self.budget.step(1)?;
                    if scope.owner.id == **declaration {
                        // The owner is validated, but its provider archive did not
                        // establish the stored shape. Don't inspect the seal.
                        return self.unknown_shape(states, seed);
                    }
                }
                self.budget.step(1)?;
                if *declaration == self.core.result.declaration {
                    for state in &mut states {
                        self.add_source(state, seed.root, seed.origin, &seed.path)?;
                    }
                }
                let mut frame = env;
                while let Some(ancestor) = frame {
                    self.budget.step(1)?;
                    if ancestor.nominal == *declaration {
                        return self.unknown_shape(states, seed);
                    }
                    frame = ancestor.outer;
                }
                let declarations = &self.inventory.type_facts().declarations;
                if let Some(facts) = declarations.get(declaration) {
                    let env = Arguments {
                        nominal: declaration,
                        binders: &facts.binders,
                        types: arguments,
                        outer: env,
                    };
                    match &facts.shape {
                        DeclarationShape::Record { fields } => {
                            return self.seed_fields(states, fields, &env, seed, depth);
                        }
                        DeclarationShape::Enum { variants } => {
                            let mut result = Vec::new();
                            for variant in variants {
                                self.budget.step(1)?;
                                let tag = &variant.declaration.id;
                                if selected.is_some_and(|selected| selected != tag) {
                                    continue;
                                }
                                self.budget.reserve(1)?;
                                let constructor = Arc::new(Constructor::Variant(tag));
                                let mut branch = Vec::new();
                                for state in &states {
                                    let mut state = state.fork(self.budget)?;
                                    state.store(
                                        seed.root,
                                        &seed.path,
                                        Content::Test(constructor.clone(), true),
                                        self.budget,
                                    )?;
                                    self.budget.reserve(1)?;
                                    branch.push(state);
                                }
                                match &variant.payload {
                                    Payload::Unit => {}
                                    Payload::Record(fields) => {
                                        branch =
                                            self.seed_fields(branch, fields, &env, seed, depth)?
                                    }
                                    Payload::Tuple(types) => {
                                        for (index, ty) in types.iter().enumerate() {
                                            self.budget.reserve(1)?;
                                            seed.path.push(ResultComponent::VariantPayload {
                                                variant: tag,
                                                index,
                                            });
                                            branch = self.shape(
                                                branch,
                                                ty,
                                                Some(&env),
                                                seed,
                                                depth + 1,
                                            )?;
                                            seed.path.pop();
                                        }
                                    }
                                }
                                result.extend(branch);
                            }
                            return Ok(result);
                        }
                        DeclarationShape::Function { .. } => {}
                    }
                }
            }
            _ => {}
        }
        self.unknown_shape(states, seed)
    }

    fn unknown_shape(
        &mut self,
        mut states: Alternatives<'g>,
        seed: &Seed<'_, 'g>,
    ) -> Work<Alternatives<'g>> {
        for state in &mut states {
            state.store(
                seed.root,
                &seed.path,
                Content::Unclassified(seed.origin),
                self.budget,
            )?;
        }
        Ok(states)
    }

    fn seed_fields(
        &mut self,
        mut states: Alternatives<'g>,
        fields: &[MemberType<'g>],
        env: &Arguments<'_, 'g>,
        seed: &mut Seed<'_, 'g>,
        depth: usize,
    ) -> Work<Alternatives<'g>> {
        for field in fields {
            self.budget.reserve(1)?;
            seed.path
                .push(ResultComponent::Field(&field.declaration.id));
            states = self.shape(states, &field.ty, Some(env), seed, depth + 1)?;
            seed.path.pop();
        }
        Ok(states)
    }
}
