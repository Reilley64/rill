//! Captured primitive providers and exact prelude protection regions.
use super::*;
use crate::analysis::ordinary::{Compatibility, FamilyFacts, ImplementationFacts, patterns};
use crate::loader::DeclarationId;
use crate::parser::NodeKind;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveRole {
    Copy,
    Dispose,
    Equality,
    Show,
}

#[derive(Clone, Copy, Debug)]
pub struct PrimitiveBinding<'g> {
    pub declaration: &'g Declaration,
    pub operation: PrimitiveOperation,
}

/// A pinned Rill implementation, with the compiler binding if its body uses one.
#[derive(Clone, Copy, Debug)]
pub struct PrimitiveProvider<'g> {
    pub role: PrimitiveRole,
    pub family: &'g DeclarationId,
    pub primitive: Primitive,
    pub implementation: &'g Declaration,
    pub binding: Option<PrimitiveBinding<'g>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveCoverage {
    Pending,
    Clear,
    Overlap,
}

#[derive(Debug)]
pub struct PrimitiveOverlap<'g> {
    pub implementation: &'g Declaration,
    /// Indices into the protected and contender alternatives, respectively.
    pub intersection: (usize, usize),
}

impl PrimitiveOverlap<'_> {
    pub fn code(&self) -> &'static str {
        "R0500"
    }
}

/// A required winner for one exact input region: `&primitive` for copy, or
/// `primitive` for dispose. It says nothing about other types in the family.
#[derive(Debug)]
pub struct PrimitiveProtection<'g> {
    pub role: PrimitiveRole,
    pub family: &'g DeclarationId,
    pub primitive: Primitive,
    pub required_winner: Option<&'g DeclarationId>,
    pub coverage: PrimitiveCoverage,
    pub overlaps: Vec<PrimitiveOverlap<'g>>,
}

#[derive(Debug, Default)]
pub(in crate::analysis) struct PrimitiveCore<'g> {
    pub copy: Option<&'g Declaration>,
    pub dispose: Option<&'g Declaration>,
    pub equality: Option<&'g Declaration>,
    pub providers: Vec<PrimitiveProvider<'g>>,
    pub protection: Vec<PrimitiveProtection<'g>>,
    pub bridges: std::collections::BTreeMap<&'g DeclarationId, PrimitiveOperation>,
    /// The exported `log` and `panic` families of the pinned output module.
    pub output: Vec<&'g DeclarationId>,
}

impl PrimitiveCore<'_> {
    /// No primitive overlap and no `Overlap` verdict (metering rule 8).
    pub(in crate::analysis::typed) fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            protection,
            // Roles, providers and bridges: facts, not findings.
            copy: _copy,
            dispose: _dispose,
            equality: _equality,
            providers: _providers,
            bridges: _bridges,
            output: _output,
        } = self;
        protection
            .iter()
            .all(PrimitiveProtection::publishes_nothing_at_limit)
    }
}

impl PrimitiveProtection<'_> {
    /// No overlap and no `Overlap` verdict. Every field is named, so a new one
    /// fails to compile until it is decided here (metering rule 8).
    fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            overlaps,
            coverage,
            // The scope's identity and its required winner: facts.
            role: _role,
            family: _family,
            primitive: _primitive,
            required_winner: _required_winner,
        } = self;
        overlaps.is_empty() && *coverage != PrimitiveCoverage::Overlap
    }
}

impl PrimitiveRole {
    fn name(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Dispose => "dispose",
            Self::Equality => "eq",
            Self::Show => "show",
        }
    }

    fn arity(self) -> usize {
        if self == Self::Equality { 2 } else { 1 }
    }

    fn borrowed(self) -> bool {
        self != Self::Dispose
    }

    fn requires(self, primitive: Primitive) -> bool {
        self != Self::Equality || !matches!(primitive, Primitive::F32 | Primitive::F64)
    }

    fn result(self, input: &str) -> &str {
        match self {
            Self::Copy => input,
            Self::Dispose => "void",
            Self::Equality => "bool",
            Self::Show => "string",
        }
    }

    fn binding(self, primitive: Primitive) -> Option<(&'static str, PrimitiveOperation)> {
        match (self, primitive) {
            (Self::Copy, Primitive::String) => Some(("copyText", PrimitiveOperation::CopyText)),
            (Self::Dispose, Primitive::String) => {
                Some(("disposeText", PrimitiveOperation::DisposeText))
            }
            (Self::Show, p) => Some((p.show_binding(), PrimitiveOperation::Show(p))),
            _ => None,
        }
    }
}

