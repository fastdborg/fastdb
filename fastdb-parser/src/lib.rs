//! Independent lexer, syntax tree, and parser for the FastDB MVP grammar.
//!
//! The crate has no dependency on Turso parser or engine types. It accepts a
//! script through [`parse`] or exactly one statement through [`parse_one`].

#![forbid(unsafe_code)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use error::{LimitKind, ParseError, ParseErrorKind};
pub use lexer::{tokenize, tokenize_with_limits, Token, TokenKind};
pub use parser::{parse, parse_one, parse_one_with_limits, parse_with_limits, StatementCursor};

/// Resource ceilings applied before or during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserLimits {
    pub max_input_bytes: usize,
    pub max_tokens: usize,
    pub max_nesting_depth: usize,
    pub max_collection_elements: usize,
    pub max_statements: usize,
    pub max_identifier_bytes: usize,
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 1 << 20,
            max_tokens: 65_536,
            max_nesting_depth: 64,
            max_collection_elements: 1_024,
            max_statements: 256,
            max_identifier_bytes: 256,
        }
    }
}
