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


## Forward migration runner — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, and all 101 tests. Three new API tests verify exact-source idempotence and changed/missing-history rejection; atomic pending-run rollback of relational DDL, collection writes and history after validation and engine-aborting runtime failures; corrected retries; transaction-control/PRAGMA/ATTACH/VACUUM/TEMP rejection before writes; ordered versions and autocommit requirements. A CLI subprocess test applies numerically ordered files to a persistent database, reopens and skips them, then rejects an edited applied file.

No dependencies or upstream implementation files changed. The ledger retains exact SQL rather than hashes. Concurrent-runner stress, interrupted-commit/recovery/ledger-upgrade rehearsal, total resource limits, restore workflows and remaining V1 qualification are not yet complete.


## Initial native Node client and TypeScript declarations — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting/Clippy for five packages, all 101 Rust tests, the native addon build, four Node tests and strict TypeScript declaration checks with pinned TypeScript 5.8.3. Native tests cover int64 extremes, typed IDs, negative zero, binary/vector values, nested objects/arrays, unsafe-Number rejection, validation and engine-abort transaction reports, explicit close/reopen persistence, cardinality helpers, and prototype-looking field names. Linux x86_64 with Node 24.19.0 is the current runtime evidence.

The root lockfile adds only the fastdb-node package entry, reusing existing pinned N-API packages; upstream implementation files remain unchanged. The scoped workflow now pins setup-node and Node 24.19.0 and installs only the declaration checker from its package lock. Hosted CI has not run. The addon is synchronous and private; broader frontend query semantics, native batch/migration/transfer methods, async/cancellation, lifecycle stress, cross-platform prebuilds and release packaging remain unfinished.


## Node migration and transfer methods — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting/Clippy, all 101 Rust tests, six native Node tests and strict TypeScript declaration checks. Two new native tests verify migration versions above the JavaScript safe-integer boundary, repeated/edited history, engine-abort rollback and corrected retries, JSON/NDJSON typed round trips, duplicate-import rollback, explicit-transaction observations after malformed input and invalid-format rejection. All operations call the existing checked Rust migration/transfer APIs through a shared native report envelope.

No dependencies, upstream implementation files or CI configuration changed. The methods remain synchronous and materialized; batch APIs, async/cancellation, frontend semantic gaps and release packaging remain V1 work.


## Dedicated-worker asynchronous Node client — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting/Clippy, all 101 Rust tests, ten native Node tests and strict TypeScript declarations. Four new async tests verify ordered transaction submissions, typed data across worker messages, migration/transfer methods, queue-count rejection and recovery, idempotent close draining accepted work, open failure cleanup, engine-abort reports, active-transaction close/reopen rollback, and isolation between two workers. A million-row native cross-join aggregate verifies that a caller-side event-loop callback runs before the query completes. Tests close workers on failure as well as success.

No dependencies or upstream implementation files changed. Database work runs on one dedicated worker per AsyncDatabase; caller-side encoding/decoding and message copies remain materialized. Queue byte accounting, forced worker failures, cancellation, broader lifecycle/resource stress and packaging still need release-level qualification.


## Cooperative engine interruption — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting/Clippy, all 102 Rust tests, eleven native Node tests and strict TypeScript declarations. A new Rust thread test and Node worker test interrupt large relational INSERT SELECT operations, verify FDB_CANCELLED with no partial target rows, and reuse the same connection successfully. Idle interruption does not poison later work, and weak handles/registry keys report false after connection close. Node interruption reaches the worker-owned connection without scheduling behind its active query or terminating the worker.

The first regression exposed the pinned engine's run_collect_rows conflation of Interrupt with Busy. FastDB now collects through run_with_row_callback, which preserves these distinct errors, across all frontend row-reading paths. Existing query/write/catalog/transfer/migration tests remain green. No dependencies or upstream implementation files changed. This establishes connection-wide cooperative interruption only; per-request signals, stage-spanning cancellation, document/index/commit/checkpoint interruption and broader resource/lifecycle qualification remain unfinished.


## Persisted binary64 precision regression — 2026-09-06

A deterministic encode/decode test reproduced a one-bit error in the former JSON reader for 2.291712365432881e-9. Enabling the existing serde_json float_roundtrip feature fixes that regression without changing serialized format or dependency versions. The test probes 4,096 deterministic bit patterns plus boundary cases, skipping non-finite inputs. A real-engine test verifies scalar/nested numeric bits and matching scalar-index lookups after close/reopen, including the failing value, subnormals, signed zero and finite extremes. The Node typed-document test includes the previously failing number.

`fastdb/scripts/check.sh` passed formatting/Clippy, all 104 Rust tests, eleven native Node tests and TypeScript declaration checks. No upstream implementation files or lockfile entries changed. Feature unification affects serde_json in the combined build and is recorded in UPSTREAM.md; broader numerical and upgrade qualification remains pending.


## Standalone typed parameter projections — 2026-09-06

`fastdb/scripts/check.sh` passed scoped formatting/Clippy, all 107 Rust tests, eleven native Node tests and strict TypeScript declarations. Three new Rust tests verify direct named/numbered composite, record, vector and boolean projections; source-free DISTINCT and empty results; scalar WHERE/GROUP BY/HAVING alias handling and first-match case-insensitive aliases; retained scalar-query behavior and missing-name errors; and rejection of predicates/grouping on post-fetch aliases. The Node prototype-looking-field regression now uses SELECT $value directly instead of a helper wrapper.

No dependencies or upstream implementation files changed. This covers the current source-free SELECT lowering subset, not complete alias/type propagation through relational sources, subqueries/CTEs, compound queries or composite ordering/grouping. Remaining V1 requirements stay open.


## Fatal worker transport cleanup — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 107 Rust tests, twelve Node tests and TypeScript declarations. An isolated child-process fixture replaces the Worker transport to inject message decoding failures, failed shutdown sends, worker error/exit sequences, unexpected exit and a lost close acknowledgement. It verifies that pending requests share the first FDB_WORKER cause, later requests reject, cleanup reaches worker exit and close remains idempotent. A completion marker ensures unresolved fixture promises cannot yield a false passing process exit. Existing real native-worker tests still pass.

Message errors now initiate cleanup, and close after a fatal error waits for exit instead of simply rejecting and abandoning the worker. An error during close rejects after cleanup. No dependencies or upstream implementation files changed. These deterministic transport tests do not qualify native crashes, unknown write outcomes, forced-termination database recovery or interrupted commits; those release gates remain open.


## Uniform reference target validation — 2026-09-06

New regressions first demonstrated that unindexed nested inserts and patches accepted reserved reference targets while indexed scalar handling already rejected them. The shared Record validator now applies canonical logical-name validation recursively, without requiring target existence or rejecting mixed-case names.

`fastdb/scripts/check.sh` passed formatting/Clippy, all 110 Rust tests, thirteen Node tests and TypeScript declarations. Three new Rust tests cover reserved/NUL/empty targets in nested insert/DOCUMENT/import paths, failed patch/upsert/SQL-write preservation of existing documents/indexes, and mixed-case index uniqueness/fetch identity. A new Node test verifies recursive target validation through typed parameters without a reference index. No dependency versions, upstream implementation files or serialized formats changed. Invalid references written by earlier prototypes now fail decoding and require correction before upgrade; broader schema/corruption/upgrade qualification remains open.


## Deterministic partial-write interruption coverage — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 112 Rust tests, thirteen Node tests and TypeScript declarations. Two new frontend tests use a one-shot engine progress callback after total_changes advances, proving that mutation has begun before cancellation. They verify document/index restoration for UPDATE, DELETE and INSERT SELECT, both in autocommit mode and inside an explicit transaction, followed by successful writes. At these interruption points the outer transaction remains active and prior uncommitted work survives; explicit rollback then restores the committed baseline. This differs from previously tested runtime helper failures that abort the outer transaction.

A cancelled index build leaves neither catalog metadata nor physical index storage and can be retried with working lookups. No production implementation, dependencies or upstream files changed. The callbacks are test-only and read atomic change counters without issuing SQL. This strengthens controlled rollback evidence, not process-crash, interrupted commit/checkpoint, power-loss or full resource qualification.


## Node script batches and ordered result conversion — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 113 Rust tests, fourteen Node tests and strict TypeScript declarations. A new Rust visitor test verifies early stopping, consumer-error preservation of prior writes, and lexical splitting before execution. A new Node test exercises both synchronous and worker clients: UTF-8 byte offsets, explicit transaction state after duplicate insertion, typed results, lexical rejection before writes, and stopping on an unrepresentable numeric result before a later insertion.

The bridge converts each statement result before executing the next statement. Statement and encoding errors are returned as batch entries; lexical errors throw or reject. Batches add no implicit transaction or rollback, and consumer errors leave prior execution effects intact. No dependencies or upstream implementation files changed. Results remain materialized; bounded script/result memory, streaming and broader release qualification remain open.


## CLI report delivery before later execution — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 115 Rust tests, fourteen Node tests and TypeScript declarations. Two new CLI tests inject write and flush failures through the actual report writer with a real engine connection. They verify that later writes/COMMIT do not execute, committed prior work remains, and an active transaction remains available for rollback. Existing CLI subprocess tests retain lexical-error, execution-error and report-shape coverage.

A separate local Linux subprocess smoke redirected stdout to /dev/full: the CLI exited nonzero without a panic, and reopening the database confirmed that the first CREATE persisted while the later INSERT never ran. This device smoke is local evidence, not a cross-platform test.

The CLI now visits reports between statements and flushes each JSON line before advancing. It no longer holds all batch reports at once. Full script input, each statement's rows and report serialization remain materialized; bounded row streaming and interactive UX remain open. No dependency or upstream implementation files changed.


## Deeper qualified collection SELECT paths — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 117 Rust tests, fourteen Node tests and TypeScript declarations. Two new collection SELECT tests verify paths beyond the pinned parser's three-name limit: typed boolean/record projections, missing fields, joins, ordering, equality results before/after managed-index creation, quoted keys containing dots/quotes, comments between segments, 64-field paths and FDB_LIMIT for 65 fields. They also verify rejection of unknown qualifiers/direct internal markers and retention of ordinary schema-qualified relational names and string literals. A separate CLI EXPLAIN smoke confirmed a managed index SEARCH for a deep-path predicate.

FastDB rewrites longer qualified paths into a temporary AST marker and resolves it to the existing typed/scalar accessors before native preparation; no engine function or upstream parser patch is added. Deeper write-expression/RETURNING parsing, derived sources and broader resource/release qualification remain pending. No dependencies or upstream implementation files changed.


## Deep paths in writes and RETURNING — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 118 Rust tests, fourteen Node tests and TypeScript declarations. A new real-engine RETURNING regression covers deep qualified paths in object INSERT snapshots, UPDATE expressions/predicates and final snapshots, INSERT SELECT sources, DELETE snapshots, and empty-result metadata. It verifies boolean preservation, scalar-index replacement, validation-failure preservation, explicit DELETE rollback and data/index restoration after a failing RETURNING qualifier.

The write parser now shares the existing bounded deep-path expansion before pinned AST parsing; UPDATE target normalization remains in place. No dependencies or upstream implementation files changed. This closes the initial write/RETURNING parsing gap for deep qualified paths within the supported query subset. Derived sources/CTEs, array subscripting, full expression/type propagation and release qualification remain open.


## Public expression column labels — 2026-09-06

A CLI probe reproduced internal deep-path markers in expression column names and accidental replacement of internal-looking substrings inside literal labels. Label rendering now recognizes function tokens and restores public dotted paths/namespaces without changing literal text.

`fastdb/scripts/check.sh` passed formatting/Clippy, all 119 Rust tests, fourteen Node tests and TypeScript declarations. A new collection SELECT regression verifies deep expression labels, literal text containing internal names or function-like syntax, namespace function labels, returned values and matching RETURNING labels. Repeating the CLI probe confirmed the corrected public path and unchanged literal label. No dependencies or upstream implementation files changed. Explicit aliases remain recommended for stable application labels; full result/type/compatibility and release qualification remain open.


## Scalar DISTINCT with typed results — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 122 Rust tests, fourteen Node tests and TypeScript declarations. Three new SELECT tests cover numeric/boolean equivalence, null deduplication, text versus numbers, typed record target/key identity, binary duplicates, explicit NOCASE collation, bound pagination, aggregate/group/window evaluation, INSERT SELECT, ordering positions and unsupported array comparison. A scalar projection/pagination query is compared with an ordinary relational table on the pinned engine. A volatile random() projection is verified to return strictly ordered distinct values when ordered by alias, position or identical expression.

Lowering places the source projection behind an inner LIMIT -1 to retain its evaluation boundary, groups scalar comparison keys in an outer query, and applies order/limit there. Projected ordering expressions reuse output values rather than evaluating them again. Each equivalence class retains an original typed representative, whose numeric/boolean representation is unspecified when equivalent inputs differ. Objects, arrays, vectors and fetched documents remain outside generic DISTINCT equality.

No dependencies or upstream implementation files changed. Temporary grouping/sorting storage, broader mixed-type/collation/volatile-expression coverage, performance and release qualification remain pending; full V1 remains incomplete.


