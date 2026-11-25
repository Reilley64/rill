//! Owns LLVM code generation from the shared compiler's results.
//!
//! [`emit_object`] lowers a compilable program to a native object for the
//! host. It builds only with the static LLVM 23.1 prefix that `mise run
//! setup:llvm` installs, named by `LLVM_SYS_231_PREFIX`.

mod error;
mod lower;
mod mono;
mod syntax;

pub use error::BackendError;

use compiler::analysis::compilable::CompilableProgram;
use compiler::loader::Graph;
use std::path::Path;

/// Write the object file of `program` to `object`, for the host's target.
///
/// `graph` is the loaded graph the program's analysis was built from. The
/// object defines exactly three external symbols, `rill_startup` (the entry
/// module's startup body), `rill_startup_cleanup` (cleanup of the startup
/// scope's owned values, which the startup body has already performed, so it
/// is empty) and the one-byte constant `rill_check_allocations`, which is 1
/// when `check_allocations` is set and 0 otherwise (the runtime reads it to
/// decide whether to report live allocations at exit), and needs the runtime's `rill_rt_allocate`, `rill_rt_release`,
/// `rill_rt_write_line` and `rill_rt_panic`. Everything else is internal.
/// The runtime owns `main`.
///
/// No optimisation pass runs, and the LLVM verifier checks the module before
/// anything is written: nothing is written on an error.
///
/// The backend refuses nothing the compilable-program rule admits, but it
/// lowers only the operations it supports yet. For any other operation it
/// returns [`BackendError::Unsupported`], which the driver reports as an
/// internal tool failure (status 3), never as R0605.
pub fn emit_object(
    graph: &Graph,
    program: &CompilableProgram<'_, '_>,
    object: &Path,
    check_allocations: bool,
) -> Result<(), BackendError> {
    lower::emit_object(graph, program, object, check_allocations)
}
