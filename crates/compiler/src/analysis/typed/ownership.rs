//! Static bindings for exact regions of the canonical copy/dispose families.
use super::core::{export, parameter_is, unconditional_declaration};
use super::*;
use crate::analysis::ordinary::{
    Compatibility, FamilyFacts, ImplementationFacts, Placement, patterns,
};
use crate::analysis::{Declaration, DeclarationFacts, DeclarationShape, Type, TypeTerm};
use crate::loader::{DeclarationId, Module, Namespace};
use std::{path::Path, sync::Arc};

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/ownership/bindings.rs"
));

macro_rules! operations {
    ($($action:ident => $module:ident::$operation:ident, $entry:literal, $body:literal;)*) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum NativeOwnershipAction { $($action,)* }

        impl NativeOwnershipAction {
            /// Implemented Rust item in the pinned archive, not a Rill ABI symbol.
            pub fn rust_item(self) -> &'static str {
                match self {
                    $(Self::$action => concat!("rill_ownership::", stringify!($module), "::", stringify!($operation)),)*
                }
            }

            fn delegation(self) -> (&'static str, &'static [u8]) {
                match self { $(Self::$action => ($entry, $body.as_bytes()),)* }
            }
        }
    };
}
ownership_bindings!(operations);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnershipRule {
    Required(NativeOwnershipAction),
    /// Guard and receiving endpoint ownership have no copy action.
    ProhibitedCopy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnershipCoverage {
    /// A prerequisite or a contender is unresolved. Known overlaps may still be reported.
    Pending,
    /// All captured contenders were checked for this input region, with no overlap violation.
    Clear,
    /// All captured contenders were checked and at least one violates this region.
    Overlap,
}

#[derive(Debug)]
pub struct NativeOwnershipProvider<'g> {
    pub implementation: &'g Declaration,
    pub entry: &'g Declaration,
    pub artifact: &'g Path,
}

#[derive(Debug)]
pub struct OwnershipProtection<'g> {
    pub owner: &'g Declaration,
    pub family: &'g DeclarationId,
    pub rule: OwnershipRule,
    pub provider: Option<NativeOwnershipProvider<'g>>,
    pub coverage: OwnershipCoverage,
}

#[derive(Debug)]
pub struct OwnershipOverlap<'g> {
    pub owner: &'g Declaration,
    pub family: &'g DeclarationId,
    pub implementation: &'g Declaration,
    pub rule: OwnershipRule,
    /// Indices into the protected and contender alternatives, respectively.
    pub intersection: (usize, usize),
}

impl OwnershipOverlap<'_> {
    pub fn code(&self) -> &'static str {
        "R0500"
    }
}

#[derive(Debug, Default)]
pub(super) struct Report<'g> {
    pub(super) protection: Vec<OwnershipProtection<'g>>,
    pub(super) overlaps: Vec<OwnershipOverlap<'g>>,
}

struct Owner(
    &'static str,
    &'static str,
    NativeOwnershipAction,
    Option<NativeOwnershipAction>,
);

// Distinct canonical record heads give disjoint regions within each family.
// Absence of retain here is an explicit prohibition, not a provider gap.
const OWNERS: &[Owner] = {
    use NativeOwnershipAction::*;
    &[
        Owner("shared.rill", "Shared", SharedRelease, Some(SharedRetain)),
        Owner("mutex.rill", "LockGuard", GuardRelease, None),
        Owner("channel.rill", "Sender", SenderRelease, Some(SenderRetain)),
        Owner("channel.rill", "Receiver", ReceiverRelease, None),
    ]
};

