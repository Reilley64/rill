//! Capture-bound admission of the linked List operations; no native ABI.
use super::*;
use crate::analysis::{Declaration, DeclarationShape, Type, TypeTerm};
use crate::loader::{Module, Namespace};
use rill_collection::{List, ListIter, ListIterMut, OwnedListIter};
use std::ops::{Index, IndexMut};

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::analysis) struct CollectionCore<'g> {
    pub list: Option<ListRole<'g>>,
    pub lookup: Option<&'g Declaration>,
    pub iteration: Option<IterationCore<'g>>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::analysis) struct IterationCore<'g> {
    pub iterate: &'g Declaration,
    pub next: &'g Declaration,
    bridges: [Option<&'g Declaration>; 6],
}

impl<'g> CollectionCore<'g> {
    pub(super) fn check(
        &mut self,
        inventory: &OrdinaryAnalysis<'g>,
        option: Option<OptionRole<'g>>,
        budget: &mut Budget,
    ) -> Work<()> {
        self.list = check_list(inventory, budget)?;
        self.lookup = check_lookup(inventory, self.list, option, budget)?;
        self.iteration = check_iteration(inventory, self.list, option, budget)?;
        Ok(())
    }

    pub fn is_bridge(self, id: &crate::loader::DeclarationId) -> bool {
        self.lookup.is_some_and(|bridge| bridge.id == *id)
            || self
                .iteration
                .is_some_and(|iter| iter.bridges.iter().flatten().any(|bridge| bridge.id == *id))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ListRole<'g> {
    pub declaration: &'g Declaration,
    pub element: &'g Declaration,
}

impl<'g> ListRole<'g> {
    pub(in crate::analysis) fn element(self, ty: &Type<'g>) -> Option<Type<'g>> {
        match ty.as_ref() {
            TypeTerm::Nominal {
                declaration,
                arguments,
            } if **declaration == self.declaration.id => {
                let [element] = arguments.as_slice() else {
                    return None;
                };
                Some(element.clone())
            }
            _ => None,
        }
    }
}

// Check these bindings for *every* T, not merely a concrete test element. The
// compiler and its installed probe link the same Rust implementation. These are
// compiler operations, not authority for an installed native ABI or Rill layout.
const fn bind_list_operations<T>() {
    let _: fn(Vec<T>) -> List<T> = List::from_elements;
    let _: fn(&List<T>, u64) -> &T = List::index;
    let _: fn(&mut List<T>, u64) -> &mut T = List::index_mut;
    let _: fn(&List<T>, u64) -> Option<&T> = List::get;
    let _: fn(List<T>) -> OwnedListIter<T> = List::into_owned_iter;
    let _: fn(&mut OwnedListIter<T>) -> Option<T> = OwnedListIter::next;
    let _: for<'a> fn(&'a List<T>) -> ListIter<'a, T> = List::iter;
    let _: for<'a> fn(&'a mut List<T>) -> ListIterMut<'a, T> = List::iter_mut;
    bind_borrowed_advances::<T>();
}

const fn bind_borrowed_advances<'source, T: 'source>() {
    let _: fn(&mut ListIter<'source, T>) -> Option<&'source T> = ListIter::next;
    let _: for<'advance> fn(&'advance mut ListIterMut<'source, T>) -> Option<&'advance mut T> =
        ListIterMut::advance;
}
const _: () = bind_list_operations::<()>();

fn check_lookup<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    list: Option<ListRole<'g>>,
    option: Option<OptionRole<'g>>,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    let (Some(list), Some(option)) = (list, option) else {
        return Ok(None);
    };
    let Some(module) = pinned_module(inventory.graph(), "collection.rill", budget)? else {
        return Ok(None);
    };
    let Some(header) = private_bridge(inventory, module, "lookupList", budget)? else {
        return Ok(None);
    };
    let (DeclarationShape::Function { parameters, result }, [binder]) =
        (&header.shape, header.binders.as_slice())
    else {
        return Ok(None);
    };
    let [source, index] = parameters.as_slice() else {
        return Ok(None);
    };
    let (Some(source), Some(result)) = (
        shared_referent(&source.ty),
        application(result, option.declaration),
    ) else {
        return Ok(None);
    };
    if matches!(index.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "u64")
        && application(source, list.declaration).is_some_and(|t| parameter(t, binder.declaration))
        && shared_referent(result).is_some_and(|t| parameter(t, binder.declaration))
    {
        budget.reserve(1)?;
        return Ok(Some(header.declaration));
    }
    Ok(None)
}

