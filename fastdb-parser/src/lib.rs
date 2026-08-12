//! # turso_fastdb_parser
//!
//! FastDB Phase 0 parser: a hand-written lexer and recursive-descent
//! parser for a tiny SurrealQL-compatible subset. It is intentionally
//! independent of `turso_parser` and `turso_core` AST types.
//!
//! Supported Phase 0 forms (see `COMPAT.md`):
//!
//! ```text
//! CREATE <table>:<id> SET <field> = '<string>';
//! SELECT * FROM <table>:<id>;
//! SELECT * FROM <table> WHERE <field> = '<string>';
//! DELETE <table>:<id>;
//! ```
//!
//! Every other syntax — including recognized clauses like `RETURN`,
//! `ONLY`, `LIMIT`, multiple `SET` assignments, multiple statements, and
//! unsupported value types — is rejected with an explicit
//! [`ParseError`] rather than ignored.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

pub use ast::{
    Assignment, CreateStatement, DeleteStatement, Identifier, Predicate, RecordIdPart,
    RecordTarget, SelectStatement, Span, Spanned, Statement, StringLit,
};
pub use error::{ParseError, ParseErrorKind};
pub use lexer::{tokenize, Token, TokenKind};
pub use parser::parse;