## Collated and parenthesized ordering aliases — 2026-09-06

A CLI comparison reproduced a wrong ordering for SELECT v AS n ORDER BY n COLLATE BINARY when the collection also stored a different n field. ORDER BY now resolves aliases/positions through COLLATE and single-expression parentheses and preserves those wrappers during typed and DISTINCT lowering.

`fastdb/scripts/check.sh` passed formatting/Clippy, all 123 Rust tests, fourteen Node tests and TypeScript declarations. One new regression compares collated alias/position ordering with the pinned ordinary relational frontend, with and without DISTINCT, including alias/field name collisions. It also checks multi-column DISTINCT with a collection/relational cross join and explicit NOCASE collation. Existing volatile ordering tests remain green.

A separate probe still shows incorrect alias resolution for ORDER BY n+0 when n is also a stored field; arbitrary arithmetic/function alias references remain unfinished and must be addressed before V1. No dependencies or upstream implementation files changed.


## Ordering expressions over projected aliases — 2026-09-06

The previously recorded ORDER BY n+0 alias/field collision is fixed. ORDER alias bindings now point to projected values after source filtering/group/window lowering. DISTINCT keeps output-dependent ordering expressions outside grouping and carries independent source inputs through hidden inner projections, preserving reuse of volatile outputs. Aggregate/window operations over output aliases fail explicitly instead of introducing an unintended outer aggregate.

`fastdb/scripts/check.sh` passed formatting/Clippy, all 125 Rust tests, fourteen Node tests and TypeScript declarations. Two new tests compare arithmetic/function alias ordering against ordinary relational results with and without DISTINCT; verify ordering of a volatile projected random value through n+0; check typed record helpers, mixed collection/relational inputs, grouped aggregate aliases and invalid nested aggregates. Existing collation, pagination and volatile-expression checks remain green.

No dependencies or upstream implementation files changed. This addresses ordering aliases in the supported expression subset; WHERE/GROUP/HAVING alias rules, derived-source propagation, resource/performance and release qualification remain unfinished.


## HAVING aliases and structural GROUP BY guard — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 127 Rust tests, fourteen Node tests and TypeScript declarations. Two new grouping tests compare aggregate/scalar HAVING aliases with the pinned relational engine, including alias/source-field collisions, qualified source references, DISTINCT and INSERT SELECT. They also verify global aggregate HAVING, case-insensitive alias lookup, typed boolean/record helper inputs, fetched-value rejection and qualified GROUP BY fields whose names collide with a projection alias.

HAVING receives projected expression bindings after grouping-key lowering, and those bindings are cleared before window-source lowering. The GROUP BY guard now examines expression references through the pinned AST walker instead of matching all text tokens. Renamed unqualified GROUP BY aliases remain explicitly unsupported because their precedence against variable-schema collection fields still needs a settled contract; qualified source expressions remain available. No dependencies or upstream implementation files changed. Full V1 and broader query/resource/release qualification remain incomplete.


## Literal-aware fallback SQL guard — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 128 Rust tests, fourteen Node tests and TypeScript declarations. A new persistent-harness test verifies managed-looking text values in ordinary SELECT/functions, INSERT/UPDATE/DELETE, CTEs/compounds, scalar/derived subqueries, views and CREATE TABLE AS SELECT. It also rejects direct/nested/CTE single-quoted managed table references, protected PRAGMAs, managed DELETE and multiple statements, then confirms collection writes still work.

The fallback guard redacts value literals in a parsed guard-only copy and scans that representation. Native preparation/execution still receives the original SQL. Object names remain present, including identifiers written with single quotes. Unparsed statements and uncovered contexts retain conservative scanning; this is not a complete reference/dependency authorization boundary. No dependency or upstream implementation files changed. Full V1 compatibility, resource and release qualification remain incomplete.


## Schema-expression and UPSERT literals — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 129 Rust tests, fourteen Node tests and TypeScript declarations. A new regression verifies actual default values and CHECK enforcement, a partial index predicate, an added column default and an ordinary UPSERT whose expressions contain collection/internal/PRAGMA-looking text. It also rejects protected foreign-key targets, index targets, constraint names and rename targets, then confirms normal collection writes still work. A separate CLI smoke confirms FDB_UNSUPPORTED for all four protected schema-reference cases.

The guard-only AST traversal now visits CREATE/ALTER column value expressions, table CHECK/index expressions and chained UPSERT expressions without removing their identifier/reference names. The original SQL remains the sole executed representation. No dependencies or upstream implementation files changed. Trigger bodies, PRAGMA/table-function argument roles, broader dependency authorization and release qualification remain unfinished.


## Ordinary trigger value literals — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 130 Rust tests, fourteen Node tests and TypeScript declarations. A new persistent regression creates a native trigger with managed-looking literals in WHEN, INSERT, UPDATE, DELETE and SELECT/CASE expressions. It verifies actual audit values, explicit transaction rollback and trigger execution after close/reopen. A RAISE(ABORT) trigger preserves its original managed-looking error message and leaves no rejected row behind.

The same regression requires FDB_UNSUPPORTED for managed body targets/references across INSERT, UPDATE, DELETE and SELECT, a logical collection body reference, and a managed trigger target. The guard-only traversal changes value expressions while preserving names and reference roles; the original SQL is still prepared, stored and executed. No dependencies or upstream implementation files changed. PRAGMA/table-function argument roles, dependency authorization for pre-existing database objects and broader V1 release qualification remain unfinished.


## Initial interactive CLI — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 133 Rust tests, fourteen Node tests and TypeScript declarations. A new parser test covers complete versus open quotes/comments/delimiters, trigger bodies and unmatched delimiters. Two CLI subprocess tests cover multiline documents, duplicate-insert recovery with explicit rollback, transaction/continuation prompts, clearing unfinished input, quit behavior, multiline trigger creation and a trailing statement at EOF.

A real pseudo-terminal smoke verified automatic interactive mode and transaction prompts through BEGIN/SELECT/ROLLBACK/.quit. A persistent-file smoke verified .quit rollback of an active write and successful reopening through --script. Prompts are written to stderr; JSON results retain the batch execution contract.

The CLI adds a direct path dependency on the existing fastql-parser crate; Cargo.lock changes only that dependency edge. No new external dependency versions or upstream implementation files changed. History/editing, explicit signal handling, input/resource bounds and wider terminal/platform qualification remain open, alongside the rest of V1.


## Bounded CLI SQL input — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 135 Rust tests, fourteen Node tests and TypeScript declarations. Two new CLI subprocess tests verify exact UTF-8 byte boundaries, overflow inside a multibyte character, rejection of an oversized script before any statement report, fatal line overflow, accumulated interactive overflow and retained transaction observations. A separate persistent-file smoke confirms no execution of an oversized script prefix, rollback of an active interactive write on exit and preservation of a prior autocommit line write.

SQL input defaults to 16 MiB and can be changed with --max-input-bytes. Size checks precede UTF-8 decoding; a sentinel byte detects oversized input without reading the entire input into the SQL buffer. The CLI emits FDB_LIMIT and exits rather than processing the remaining tail. Import/migration limits remain independent. No dependencies or upstream implementation files changed. These are input byte-length bounds, not total allocation, result, deadline or SDK resource limits; broader V1 qualification remains open.


## Multi-connection isolation and busy error categories — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 137 Rust tests, fifteen Node tests and TypeScript declarations. Two new persistent Rust tests verify uncommitted document/index invisibility, retained read snapshots across writer commit, stale-snapshot write rejection, successful rollback/retry, contending index builds under both winner commit and rollback, uniqueness preservation and close/reopen consistency. A new Node test opens two Database instances on the same file and verifies lock contention, stale-snapshot failure, transaction observations and committed-value preservation.

Native Busy and BusySnapshot now map to FDB_BUSY and FDB_BUSY_SNAPSHOT rather than FDB_ENGINE. Tests exercise the actual variants through the engine and Node error envelope. No automatic retry is added. No dependencies or upstream implementation files changed. These deterministic overlaps establish targeted in-process evidence, not broad threaded/cross-process stress, interrupted commit/checkpoint or full release qualification.


## Native constraint error category — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 139 Rust tests, fifteen Node tests and TypeScript declarations. Two new Rust tests cover primary-key, unique, NOT NULL, CHECK, foreign-key and trigger-raise failures, preservation of earlier outer-transaction work under default abort behavior, and partial row retention under ordinary INSERT OR FAIL. Existing Node and CLI tests now verify FDB_CONSTRAINT through their public error envelopes; frontend validation remains FDB_VALIDATION.

The mapping uses native error variants and preserves engine messages and transaction observations. The native Constraint variant includes some runtime validation errors; the category specifies neither a finer constraint subtype nor rollback scope. No upstream files or dependencies changed. Full V1 error/result, concurrency, recovery and release qualification remain pending.


## Parenthesized and collated grouping ordinals — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 141 Rust tests, fifteen Node tests and TypeScript declarations. Two new grouping tests first reproduced distinct typed encodings separating equivalent numeric values under GROUP BY (2), and ignored text collation under GROUP BY 1 COLLATE NOCASE. The corrected frontend substitutes the original scalar expression through the pinned engine's supported ordinal wrappers, retaining collation and protecting projected constants from a second ordinal interpretation.

Differential relational-engine checks cover numeric/boolean equality, explicit and projected collation, and projection/source alias collisions. Tests also cover invalid positions, non-ordinal constant expressions and grouped INSERT SELECT. The rule intentionally mirrors the pinned engine's single-sign numeric handling rather than evaluating arbitrary constant arithmetic as an ordinal. Named grouping aliases and broader expression/derived-source qualification remain pending. No dependencies or upstream files changed; this does not establish full V1 completion.


## Checkpointed offline backup and restore — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 142 Rust tests, fifteen Node tests and TypeScript declarations. A new persistent integration test performs a successful truncating checkpoint, verifies the WAL is empty, closes all source handles, copies the main file, advances the source independently, and restores the backup into a fresh directory. It checks native integrity plus typed document values/binary64 bits, relational rows/views, logical metadata, required/CHECK/unique validation, nested index lookup and rollback, exact migration history, new committed writes and close/reopen. The backup bytes remain unchanged by restoration and subsequent writes.

The tested maintenance procedure is documented in backup-restore.md and linked from the prototype README. This is same-build, exclusive-access Linux evidence, not an online backup implementation or interrupted-copy/checkpoint/commit, power-loss, previous-release upgrade or cross-platform certification. No upstream files, dependencies or runtime APIs changed. Remaining V1 release gates stay open.


## Numeric ORDER BY recognition — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 143 Rust tests, fifteen Node tests and TypeScript declarations. A new differential SELECT regression first reproduced wrong ascending output for ORDER BY +(1),v DESC, where the pinned engine treats +(1) as constant and orders by v descending. The frontend previously recursively interpreted signed/parenthesized constants as positions. ORDER BY and GROUP BY now share numeric recognition that matches the pinned engine's wrapper, single-sign and platform-sized integer rules.

Coverage includes ordinary and DISTINCT collection ordering, parenthesized/collated positions, nested-sign and arithmetic constants, and invalid positions including integers above signed-int64 range. Recognized invalid positions consistently return FDB_VALIDATION in collection lowering; ordinary relational error delegation is unchanged. All existing grouping tests pass. No upstream files or dependencies changed; broader expression/type/alias and V1 qualification remains pending.


## Initial typed record range comparisons — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 145 Rust tests, fifteen Node tests and TypeScript declarations. The previous CLI build reproduced a missing result for a stored docs:2 reference under ref < docs:10 because serialized record bytes ordered the keys lexically. A typed comparator now handles SQL range operators when both operands retain FastDB values, using numeric integer keys and case-folded target names. Non-record operands use the pinned engine scalar ordering after the existing index-scalar conversion.

Two new tests cover negative/positive record keys, constructors, parentheses, bound typed values, null/mixed-type behavior, UPDATE/DELETE/RETURNING and rollback, plus differential numeric/null/text comparisons with affinity-free relational columns. The callback performs no database work. No persisted formats, indexes, dependencies or upstream files changed. Untyped expression propagation, explicit collation, BETWEEN, CHECK and mixed relational affinity remain release work; full V1 is incomplete.


## Typed BETWEEN with single operand evaluation — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 147 Rust tests, fifteen Node tests and TypeScript declarations. Typed BETWEEN/NOT BETWEEN now share the range comparator when all three operands retain FastDB values. An integration test verifies inclusive record ranges, NOT, DELETE/RETURNING rollback, and differential numeric/text/null truth tables against the pinned relational engine. A frontend test registers a test-only volatile counter and verifies that each BETWEEN form evaluates its left operand exactly once.

The callback decodes each operand once and combines comparisons with SQL three-valued AND; native NOT handles negation. It performs no database work. Existing scalar comparison behavior and persisted formats are unchanged. Mixed/unpreserved operands, explicit collation, CHECK propagation and wider V1 qualification remain pending. No dependencies or upstream files changed.


## Candidate-field record range validation — 2026-09-06

