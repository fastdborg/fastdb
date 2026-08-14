# FastDB Phase 6 report

Status: technically complete locally on 2026-08-13; Phase 7 is not started

Phase 6 migrated FastDB format 1 to format 2 and established the sealed
multimodel provider foundation without changing the pinned Turso engine. This
report is local engineering evidence, not an alpha release authorization or a
claim that FastDB Core is production-ready.

## Engine and upstream decision

- Retained engine SHA: `977383ff40edc44ef410af062ed0d2322252a869`.
- Read-only comparison target: `upstream/main` at
  `a94102c20b4c1c554f7c246606c2ed74db47199c` after fetching the official
  upstream remote on 2026-08-13.
- Decision: retain the pin. No upstream merge, cherry-pick, pin update, Turso
  implementation edit, push, tag, publication, or release operation occurred.
- Detailed capability findings are in `docs/phase6-engine-audit.md`. The pin
  provides translated statement preparation, ordinary B-tree maintenance,
  stable vector scalar functions, integrity checking, and WAL checkpointing.
  Its FTS/custom-index paths are experimental or option-gated, and its MVCC
  implementation is not accepted for a production concurrency claim.
- The engine cannot directly prepare a translated `EXPLAIN` command. FastDB's
  internal adapter renders only its already-constructed, value-free opaque
  physical AST, reparses it with Turso, proves structural identity, and then
  requests the plan. FastDB source and values are never rendered as SQL.

## Format 2

`docs/format-v2.md` is the authoritative Phase 6 format contract. Format 2
retains the existing record, document, path, and B-tree encodings while adding:

- normal/relation table metadata;
- analyzer, hidden-column, and capability catalogs;
- index kind, provider, provider version, canonical options, lifecycle state,
  and encoding version;
- closed validation that rejects every unregistered capability or incompatible
  provider value before publishing a catalog snapshot.

Format-1 migration uses one immediate transaction. It adds table and index
metadata columns, creates the three new catalogs, validates ownership and exact
physical schemas, publishes the format header last, commits, then replaces the
shared immutable catalog snapshot. Five deterministic failpoints prove rollback
at every publication boundary. Each failed attempt left the main database bytes
identical to the source fixture, reopened cleanly, and subsequently migrated.

The committed format-2 fixture is
`fastdb-tests/fixtures/phase6-format2.fastdb`, with SHA-256:

```text
26c6e047114903dcf22fe4540cd23fb182b7767c847f61ee22e4a9441f0d33aa
```

The fixture reopens, accepts further mutations, passes `PRAGMA integrity_check`,
and continues to select its original expression index. A child process also
migrates a copied format-1 fixture and exits abruptly without engine close;
recovery reopens format 2 with an intact B-tree plan and integrity result.

## Closed provider foundation

The crate-private provider registry exposes no public plugin ABI, callbacks,
dynamic code loading, logical physical names, or user-SQL generation. Phase 6
registers only the built-in B-tree provider. FTS, vector indexes, and graph
adjacency providers remain unavailable until their implementation phases.

Unknown kinds, providers, provider versions, options, states, encodings, table
kinds, hidden-column rows, analyzer rows, and capability requirements are
format errors before cached catalog publication or FastDB mutation.

A provider available only under the `testing` Cargo feature derives a hidden
integer from a bound JSON document. Its physical table and hidden column use
independently allocated opaque IDs and direct Turso AST. A failpoint between
the document update and hidden-column update drives the real transaction
rollback path: both remain at the old value after failure and both advance
after commit. The test provider is not persisted in the production registry
and cannot be selected through query syntax.

## Language and maintenance surface

The independent FastDB AST now represents namespaced function calls,
expression projections and aliases, provider-specific index options,
`EXPLAIN`, `REMOVE INDEX`, and `REBUILD INDEX`.

Executable Phase 6 behavior is deliberately smaller than the parseable AST:

- expression projections execute; non-field expressions require `AS`, while
  existing field projections preserve their nested object shape;
- `EXPLAIN SELECT ...` returns ordinary structured values containing ordinal
  and plan detail;
