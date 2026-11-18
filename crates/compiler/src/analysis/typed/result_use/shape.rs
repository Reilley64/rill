//! Instantiate real stored members, separating inhabited enum alternatives.
use super::*;
use crate::analysis::scalars::data::Constructor;

struct Seed<'g> {
    root: Root<'g>,
    origin: NodeId,
    path: Vec<ResultComponent<'g>>,
}

struct Arguments<'a, 'g> {
    nominal: &'g DeclarationId,
    binders: &'a [Binder<'g>],
    types: &'a [Type<'g>],
    outer: Option<&'a Arguments<'a, 'g>>,
}

impl<'a, 'g> Check<'a, 'g> {
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
            },
            1,
        )
    }

    fn shape(
        &mut self,
        mut states: Alternatives<'g>,
        ty: &Type<'g>,
        env: Option<&Arguments<'_, 'g>>,
        seed: &mut Seed<'g>,
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
            TypeTerm::Nominal {
                declaration,
                arguments,
            } if Some(*declaration) != self.core.opaque_cause
                && Some(*declaration) != self.core.opaque_list =>
            {
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
                if let Some(facts) = self.inventory.type_facts().declarations.get(*declaration) {
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
        seed: &Seed<'g>,
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
        seed: &mut Seed<'g>,
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