`fastdb/scripts/check.sh` passed formatting/Clippy, all 148 Rust tests, fifteen Node tests and TypeScript declarations. CHECK range operators between two candidate fields and BETWEEN/NOT BETWEEN with three candidate fields now bind typed values and use the shared comparator. A persistent test defines the constraint over existing numeric record keys, rejects object/SQL inserts and updates plus UPSERT, verifies active transaction retention for false CHECKs, rollback/index consistency, and continued enforcement and index maintenance after reopen.

The CHECK function allowlist is unchanged: internal comparator calls are introduced only after identifying direct field operands. Candidate bindings read no stored rows. Catalog format stays version 2, and the contract explains that affected prototype data must be revalidated before relying on corrected record-range semantics. Non-field expressions, explicit collation, broader type propagation and full V1 release qualification remain pending. No dependencies or upstream files changed.


## Deep candidate CHECK paths — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 149 Rust tests, fifteen Node tests and TypeScript declarations. A CLI probe reproduced a syntax error for a four-segment candidate path in CHECK. The shared path expander now feeds CHECK parsing, and generated markers resolve to candidate bindings while retaining quoted segments and literal text. Explicit internal marker calls and overlong paths are rejected without widening CHECK function eligibility.

A persistent regression verifies definition over existing data, quoted dot-containing segments, unchanged string literals, failed SQL/object updates, transaction/index preservation, and enforcement and index maintenance after reopening. Preliminary boolean CHECK probes showed correct behavior, so no boolean handling changed. No stored format, dependency or upstream file changed. Broader expression/type propagation and V1 release qualification remain pending.


## Relational schema and index inspection — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 150 Rust tests, fifteen Node tests and TypeScript declarations. INFO FOR DB now lists views separately; relational table/view INFO adds kind, native table_xinfo columns and index_list entries; relational index INFO adds index_xinfo key metadata. The persistent regression verifies defaults/nullability/hidden flags, native unique and partial indexes, expression keys with descending order/collation, view discovery, managed-name hiding and index drop/rollback visibility after reopening.

These additions preserve native metadata values rather than claiming portable expression-column numbering or enabling experimental features. The README and inspection contract describe the prototype shape and reserved-name behavior for autoindexes. No dependencies or upstream files changed. Wider dependency authorization, inspection protocol stabilization and remaining V1 release work remain pending.


## Managed membership index planning — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 152 Rust tests, fifteen Node tests and TypeScript declarations. The leading collection can now use a managed index for positive constant IN lists, including duplicate/null/parameter/record keys. Parentheses no longer hide eligible equality or membership predicates, and the original WHERE remains outside the filtered source to enforce residual conditions.

Two regressions compare scan results with indexed results and assert actual SEARCH plans. They cover conjunction residuals, numeric equivalents, typed record target/key identity, duplicate keys, parameter binding and delete/rollback. NOT IN, OR and empty lists retain their existing planning and results. No dependency, persisted format or upstream file changed. Cost-based/multi-index/range/outer-join planning and broader resource/performance qualification remain pending.


## Null-predicate managed candidates — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 153 Rust tests, fifteen Node tests and TypeScript declarations. Single-source IS NULL/IS NOT NULL predicates now filter managed-index entries before document ID lookup; postfix and reversed literal-null forms share that path. A regression compares results before and after index creation for missing/null/non-null fields, equality-to-null, residual predicates, update/index maintenance and delete/rollback. It also verifies a right join whose results would change under unsafe null-accepting pushdown; joined sources retain their existing planning.

A native-table probe showed that pinned Turso scans rather than seeks for null predicates even with a native index. Tests therefore verify the compact index-entry scan plus document-ID lookup without claiming a null-key seek or measured speedup. Cost selection and performance qualification remain pending. No persisted format, dependency or upstream file changed; full V1 is incomplete.


## Native Node tarball installation — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 153 Rust tests, fifteen Node tests and TypeScript declarations; npm ci also passed offline. The initial npm pack inventory omitted fastdb.node because of the gitignore fallback. An explicit runtime allowlist now includes the addon, worker, entry point, declarations, README and MIT license, excluding Rust sources and development tests. Normal packing checks that the addon loads first; a missing-addon fixture fails without producing an archive.

`node fastdb/scripts/check-node-package.cjs` passed against the final package manifest on Linux x64/Node 24.19.0. It packed seven files (57,929,232 compressed bytes for this debug build), installed offline into a fresh consumer outside the checkout with lifecycle scripts disabled, exercised synchronous and worker operations with int64/record values, rollback and persistence, and compiled imports from installed declarations. The temporary tarball and consumer were removed afterward. This heavier smoke is a maintainer check rather than routine CI.

The package remains private and unpublished. Platform/version coverage, prebuild selection, optimized artifact sizing and complete distribution notices remain release gates. The npm lockfile change records license metadata only; dependency versions and upstream files are unchanged. Full V1 remains incomplete.


## Standalone Rust path consumer — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 153 Rust tests, fifteen Node tests and TypeScript declarations. Separately, check-rust-client.py built and ran an offline consumer in a temporary workspace outside the checkout using Rust 1.88.0 on Linux x64. The fresh dedicated build took 1 minute 42 seconds. The consumer exercised typed record/int64 parameters, CHECK validation, unique index lookup/maintenance, transaction observations, rollback, bundled QuickJS, vector values and persistent reopen.

The smoke seeded the consumer lockfile from the pinned workspace lockfile, allowed Cargo to add/prune consumer entries, and verified all 240 resolved registry/git package identities/checksums against the baseline before its locked build. Host-filtered metadata avoids fetching irrelevant platform dependencies. General Rust flag overrides were removed and the checkout's .cargo configuration was outside the consumer's search path. Temporary consumer files were cleaned up; the separate build cache remains under target/fastdb-rust-consumer.

The Rust client guide documents the tested local-path workflow and the application's responsibility to retain its own lockfile. This is not registry package publication, arbitrary dependency-unification or cross-platform qualification. No dependency versions, workspace manifests/lockfile or upstream files changed. Full V1 release work remains pending.


## Binary literals in equality and membership — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 154 Rust tests, fifteen Node tests and TypeScript declarations. A CLI probe reproduced a stored X'0102' value comparing unequal to the same literal. Blob literals in equality/IS and membership comparisons with preserved typed operands now use the same binary scalar-key representation as fields and parameters. Managed equality/IN candidates apply the same conversion.

The regression covers indexed/unindexed results, actual SEARCH plans, reversed and parenthesized literals, inequality/null behavior, typed parameters, DELETE/RETURNING rollback and binary bytes that imitate record encoding without acquiring record identity. No stored format, dependency or upstream file changed. The probe also identified a separate pending scalar-function issue (length of a stored binary value sees its encoded representation); broader binary function/range propagation and full V1 qualification remain incomplete.


## Binary payloads in native functions and casts — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 155 Rust tests, fifteen Node tests and TypeScript declarations. Preserved values passed into native function arguments or CAST now expose binary payload bytes through a dedicated scalar conversion; internal frontend helper protocols and predicate/index representations remain separate. The original CLI probe now reports length 2 and equality true for stored X'0102', replacing encoded length 35 and the previously corrected equality failure.

A differential regression covers length/hex/typeof/substr, text/integer casts, nested and null-helper composition, empty/null values and typed parameters against native BLOB columns. Indexed equality, binary UPDATE/RETURNING output and rollback/index maintenance also pass. No dependency, persisted encoding or upstream file changed. CHECK argument handling, broader expression/range/aggregate/window qualification and full V1 release work remain pending.


## Binary payloads in candidate CHECK arguments — 2026-09-07

`fastdb/scripts/check.sh` passed formatting/Clippy, all 156 Rust tests, fifteen Node tests and TypeScript declarations. A CLI probe reproduced length(payload)=2 rejecting a two-byte binary payload. CHECK lowering now separates key, typed and native-scalar field bindings, using payload bytes for allowed native function/CAST arguments and propagating that argument context through CASE result branches, parentheses and built-in collation.

The persistent regression adds the constraint over existing binary data, checks length/substr/coalesce/CASE/collation/casts, rejects invalid INSERT/UPDATE/UPSERT while preserving the tested transaction and index state, and verifies enforcement after reopening. The deterministic function allowlist and stored format remain unchanged. Affected prototype definitions/data need revalidation; broader binary comparison/arithmetic and full V1 qualification remain pending. No dependencies or upstream files changed.

## Collection sources for native INSERT targets — 2026-09-07

`fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied, all 179 Rust tests, fifteen Node tests and strict TypeScript declarations. Two new real-engine tests cover collection sources inserted into ordinary relational targets: binary payloads, duplicate projection names, grouping, DISTINCT, record-key extraction, target triggers, UPSERT, RETURNING, named parameters, rollback and close/reopen. They distinguish native ABORT from FAIL partial progress and verify IGNORE/REPLACE behavior. Internal table/function names are rejected even with single quotes, while managed-looking value literals remain allowed.

The source is lowered into a single native INSERT statement, preserving native target constraints, affinity and conflict dispositions. Records retain encoded identity unless explicitly reduced with record::id; composite values have no implicit scalar conversion. Fetched projections and broader collection CTE/compound/derived sources remain unsupported. No upstream source or dependency changes were made. This is local Linux evidence, not complete V1 qualification.

## Projection aliases in collection join predicates — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 180 Rust tests, fifteen Node tests and strict TypeScript declarations. A new real-engine regression compares collection JOIN ON aliases with explicit relational source expressions across inner/left joins, computed/constant aliases, case-insensitive references and unmatched rows. It also verifies qualified stored-field collisions, typed binary and record comparisons, aggregate/fetched-alias rejection, unqualified-field rejection, and native-target INSERT SELECT rollback.

The pinned ordinary relational engine rejects the tested projection-alias reference in ON; that rejection remains unchanged and has an explicit assertion. Collection JOIN ON follows the existing collection WHERE/GROUP BY source-expression substitution rule. This does not guarantee materialized volatile alias values, change join ordering, add outer-join index pushdown or complete derived-query support. No upstream files or dependencies changed; full V1 release qualification remains open.

## Quoted internal names in collection operations — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 181 Rust tests, fifteen Node tests and strict TypeScript declarations. A new real-engine regression verifies FDB_UNSUPPORTED for single-quoted internal calls in SELECT projections, predicates and ordering; collection VALUES/SELECT inserts; UPDATE/DELETE; and SQL-shaped/object-write RETURNING. It covers mixed-case names and intervening comments. Each rejected write leaves the original document intact, including object operations whose RETURNING guard runs within the existing savepoint.

Ordinary managed-looking string values still work alongside fixed record IDs and namespaced helpers. The shared guard screens original user tokens and leaves generated helper calls to the trusted lowering paths. Catalog source checks and native SQL guards remain separate. This does not establish complete authorization of stored-object dependencies or full V1 release qualification. No upstream files or dependencies changed.

## Mixed native-column equality — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 182 Rust tests, fifteen Node tests and strict TypeScript declarations. A new differential real-engine regression checks native BLOB/document Binary equality and inequality in both operand orders, IS/IS NOT with NULL, numeric-column affinity against document text, declared NOCASE and explicit COLLATE precedence, and unmatched left-join rows. It verifies matching binary INSERT SELECT results and transaction rollback.

A separate assertion in the same test supplies serialized record bytes through an ordinary BLOB column: those bytes match a document Binary containing identical bytes and do not match the typed record. Native BLOB values use comparison keys; nonbinary branches retain column affinity. A collation regression exposed during testing was corrected by visiting the native scalar column first when doing so does not change explicit COLLATE precedence. Mixed ranges/membership, broader alias/derived expressions and planner qualification remain open. No upstream files or dependencies changed; this does not complete V1 qualification.

## Mixed native membership and alias regression coverage — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 184 Rust tests, fifteen Node tests and strict TypeScript declarations. A new differential real-engine test checks mixed IN/NOT IN across native BLOB and collection values, both operand directions, NULL-containing and empty lists, native integer affinity against document text, declared and explicit collation, and HAVING membership aliases.

A second new regression confirms existing native column projection aliases in mixed HAVING queries across stored-field collisions, binary equality, numeric affinity and collation; this test passed before the membership change and required no alias implementation change. Native-column membership now chooses binary keys only for BLOB values and retains the native column reference for scalar affinity. Mixed ranges, subquery membership, broader derived propagation and planner/resource qualification remain open. No upstream files or dependencies changed; full V1 qualification remains incomplete.

## Process kills near repeated commits and checkpoints — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 186 Rust tests, fifteen Node tests and strict TypeScript declarations. The new subprocess harness and helper run six fresh-database scenarios, killing the writer zero, one or five milliseconds after a flushed marker preceding either COMMIT or TRUNCATE checkpoint. Each transaction contains twelve typed documents with 2 KiB binary payloads, unique index entries, corresponding relational audit rows and a relational version update.

Two consecutive reopens per scenario verify native integrity, retention of all acknowledged commits, no more than one additional unacknowledged commit, contiguous complete batches, exact typed payloads, all expected index entries and absence of every document/index entry in the next batch. A guard kills/reaps children on failure; phase waits are bounded to thirty seconds. The targeted six-scenario run passed in 3.86 seconds before strengthening the next-batch absence checks; the strengthened harness also passed the full scoped run.

Phase markers do not prove termination inside a particular engine instruction or filesystem operation. These results establish bounded local Linux process-failure evidence around commit/checkpoint activity, not deterministic mid-I/O fault injection, power-loss, cross-platform, previous-version or full release qualification. No production implementation, upstream files or dependencies changed.

## Crash recovery of replacement and deleted index entries — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 186 Rust tests, fifteen Node tests and strict TypeScript declarations. The existing crash test now runs nine scenarios: zero-, one- and five-millisecond kill delays after prior-batch rewrites, before COMMIT, and before TRUNCATE checkpoint. Each transaction updates half of the previous batch's documents (unique indexed name and binary payload), deletes the other half, mirrors those changes in a native live table, and inserts a new batch while retaining complete relational audit history.

Two reopens verify the recovered version against acknowledgements, exact surviving document/payload and relational state, current index entries, and absence of stale, deleted and uncommitted index entries. The targeted nine-scenario run passed in 8.92 seconds; the full scoped run passed afterward. The test count is unchanged because this expands the existing test and helper. No production code, dependencies or upstream files changed. Process-kill timing remains a call-window probe, not deterministic mid-I/O or power-loss certification; broader release qualification remains open.

## Controlled catalog and replacement-index cancellation — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 188 Rust tests, fifteen Node tests and strict TypeScript declarations. Two new frontend unit tests use the existing one-shot engine progress hook, armed to interrupt only after total_changes advances. They cover DROP INDEX, collection DROP TABLE and DEFINE FIELD OVERWRITE in autocommit and explicit transactions, verifying cancellation, restored logical metadata and exact physical schema, required integer validation, existing index lookups, preservation of prior transaction work, successful retry and explicit rollback.

The second test cancels a CREATE INDEX after an earlier DROP INDEX in the same transaction. The failed CREATE leaves the earlier DROP pending; outer ROLLBACK restores the original index and lookup path. A retry followed by COMMIT installs the replacement path. The targeted four-test interruption module passed before the full scoped run. These tests cover specific observed interruption points in the pinned engine; they do not promise outer-transaction preservation after every engine error or complete persistent fault-injection qualification. No production implementation, dependencies or upstream files changed.

## Catalog identity and structural corruption checks — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 189 Rust tests, fifteen Node tests and strict TypeScript declarations. A new frontend regression injects malformed JSON and structurally invalid entries directly through the test-only engine access. Cases include collection-name/storage mismatch, redirected native storage, empty/reserved/duplicate fields, invalid record targets and vector dimensions, incompatible indexed types, and duplicate/noncanonical/misdirected index identities.

Both direct lookup and catalog enumeration reject the entries with FDB_STORAGE. Attempted collection DROP is rejected before it can affect an unrelated native sentinel table. Restoring valid metadata recovers existing index access and validation; valid implicit version-1 metadata remains readable. The Rust API now rejects invalid reference targets before persistence and retains case-insensitive valid targets. These checks do not authenticate externally modified metadata, verify every physical table layout/index entry or repair damaged databases. No upstream files or dependencies changed; full V1 qualification remains open.

## Statement delimiter-depth preflight — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 191 Rust tests, fifteen Node tests and strict TypeScript declarations. A new parser regression accepts 64 nested delimiters at dispatch, rejects the 65th at its exact byte offset, rejects 10,000 nested parentheses, and excludes quoted strings/identifiers/comments from depth counting. A new real-engine transaction regression rejects deeply nested native INSERT, collection INSERT RETURNING, UPDATE and SELECT before mutation, preserves earlier outer-transaction work, permits a later valid write and verifies final rollback.

The initial 64-delimiter bound applies to input before FastQL/native SQL dispatch. It does not establish safe engine execution for all inputs below the bound or cover flat operator chains, delimiter-free CASE nesting, direct API CHECK strings, generated AST expansion, tokenization allocation or result budgets. The master syntax proposal and contracts now state this limit and its scope. No upstream files or dependencies changed; full V1 resource qualification remains open.

## Direct and persisted CHECK delimiter limits — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 192 Rust tests, fifteen Node tests and strict TypeScript declarations. A new real-engine test submits direct Rust field overwrites containing 65 and 10,000 nested parentheses, verifies FDB_SYNTAX before metadata changes, preserves the existing CHECK and indexed document in an outer transaction, permits a subsequent valid write and verifies explicit rollback. The existing metadata-corruption test now also rejects a stored CHECK with 128 nested parentheses through direct/enumerated catalog access and attempted DROP.

The delimiter validator is shared with statement preflight. Direct CHECK definitions use it before expression parsing; stored metadata uses it before write-time CHECK parsing can occur. Generated expressions do not enter the user-input guard. Flat expression chains, CASE nesting, generated AST depth and allocation/result budgets still need qualification. No upstream files or dependencies changed; full V1 remains incomplete.

## Separate SELECT lowering from execution — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 193 Rust tests, fifteen Node tests and strict TypeScript declarations. Collection lowering now produces an internal command/output-metadata plan; execution separately prepares and binds it, decodes results and performs fetch expansion. Native-target INSERT SELECT shares the execution stage with its native binding/result/affected-row behavior retained. The existing fetch savepoint wrapper continues to cover both stages.

A new frontend unit test checks Boolean/Binary/Record output metadata and exact values, and proves that a native INSERT plan leaves its target empty until executed, then returns native RETURNING values and affected count. Existing query, grouping, window, fetch snapshot, INSERT conflict/trigger, RETURNING, cancellation and crash-recovery regressions pass unchanged. This refactoring establishes a lowering boundary needed for nested queries; CTE/derived/subquery support is still unfinished. Plans can embed typed parameters and are not a reusable public prepared-statement contract. No upstream files or dependencies changed; full V1 remains incomplete.

## Typed collection derived-table sources — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 196 Rust tests, fifteen Node tests and strict TypeScript declarations. Three new integration tests cover aliased collection FROM subqueries: Boolean/Binary/Record/Object/Array/vector propagation, object paths, nested star expansion, native/helper expressions, empty result metadata, named and positional parameters, grouping, DISTINCT, pagination and both sides of left joins. A top-level forward fetch through a derived record column matches the direct collection query.

Write-source tests verify collection CHECK failure leaves no partial rows and native INSERT SELECT RETURNING with inner parameters rolls back explicitly. Negative cases retain rejection of duplicate derived names, fetched inner projections, managed function/table access and missing columns. Native-only derived queries retain delegation. A positional column-name wrapper fixes the private-alias mismatch exposed by the first DISTINCT regression without changing inner ordering references. The final full run passed after correcting a test's doc::get path to the established '$.city' syntax.

Inner plans remain engine queries, with logical column metadata and consumed parameters propagated to the outer lowering stage. This does not add user CTEs, compounds, scalar/correlated queries, unaliased collection sources, a materialization guarantee or full nested planner/resource qualification. No upstream files or dependencies changed; full V1 remains incomplete.

## Nonrecursive typed collection CTEs — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 199 Rust tests, fifteen Node tests and strict TypeScript declarations. Three new integration tests cover typed CTE chains, explicit column lists, grouping/DISTINCT, parameters, empty metadata, native CTE joins, nested scope, collection-name shadowing with main-qualified access, and top-level forward fetch. MATERIALIZED random projections retain shared values across multiple references; deterministic NOT MATERIALIZED results also pass.

Source-form collection/native INSERT tests cover validation rollback, inner parameters, native RETURNING and explicit rollback. Negative tests cover column mismatches/duplicates, fetched inner projections, forward/recursive collection references and managed calls. The native recursive-CTE test preserves the pinned engine's existing rejection. A shared Binary parameter regression verifies raw payloads in a native CTE and encoded keys in a collection index predicate. Native metadata-probe fallback is restricted to parse errors so other engine failures propagate.

CTE bodies remain engine definitions with their materialization hints. Earlier-definition references are typed; forward references, recursive collection CTEs, compounds, scalar/correlated queries, leading-WITH writes and broader planner/resource qualification remain unfinished or unsupported. No upstream files or dependencies changed; full V1 remains incomplete.

## Leading WITH INSERT SELECT — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy with warnings denied, all 200 Rust tests, fifteen Node tests and strict TypeScript declarations. A new integration test covers leading nonrecursive WITH for collection and native INSERT SELECT, including typed RETURNING, inner parameters, validation-failure rollback, explicit rollback, native IGNORE and native-only delegation.

Leading definitions are lowered as the SELECT source's WITH clause. An AST-based check rejects subqueries and table-membership expressions outside that source on the managed-source native INSERT route; a same-named physical table regression proves those references cannot silently resolve differently and execute a write. Ordinary native-only statements retain their original SQL and scope. Existing source-form CTE and INSERT tests pass. Leading-WITH VALUES/UPDATE/DELETE, dual WITH scopes and broader cross-clause typing remain incomplete. No upstream files or dependencies changed; full V1 remains open.

## CTE and derived-source cancellation — 2026-09-07

The scoped check log records successful formatting, Clippy with warnings denied, all 201 Rust tests and fifteen Node tests, followed by strict TypeScript checking with no diagnostics. The run has finished; its original process exit status was not retained across session compaction. The targeted five interruption unit tests separately exited successfully. The final strict TypeScript step was rerun separately and exited successfully; subsequent Rust edits only improved test formatting.

The partial-write interruption matrix now includes leading-WITH and derived-table collection INSERT SELECT in autocommit and outer transactions. A new mixed native/collection CTE test interrupts reads and inserts at three early engine-operation counts. At those points, FDB_CANCELLED propagates, prior outer work survives, target documents and indexes remain empty, and retry and rollback succeed. These tests exercise selected deterministic points rather than proving cancellation at every lowering stage or a complete deadline contract. No production code, upstream files or dependencies changed; full V1 remains incomplete.

## Mixed native-column range ordering — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, 202 Rust tests, fifteen Node tests and strict TypeScript checking. The new regression first failed on document/native binary `<` because encoded document bytes were compared with raw native payloads. Lowering now converts the logical operand to a raw scalar while keeping the ordinary column's affinity/collation and rejecting non-null record/scalar ordering.

All four range operators are checked in both operand orders against native SQL for empty and nonempty binary payloads, NULLs, numeric affinity and text collation, including native parentheses/unary plus/COLLATE. The text oracle explicitly chooses the native column's NOCASE collation because its physical baseline counterpart otherwise introduces an implicit BINARY collation absent from collection fields. Left joins, typed derived/CTE sources and native INSERT SELECT followed by rollback also pass. Mixed BETWEEN, arbitrary native expressions and broader logical-operand wrappers remain unqualified. No upstream files or dependencies changed; full V1 remains incomplete.

## Native-column BETWEEN document bounds — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, 203 Rust tests, fifteen Node tests and strict TypeScript checking. The new differential test first failed on native BLOB BETWEEN document binary bounds. Lowering now preserves the native BETWEEN node and converts either/both typed bounds to raw scalars with NULL-aware record/scalar rejection. Native column affinity, collation, parentheses and unary plus retain their roles.

The test checks BETWEEN/NOT BETWEEN, empty and nonempty bytes, reversed and NULL bounds, numeric affinity, NOCASE and explicit BINARY collation, a left join, CTE bounds, and a native INSERT target CHECK failure with no partial rows and subsequent rollback. The existing controlled counter test now also verifies each typed bound evaluates once when the left operand is a stored native integer column. Logical left operands with native bounds, broader typed-bound wrappers and volatile expressions behind native views/derived columns remain unqualified; the conversion reads the native column again for NULL-aware validation. No upstream files or dependencies changed; full V1 remains incomplete.

## CLI terminal editing and history — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, 205 Rust tests, fifteen Node tests and strict TypeScript checking. The CLI now uses already-pinned Rustyline 15.0.0 for Unix terminal editing and in-memory history, with optional explicit file persistence. Cargo.lock adds only the CLI's dependency edge; no dependency versions or upstream implementation files changed.

A Python 3 controlling-PTY harness checks cursor deletion, up-arrow repeat, multiline recall, Ctrl-C clearing unfinished input while preserving an active transaction, history reload across processes, omission of cleared/leading-space entries, private new file permissions, Ctrl-D, input-limit rejection and parseable JSON-only stdout. Existing piped interactive, script, transaction, migration, transfer and output-error tests pass. A new unit test rejects database/main-WAL-SHM history path collisions and existing Unix hardlink/symlink aliases without changing a sentinel file.

Verification is on Linux. Editing requires terminal stdin/stderr and /dev/tty; other sessions retain plain input. The submission limit does not bound the live editor buffer. Running-query signal cancellation, other terminals/platforms, concurrent history merging and full resource qualification remain open. History is saved on normal session return, not guaranteed after process or I/O failure. Full V1 remains incomplete.

## Interactive CLI running-query cancellation — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, all 205 Rust tests, fifteen Node tests and strict TypeScript checking. The existing real-terminal harness was extended to issue Ctrl-C after a flushed batch marker and through possible engine prepare windows. Against the previous CLI binary it failed with process exit by SIGINT (-2); against the new listener it reports FDB_CANCELLED, skips the later SELECT in the batch and returns to the prompt.

The harness cancels an aggregate read inside an outer transaction and verifies prior native rows and active state survive at the tested point, then rolls back. It also cancels a large native INSERT in autocommit and verifies zero partial rows. Both cases permit a subsequent query; session shutdown joins the signal worker and retains nonzero exit status after statement failure. Earlier editing/history/prompt interruption tests remain passing.

The Unix listener uses signal-hook 0.3.18 already pinned in Cargo.lock and forwards SIGINT to the weak interrupt handle from a regular thread. Only the CLI dependency edge was added; upstream source and dependency versions are unchanged. Nonterminal signals, request-ID-specific delivery, complete parsing/output/cleanup deadlines and broader platform/race qualification remain open. Full V1 remains incomplete.

## Local document/vector benchmark harness — 2026-09-07

The new Python harness completed a final 1,000-document, three-sample smoke and a 100,000-document, two-sample run against the existing dev CLI binary at d732de4bd. Both checked result counts, index plan use, nearest-vector identity and distance ordering. Raw reports under benchmark-results retain exact samples, query plans, memory/disk measurements, binary hashes and source state. The script also passed Python parsing and its help command. Only Python and documentation changed, so the unchanged Rust/Node suite was not rerun.

At 100,000 synthetic 16-dimensional vectors, medians were 5,844.4 ms unindexed filtering, 321.2 ms indexed filtering and 29,416.9 ms exact top-10. Load/index construction took 310.6/98.0 seconds, and final Linux process peak RSS was 177,102,848 bytes. These are unoptimized CLI round trips with warmups and few samples; the fixture's repeated synthetic embeddings are explicitly documented. No production-speed or tail-latency guarantee follows. Frontend scan-counter exposure, representative high-dimensional data, optimized/cold/concurrent qualification and million-vector evidence remain open. Full V1 remains incomplete.

## Primary SELECT engine counters — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, 208 Rust tests, fifteen Node tests and strict TypeScript checking. Two new Rust integration tests verify identical typed results before/after profiling, measured scan/read reduction through a managed index, reset counters on repeated calls, CTE execution, native parameters and rejection of profiled writes/multiple statements/managed calls/FETCH. An initial test accidentally used FETCH as an ordinary column/alias; it was corrected to the actual record::fetch function before the final run. A new CLI test verifies .profile output and write rejection in script, line and interactive modes.

The final instrumented 1,000-document benchmark completed three samples per workload with identical counters: 1,000 physical rows read and 999 fullscan steps unindexed; 20 rows read, zero fullscan steps and 31 seeks indexed; 1,000 reads, 999 fullscan steps and one sort for exact vector top-10. The raw report is retained under benchmark-results. Counters measure the primary engine statement only, not catalog/lowering helper work, Rust decoding or transport. Errors do not return partial counters; FETCH and Node profiling remain unsupported. The existing 100k artifact still has no measured counters. No upstream files or dependencies changed; full V1 remains incomplete.

## Node synchronous/async SELECT profiling — 2026-09-07

The scoped run passed formatting, Clippy with warnings denied and all 208 Rust tests. Its new Node test initially expected a plain Uint8Array where the established decoder returns Buffer; only that expectation was corrected. `fastdb/scripts/check-node.sh` then exited successfully with all sixteen Node tests and strict TypeScript checking.

Both Database.profileSelect and AsyncDatabase.profileSelect preserve typed Record/Binary/Boolean results and parameter encoding, return transaction observations, and expose camelCase bigint counters transported as decimal strings. The test verifies measured scan/read reduction with a managed index, counter reset, native binary parameter results, write rejection inside an active transaction, retained documents and closed-connection rejection. Async profiling uses the existing worker allowlist, ordered bounded queue and interruption machinery. Type checks cover synchronous and Promise results and bigint metric fields.

This adds no partial error metrics, FETCH profiling, deadline or request-specific cancellation contract. No upstream files or dependencies changed; full V1 remains incomplete.

## Managed physical schema validation on connect — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, 211 Rust tests, sixteen Node tests and strict TypeScript checking. New connections validate catalog metadata and referenced physical table/index DDL in a single snapshot before exposure. Missing objects, changed columns/uniqueness and unexpected explicit indexes/triggers return FDB_STORAGE. DDL comparison is lexical, avoiding recursive parsing of modified stored schema SQL, and accounts for the pinned engine's quoted key identifier.

Two new unit tests cover healthy reconnect, eight schema/dependency mutations, retained sentinel rows, changed uniqueness and successful reconnect after restoration. A persistent integration test modifies the physical collection with the raw engine and verifies rejection on reconnect after reopening. Existing legacy catalog, migration, backup/recovery, concurrent-connection and client tests continue to pass. Validation is not repeated on each operation and does not scan data/index contents, detect all orphan objects or prevent later out-of-band modification. No upstream files or dependencies changed; full V1 remains incomplete.

## Orphan storage and shared index ownership — 2026-09-07

The final `fastdb/scripts/check.sh` run exited successfully: scoped formatting, Clippy with warnings denied, 214 Rust tests, sixteen Node tests and strict TypeScript checking. Connection validation now inventories reserved collection/index storage names and rejects entries absent from catalog metadata, as well as index storage referenced by multiple collections. Validation remains inside the existing connection-creation snapshot.

Two new unit tests cover removed collection/index metadata, orphan reserved tables/views and uppercase prefixes, retained physical document rows after rejection, shared index ownership and successful reconnect after restoration. A persistent integration test drops the catalog through the raw engine and verifies that reopening/recreating the empty catalog cannot hide surviving collection storage. The earlier orphan-only scoped run passed too; the final run includes the additional ownership check. No metadata reconstruction or data deletion is performed by validation. Arbitrary renamed objects, content corruption and changes after connection creation remain outside this check. No upstream files or dependencies changed; full V1 remains incomplete.

## Explicit bounded collection content audit — 2026-09-07

The final `fastdb/scripts/check.sh` run exited successfully: scoped formatting, Clippy with warnings denied, 219 Rust tests, sixteen Node tests and strict TypeScript checking. Rust check_collection_integrity validates managed schema, streams physical ID/document rows, checks canonical typed IDs and field/CHECK validity, verifies exactly one expected index entry per document and checks total entry counts. Processed documents and encoded ID/document bytes have configurable limits; no repair is performed.

Four new unit tests cover valid counts, NULL/missing index keys, outer-transaction retention after limit failures, invalid encoding/IDs/types/CHECK values, missing/stale/duplicate/extra entries, and preservation of engine cancellation during CHECK evaluation. A persistent vector/binary integration test verifies reopening and exact byte-limit boundaries. The first full run used an earlier fixture with a NULL parent on an indexed nested path; the fixture was corrected to the existing missing-parent semantics and formatting was refreshed before the successful final run.

The audit retains one document at a time but does not cap engine allocations or elapsed time. CHECK engine errors propagate for this audit without changing ordinary write CHECK handling. Native page/B-tree integrity, repair, full resource/cancellation qualification and CLI/Node audit bindings remain open. No upstream files or dependencies changed; full V1 remains incomplete.

## Node collection integrity audit — 2026-09-07

`fastdb/scripts/check.sh` exited successfully: scoped formatting, Clippy with warnings denied, 219 Rust tests, seventeen Node tests and strict TypeScript checking. Database and AsyncDatabase now expose checkCollectionIntegrity, with uint64-bounded bigint limits and bigint report counters transported as decimal strings. Omitted limits use the Rust defaults; async execution uses the existing bounded worker queue.

The new test checks both clients: empty collections with zero limits, document/index counts, exact encoded-byte limits, FDB_LIMIT while preserving outer transaction work, invalid limit objects/types/ranges, rollback, missing collections and closed-connection rejection. Type checks cover sync/async counters and reject numeric limits. Native failures retain transaction observations and no partial report is returned. No repair, hard deadline or physical B-tree verification is added; CLI audit commands and broader resource/recovery qualification remain open. No upstream files or dependencies changed; full V1 remains incomplete.

## Standalone CLI collection audit — 2026-09-07

The scoped check log records successful formatting, Clippy with warnings denied, 221 Rust tests and seventeen Node tests, followed by the TypeScript command with no errors. The original process handle was unavailable after context handoff; no check process remained. A separate strict TypeScript run exited zero to confirm that final stage. No production source changed after these checks.

Two new CLI subprocess tests cover a persistent collection with an index, count reports after reopening, exact document/byte limits, FDB_LIMIT without partial counts, retained documents after failure, missing-file rejection without creation, conflicting input modes and invalid unsigned limits. The standalone command uses the existing Rust audit and normal database opening/recovery, prints JSON and returns nonzero on failure. It performs no repair and does not establish physical page integrity or hard resource bounds. No dependencies or upstream files changed; full V1 remains incomplete.

## Collated collection range resolution — 2026-09-07

The existing CLI reproduced `SELECT a < b COLLATE NOCASE FROM docs` failing with a missing physical column b. Range lowering now uses the existing scope-aware native-column check for both operand positions, avoiding the native conversion path when the operand is a collated collection field.

The final scoped check exited zero: formatting, Clippy with warnings denied, 222 Rust tests, seventeen Node tests and strict TypeScript checking. A new regression compares text/NULL results against native SQL across four range operators, both collated operand positions, NOCASE/BINARY, and direct/derived/CTE collection sources. Existing mixed-native range and native-column BETWEEN regressions gained parenthesized/unary-plus document cases. Initial investigation confirmed those wrappers already work through the existing helper; no redundant wrapper conversion was retained.

The change does not qualify explicit-collation binary/record ordering, arbitrary native expressions, or logical BETWEEN operands with native bounds. No upstream files, dependencies or persisted encodings changed; full V1 remains incomplete.

## Document BETWEEN native column bounds — 2026-09-07

A new native-SQL differential test first reproduced incorrect binary BETWEEN results from comparing encoded document values with raw native bounds. Converting the left value fixed binary/numeric comparisons; implicit NOCASE then exposed collation propagation from physical document storage. The final lowerer isolates that conversion in a generated scalar subquery while retaining the native BETWEEN node and each bound's affinity/collation.

The final scoped check exited zero: formatting, Clippy with warnings denied, 223 Rust tests, seventeen Node tests and strict TypeScript checking. The regression covers binary/numeric/text/NULL values, reversed and null bounds, NOT BETWEEN, parenthesis/unary-plus column wrappers, explicit and implicit collation including different collations on the two bounds, derived/CTE logical sources, null-aware record rejection, and native-target constraint rollback. The existing controlled callback test confirms one evaluation of a volatile logical left operand for both BETWEEN and NOT BETWEEN. An earlier full scoped run also passed before the final different-collation cases were added.

Native bounds are read again for record/null validation; volatile expressions behind native views or derived columns remain unqualified. Mixed typed/native bounds and arbitrary expressions are still open. This generated subquery does not add general public scalar-subquery support. No dependencies, upstream files or persisted encodings changed; full V1 remains incomplete.

## External profiling and audit consumers — 2026-09-07

Both final maintainer consumer checks exited zero. The standalone Rust 1.88 path-dependent application built and ran offline outside the workspace without injected workspace compiler flags; all 240 resolved registry/git identities remained within the pinned lockfile. The Node 24.19.0 Linux/x64 smoke packed seven files (58,564,863 compressed bytes), installed the tarball offline into a separate application, exercised synchronous/worker APIs and compiled strict TypeScript against the installed declarations. No package was published.

The expanded scripts verify exported profiling/audit types, record/int64 query results, lossless metrics, unique-index seeks, repeated counters, exact audit byte limits, audit-limit failure retaining active work, rollback and reopened index counts. Rust also rejects a profiling write and confirms retained collection contents. Installed TypeScript checks cover sync/async result types and reject numeric audit limits.

Both initial runs exposed a bad new assertion that a unique-index lookup must have positive index-iteration steps. The pinned engine increments those counters on iteration; a direct CLI probe reported four B-tree seeks and zero index steps for a unique lookup. Assertions now check seeks and lossless counter types. The corrected complete consumer runs passed. Script syntax and diff checks passed; production sources were unchanged, so the existing 223 Rust/17 Node scoped-test baseline was not rerun. Registry release, cross-platform prebuilds and complete distribution qualification remain open; full V1 remains incomplete.

## Shared tokenizer byte and token limits — 2026-09-07

The scoped check exited zero: formatting, Clippy with warnings denied, 225 Rust tests, seventeen Node tests and strict TypeScript checking. Shared tokenization now checks the 16 MiB UTF-8 input limit before scanning and the 262,144-token limit before copying each excess token. Comments/whitespace count toward bytes but not tokens; exact tokenizer boundaries remain accepted. Parser errors retain FDB_SYNTAX, with offset zero for byte rejection and the first excess token offset for token rejection.

A new parser test covers exact limits, an oversized UTF-8 input, ignored comments, excess quoted tokens and batch splitter propagation. A real-engine transaction test exercises oversized DELETE inputs through both execute_report and execute_batch, verifies that no prefix executes and that active work/index entries remain, then rolls back successfully. The parent FastQL plan and repository contracts/status were updated with the new bounds.

No dependencies or upstream files changed. Tokenizer bounds do not qualify full AST depth, flat expression chains, generated SQL expansion, deadlines, caller stack sizes or result memory. Existing CLI input-buffer overrides cannot increase these fixed tokenizer limits. Full V1 remains incomplete.

## Native parser stack exhaustion and reprepare — 2026-09-07

Local CLI subprocess probes reproduced SIGABRT from long NOT, unary plus/minus/bitwise-not chains and nested CASE before the pinned parser could return its depth-100 error. Flat arithmetic/AND/collation chains already rejected cleanly. FastDB now uses stacker 0.1.22 around public SQL execution/profiling/audits and internal AST parsing, statement preparation and row callbacks. The same-thread helper requests 32 MiB when less than 16 MiB remains; nested internal calls reuse sufficient available stack. The lockfile changes only the FastDB dependency edge to the already-pinned package. No upstream implementation or dependency versions changed.

The final scoped check exited zero: formatting, Clippy with warnings denied, 228 Rust tests, eighteen Node tests and strict TypeScript checking. New tests cover CLI process survival, native/collection error paths, retained transactions, rollback and later queries; a two-MiB Rust caller thread; Node sync/worker calls and profiling; and accepted depth-80 native expressions that actually reprepare after a schema change, verified with the engine reprepare counter. The first CLI regression expected the native depth message on collection execution too; that expectation was corrected to its existing FDB_UNSUPPORTED fallback, while profiling verifies the direct parser error.

An earlier complete scoped run was recovered from its finished log after its handle was unavailable, with TypeScript separately reconfirmed. The final run above has an observed zero exit status and includes the outer stack-reuse guards. The external Rust consumer passed with 244 pinned registry/git package identities before those final outer wrappers were added. A final 1,000-document benchmark passed all result/plan checks; its retained report and limitations are in benchmarks.md. This fixes the observed crash paths without establishing arbitrary caller-stack, frontend recursion, allocation-failure or cross-platform resource qualification. Full V1 remains incomplete.

## Native fallback parse-error preservation — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 229 Rust tests, eighteen Node tests and strict TypeScript checking. The native fallback guard now preserves the original first-statement parser error through the engine error conversion before unresolved managed names can hide it. Successful normalization keeps its existing path; the separate FastQL write guard retains lexical fallback for syntax awaiting expansion.

A new differential test compares exact messages with the pinned engine for malformed native/collection SELECT, CTE and INSERT SELECT, including a harmless collection-name string literal. It verifies retained active work, no target inserts and unchanged single-statement restrictions. CLI, small-stack Rust and Node sync/worker recursion tests now require the depth error for collection fallback too; profiling retains its existing validation wrapper. The first scoped attempt stopped on a Clippy test-style issue, corrected before the final successful run. No dependencies, upstream files or persisted encodings changed; broader syntax/authorization and V1 release qualification remain open.

## Instrumented 100,000-document benchmark — 2026-09-07

`python3 fastdb/scripts/benchmark.py --rows 100000 --samples 3 --output /tmp/fastdb-instrumented-100000.json` exited zero against the dev CLI from clean commit 10af87419. The run loaded and committed 100,000 documents, validated unindexed/indexed filter counts, asserted a named managed-index SEARCH, validated exact cosine top-10 ordering/nearest record, checkpointed and exited cleanly. The original process was polled through completion and was not restarted.

Post-run artifact validation confirmed three samples per workload, unsigned integer counters, identical counters across each workload's samples, exact expected physical row/fullscan counts and clean source state. The report was copied byte-for-byte to benchmark-results/2026-09-07-linux-dev-profile-100000.json. Median times, physical counters, load/index costs, database size and process high-water RSS are documented in benchmarks.md. No implementation or harness source changed, so the prior 229 Rust/18 Node scoped baseline was not rerun. This is a synthetic 16-dimensional Linux dev measurement; representative/high-dimensional/million-vector and broader release qualification remain open. Full V1 remains incomplete.

## Seeded high-dimensional benchmark and independent cosine reference — 2026-09-07

The updated benchmark harness passed two complete dev-CLI runs: cyclic/16 dimensions and seeded/768 dimensions, each with 1,000 documents, one warmup and three measured samples per workload. Both processes exited zero. The seeded generator retains the basis-vector nearest record, uses per-record Random seeds and mixed-sign tail coordinates, and avoids the legacy fixed 997-key cycle. Direct fixture checks confirmed reproducibility, the legacy period, seeded distinction for keys 1/998 and the nearest reference value.

The independent reference rounds coordinates to float32 and computes cosine using float64 fsum, retaining ten candidates in a bounded heap outside measured query samples. Assertions verify every returned distance within 2e-6, reference-cutoff membership, inclusion of records unambiguously nearer than the cutoff, unique/in-range keys and SQL distance/ID ordering. Reports retain the reference, tolerance and fixture/Python versions. Artifact checks confirmed dimensions, row/sample counts, reference cardinality and identical counters within each workload's samples. Progress output now identifies index construction and reference calculation.

Both reports are retained byte-for-byte under benchmark-results; timings and limitations are in benchmarks.md. Python compilation and diff checks passed. Rust/Node production sources were unchanged, so the existing 229 Rust/18 Node scoped baseline was not rerun. This is synthetic dimension/numerical coverage; real embedding data, high-dimensional 100k–1m scaling and full V1 qualification remain open.

## Typed VALUES sources and leading WITH collection inserts — 2026-09-07

A CLI probe reproduced a typed record from `WITH input(id) AS (VALUES (docs:a)) SELECT * FROM input` escaping as Binary. The final probe returns Record. Logical VALUES sources now lower every cell with typed metadata while ordinary native-only VALUES retain their engine path. CTEs and derived sources reuse existing metadata propagation. Leading WITH clauses on collection INSERT VALUES move into the source, which materializes through the CTE lowerer before atomic target writes.

The final scoped check exited zero: formatting, Clippy with warnings denied, 231 Rust tests, eighteen Node tests and strict TypeScript checking. Two new CTE tests cover record/boolean/binary/object/vector parameters, nested projection, default column names, derived VALUES, native-only binary behavior, collection/native inserts, RETURNING, unique/CHECK statement rollback and retained outer work. Initial test expectations were corrected to the existing qualified nested-path rule and FDB_CONSTRAINT unique-index code. The implementation uses syntax supported by the pinned Rust toolchain.

No dependencies, upstream files or persisted encodings changed. Scalar/correlated VALUES cells, modified/compound typed VALUES, recursive/forward CTEs and broader cross-clause scope remain open. Full V1 remains incomplete.

## VALUES identity, materialization and client rollback — 2026-09-07

The scoped check exited zero: formatting, Clippy with warnings denied, 232 Rust tests, nineteen Node tests and strict TypeScript checking. A new CTE regression covers heterogeneous record/Binary/NULL/integer/text/array/boolean rows, encoded-looking Binary identity, positional parameters and rejected inserts retaining outer work. The existing controlled callback test now verifies exactly two evaluations for two MATERIALIZED VALUES cells joined through two references. A new Node test covers both synchronous and worker clients, mixed value transport, unique-index statement rollback, collection/index integrity audits and explicit rollback.

The first check stopped because the integration fixture called a private encoding method; it now uses explicit encoded-looking bytes, consistent with other boundary tests. The arity, missing-parameter and unused-parameter cases use independent parameter sets so the latter cannot mask the former. A final focused CTE lint/test check covers that adjustment. No production implementation, dependencies, upstream files or persisted encodings changed. Broader VALUES/subquery and V1 release qualification remain open.

## Typed UNION ALL query and insert sources — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 235 Rust tests, nineteen Node tests and strict TypeScript declarations. Three new integration tests cover duplicate rows, heterogeneous/native/document arms in both orders, SELECT/VALUES sources, typed parameters, CTE/derived consumers, first-arm names, ordering/pagination, empty metadata, profiling, both EXPLAIN forms, collection statement rollback/index audits, native IGNORE and parameter/arity failures preserving outer work. Both Node clients now check heterogeneous UNION ALL transport. The controlled callback test verifies two evaluations for two MATERIALIZED VALUES cells referenced by both arms.

An initial generated nested-WITH layout failed to resolve an outer CTE in the pinned engine. Generated arm and compound definitions now share the user WITH scope, and the materialization test passes. The pinned native engine rejects COLLATE directly in compound ORDER BY; that ordering oracle uses a native compound wrapped in an outer SELECT. Other scalar ordering cases compare directly with native compounds. Initial checks also caught a lint shorthand issue and an incomplete intermediate edit; the final run includes the corrected tree. The earlier rejected VALUES-first UNION ALL fixture now checks unsupported UNION instead, with successful VALUES-first UNION ALL covered separately.

Native insert finalization is shared with ordinary lowered SELECT plans, preserving a single engine INSERT. No dependencies, upstream implementation files or persisted encodings changed. Parent FastQL.md and repository contracts/status describe the extension and remaining operators. UNION/INTERSECT/EXCEPT logical equality, broader ORDER BY matching, fetched arms, scalar/correlated queries and compound planner/resource/cancellation qualification remain unfinished. Full V1 remains incomplete.

## Compound ORDER BY name scope and parameter boundaries — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 237 Rust tests, nineteen Node tests and strict TypeScript checks. A differential regression first reproduced FDB_UNSUPPORTED for ORDER BY using an alias from the second UNION ALL arm. Name resolution now searches each arm left to right, following the pinned engine resolver while retaining first-arm result labels. The test compares exact columns and rows for later aliases, leftmost-arm precedence, quoted casing, three arms and a CTE consumer.

A second regression verifies anonymous placeholder positions across arms and LIMIT, and a shared encoded-looking Binary parameter in native and indexed collection predicates in both arm orders, plus a native VALUES CTE feeding the compound. Those parameter cases passed before the production fix. The first fixture compile was corrected to compare QueryResult columns/rows because QueryResult has no PartialEq implementation. No dependencies, upstream files or persisted encodings changed. Parent FastQL.md and current contracts/status now describe all-arm name resolution. Other compound operators, arbitrary ORDER BY expressions, subqueries and full V1 qualification remain unfinished.

## UNION ALL source and partial-write interruption — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 238 Rust tests, nineteen Node tests and strict TypeScript declarations. A new controlled test registers a counting scalar and uses progress callbacks to interrupt reads and collection inserts after exactly two or four source evaluations, in autocommit and outer transactions. It requires FDB_CANCELLED without a rowset, intact prior work, successful source/target document-index audits, exact-statement retry returning all six expected values, and successful explicit rollback. The existing after-write interruption matrix now includes plain and MATERIALIZED-CTE UNION ALL inserts and collection-content audits.

The focused interruption suite passed before the stronger exact-value/count assertions; the final scoped run includes those assertions. A final package-focused lint check covers the last assertion adjustment. An initial compile found a test callback export-name collision, corrected by giving the callback its own Rust name. No production implementation, dependencies, upstream files or persisted encodings changed. These selected deterministic points do not prove every cancellation point, native-target interruption semantics, deadlines or cross-platform qualification. Full V1 remains incomplete.

## Typed UNION, INTERSECT and EXCEPT — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 242 Rust tests, nineteen Node tests and strict TypeScript checks. Four new integration tests verify native differential scalar results, left-associated mixed operators including UNION ALL, numeric equality on the logical path, whole-row/NULL equality, pagination, empty derived metadata, CTE insert sources, canonical records versus encoded-looking Binary, explicit and implicit NOCASE collation, collection unique-index rollback/audits, native INSERT/IGNORE and composite-error transaction observations. Both Node clients exercise all three added operators. The existing controlled callback test confirms two source projections execute exactly twice across key construction and typed representative recovery.

Lowering materializes source and intermediate rows, delegates membership to native set operations over existing SQL scalar keys, and recovers typed representatives through null-safe joins/grouping. Pure UNION ALL retains its prior lowering. Initial tests that expected all distinct set operators to fail now cover malformed compound arity instead. A numeric-equality fixture was corrected to qualify a stored field rather than filtering its projection alias. The evaluated-composite rejection test records the pinned engine's outer-transaction abort on a scalar-function error, consistent with the existing transaction-report contract. A stale single-element rejection loop was fixed before the successful lint run.

No dependencies, upstream implementation files or persisted encodings changed. Parent FastQL.md and repository contracts/status describe the implemented set operators, unspecified representatives among equivalent values and composite-equality limitation. Materialization performance/memory costs, broader planner/cancellation/platform qualification and full V1 remain unfinished.

## Set-collation precedence and intermediate results — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 243 Rust tests, nineteen Node tests and strict TypeScript declarations. One new differential regression checks 48 two-arm cases across default/BINARY/NOCASE/RTRIM and UNION/INTERSECT/EXCEPT, plus 64 three-arm chains covering all pairs of compound operators under a shared collation. Fixtures include NULL, ASCII case variants, trailing spaces and duplicates. Comparison normalizes equivalent text representatives and preserves duplicate multiplicity, matching the existing unspecified-representative contract. All 112 cases agree with the pinned native engine; the focused two-arm probe also passed before the chain extension.

No production implementation, dependencies, upstream files or persisted encodings changed. This verifies the exercised precedence and intermediate-result cases, not byte-identical representative selection, mixed-collation chains beyond two arms, broader Unicode/custom collations or complete V1 qualification. Those gates remain open.

## Distinct-set source and write cancellation — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 243 Rust tests, nineteen Node tests and strict TypeScript declarations. Existing interruption tests now cover all four compound operators across reads/collection inserts, exact evaluation counts of two/four and both transaction modes (32 source combinations). They require FDB_CANCELLED without result rows, source/target document-index integrity, retained prior work and exact retry values/counts, including an empty INTERSECT result. The after-write matrix adds UNION/INTERSECT/EXCEPT and VALUES-first UNION in both modes, checking rollback after engine change counters advance.

The focused interruption suite also passed. An initial diagnostic edit referenced a statement variable outside its test scope and was corrected before verification. No production implementation, dependencies, upstream files or persisted encodings changed. These selected points extend the earlier UNION ALL evidence; native-target cancellation, exhaustive instruction/resource/deadline coverage and full V1 qualification remain open.

## Rust constructors for sparse, quantized and bit vectors — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 245 Rust tests, nineteen Node tests and strict TypeScript checking. Two new vector tests compare vector32_sparse/vector8/vector1bit Rust construction byte-for-byte with native SQL on mixed-sign, all-zero, constant and packing-boundary inputs, persist/reopen twelve values, audit stored documents, and reject empty/non-finite/oversized inputs while accepting 65,536 components. Dense constructors now check dimensions before allocating output bytes. The first scoped attempt found a test function-pointer type-complexity lint, fixed with a local type alias before the successful check.

The standalone offline Rust consumer also exited zero, calling all five public constructors through its sole FastDB path dependency and binding the results as typed parameters. Its existing validation/index/transaction/QuickJS/profile/audit/reopen checks pass, with 244 registry/git package identities verified against the workspace lockfile. The consumer remains outside the workspace and uses its own generated lockfile/build configuration.

Conversion delegates to pinned upstream vector conversion/serialization and validates generated bytes; no dependencies, upstream implementation files or persisted encodings changed. The Rust client guide documents dense input slices, quantization loss, dimensions and the expanded smoke. Broader numerical/platform/resource and full V1 qualification remain open.

## Node factories for all five vector encodings — 2026-09-07

The scoped check exited zero: formatting, Clippy with warnings denied, 245 Rust tests, twenty Node tests and strict TypeScript declarations. A new Node test compares all five factory encodings with native SQL for arrays/float typed arrays, zero/constant/mixed-sign and packing-boundary inputs; binds/inserts through synchronous and worker clients; audits stored documents; and checks dimensions, finite components, accepted float64 versus rejected float32 overflow, copied inputs and malformed direct-native buffers. A focused post-check run also passes the final negative-zero and direct-native NaN assertions.

The offline packed-package smoke exited zero on Linux/x64, Node 24.19.0: seven expected files, 58,846,096 compressed bytes. The installed consumer calls all five factories through both clients and type-checks VectorComponents, readonly arrays and rejection of bigint components. Temporary package/consumer artifacts are cleaned by the script; nothing was published.

Factories send bounded binary64 component buffers to a native adapter that calls the Rust Value constructors and validates results. No database connection is opened. No dependencies, upstream implementation files or persisted encodings changed. The Node guide and current contracts/status document synchronous construction, float32 conversion, lossy quantized/bit formats and argument errors. Broader numerical/platform/resource and full V1 qualification remain open.

## Vector-factory rounding and quantization overflow — 2026-09-07

The final scoped check exited zero: formatting, Clippy with warnings denied, 245 Rust tests, twenty-one Node tests and strict TypeScript declarations. A new Node test checks exact known IEEE-754 patterns for float32 halfway ties, subnormal rounding/underflow and signed zero, plus binary64 positive/negative minimum subnormals, the value adjacent to one and maximum finite value. Bit output is checked independently; sparse/quantized/bit bytes also match native conversion of the checked dense bytes.

Rust and Node regressions reject quantizer scale overflow from finite negative/positive float32 maxima and accept an equal-maximum constant vector. The Node case runs while unrelated database work is active, verifies that construction failure leaves that transaction/data intact, and then rolls it back successfully. The focused Node precision probe passed before the full run.

No production implementation, dependencies, upstream files or persisted encodings changed. The client guide and contracts/status document the precision boundaries and finite-input quantization limitation. Broader numerical/platform/resource and full V1 qualification remain open.

## Sparse entry Rust constructor (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 247 Rust tests, twenty-one Node tests and strict TypeScript declarations. Two new vector tests exercise sparse index/value construction, equality with dense-input sparse encoding, native dense extraction, typed binding, vector field-dimension rejection, content audit and close/reopen. Boundary cases include empty and zero-only entries at 65,536 dimensions, the last valid index, non-finite components, unordered/duplicate/out-of-range indices and zero/oversized dimensions. The implementation validates before allocation and emits the existing little-endian sparse layout with output storage proportional to nonzero entries.

The standalone Rust consumer also exited zero after exercising the new public API outside the workspace. Its 244 registry/git dependency identities matched the workspace lockfile; the cached build completed in 19.60 seconds. Logs: /tmp/fastdb-sparse-entries-check.log and /tmp/fastdb-sparse-entries-consumer.log. No upstream source, dependency or persisted-format changes. Broader numerical/platform and full V1 qualification remain open.

## Node sparse entry construction (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 247 Rust tests, twenty-two Node tests and strict TypeScript declarations. The new Node test covers sparse entry construction and typed roundtrips through synchronous and worker clients, native dense extraction, collection field validation and integrity audit, float32 halfway rounding, input ownership, empty/zero-only maximum-dimension encodings and the last valid index. It rejects malformed pairs, invalid dimensions, unordered/duplicate/out-of-range indices, non-finite values and float32 overflow. Direct-addon tests independently cover dimensions, malformed buffers, excess entry counts, duplicate indices and invalid components. A local constructor error also leaves both clients able to query and roll back an existing transaction.

The offline packed-package consumer passed on Linux x64 / Node 24.19.0: seven files, 58,861,724 compressed bytes. Installed runtime checks use the new factory through both clients; installed declaration checks exercise readonly SparseVectorEntry tuples and reject bigint components. Logs: /tmp/fastdb-node-sparse-check.log and /tmp/fastdb-node-sparse-package.log. No publishing, upstream-source, dependency or persisted-format changes. Other platforms and full V1 release qualification remain open.

## Initial typed scalar subqueries (2026-09-07)

The final scoped check exited zero: formatting, Clippy with warnings denied, 249 Rust tests, twenty-three Node tests and strict TypeScript declarations. Two new integration tests cover typed record/object/array/binary projections, scalar arithmetic and aggregate comparison, nested scalar queries, zero-row NULL and ordered first-row selection, inner parameters and earlier CTE references, collection/native insert sources, field-validation rollback and integrity audit. Multiple inner output columns and the current correlated collection probe reject. A new Node regression covers records, arrays, NULL and comparison through both clients.

The existing scalar counter test now verifies one execution of an uncorrelated scalar subquery across multiple outer rows. An initial expectation of zero execution in an unused CASE branch failed: the callback ran once. A native-only CASE/scalar-subquery differential probe also runs once, and the final test asserts that shared pinned behavior. Documentation explicitly records eager scalar-subquery execution; this result does not establish lazy subquery suppression. The first compile attempt also identified that the pinned core does not export its SELECT walker, so the frontend collects its own outer expression inputs and uses the exported expression walker.

Logs: /tmp/fastdb-scalar-new.log (focused integration), /tmp/fastdb-scalar-evaluation.log (native evaluation comparison), /tmp/fastdb-scalar-final-check.log (successful scoped check). Scalar lowering prepares AST plans and retains type/parameter metadata without running source queries; actual evaluation stays in the engine statement. No upstream, dependency, publishing or persisted-format changes. Correlation, EXISTS/IN, broader native-inner/cross-clause semantics and resource/platform/release qualification remain open.

## Initial collection EXISTS subqueries (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 250 Rust tests, twenty-three Node tests and strict TypeScript declarations. A new integration test compares fourteen EXISTS/NOT EXISTS expressions with native SQL: nonempty/empty inputs, duplicate projected column names, aggregates over empty inputs, LIMIT zero, OFFSET past the end and GROUP BY/HAVING. It also covers collection stars, compound sources, outer filtering, earlier CTE references with a bound parameter and collection/native INSERT SELECT. Both Node clients check existence over typed record/array projections and an empty inner filter.

The existing test-only scalar counter additionally proves that an unused EXISTS projection is not evaluated and a matching WHERE predicate runs only once despite two source rows. Both counts match native SQL probes. Focused scalar/EXISTS integration tests passed before the full check. Logs: /tmp/fastdb-exists-probe.log and /tmp/fastdb-exists-check.log. No upstream-source, dependency, persisted-format or publishing changes. Correlated collection queries, IN subqueries and broader cross-clause/resource/platform/release qualification remain open.

## Initial collection IN/NOT IN subqueries (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 252 Rust tests, twenty-three Node tests and strict TypeScript declarations. Two new membership regressions compare thirty scalar/NULL/empty-source expressions with native SQL and cover filtered collection insertion, a scalar subquery on the left, earlier logical CTE references, record integer/string keys, binary bytes matching a record encoding, bound binary values, native integer affinity and native BLOB left operands. Both Node clients additionally check record membership, NOT IN and NULL results.

The initial native-column CASE path duplicated RHS evaluation: a two-row source invoked the test-only scalar four times. Sharing one materialized source between the scalar and binary branches reduces that to two, including across multiple outer rows; the counter test now passes. Early test drafts also referenced a private encoding method and unsupported quoted fixed-record syntax; they were corrected to an explicit persisted-byte fixture and type::record constructor. No product behavior was weakened to pass those fixture checks.

Logs: /tmp/fastdb-in-probe.log (focused integration iterations), /tmp/fastdb-in-count.log (successful evaluation-count regression), /tmp/fastdb-in-check.log (successful scoped check). No upstream-source, dependency, persisted-format or publishing changes. Correlated collection queries, native-only inner routes, row-value/composite membership, broader collation/cross-clause semantics and resource/platform/release qualification remain open.

## Membership affinity and collation correction (2026-09-07)

The final scoped check exited zero: formatting, Clippy with warnings denied, 253 Rust tests, twenty-three Node tests and strict TypeScript declarations. One new matrix compares 504 IN/NOT IN query pairs with native SQL, each returning ten rows. It spans absent/INTEGER/REAL/NUMERIC/TEXT/BLOB declarations and implicit NOCASE on the native left column; bare columns, unary plus, text casts and explicit BINARY/NOCASE/RTRIM on the left; and bare/collated columns, unary plus and concatenation on the right. Numeric/text/NULL/BLOB inputs exercise both membership operators. Both Node clients reproduce the two corrected cases.

The first mismatch made a TEXT cast match numeric RHS values that a native typeless column rejected: the decode function erased column affinity. A derived column boundary restores that distinction. The expanded matrix then found the inverse issue for computed RHS values: native-column materialization introduced affinity where SELECT +v had none. The cached plan now carries the column/expression distinction, and unary plus removes the temporary column affinity when needed. The existing volatile-source count regression continues to pass.

The focused final matrix passed in 6.18 seconds (/tmp/fastdb-in-affinity-final-probe.log). The first scoped attempt retained the computed-RHS failure in /tmp/fastdb-in-affinity-check.log; the successful full run is /tmp/fastdb-in-affinity-final-check.log. No upstream, dependency, persisted-format or publishing changes. Broader RHS casts, mixed projection metadata, correlation and resource/platform/release qualification remain open.

## Subquery write failure and retry (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 254 Rust tests, twenty-three Node tests and strict TypeScript declarations. One new integration test exercises four late unique-index failures through IN, NOT IN, EXISTS and scalar-subquery comparison INSERT SELECT filters. The source is ordered so values 1 and 2 precede the conflicting value 9. After each failure, the earlier target document retains its exact ID/value, the transaction remains active, an integrity audit finds one document, and an indexed equality query finds no partial value 1. A valid filtered insert then succeeds and cleanup restores the prior state before the next failure. Final outer rollback leaves an empty audited target and unchanged source values.

Log: /tmp/fastdb-subquery-rollback.log. This slice changes tests and documentation only; no runtime, upstream-source, dependency, persisted-format or publishing changes. Broader cancellation, engine-error dispositions and resource/platform/release qualification remain open.

## Subquery source interruption and retry (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 254 Rust tests, twenty-three Node tests and strict TypeScript declarations. The existing compound-source interruption unit test now also covers IN, NOT IN, EXISTS and a scalar aggregate comparison. Each inner query invokes a counting scalar twice per source row; a progress handler interrupts after exactly two or four callback evaluations. Reads and collection INSERT SELECT run in both autocommit and explicit transactions, adding 32 cases to the existing 32 compound cases.

Every new case asserts that the handler fired at the requested count, reports FDB_CANCELLED, retains the expected observed transaction state and earlier native-table work, and leaves the insert target empty and both collections passing integrity audits. Exact retry values and insert counts are verified; explicit outer transactions retain the existing rollback checks. Log: /tmp/fastdb-subquery-cancel.log. This slice changes test coverage and documentation only. Per-operation Node cancellation, native-target interruption coverage, hard deadlines and broader resource/platform/release qualification remain open.

## Native INSERT SELECT source cancellation (2026-09-07)

The scoped check exited zero: formatting, Clippy with warnings denied, 255 Rust tests, twenty-three Node tests and strict TypeScript declarations. A new deterministic unit test compares sixteen collection-backed/native-only query pairs: four subquery forms, two source callback thresholds and autocommit versus explicit transactions. Both queries insert into a native INTEGER UNIQUE target. Progress-handler cancellation must occur after exactly two/four callbacks and report FDB_CANCELLED; targets must remain empty, collection source audits must retain three documents, and transaction state/prior native-table work must match the native oracle. Retrying must insert the exact values 1, 2 and 3; any surviving explicit transaction must still roll back cleanly.

The focused test passed in 1.56 seconds (/tmp/fastdb-native-subquery-cancel-probe.log). Full results: /tmp/fastdb-native-subquery-cancel-check.log. No runtime, upstream, dependency, persisted-format or publishing changes. This qualifies source evaluation before output rows reach the native target; interruption after partial native writes, per-operation client cancellation and broader resource/platform/release qualification remain open.

## Trigger interruption release gate and proposed fix (2026-09-07)

The post-write probe found a pinned core defect. A counter at the end of an AFTER INSERT trigger confirms that target and side-effect inserts occurred before progress-handler cancellation. The first native/autocommit case then reports FDB_BUSY rather than FDB_CANCELLED. A diagnostic read showed autocommit state and empty target/effects tables. Core inspection identifies OpProgram's shared StepResult::Interrupt/Busy branch returning LimboError::Busy. The proposed patch preserves the distinction but is not applied, built or runtime-validated; git apply --check passes.

The strict ignored regression was run explicitly and fails as expected with FDB_BUSY versus FDB_CANCELLED (/tmp/fastdb-trigger-release-gate.log). It is intended to cover eight native/logical after-write pairs after the fix, but currently fails on the first native case. This is incomplete release qualification. The prior sixteen source-boundary pairs remain enabled, now with trigger-effect retry checks.

Routine scoped checks pass formatting, Clippy with warnings denied, 255 Rust tests and twenty-three Node tests plus strict TypeScript; one known release-gate test is ignored (/tmp/fastdb-trigger-gate-check.log). The initial failure/state probes are /tmp/fastdb-native-write-cancel-probe.log and /tmp/fastdb-native-write-cancel-state.log. No upstream core change has been made. See trigger-interrupt.md and trigger-interrupt.patch for the reviewable proposal and required FastDB/upstream verification. Core approval remains pending under the project workflow.

## Source-free typed subqueries and CTEs (2026-09-07)

The final scoped check exited zero: formatting, Clippy with warnings denied, 256 Rust tests, twenty-three Node tests and strict TypeScript declarations. One known trigger-interruption release-gate regression remains ignored and unresolved. A new integration test retains records, booleans, objects, arrays, encoded-looking binary and vectors through source-free scalar queries, nested scalar levels, CTEs and derived sources; checks EXISTS, record constructors/IN, statement-wide numbered parameters and collection inserts; and verifies native composite rejection leaves its target empty. Both Node clients check typed scalar and CTE roundtrips.

The first full attempt caught a compatibility regression: treating a binary-only CTE as a logical source changed qualified projection labels and caused a duplicate-name error. Binary-only CTEs and derived tables now retain their native route, while binary expression subqueries explicitly opt into lowering. The CTE/subquery retest passed all fifteen tests, including the existing shared binary/index parameter regression and affinity matrix. Logs: /tmp/fastdb-source-free-subquery.log, /tmp/fastdb-source-free-check.log (first failure), /tmp/fastdb-source-free-retest.log and /tmp/fastdb-source-free-final-check.log. No upstream, dependency, persisted-format or publishing changes. The trigger-core proposal, native-only inner queries, broader correlation/affinity/resource/platform work and full V1 release qualification remain open.

## Nested anonymous parameters and failed writes (2026-09-07)

The focused regression and full scoped check passed. Full results: formatting, Clippy with warnings denied, 257 Rust tests, twenty-three Node tests and strict TypeScript declarations; one known trigger-interruption release gate remains ignored. The new test binds anonymous placeholders through outer/scalar queries and leading CTEs using integers, arrays and encoded-looking binary values. It rejects missing and unused parameters in collection INSERT SELECT while retaining the prior document ID/value, active transaction and audited unique index. Corrected bindings permit retry, and outer rollback leaves an empty audited collection. Both Node clients verify anonymous parameter numbering through a nested scalar query.

Logs: /tmp/fastdb-nested-parameters.log and /tmp/fastdb-nested-parameters-check.log. This slice changes tests and documentation only. The trigger core proposal and broader SQL/resource/platform/release work remain open.

## Native-source typed nested projections (2026-09-07)

The final scoped check exited zero: formatting, Clippy with warnings denied, 258 Rust tests, twenty-three Node tests and strict TypeScript declarations; the known trigger-interruption release gate remains ignored. A new integration regression covers record/boolean/array/object/binary/vector parameter projections over native tables through scalar queries, CTEs, derived sources and EXISTS, plus empty scalar results, source-side binary filtering, record-helper IN and collection insertion. Both Node clients check native-source record/array scalar queries and a record CTE.

The first broad opt-in made a binary predicate alone trigger logical lowering, causing '1' IN a native INTEGER-column subquery to return 0 instead of the native result 1. Restricting native-source opt-in to explicit logical projections preserves that delegation/affinity behavior; the regression remains. The rule then extends consistently to native-source CTE and derived-table projections, with binary-only native paths unchanged. Projection text is owned only when needed; ordinary query text stays borrowed.

Logs: /tmp/fastdb-native-source-typed.log, /tmp/fastdb-native-source-oracle.log (observed affinity failure), /tmp/fastdb-native-source-retest.log and /tmp/fastdb-native-nested-final-check.log (final scope). No upstream, dependency, persisted-format or publishing changes. The trigger-core proposal, broader helper-only predicates/native projection metadata, correlation and resource/platform/release qualification remain open.

## Scalar-subquery pagination — 2026-09-07

`fastdb/scripts/check.sh` passed scoped formatting, Clippy, 259 Rust tests, twenty-three Node tests and strict TypeScript checking (log: `/tmp/fastdb-pagination-check.log`). One known trigger-interruption release-gate regression remains ignored. The new pagination regression covers native differential limits, offsets, DISTINCT, bound values, native-only subqueries and failed/successful collection insertion. A separate native oracle confirmed that an outer CTE referenced from LIMIT fails on the pinned engine too; the regression retains both rejection checks. Both Node clients exercise bound DISTINCT pagination. No upstream source changes.

## Compound scalar-subquery pagination — 2026-09-07

`fastdb/scripts/check.sh` passed formatting, Clippy, 260 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-compound-pagination-check.log`). The known trigger-interruption release gate remains ignored. Twelve successful native derived-table differential cases cover four set operators and three pagination forms. Additional cases cover bound LIMIT/OFFSET, native arms with a logical subquery, failed insertion with an empty scalar limit and successful insertion. The direct native compound form's datatype-mismatch rejection is recorded separately, not counted as a successful equivalence case. Both Node clients pass bound UNION pagination. No upstream implementation changes.

