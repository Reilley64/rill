//! Capture-bound admission of the linked List operations; no native ABI.
use super::*;
use crate::analysis::ordinary::{Compatibility, FamilyFacts, Placement, patterns};
use crate::analysis::{Declaration, DeclarationFacts, DeclarationShape, Type, TypeTerm};
use crate::loader::{DeclarationId, Module, Namespace};
use rill_collection::{List, ListIter, ListIterMut, OwnedListIter};
use std::ops::{Index, IndexMut};

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::analysis) struct CollectionCore<'g> {
    pub list: Option<ListRole<'g>>,
    pub iteration: Option<IterationCore<'g>>,
    iterators: [Option<ListRole<'g>>; 3],
    pub protection: [Option<CollectionProtection<'g>>; 8],
}

#[derive(Clone, Copy, Debug)]
pub(in crate::analysis) struct IterationCore<'g> {
    pub iterate: &'g Declaration,
    pub next: &'g Declaration,
}

// Fixed scopes for the delivered List backing. No closure over other collection families.
const OPERATIONS: [(CollectionOperation, &str, &str); 8] = {
    use CollectionOperation::*;
    use ListTraversal::*;
    [
        (
            Get,
            "lookupList",
            "{\n    return lookupList(source, index);\n}",
        ),
        (
            First,
            "lookupList",
            "{\n    return lookupList(source, 0u64);\n}",
        ),
        (
            Iterate(Owned),
            "intoOwned",
            "{\n    return intoOwned(source);\n}",
        ),
        (
            Next(Owned),
            "advanceOwned",
            "{\n    return advanceOwned(iterator);\n}",
        ),
        (
            Iterate(Shared),
            "intoShared",
            "{\n    return intoShared(source);\n}",
        ),
        (
            Next(Shared),
            "advanceShared",
            "{\n    return advanceShared(iterator);\n}",
        ),
        (
            Iterate(Exclusive),
            "intoExclusive",
            "{\n    return intoExclusive(source);\n}",
        ),
        (
            Next(Exclusive),
            "advanceExclusive",
            "{\n    return advanceExclusive(iterator);\n}",
        ),
    ]
};

impl<'g> CollectionCore<'g> {
    pub(super) fn check(
        &mut self,
        inventory: &OrdinaryAnalysis<'g>,
        option: Option<OptionRole<'g>>,
        budget: &mut Budget,
        overlaps: &mut Vec<CollectionOverlap<'g>>,
    ) -> Work<()> {
        use CollectionOperation::*;
        let graph = inventory.graph();
        let collection = pinned_module(graph, "collection.rill", budget)?;
        let iter = pinned_module(graph, "iter.rill", budget)?;
        if let Some(module) = collection {
            self.list = opaque_role(inventory, module, "List", budget)?;
        }
        if let Some(module) = iter {
            for (slot, name) in
                self.iterators
                    .iter_mut()
                    .zip(["OwnedListIter", "ListIter", "ListIterMut"])
            {
                *slot = opaque_role(inventory, module, name, budget)?;
            }
        }
        let (Some(list), Some(option)) = (self.list, option) else {
            return Ok(());
        };
        for (index, (operation, entry, wrapper)) in OPERATIONS.into_iter().enumerate() {
            let (module, owner, result_owner) = match operation {
                Get | First => (collection, Some(list), Some(option.declaration)),
                Iterate(kind) => (
                    iter,
                    Some(list),
                    self.iterators[kind as usize].map(|role| role.declaration),
                ),
                Next(kind) => (
                    iter,
                    self.iterators[kind as usize],
                    Some(option.declaration),
                ),
            };
            let (Some(module), Some(owner), Some(result_owner)) = (module, owner, result_owner)
            else {
                continue;
            };
            if let Some(family) =
                check_family(inventory, module, operation, option.declaration, budget)?
            {
                let scope = OperationScope {
                    family,
                    owner: owner.declaration,
                    result_owner,
                    operation,
                    entry,
                    wrapper,
                };
                self.protection[index] =
                    Some(check_operation(inventory, module, scope, budget, overlaps)?);
            }
        }
        if let (Some(iterate), Some(next)) = (
            self.protection
                .iter()
                .flatten()
                .find(|p| matches!(p.operation, Iterate(_))),
            self.protection
                .iter()
                .flatten()
                .find(|p| matches!(p.operation, Next(_))),
        ) {
            self.iteration = Some(IterationCore {
                iterate: iterate.family,
                next: next.family,
            });
        }
        Ok(())
    }

