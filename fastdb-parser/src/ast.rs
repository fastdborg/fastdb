//! Phase 0 FastDB AST.
//!
//! These types are deliberately independent of `turso_parser` and
//! `turso_core` AST types. The frontend crate lowers them into Turso AST;
//! the parser crate must not depend on the engine.

use miette::SourceSpan;

/// A byte span into the source: `[offset, offset + len)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

impl Span {
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }

    /// End offset (exclusive) of the span.
    pub const fn end(self) -> usize {
        self.offset + self.len
    }

    pub fn to_source_span(self) -> SourceSpan {
        SourceSpan::new(self.offset.into(), self.len)
    }

    /// Smallest span covering both `self` and `other`.
    pub fn union(self, other: Span) -> Span {
        let start = self.offset.min(other.offset);
        let end = self.end().max(other.end());
        Span::new(start, end.saturating_sub(start))
    }
}

/// A value paired with the source span it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

/// A bare identifier. The original Unicode text is preserved verbatim;
/// only keywords are matched case-insensitively.
pub type Identifier = Spanned<String>;

/// One Phase 0 statement. Exactly the four supported forms lower to one
/// of these variants; any other syntax is rejected by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Create(CreateStatement),
    Select(SelectStatement),
    Delete(DeleteStatement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateStatement {
    /// Span of the whole statement.
    pub span: Span,
    pub target: RecordTarget,
    /// Phase 0 supports exactly one string-valued `SET` assignment.
    pub assignment: Assignment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub span: Span,
    pub field: Identifier,
    pub value: StringLit,
}

/// A decoded single-quoted string literal and its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLit {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectStatement {
    pub span: Span,
    pub target: RecordTarget,
    /// Present for the equality-filter form; absent for the record form.
    pub filter: Option<Predicate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteStatement {
    pub span: Span,
    pub target: RecordTarget,
}

/// `table` optionally followed by `:id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTarget {
    pub span: Span,
    pub table: Identifier,
    /// `None` for the filter form of `SELECT`. `CREATE` and `DELETE`
    /// require an id in Phase 0 (no generated ids).
    pub id: Option<RecordIdPart>,
}

/// Phase 0 supports only a single bare-string (or quoted-string) id part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordIdPart {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    /// `field = 'value'` where the value is a string literal.
    StringEquals {
        span: Span,
        field: Identifier,
        value: StringLit,
    },
}
