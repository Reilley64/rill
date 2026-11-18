//! Capture-local nominal roles. These checks do not admit opaque storage or actions.
use super::*;
use crate::analysis::{MemberType, ordinary::RequirementTarget};
use rill_error_storage::{Cause, Error, OwnedErr, SourceLocation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorOperation {
    Construct,
    Retain,
    Wrap,
    RenderLocation,
    RenderCause,
    RenderError,
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorProviders<'g> {
    pub constructor: &'g Declaration,
    pub storage_source: &'g std::path::Path,
    pub renderers: [Option<ErrorRenderer<'g>>; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorRenderer<'g> {
    pub operation: ErrorOperation,
    pub entry: &'g Declaration,
}

impl ErrorOperation {
    pub fn rust_binding(self) -> &'static str {
        match self {
            Self::Construct => "rill_error_storage::OwnedErr::new",
            Self::Retain => "rill_error_storage::OwnedErr::wrapping",
            Self::Wrap => "rill_error_storage::wrap_result",
            Self::RenderLocation => "rill_error_storage::SourceLocation::format",
            Self::RenderCause => "rill_error_storage::Cause::format",
            Self::RenderError => "rill_error_storage::OwnedErr::format",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorRepresentation<'g> {
    pub storage_source: &'g std::path::Path,
    pub wrapper: &'g Declaration,
    pub payload_parameter: &'g Declaration,
    pub payload_field: &'g Declaration,
    pub retained_field: &'g Declaration,
    pub retained_some: &'g Declaration,
    pub source_location: &'g Declaration,
    pub cause: &'g Declaration,
    pub message: &'g Declaration,
}

// Validate the bindings at their actual Rust signatures, including a primary
// E with no erasure bounds and a separately bounded retained C.
const fn bind_error_operations<
    T,
    E,
    C: Send + Sync + 'static,
    F: FnOnce() -> (E, SourceLocation),
>() {
    let _: fn(&str, u64) -> SourceLocation = SourceLocation::new;
    let _: fn(String) -> Error = Error::new;
    let _: fn(E, SourceLocation) -> OwnedErr<E> = OwnedErr::new;
    let _: Retain<E, C> = OwnedErr::wrapping;
    let _: Wrap<T, E, C, F> = rill_error_storage::wrap_result;
    let _: fn(&SourceLocation) -> String = SourceLocation::format;
    let _: fn(&Cause) -> String = Cause::format;
    let _: fn(&OwnedErr<E>, fn(&E) -> String) -> String = OwnedErr::format;
    assert_sealed::<Error>();
    assert_sealed::<SourceLocation>();
    assert_sealed::<OwnedErr<()>>();
}
const fn assert_sealed<T: Send + Sync + 'static>() {}
type Retain<E, C> = fn(E, SourceLocation, OwnedErr<C>, fn(&C) -> String) -> OwnedErr<E>;
type Wrap<T, E, C, F> = fn(Result<T, OwnedErr<C>>, F, fn(&C) -> String) -> Result<T, OwnedErr<E>>;
const _: () = bind_error_operations::<(), (), (), fn() -> ((), SourceLocation)>();

#[derive(Clone, Copy, Debug)]
pub struct ResultRole<'g> {
    pub declaration: &'g Declaration,
    pub ok: &'g Declaration,
    pub err: &'g Declaration,
    pub wrapper: &'g Declaration,
    pub default_error: &'g Declaration,
    pub wrapper_payload: &'g Declaration,
    pub source_location: &'g Declaration,
    pub cause: &'g Declaration,
    /// Value-namespace role, separate from the wrapper type and all providers.
    pub constructor: Option<&'g Declaration>,
    pub display: Option<&'g Declaration>,
    /// Closed constructor accepts no ordinary registrations, for either arity.
    pub constructor_protected: bool,
    pub display_protection: Option<DisplayProtection<'g>>,
    pub providers: Option<ErrorProviders<'g>>,
    pub representation: Option<ErrorRepresentation<'g>>,
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayProtection<'g> {
    /// Display is ordinary dispatch. RFC 0008 specifies no protected actions for
    /// its single borrowed input; this clearance is not inherited by its callees.
    pub family: &'g Declaration,
}

fn arguments_of<'a, 'g>(term: &'a Type<'g>, owner: &Declaration) -> Option<&'a [Type<'g>]> {
    match term.as_ref() {
        TypeTerm::Nominal {
            declaration,
            arguments,
        } if **declaration == owner.id => Some(arguments),
        _ => None,
    }
}

fn private_field(field: &MemberType<'_>, name: &str) -> bool {
    !field.declaration.public && field.declaration.name.as_deref() == Some(name)
}

fn private_primitive(field: &MemberType<'_>, name: &str, primitive: &str) -> bool {
    private_field(field, name)
        && matches!(field.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == primitive)
}

pub(in crate::analysis::typed) fn check_result<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    option: OptionRole<'g>,
    budget: &mut Budget,
) -> Work<Option<ResultRole<'g>>> {
    let graph = inventory.graph();
    let (Some(result_module), Some(error_module)) = (
        pinned_module(graph, "result.rill", budget)?,
        pinned_module(graph, "error.rill", budget)?,
    ) else {
        return Ok(None);
    };
    let mut role = |module, name| export(graph, module, Namespace::TypeModule, name, budget);
    let (Some(result), Some(wrapper), Some(error), Some(site), Some(cause)) = (
        role(result_module, "Result")?,
        role(result_module, "Err")?,
        role(error_module, "Error")?,
        role(error_module, "SourceLocation")?,
        role(error_module, "Cause")?,
    ) else {
        return Ok(None);
    };
    let mut plain = |decl| unconditional_declaration(inventory, decl, budget);
    let (Some(envelope), Some(error_facts), Some(site_facts), Some(cause_facts)) =
        (plain(wrapper)?, plain(error)?, plain(site)?, plain(cause)?)
    else {
        return Ok(None);
    };
    let (DeclarationShape::Record { fields }, [payload_type]) =
        (&envelope.shape, envelope.binders.as_slice())
    else {
        return Ok(None);
    };
    let [payload, location, retained] = fields.as_slice() else {
        return Ok(None);
    };
    if !private_field(payload, "payload")
        || !binder_is(&payload.ty, payload_type)
        || !private_field(location, "site")
        || !matches!(arguments_of(&location.ty, site), Some([]))
        || !private_field(retained, "retained")
        || !matches!(arguments_of(&retained.ty, option.declaration), Some([arg]) if matches!(arguments_of(arg, cause), Some([])))
        || !matches!((&site_facts.shape, site_facts.binders.as_slice()), (DeclarationShape::Record { fields }, [])
            if matches!(fields.as_slice(), [file, offset] if private_primitive(file, "file", "string")
                && private_primitive(offset, "byteOffset", "u64")))
        || !matches!((&cause_facts.shape, cause_facts.binders.as_slice()), (DeclarationShape::Record { fields }, [])
            if matches!(fields.as_slice(), [storage] if private_primitive(storage, "storage", "never")))
        || !matches!((&error_facts.shape, error_facts.binders.as_slice()), (DeclarationShape::Record { fields }, [])
            if matches!(fields.as_slice(), [message] if private_primitive(message, "message", "string")))
    {
        return Ok(None);
    }
    budget.step(3)?;
    let (Some(facts), Some(header)) = (
        inventory.type_facts().declarations.get(&result.id),
        inventory.headers().get(&result.id),
    ) else {
        return Ok(None);
    };
    if inventory.declaration_unusable(&result.id) || !header.requirements.is_empty() {
        return Ok(None);
    }
    let (DeclarationShape::Enum { variants }, [t, e], [default]) = (
        &facts.shape,
        facts.binders.as_slice(),
        header.defaults.as_slice(),
    ) else {
        return Ok(None);
    };
    if *default.binder != e.declaration.id
        || !matches!(
            default.ty.as_ref().and_then(|ty| arguments_of(ty, error)),
            Some([])
        )
    {
        return Ok(None);
    }
    let [ok, err] = variants.as_slice() else {
        return Ok(None);
    };
    if ok.declaration.name.as_deref() != Some("Ok")
        || err.declaration.name.as_deref() != Some("Err")
        || !ok.declaration.public
        || !err.declaration.public
        || !matches!(&ok.payload, Payload::Tuple(args) if matches!(args.as_slice(), [arg] if binder_is(arg, t)))
        || !matches!(&err.payload, Payload::Tuple(args) if matches!(args.as_slice(), [arg]
            if matches!(arguments_of(arg, wrapper), Some([term]) if binder_is(term, e))))
    {
        return Ok(None);
    }
    let constructor = match export(
        graph,
        result_module,
        Namespace::ValueFunction,
        "Err",
        budget,
    )? {
        Some(candidate) if constructor_matches(inventory, candidate, result, budget)? => {
            Some(candidate)
        }
        _ => None,
    };
    let constructor_protected = if let Some(constructor) = constructor {
        let mut no_registrations = true;
        for registration in graph.implementations() {
            budget.step(1)?;
            no_registrations &= registration.family != constructor.id;
        }
        no_registrations
    } else {
        false
    };
    let display = match export(
        graph,
        error_module,
        Namespace::ValueFunction,
        "display",
        budget,
    )? {
        Some(candidate) if display_matches(inventory, candidate, budget)? => Some(candidate),
        _ => None,
    };
    let display_protection = match display {
        Some(family)
            if crate::analysis::ordinary::source::family(inventory, &family.id, budget)?
                .is_ok() =>
        {
            budget.reserve(1)?;
            Some(DisplayProtection { family })
        }
        _ => None,
    };
    let representation = graph
        .error_storage_source()
        .map(|storage_source| ErrorRepresentation {
            storage_source,
            wrapper,
            payload_parameter: payload_type.declaration,
            payload_field: payload.declaration,
            retained_field: retained.declaration,
            retained_some: option.some,
            source_location: site,
            cause,
            message: error,
        });
    let providers = if let Some((constructor, storage_source)) =
        constructor.zip(graph.error_storage_source())
    {
        let mut renderers = [None; 3];
        for (slot, (module, name, subject, operation)) in renderers.iter_mut().zip([
            (
                error_module,
                "renderLocation",
                site,
                ErrorOperation::RenderLocation,
            ),
            (
                error_module,
                "renderCause",
                cause,
                ErrorOperation::RenderCause,
            ),
            (
                result_module,
                "renderError",
                wrapper,
                ErrorOperation::RenderError,
            ),
        ]) {
            *slot = renderer(
                inventory,
                module,
                (name, subject, operation),
                display,
                budget,
            )?;
        }
        Some(ErrorProviders {
            constructor,
            storage_source,
            renderers,
        })
    } else {
        None
    };
    budget.reserve(1 + usize::from(representation.is_some()) + usize::from(providers.is_some()))?;
    Ok(Some(ResultRole {
        declaration: result,
        ok: ok.declaration,
        err: err.declaration,
        wrapper,
        default_error: error,
        wrapper_payload: payload_type.declaration,
        source_location: site,
        cause,
        constructor,
        constructor_protected,
        display,
        display_protection,
        providers,
        representation,
    }))
}

