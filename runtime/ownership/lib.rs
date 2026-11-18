//! Native ownership primitives. Rust interfaces only; no Rill ABI.
#![forbid(unsafe_code)]

pub mod channel;
pub mod mutex;
pub mod shared;

include!("bindings.rs");

macro_rules! check_items {
    ($($action:ident => $module:ident::$operation:ident, $entry:literal, $body:literal;)*) => {
        const _: () = { $(let _ = $module::$operation::<()>;)* };
    };
}
ownership_bindings!(check_items);

// Keep the pinned representation's type argument attached to real storage,
// retained sources and forwarding operations, not to the Rill seal fields.
const fn check_storage<T>() {
    let _: fn(T) -> shared::Shared<T> = shared::new;
    let _: fn(&shared::Shared<T>) -> &T = shared::view;
    let _: fn(T) -> mutex::Mutex<T> = mutex::new;
    let _: fn(&mutex::Mutex<T>) -> mutex::LockGuard<'_, T> = mutex::acquire;
    let _: fn(usize) -> (channel::Sender<T>, channel::Receiver<T>) = channel::new;
    let _: fn(&channel::Sender<T>, T) -> Result<(), T> = channel::send;
    let _: fn(&channel::Receiver<T>) -> Option<T> = channel::receive;
}
const _: () = check_storage::<()>();