pub(super) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    budget: &mut Budget,
    report: &mut Report<'g>,
) -> Work<()> {
    let graph = inventory.graph();
    for module in graph.modules() {
        budget.step(1)?;
        if !graph.is_pinned_standard_source(module) {
            continue;
        }
        for &Owner(path, name, dispose, retain) in OWNERS {
            budget.step(1)?;
            if path != module.key.path {
                continue;
            }
            let Some(owner) = export(graph, module, Namespace::TypeModule, name, budget)? else {
                continue;
            };
            let Some(facts) = unconditional_declaration(inventory, owner, budget)? else {
                continue;
            };
            let (DeclarationShape::Record { fields }, [_]) =
                (&facts.shape, facts.binders.as_slice())
            else {
                continue;
            };
            if !matches!(fields.as_slice(), [field] if !field.declaration.public) {
                continue;
            }
            let release = Scope {
                module,
                owner,
                family: &graph.prelude().dispose,
                rule: OwnershipRule::Required(dispose),
            };
            let release = check_required(inventory, release, dispose, budget, report)?;
            let copy = Scope {
                module,
                owner,
                family: &graph.prelude().copy,
                rule: retain.map_or(OwnershipRule::ProhibitedCopy, OwnershipRule::Required),
            };
            if let Some(retain) = retain {
                check_required(inventory, copy, retain, budget, report)?;
            } else {
                // A prohibition has no provider or dominance exception. Its
                // owner must still have genuine native evidence.
                let input = release
                    .map(|body| forbidden_copy(body, budget))
                    .transpose()?;
                check_scope(
                    inventory,
                    copy,
                    None,
                    input.as_ref().map(std::slice::from_ref),
                    budget,
                    report,
                )?;
            }
        }
    }
    Ok(())
}

struct Scope<'g> {
    module: &'g Module,
    owner: &'g Declaration,
    family: &'g DeclarationId,
    rule: OwnershipRule,
}

fn check_required<'a, 'g>(
    inventory: &'a OrdinaryAnalysis<'g>,
    scope: Scope<'g>,
    action: NativeOwnershipAction,
    budget: &mut Budget,
    report: &mut Report<'g>,
) -> Work<Option<&'a ImplementationFacts<'g>>> {
    let provider = find_provider(inventory, &scope, action, budget)?;
    let delivered = provider.as_ref().map(|(_, body)| *body);
    let alternatives = delivered.map(|body| body.alternatives.as_slice());
    check_scope(
        inventory,
        scope,
        provider.map(|(evidence, _)| evidence),
        alternatives,
        budget,
        report,
    )?;
    Ok(delivered)
}

fn forbidden_copy<'g>(
    release: &ImplementationFacts<'g>,
    budget: &mut Budget,
) -> Work<patterns::InputKey<'g>> {
    let DeclarationShape::Function { parameters, .. } = &release.header.shape else {
        unreachable!("validated release signature")
    };
    budget.reserve(1)?;
    let input = Arc::new(TypeTerm::Reference {
        mutable: false,
        referent: parameters[0].ty.clone(),
    });
    patterns::normalized(std::iter::once(&input), &release.header.binders, budget)
        .map(|(key, _)| key)
}

fn family<'a, 'g>(
    inventory: &'a OrdinaryAnalysis<'g>,
    scope: &Scope<'g>,
    budget: &mut Budget,
) -> Work<Option<&'a FamilyFacts<'g>>> {
    budget.step(2)?;
    Ok(inventory.captured_family(scope.family).filter(|f| {
        scope.family.module.path == "core.rill"
            && !inventory.declaration_unusable(&f.header.declaration.id)
    }))
}

fn find_provider<'a, 'g>(
    inventory: &'a OrdinaryAnalysis<'g>,
    scope: &Scope<'g>,
    action: NativeOwnershipAction,
    budget: &mut Budget,
) -> Work<Option<(NativeOwnershipProvider<'g>, &'a ImplementationFacts<'g>)>> {
    let Some(artifact) = inventory.graph().ownership_archive() else {
        return Ok(None);
    };
    let Some(family) = family(inventory, scope, budget)? else {
        return Ok(None);
    };
    let (entry_name, wrapper) = action.delegation();
    let mut entry = None;
    for declaration in &scope.module.declarations {
        budget.step(1)?;
        if !declaration.public
            && declaration.name.as_deref() == Some(entry_name)
            && let Some(facts) = inventory.captured_family(&declaration.id)
            && facts.implementations.is_empty()
            && closed_signature(inventory, &facts.header, scope, budget)?
        {
            entry = Some(declaration);
            break;
        }
    }
    let Some(entry) = entry else {
        return Ok(None);
    };
    for body in &family.implementations {
        budget.step(1)?;
        if body.source.declaration.module == scope.module.key
            && scope.module.syntax.nodes[body.source.body_node]
                .span
                .bytes()
                == wrapper
            && body.compatibility == Compatibility::Compatible
            && body.placement == Placement::ParameterOwner(&scope.owner.id)
            && !body.alternatives.is_empty()
            && closed_signature(inventory, &body.header, scope, budget)?
        {
            return Ok(Some((
                NativeOwnershipProvider {
                    implementation: body.header.declaration,
                    entry,
                    artifact,
                },
                body,
            )));
        }
    }
    Ok(None)
}

