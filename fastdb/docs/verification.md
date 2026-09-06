# Local foundation verification — 2026-09-06

- Upstream base: `046e9cbf67d22491e8ecc941ec2891b02a9f3cad` (v0.7.2).
- Development branch: `feat/embedded-foundation`.
- Platform: Linux x86_64; Rust 1.88.0; default turso_core features.
- Cargo.lock SHA-256: `a38e8a443a0f8e2e87a20246e6064062c13822c6f68530fd76d711019fb1f4c0`.
- `fastdb/scripts/check.sh`: passed scoped formatting, Clippy `--all-targets --no-deps -- -D warnings`, and 11 tests (7 persistence/recovery harness tests including the subprocess helper, 1 differential SQL test, 3 parser tests).
- Differential harness runs 14 sequential relational SQL probes against the raw pinned engine and FastDB, comparing column names and typed rows. It is compact smoke coverage, not the final SQL compatibility matrix.
- Recovery harness exits a child process without dropping its connection after a committed indexed document and uncommitted changes. Reopen preserves the committed record/index and discards uncommitted changes. Mid-commit/checkpoint fault injection remains untested.
- All 34 archived inherited workflows match their upstream bytes. Only fastdb-ci.yml remains in the executable workflow directory.
- Upstream source changes are restricted to workspace member/lockfile wiring and workflow relocation; no core/parser/bindings/CLI implementation edits.

No remote CI result, platform packaging evidence, release benchmark, restore/upgrade rehearsal, or V1 completion is claimed.

CLI smoke: the 10-statement `fastdb/examples/persistent.fastql` example ran successfully through `cargo run --locked -p fastdb-cli -- :memory:`; all output lines parsed as JSON, no errors were returned, and the post-rollback SELECT returned Alice.

## Collection SELECT verification — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 16 tests after adding AST lowering and pure accessors. Five new real-engine tests cover typed/missing/null projections, nested and quoted field paths, boolean scalar predicates, record-ID predicates and numeric sorting, aliases and positional ORDER BY, pagination, document/document LEFT JOIN, mixed relational/document JOIN, typed bound filters, indexed transaction visibility, and protected internal names. EXPLAIN QUERY PLAN asserts a managed-index SEARCH for nested equality and a primary-key SEARCH for id equality. Existing differential SQL and recovery tests continue to pass.

Current Cargo.lock SHA-256: `bca7216de2f75f7f2cdfe7935f016ac82aa9b70c7bbe76ad6922aa91d2f42791`.

This does not validate unimplemented SQL-shaped writes, advanced collection expressions, concurrency races, or the complete V1 query contract. See status.md for remaining work.

## SQL-shaped writes and records — 2026-09-06

`fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied, and all 23 tests. Seven new real-engine tests cover multirow column-list inserts; pre-update assignment semantics; multirow failure rollback inside an outer mixed transaction; typed object/boolean parameters; SQL-shaped validation and index maintenance; nested SET/UNSET and quoted-path distinctions; immutable and overlapping targets; fixed/dynamic record expressions and typed sorting; and numbered/anonymous parameter binding. The existing persistence, abrupt-exit recovery, SELECT/index-plan and differential SQL tests continue to pass.

A new differential probe confirms that this pinned engine rejects `$name::suffix` parameters and that FastDB preserves its exact error. This is an upstream syntax limitation, not permission to reinterpret namespace-like parameter text.

No upstream core/parser/bindings/CLI source modifications or dependency changes were needed. Full V1 write/expression/authorization and release gates remain open as listed in status.md.

CLI persistence smoke: built fastdb-cli with --locked; ran the nine-statement sql-writes.fastql example against a fresh file, verified rollback returned Alice, exited, reopened in a separate process, and queried the indexed city predicate to retrieve both committed names. All output parsed as JSON without errors.

## UPSERT and catalog lifecycle — 2026-09-06

`fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied, and all 30 tests. Seven new real-engine tests verify ID-based and target UPSERT, required/unique failures, shallow patches, outer-transaction rollback, DROP INDEX metadata cleanup and restored uniqueness after rollback, field removal without data deletion, incompatible field/index definitions, collection drop/recreation and weak links, non-converting IF NOT EXISTS, logical INFO, close/reopen of lifecycle changes, and rejection of unknown catalog versions. The compatibility fixture also reads the initial unversioned catalog as version 1.