- `REMOVE INDEX name ON [TABLE] table` and `REBUILD INDEX name ON [TABLE]
  table` resolve logical names through catalogs and execute opaque physical AST
  transactionally;
- namespaced functions, `FULLTEXT ANALYZER`, and `USING ... WITH (...)` are
  parsed structurally but return `UnsupportedSyntax` before mutation until the
  owning provider phase implements them.

`COMPAT.md` marks only expression projections/aliases, FastDB's structured
explain subset, and B-tree remove/rebuild as Partial with executable evidence.
All FTS, vector, graph, and namespaced function behavior remains Unsupported.
The clean-room notes in `docs/compat-research/phase6.md` use public SurrealQL
documentation; no SurrealDB source, tests, fixtures, corpus, or expected output
was copied.

## Verification evidence

The authoritative local matrix completed on x86-64 Linux. Pre-existing Turso
warnings were emitted by inherited crates; all FastDB clippy checks passed.

| Command or gate | Result |
| --- | --- |
| `cargo metadata --locked --no-deps --format-version 1` | Passed. |
| all three required formatting checks; `git diff --check` | Passed. |
| FastDB package clippy matrix, all targets | Passed. |
| parser tests and compatibility checker | Passed, including 4 Phase 6 AST tests. |
| frontend tests | Passed: 14. |
| async Rust API all targets and doc tests | Passed. |
| CLI all targets | Passed, including 2 Phase 6 CLI tests. |
| FastDB integration package | Passed all binaries, including 10 Phase 6 tests. |
| parser fuzz, `-max_total_time=300` | 4,612,408 runs in 301 seconds; no crash. |
| post-change structured CRUD fuzz, `-max_total_time=300` | 2,190 runs in 301 seconds; no crash. |
| `turso_core --lib` | Passed: 2,286; 17 ignored. |
| core expression-index filter | Passed: 3. |
| core stable-WAL/no-MVCC filter | Passed: 5. |
| core transaction-visibility filter | Passed: 1. |
| PostgreSQL inherited suite | Passed: 412. |
| Whopper inherited suites | Passed: 37 unit, 12 regression, 1 cross-platform. |
| fixture SHA-256 verification | Passed for all three fixtures. |
| release CLI and Phase 5 benchmark builds | Passed. |

The parser fuzz corpus included the Phase 6 projection, namespaced-call,
provider-option, explain, remove, and rebuild shapes. The structured CRUD target
reran after the final provider test support change.

## Unchanged Phase 5 benchmark gate

Raw samples are preserved in
`docs/benchmarks/phase6-phase5-regression.json`. The run used the retained SHA,
10,000 seed records, 200 warmups, 200 measured samples, stable WAL/full
durability, identical 1,024-byte document payloads, equivalent opaque physical
schemas, and complete public result materialization.

| Gate | Ratio | Limit | Result |
| --- | ---: | ---: | --- |
| Point read p50 | 1.0634x | 1.5x | Passed |
| Point read p99 | 1.0555x | 2.0x | Passed |
| Indexed filter p95 | 1.2856x | 2.0x | Passed |
| Write p95 | 1.5364x | 2.0x | Passed |
| Checkpointed main-file storage | 1.0059x | 1.5x | Passed |

FastDB used 14,704,640 bytes and native Turso used 14,618,624 bytes after
checkpoint and close. The benchmark's machine-readable `gates.passed` value is
`true`.

## Provenance and phase decision

Implementation changes are confined to FastDB-authored crates, fixtures,
plans, compatibility/format/release documentation, and the FastDB crash helper.
No inherited Turso engine, SQLite parser, PostgreSQL frontend, binding, WAL,
optimizer, JSONB, or inherited test source changed.

All Phase 6 definition-of-done gates pass locally. No stop condition was
encountered: migration cannot publish a mixed snapshot, unavailable providers
fail closed, ordinary B-tree plans remain selected, options do not reach
mutation, opaque names remain validated, and the retained pin required no
change. Phase 7 may begin under a new authoritative `plan-phase7.md`.

Core remains pre-alpha and is not production-ready. Phases 7 through 12, plus
separate release authorization, remain required.
