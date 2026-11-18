//! M4.4 data extensions over the existing scalar/control engine.
use super::*;
mod matches;
pub(super) mod nominal;
mod patterns;
mod places;
pub use matches::{Arm, Match};
pub use patterns::{
    Access, Constructor, Coverage, CoverageStep, PatternBinder, PatternFacts, PatternProjection,
};
pub use places::{PathProjection, PlacePath, Refinement};

#[derive(Clone, Debug)]
pub struct FieldValue<'g> {
    pub field: &'g Declaration,
    pub expression: NodeId,
}
#[derive(Clone, Debug)]
pub struct Construction<'g> {
    pub ty: Type<'g>,
    pub fields: Vec<FieldValue<'g>>,
    pub arguments: Vec<NodeId>,
    pub base: Option<NodeId>,
    pub substitution: Vec<Substitution<'g>>,
    pub argument_sources: Vec<ArgumentSource<'g>>,
    pub bindings: Vec<&'g Binding>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentOrigin {
    Written,
    Expected,
    RebuildBase,
    Member,
    Default,
}
#[derive(Clone, Debug)]
pub struct ArgumentSource<'g> {
    pub position: usize,
    pub site: Site<'g>,
    pub origin: ArgumentOrigin,
}
#[derive(Clone, Debug)]
pub struct Projection<'g> {
    pub field: &'g Declaration,
    pub base: NodeId,
}
#[derive(Clone, Debug)]
pub struct Admission<'g> {
    pub actual: Type<'g>,
    pub expected: Type<'g>,
}
#[derive(Clone, Debug)]
pub enum DataExpression<'g> {
    Construction(Box<Construction<'g>>),
    Projection(Projection<'g>),
    Tuple {
        elements: Vec<NodeId>,
    },
    TupleIndex {
        base: NodeId,
        selector: NodeId,
        component: usize,
    },
    Match(Box<Match<'g>>),
    Propagation(Box<Propagation<'g>>),
}
#[derive(Debug)]
pub struct DataReceipt<'r, 'g>(ScalarReceipt<'r, 'g>);
impl<'g> DataReceipt<'_, 'g> {
    pub fn context(&self) -> GraphId {
        self.0.context()
    }
    pub fn statistics(&self) -> Statistics {
        self.0.statistics()
    }
    pub fn not_performed(&self) -> &'static [Stage] {
        self.0.not_performed()
    }
    pub fn body(
        &self,
        module: &Module,
        owner: BodyOwner<'_>,
    ) -> Result<Option<&BodyFacts<'g>>, QueryError> {
        self.0.body(module, owner)
    }
    pub fn node(
        &self,
        module: &Module,
        node: NodeId,
    ) -> Result<Option<&NodeFacts<'g>>, QueryError> {
        self.0.node(module, node)
    }
}
pub fn check_data<'r, 'g>(
    input: &DeclarationReceipt<'r, 'g>,
    limits: AnalysisLimits,
) -> Result<DataReceipt<'r, 'g>, ScalarFailure<'g>> {
    check(input, limits, true).map(DataReceipt)
}
pub(super) struct PreparedData<'g> {
    pub ty: Type<'g>,
    pub flow: Option<Flow>,
    pub contexts: Vec<(NodeId, Type<'g>)>,
    pub inputs: Vec<NodeId>,
    pub fact: DataExpression<'g>,
}
struct RecordInitializers<'g> {
    fields: Vec<FieldValue<'g>>,
    contexts: Vec<(NodeId, Type<'g>)>,
}