The raw DROP INDEX metadata gap discovered during inspection is covered by a regression test that verifies lookup metadata disappears after drop and returns with uniqueness after rollback. No upstream implementation files or dependencies changed. Full V1 semantics, clients, tools, resource limits and release evidence remain incomplete.

## Field CHECK validation and catalog version 2 — 2026-09-06

`fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied, and all 37 tests. Seven new real-engine tests cover every supported document write path; multirow rollback and final-candidate cross-field checks; failed definition overwrite; missing/null skip rules; nested paths and quoted parentheses; rejection of reads, parameters, nondeterminism and non-eligible functions/types/collations; eligible scalar casts/collations/GLOB; persisted CHECK enforcement; and atomic upgrade/rollback of an unversioned catalog fixture to version 2. A CHECK-bearing entry falsely marked version 1 is rejected.

The existing catalog, persistence, crash-smoke, SELECT, SQL-write and differential tests continue to pass. No upstream implementation files or dependencies changed. These checks do not constitute the final previous-binary upgrade rehearsal, expression resource-limit certification, or V1 release audit.

## Document expressions and predicate object patches — 2026-09-06

`fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied, and all 43 tests. Six new real-engine tests cover scalar precedence against the pinned engine, pre-update field reads, lazy typed null helpers, UPSERT insert/update expressions, predicate/body parameter binding, whole-statement rollback on a later candidate's CHECK failure, typed path reads and missing/null distinction, logical record comparisons, invalid composite operations/paths, and bounded flat expression chains. Existing persistence, catalog, indexes, recovery and ordinary SQL differential tests continue to pass.

No upstream implementation files or dependencies changed. Helpers are currently available in object-write expressions; SQL helper propagation, broader expression grammar, complete resource accounting and the full V1 release gates remain incomplete.

## Typed SQL helper lowering — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 48 tests. Five new real-engine tests cover nested helpers in SELECT/SET/VALUES, typed object/binary/record arguments, numbered parameters, null/presence predicates, outer-join doc::row, lazy null-helper evaluation, parentheses, numeric record-key sorting, invalid helper arguments/modifiers, and generated projection alias quoting. The wider suite caught numeric VALUES aliases that required quoting; the fix and regression test are included. Existing write, persistence, index, recovery and ordinary SQL differential tests pass.

No dependencies or upstream implementation files changed. This does not prove full expression type propagation, complete resource limits, QuickJS, vector/link semantics, bindings, tools or V1 release readiness.

## Typed CASE expressions — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 52 tests. Four new real-engine tests cover searched/simple CASE, arrays/records/booleans in branch results, omitted ELSE and SQL null matching, nested/helper composition, scalar predicates, ordering, lazy unselected results, malformed object CASE syntax, and whole-statement rollback after a later candidate fails array validation. SELECT, SET, VALUES, object UPDATE and UPSERT paths are exercised. Existing ordinary SQL, persistence, index and recovery tests remain green.

No dependencies or upstream implementation files changed. Broader comparison semantics and the remaining V1 requirements stay open.

## Collection INSERT SELECT — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 56 tests. Four new real-engine tests verify finite self-inserts, typed array/record/boolean copying, positional repeated projections, sorting/limits, explicit relational sources and parameters, empty-source width validation, whole-statement unique-conflict rollback with index cleanup inside an outer transaction, and rejection of compound VALUES without a partial insert. Existing validation, persistence, recovery and ordinary SQL tests continue to pass.

No dependencies or upstream implementation files changed. Source-query coverage remains bounded by current SELECT lowering; complete source semantics, resource accounting and the rest of V1 are not yet verified.