## Pagination evaluation and transaction failures — 2026-09-07

The existing native-callback unit test now checks exactly two calls (one LIMIT, one OFFSET) across plain, DISTINCT, UNION ALL and UNION queries. The new integration regression covers twelve invalid-limit insertion cases across three query shapes, preserving prior IDs/values, active transaction state and index integrity, followed by successful retry and rollback. Focused tests passed (`/tmp/fastdb-pagination-evaluation.log`, `/tmp/fastdb-pagination-errors.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 261 Rust tests, twenty-three Node tests and strict TypeScript checks (`/tmp/fastdb-pagination-qualification-check.log`). One known trigger-interruption release gate remains ignored. No production implementation or upstream files changed.

## Pagination cancellation — 2026-09-07

The existing compound/subquery cancellation matrix now passes ninety-six cases, including thirty-two LIMIT/OFFSET cases across plain/UNION queries, reads/inserts, two callback thresholds and autocommit/explicit transactions. It checks exact callback counts, FDB_CANCELLED, prior transaction work, source/target integrity, retry values and rollback. Identical callback expressions initially produced only three calls, so the final OFFSET probe uses distinct arguments and reaches both thresholds (`/tmp/fastdb-pagination-interrupt-final.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 261 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-pagination-cancellation-check.log`). One known trigger-interruption gate remains ignored. No production implementation or upstream source changes.

## Native scalar sources in logical queries — 2026-09-07

The new native-scalar integration regression covers INTEGER/BLOB/NULL projection, ordered first-row selection, named parameters, filtering, multi-column rejection and collection insertion. Both Node clients cover TEXT. An initial full run exposed a missing-parameter insert accepted as NULL; recording native inner bindings and validating them at the typed boundary restored the existing failure-atomicity contract. All fourteen subquery tests passed (`/tmp/fastdb-native-scalar-retest.log`). Final `fastdb/scripts/check.sh` passed formatting, Clippy, 262 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-native-scalar-final-check.log`). One known trigger-interruption gate remains ignored. No upstream source or persisted-format changes.

