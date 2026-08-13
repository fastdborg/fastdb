# Phase 6 pinned-engine capability audit

Status: read-only audit, 2026-08-13

## Scope and decision

FastDB retains Turso commit
`977383ff40edc44ef410af062ed0d2322252a869` for Phase 6. The mandatory
fetch-only comparison inspected upstream `main` at
`a94102c20b4c1c554f7c246606c2ed74db47199c` (2026-08-12). No upstream commit
was merged or cherry-picked and no pin record changed.

The pinned engine is sufficient for the format-2 catalog migration, ordinary
B-tree rebuild/removal, scalar function lowering, integrity checks, and stable
WAL checkpoints without a Turso core change. Later FTS use requires a
FastDB-owned option/capability adapter and remains unavailable in Phase 6.
Exact vector functions are usable unchanged later, with FastDB enforcing its
own public limits. The toy sparse-IVF provider and experimental MVCC are unfit
for Core 1.0.

## Baseline and comparison

| Item | Evidence |
| --- | --- |
| FastDB branch | `phase-6` at `998354ed8` before Phase 6 working changes |
| Retained engine pin | `977383ff40edc44ef410af062ed0d2322252a869` |
| Pin ancestry | `git merge-base --is-ancestor <pin> HEAD` passed |
| Upstream remote | `https://github.com/tursodatabase/turso.git` |
| Compared upstream SHA | `a94102c20b4c1c554f7c246606c2ed74db47199c` |
| Working-tree preservation | Existing Phase 6 roadmap edits remained in place during fetch/audit |

The compared range changes 109 files under the focused core/parser/PostgreSQL
surface. Relevant upstream improvements include translated-command preparation
and stricter FTS `WITH` validation/ngram configuration. Those improvements do
not justify a Phase 6 engine update because the retained pin supports the
foundation work and a pin change would expand the regression surface without a
Phase 6 requirement.

## Capability classification

### Direct translated AST and functions — usable unchanged

- `core/connection.rs` exposes
  `prepare_translated_stmt_with_options(ast::Stmt, input, PrepareOptions)`.
  It bypasses dialect parsing while retaining source text for diagnostics and
  schema re-preparation.
- `postgres/frontend/session.rs` is the inherited reference integration: it
  translates its independent AST, executes prerequisites, then prepares the
  translated Turso statement with options.
- `sqlite/parser/src/ast.rs` represents scalar calls as `Expr::FunctionCall`.
  The pinned SQLite dialect resolves built-in vector and, when compiled,
  feature-gated FTS functions.
- FastDB already calls this seam only with constructed Turso AST and bound
  values. No Phase 6 request path needs Turso's SQLite parser.

The pin accepts only `ast::Stmt`, not an `ast::Cmd`, so it cannot directly
prepare `Cmd::ExplainQueryPlan`. Upstream adds
`prepare_translated_cmd_with_options`, but Phase 6 can retain the pin by using
the existing FastDB diagnostic pattern: render an internally constructed
explain command, reparse it, prove structural equality with the lowered AST,
then execute it. This is an internal, value-free diagnostic command, not
generated SQL from user input.

### B-tree index lifecycle — usable unchanged

- Ordinary `CREATE INDEX`, `DROP INDEX`, and `REINDEX` are implemented in the
  pinned AST/translator and run under normal engine transactions.
- FastDB physical names are opaque catalog-derived identifiers. Phase 6 can
  construct drop/reindex AST only after catalog resolution, without inserting
  a logical name into SQL.
- Existing expression-index tests prove the canonical JSON expression is
  selectable. Phase 6 must rerun these tests after catalog migration and add
  failure/reopen evidence for remove/rebuild.

### Custom index methods — usable only behind a private adapter

- `core/index_method/mod.rs` exposes factory, attachment, cursor, cost, DML,
  pre-commit, and optimize traits. The cursor API uses resumable `IOResult` and
  participates in engine transaction commit.
- Built-in method registration in `core/ext/mod.rs` includes
  `backing_btree`, `toy_vector_sparse_ivf`, and feature-gated `fts`.
- SQL use is guarded by `DatabaseOpts::enable_index_method`, whose default is
  false. FastDB currently opens with `DatabaseOpts::default()`.
- The engine interface is explicitly experimental and does not provide the
  stable capability/version/refusal contract required by FastDB format 2.

Phase 6 therefore defines a sealed FastDB catalog/provider boundary but
registers only FastDB's ordinary B-tree behavior. It does not enable engine
custom-index methods or expose a plugin ABI.

### Full-text search — later internal adapter; unavailable in Phase 6

- Turso FTS is compiled by the optional `turso_core/fts` feature and excluded
  on `target_family = "wasm"`. `fastdb-frontend` does not currently enable
  that feature.
