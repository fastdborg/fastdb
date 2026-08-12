# Phase 0 Implementation Review

## Resolution re-review — 2026-08-12

Re-reviewed commit `05ced4eae` plus the corrective working-tree changes made
during verification.

Verdict: **PROCEED to Phase 1.** Every P1/P2 finding below is resolved and the
P3 planning-reference mismatch is accurately documented in `AGENTS.md`.

The re-review additionally found and fixed four residual issues: real WAL-sync
failure could have its original `Io` error masked by a redundant rollback;
the native benchmark still did less typed result materialization; the
execution-plan helper did not prove its rendered command preserved the
translated AST; and the engine-error mapper treated non-I/O completion errors
as I/O. Deterministic tests now cover the real WAL completion boundary and a
separate cleanup failure. The corrected full-sampling benchmark remains
approximately 4.1–14.8× over the cached native baseline, so performance is a
prominent Phase 1–3 risk and not a Phase 0 failure gate.

Final verification passed: 66 FastDB tests, 2,286 Turso core tests, 412
PostgreSQL frontend tests, the targeted expression-index/WAL tests, formatting,
FastDB clippy, and diff hygiene. No Turso implementation file differs from the
pinned baseline. Two pre-existing Turso warnings remain documented.

The original review is retained below as historical evidence for the changes.

Reviewed: `phase-0` at `5fb6e19e0d0b565c34723dec3b56eff66d290559`, relative to Turso baseline `977383ff40edc44ef410af062ed0d2322252a869`.

Verdict: **Request changes. Do not start Phase 1 yet.**

The core feasibility result is promising: FastDB has an independent parser/AST, lowers user statements directly to Turso AST, binds values, preserves opaque physical names, performs transactional first-use DDL+DML, survives reopen, and uses an expression index without modifying Turso core. The current implementation nevertheless misses two correctness gates and several explicit Phase 0 evidence/compatibility requirements. The committed `PROCEED` report is therefore premature.

Severity labels:

- **P1:** blocks the Phase 0 correctness decision.
- **P2:** blocks the Phase 0 Definition of Done or makes published evidence unreliable.
- **P3:** should be corrected, but does not invalidate the architectural spike alone.

## Findings

### P1 — Future format and dialect versions can be mutated