fn primitive_name<'a>(term: &'a Type<'_>) -> Option<&'a str> {
    match term.as_ref() {
        TypeTerm::Primitive(p) => Some(p.name()),
        _ => None,
    }
}

fn input_type<'a, 'g>(term: &'a Type<'g>, role: PrimitiveRole) -> Option<&'a Type<'g>> {
    if !role.borrowed() {
        return Some(term);
    }
    match term.as_ref() {
        TypeTerm::Reference {
            mutable: false,
            referent,
        } => Some(referent),
        _ => None,
    }
}

fn check_family<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    role: PrimitiveRole,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    let Some(declaration) = export(
        inventory.graph(),
        module,
        Namespace::ValueFunction,
        role.name(),
        budget,
    )?
    else {
        return Ok(None);
    };
    if declaration.id.kind != DeclarationKind::Family {
        return Ok(None);
    }
    let Some(header) = family_header(inventory, declaration, budget)? else {
        return Ok(None);
    };
    let (DeclarationShape::Function { parameters, result }, [binder]) =
        (&header.shape, header.binders.as_slice())
    else {
        return Ok(None);
    };
    Ok((parameters.len() == role.arity()
        && parameters
            .iter()
            .all(|input| input_type(&input.ty, role).is_some_and(|t| binder_is(t, binder)))
        && if role == PrimitiveRole::Copy {
            binder_is(result, binder)
        } else {
            primitive_name(result) == Some(role.result(""))
        })
    .then_some(declaration))
}

pub(in crate::analysis::typed) fn check_show<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    match pinned_module(inventory.graph(), "format.rill", budget)? {
        Some(module) => check_family(inventory, module, PrimitiveRole::Show, budget),
        None => Ok(None),
    }
}

impl<'g> PrimitiveCore<'g> {
    pub(super) fn providers_complete(&self, budget: &mut Budget) -> Work<bool> {
        for role in [
            PrimitiveRole::Copy,
            PrimitiveRole::Dispose,
            PrimitiveRole::Equality,
            PrimitiveRole::Show,
        ] {
            for &primitive in Primitive::ALL {
                if !role.requires(primitive) {
                    continue;
                }
                budget.step(1 + self.providers.len())?;
                if !self
                    .providers
                    .iter()
                    .any(|p| p.role == role && p.primitive == primitive)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(super) fn protection_complete(&self, budget: &mut Budget) -> Work<bool> {
        // check_providers visits each exact (role, primitive) region once.
        budget.step(self.protection.len())?;
        Ok(self.protection.len() == 2 * Primitive::ALL.len()
            && self
                .protection
                .iter()
                .all(|scope| scope.coverage != PrimitiveCoverage::Pending))
    }

    pub(in crate::analysis) fn prelude(&self, id: &DeclarationId) -> Option<PrimitiveRole> {
        if self.copy.is_some_and(|d| d.id == *id) {
            Some(PrimitiveRole::Copy)
        } else if self.dispose.is_some_and(|d| d.id == *id) {
            Some(PrimitiveRole::Dispose)
        } else {
            None
        }
    }

    pub(in crate::analysis) fn winner(
        &self,
        id: &DeclarationId,
        subject: &Type<'g>,
    ) -> Option<&'g DeclarationId> {
        let primitive = primitive_name(subject).and_then(Primitive::named)?;
        self.protection
            .iter()
            .find(|p| {
                p.family == id && p.primitive == primitive && p.coverage == PrimitiveCoverage::Clear
            })?
            .required_winner
    }

    pub(in crate::analysis) fn winner_for_inputs<'a>(
        &self,
        id: &DeclarationId,
        inputs: &'a [Type<'g>],
    ) -> Option<(&'g DeclarationId, &'a Type<'g>)> {
        let (Some(role), [input]) = (self.prelude(id), inputs) else {
            return None;
        };
        let subject = input_type(input, role)?;
        Some((self.winner(id, subject)?, subject))
    }