- The provider is Tantivy-backed and stored in engine B-trees. Its cursor
  flushes pending mutations from `pre_commit`, so rollback can cover table and
  FTS state. Existing inherited tests cover insert/update/delete, rollback,
  reopen/cache behavior, plan selection, and optimize.
- Pinned tokenizers are `default`, `raw`, `simple`, `whitespace`, and `ngram`;
  weights are supported. Writer, hot-cache, and chunk-cache defaults are 64
  MiB, 64 MiB, and 128 MiB respectively, with bounded retained connection
  snapshots.
- `OPTIMIZE INDEX` invokes the provider's segment merge, but the statement and
  all custom-index use require the experimental index-method option.
- Turso documents transaction visibility only after commit. The pinned test
  suite verifies post-commit and rollback behavior but does not supply
  read-your-writes for an FTS query in the writer transaction.
- The pinned provider validates tokenizer values but accepts unknown `WITH`
  keys without consuming them. Upstream commits `514263c6d` and `5e47d6557`
  add complete key validation/configurable ngrams and fix ngram casing.

Phase 8 must compile FTS only on supported native targets, enable it through a
FastDB-owned closed capability, validate every option before mutation, and
reject an affected FTS read after an indexed transaction write until commit.
The upstream differences are candidates for a later audited pin review, not a
reason to move the Phase 6 pin.

### Exact vectors — usable unchanged later

- `core/vector/mod.rs` provides native BLOB conversion with `vector64`, reverse
  extraction, cosine distance, and L2 distance. The SQLite dialect registers
  these functions without an optional feature gate.
- Dense float64 serialization is type-tagged. Distance implementations reject
  incompatible types or dimensions. FastDB must additionally reject non-finite
  public values and enforce the public 65,536-dimension ceiling before calling
  the engine.
- The engine supplies scalar distance functions, not a production ANN access
  path. Exact `ORDER BY distance LIMIT k` remains a linear scan, matching the
  Turso vector documentation.
- `toy_vector_sparse_ivf` is registered as an experimental index method. Its
  name and implementation status make it unsuitable for a GA claim; FastDB
  will reject it together with HNSW and DiskANN in Phase 9.

### Explain, integrity, checkpoint, and backup — mixed

| Facility | Classification | Pinned evidence and Phase 6/10 consequence |
| --- | --- | --- |
| Structured explain | Adapter | `Cmd::ExplainQueryPlan` exists, but the pin prepares only `Stmt`; use the structural round-trip adapter until a later audited pin supplies translated-command preparation. |
| Integrity check | Usable unchanged | `PRAGMA integrity_check` is implemented and has inherited corruption/index tests. Phase 10 will combine it with FastDB catalog/provider checks. |
| Stable-WAL checkpoint | Usable unchanged | `Connection::checkpoint` and `PRAGMA wal_checkpoint` use blocking stable-WAL checkpoint behavior when MVCC is disabled. |
| Deterministic connection close | Usable unchanged | `Connection::close` is idempotent, rolls back an active transaction, and checkpoints; the public FastDB database-wide lifecycle remains Phase 10 work. |
| Backup copy | Adapter | `VACUUM INTO` exists behind `DatabaseOpts::enable_vacuum`, but the public path is a SQL literal rather than a bound value. Phase 10 needs a FastDB-owned path/overwrite/cleanup contract before exposing `backup_to`. |
| FTS optimize | Experimental adapter | `OPTIMIZE INDEX` is custom-index-only and option-gated. Phase 8 must map `REBUILD INDEX` only after provider-specific tests. |

### Parallel writers — experimental/unfit

The retained pin's MVCC guidance and implementation remain experimental. It
does not meet the Phase 11 recovery, garbage-collection, bounded-memory, and
checkpoint gates. Phase 6 does not enable MVCC or multiprocess WAL. Phase 11
must perform a fresh exact-SHA audit and stop the Core 1.0 track if no stable
candidate qualifies.

## Focused verification obligations

Before Phase 6 completion, run and record:

```sh
cargo test --locked -p turso_core --lib
cargo test --locked -p core_tester --test integration_tests expression_index
cargo test --locked -p core_tester --test integration_tests reindex
cargo test --locked -p core_tester --test integration_tests integrity_check
cargo test --locked -p core_tester --test integration_tests index_method
cargo test --locked -p turso_pg_tests
```

Filtered or feature-gated coverage must be reported rather than assumed. FTS
remains a Phase 8 capability even if inherited FTS tests pass during this
audit.

## Audit result

**Retain** `977383ff40edc44ef410af062ed0d2322252a869`. Phase 6 foundation work can
proceed without a Turso implementation change. Upstream
`a94102c20b4c1c554f7c246606c2ed74db47199c` contains useful future candidates,
but adopting it is neither necessary nor authorized by this phase.