fn check_iteration<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    list: Option<ListRole<'g>>,
    option: Option<OptionRole<'g>>,
    budget: &mut Budget,
) -> Work<Option<IterationCore<'g>>> {
    let (Some(list), Some(option)) = (list, option) else {
        return Ok(None);
    };
    let Some(module) = pinned_module(inventory.graph(), "iter.rill", budget)? else {
        return Ok(None);
    };
    let Some(iterate) = iteration_family(inventory, module, "iterate", None, budget)? else {
        return Ok(None);
    };
    let Some(next) = iteration_family(inventory, module, "next", Some(option.declaration), budget)?
    else {
        return Ok(None);
    };
    let mut bridges = [None; 6];
    for (position, (name, into, advance, access)) in [
        ("OwnedListIter", "intoOwned", "advanceOwned", None),
        ("ListIter", "intoShared", "advanceShared", Some(false)),
        (
            "ListIterMut",
            "intoExclusive",
            "advanceExclusive",
            Some(true),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(iterator) = core::export(
            inventory.graph(),
            module,
            Namespace::TypeModule,
            name,
            budget,
        )?
        else {
            continue;
        };
        if opaque_element(inventory, module, iterator, budget)?.is_none() {
            continue;
        }
        if let Some(header) = private_bridge(inventory, module, into, budget)?
            && let (DeclarationShape::Function { parameters, result }, [binder]) =
                (&header.shape, header.binders.as_slice())
            && let [input] = parameters.as_slice()
            && through_access(&input.ty, access)
                .and_then(|ty| application(ty, list.declaration))
                .is_some_and(|arg| parameter(arg, binder.declaration))
            && application(result, iterator).is_some_and(|arg| parameter(arg, binder.declaration))
        {
            budget.reserve(1)?;
            bridges[position * 2] = Some(header.declaration);
        }
        if let Some(header) = private_bridge(inventory, module, advance, budget)?
            && let (DeclarationShape::Function { parameters, result }, [binder]) =
                (&header.shape, header.binders.as_slice())
            && let [input] = parameters.as_slice()
            && referent(&input.ty, true)
                .and_then(|ty| application(ty, iterator))
                .is_some_and(|arg| parameter(arg, binder.declaration))
            && application(result, option.declaration)
                .and_then(|arg| through_access(arg, access))
                .is_some_and(|arg| parameter(arg, binder.declaration))
        {
            budget.reserve(1)?;
            bridges[position * 2 + 1] = Some(header.declaration);
        }
    }
    budget.reserve(1)?;
    Ok(Some(IterationCore {
        iterate,
        next,
        bridges,
    }))
}

fn through_access<'a, 'g>(ty: &'a Type<'g>, access: Option<bool>) -> Option<&'a Type<'g>> {
    match access {
        Some(mutable) => referent(ty, mutable),
        None => Some(ty),
    }
}

fn iteration_family<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    option: Option<&Declaration>,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    let Some(declaration) = core::export(
        inventory.graph(),
        module,
        Namespace::ValueFunction,
        name,
        budget,
    )?
    else {
        return Ok(None);
    };
    budget.step(1)?;
    let Some(family) = inventory.captured_family(&declaration.id) else {
        return Ok(None);
    };
    if !plain_header(inventory, declaration, budget)? {
        return Ok(None);
    }
    let (DeclarationShape::Function { parameters, result }, [input, output]) =
        (&family.header.shape, family.header.binders.as_slice())
    else {
        return Ok(None);
    };
    let [source] = parameters.as_slice() else {
        return Ok(None);
    };
    let (source, result) = if let Some(option) = option {
        (referent(&source.ty, true), application(result, option))
    } else {
        (Some(&source.ty), Some(result))
    };
    Ok(
        (source.is_some_and(|ty| binder_at(ty, input.declaration, 0))
            && result.is_some_and(|ty| binder_at(ty, output.declaration, 1)))
        .then_some(declaration),
    )
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
            .is_some_and(|header| header.defaults.is_empty() && header.requirements.is_empty()))
}

