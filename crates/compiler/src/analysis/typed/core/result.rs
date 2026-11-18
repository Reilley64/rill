//! Capture-local nominal roles. These checks do not admit opaque storage or actions.
use super::*;
use crate::analysis::MemberType;

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
    budget.reserve(1)?;
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
    }))
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