## Native EXISTS sources — 2026-09-07

The focused native EXISTS regression passed five native differential query pairs (stars, multiple columns, empty count, LIMIT zero and OFFSET past end), bound filtering and a missing-parameter insert preserving prior IDs/values, index integrity and active transaction state before successful retry/rollback (`/tmp/fastdb-native-exists.log`). The existing callback unit test verifies zero evaluations of an unused native EXISTS projection. Both Node clients pass a bound EXISTS projection. Full `fastdb/scripts/check.sh` passed formatting, Clippy, 263 Rust tests, twenty-three Node tests and strict TypeScript checks (`/tmp/fastdb-native-exists-check.log`). One known trigger-interruption gate remains ignored. No upstream implementation or encoding changes.

## Native scalar comparison affinity — 2026-09-07

The initial differential probe exposed lost numeric affinity (`'2'` failed to equal a native INTEGER scalar). Subsequent probes exposed TEXT/no-affinity distinctions and operand-order collation differences. The final implementation uses a shared first-row native source, a typeless logical column boundary, and collation from prepared native result metadata after logical-route selection. Forty-eight query pairs now match native SQL across three column declarations, eight comparison operators and both operand orders. Additional tests check native-only correlation, BLOB/record distinction, NULL, and one native callback evaluation across outer rows. Both Node clients pass a native numeric scalar comparison. Full `fastdb/scripts/check.sh` passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-native-affinity-check.log`). One known trigger-interruption gate remains ignored. No upstream implementation or encoding changes. Explicit COLLATE wrappers and broader expression/CTE metadata remain unqualified.

## Explicit scalar comparison collation — 2026-09-07

Inner COLLATE projections passed the first expanded probe. An outer COLLATE wrapper exposed lost numeric affinity, fixed by recognizing wrapped scalar sources and retaining explicit operand-order collation precedence. The full existing differential test now passes 768 query pairs across three declarations, four inner projection forms, eight operators and eight operand/wrapper arrangements. The callback unit test verifies once-only native source evaluation with a wrapper, and both Node clients pass wrapped numeric comparison. Full `fastdb/scripts/check.sh` passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-scalar-collation-check.log`). One known trigger-interruption gate remains ignored. No upstream implementation or encoding changes.

## Trailing-space scalar collation coverage — 2026-09-07

The scalar affinity matrix now passes 1,024 native differential query pairs over nine values (9,216 compared result cells per route), including single/double trailing spaces, a trailing tab, empty text and mixed case. A declared RTRIM source joins INTEGER, TEXT and NOCASE, and the existing four inner projection/eight operator/eight wrapper arrangements remain covered. The initial nine-value probe passed (`/tmp/fastdb-scalar-collation-values.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-scalar-collation-values-check.log`). One known trigger-interruption gate remains ignored. No production implementation or upstream files changed.

## Computed native scalar affinity — 2026-09-07

The existing scalar comparison matrix now passes 1,792 native query pairs over nine values (16,128 result cells per route), adding +v, CAST(v AS TEXT) and CAST(v AS NUMERIC) as inner projections across four native declarations, eight operators and eight operand/wrapper arrangements. The focused probe passed (`/tmp/fastdb-scalar-computed-affinity.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checks (`/tmp/fastdb-scalar-computed-affinity-check.log`). One known trigger-interruption gate remains ignored. No production implementation or upstream source changes. Outer unary-plus/CAST wrappers and broader compound/CTE metadata remain unqualified.
