//! Typed anchors for the Rust implementation in the containing executable.
use super::{List, ListIter, ListIterMut, OwnedListIter};
use std::ops::{Index, IndexMut};

/// Real function pointers retain the operations at link time without executing them.
pub struct ProviderLink<'source, T: 'source> {
    _construct: fn(Vec<T>) -> List<T>,
    _index: fn(&List<T>, u64) -> &T,
    _index_mut: fn(&mut List<T>, u64) -> &mut T,
    _get: fn(&List<T>, u64) -> Option<&T>,
    _copy: fn(&List<T>, CopyElement<T>) -> List<T>,
    _dispose: fn(List<T>, fn(T)),
    _iterate_owned: fn(List<T>) -> OwnedListIter<T>,
    _next_owned: fn(&mut OwnedListIter<T>) -> Option<T>,
    _dispose_owned: fn(OwnedListIter<T>, fn(T)),
    _iterate_shared: for<'a> fn(&'a List<T>) -> ListIter<'a, T>,
    _next_shared: fn(&mut ListIter<'source, T>) -> Option<&'source T>,
    _dispose_shared: fn(ListIter<'source, T>),
    _iterate_exclusive: for<'a> fn(&'a mut List<T>) -> ListIterMut<'a, T>,
    _next_exclusive:
        for<'advance> fn(&'advance mut ListIterMut<'source, T>) -> Option<&'advance mut T>,
    _dispose_exclusive: fn(ListIterMut<'source, T>),
}

type CopyElement<T> = fn(&T) -> T;

// Check the bindings generically, not just for the type used to keep them linked.
impl<'source, T: 'source> ProviderLink<'source, T> {
    const LINK: Self = Self {
        _construct: List::from_elements,
        _index: List::index,
        _index_mut: List::index_mut,
        _get: List::get,
        _copy: List::copy_with,
        _dispose: List::dispose_with,
        _iterate_owned: List::into_owned_iter,
        _next_owned: OwnedListIter::next,
        _dispose_owned: OwnedListIter::dispose_with,
        _iterate_shared: List::iter,
        _next_shared: ListIter::next,
        _dispose_shared: std::mem::drop,
        _iterate_exclusive: List::iter_mut,
        _next_exclusive: ListIterMut::advance,
        _dispose_exclusive: std::mem::drop,
    };
}

pub fn providers() -> &'static ProviderLink<'static, u8> {
    static LINK: ProviderLink<'static, u8> = ProviderLink::LINK;
    std::hint::black_box(&LINK)
}

/// Constructors of the actual private backing fields check the storage mapping
/// independently of the operation signatures and without relying on Rust layout.
pub struct RepresentationLink<'source, T: 'source> {
    _elements: fn(Vec<T>) -> List<T>,
    _unyielded: fn(std::vec::IntoIter<T>) -> OwnedListIter<T>,
    _shared: fn(std::slice::Iter<'source, T>) -> ListIter<'source, T>,
    _exclusive: fn(std::slice::IterMut<'source, T>) -> ListIterMut<'source, T>,
}

impl<'source, T: 'source> RepresentationLink<'source, T> {
    const LINK: Self = Self {
        _elements: List,
        _unyielded: OwnedListIter,
        _shared: ListIter,
        _exclusive: ListIterMut,
    };
}

pub fn representations() -> &'static RepresentationLink<'static, u8> {
    static LINK: RepresentationLink<'static, u8> = RepresentationLink::LINK;
    std::hint::black_box(&LINK)
}
