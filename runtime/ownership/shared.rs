use std::sync::Arc;

/// One counted owner of an immutable payload. No implicit clone.
pub struct Shared<T>(Arc<T>);

pub fn new<T>(value: T) -> Shared<T> {
    Shared(Arc::new(value))
}

pub fn view<T>(owner: &Shared<T>) -> &T {
    &owner.0
}

pub fn retain<T>(owner: &Shared<T>) -> Shared<T> {
    Shared(Arc::clone(&owner.0))
}

pub fn release<T>(owner: Shared<T>) {
    drop(owner);
}
