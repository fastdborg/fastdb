//! FastDB frontend error model.
//!
//! Phase 0 needs enough structure to distinguish parse/unsupported,
//! duplicate-id constraint, unknown future format, injected rollback
//! failure, and underlying engine failure. The full stable public error
//! API is Phase 4.

use miette::Diagnostic;
use thiserror::Error;
use turso_fastdb_parser::{ParseError, ParseErrorKind};

/// Convenience alias for `Result<T, FastDbError>`.
pub type Result<T> = std::result::Result<T, FastDbError>;

/// Stable Phase 0 error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Parse,
    UnsupportedSyntax,
    Constraint,
    Format,
    Transaction,
    Engine,
    Io,
}

/// A FastDB frontend error. Parse and unsupported-syntax errors carry the
/// original [`ParseError`] so the source span/diagnostic is preserved.
#[derive(Debug, Clone, Error)]
pub enum FastDbError {
    #[error("parse error: {0}")]
    Parse(ParseError),
    #[error("unsupported syntax: {0}")]
    UnsupportedSyntax(ParseError),
    #[error("constraint violation: {0}")]
    Constraint(String),
    #[error("format error: {0}")]
    Format(String),
    #[error("transaction error: {0}")]
    Transaction(String),
    #[error("engine error: {0}")]
    Engine(String),
    #[error("io error: {0}")]
    Io(String),
}

impl FastDbError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Parse(_) => ErrorCategory::Parse,
            Self::UnsupportedSyntax(_) => ErrorCategory::UnsupportedSyntax,
            Self::Constraint(_) => ErrorCategory::Constraint,
            Self::Format(_) => ErrorCategory::Format,
            Self::Transaction(_) => ErrorCategory::Transaction,
            Self::Engine(_) => ErrorCategory::Engine,
            Self::Io(_) => ErrorCategory::Io,
        }
    }

    pub fn format(msg: impl Into<String>) -> Self {
        Self::Format(msg.into())
    }
}

/// Route parser errors by kind: an `UnsupportedSyntax` parse error becomes a
/// FastDB `UnsupportedSyntax` (distinct category), preserving its span; all
/// other parse errors become `Parse`.
impl From<ParseError> for FastDbError {
    fn from(e: ParseError) -> Self {
        match e.kind {
            ParseErrorKind::UnsupportedSyntax { .. } => Self::UnsupportedSyntax(e),
            _ => Self::Parse(e),
        }
    }
}

impl Diagnostic for FastDbError {
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        match self {
            Self::Parse(pe) | Self::UnsupportedSyntax(pe) => Diagnostic::labels(pe),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FastDbError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Map typed `LimboError` variants to FastDB categories: constraint and
/// foreign-key violations to `Constraint`; I/O (`CompletionError`, which is
/// how Turso wraps all `std::io::Error`) to `Io`; everything else to
/// `Engine`. This does not string-match rendered error text.
impl From<turso_core::LimboError> for FastDbError {
    fn from(e: turso_core::LimboError) -> Self {
        use turso_core::LimboError;
        match &e {
            LimboError::Constraint(msg) | LimboError::ForeignKeyConstraint(msg) => {
                Self::Constraint(msg.clone())
            }
            LimboError::CompletionError(_) => Self::Io(e.to_string()),
            _ => Self::Engine(e.to_string()),
        }
    }
}
