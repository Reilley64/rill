//! Rust backing for first-party List operations; no Rill ABI.
use std::ops::{Index, IndexMut};

#[doc(hidden)]
pub mod link;

pub struct List<T>(Vec<T>);

impl<T> List<T> {
    pub fn from_elements(elements: impl IntoIterator<Item = T>) -> Self {
        Self(elements.into_iter().collect())
    }

    pub fn len(&self) -> u64 {
        u64::try_from(self.0.len()).expect("List length must fit u64")
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, index: u64) -> Option<&T> {
        self.0.get(usize::try_from(index).ok()?)
    }

    pub fn get_mut(&mut self, index: u64) -> Option<&mut T> {
        self.0.get_mut(usize::try_from(index).ok()?)
    }

    /// Copy using the element operation supplied by the caller, not a Rust Clone bound.
    pub fn copy_with(&self, copy: impl FnMut(&T) -> T) -> Self {
        Self::from_elements(self.0.iter().map(copy))
    }

    pub fn dispose_with(self, dispose: impl FnMut(T)) {
        self.into_owned_iter().dispose_with(dispose);
    }

    pub fn into_owned_iter(self) -> OwnedListIter<T> {
        OwnedListIter(self.0.into_iter())
    }

    pub fn iter(&self) -> ListIter<'_, T> {
        ListIter(self.0.iter())
    }

    pub fn iter_mut(&mut self) -> ListIterMut<'_, T> {
        ListIterMut(self.0.iter_mut())
    }
}

impl<T> Index<u64> for List<T> {
    type Output = T;

    fn index(&self, index: u64) -> &T {
        self.get(index).expect("List index out of bounds")
    }
}

impl<T> IndexMut<u64> for List<T> {
    fn index_mut(&mut self, index: u64) -> &mut T {
        self.get_mut(index).expect("List index out of bounds")
    }
}

pub struct OwnedListIter<T>(std::vec::IntoIter<T>);

impl<T> OwnedListIter<T> {
    /// Dispose only the still-initialized slots, in reverse position order.
    pub fn dispose_with(self, dispose: impl FnMut(T)) {
        self.0.rev().for_each(dispose);
    }
}

impl<T> Iterator for OwnedListIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.0.next()
    }
}

pub struct ListIter<'a, T>(std::slice::Iter<'a, T>);

impl<'a, T> Iterator for ListIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.0.next()
    }
}

pub struct ListIterMut<'a, T>(std::slice::IterMut<'a, T>);

impl<T> ListIterMut<'_, T> {
    /// Keep exclusive yields tied to the advance borrow, not the source lifetime.
    pub fn advance(&mut self) -> Option<&mut T> {
        self.0.next()
    }
}
