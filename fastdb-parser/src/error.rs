//! Phase 0 parse errors.
//!
//! The parser only ever produces `Parse`-category errors. `UnsupportedSyntax`
//! is used for recognized-but-unimplemented clauses so they are never
//! silently ignored. The broader error category enum lives in the frontend
//! crate.

use crate::ast::Span;
use miette::{Diagnostic, LabeledSpan};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseErrorKind {
    /// The input contained no statement.
    #[error("empty input; expected a statement")]
    EmptyInput,
    /// A single-quoted string was not closed.
    #[error("unterminated string literal")]
    UnterminatedString,
    /// The next token did not match what the grammar required.
    #[error("expected {expected}, found {found}")]
    UnexpectedToken {
        expected: &'static str,
        found: String,
    },
    /// A character that cannot start any Phase 0 token.
    #[error("unexpected character {ch:?}")]
    UnexpectedChar { ch: char },
    /// A clause or form that Phase 0 recognizes but does not implement.
    #[error("unsupported syntax in Phase 0: {what}")]
    UnsupportedSyntax { what: &'static str },
    /// More than one statement in the input.
    #[error("multiple statements are not supported in Phase 0")]
    MultipleStatements,
    /// Tokens remain after a complete statement and its optional semicolon.
    #[error("unexpected trailing tokens after statement")]
    TrailingTokens,
    /// An input-size or token-count guard tripped.
    #[error("input limit exceeded: {what}")]
    LimitExceeded { what: &'static str },
}

/// A parse failure carrying the [`ParseErrorKind`] and a source [`Span`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind}")]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    fn label(&self) -> &'static str {
        match self.kind {
            ParseErrorKind::EmptyInput => "empty input",
            ParseErrorKind::UnterminatedString => "unterminated string",
            ParseErrorKind::UnexpectedToken { .. } => "unexpected token",
            ParseErrorKind::UnexpectedChar { .. } => "unexpected character",
            ParseErrorKind::UnsupportedSyntax { .. } => "unsupported in Phase 0",
            ParseErrorKind::MultipleStatements => "second statement",
            ParseErrorKind::TrailingTokens => "trailing tokens",
            ParseErrorKind::LimitExceeded { .. } => "limit exceeded",
        }
    }
}

impl Diagnostic for ParseError {
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan>>> {
        let label =
            LabeledSpan::new_with_span(Some(self.label().to_string()), self.span.to_source_span());
        Some(Box::new(std::iter::once(label)))
    }
}