    pub fn lookup(&self) -> Option<&'g Declaration> {
        self.protection.first()?.as_ref()?.bridge
    }

    pub fn representations(&self) -> impl Iterator<Item = CollectionRepresentation<'g>> {
        let kinds = [
            CollectionStorage::Elements,
            CollectionStorage::UnyieldedElements,
            CollectionStorage::BorrowedList { mutable: false },
            CollectionStorage::BorrowedList { mutable: true },
        ];
        [
            self.list,
            self.iterators[0],
            self.iterators[1],
            self.iterators[2],
        ]
        .into_iter()
        .zip(kinds)
        .filter_map(|(role, storage)| {
            role.map(|role| CollectionRepresentation {
                declaration: role.declaration,
                element: role.element,
                storage,
            })
        })
    }

    pub fn representation(
        &self,
        ty: &Type<'g>,
    ) -> Option<(CollectionRepresentation<'g>, Type<'g>)> {
        self.representations()
            .find_map(|repr| Some((repr, application(ty, repr.declaration)?.clone())))
    }

    pub fn scope(
        &self,
        family: &DeclarationId,
        inputs: &[Option<&Type<'g>>],
    ) -> Option<CollectionProtection<'g>> {
        self.protection.into_iter().flatten().find(|scope| {
            scope.family.id == *family
                && match inputs {
                    [Some(input), Some(index)] if scope.operation == CollectionOperation::Get => {
                        matches!(index.as_ref(), TypeTerm::Primitive(p) if p.name() == "u64")
                            && scope.matches(input)
                    }
                    [Some(input)] if scope.operation != CollectionOperation::Get => {
                        scope.matches(input)
                    }
                    _ => false,
                }
        })
    }

    pub fn is_iteration_family(&self, id: &DeclarationId) -> bool {
        self.iteration
            .is_some_and(|role| role.iterate.id == *id || role.next.id == *id)
    }

    pub fn is_wrapper(&self, id: &DeclarationId) -> bool {
        self.protection.into_iter().flatten().any(|scope| {
            scope
                .provider
                .is_some_and(|provider| provider.implementation.id == *id)
        })
    }

    pub fn is_bridge(&self, id: &DeclarationId) -> bool {
        self.protection
            .iter()
            .flatten()
            .any(|region| region.bridge.is_some_and(|bridge| bridge.id == *id))
    }
}

/// Stored shape of the linked backing, not the private source seal or a layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionStorage {
    Elements,
    UnyieldedElements,
    BorrowedList { mutable: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct CollectionRepresentation<'g> {
    pub declaration: &'g Declaration,
    /// Substitution mapping for the actual element type (binder position zero).
    pub element: &'g Declaration,
    pub storage: CollectionStorage,
}

