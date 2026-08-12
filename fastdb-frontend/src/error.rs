//! FastDB frontend error model.
//!
//! Phase 0 needs enough structure to distinguish parse/unsupported,
//! duplicate-id constraint, unknown future format, injected rollback
//! failure, and underlying engine failure. The full stable public error
//! API is Phase 4.

use miette::Diagnostic;
use thiserror::Error;
use turso_fastdb_parser::ParseError;

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

/// A FastDB frontend error. Carries a category; parse errors additionally
/// carry a source span via the inner [`ParseError`].
#[derive(Debug, Clone, Error)]
pub enum FastDbError {
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("unsupported syntax: {0}")]
    UnsupportedSyntax(String),
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

    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::UnsupportedSyntax(msg.into())
    }

    pub fn constraint(msg: impl Into<String>) -> Self {
        Self::Constraint(msg.into())
    }

    pub fn format(msg: impl Into<String>) -> Self {
        Self::Format(msg.into())
    }
}

impl Diagnostic for FastDbError {
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        match self {
            Self::Parse(pe) => Diagnostic::labels(pe),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FastDbError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