fn renderer<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    (name, owner, operation): (&str, &'g Declaration, ErrorOperation),
    display: Option<&'g Declaration>,
    budget: &mut Budget,
) -> Work<Option<ErrorRenderer<'g>>> {
    let mut entry = None;
    for declaration in &module.declarations {
        budget.step(1)?;
        if declaration.name.as_deref() == Some(name)
            && declaration.id.kind == DeclarationKind::Family
            && !declaration.public
        {
            entry = Some(declaration);
            break;
        }
    }
    let Some(entry) = entry else {
        return Ok(None);
    };
    budget.step(3)?;
    let (Some(family), Some(header)) = (
        inventory.captured_family(&entry.id),
        inventory.headers().get(&entry.id),
    ) else {
        return Ok(None);
    };
    if inventory.declaration_unusable(&entry.id)
        || !header.defaults.is_empty()
        || !family.implementations.is_empty()
    {
        return Ok(None);
    }
    let DeclarationShape::Function { parameters, result } = &family.header.shape else {
        return Ok(None);
    };
    let [parameter] = parameters.as_slice() else {
        return Ok(None);
    };
    let TypeTerm::Reference {
        mutable: false,
        referent,
    } = parameter.ty.as_ref()
    else {
        return Ok(None);
    };
    let Some(arguments) = arguments_of(referent, owner) else {
        return Ok(None);
    };
    let schema = match (
        operation,
        family.header.binders.as_slice(),
        arguments,
        header.requirements.as_slice(),
    ) {
        (ErrorOperation::RenderError, [e], [arg], [requirement]) => {
            binder_is(arg, e)
                && display.is_some_and(|display| {
                    requirement.target == Some(RequirementTarget::Family(&display.id))
                })
                && matches!(requirement.arguments.as_slice(), [Some(ty)] if binder_is(ty, e))
        }
        (ErrorOperation::RenderCause | ErrorOperation::RenderLocation, [], [], []) => true,
        _ => false,
    };
    if !schema || !matches!(result.as_ref(), TypeTerm::Primitive(ty) if ty.name() == "string") {
        return Ok(None);
    }
    budget.reserve(1)?;
    Ok(Some(ErrorRenderer { operation, entry }))
}