impl CollectionStorage {
    /// Backing type in the compiler's linked collection crate.
    pub fn rust_item(self) -> &'static str {
        match self {
            Self::Elements => "rill_collection::List",
            Self::UnyieldedElements => "rill_collection::OwnedListIter",
            Self::BorrowedList { mutable: false } => "rill_collection::ListIter",
            Self::BorrowedList { mutable: true } => "rill_collection::ListIterMut",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListTraversal {
    /// `Some` forwards one element. `None` proves no unyielded elements remain.
    Owned,
    /// Borrows the source; does not transfer any of its owned elements.
    Shared,
    /// Borrows the source and ties the yield to the advance access.
    Exclusive,
}

impl ListTraversal {
    fn access(self) -> Option<bool> {
        match self {
            Self::Owned => None,
            Self::Shared => Some(false),
            Self::Exclusive => Some(true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionOperation {
    Get,
    First,
    Iterate(ListTraversal),
    Next(ListTraversal),
}

impl CollectionOperation {
    fn access(self) -> Option<bool> {
        match self {
            Self::Get | Self::First => Some(false),
            Self::Iterate(kind) => kind.access(),
            Self::Next(_) => Some(true),
        }
    }

    /// Compiler-linked Rust operation, not an ABI symbol.
    pub fn rust_item(self) -> &'static str {
        use ListTraversal::*;
        match self {
            Self::Get | Self::First => "rill_collection::List::get",
            Self::Iterate(Owned) => "rill_collection::List::into_owned_iter",
            Self::Iterate(Shared) => "rill_collection::List::iter",
            Self::Iterate(Exclusive) => "rill_collection::List::iter_mut",
            Self::Next(Owned) => "rill_collection::OwnedListIter::next",
            Self::Next(Shared) => "rill_collection::ListIter::next",
            Self::Next(Exclusive) => "rill_collection::ListIterMut::advance",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CollectionProvider<'g> {
    pub implementation: &'g Declaration,
    pub bridge: &'g Declaration,
}

/// Only the region identified by the operation and canonical input owner is
/// closed. No assertion is made about disjoint List-external input regions.
#[derive(Clone, Copy, Debug)]
pub struct CollectionProtection<'g> {
    pub family: &'g Declaration,
    pub owner: &'g Declaration,
    pub operation: CollectionOperation,
    pub provider: Option<CollectionProvider<'g>>,
    bridge: Option<&'g Declaration>,
    pub coverage: OwnershipCoverage,
}

impl<'g> CollectionProtection<'g> {
    fn matches(self, input: &Type<'g>) -> bool {
        through_access(input, self.operation.access())
            .is_some_and(|ty| application(ty, self.owner).is_some())
    }

    pub(in crate::analysis) fn winner(self) -> Option<&'g DeclarationId> {
        (self.coverage == OwnershipCoverage::Clear)
            .then_some(self.provider)
            .flatten()
            .map(|provider| &provider.implementation.id)
    }
}

#[derive(Debug)]
pub struct CollectionOverlap<'g> {
    pub family: &'g DeclarationId,
    pub operation: CollectionOperation,
    pub implementation: &'g Declaration,
    pub intersection: (usize, usize),
}

impl CollectionOverlap<'_> {
    pub fn code(&self) -> &'static str {
        "R0500"
    }
}

#[derive(Clone, Copy)]
struct OperationScope<'g> {
    family: &'g Declaration,
    owner: &'g Declaration,
    operation: CollectionOperation,
    result_owner: &'g Declaration,
    entry: &'static str,
    wrapper: &'static str,
}

fn check_operation<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    scope: OperationScope<'g>,
    budget: &mut Budget,
    overlaps: &mut Vec<CollectionOverlap<'g>>,
) -> Work<CollectionProtection<'g>> {
    let mut record = CollectionProtection {
        family: scope.family,
        owner: scope.owner,
        operation: scope.operation,
        provider: None,
        coverage: OwnershipCoverage::Pending,
        bridge: None,
    };
    budget.reserve(1)?;
    let bridge_shape = if scope.operation == CollectionOperation::First {
        OperationScope {
            operation: CollectionOperation::Get,
            ..scope
        }
    } else {
        scope
    };
    let Some(bridge) = private_bridge(inventory, module, scope.entry, budget)?
        .filter(|header| operation_schema(header, &bridge_shape))
        .map(|header| header.declaration)
    else {
        return Ok(record);
    };
    record.bridge = Some(bridge);
    let family = inventory.captured_family(&scope.family.id).unwrap();
    for body in &family.implementations {
        budget.step(1)?;
        if body.source.declaration.module != module.key
            || body.placement != Placement::FamilyModule
            || body.compatibility != Compatibility::Compatible
            || body.alternatives.is_empty()
            || module.syntax.nodes[body.source.body_node].span.bytes() != scope.wrapper.as_bytes()
            || !plain_header(inventory, body.header.declaration, budget)?
            || !operation_schema(&body.header, &scope)
        {
            continue;
        }
        record.provider = Some(CollectionProvider {
            implementation: body.header.declaration,
            bridge,
        });
        record.coverage = check_region(
            inventory,
            family,
            &body.alternatives,
            record,
            budget,
            overlaps,
        )?;
        break;
    }
    Ok(record)
}