impl<'r, 'g> Checker<'r, 'g> {
    fn nominal_substitution(
        &mut self,
        binders: &[Binder<'g>],
        arguments: &[Type<'g>],
    ) -> Work<Vec<Substitution<'g>>> {
        self.elab.reserve(binders.len())?;
        Ok(binders
            .iter()
            .zip(arguments)
            .map(|(binder, argument)| Substitution {
                binder: &binder.declaration.id,
                position: binder.position,
                argument: argument.clone(),
            })
            .collect())
    }
    pub(super) fn tuple_step(&mut self, inference: &mut Inference<'g>, node: NodeId) -> Work<()> {
        let Some(ty) = inference.slots[&node].expected.clone() else {
            return Ok(());
        };
        let TypeTerm::Tuple(types) = ty.as_ref() else {
            return Ok(());
        };
        let n = &inference.module.syntax.nodes[node];
        if types.len() != n.children.len() {
            self.finding(inference.module, node, ScalarFindingKind::Arity)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = inference.data.entry(node) {
            self.elab.reserve(n.children.len() * 3 + 1)?;
            entry.insert(PreparedData {
                flow: None,
                ty: ty.clone(),
                contexts: n
                    .children
                    .iter()
                    .copied()
                    .zip(types.iter().cloned())
                    .collect(),
                inputs: n.children.clone(),
                fact: DataExpression::Tuple {
                    elements: n.children.clone(),
                },
            });
        }
        self.elab.reserve(types.len())?;
        let contexts: Vec<_> = n
            .children
            .iter()
            .copied()
            .zip(types.iter().cloned())
            .collect();
        self.establish(inference, node, ty, false)?;
        for (child, ty) in contexts {
            self.establish(inference, child, ty, true)?;
        }
        Ok(())
    }
    pub(super) fn construction_inputs(
        &mut self,
        module: &'g Module,
        node: NodeId,
    ) -> Work<Vec<NodeId>> {
        let mut inputs = Vec::new();
        for &child in &module.syntax.nodes[node].children[1..] {
            self.elab.reserve(1)?;
            let n = &module.syntax.nodes[child];
            inputs.push(if n.kind == NodeKind::Spread {
                n.children[0]
            } else {
                *n.children.last().unwrap()
            });
        }
        Ok(inputs)
    }
    pub(super) fn defer_pending_constructions(
        &mut self,
        inference: &mut Inference<'g>,
    ) -> Work<()> {
        self.elab.reserve(inference.slots.len())?;
        let pending: Vec<_> = inference
            .data_nodes
            .iter()
            .copied()
            .filter(|node| !inference.data.contains_key(node))
            .collect();
        for node in pending {
            let mut check_receivers = false;
            if !inference.slots[&node].blocked {
                let module = inference.module;
                if module.syntax.nodes[node].kind == NodeKind::Bracket {
                    if inference.slots[&node].nominal_tuple {
                        let inputs = self.runtime_children(inference, node)?;
                        if !self.has_blocked_input(inference, inputs)? {
                            self.finding(module, node, ScalarFindingKind::InsufficientContext)?;
                        }
                    } else {
                        self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
                    }
                } else if let Some(frame) = inference.nominals.get(&node) {
                    let inputs = self.nominal_inputs(frame)?;
                    if !self.has_blocked_input(inference, inputs)? {
                        check_receivers = frame.arguments_closed;
                        if !check_receivers {
                            self.finding(module, node, ScalarFindingKind::InsufficientContext)?;
                        }
                    }
                } else {
                    self.defer(module, node, ScalarUnsupported::GenericInference)?;
                }
            }
            inference.slots.get_mut(&node).unwrap().blocked = true;
            self.elab.reserve(1)?;
            let mut stack = vec![node];
            while let Some(child) = stack.pop() {
                self.elab.step(1)?;
                if let Some(slot) = inference.slots.get_mut(&child)
                    && (self.input.ordinary.is_none()
                        || (slot.ty.is_none() && !(check_receivers && slot.expected.is_some())))
                {
                    slot.blocked = true;
                }
                let children = &inference.module.syntax.nodes[child].children;
                self.elab.reserve(children.len())?;
                stack.extend(children);
            }
        }
        Ok(())
    }
    pub(super) fn enum_base(
        &mut self,
        module: &'g Module,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<bool> {
        if module.syntax.nodes[node].kind != NodeKind::ValueReference {
            return Ok(false);
        }
        let path = &module.syntax.nodes[module.syntax.nodes[node].children[0]];
        let first = module.syntax.nodes[path.children[0]].span.bytes();
        if context.signature.is_some_and(|s| {
            s.binders.iter().any(|b| {
                b.declaration
                    .name
                    .as_ref()
                    .is_some_and(|n| n.as_bytes() == first)
            })
        }) {
            return Ok(false);
        }
        if let Some(local) = self.elab.lexical_declaration(
            module,
            node,
            Namespace::TypeModule,
            std::str::from_utf8(first).unwrap(),
        )? {
            return Ok(path.children.len() == 1 && local.id.kind == DeclarationKind::Enum);
        }
        let mut key = &module.key;
        for (index, &segment) in path.children.iter().enumerate() {
            let name = std::str::from_utf8(module.syntax.nodes[segment].span.bytes()).unwrap();
            let Some(binding) = self.elab.binding(key, name)? else {
                return Ok(false);
            };
            match &binding.target {
                Target::Module(next) => key = next,
                Target::Declaration(id) if index + 1 == path.children.len() => {
                    return Ok(id.kind == DeclarationKind::Enum);
                }
                _ => return Ok(false),
            }
        }
        Ok(false)
    }
    pub(super) fn variant_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        if self.input.ordinary.is_some() {
            return self.nominal_step(inference, node, context);
        }
        let module = inference.module;
        let n = &module.syntax.nodes[node];
        if !inference.data.contains_key(&node) {
            let before = self.marks();
            let name = if n.kind == NodeKind::Call {
                n.children[0]
            } else {
                node
            };
            let ty = self.constructor_type(
                module,
                name,
                context,
                inference.slots[&node].expected.as_ref(),
                None,
            )?;
            if let Some(ty) = ty {
                let TypeTerm::Variant {
                    parent,
                    declaration,
                } = ty.as_ref()
                else {
                    unreachable!()
                };
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
                let substitution = self.nominal_substitution(&facts.binders, arguments)?;
                let mut inputs = Vec::new();
                let mut contexts = Vec::new();
                match (&variant.payload, n.kind) {
                    (Payload::Unit, NodeKind::Field) => (),
                    (Payload::Tuple(types), NodeKind::Call) => {
                        if types.len() != n.children.len() - 1 {
                            self.finding(module, node, ScalarFindingKind::Arity)?;
                        }
                        self.elab.reserve(n.children.len() - 1)?;
                        inputs.extend(&n.children[1..]);
                        for (&argument, ty) in inputs.iter().zip(types) {
                            let ty = self.substitute(ty, &substitution, 1)?;
                            self.elab.reserve(1)?;
                            contexts.push((argument, ty));
                        }
                    }
                    _ => self.finding(module, node, ScalarFindingKind::TypeMismatch)?,
                }
                if before == self.marks() {
                    self.elab.reserve(1 + inputs.len())?;
                    let arguments = inputs.clone();
                    inference.data.insert(
                        node,
                        PreparedData {
                            flow: None,
                            ty: ty.clone(),
                            inputs,
                            contexts,
                            fact: DataExpression::Construction(Box::new(Construction {
                                ty,
                                fields: Vec::new(),
                                arguments,
                                base: None,
                                substitution,
                                argument_sources: Vec::new(),
                                bindings: Vec::new(),
                            })),
                        },
                    );
                }
            }
            if before != self.marks() {
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
        }
        if let Some(prepared) = inference.data.get(&node) {
            self.elab.reserve(prepared.contexts.len())?;
            let contexts = prepared.contexts.clone();
            let ty = prepared.ty.clone();
            self.establish(inference, node, ty, false)?;
            for (child, ty) in contexts {
                self.establish(inference, child, ty, true)?;
            }
        }
        Ok(())
    }
    pub(super) fn data_join(&mut self, a: &Type<'g>, b: &Type<'g>) -> Work<Option<Type<'g>>> {
        if self.data_admission(a, b)? {
            return Ok(Some(b.clone()));
        }
        if self.data_admission(b, a)? {
            return Ok(Some(a.clone()));
        }
        if let (TypeTerm::Variant { parent: pa, .. }, TypeTerm::Variant { parent: pb, .. }) =
            (a.as_ref(), b.as_ref())
            && self.same(pa, pb)?
        {
            return Ok(Some(pa.clone()));
        }
        Ok(None)
    }
    fn member_types(&mut self, ty: &Type<'g>) -> Work<Option<Vec<(&'g Declaration, Type<'g>)>>> {
        let parent = match ty.as_ref() {
            TypeTerm::Variant { parent, .. } => parent,
            _ => ty,
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = parent.as_ref()
        else {
            return Ok(None);
        };
        let facts = &self.input.facts.declarations[declaration];
        let fields = match (&facts.shape, ty.as_ref()) {
            (DeclarationShape::Record { fields }, _) => fields,
            (DeclarationShape::Enum { variants }, TypeTerm::Variant { declaration, .. }) => {
                self.elab.step(variants.len())?;
                let v = variants
                    .iter()
                    .find(|v| v.declaration.id == **declaration)
                    .unwrap();
                let Payload::Record(fields) = &v.payload else {
                    return Ok(None);
                };
                fields
            }
            _ => return Ok(None),
        };
        self.elab.reserve(fields.len())?;
        let substitution = self.nominal_substitution(&facts.binders, arguments)?;
        fields
            .iter()
            .map(|f| Ok((f.declaration, self.substitute(&f.ty, &substitution, 1)?)))
            .collect::<Work<Vec<_>>>()
            .map(Some)
    }
    pub(super) fn untyped_index_literal(&mut self, module: &'g Module, node: NodeId) -> Work<bool> {
        Ok(module.syntax.nodes[node].kind == NodeKind::Literal
            && matches!(&self.token(module, node)?.kind, TokenKind::Number(number) if number.suffix.is_none()))
    }

    pub(super) fn tuple_index(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        if inference.data.contains_key(&node) {
            return Ok(());
        }
        let module = inference.module;
        let syntax = &module.syntax.nodes[node];
        let [base, selector] = [syntax.children[0], syntax.children[1]];
        let Some(mut ty) = inference.slots[&base].ty.clone() else {
            return Ok(());
        };
        let slot = &inference.slots[&base];
        let mut place = slot.place;
        let mut access = slot.access;
        let restricted = slot.read_only_projection;
        let mut path = self.copy_path(slot.path.as_ref())?;
        if let TypeTerm::Reference { mutable, referent } = ty.as_ref() {
            path = self.project_path(path.as_ref(), PathProjection::Dereference)?;
            (place, access) = if *mutable && !restricted {
                (Place::Writable, Access::Writable)
            } else {
                (Place::ReadOnly, Access::ReadOnly)
            };
            ty = referent.clone();
        }
        let TypeTerm::Tuple(elements) = ty.as_ref() else {
            if matches!(
                ty.as_ref(),
                TypeTerm::Nominal { .. } | TypeTerm::Parameter { .. }
            ) {
                self.defer(module, node, ScalarUnsupported::CoreAdmission)?;
            } else {
                self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            }
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        let component = if self.untyped_index_literal(module, selector)? {
            let token = self.token(module, selector)?;
            let TokenKind::Number(number) = &token.kind else {
                unreachable!()
            };
            if number.form == NumberForm::Float {
                None
            } else {
                let digits = token.span.bytes();
                self.elab.step(digits.len())?;
                let digits = if number.base == 10 {
                    digits
                } else {
                    &digits[2..]
                };
                digits
                    .iter()
                    .copied()
                    .filter(|&b| b != b'_')
                    .try_fold(0usize, |value, digit| {
                        let digit = char::from(digit).to_digit(number.base)? as usize;
                        value.checked_mul(number.base as usize)?.checked_add(digit)
                    })
                    .filter(|&index| index < elements.len())
            }
        } else {
            None
        };
        let Some(component) = component else {
            self.finding(module, selector, ScalarFindingKind::InvalidTupleIndex)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        };
        path = self.project_path(path.as_ref(), PathProjection::TupleElement(component))?;
        let (ty, refinement) = self.refined(path.as_ref(), elements[component].clone(), context)?;
        self.elab.reserve(2)?;
        inference.data.insert(
            node,
            PreparedData {
                ty: ty.clone(),
                flow: None,
                inputs: vec![base],
                contexts: Vec::new(),
                fact: DataExpression::TupleIndex {
                    base,
                    selector,
                    component,
                },
            },
        );
        let slot = inference.slots.get_mut(&node).unwrap();
        slot.place = place;
        slot.access = access;
        slot.path = path;
        slot.refinement = refinement;
        slot.read_only_projection = restricted || place == Place::ReadOnly;
        self.establish(inference, node, ty, false)
    }

    pub(super) fn field_projection(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        if let Some(prepared) = inference.data.get(&node) {
            let ty = prepared.ty.clone();
            return self.establish(inference, node, ty, false);
        }
        let module = inference.module;
        let n = &module.syntax.nodes[node];
        let base = n.children[0];
        let Some(mut ty) = inference.slots[&base].ty.clone() else {
            return Ok(());
        };
        let mut place = inference.slots[&base].place;
        let restricted = inference.slots[&base].read_only_projection;
        let mut access = inference.slots[&base].access;
        let mut path = self.copy_path(inference.slots[&base].path.as_ref())?;
        if let TypeTerm::Reference { mutable, referent } = ty.as_ref() {
            path = self.project_path(path.as_ref(), PathProjection::Dereference)?;
            place = if *mutable && !restricted {
                Place::Writable
            } else {
                Place::ReadOnly
            };
            access = if *mutable && !restricted {
                Access::Writable
            } else {
                Access::ReadOnly
            };
            ty = referent.clone();
        }
        let (refined, _) = self.refined(path.as_ref(), ty, context)?;
        ty = refined;
        if self.type_unavailable(&ty, 1)? {
            self.defer(module, node, ScalarUnsupported::DeclarationPrerequisite)?;
            inference.slots.get_mut(&node).unwrap().blocked = true;
            return Ok(());
        }
        let parent = if let TypeTerm::Variant { parent, .. } = ty.as_ref() {
            parent
        } else {
            &ty
        };
        if let TypeTerm::Nominal { declaration, .. } = parent.as_ref() {
            let def = &self.input.facts.declarations[declaration].declaration;
            if !def.public && declaration.module != module.key {
                self.finding(module, node, ScalarFindingKind::PrivateValue)?;
                return Ok(());
            }
        }
        let Some(fields) = self.member_types(&ty)? else {
            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            return Ok(());
        };
        let spelling = module.syntax.nodes[n.children[1]].span.bytes();
        self.elab.step(fields.len())?;
        let Some((field, ty)) = fields
            .into_iter()
            .find(|(d, _)| d.name.as_ref().is_some_and(|n| n.as_bytes() == spelling))
        else {
            self.finding(module, node, ScalarFindingKind::MissingField)?;
            return Ok(());
        };
        if !field.public && field.id.module != module.key {
            self.finding(module, node, ScalarFindingKind::PrivateValue)?;
            return Ok(());
        }
        path = self.project_path(path.as_ref(), PathProjection::Field(&field.id))?;
        let (ty, refinement) = self.refined(path.as_ref(), ty, context)?;
        self.elab.reserve(2)?;
        inference.data.insert(
            node,
            PreparedData {
                flow: None,
                ty: ty.clone(),
                inputs: vec![base],
                contexts: Vec::new(),
                fact: DataExpression::Projection(Projection { field, base }),
            },
        );
        let slot = inference.slots.get_mut(&node).unwrap();
        slot.place = place;
        slot.path = path;
        slot.access = access;
        slot.refinement = refinement;
        slot.read_only_projection = restricted || place == Place::ReadOnly;
        self.establish(inference, node, ty, false)
    }
    pub(super) fn data_admission(&mut self, actual: &Type<'g>, expected: &Type<'g>) -> Work<bool> {
        self.type_depth(actual, 1)?;
        self.type_depth(expected, 1)?;
        Ok(match (actual.as_ref(), expected.as_ref()) {
            (TypeTerm::Variant { parent, .. }, _) => parent == expected,
            (
                TypeTerm::Reference {
                    mutable: false,
                    referent: a,
                },
                TypeTerm::Reference {
                    mutable: false,
                    referent: b,
                },
            ) => matches!(a.as_ref(), TypeTerm::Variant { parent, .. } if parent == b),
            _ => false,
        })
    }
    fn constructor_type(
        &mut self,
        module: &'g Module,
        name: NodeId,
        context: &Context<'r, 'g>,
        expected: Option<&Type<'g>>,
        base: Option<&Type<'g>>,
    ) -> Work<Option<Type<'g>>> {
        let n = &module.syntax.nodes[name];
        if n.kind == NodeKind::Field {
            let Some(parent) =
                self.constructor_type(module, n.children[0], context, expected, None)?
            else {
                return Ok(None);
            };
            let TypeTerm::Nominal { declaration, .. } = parent.as_ref() else {
                self.finding(module, name, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            };
            let DeclarationShape::Enum { variants } =
                &self.input.facts.declarations[declaration].shape
            else {
                self.finding(module, name, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            };
            self.elab.step(variants.len())?;
            let spelling = module.syntax.nodes[n.children[1]].span.bytes();
            let Some(variant) = variants.iter().find(|v| {
                v.declaration
                    .name
                    .as_ref()
                    .is_some_and(|s| s.as_bytes() == spelling)
            }) else {
                self.finding(module, name, ScalarFindingKind::MissingField)?;
                return Ok(None);
            };
            self.elab.reserve(1)?;
            return Ok(Some(Arc::new(TypeTerm::Variant {
                parent,
                declaration: &variant.declaration.id,
            })));
        }
        if !matches!(n.kind, NodeKind::ValueReference | NodeKind::PatternPath) {
            self.finding(module, name, ScalarFindingKind::TypeMismatch)?;
            return Ok(None);
        }
        let binders: &[Binder<'g>] = context.signature.map_or(&[], |s| &s.binders[..]);
        let path = &module.syntax.nodes[n.children[0]];
        let first = module.syntax.nodes[path.children[0]].span.bytes();
        if n.children
            .iter()
            .any(|&id| module.syntax.nodes[id].kind == NodeKind::TypeArguments)
            || binders.iter().any(|b| {
                b.declaration
                    .name
                    .as_ref()
                    .is_some_and(|s| s.as_bytes() == first)
            })
        {
            return self.elab.named_type(module, name, binders, 1);
        }
        let mut key = &module.key;
        let mut bindings = Vec::new();
        let mut target = self
            .elab
            .lexical_declaration(
                module,
                name,
                Namespace::TypeModule,
                std::str::from_utf8(first).unwrap(),
            )?
            .filter(|_| path.children.len() == 1)
            .map(|d| &d.id);
        for (index, &segment) in path.children.iter().enumerate() {
            if target.is_some() {
                break;
            }
            let spelling = std::str::from_utf8(module.syntax.nodes[segment].span.bytes()).unwrap();
            let Some(binding) = self.elab.binding(key, spelling)? else {
                break;
            };
            if index > 0 && !binding.public {
                self.elab
                    .binding_finding(module, name, FindingKind::PrivateType, binding)?;
                return Ok(None);
            }
            self.elab.reserve(1)?;
            bindings.push(binding);
            match &binding.target {
                Target::Module(next) => key = next,
                Target::Declaration(id) if index + 1 == path.children.len() => {
                    target = Some(id);
                    break;
                }
                _ => break,
            }
        }
        let Some(id) = target else {
            return self.elab.named_type(module, name, binders, 1);
        };
        let Some(facts) = self.input.facts.declarations.get(id) else {
            return self.elab.named_type(module, name, binders, 1);
        };
        if facts.binders.is_empty() {
            return self.elab.named_type(module, name, binders, 1);
        }
        let mut application = None;
        for candidate in [expected, base].into_iter().flatten() {
            let candidate = if let TypeTerm::Variant { parent, .. } = candidate.as_ref() {
                parent
            } else {
                candidate
            };
            if matches!(candidate.as_ref(), TypeTerm::Nominal { declaration, .. } if *declaration == id)
            {
                if let Some(previous) = &application
                    && !self.same(previous, candidate)?
                {
                    self.mismatch(module, name, candidate, previous)?;
                    return Ok(None);
                }
                application = Some(candidate.clone());
            }
        }
        let Some(application) = application else {
            return Ok(None);
        };
        let TypeTerm::Nominal { arguments, .. } = application.as_ref() else {
            unreachable!()
        };
        let substitution = self.nominal_substitution(&facts.binders, arguments)?;
        self.elab.reserve(1 + arguments.len())?;
        let mut term = application.as_ref().clone();
        if let Some(&suffix) = n.children.last()
            && module.syntax.nodes[suffix].kind == NodeKind::Name
        {
            let DeclarationShape::Enum { variants } = &facts.shape else {
                self.finding(module, name, ScalarFindingKind::TypeMismatch)?;
                return Ok(None);
            };
            self.elab.step(variants.len())?;
            let spelling = module.syntax.nodes[suffix].span.bytes();
            let Some(variant) = variants.iter().find(|v| {
                v.declaration
                    .name
                    .as_ref()
                    .is_some_and(|n| n.as_bytes() == spelling)
            }) else {
                self.finding(module, name, ScalarFindingKind::MissingField)?;
                return Ok(None);
            };
            term = TypeTerm::Variant {
                parent: application,
                declaration: &variant.declaration.id,
            };
        }
        self.elab
            .retain_type(module, name, term, substitution, bindings)
            .map(Some)
    }
    pub(super) fn construction_step(
        &mut self,
        inference: &mut Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<()> {
        if self.input.ordinary.is_some() {
            return self.nominal_step(inference, node, context);
        }
        if !inference.data.contains_key(&node) {
            let before = self.marks();
            let prepared = self.prepare_construction(inference, node, context)?;
            if let Some(prepared) = prepared {
                self.elab.reserve(1)?;
                inference.data.insert(node, prepared);
            } else if before != self.marks() {
                inference.slots.get_mut(&node).unwrap().blocked = true;
            }
        }
        if let Some(prepared) = inference.data.get(&node) {
            self.elab.reserve(prepared.contexts.len())?;
            let contexts = prepared.contexts.clone();
            let ty = prepared.ty.clone();
            self.establish(inference, node, ty, false)?;
            for (child, ty) in contexts {
                self.establish(inference, child, ty, true)?;
            }
        }
        Ok(())
    }
    fn prepare_construction(
        &mut self,
        inference: &Inference<'g>,
        node: NodeId,
        context: &Context<'r, 'g>,
    ) -> Work<Option<PreparedData<'g>>> {
        let module = inference.module;
        let n = &module.syntax.nodes[node];
        let base = n
            .children
            .iter()
            .find(|&&id| module.syntax.nodes[id].kind == NodeKind::Spread)
            .map(|&id| module.syntax.nodes[id].children[0]);
        let base_ty = base.and_then(|id| inference.slots[&id].ty.as_ref());
        let Some(ty) = self.constructor_type(
            module,
            n.children[0],
            context,
            inference.slots[&node].expected.as_ref(),
            base_ty,
        )?
        else {
            return Ok(None);
        };
        let Some(fields) = self.member_types(&ty)? else {
            self.finding(module, node, ScalarFindingKind::TypeMismatch)?;
            return Ok(None);
        };
        let parent = if let TypeTerm::Variant { parent, .. } = ty.as_ref() {
            parent
        } else {
            &ty
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = parent.as_ref()
        else {
            unreachable!()
        };
        let facts = &self.input.facts.declarations[declaration];
        if declaration.module != module.key
            && (!facts.declaration.public || fields.iter().any(|(d, _)| !d.public))
        {
            self.finding(module, node, ScalarFindingKind::PrivateValue)?;
            return Ok(None);
        }
        let substitution = self.nominal_substitution(&facts.binders, arguments)?;
        let Some(RecordInitializers {
            fields: written,
            mut contexts,
        }) = self.record_initializers(module, node, &fields, base.is_some())?
        else {
            return Ok(None);
        };
        self.elab
            .reserve(contexts.len() + usize::from(base.is_some()) * 2)?;
        let inputs = base
            .into_iter()
            .chain(contexts.iter().map(|(node, _)| *node))
            .collect();
        if let Some(base) = base {
            contexts.insert(0, (base, ty.clone()));
        }
        Ok(Some(PreparedData {
            flow: None,
            ty: ty.clone(),
            contexts,
            inputs,
            fact: DataExpression::Construction(Box::new(Construction {
                ty,
                fields: written,
                arguments: Vec::new(),
                base,
                substitution,
                argument_sources: Vec::new(),
                bindings: Vec::new(),
            })),
        }))
    }
    fn record_initializers(
        &mut self,
        module: &'g Module,
        node: NodeId,
        fields: &[(&'g Declaration, Type<'g>)],
        use_base: bool,
    ) -> Work<Option<RecordInitializers<'g>>> {
        let n = &module.syntax.nodes[node];
        let before = self.marks();
        let mut written = Vec::new();
        let mut contexts = Vec::new();
        let mut seen = BTreeSet::new();
        for &initializer in &n.children[1..] {
            self.elab.step(1)?;
            let field = &module.syntax.nodes[initializer];
            let NodeKind::InitializerField { shorthand } = field.kind else {
                continue;
            };
            let spelling = module.syntax.nodes[field.children[0]].span.bytes();
            self.elab.step(fields.len())?;
            let Some((member, expected)) = fields
                .iter()
                .find(|(d, _)| d.name.as_ref().is_some_and(|s| s.as_bytes() == spelling))
            else {
                self.finding(module, initializer, ScalarFindingKind::MissingField)?;
                continue;
            };
            self.elab.reserve(1)?;
            if !seen.insert(&member.id) {
                self.finding(module, initializer, ScalarFindingKind::DuplicateField)?;
            }
            let value = field.children[usize::from(!shorthand)];
            self.elab.reserve(2)?;
            contexts.push((value, expected.clone()));
            written.push(FieldValue {
                field: member,
                expression: value,
            });
        }
        if !use_base && seen.len() != fields.len() {
            self.finding(module, node, ScalarFindingKind::MissingField)?;
        }
        Ok((self.marks() == before).then_some(RecordInitializers {
            fields: written,
            contexts,
        }))
    }
}