fn display_matches(
    inventory: &OrdinaryAnalysis<'_>,
    candidate: &Declaration,
    budget: &mut Budget,
) -> Work<bool> {
    budget.step(3)?;
    if candidate.id.kind != DeclarationKind::Family
        || inventory.declaration_unusable(&candidate.id)
        || !inventory
            .headers()
            .get(&candidate.id)
            .is_some_and(|header| header.defaults.is_empty() && header.requirements.is_empty())
    {
        return Ok(false);
    }
    let Some(family) = inventory.captured_family(&candidate.id) else {
        return Ok(false);
    };
    let (DeclarationShape::Function { parameters, result }, [e]) =
        (&family.header.shape, family.header.binders.as_slice())
    else {
        return Ok(false);
    };
    Ok(
        matches!(parameters.as_slice(), [payload] if matches!(payload.ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if binder_is(referent, e)))
            && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "string"),
    )
}

fn constructor_matches(
    inventory: &OrdinaryAnalysis<'_>,
    candidate: &Declaration,
    result: &Declaration,
    budget: &mut Budget,
) -> Work<bool> {
    budget.step(3)?;
    if inventory.declaration_unusable(&candidate.id)
        || !inventory
            .headers()
            .get(&candidate.id)
            .is_some_and(|header| header.defaults.is_empty() && header.requirements.is_empty())
    {
        return Ok(false);
    }
    let Some(family) = inventory.captured_family(&candidate.id) else {
        return Ok(false);
    };
    let facts = &family.header;
    let (
        DeclarationShape::Function {
            parameters,
            result: output,
        },
        [t, e],
    ) = (&facts.shape, facts.binders.as_slice())
    else {
        return Ok(false);
    };
    Ok(candidate.id.kind == DeclarationKind::Family
        && matches!(parameters.as_slice(), [payload] if binder_is(&payload.ty, e))
        && matches!(arguments_of(output, result), Some([success, error]) if binder_is(success, t) && binder_is(error, e)))
}
