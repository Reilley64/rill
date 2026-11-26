use compiler::analysis::scalars::Site;
use compiler::source::Span;
use std::fmt;

/// Why the backend wrote no object. Every variant is a backend bug or a host
/// failure: whether a program may be compiled is decided by the compiler's
/// compilable-program rule before the backend runs, so a program that reaches
/// the backend contains only shapes the backend lowers.
#[derive(Debug, PartialEq, Eq)]
pub enum BackendError {
    /// The LLVM verifier rejected the module the backend built.
    Verifier(String),
    /// LLVM could not provide the host target or write the object.
    Target(String),
    /// The program broke an assumption the backend relies on: a shape the
    /// compilable-program rule admits and the backend does not lower, or a
    /// MIR invariant that does not hold.
    Internal(String),
}

impl BackendError {
    pub(crate) fn unsupported(site: Site<'_>, what: impl Into<String>) -> Self {
        Self::unsupported_at(&site.module.path, site.span, what)
    }

    pub(crate) fn unsupported_at(module: &str, span: &Span, what: impl Into<String>) -> Self {
        let (line, column) = span
            .coordinates()
            .map_or((0, 0), |(start, _)| (start.line + 1, start.scalar + 1));
        Self::Internal(format!(
            "the backend cannot lower {} at {module}:{line}:{column}, which the compilable-program rule admitted",
            what.into()
        ))
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verifier(message) => write!(formatter, "LLVM rejected the module: {message}"),
            Self::Target(message) => write!(formatter, "LLVM target failure: {message}"),
            Self::Internal(message) => write!(formatter, "backend failure: {message}"),
        }
    }
}

impl std::error::Error for BackendError {}

#[cfg(test)]
mod tests {
    use super::*;
    use compiler::source::Snapshot;

    #[test]
    fn a_refusal_is_an_internal_error_named_without_debug_output() {
        let source = Snapshot::capture("main.rill", b"import std::log;\nlet flag = true;\n")
            .expect("a snapshot");
        let span = source.span(21, 25).expect("a span");
        let error =
            BackendError::unsupported_at("main.rill", &span, "an operation with no lowering yet");
        assert!(matches!(error, BackendError::Internal(_)));
        assert_eq!(
            error.to_string(),
            "backend failure: the backend cannot lower an operation with no lowering yet at main.rill:2:5, which the compilable-program rule admitted"
        );
    }
}