    pub(in crate::analysis) fn missing_provider(
        &self,
        family: &DeclarationId,
        role: PrimitiveRole,
        inputs: &[Type<'g>],
    ) -> bool {
        let Some(primitive) = inputs
            .first()
            .and_then(|ty| input_type(ty, role))
            .and_then(primitive_name)
            .and_then(Primitive::named)
        else {
            return false;
        };
        role.requires(primitive)
            && !self
                .providers
                .iter()
                .any(|p| p.family == family && p.primitive == primitive)
    }

    pub(in crate::analysis) fn check(
        &mut self,
        inventory: &OrdinaryAnalysis<'g>,
        show: Option<&'g Declaration>,
        budget: &mut Budget,
    ) -> Work<()> {
        let graph = inventory.graph();
        if let Some(module) = pinned_module(graph, "core.rill", budget)? {
            for (role, prelude) in [
                (PrimitiveRole::Copy, &graph.prelude().copy),
                (PrimitiveRole::Dispose, &graph.prelude().dispose),
            ] {
                let declaration =
                    check_family(inventory, module, role, budget)?.filter(|d| d.id == *prelude);
                if role == PrimitiveRole::Copy {
                    self.copy = declaration;
                } else {
                    self.dispose = declaration;
                }
                if let Some(declaration) = declaration {
                    self.check_providers(inventory, module, declaration, role, budget)?;
                }
            }
        }
        if let Some(module) = pinned_module(graph, "compare.rill", budget)? {
            self.equality = check_family(inventory, module, PrimitiveRole::Equality, budget)?;
            if let Some(eq) = self.equality {
                self.check_providers(inventory, module, eq, PrimitiveRole::Equality, budget)?;
            }
        }
        if let Some(show) = show {
            let module = pinned_module(graph, "format.rill", budget)?.unwrap();
            self.check_providers(inventory, module, show, PrimitiveRole::Show, budget)?;
        }
        if let Some(module) = pinned_module(graph, "output.rill", budget)? {
            for (family, name, operation, result) in [
                ("log", "writeLine", PrimitiveOperation::WriteLine, "void"),
                ("panic", "panicText", PrimitiveOperation::Panic, "never"),
            ] {
                let Some(bridge) = check_output_binding(inventory, module, name, result, budget)?
                else {
                    continue;
                };
                let Some(family) = check_output_family(inventory, module, family, result, budget)?
                else {
                    continue;
                };
                budget.reserve(2)?;
                self.bridges.insert(&bridge.id, operation);
                self.output.push(&family.id);
            }
        }
        Ok(())
    }

    fn check_providers(
        &mut self,
        inventory: &OrdinaryAnalysis<'g>,
        module: &'g Module,
        declaration: &'g Declaration,
        role: PrimitiveRole,
        budget: &mut Budget,
    ) -> Work<()> {
        let id = &declaration.id;
        let Some(family) = inventory.captured_family(id) else {
            return Ok(());
        };
        let protected = matches!(role, PrimitiveRole::Copy | PrimitiveRole::Dispose);
        let mut captured = 0;
        if protected {
            for implementation in inventory.graph().implementations() {
                budget.step(1)?;
                captured += usize::from(implementation.family == *id);
            }
        }
        for &primitive in Primitive::ALL {
            if !role.requires(primitive) {
                continue;
            }
            let operation = role.binding(primitive);
            let binding = if let Some((name, operation)) = operation {
                check_binding(inventory, module, name, role, primitive, budget)?.map(
                    |declaration| PrimitiveBinding {
                        declaration,
                        operation,
                    },
                )
            } else {
                None
            };
            if let Some(binding) = binding {
                budget.reserve(1)?;
                self.bridges
                    .insert(&binding.declaration.id, binding.operation);
            }
            let implementation = if operation.is_none() || binding.is_some() {
                provider(inventory, module, family, role, primitive, binding, budget)?
            } else {
                None
            };
            if let Some(implementation) = implementation {
                budget.reserve(1)?;
                self.providers.push(PrimitiveProvider {
                    role,
                    family: id,
                    primitive,
                    implementation,
                    binding,
                });
            }
            if protected {
                budget.reserve(1)?;
                let mut scope = PrimitiveProtection {
                    family: id,
                    role,
                    primitive,
                    required_winner: implementation.map(|d| &d.id),
                    coverage: PrimitiveCoverage::Pending,
                    overlaps: Vec::new(),
                };
                scope.check(inventory.graph(), family, captured, budget)?;
                self.protection.push(scope);
            }
        }
        Ok(())
    }
}

impl<'g> PrimitiveProtection<'g> {
    fn check(
        &mut self,
        graph: &'g Graph,
        family: &FamilyFacts<'g>,
        captured: usize,
        budget: &mut Budget,
    ) -> Work<()> {
        let mut complete =
            self.required_winner.is_some() && captured == family.implementations.len();
        budget.step(graph.prelude().intrinsics.len())?;
        let intrinsic = graph
            .prelude()
            .intrinsics
            .iter()
            .find(|p| p.name() == self.primitive.name())
            .unwrap();
        budget.reserve(1 + usize::from(self.role.borrowed()))?;
        let mut input = Arc::new(TypeTerm::Primitive(intrinsic));
        if self.role.borrowed() {
            input = Arc::new(TypeTerm::Reference {
                mutable: false,
                referent: input,
            });
        }
        let protected = [patterns::ground(std::iter::once(&input), budget)?];
        for body in &family.implementations {
            budget.step(1)?;
            if Some(&body.source.declaration) == self.required_winner {
                continue;
            }
            if body.alternatives.is_empty() {
                complete = false;
                continue;
            }
            let other = &body.alternatives;
            if let Some(intersection) = patterns::intersection(&protected, other, budget)?
                && !(patterns::included(&protected, other, budget)?
                    && !patterns::included(other, &protected, budget)?)
            {
                budget.reserve(1)?;
                self.overlaps.push(PrimitiveOverlap {
                    implementation: body.header.declaration,
                    intersection,
                });
            }
        }
        self.coverage = if !complete {
            PrimitiveCoverage::Pending
        } else if self.overlaps.is_empty() {
            PrimitiveCoverage::Clear
        } else {
            PrimitiveCoverage::Overlap
        };
        Ok(())
    }
}

fn plain_header(
    inventory: &OrdinaryAnalysis<'_>,
    declaration: &Declaration,
    budget: &mut Budget,
) -> Work<bool> {
    budget.step(2)?;
    Ok(!inventory.declaration_unusable(&declaration.id)
        && inventory
            .headers()
            .get(&declaration.id)
            .is_some_and(|h| h.defaults.is_empty() && h.requirements.is_empty()))
}

fn family_header<'a, 'g>(
    inventory: &'a OrdinaryAnalysis<'g>,
    declaration: &Declaration,
    budget: &mut Budget,
) -> Work<Option<&'a DeclarationFacts<'g>>> {
    if !plain_header(inventory, declaration, budget)? {
        return Ok(None);
    }
    budget.step(1)?;
    Ok(inventory
        .captured_family(&declaration.id)
        .map(|f| &f.header))
}

fn exact_signature(
    header: &DeclarationFacts<'_>,
    role: PrimitiveRole,
    primitive: Primitive,
) -> bool {
    let DeclarationShape::Function { parameters, result } = &header.shape else {
        return false;
    };
    header.binders.is_empty()
        && parameters.len() == role.arity()
        && parameters.iter().all(|input| {
            input_type(&input.ty, role).and_then(primitive_name) == Some(primitive.name())
        })
        && primitive_name(result) == Some(role.result(primitive.name()))
}

fn check_binding<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    role: PrimitiveRole,
    primitive: Primitive,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    for declaration in &module.declarations {
        budget.step(1)?;
        if !declaration.public
            && declaration.scope.is_none()
            && declaration.id.kind == DeclarationKind::Family
            && declaration.name.as_deref() == Some(name)
            && inventory
                .captured_family(&declaration.id)
                .is_some_and(|f| f.implementations.is_empty())
            && let Some(signature) = family_header(inventory, declaration, budget)?
            && exact_signature(signature, role, primitive)
        {
            return Ok(Some(declaration));
        }
    }
    Ok(None)
}

