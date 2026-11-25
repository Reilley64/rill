//! Scalar conversion operations and the independent protected numeric matrix.
use super::collections::private_bridge;
use super::core::{export, pinned_module, unconditional_declaration};
use super::*;
use crate::analysis::conversions::{self, ConversionFamily, RegistrationIssueKind, RoleHypothesis};
use crate::analysis::{Binding, Declaration, DeclarationShape, Payload, Type};
use crate::loader::keyed::DeclarationTable;
use crate::loader::{DeclarationId, Implementation, Module, Namespace, Target};
use crate::parser::NodeKind;
use std::collections::BTreeMap;

pub mod values;

#[derive(Debug, Default)]
pub(in crate::analysis) struct ConversionCore<'g> {
    pub into: Option<&'g Declaration>,
    pub try_into: Option<&'g Declaration>,
    pub families: DeclarationTable<'g, ConversionFamily<'g>>,
    pub numeric_error: Option<&'g Declaration>,
    pub registered: BTreeMap<&'g DeclarationId, RegisteredConversion<'g>>,
    pub bridges: BTreeMap<&'g DeclarationId, ConversionProvider>,
    bytes: Option<&'g Declaration>,
    list: Option<ListRole<'g>>,
}

impl<'g> ConversionCore<'g> {
    /// A `ResourceLimit` report publishes no finding (metering rule 8).
    pub(super) fn withhold_findings(&mut self) {
        for family in self.families.values_mut() {
            family.issues.clear();
        }
    }

    /// No registration issue is published. Every field is named, so a new one
    /// fails to compile until it is decided here (metering rule 8).
    pub(super) fn publishes_nothing_at_limit(&self) -> bool {
        let Self {
            families,
            // Roles, providers and registrations: facts, not findings.
            into: _into,
            try_into: _try_into,
            numeric_error: _numeric_error,
            registered: _registered,
            bridges: _bridges,
            bytes: _bytes,
            list: _list,
        } = self;
        families
            .values()
            .all(ConversionFamily::publishes_nothing_at_limit)
    }