## SQL-shaped RETURNING projections — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 60 tests. Four new real-engine tests cover inserted/updated/deleted snapshot values, typed fields/helpers/parameters, aliases, mixed star projections, INSERT SELECT, empty-result metadata, rejection of aggregate/window/subquery projections, and data/index rollback after runtime projection failure. The engine aborts the outer transaction on these runtime helper errors; the test verifies this exact pinned behavior and successful new-transaction use afterward. It does not claim preservation of an outer transaction after engine abort.

No dependencies or upstream implementation files changed. Object-write projection grammar, full error/transaction-state contracts, bounded snapshot processing and the remaining V1 requirements remain open.

## Object-write RETURNING projections — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 63 tests. Three additional real-engine tests exercise object INSERT, direct/predicate UPDATE, insert/update UPSERT and direct DELETE projections; typed results and bound values; missing-target metadata; malformed/source-reading/aggregate/internal-name projection rejection with mutation rollback; quoted RETURNING fields and strings; nested predicates; and literal semicolon preservation. The prior SQL-shaped RETURNING, engine-abort, persistence, recovery and differential tests remain green.

No dependencies or upstream implementation files changed. Broader query/type semantics, transaction-state reporting, bounded execution, bindings, tools and the remaining V1 requirements stay open.

## Transaction-state reporting — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 67 tests. Three new engine integration tests verify BEGIN/COMMIT/savepoint observations, independent connection state, syntax/validation errors preserving active transactions, retained writes after close/reopen, and runtime helper aborts discarding earlier outer-transaction work while allowing a new transaction. A CLI subprocess test checks before/after state fields on success and error JSON and confirms aborted data is absent. Existing query/write/recovery tests continue to pass.

No dependencies or upstream implementation files changed. The API samples the engine autocommit flag; it does not infer commit/rollback cause, guarantee future commit success, or solve the remaining transaction/error/resource and V1 release gates.

## Multiline script and batch execution — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 71 tests. Three new integration tests cover nested multiline document literals, semicolons in strings/comments, UTF-8 byte offsets, optional final terminators, empty scripts, stop-on-error with explicit transaction state, lexical failure before execution, and a real multi-statement SQL trigger containing CASE END. A new CLI subprocess test verifies default script mode, structured offsets/errors, stopping before later statements and nonzero failure exit status. The existing CLI transaction test now exercises --line continuation and its nonzero error exit status.

No dependencies or upstream implementation files changed. Scripts/results are currently materialized; interactive/streaming/bounded CLI work, import/export, migrations and the remaining V1 gates stay open.

## Batched one-hop forward links — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 75 tests. Four new real-engine tests cover 260 unique collection references spanning multiple 128-key batches, canonical targets, ordered duplicates/nulls/missing targets, reference-count limits, top-level SELECT fetches, retained nested record values without recursion, forbidden nested/filter/order/write uses, relational TEXT/INTEGER primary keys and invalid targets, and two-connection snapshot visibility before/after commit. Existing write/query/CLI/recovery tests continue to pass.

No dependencies or upstream implementation files changed. The fetch stage runs in frontend query lowering under a shared savepoint; no database-reading UDF was introduced. Full target-batch EXPLAIN instrumentation, total fetched-byte/time budgets, collation/schema-race/interleaving stress and the rest of V1 remain release work.

## Initial dense vectors and exact queries — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 78 tests. Three new real-engine tests cover dense32/dense64 typed construction and parameters, vector<N> definitions against existing data, SQL/object writes, malformed/non-finite/wrong-dimension failures, scalar-index rejection, INFO dimensions, native extraction, exhaustive cosine ranking, and close/reopen. The cosine test compares collection output directly with the unlowered pinned native query and uses tolerance against mathematical zero; upstream floating-point output is not rewritten. Existing query/write/recovery tests remain green.

No dependencies or upstream implementation files changed. Dense float32/float64 are the initial validated representations; other vector encodings/functions, numerical edge cases, resource/platform coverage, benchmarks and the remaining V1 gates are unfinished.