/// An exported `name<S>(S): result` family of the pinned output module.
fn check_output_family<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    result: &str,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    let Some(declaration) = export(
        inventory.graph(),
        module,
        Namespace::ValueFunction,
        name,
        budget,
    )?
    else {
        return Ok(None);
    };
    if declaration.id.kind != DeclarationKind::Family {
        return Ok(None);
    }
    let Some(header) = family_header(inventory, declaration, budget)? else {
        return Ok(None);
    };
    let (
        DeclarationShape::Function {
            parameters,
            result: actual,
        },
        [binder],
    ) = (&header.shape, header.binders.as_slice())
    else {
        return Ok(None);
    };
    Ok(
        (matches!(parameters.as_slice(), [input] if binder_is(&input.ty, binder))
            && primitive_name(actual) == Some(result))
        .then_some(declaration),
    )
}

/// A private bodyless `name(&string): result` in the pinned output module.
fn check_output_binding<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    result: &str,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    for declaration in &module.declarations {
        budget.step(1)?;
        if !declaration.public
            && declaration.scope.is_none()
            && declaration.id.kind == DeclarationKind::Family
            && declaration.name.as_deref() == Some(name)
            && inventory
                .captured_family(&declaration.id)
                .is_some_and(|f| f.implementations.is_empty())
            && let Some(signature) = family_header(inventory, declaration, budget)?
            && let DeclarationShape::Function {
                parameters,
                result: actual,
            } = &signature.shape
            && signature.binders.is_empty()
            && let [parameter] = parameters.as_slice()
            && input_type(&parameter.ty, PrimitiveRole::Show).and_then(primitive_name)
                == Some("string")
            && primitive_name(actual) == Some(result)
        {
            return Ok(Some(declaration));
        }
    }
    Ok(None)
}

