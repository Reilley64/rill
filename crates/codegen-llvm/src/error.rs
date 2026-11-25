use compiler::analysis::scalars::Site;
use compiler::source::Span;
use std::fmt;

/// Why the backend wrote no object.
#[derive(Debug, PartialEq, Eq)]
pub enum BackendError {
    /// A reached operation, type or shape the backend cannot lower yet.
    Unsupported {
        what: String,
        module: String,
        line: usize,
        column: usize,
    },
    /// The LLVM verifier rejected the module the backend built.
    Verifier(String),
    /// LLVM could not provide the host target or write the object.
    Target(String),
    /// The program broke an assumption the backend relies on.
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
        Self::Unsupported {
            what: what.into(),
            module: module.to_owned(),
            line,
            column,
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported {
                what,
                module,
                line,
                column,
            } => write!(
                formatter,
                "the backend cannot lower {what} at {module}:{line}:{column}"
            ),
            Self::Verifier(message) => write!(formatter, "LLVM rejected the module: {message}"),
            Self::Target(message) => write!(formatter, "LLVM target failure: {message}"),
            Self::Internal(message) => write!(formatter, "backend failure: {message}"),
        }
    }
}

impl std::error::Error for BackendError {}