fn check_region<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    family: &FamilyFacts<'g>,
    region: &[patterns::InputKey<'g>],
    record: CollectionProtection<'g>,
    budget: &mut Budget,
    overlaps: &mut Vec<CollectionOverlap<'g>>,
) -> Work<OwnershipCoverage> {
    let admitted = record.provider.unwrap().implementation;
    let mut captured = 0;
    for body in inventory.graph().implementations() {
        budget.step(1)?;
        captured += usize::from(body.family == record.family.id);
    }
    let mut complete = captured == family.implementations.len();
    let before = overlaps.len();
    for body in &family.implementations {
        budget.step(1)?;
        if body.header.declaration.id == admitted.id {
            continue;
        }
        if body.alternatives.is_empty() {
            complete = false;
            continue;
        }
        if let Some(intersection) = patterns::intersection(region, &body.alternatives, budget)?
            && !(patterns::included(region, &body.alternatives, budget)?
                && !patterns::included(&body.alternatives, region, budget)?)
        {
            budget.reserve(1)?;
            overlaps.push(CollectionOverlap {
                family: &record.family.id,
                operation: record.operation,
                implementation: body.header.declaration,
                intersection,
            });
        }
    }
    Ok(if !complete {
        OwnershipCoverage::Pending
    } else if overlaps.len() != before {
        OwnershipCoverage::Overlap
    } else {
        OwnershipCoverage::Clear
    })
}

fn operation_schema(header: &DeclarationFacts<'_>, scope: &OperationScope<'_>) -> bool {
    use CollectionOperation::*;
    let (DeclarationShape::Function { parameters, result }, [binder]) =
        (&header.shape, header.binders.as_slice())
    else {
        return false;
    };
    let source = match parameters.as_slice() {
        [source, key]
            if scope.operation == Get
                && matches!(key.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "u64") =>
        {
            source
        }
        [source] if scope.operation != Get => source,
        _ => return false,
    };
    let result_access = match scope.operation {
        Get | First => Some(false),
        Iterate(_) => None,
        Next(kind) => kind.access(),
    };
    through_access(&source.ty, scope.operation.access())
        .and_then(|ty| application(ty, scope.owner))
        .is_some_and(|ty| parameter(ty, binder.declaration))
        && application(result, scope.result_owner)
            .and_then(|ty| through_access(ty, result_access))
            .is_some_and(|ty| parameter(ty, binder.declaration))
}

fn check_family<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    op: CollectionOperation,
    option: &Declaration,
    budget: &mut Budget,
) -> Work<Option<&'g Declaration>> {
    use CollectionOperation::*;
    let name = match op {
        Get => "get",
        First => "first",
        Iterate(_) => "iterate",
        Next(_) => "next",
    };
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
    let Some(family) = inventory.captured_family(&declaration.id) else {
        return Ok(None);
    };
    if !plain_header(inventory, declaration, budget)? {
        return Ok(None);
    }
    let DeclarationShape::Function { parameters, result } = &family.header.shape else {
        return Ok(None);
    };
    let binders = &family.header.binders;
    let (input, output) = match (op, parameters.as_slice(), binders.as_slice()) {
        (Get, [source, key], [_, q, r]) if binder_at(&key.ty, q.declaration, 1) => {
            (Some(&source.ty), r)
        }
        (Next(_), [source], [_, t]) => (referent(&source.ty, true), t),
        (First | Iterate(_), [source], [_, r]) => (Some(&source.ty), r),
        _ => return Ok(None),
    };
    let result = if matches!(op, Iterate(_)) {
        Some(result)
    } else {
        application(result, option)
    };
    Ok(
        (input.is_some_and(|ty| parameter(ty, binders[0].declaration))
            && result.is_some_and(|ty| binder_at(ty, output.declaration, output.position)))
        .then_some(declaration),
    )
}

fn opaque_role<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    module: &'g Module,
    name: &str,
    budget: &mut Budget,
) -> Work<Option<ListRole<'g>>> {
    let Some(declaration) = core::export(
        inventory.graph(),
        module,
        Namespace::TypeModule,
        name,
        budget,
    )?
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

fn through_access<'a, 'g>(ty: &'a Type<'g>, access: Option<bool>) -> Option<&'a Type<'g>> {
    match access {
        Some(mutable) => referent(ty, mutable),
        None => Some(ty),
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
    type CopyElement<T> = fn(&T) -> T;
    let _: fn(&List<T>, CopyElement<T>) -> List<T> = List::copy_with;
    let _: fn(List<T>, fn(T)) = List::dispose_with;
    let _: fn(OwnedListIter<T>, fn(T)) = OwnedListIter::dispose_with;
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

pub(super) fn private_bridge<'a, 'g>(
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
