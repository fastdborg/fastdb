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
    Schema,
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
    #[error("schema error: {0}")]
    Schema(String),
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
            Self::Schema(_) => ErrorCategory::Schema,
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

/// Map typed `LimboError` variants to FastDB categories. Only completion
/// failures that describe storage I/O are classified as `Io`; codec,
/// checksum, cancellation, and other completion failures remain `Engine`.
/// This does not string-match rendered error text.
impl From<turso_core::LimboError> for FastDbError {
    fn from(e: turso_core::LimboError) -> Self {
        use turso_core::{CompletionError, LimboError};
        match &e {
            LimboError::Constraint(msg) | LimboError::ForeignKeyConstraint(msg) => {
                Self::Constraint(msg.clone())
            }
            LimboError::CompletionError(
                CompletionError::IOError(..)
                | CompletionError::ShortWrite
                | CompletionError::ShortRead { .. }
                | CompletionError::ShortReadWalFrame { .. },
            ) => Self::Io(e.to_string()),
            #[cfg(target_family = "unix")]
            LimboError::CompletionError(CompletionError::RustixIOError(..)) => {
                Self::Io(e.to_string())
            }
            LimboError::Busy
            | LimboError::BusySnapshot
            | LimboError::TableLocked
            | LimboError::StatementsInProgress(_)
            | LimboError::LockingError(_)
            | LimboError::SchemaConflict => {
                Self::Transaction("database is busy or locked by another transaction".into())
            }
            _ => Self::Engine(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorCategory, FastDbError};
    use std::io::ErrorKind;
    use turso_core::{CompletionError, LimboError};

    #[test]
    fn completion_errors_distinguish_storage_io_from_corruption() {
        let io = FastDbError::from(LimboError::CompletionError(CompletionError::IOError(
            ErrorKind::Other,
            "test",
        )));
        assert_eq!(io.category(), ErrorCategory::Io);

        let checksum = FastDbError::from(LimboError::CompletionError(
            CompletionError::ChecksumMismatch {
                page_id: 1,
                expected: 2,
                actual: 3,
            },
        ));
        assert_eq!(checksum.category(), ErrorCategory::Engine);
    }
}