[`fastdb-frontend/src/execute.rs:54`](fastdb-frontend/src/execute.rs#L54) checks only `format_version`, only in `CREATE`. [`run_delete`](fastdb-frontend/src/execute.rs#L127) resolves the catalog and deletes immediately without validating either stored version. `CREATE` never validates `dialect_version`, and [`read_format_version`](fastdb-frontend/src/catalog.rs#L50) does not even fetch it.

Consequences:

- An older FastDB can open a database written with a future format and execute `DELETE`, permanently mutating a format it does not understand.
- A future `dialect_version` is accepted by every operation, including mutation.
- `SELECT` also attempts to interpret future-format catalog/data rows instead of refusing them.

This directly fails `P0-CAT-003` and the Definition of Done requirement that unknown nonzero/future format **or dialect** versions be refused before mutation.

Required correction:

1. Add one catalog validation function that reads and validates both versions.
2. Call it before resolving or interpreting an existing catalog on every operation; at an absolute minimum, before every mutation.
3. Add file-backed tests that independently set future format and dialect versions, reopen, assert a `Format` error, and prove schema/data remain byte-for-byte or logically unchanged.

### P1 — A failed `COMMIT` bypasses rollback and leaves transaction state unspecified

[`with_tx`](fastdb-frontend/src/execute.rs#L29) rolls back errors from the transaction body, but its successful-body branch uses `conn.exec_bound(COMMIT)?`. If COMMIT returns an I/O, busy, or engine error, the function returns immediately without attempting rollback or cleaning up the connection.

That contradicts the function's own “rolling back on any error” contract and the Phase 0 rule: after a transaction starts, every error must enter the real rollback path, with rollback failure attached if needed. Depending on Turso's failure point, the connection may retain an active/failed transaction and locally visible changes, or the commit outcome may be uncertain. The existing five failpoints all occur before COMMIT and cannot detect this path.

Required correction:

1. Treat COMMIT failure as a transaction failure requiring explicit cleanup according to audited Turso semantics.
2. Preserve both the commit error and any rollback/cleanup error.
3. Add deterministic failure coverage at commit/WAL durability boundaries and assert connection reuse plus reopen state. If Turso cannot safely expose the needed injection seam without a core change, record the Phase 0 stop condition rather than assuming success.

### P2 — The advertised error taxonomy does not distinguish unsupported syntax or I/O

[`FastDbError::Parse`](fastdb-frontend/src/error.rs#L31) derives `From<ParseError>`, so parser errors with `ParseErrorKind::UnsupportedSyntax` become `ErrorCategory::Parse`. The separate [`FastDbError::UnsupportedSyntax`](fastdb-frontend/src/error.rs#L33) constructor is never used by `Connection::execute`. Similarly, [`From<LimboError>`](fastdb-frontend/src/error.rs#L88) converts every non-string-matched constraint failure—including Turso `CompletionError` I/O failures—to `Engine`, leaving the `Io` category ineffective for engine I/O.

This fails the explicit Phase 0 requirement that callers/tests distinguish `Parse`, `UnsupportedSyntax`, `Constraint`, `Format`, `Transaction`, `Engine`, and `Io` boundaries.

Required correction:

- Implement a manual `From<ParseError>` that maps the unsupported kind while preserving its source span/diagnostic.
- Match typed `LimboError` variants rather than searching rendered strings. At least `Constraint`/foreign-key forms and I/O `CompletionError` forms need their correct categories.
- Add public-path category tests for malformed syntax, recognized unsupported syntax, duplicate ID, future format/dialect, injected transaction failure, engine failure, and I/O failure.

### P2 — Benchmark ratios do not compare equivalent result materialization

The benchmark report says both paths use equivalent result materialization, but the native path does substantially less work:

- [`Native::read_by_rid`](fastdb-benchmarks/benches/phase0.rs#L105) selects and returns only `json(doc)`, while FastDB selects `rid` plus `json(doc)`, decodes the canonical record ID, parses JSON with `serde_json`, allocates fields, and constructs a typed `Record`.
- [`Native::filter`](fastdb-benchmarks/benches/phase0.rs#L119) merely increments a row count and does not materialize or decode any result at all, while FastDB fully decodes the matching record.
- The benchmark called `delete` times a new `CREATE` plus `DELETE` on both paths ([lines 259–289](fastdb-benchmarks/benches/phase0.rs#L259)); it is not a delete-only measurement.
- The documented `cold_create` says it includes open, but both database opens occur in Criterion's untimed setup closure ([lines 154–190](fastdb-benchmarks/benches/phase0.rs#L154)).
- Native cold/delete setup constructs a path from a temporary directory that is dropped immediately ([lines 175 and 282](fastdb-benchmarks/benches/phase0.rs#L175)), so the file lifecycle is not equivalent to the FastDB path that retains its `TempDir`.

The published 12.6×/12.9× read ratios and 4.3× delete ratio therefore cannot be used to attribute overhead or judge the plan's future performance risk.

Required correction:

Make both paths return and decode the same logical record, move untimed setup out of measured closures, keep temporary-directory lifetimes equivalent, give each workload an accurate name/definition, rerun release benchmarks, and replace the published results and conclusions.

### P2 — The execution-plan test does not explain the translated FastDB statement

[`explain_field_filter`](fastdb-frontend/src/connection.rs#L145) claims to explain the exact translated predicate, but it constructs a second SQL string and sends it through `self.conn.prepare` ([lines 156–160](fastdb-frontend/src/connection.rs#L156)). The actual FastDB filter is separately lowered to Turso AST and executed through `prepare_translated_stmt_with_options`.

The current test proves that Turso's SQLite parser/compiler can select the index for a textually equivalent query. It does not prove that the actual FastDB-translated statement selects it, which is the P0.9 requirement and the architectural point of the spike.

Required correction:

Build `EXPLAIN QUERY PLAN` around the same translated AST produced by `physical_select_by_field_stmt`, or inspect the prepared FastDB statement's plan through an engine API. The test must fail if FastDB lowering changes the expression shape even when the handwritten diagnostic SQL remains unchanged.

### P2 — The parser accepts quoted record IDs outside the declared compatibility subset

The compatibility matrix says Phase 0 supports only a bare record ID (`table:identifier`) and that all other grammar is unsupported. [`parse_record_id_part`](fastdb-parser/src/parser.rs#L244), however, accepts both `TokenKind::Ident` and `TokenKind::String`. Inputs such as `CREATE person:'tobie' SET name = 'Tobie'` therefore parse and execute even though `COMPAT.md` does not declare or test that surface.

This violates the clean-room rule that every accepted syntax shape be deliberately specified and independently tested.

Required correction:

Either reject quoted IDs in Phase 0, or deliberately add them to `COMPAT.md`, compatibility research, parser tests, end-to-end CRUD/reopen tests, and record-ID escaping/injection tests.

### P2 — Required Phase 0 verification was omitted while the report marks every gate complete

[`docs/phase0-report.md:76`](docs/phase0-report.md#L76) records only the core library suite and explicitly says the Turso integration and PostgreSQL frontend suites are “not gating” ([lines 83–86](docs/phase0-report.md#L83)). That contradicts `plan-phase0.md` §12, which requires relevant unchanged Turso JSONB, expression-index, transaction, WAL, reopen, and PostgreSQL frontend tests.

The required test matrix is also incomplete:

- `P0-CAT-003` (unknown format/version) has no test and, as noted above, is not correctly implemented.
- `P0-INJECT-001` requires an end-to-end quoted/semicolon value test proving literal storage and unchanged schema. Only parser tokenization is tested.
- Error-boundary tests do not distinguish unsupported, format, engine, and I/O categories.

The review did reproduce passing targeted engine tests and the full PostgreSQL frontend suite, so the upstream baseline appears healthy. Those results do not cure the missing FastDB cases or make the committed report accurate.

Required correction:

Add the missing FastDB tests, record the exact upstream commands/results, and change the final report's decision only after every Definition of Done item has direct evidence.

### P2 — Formatting does not pass despite the report claiming the quality gate is satisfied

`cargo fmt --all -- --check` fails across the new benchmark, frontend, and integration-test files. The final report nevertheless states that all performance/quality items are satisfied at [`docs/phase0-report.md:161`](docs/phase0-report.md#L161).

Run `cargo fmt --all`, rerun the check, and record the result. `git diff --check` also reports an extra blank line at the end of `plan-phase0.md`; clean that up while updating the report.

### P3 — The repository guide references a missing preserved planning document

`AGENTS.md` lists `plan.md` as historical input, but `plan.md` is absent from the imported monorepo. Either restore the original planning document or update the durable guide and Phase 0 preservation claim to accurately describe its disposition.

## Verification Performed

| Command | Result |
| --- | --- |
| `cargo test -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests` | Pass: parser 29, frontend 14, atomicity 6, index 1, vertical slice 1; no failures. |
| `cargo clippy -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets` | Pass for FastDB crates; two pre-existing `turso_core` warnings. |
| `cargo fmt --all -- --check` | **Fail:** multiple new FastDB files require formatting. |
| `cargo test -p core_tester --test integration_tests expression_index` | Pass: 3 tests. |
| `cargo test -p core_tester --test integration_tests without_mvcc` | Pass: 5 filtered WAL/transaction/header tests. |
| `cargo test -p core_tester --test integration_tests committed_wal_survives_power_loss` | Pass: 1 test. |
| `cargo test -p turso_pg_tests` | Pass: 412 tests. |
| `git diff --name-only 977383ff..HEAD -- core sqlite postgres bindings sync testing tests` | Empty: no Turso implementation changes. |
| `git merge-base --is-ancestor 977383ff HEAD` | Pass. |

I did not rerun the Criterion benchmark because its comparison methodology must be corrected before new measurements would be meaningful.

## Recommended Phase 0 Re-entry Gate

Keep the current architecture and fix the review findings rather than redesigning the spike. Phase 0 can return to **PROCEED** when:

1. Future format and dialect versions are rejected before all mutations and tested after reopen.
2. Commit failure has a defined, tested cleanup/rollback path.
3. Error categories behave as declared through the public execution path.
4. The actual translated filter is the statement whose execution plan names the index.
5. Benchmark workloads are equivalent and the report contains corrected results.
6. Undeclared quoted-ID syntax is rejected or fully specified/tested.
7. All required FastDB and unchanged upstream evidence is recorded.
8. Formatting and lint checks pass, and the final report is revised to match the evidence.
