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
