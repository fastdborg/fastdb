use std::fmt;

/// Stable public error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Parse,
    UnsupportedSyntax,
    Schema,
    Constraint,
    ResourceLimit,
    Transaction,
    Engine,
    Io,
}

impl ErrorCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "Parse",
            Self::UnsupportedSyntax => "UnsupportedSyntax",
            Self::Schema => "Schema",
            Self::Constraint => "Constraint",
            Self::ResourceLimit => "ResourceLimit",
            Self::Transaction => "Transaction",
            Self::Engine => "Engine",
            Self::Io => "Io",
        }
    }
}

/// A half-open byte range in the submitted UTF-8 source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub offset: usize,
    pub len: usize,
}

/// Stable public FastDB error. Detail text is descriptive, not a matching API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    category: ErrorCategory,
    detail: String,
    span: Option<SourceSpan>,
}

impl Error {
    pub fn new(category: ErrorCategory, detail: impl Into<String>) -> Self {
        Self {
            category,
            detail: detail.into(),
            span: None,
        }
    }

    pub(crate) fn transaction_cleanup(original: Self, cleanup: Self) -> Self {
        Self::new(
            ErrorCategory::Transaction,
            format!(
                "guarded operation failed and transaction cleanup failed; original: {original}; cleanup: {cleanup}"
            ),
        )
    }

    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub fn span(&self) -> Option<SourceSpan> {
        self.span
    }

    pub(crate) fn from_frontend(error: turso_fastdb::FastDbError) -> Self {
        use turso_fastdb::FastDbError;
        let (category, span) = match &error {
            FastDbError::Parse(parse) => (
                ErrorCategory::Parse,
                Some(SourceSpan {
                    offset: parse.span.offset,
                    len: parse.span.len,
                }),
            ),
            FastDbError::UnsupportedSyntax(parse) => (
                ErrorCategory::UnsupportedSyntax,
                Some(SourceSpan {
                    offset: parse.span.offset,
                    len: parse.span.len,
                }),
            ),
            FastDbError::Schema(_) => (ErrorCategory::Schema, None),
            FastDbError::Constraint(_) => (ErrorCategory::Constraint, None),
            FastDbError::ResourceLimit(_) => (ErrorCategory::ResourceLimit, None),
            FastDbError::Format(_) => (ErrorCategory::Engine, None),
            FastDbError::Transaction(_) => (ErrorCategory::Transaction, None),
            FastDbError::Engine(_) => (ErrorCategory::Engine, None),
            FastDbError::Io(_) => (ErrorCategory::Io, None),
        };
        Self {
            category,
            detail: error.to_string(),
            span,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for Error {}

pub(crate) type Result<T> = std::result::Result<T, Error>;
