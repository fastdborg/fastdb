//! Typed parser diagnostics with stable codes and byte spans.

use crate::ast::Span;
use miette::{Diagnostic, LabeledSpan};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    InputBytes,
    Tokens,
    NestingDepth,
    CollectionElements,
    Statements,
    IdentifierBytes,
}

impl fmt::Display for LimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InputBytes => "input bytes",
            Self::Tokens => "tokens",
            Self::NestingDepth => "nesting depth",
            Self::CollectionElements => "collection elements",
            Self::Statements => "statements",
            Self::IdentifierBytes => "identifier or parameter bytes",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseErrorKind {
    #[error("empty input; expected a statement")]
    EmptyInput,
    #[error("unexpected character {ch:?}")]
    UnexpectedCharacter { ch: char },
    #[error("expected {expected}, found {found}")]
    UnexpectedToken {
        expected: &'static str,
        found: String,
    },
    #[error("unexpected end of input; expected {expected}")]
    UnexpectedEof { expected: &'static str },
    #[error("unterminated {delimiter}-quoted string literal")]
    UnterminatedString { delimiter: char },
    #[error("unterminated backtick-quoted identifier")]
    UnterminatedQuotedIdentifier,
    #[error("unterminated block comment")]
    UnterminatedComment,
    #[error("invalid string escape {escape:?}")]
    InvalidEscape { escape: String },
    #[error("invalid number {literal:?}: {reason}")]
    InvalidNumber {
        literal: String,
        reason: &'static str,
    },
    #[error("missing semicolon between statements")]
    MissingStatementSeparator,
    #[error("empty statements are not allowed")]
    EmptyStatement,
    #[error("expected one statement, found {count}")]
    MultipleStatements { count: usize },
    #[error("duplicate clause {clause}")]
    DuplicateClause { clause: &'static str },
    #[error("clause {clause} is out of order")]
    ClauseOrder { clause: &'static str },
    #[error("invalid syntax combination: {what}")]
    InvalidCombination { what: &'static str },
    #[error("unsupported syntax: {what}")]
    UnsupportedSyntax { what: &'static str },
    #[error("{kind} limit exceeded (maximum {limit})")]
    LimitExceeded { kind: LimitKind, limit: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind}")]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

impl ParseError {
    pub const fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub const fn unsupported(what: &'static str, span: Span) -> Self {
        Self::new(ParseErrorKind::UnsupportedSyntax { what }, span)
    }

    fn label(&self) -> &'static str {
        match self.kind {
            ParseErrorKind::EmptyInput => "expected a statement",
            ParseErrorKind::UnexpectedCharacter { .. } => "unexpected character",
            ParseErrorKind::UnexpectedToken { .. } => "unexpected token",
            ParseErrorKind::UnexpectedEof { .. } => "input ended here",
            ParseErrorKind::UnterminatedString { .. } => "string starts here",
            ParseErrorKind::UnterminatedQuotedIdentifier => "quoted identifier starts here",
            ParseErrorKind::UnterminatedComment => "comment starts here",
            ParseErrorKind::InvalidEscape { .. } => "invalid escape",
            ParseErrorKind::InvalidNumber { .. } => "invalid number",
            ParseErrorKind::MissingStatementSeparator => "separator required here",
            ParseErrorKind::EmptyStatement => "empty statement",
            ParseErrorKind::MultipleStatements { .. } => "additional statement",
            ParseErrorKind::DuplicateClause { .. } => "duplicate clause",
            ParseErrorKind::ClauseOrder { .. } => "out-of-order clause",
            ParseErrorKind::InvalidCombination { .. } => "invalid combination",
            ParseErrorKind::UnsupportedSyntax { .. } => "unsupported syntax",
            ParseErrorKind::LimitExceeded { .. } => "limit exceeded here",
        }
    }

    fn diagnostic_code(&self) -> &'static str {
        match self.kind {
            ParseErrorKind::EmptyInput => "fastdb::parse::empty_input",
            ParseErrorKind::UnexpectedCharacter { .. } => "fastdb::parse::unexpected_character",
            ParseErrorKind::UnexpectedToken { .. } => "fastdb::parse::unexpected_token",
            ParseErrorKind::UnexpectedEof { .. } => "fastdb::parse::unexpected_eof",
            ParseErrorKind::UnterminatedString { .. } => "fastdb::parse::unterminated_string",
            ParseErrorKind::UnterminatedQuotedIdentifier => {
                "fastdb::parse::unterminated_quoted_identifier"
            }
            ParseErrorKind::UnterminatedComment => "fastdb::parse::unterminated_comment",
            ParseErrorKind::InvalidEscape { .. } => "fastdb::parse::invalid_escape",
            ParseErrorKind::InvalidNumber { .. } => "fastdb::parse::invalid_number",
            ParseErrorKind::MissingStatementSeparator => {
                "fastdb::parse::missing_statement_separator"
            }
            ParseErrorKind::EmptyStatement => "fastdb::parse::empty_statement",
            ParseErrorKind::MultipleStatements { .. } => "fastdb::parse::multiple_statements",
            ParseErrorKind::DuplicateClause { .. } => "fastdb::parse::duplicate_clause",
            ParseErrorKind::ClauseOrder { .. } => "fastdb::parse::clause_order",
            ParseErrorKind::InvalidCombination { .. } => "fastdb::parse::invalid_combination",
            ParseErrorKind::UnsupportedSyntax { .. } => "fastdb::parse::unsupported_syntax",
            ParseErrorKind::LimitExceeded { kind, .. } => match kind {
                LimitKind::InputBytes => "fastdb::parse::limit_input",
                LimitKind::Tokens => "fastdb::parse::limit_tokens",
                LimitKind::NestingDepth => "fastdb::parse::limit_nesting",
                LimitKind::CollectionElements => "fastdb::parse::limit_collection",
                LimitKind::Statements => "fastdb::parse::limit_statements",
                LimitKind::IdentifierBytes => "fastdb::parse::limit_identifier",
            },
        }
    }
}

impl Diagnostic for ParseError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(self.diagnostic_code()))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let label =
            LabeledSpan::new_with_span(Some(self.label().to_string()), self.span.to_source_span());
        Some(Box::new(std::iter::once(label)))
    }
}
