//! # turso_fastdb
//!
//! FastDB Phase 0 frontend: parses SurrealQL-subset input with the
//! independent parser crate, lowers it directly into Turso AST, executes
//! via `Connection::prepare_translated_stmt_with_options`, and decodes
//! results into FastDB value/record types.
//!
//! The request path is:
//! ```text
//! FastDB source -> parser AST -> frontend plan
//!   -> Turso AST + bound values -> prepare_translated_stmt_with_options
//!   -> Turso execution -> FastDB result decoding
//! ```
//! FastDB user input is never parsed by Turso's SQLite parser and never
//! rendered into SQLite text. Logical identifiers resolve through the
//! catalog; physical names are opaque.

// Deny warnings in-crate so that FastDB code stays lint-clean. This is
// scoped to FastDB crates only: upstream workspace-member dependencies
// (e.g. turso_core) carry their own, pre-existing lint state and are not
// fixed by Phase 0 (per plan-phase0.md: "do not fix upstream failures").
// The verified clippy command is therefore `cargo clippy -p <fastdb crate>
// --all-targets` WITHOUT a global `-D warnings`, which would otherwise
// fatalize pre-existing upstream warnings. See docs/phase0-engine-audit.md.
#![forbid(unsafe_code)]
#![deny(warnings)]

pub mod decode;
pub mod error;
pub mod names;

pub use decode::{Record, RecordId, Value};
pub use error::{ErrorCategory, FastDbError};