    /// The pinned opaque nominal is backed by the linked owned byte buffer.
    /// Borrowed text/byte views are descriptors, not additional owned buffers.
    pub(in crate::analysis) fn bytes_representation(&self) -> Option<&'g Declaration> {
        self.bytes
    }

    pub(super) fn roles_complete(&self) -> bool {
        self.into.is_some()
            && self.try_into.is_some()
            && self.numeric_error.is_some()
            && self.bytes.is_some()
    }

    pub(super) fn providers_complete(&self, budget: &mut Budget) -> Work<bool> {
        use ConversionProvider::*;
        if !self.roles_complete() || !check_numeric_matrix(budget)? {
            return Ok(false);
        }
        let expected = numeric_primitives().map(|p| Parse(DecimalParse(p))).chain([
            CodePoint,
            UnicodeScalar,
            TextBytes,
            DecodeOwned,
            DecodeView,
            TakeList,
            CopyList,
            TakeBytes,
            CopyBytes,
        ]);
        for operation in expected {
            budget.step(1 + self.registered.len())?;
            if self
                .registered
                .values()
                .filter(|entry| entry.operation == operation)
                .count()
                != 1
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn protection_complete(&self, budget: &mut Budget) -> Work<bool> {
        if !self.roles_complete() || !check_numeric_matrix(budget)? {
            return Ok(false);
        }
        // The identity and numeric gates use the same admission rule as the
        // selected calls. Check even unused registrations; unknown domains
        // cannot establish that the protected pairs are untouched.
        for family in self.families.values() {
            budget.step(1 + family.registrations.len() + family.issues.len())?;
            if family
                .issues
                .iter()
                .any(|issue| issue.kind == conversions::RegistrationIssueKind::IncompleteSignature)
                || family
                    .registrations
                    .iter()
                    .any(|registration| registration.alternatives.is_none())
            {
                return Ok(false);
            }
        }
        Ok(self.families.len() == 2)
    }

    pub(super) fn protected_violation(&self, budget: &mut Budget) -> Work<bool> {
        for family in self.families.values() {
            budget.step(1 + family.issues.len())?;
            if family.issues.iter().any(|issue| {
                matches!(
                    issue.kind,
                    RegistrationIssueKind::ProtectedIdentity
                        | RegistrationIssueKind::ProtectedNumericPair
                )
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn check(
        &mut self,
        inventory: &OrdinaryAnalysis<'g>,
        result: Option<ResultRole<'g>>,
        option: Option<OptionRole<'g>>,
        list: Option<ListRole<'g>>,
        budget: &mut Budget,
    ) -> Work<()> {
        let graph = inventory.graph();
        let Some(module) = pinned_module(graph, "convert.rill", budget)? else {
            return Ok(());
        };
        let count = graph.declaration_count();
        budget.reserve(count)?;
        self.families.size_for(graph.id(), count);
        for (name, alias, role) in [("into", "from", None), ("tryInto", "tryFrom", result)] {
            if name == "tryInto" && role.is_none() {
                continue;
            }
            let Some(declaration) = export(graph, module, Namespace::ValueFunction, name, budget)?
            else {
                continue;
            };
            let Some(primary) = route(graph, module, name, declaration, budget)? else {
                continue;
            };
            if route(graph, module, alias, declaration, budget)?.is_none() {
                continue;
            }
            let Some(family) = inventory.captured_family(&declaration.id) else {
                continue;
            };
            budget.step(2)?;
            if inventory.declaration_unusable(&declaration.id)
                || !inventory
                    .headers()
                    .get(&declaration.id)
                    .is_some_and(|h| h.defaults.is_empty() && h.requirements.is_empty())
                || !conversions::family_signature(&family.header, role.map(|r| &r.declaration.id))
            {
                continue;
            }
            let premise = if let Some(role) = role {
                let result_module = pinned_module(graph, "result.rill", budget)?.unwrap();
                let Some(result) = route(graph, result_module, "Result", role.declaration, budget)?
                else {
                    continue;
                };
                self.try_into = Some(declaration);
                RoleHypothesis::Fallible {
                    family: primary,
                    result,
                }
            } else {
                self.into = Some(declaration);
                RoleHypothesis::Infallible(primary)
            };
            budget.reserve(1)?;
            let mut collected = ConversionFamily {
                premise,
                routes: Vec::new(),
                registrations: Vec::new(),
                issues: Vec::new(),
            };
            conversions::collect(inventory, family, &mut collected, budget)?;
            self.families.insert(&declaration.id, collected);
        }
        self.families.follow_order(
            graph
                .modules()
                .iter()
                .flat_map(|module| module.declarations.iter().map(|d| &d.id)),
        );
        if let Some(number) = pinned_module(graph, "number.rill", budget)? {
            self.numeric_error = unit_error(
                inventory,
                number,
                "ConversionError",
                &["OutOfRange", "NotFinite"],
                budget,
            )?;
            if let Some(role) = result
                && let Some(error) = unit_error(
                    inventory,
                    number,
                    "ParseError",
                    &["InvalidSyntax", "OutOfRange"],
                    budget,
                )?
            {
                for (primitive, name) in DECIMAL_PARSERS {
                    self.register(
                        inventory,
                        module,
                        name,
                        ConversionProvider::Parse(DecimalParse(primitive)),
                        Endpoint::Primitive(primitive),
                        Endpoint::Shared(&Endpoint::Primitive(Primitive::String)),
                        Some((role.declaration, error)),
                        budget,
                    )?;
                }
            }
        }
        use Endpoint::{List, Nominal, Primitive as P, Shared};
        use Primitive::{Char, String, U32};
        self.register(
            inventory,
            module,
            "codePoint",
            ConversionProvider::CodePoint,
            P(U32),
            P(Char),
            None,
            budget,
        )?;
        if let (Some(result), Some(char_module)) =
            (result, pinned_module(graph, "char.rill", budget)?)
            && let Some(error) = unit_error(
                inventory,
                char_module,
                "ScalarError",
                &["InvalidScalar"],
                budget,
            )?
        {
            self.register(
                inventory,
                module,
                "unicodeScalar",
                ConversionProvider::UnicodeScalar,
                P(Char),
                P(U32),
                Some((result.declaration, error)),
                budget,
            )?;
        }
        self.list = list;
        let Some(module) = pinned_module(graph, "bytes.rill", budget)? else {
            return Ok(());
        };
        let Some(bytes) = bytes_role(inventory, module, budget)? else {
            return Ok(());
        };
        self.bytes = Some(bytes);
        let bytes = Nominal(bytes);
        let string = P(String);
        self.register(
            inventory,
            module,
            "textBytes",
            ConversionProvider::TextBytes,
            Shared(&bytes),
            Shared(&string),
            None,
            budget,
        )?;
        if let (Some(result), Some(option)) = (result, option)
            && let Some(error) = utf8_error(inventory, option, budget)?
        {
            for (name, provider, to) in [
                ("decodeOwned", ConversionProvider::DecodeOwned, string),
                (
                    "decodeView",
                    ConversionProvider::DecodeView,
                    Shared(&string),
                ),
            ] {
                self.register(
                    inventory,
                    module,
                    name,
                    provider,
                    to,
                    Shared(&bytes),
                    Some((result.declaration, error)),
                    budget,
                )?;
            }
        }
        if let Some(list) = list {
            let list = List(list.declaration);
            for (name, provider, to, from) in [
                ("takeList", ConversionProvider::TakeList, bytes, list),
                (
                    "copyList",
                    ConversionProvider::CopyList,
                    bytes,
                    Shared(&list),
                ),
                ("takeBytes", ConversionProvider::TakeBytes, list, bytes),
                (
                    "copyBytes",
                    ConversionProvider::CopyBytes,
                    list,
                    Shared(&bytes),
                ),
            ] {
                self.register(inventory, module, name, provider, to, from, None, budget)?;
            }
        }
        Ok(())
    }

    pub fn promises_registered_pair(&self, to: &Type<'g>, from: &Type<'g>, fallible: bool) -> bool {
        use Endpoint::{List, Nominal, Primitive as P, Shared};
        let text = P(Primitive::String);
        let code = P(Primitive::U32);
        let char = P(Primitive::Char);
        if fallible
            && ((Shared(&text).matches(from)
                && matches!(to.as_ref(), crate::analysis::TypeTerm::Primitive(p) if Primitive::named(p.name()).and_then(width).is_some()))
                || (char.matches(to) && code.matches(from)))
            || !fallible && code.matches(to) && char.matches(from)
        {
            return true;
        }
        let Some(bytes) = self.bytes.map(Nominal) else {
            return false;
        };
        let bytes_view = Shared(&bytes);
        if fallible {
            return bytes_view.matches(from) && (text.matches(to) || Shared(&text).matches(to));
        }
        if bytes_view.matches(to) && Shared(&text).matches(from) {
            return true;
        }
        self.list.is_some_and(|role| {
            let list = List(role.declaration);
            (bytes.matches(to) && (list.matches(from) || Shared(&list).matches(from)))
                || (list.matches(to) && (bytes.matches(from) || bytes_view.matches(from)))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn register(
        &mut self,
        inventory: &OrdinaryAnalysis<'g>,
        module: &'g Module,
        name: &str,
        operation: ConversionProvider,
        destination: Endpoint<'g, '_>,
        source: Endpoint<'g, '_>,
        fallible: Option<(&'g Declaration, &'g Declaration)>,
        budget: &mut Budget,
    ) -> Work<()> {
        let family = if fallible.is_some() {
            self.try_into
        } else {
            self.into
        };
        let Some(family) = family.and_then(|d| self.families.get(&d.id)) else {
            return Ok(());
        };
        let Some(bridge) = private_bridge(inventory, module, name, budget)? else {
            return Ok(());
        };
        let DeclarationShape::Function { parameters, result } = &bridge.shape else {
            return Ok(());
        };
        let [parameter] = parameters.as_slice() else {
            return Ok(());
        };
        if !bridge.binders.is_empty()
            || !source.matches(&parameter.ty)
            || !result_matches(result, destination, fallible)
        {
            return Ok(());
        }
        for registration in &family.registrations {
            budget.step(1)?;
            let implementation = registration.source;
            if implementation.declaration.module != module.key
                || !destination.matches(&registration.destination)
                || !source.matches(&registration.parameter)
                || !match (fallible, &registration.error) {
                    (Some((_, error)), Some(found)) => Endpoint::Nominal(error).matches(found),
                    (None, None) => true,
                    _ => false,
                }
                || !returns_bridge(module, implementation, name, budget)?
            {
                continue;
            }
            budget.reserve(2)?;
            self.registered.insert(
                &implementation.declaration,
                RegisteredConversion {
                    bridge: bridge.declaration,
                    operation,
                },
            );
            self.bridges.insert(&bridge.declaration.id, operation);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RegisteredConversion<'g> {
    pub bridge: &'g Declaration,
    pub operation: ConversionProvider,
}

#[derive(Clone, Copy)]
enum Endpoint<'g, 'a> {
    Primitive(Primitive),
    Nominal(&'g Declaration),
    List(&'g Declaration),
    Shared(&'a Self),
}

impl Endpoint<'_, '_> {
    fn matches(self, ty: &Type<'_>) -> bool {
        use crate::analysis::TypeTerm as T;
        match (self, ty.as_ref()) {
            (Self::Primitive(p), T::Primitive(t)) => Some(p) == Primitive::named(t.name()),
            (
                Self::Nominal(d),
                T::Nominal {
                    declaration,
                    arguments,
                },
            ) => **declaration == d.id && arguments.is_empty(),
            (
                Self::List(d),
                T::Nominal {
                    declaration,
                    arguments,
                },
            ) => {
                **declaration == d.id
                    && matches!(arguments.as_slice(), [element] if Self::Primitive(Primitive::U8).matches(element))
            }
            (
                Self::Shared(inner),
                T::Reference {
                    mutable: false,
                    referent,
                },
            ) => inner.matches(referent),
            _ => false,
        }
    }
}

fn bytes_role<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    use crate::analysis::TypeTerm;
    let Some(bytes) = export(
        inventory.graph(),
        module,
        Namespace::TypeModule,
        "Bytes",
        budget,
    )?
    else {
        return Ok(None);
    };
    let Some(facts) = unconditional_declaration(inventory, bytes, budget)? else {
        return Ok(None);
    };
    let DeclarationShape::Record { fields } = &facts.shape else {
        return Ok(None);
    };
    let [field] = fields.as_slice() else {
        return Ok(None);
    };
    let TypeTerm::Nominal {
        declaration,
        arguments,
    } = field.ty.as_ref()
    else {
        return Ok(None);
    };
    budget.step(1)?;
    let storage = inventory.type_facts().declarations.get(declaration);
    Ok((facts.binders.is_empty() && arguments.is_empty() && !field.declaration.public
        && field.declaration.name.as_deref() == Some("storage")
        && storage.is_some_and(|storage| storage.declaration.id.module == module.key && !storage.declaration.public
            && storage.declaration.name.as_deref() == Some("BytesStorage") && storage.binders.is_empty()
            && matches!(&storage.shape, DeclarationShape::Enum { variants } if variants.is_empty())))
        .then_some(bytes))
}

fn utf8_error<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    option: OptionRole<'g>,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    use crate::analysis::TypeTerm;
    let graph = inventory.graph();
    let Some(module) = pinned_module(graph, "string.rill", budget)? else {
        return Ok(None);
    };
    let Some(error) = export(graph, module, Namespace::TypeModule, "Utf8Error", budget)? else {
        return Ok(None);
    };
    let Some(facts) = unconditional_declaration(inventory, error, budget)? else {
        return Ok(None);
    };
    let DeclarationShape::Record { fields } = &facts.shape else {
        return Ok(None);
    };
    let [valid, length] = fields.as_slice() else {
        return Ok(None);
    };
    let size = Endpoint::Primitive(Primitive::U64);
    Ok((facts.binders.is_empty() && valid.declaration.public && length.declaration.public
        && valid.declaration.name.as_deref() == Some("validUpTo") && size.matches(&valid.ty)
        && length.declaration.name.as_deref() == Some("errorLength")
        && matches!(length.ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
            if **declaration == option.declaration.id && matches!(arguments.as_slice(), [t] if size.matches(t))))
        .then_some(error))
}

fn result_matches(
    result: &Type<'_>,
    to: Endpoint<'_, '_>,
    fallible: Option<(&Declaration, &Declaration)>,
) -> bool {
    use crate::analysis::TypeTerm;
    match fallible {
        None => to.matches(result),
        Some((role, error)) => {
            matches!(result.as_ref(), TypeTerm::Nominal { declaration, arguments }
            if **declaration == role.id && matches!(arguments.as_slice(), [success, failure]
                if to.matches(success) && Endpoint::Nominal(error).matches(failure)))
        }
    }
}

fn returns_bridge(
    module: &Module,
    implementation: &Implementation,
    name: &str,
    budget: &mut Budget,
) -> Work<bool> {
    let seen = &module.syntax.nodes;
    let signature = &seen[implementation.signature_node];
    budget.step(signature.children.len() + 4)?;
    let Some(parameters) = signature
        .children
        .iter()
        .map(|&n| &seen[n])
        .find(|n| n.kind == NodeKind::ValueParameters)
    else {
        return Ok(false);
    };
    let [param] = parameters.children.as_slice() else {
        return Ok(false);
    };
    let param_name = &seen[seen[*param].children[0]];
    let [statement] = seen[implementation.body_node].children.as_slice() else {
        return Ok(false);
    };
    if seen[*statement].kind != NodeKind::Return {
        return Ok(false);
    }
    let [call] = seen[*statement].children.as_slice() else {
        return Ok(false);
    };
    let [callee, argument] = seen[*call].children.as_slice() else {
        return Ok(false);
    };
    Ok(seen[*call].kind == NodeKind::Call
        && seen[*callee].kind == NodeKind::ValueReference
        && seen[*callee].span.bytes() == name.as_bytes()
        && seen[*argument].kind == NodeKind::ValueReference
        && seen[*argument].span.bytes() == param_name.span.bytes())
}

fn route<'g>(
    graph: &'g Graph,
    module: &Module,
    name: &str,
    declaration: &Declaration,
    budget: &mut Budget,
) -> Work<Option<&'g Binding>> {
    for binding in graph.bindings() {
        budget.step(1)?;
        if binding.module == module.key
            && binding.public
            && binding.name == name
            && matches!(&binding.target, Target::Declaration(id) if *id == declaration.id)
        {
            return Ok(Some(binding));
        }
    }
    Ok(None)
}

fn unit_error<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    variants: &[&str],
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    let Some(decl) = export(
        inventory.graph(),
        module,
        Namespace::TypeModule,
        name,
        budget,
    )?
    else {
        return Ok(None);
    };
    let Some(facts) = unconditional_declaration(inventory, decl, budget)? else {
        return Ok(None);
    };
    let DeclarationShape::Enum { variants: found } = &facts.shape else {
        return Ok(None);
    };
    budget.step(found.len())?;
    Ok((facts.binders.is_empty()
        && found.len() == variants.len()
        && found.iter().zip(variants).all(|(variant, name)| {
            variant.declaration.public
                && variant.declaration.name.as_deref() == Some(*name)
                && matches!(variant.payload, Payload::Unit)
        }))
    .then_some(decl))
}

/// The inherited protection rule for an exact pair, independent of provider and representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionProtection {
    RequiredIdentity,
    RequiredNumeric,
    ProhibitedNumeric,
}

/// A specific linked conversion operation, not a representation or ABI claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionProvider {
    Identity,
    NumericInto(NumericConversion),
    NumericTryInto(NumericConversion),
    Parse(DecimalParse),
    CodePoint,
    UnicodeScalar,
    TextBytes,
    DecodeOwned,
    DecodeView,
    TakeList,
    CopyList,
    TakeBytes,
    CopyBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Width {
    Integer { bits: u32, signed: bool },
    Float,
}

fn width(primitive: Primitive) -> Option<Width> {
    use Primitive::*;
    let (bits, signed) = match primitive {
        I8 => (8, true),
        I16 => (16, true),
        I32 => (32, true),
        I64 => (64, true),
        I128 => (128, true),
        U8 => (8, false),
        U16 => (16, false),
        U32 => (32, false),
        U64 => (64, false),
        U128 => (128, false),
        F32 | F64 => return Some(Width::Float),
        Bool | Char | String => return None,
    };
    Some(Width::Integer { bits, signed })
}

/// The protected infallible policy. A gap prohibits an application registration;
/// it is not inferred from missing providers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfallibleNumericContract {
    RequiredProvider,
    Gap,
}

impl InfallibleNumericContract {
    pub fn for_pair(destination: Primitive, source: Primitive) -> Option<Self> {
        use Width::*;
        let allowed = match (width(destination)?, width(source)?) {
            (Float, _) => true,
            (Integer { .. }, Float) => false,
            (
                Integer {
                    bits: to,
                    signed: to_signed,
                },
                Integer {
                    bits: from,
                    signed: from_signed,
                },
            ) => {
                if to_signed == from_signed {
                    to >= from
                } else {
                    to_signed && to > from
                }
            }
        };
        Some(if allowed {
            Self::RequiredProvider
        } else {
            Self::Gap
        })
    }
}

/// The implemented scalar operation. The Rust result here is not a Rill Result or Err.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumericConversion {
    destination: Primitive,
    source: Primitive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericConversionError {
    OutOfRange,
    NotFinite,
}

impl NumericConversion {
    pub fn for_pair(destination: Primitive, source: Primitive) -> Option<Self> {
        // This table describes operation implementations, not availability of into.
        let numeric = |p| !matches!(p, Primitive::Bool | Primitive::Char | Primitive::String);
        (numeric(destination) && numeric(source)).then_some(Self {
            destination,
            source,
        })
    }

    pub fn destination(self) -> Primitive {
        self.destination
    }
    pub fn source(self) -> Primitive {
        self.source
    }

    pub fn convert(
        self,
        value: PrimitiveValue<'_>,
    ) -> Option<Result<PrimitiveValue<'static>, NumericConversionError>> {
        if value.primitive() != self.source {
            return None;
        }
        use Primitive::*;
        Some(match self.destination {
            I8 => to_integer!(value, I8, i8),
            I16 => to_integer!(value, I16, i16),
            I32 => to_integer!(value, I32, i32),
            I64 => to_integer!(value, I64, i64),
            I128 => to_integer!(value, I128, i128),
            U8 => to_integer!(value, U8, u8),
            U16 => to_integer!(value, U16, u16),
            U32 => to_integer!(value, U32, u32),
            U64 => to_integer!(value, U64, u64),
            U128 => to_integer!(value, U128, u128),
            F32 => Ok(PrimitiveValue::F32(to_float!(value, f32))),
            F64 => Ok(PrimitiveValue::F64(to_float!(value, f64))),
            Bool | Char | String => unreachable!("numeric operation has a numeric destination"),
        })
    }
}

// The same implemented operation covers every fallible numeric pair. An
// infallible gap is closed by the explicit contract, not by lack of an operation.
fn check_numeric_matrix(budget: &mut Budget) -> Work<bool> {
    for to in numeric_primitives() {
        for from in numeric_primitives() {
            budget.step(1)?;
            if InfallibleNumericContract::for_pair(to, from).is_none()
                || NumericConversion::for_pair(to, from).is_none()
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn numeric_primitives() -> impl Iterator<Item = Primitive> {
    Primitive::ALL
        .iter()
        .copied()
        .filter(|&p| width(p).is_some())
}

// The rounding of an integer to f32 must not pass through f64 first.
macro_rules! to_float {
    ($value:expr, $dest:ty) => {{
        use PrimitiveValue::*;
        match $value {
            I8(v) => v as $dest,
            I16(v) => v as $dest,
            I32(v) => v as $dest,
            I64(v) => v as $dest,
            I128(v) => v as $dest,
            U8(v) => v as $dest,
            U16(v) => v as $dest,
            U32(v) => v as $dest,
            U64(v) => v as $dest,
            U128(v) => v as $dest,
            F32(v) => v as $dest,
            F64(v) => v as $dest,
            Bool(_) | Char(_) | String(_) => unreachable!("numeric input"),
        }
    }};
}

macro_rules! to_integer {
    ($value:expr, $variant:ident, $dest:ty) => {{
        use PrimitiveValue::*;
        let checked = match $value {
            I8(v) => <$dest>::try_from(v).ok(),
            I16(v) => <$dest>::try_from(v).ok(),
            I32(v) => <$dest>::try_from(v).ok(),
            I64(v) => <$dest>::try_from(v).ok(),
            I128(v) => <$dest>::try_from(v).ok(),
            U8(v) => <$dest>::try_from(v).ok(),
            U16(v) => <$dest>::try_from(v).ok(),
            U32(v) => <$dest>::try_from(v).ok(),
            U64(v) => <$dest>::try_from(v).ok(),
            U128(v) => <$dest>::try_from(v).ok(),
            F32(v) => {
                return Some(
                    float_integer(f64::from(v), <$dest>::BITS, <$dest>::MIN != 0)
                        .map(|v| $variant(v as $dest)),
                );
            }
            F64(v) => {
                return Some(
                    float_integer(v, <$dest>::BITS, <$dest>::MIN != 0)
                        .map(|v| $variant(v as $dest)),
                );
            }
            Bool(_) | Char(_) | String(_) => unreachable!("numeric input"),
        };
        checked
            .map($variant)
            .ok_or(NumericConversionError::OutOfRange)
    }};
}

use {to_float, to_integer};

fn float_integer(value: f64, bits: u32, signed: bool) -> Result<f64, NumericConversionError> {
    if !value.is_finite() {
        return Err(NumericConversionError::NotFinite);
    }
    let truncated = value.trunc();
    // Powers of two are exact, including 2^128. Max integer endpoints are not.
    let upper = 2.0f64.powi((bits - u32::from(signed)) as i32);
    let lower = if signed { -upper } else { 0.0 };
    if truncated < lower || truncated >= upper {
        return Err(NumericConversionError::OutOfRange);
    }
    Ok(truncated)
}

const DECIMAL_PARSERS: [(Primitive, &str); 12] = [
    (Primitive::I8, "parseI8"),
    (Primitive::I16, "parseI16"),
    (Primitive::I32, "parseI32"),
    (Primitive::I64, "parseI64"),
    (Primitive::I128, "parseI128"),
    (Primitive::U8, "parseU8"),
    (Primitive::U16, "parseU16"),
    (Primitive::U32, "parseU32"),
    (Primitive::U64, "parseU64"),
    (Primitive::U128, "parseU128"),
    (Primitive::F32, "parseF32"),
    (Primitive::F64, "parseF64"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecimalParse(Primitive);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecimalParseError {
    InvalidSyntax,
    OutOfRange,
}

impl DecimalParse {
    pub fn destination(self) -> Primitive {
        self.0
    }

    pub fn parse(self, text: &str) -> Result<PrimitiveValue<'static>, DecimalParseError> {
        use DecimalParseError::*;
        let numeric_width = width(self.0).expect("registered decimal width");
        let special = numeric_width == Width::Float && matches!(text, "NaN" | "inf" | "-inf");
        if !special && !strict_decimal(text.as_bytes(), numeric_width) {
            return Err(InvalidSyntax);
        }
        macro_rules! integer {
            ($variant:ident, $ty:ty) => {
                text.parse::<$ty>()
                    .map(PrimitiveValue::$variant)
                    .map_err(|_| OutOfRange)
            };
        }
        macro_rules! float {
            ($variant:ident, $ty:ty) => {{
                let value = text.parse::<$ty>().map_err(|_| OutOfRange)?;
                if special || value.is_finite() {
                    Ok(PrimitiveValue::$variant(value))
                } else {
                    Err(OutOfRange)
                }
            }};
        }
        match self.0 {
            Primitive::I8 => integer!(I8, i8),
            Primitive::I16 => integer!(I16, i16),
            Primitive::I32 => integer!(I32, i32),
            Primitive::I64 => integer!(I64, i64),
            Primitive::I128 => integer!(I128, i128),
            Primitive::U8 => integer!(U8, u8),
            Primitive::U16 => integer!(U16, u16),
            Primitive::U32 => integer!(U32, u32),
            Primitive::U64 => integer!(U64, u64),
            Primitive::U128 => integer!(U128, u128),
            Primitive::F32 => float!(F32, f32),
            Primitive::F64 => float!(F64, f64),
            _ => unreachable!("registered decimal width"),
        }
    }
}

fn strict_decimal(mut bytes: &[u8], width: Width) -> bool {
    fn digits(bytes: &mut &[u8]) -> bool {
        let length = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        *bytes = &bytes[length..];
        length != 0
    }
    if !matches!(width, Width::Integer { signed: false, .. })
        && let Some(rest) = bytes.strip_prefix(b"-")
    {
        bytes = rest;
    }
    if !digits(&mut bytes) {
        return false;
    }
    if width == Width::Float {
        if let Some(rest) = bytes.strip_prefix(b".") {
            bytes = rest;
            if !digits(&mut bytes) {
                return false;
            }
        }
        if let [b'e' | b'E', rest @ ..] = bytes {
            bytes = match rest {
                [b'+' | b'-', rest @ ..] => rest,
                _ => rest,
            };
            if !digits(&mut bytes) {
                return false;
            }
        }
    }
    bytes.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use PrimitiveValue as V;

    #[test]
    fn each_numeric_pair_has_an_operation_but_only_64_have_infallible_contracts() {
        use Primitive::*;
        let examples = [
            (V::I8(i8::MIN), 7),
            (V::I16(i16::MIN), 6),
            (V::I32(i32::MIN), 5),
            (V::I64(i64::MIN), 4),
            (V::I128(i128::MIN), 3),
            (V::U8(u8::MAX), 11),
            (V::U16(u16::MAX), 9),
            (V::U32(u32::MAX), 7),
            (V::U64(u64::MAX), 5),
            (V::U128(u128::MAX), 3),
            (V::F32(f32::MAX), 2),
            (V::F64(f64::MAX), 2),
        ];
        for (value, allowed) in examples {
            let from = value.primitive();
            let mut required = 0;
            for to in [I8, I16, I32, I64, I128, U8, U16, U32, U64, U128, F32, F64] {
                let contract = InfallibleNumericContract::for_pair(to, from).unwrap();
                let operation = NumericConversion::for_pair(to, from).unwrap();
                let result = operation.convert(value).unwrap();
                if contract == InfallibleNumericContract::RequiredProvider {
                    required += 1;
                    assert_eq!(result.unwrap().primitive(), to);
                }
            }
            assert_eq!(required, allowed, "from {from:?}");
        }
        assert!(InfallibleNumericContract::for_pair(I32, Bool).is_none());
        assert!(NumericConversion::for_pair(I32, Bool).is_none());
    }

    #[test]
    fn numeric_operation_checks_range_after_truncation_without_lossy_boundary_casts() {
        let convert = |to, from: V<'_>| {
            NumericConversion::for_pair(to, from.primitive())
                .unwrap()
                .convert(from)
                .unwrap()
        };
        use NumericConversionError::*;
        assert_eq!(convert(Primitive::U8, V::F64(-0.5)), Ok(V::U8(0)));
        assert_eq!(convert(Primitive::U8, V::F64(-1.0)), Err(OutOfRange));
        assert_eq!(convert(Primitive::U8, V::F64(f64::NAN)), Err(NotFinite));
        assert_eq!(
            convert(Primitive::U128, V::F64(2.0f64.powi(128))),
            Err(OutOfRange)
        );
        assert_eq!(
            convert(Primitive::I128, V::F64(-2.0f64.powi(127))),
            Ok(V::I128(i128::MIN))
        );
        assert_eq!(
            convert(Primitive::I128, V::F64(2.0f64.powi(127))),
            Err(OutOfRange)
        );
        assert_eq!(
            convert(Primitive::U128, V::U128(u128::MAX)),
            Ok(V::U128(u128::MAX))
        );
        assert_eq!(
            convert(Primitive::I128, V::U128(u128::MAX)),
            Err(OutOfRange)
        );
        assert_eq!(
            convert(Primitive::F32, V::U128(u128::MAX)),
            Ok(V::F32(f32::INFINITY))
        );
    }
}