fn private_bridge<'a, 'g>(
    inventory: &'a OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    budget: &mut Budget,
) -> Work<Option<&'a crate::analysis::DeclarationFacts<'g>>> {
    for decl in &module.declarations {
        budget.step(1)?;
        if decl.scope.is_none()
            && !decl.public
            && decl.name.as_deref() == Some(name)
            && let Some(family) = inventory.captured_family(&decl.id)
        {
            for implementation in inventory.graph().implementations() {
                budget.step(1)?;
                if implementation.family == decl.id {
                    return Ok(None);
                }
            }
            return Ok(plain_header(inventory, decl, budget)?.then_some(&family.header));
        }
    }
    Ok(None)
}

fn shared_referent<'a, 'g>(ty: &'a Type<'g>) -> Option<&'a Type<'g>> {
    referent(ty, false)
}

fn referent<'a, 'g>(ty: &'a Type<'g>, writable: bool) -> Option<&'a Type<'g>> {
    match ty.as_ref() {
        TypeTerm::Reference { mutable, referent } if *mutable == writable => Some(referent),
        _ => None,
    }
}

fn application<'a, 'g>(ty: &'a Type<'g>, owner: &Declaration) -> Option<&'a Type<'g>> {
    match ty.as_ref() {
        TypeTerm::Nominal {
            declaration,
            arguments,
        } if **declaration == owner.id => {
            let [argument] = arguments.as_slice() else {
                return None;
            };
            Some(argument)
        }
        _ => None,
    }
}

fn parameter(ty: &Type<'_>, binder: &Declaration) -> bool {
    binder_at(ty, binder, 0)
}

fn binder_at(ty: &Type<'_>, binder: &Declaration, index: usize) -> bool {
    matches!(ty.as_ref(), TypeTerm::Parameter { declaration, position } if **declaration == binder.id && *position == index)
}

fn pinned_module<'g>(
    graph: &'g Graph,
    path: &str,
    budget: &mut Budget,
) -> Work<Option<&'g Module>> {
    for module in graph.modules() {
        budget.step(1)?;
        if module.key.path == path && graph.is_pinned_standard_source(module) {
            return Ok(Some(module));
        }
    }
    Ok(None)
}

fn check_list<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    budget: &mut Budget,
) -> Work<Option<ListRole<'g>>> {
    let graph = inventory.graph();
    let Some(module) = pinned_module(graph, "collection.rill", budget)? else {
        return Ok(None);
    };
    let Some(declaration) = core::export(graph, module, Namespace::TypeModule, "List", budget)?
    else {
        return Ok(None);
    };
    let Some(element) = opaque_element(inventory, module, declaration, budget)? else {
        return Ok(None);
    };
    budget.reserve(1)?;
    Ok(Some(ListRole {
        declaration,
        element,
    }))
}

fn opaque_element<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &Module,
    declaration: &Declaration,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    let Some(facts) = core::unconditional_declaration(inventory, declaration, budget)? else {
        return Ok(None);
    };
    let (DeclarationShape::Record { fields }, [element]) = (&facts.shape, facts.binders.as_slice())
    else {
        return Ok(None);
    };
    let [field] = fields.as_slice() else {
        return Ok(None);
    };
    let TypeTerm::Nominal {
        declaration: seal,
        arguments,
    } = field.ty.as_ref()
    else {
        return Ok(None);
    };
    budget.step(1)?;
    let closed_slot = !field.declaration.public && arguments.is_empty()
        && seal.module == module.key
        && inventory.type_facts().declarations.get(seal).is_some_and(|facts| {
            !facts.declaration.public && facts.binders.is_empty()
                && matches!(&facts.shape, DeclarationShape::Enum { variants } if variants.is_empty())
        });
    Ok(closed_slot.then_some(element.declaration))
}