fn closed_signature(
    inventory: &OrdinaryAnalysis<'_>,
    header: &DeclarationFacts<'_>,
    scope: &Scope<'_>,
    budget: &mut Budget,
) -> Work<bool> {
    budget.step(3)?;
    if inventory.declaration_unusable(&header.declaration.id)
        || !inventory
            .headers()
            .get(&header.declaration.id)
            .is_some_and(|h| h.defaults.is_empty() && h.requirements.is_empty())
    {
        return Ok(false);
    }
    let (DeclarationShape::Function { parameters, result }, [binder]) =
        (&header.shape, header.binders.as_slice())
    else {
        return Ok(false);
    };
    let [input] = parameters.as_slice() else {
        return Ok(false);
    };
    if input.declaration.name.as_deref() != Some("value") {
        return Ok(false);
    }
    let owner = |term: &Type<'_>| {
        matches!(term.as_ref(), TypeTerm::Nominal { declaration, arguments }
        if **declaration == scope.owner.id
            && matches!(arguments.as_slice(), [arg] if parameter_is(arg, binder.declaration)))
    };
    Ok(if scope.family == &inventory.graph().prelude().copy {
        matches!(input.ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if owner(referent))
            && owner(result)
    } else {
        owner(&input.ty) && matches!(result.as_ref(), TypeTerm::Primitive(p) if p.name() == "void")
    })
}

fn check_scope<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    scope: Scope<'g>,
    provider: Option<NativeOwnershipProvider<'g>>,
    alternatives: Option<&[patterns::InputKey<'g>]>,
    budget: &mut Budget,
    report: &mut Report<'g>,
) -> Work<()> {
    let winner = provider.as_ref().map(|p| &p.implementation.id);
    budget.reserve(1)?;
    let index = report.protection.len();
    report.protection.push(OwnershipProtection {
        owner: scope.owner,
        family: scope.family,
        rule: scope.rule,
        provider,
        coverage: OwnershipCoverage::Pending,
    });
    let Some(protected) = alternatives else {
        return Ok(());
    };
    let Some(family) = family(inventory, &scope, budget)? else {
        return Ok(());
    };

    // Collection can omit an unresolved header. Counting graph declarations
    // prevents the narrower typed inventory from silently closing this region.
    let mut captured = 0;
    for body in inventory.graph().implementations() {
        budget.step(1)?;
        captured += usize::from(body.family == *scope.family);
    }
    let mut complete = captured == family.implementations.len();
    let before = report.overlaps.len();
    for body in &family.implementations {
        budget.step(1)?;
        if Some(&body.header.declaration.id) == winner {
            continue;
        }
        let input = &body.alternatives;
        if input.is_empty() {
            complete = false;
            continue;
        }
        if let Some(intersection) = patterns::intersection(protected, input, budget)?
            && (scope.rule == OwnershipRule::ProhibitedCopy
                || !(patterns::included(protected, input, budget)?
                    && !patterns::included(input, protected, budget)?))
        {
            budget.reserve(1)?;
            report.overlaps.push(OwnershipOverlap {
                owner: scope.owner,
                family: scope.family,
                rule: scope.rule,
                implementation: body.header.declaration,
                intersection,
            });
        }
    }
    if complete {
        report.protection[index].coverage = if report.overlaps.len() == before {
            OwnershipCoverage::Clear
        } else {
            OwnershipCoverage::Overlap
        };
    }
    Ok(())
}