## Sparse, quantized and bit vectors — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 81 tests. Three additional vector tests compare sparse32/float8/bit constructor bytes with native output, retain their encodings across close/reopen, convert to dense values, exercise typed slice/concat/Jaccard operations, and reject malformed lengths/dimensions/sparse indexes/quantization metadata. A zero-entry sparse vector with positive dimensions remains valid.

The new validator exposed the pinned sparse-concat index-offset defect. The typed frontend corrects that operation without changing upstream source or ordinary SQL delegation; a coordinate-level regression asserts [1,0,3,1,0,3]. Other native format restrictions remain enforced. No dependencies changed. Wider numerical/fuzz/resource/platform qualification and performance benchmarks remain V1 release work.


## Initial bounded bundled QuickJS functions — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 86 tests. Three runtime tests exercise interrupt recovery, absent host I/O bindings, allocation/stack exhaustion and expanded-output limits. Two integration tests cover SQL/object Unicode helpers, validated writes and unique-index conflicts, argument-as-data handling, invalid names/types/forms and oversized-input rejection before mutation. The stack fixture exposed QuickJS's `Maximum call stack size exceeded` exception wording; its mapping to `FDB_LIMIT` is now covered.

The locked dependency change adds rquickjs/core/sys 0.12.2, hashbrown 0.17.1 and relative-path 2.0.1. Existing dependency edges are preserved, with the old relative-path edge explicitly qualified as 1.9.3. No upstream implementation files changed. Evidence is local Linux only; runtime limits are per call and cooperative, and broader packaging, security, performance and V1 release qualification remain pending.


## Initial scalar grouping and HAVING — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 88 tests. Two new real-engine tests compare grouped counts/sums/averages, WHERE/HAVING filters, ordinal keys and pagination against equivalent relational tables; verify missing/null grouping and numeric-constant ordinal handling; reject unsupported projection aliases; and verify atomic grouped INSERT SELECT validation with no partial target rows. Existing query, runtime, persistence and recovery checks remain green.

No dependencies or upstream implementation files changed. Grouping currently uses scalar SQL semantics; alias precedence, composite/typed equality, broader queries and the remaining V1 release requirements remain incomplete.


## Mixed relational and collection stars — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 90 tests. Two new real-engine tests verify relational star column order, quoted names, binary values, outer-join null extension, empty-result metadata, source-order mixed unqualified stars, view column expansion, validated INSERT SELECT and GROUP BY ordinals over expanded columns. Relational values are compared with native ordinary-table/view queries. The grouping guard now accepts qualified field aliases identical to their final field name, resolving the regression exposed by expanded stars.

No dependencies or upstream implementation files changed. Duplicate-result-name policy remains in force. Broader alias/query semantics, schema concurrency, dependency authorization and all remaining V1 release gates stay open.


## Initial native collection windows — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 93 tests. Three new real-engine tests compare partitioned row_number, sum and count windows, inline/named windows and final ordering with ordinary relational queries; verify windowed INSERT SELECT validation rolls back every row; and retain RETURNING/WHERE restrictions. Differential errors establish that the pinned engine rejects custom frame specifications and lag. Aggregate-local ORDER BY with OVER is rejected before the upstream window-rewrite assertion.

No dependencies or upstream implementation files changed. This verifies the initial scalar window subset, not full typed/window/collation/resource semantics or remaining V1 release requirements.


## Versioned collection JSON/NDJSON transfers — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 97 tests. Three new API tests round-trip both formats between databases with int64 extremes and typed integer IDs, strings resembling records, booleans/nulls, tagged-looking nested objects, binary/vector bytes and negative-zero binary64 bits. They verify whole-import unique-index rollback, malformed trailing lines, unsupported versions, duplicate fields, noncanonical/numeric integer payloads and non-finite number-bit rejection. A CLI subprocess test exports a persistent collection, imports into another file, reopens and compares export bytes, and confirms duplicate import failure leaves data unchanged.

No dependencies or upstream implementation files changed. The versioned transfer format is documented in transfer.md and is separate from existing query-result JSON. The current implementation materializes transfers with encoded-size/document-count limits; streaming, relational transfer, schema backup, migration tooling, total-memory qualification and wider V1 release work remain pending.