fn provider<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    family: &FamilyFacts<'g>,
    role: PrimitiveRole,
    primitive: Primitive,
    binding: Option<PrimitiveBinding<'g>>,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    for body in &family.implementations {
        budget.step(1)?;
        let declaration = body.header.declaration;
        if declaration.id.module == module.key
            && declaration.scope.is_none()
            && body.compatibility == Compatibility::Compatible
            && body.alternatives.len() == 1
            && exact_signature(&body.header, role, primitive)
            && plain_header(inventory, declaration, budget)?
        {
            budget.step(usize::from(binding.is_some()) * 5)?;
            if binding.is_none_or(|binding| delegates(module, body, role, binding.declaration)) {
                return Ok(Some(declaration));
            }
        }
    }
    Ok(None)
}

// The pinned compiler-backed wrappers have one unqualified bridge call and
// forward their sole parameter. No local source can shadow the bridge here.
fn delegates(
    module: &Module,
    body: &ImplementationFacts<'_>,
    role: PrimitiveRole,
    binding: &Declaration,
) -> bool {
    let DeclarationShape::Function { parameters, .. } = &body.header.shape else {
        return false;
    };
    if parameters[0].declaration.name.as_deref() != Some("value") {
        return false;
    }
    let nodes = &module.syntax.nodes;
    let [statement] = nodes[body.source.body_node].children.as_slice() else {
        return false;
    };
    let kind = if role == PrimitiveRole::Dispose {
        NodeKind::ExpressionStatement
    } else {
        NodeKind::Return
    };
    let [call] = nodes[*statement].children.as_slice() else {
        return false;
    };
    let [callee, value] = nodes[*call].children.as_slice() else {
        return false;
    };
    nodes[*statement].kind == kind
        && nodes[*call].kind == NodeKind::Call
        && nodes[*callee].kind == NodeKind::ValueReference
        && binding
            .name
            .as_deref()
            .is_some_and(|name| nodes[*callee].span.bytes() == name.as_bytes())
        && nodes[*value].kind == NodeKind::ValueReference
        && nodes[*value].span.bytes() == b"value"
}
