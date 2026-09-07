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

## Native scalar comparison write rollback — 2026-09-07

The new integration regression checks four native INTEGER scalar comparison predicates over document strings (both operand orders, with/without outer COLLATE). A late unique conflict preserves prior record IDs/values, active transaction state and index integrity; each corrected retry inserts exact rows, followed by final rollback to an empty target/index. Focused evidence passed (`/tmp/fastdb-native-scalar-write-rollback.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 266 Rust tests, twenty-three Node tests and strict TypeScript checking (`/tmp/fastdb-native-scalar-write-check.log`). One known trigger-interruption gate remains ignored. No production implementation or upstream source changes.

## Operation-scoped Rust cancellation tokens — 2026-09-07

Two new unit tests pass pre-cancellation from a second thread, preserved prior transaction work, fresh-token execution, late cancellation isolation, and deterministic active cancellation of collection reads/insert sources in autocommit/explicit transactions. They check exact callback counts, FDB_CANCELLED, target/index state, retry and rollback (`/tmp/fastdb-cancellation-token-retest.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 268 Rust tests, twenty-three Node tests and strict TypeScript checks (`/tmp/fastdb-cancellation-token-check.log`). One known trigger-interruption gate remains ignored. Public Rust cancellation is cooperative at engine progress boundaries; Node request-scoped integration, performance and platform qualification remain open. No upstream source changes.

## Async query AbortSignal — 2026-09-07

The new real-worker test passes queued/active query cancellation, following-query isolation, prior transaction/index integrity, pre-aborted requests, late abort, listener cleanup (including stopImmediatePropagation resistance) and invalid-signal rejection (`/tmp/fastdb-node-abort-test.log`). Transport fault injection additionally verifies released native token IDs and listeners after response-channel or send failure. TypeScript checks cover execute/all/first/exactlyOne options and reject non-signals. Full `fastdb/scripts/check.sh` passed formatting, Clippy, 268 Rust tests, twenty-four Node tests and strict TypeScript checking (`/tmp/fastdb-node-abort-check.log`). One known trigger-interruption gate remains ignored. Other operation types, deadlines and broader lifecycle/platform/performance qualification remain open. No upstream source changes.

## Cancellable SELECT profiling — 2026-09-07

The focused real-worker profiling test passes active/pre-cancellation, following-query isolation, preserved prior transaction data, no partial metrics on cancellation, complete retry metrics, listener disposal and late-abort isolation (`/tmp/fastdb-profile-abort-retest.log`). The first test assertion was corrected to use the existing result.transaction nesting. Strict types accept profileSelect's new options. Full `fastdb/scripts/check.sh` passed formatting, Clippy, 268 Rust tests, twenty-five Node tests and strict TypeScript checks (`/tmp/fastdb-profile-abort-check.log`). One known trigger-interruption gate remains ignored. Other operation types, deadlines and broader runtime/platform/performance qualification remain open. No upstream source changes.

## Cancellable integrity audits — 2026-09-07

The real-worker test passes active/pre-cancellation on an indexed collection, following-query isolation, prior transaction data, exact 1,001-document/index-entry retry counts, listener cleanup and rollback (`/tmp/fastdb-audit-abort-bounded.log`, about seven seconds). The initial 10,000-document workload was intentionally terminated after more than 80 seconds CPU without completion; its population/audit phases were not instrumented, so the costly phase remains an investigation gap. Full `fastdb/scripts/check.sh` passed formatting, Clippy, 268 Rust tests, twenty-six Node tests and strict TypeScript checks (`/tmp/fastdb-audit-abort-check.log`). One known trigger-interruption gate remains ignored. No upstream source changes.


## Index audit scan scaling — 2026-09-07

The audit now streams each index once and checks document primary-key lookups, native key equality and unique document coverage. Existing corruption and cancellation regressions pass; a new VM-step regression checks 64/256 documents with duplicate and NULL keys without wall-clock assertions. Scoped checks passed formatting, Clippy, 269 Rust tests and twenty-six Node tests, with strict TypeScript checking (`/tmp/fastdb-audit-scaling-check.log`). One known trigger-interruption gate remains ignored.

Separate insertion/audit diagnostics identified poor audit scaling: at 1,000 documents, audit time changed from about 3.40 seconds to 0.79 seconds while insertion stayed near 2.5 seconds. A subsequent 10,000-document VALUES workload completed insertion in 27.7 seconds and audit in 8.65 seconds. This is not an exact repeat of the previously stopped INSERT SELECT cancellation fixture. The maintainer script `fastdb/scripts/bench-audit.cjs` passed syntax and 100-row execution checks. See benchmarks.md for measurement limits. No upstream files, dependencies or stored schema changed; broader performance and full V1 qualification remain open.


## Operation-scoped batch cancellation — 2026-09-07

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 270 Rust tests, twenty-seven Node tests and strict TypeScript checking (`/tmp/fastdb-batch-abort-check.log`). One known trigger-interruption gate remains ignored. The new batch integration test covers deterministic cancellation between statements in both transaction modes, UTF-8 offsets, pre-cancelled malformed input, prior index integrity, retry and rollback. The existing deterministic token callback matrix now also runs read/insert cancellation through batches in both transaction modes, requiring exactly two source callbacks and no later statement execution.

The real-worker AbortSignal test passed active batch cancellation with retained successful entries, the cancelled statement report, unchanged prior indexed data, following-request isolation, pre-aborted rejection, listener disposal, successful retry and rollback. Sync batch tests and encoding-error stop behavior still pass. No upstream files or dependencies changed. Migrations/transfers cancellation, broader lifecycle/platform/deadline qualification and full V1 remain incomplete.


## Rust transfer cancellation — 2026-09-07

The focused regression passed both JSON/NDJSON and autocommit/explicit transaction cases (`/tmp/fastdb-transfer-cancel.log`). A deterministic engine progress interruption after import writes began verifies FDB_CANCELLED, rollback of imported data, preserved prior document/index state and exact retry. A separate immediate engine interruption checks export failure without a payload. Token API coverage verifies pre-cancelled import rejects even invalid input, pre-cancelled export rejection, fresh-token import/export success and late-token isolation. These engine-level active interruption probes do not independently qualify an active cross-thread transfer token race. Node transfer AbortSignal integration remains open.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 271 Rust tests, twenty-seven Node tests and strict TypeScript checking (`/tmp/fastdb-transfer-cancel-check.log`). One known trigger-interruption gate remains ignored. No upstream files, stored format or dependencies changed. Parsing/serialization latency, broader resources/platforms and full V1 remain incomplete.


## Node transfer AbortSignal — 2026-09-07

The real-worker transfer test passes both JSON and NDJSON: timed import/export cancellation, prior indexed document preservation, following-request isolation, pre-aborted rejection (including invalid import input), successful 1,000-document retry, complete export equality, listener cleanup, late cancellation and final rollback. The Node-only suite and strict TypeScript passed (`/tmp/fastdb-node-transfer-abort.log`). Timer-based active probes complement the deterministic Rust after-write rollback coverage; they do not establish cancellation latency bounds.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 271 Rust tests, twenty-eight Node tests and strict TypeScript checks (`/tmp/fastdb-node-transfer-check.log`). One known trigger-interruption gate remains ignored. No upstream files, dependencies or transfer encodings changed. Migration cancellation, parsing/serialization latency, platform/resource qualification and full V1 remain incomplete.


## Migration AbortSignal — 2026-09-07

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 271 Rust tests, twenty-nine Node tests and strict TypeScript checks (`/tmp/fastdb-migration-abort-check.log`). One known trigger-interruption gate remains ignored. The real-worker migration test cancels a long pending INSERT SELECT after an earlier pending migration, checks FDB_CANCELLED/autocommit, absence of pending schema, retained baseline data/history, pre-aborted rejection, successful replacement pending plan, exact applied versions/history count, index integrity and listener cleanup. Existing non-cancellation migration failure tests still pass with FDB_MIGRATION.

This is a timed active cancellation probe, not exhaustive instruction-level or deadline qualification. Rust migration-wrapped Interrupt now exposes FDB_CANCELLED while preserving version/offset/source context. No upstream files or dependencies changed; broader cancellation/resource/platform qualification and full V1 remain incomplete.


## Cancellation queue and close — 2026-09-07

The focused real-worker persistent-close test passed (`/tmp/fastdb-cancel-close.log`). It covers active query cancellation plus queued cancellation for all other signalled operation classes, transaction observations, idempotent close, rejection of new work, listener disposal, dead interrupt handle, and reopening with only the committed document and valid index. The isolated transport test passed 256 signalled queued requests, queue-limit rejection without listener acquisition, slot retention after abort and complete token/listener release after response-channel failure (`/tmp/fastdb-cancel-queue.log`).

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 271 Rust tests, thirty Node tests and strict TypeScript checking (`/tmp/fastdb-cancel-lifecycle-check.log`). One known trigger-interruption gate remains ignored. These are additional bounded lifecycle probes, not exhaustive race/crash/platform qualification. Production code and upstream files were unchanged; full V1 remains incomplete.


## Native addon load diagnostics — 2026-09-07

The offline installed-package smoke passed on Linux x64, Node 24.19.0 (`/tmp/fastdb-native-loader-package.log`): eight exact package files, synchronous/worker query and type checks, then missing and invalid native-addon probes inside the temporary installation. Both failure probes require FDB_NATIVE_LOAD, platform identity, source-build guidance and an Error cause. The addon is restored before cleanup. The debug tarball measured 59,315,966 bytes in this run; this is not release artifact sizing qualification.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 271 Rust tests, thirty Node tests and strict TypeScript checking (`/tmp/fastdb-native-loader-check.log`). One known trigger-interruption gate remains ignored. No upstream files or dependencies changed, and nothing was published. Prebuild selection, other platforms/Node versions and full V1 remain incomplete.


## Installed-package cancellation APIs — 2026-09-07

The offline package smoke passed with the local Linux x64/Node 24.19.0 addon (`/tmp/fastdb-package-cancellation.log`). The separate consumer installation now checks pre-aborted execute/all/first/exactlyOne, batches, profiling, audits, imports, exports and migrations. Each rejection retains FDB_CANCELLED and active transaction observations. It verifies listener disposal, fresh-token batch success, late-cancellation isolation and the existing rollback/reopen checks. Consumer TypeScript compilation accepts all optional signal signatures and rejects a boolean migration signal.

This extends distribution evidence for the existing implementation; no production code changed. The smoke also retains exact file inventory and missing/invalid-addon diagnostics. Script syntax and git diff whitespace checks passed. The previously passing scoped baseline remains 271 Rust tests and thirty Node tests, with one known ignored trigger gate. No publishing occurred; prebuilds, other runtimes/platforms and full V1 remain incomplete.


## Initial native membership sources — 2026-09-07

The new native-membership differential matrix passes five native declarations, three RHS projection forms, three source predicates and both IN/NOT IN operators against six left values (90 query pairs). A first run exposed lost NOCASE collation; resolved source collation is now retained. The write/identity test passes record-shaped native binary separation, missing-parameter rejection, unique-failure rollback with prior indexed work and retry. Early test-only compilation mistakes used a private encoding API and then an unavailable dependency; the test now uses the existing literal fixture format. Focused results are `/tmp/fastdb-native-membership-expanded.log`.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript checking (`/tmp/fastdb-native-membership-check.log`). One known trigger-interruption gate remains ignored. No upstream source or dependency changes. Broader operand/CTE/correlation semantics, volatile evaluation, cancellation and materialization cost remain unqualified; full V1 remains incomplete.


## Native membership operand affinity — 2026-09-07

The expanded differential test passes 630 query pairs across eight left values (5,040 result cells per route), covering seven LHS forms, five native declarations, three RHS projections, three source predicates and both membership operators. The first run reproduced a CAST TEXT LHS versus unary-plus RHS mismatch. Restoring only LHS affinity was insufficient: the materialized computed RHS also required removal of artificial column affinity. Explicit CAST boundaries and computed RHS unary plus now preserve the tested baseline coercions (`/tmp/fastdb-membership-lhs-retest.log`).

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript checking (`/tmp/fastdb-membership-affinity-check.log`). One known trigger-interruption gate remains ignored. No upstream files or dependencies changed. Broader expression/CTE/correlation metadata, volatile evaluation, cancellation and performance qualification remain open; full V1 remains incomplete.


## Native membership cancellation checkpoints — 2026-09-07

The focused compound/subquery interruption suite passed with sixteen native IN/NOT IN combinations added to the existing matrix (`/tmp/fastdb-membership-cancel.log`). Each added case interrupts after exactly two or four source callbacks, checks FDB_CANCELLED, transaction state, preserved prior work, intact source and empty target indexes, then retries for exact rows and rolls back explicit transactions. Coverage spans read/insert and autocommit/outer-transaction cases.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript checks (`/tmp/fastdb-membership-cancel-check.log`). One known trigger-interruption gate remains ignored. This extends deterministic checkpoint evidence without qualifying all interruption points, source evaluation frequency or performance. No production/upstream changes; full V1 remains incomplete.


## Shared native membership source evaluation — 2026-09-07

A deterministic callback probe reproduced four native RHS evaluations for two source rows/two outer collection rows, versus two on the native route (`/tmp/fastdb-membership-evaluations.log`). Hoisting the CTE alone did not correct the pinned engine's execution behavior. The final implementation uses native IN/NOT IN against a shared enclosing source and materializes the left value separately. Source callbacks now match the native two-call baseline for both membership operators, and a volatile left expression runs exactly once per outer row.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript (`/tmp/fastdb-membership-shared-check.log`). This includes the 630-query-pair affinity matrix, binary/record identity and deterministic cancellation/retry coverage. One known trigger-interruption gate remains ignored. No upstream or dependency changes. Broader correlation/CTE/compound semantics and performance/resource qualification remain open; full V1 remains incomplete.


## Compound native membership compiler panic — 2026-09-07

Live Node probes aborted while preparing native membership projections in UNION ALL, including a case with membership in only one arm. Rust reproduced `No index cursor found for table t3` (`/tmp/fastdb-membership-compound-backtrace.log`). Name isolation and CTE hoisting did not fix it and were removed. The retained frontend change uses NOT MATERIALIZED for the correlated left-value CTE, avoiding premature materialization while keeping the shared RHS unchanged.

The minimal fix passed four set-operator comparisons (`/tmp/fastdb-membership-compound-minimal.log`). Full `fastdb/scripts/check.sh` passed formatting, Clippy, 274 Rust tests, thirty-one Node tests and strict TypeScript (`/tmp/fastdb-membership-compound-check.log`). Coverage includes unchanged source/LHS callback counts, the affinity matrix, cancellation retries, and sync/worker execution of the formerly crashing compound plus a native CTE query. One known trigger-interruption gate remains ignored. No upstream changes; broader nested/correlated metadata and full V1 remain incomplete.


## Outer CTE membership compound scope — 2026-09-07

A live probe failed to resolve an outer native CTE from membership subqueries in compound collection arms. Metadata preparation now receives enclosing CTE definitions, and generated arm CTEs are hoisted into the compound scope with separate membership names. The differential test passes parameterized outer CTE membership for UNION ALL, UNION, INTERSECT and EXCEPT. It also requires missing bindings to fail. The first full run still failed that assertion because native CTE consumed tracking only included rewritten binary parameters; logical compound validation now collects enclosing CTE parameter tokens explicitly. The corrected focused run passed (`/tmp/fastdb-membership-cte-final-focused.log`).

Final `fastdb/scripts/check.sh` passed formatting, Clippy, 275 Rust tests, thirty-one Node tests and strict TypeScript (`/tmp/fastdb-membership-cte-final-check.log`). One known trigger-interruption gate remains ignored. Temporary preparation logging was removed. No upstream files or dependencies changed; deeper nested/correlated scope and full V1 remain incomplete.


## CTE membership compound write atomicity — 2026-09-07

The focused INSERT SELECT regression passed both autocommit and explicit transactions (`/tmp/fastdb-cte-membership-insert.log`). It combines an outer parameterized native CTE, native membership and UNION ALL; checks FDB_PARAMETER for missing input, uniqueness-failure rollback with prior data/indexes retained, exact three-row retry and explicit rollback.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 276 Rust tests, thirty-one Node tests and strict TypeScript (`/tmp/fastdb-cte-membership-insert-check.log`). One known trigger-interruption gate remains ignored. No production/upstream changes; broader query/write combinations and full V1 remain incomplete.


## Nested membership CTE scope — 2026-09-07

Live probes found missing outer-CTE resolution inside derived membership queries and later CTE definitions. Passing native scope corrected the derived query; assembling generated definitions in resolved CTE order also corrected the later-CTE case. The regression covers both and a third CTE consuming the result (`/tmp/fastdb-nested-membership-retest.log`, expanded in the full run).

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 277 Rust tests, thirty-one Node tests and strict TypeScript (`/tmp/fastdb-nested-membership-check.log`). Existing CTE/compound, affinity, evaluation-count and cancellation suites passed. One known trigger-interruption gate remains ignored. No upstream or dependency changes. Broader deep/local-shadowing/correlated scope and full V1 remain incomplete.


## Same-name native CTE resolution — 2026-09-07

Nested same-name native CTE probes first exposed duplicate-name errors from flattened metadata scopes. Keeping those scopes nested removed that error but revealed differing selected definitions. A direct turso_core connection confirmed enclosing-native-definition resolution for the tested derived and later-CTE forms. The new regression compares both ordinary and collection routes to that raw baseline. Preserving enclosing definitions was initially too broad and regressed an existing logical collection CTE test; the rule is now restricted to inherited native CTEs.

Final `fastdb/scripts/check.sh` passed formatting, Clippy, 278 Rust tests, thirty-one Node tests and strict TypeScript (`/tmp/fastdb-membership-shadow-final-check.log`). Both the existing logical CTE scope test and the new raw-engine comparison passed. One known trigger-interruption gate remains ignored. No upstream or dependency changes. These pinned cases do not establish general SQLite shadowing compatibility; deeper/recursive scope and full V1 remain incomplete.


## Task-tracker application template — 2026-09-07

The executable Node template combines migrations, document validation, typed records/booleans, a managed index, one-hop owner expansion and document/relational transactions. The smoke passed title validation, deliberately conflicting event insertion after a task update, rollback/index integrity, retry, duplicate-completion rejection, linked listing, migration reopen and NDJSON export (`/tmp/fastdb-task-tracker.log`). The CLI ran twice against a temporary database and retained one completed task. Initial example assumptions about UPSERT DOCUMENT and SQL boolean literals were corrected to supported object UPSERT and bound booleans.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 278 Rust tests, thirty-two Node tests (31 binding tests plus the template) and strict TypeScript (`/tmp/fastdb-task-tracker-check.log`). One known trigger-interruption gate remains ignored. The template test now runs in the scoped Node check. No engine/upstream/dependency changes; external pilots and full V1 remain incomplete.


## AI application guide — 2026-09-07

The JavaScript code block in ai-application-guide.md was extracted directly from the file and executed with Node from the repository root. Its JSON result was parsed and checked for one completed task titled Review the schema with fetched owner Sam. The template smoke passed again (`/tmp/fastdb-ai-guide-template.log`); git diff whitespace checks passed. No production code changed, so the existing scoped baseline remains 278 Rust tests and thirty-two Node tests with one known ignored trigger gate. External pilots and full V1 remain incomplete.


## Incremental export encoding — 2026-09-07

The transfer integration suite passed after replacing full-rowset collection with a row callback and bounded writer (`/tmp/fastdb-export-stream.log`). A new unit test checks byte-for-byte JSON compatibility with the previous Bundle serializer, exact byte/document thresholds, zero/one/one-byte-short budgets, FDB_LIMIT, preserved active transaction data and JSON/NDJSON import/export equality. It uses small private test budgets rather than large CI allocations.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 279 Rust tests, thirty-two Node tests and strict TypeScript (`/tmp/fastdb-export-incremental-check.log`), including existing Rust/Node transfer cancellation and rollback tests. One known trigger-interruption gate remains ignored. No upstream/dependency/format changes. The complete output string, current row, buffer capacity and engine allocations still prevent a hard total-memory claim; broader resource qualification and full V1 remain incomplete.


## NDJSON import preflight/replay — 2026-09-07

The transfer integration suite passed with NDJSON linewise validation and transactional replay (`/tmp/fastdb-ndjson-import.log`). A new unit test appends malformed JSON, a non-object typed value and a noncanonical integer after a valid document. Every failure leaves engine total_changes unchanged and preserves prior outer work; valid retry and rollback pass. This demonstrates validation before mutations without retaining a full document vector. Parsing occurs twice, so no throughput improvement is claimed.

Full `fastdb/scripts/check.sh` passed formatting, Clippy, 280 Rust tests, thirty-two Node tests and strict TypeScript (`/tmp/fastdb-ndjson-replay-check.log`), including existing transfer cancellation/rollback coverage. One known trigger-interruption gate remains ignored. No upstream/dependency/format changes. JSON import materialization and broader memory/latency/platform qualification remain open; full V1 remains incomplete.


## Isolated transfer diagnostic — 2026-09-07

The new bench-transfer.cjs passed Node syntax checking and a complete default run on Linux x64/Node 24.19.0. Separate JSON/NDJSON child processes imported/exported 1,000 indexed documents with 4,096 ASCII text bytes each. Counts, numeric sums, text lengths, audit counts and export/import/export identity passed. Measurements precede correctness work, and source/addon/harness hashes are stored in benchmark-results/2026-09-07-linux-dev-transfer-1000.json. See benchmarks.md for cumulative peak-RSS and single-run limitations.

No production code changed; the scoped baseline remains 280 Rust tests and thirty-two Node tests with one known ignored trigger gate. No before-change binary comparison, release sizing, other platforms or full V1 completion is claimed.


## JSON import preflight/replay — 2026-09-07

The JSON envelope and document array now use serde visitors to validate and replay without retaining all documents. New unit tests cover late envelope/document failures with engine total_changes unchanged, reversed field order, exact 100,000-document acceptance and excess rejection. A real-engine integration regression checks that replay preserves FDB_CONSTRAINT, earlier outer work and index integrity, permits corrected retry and respects outer rollback.

- `fastdb/scripts/check.sh`: passed; log `/tmp/fastdb-json-replay-check.log`; 282 Rust passed, one ignored; 32 Node passed; strict TypeScript, formatting and scoped Clippy passed.
- The subsequently added integration test passed with `cargo test --locked -p fastdb-tests --test transfer`: four passed (`/tmp/fastdb-json-replay-integration.log`). Distinct current Rust coverage is 283. Final formatting and test-package Clippy were rerun for that addition.
- Prior transfer benchmark report predates this change and does not measure its performance. The complete input, current document and engine allocations remain outside a total-memory guarantee.


## JSON replay transfer benchmark — 2026-09-07

Ran `node fastdb/scripts/bench-transfer.cjs` three times against clean implementation 8e7b5893d. All six JSON/NDJSON samples passed aggregate, index and exact round-trip assertions. Stored report: benchmark-results/2026-09-07-linux-dev-transfer-json-replay-1000.json. Checked all source IDs, empty implementationChanges fields and addon/harness hashes against current files. Existing 283 distinct Rust / 32 Node scoped evidence is unchanged; no production code changed in this measurement task. The report supplies debug workload observations, not proof of release performance or total-memory limits.


## Forward-fetch encoded-value limits — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-fetch-budget-check.log`): formatting, scoped Clippy, 284 Rust tests, 32 Node tests and strict TypeScript; one known trigger-interruption gate ignored. The new real-engine unit regression uses private adjustable byte thresholds to check exact acceptance and one-byte-short rejection for collection and native targets with Unicode and duplicate expansion, plus null/empty output, retained active work, retry and outer rollback. Existing forward-link snapshot and query tests remain enabled. The 64 MiB limit counts logical tagged JSON value bytes separately for retained targets and expanded output. Current engine batches, reference keys, containers and outer-query materialization remain outside this accounting.


## Incremental forward-fetch target rows — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-fetch-stream-check.log`): formatting, scoped Clippy, 284 Rust tests, 32 Node tests and strict TypeScript; one known trigger-interruption gate ignored. Existing exact-byte-limit tests now exercise callback failure propagation for both collection and relational targets, including retained active work, retry and rollback. Existing multi-batch, order/duplicate and snapshot tests remain enabled. Collection/native target queries retain their 128-key grouping but no longer collect whole result batches before budget checks. No API, encoding or total-memory guarantee changed.


## Fetch target evaluation count — 2026-09-07

`cargo test --locked -p fastdb --lib links::tests` passed both tests (`/tmp/fastdb-fetch-evaluation.log`). The new target-row visitor regression counts actual native scalar calls at three budget boundaries, checks FDB_LIMIT, preserved active transaction state, exact retry results/evaluations and outer rollback. Production code is unchanged in this task, so the full scoped suite and Node rebuild were not repeated. Prior baseline: 284 Rust / 32 Node with one ignored gate; current distinct Rust coverage: 285. Scoped frontend Clippy and formatting checks cover the test addition.


## Shared SELECT fetch budget — 2026-09-07

`cargo test --locked -p fastdb-tests --test links` passed five tests (`/tmp/fastdb-fetch-shared-budget.log`). The new regression exceeds the 64 MiB expanded-value budget with two projections and 8,192 total references, then successfully retries one projection in the same transaction and rolls back prior work. Explicit aliases keep the fixture within the existing unique projection-name rules. Source inspection confirms execute_lowered_profiled flattens all fetched cells into one call; earlier documentation claiming separate per-projection budgets was corrected. Test-package Clippy and formatting passed. No production code changed; prior full 284 Rust / 32 Node evidence plus two subsequent new regressions gives 286 distinct Rust tests, with the same known ignored gate.


## Lowered SELECT row decoding — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-select-row-decode-check.log`): formatting, scoped Clippy, 286 Rust tests, 32 Node tests and strict TypeScript; one known trigger-interruption gate ignored. Lowered results now decode in engine callbacks, and fetched-reference counting stops before retaining the first excess decoded row. The scalar evaluation regression uses 24,576 rows, proves exactly 16,385 evaluations at the 16,384-position limit, and verifies active transaction state, two-row retry and rollback. Existing typed/compound/write/profile/client tests passed through the new path. Public results remain materialized; engine-side sorts/materialization and ordinary result bytes are not bounded by this change.


## Forward-fetch profiling — 2026-09-07

Forward SELECT profiles now use one atomic snapshot scope and expose separate target-batch, target-row and target-VM counters alongside unchanged primary counters. Rust tests cover collection/native batching and deduplication, stable repeat calls, missing targets, existing snapshots, budget failure and unchanged ordinary counters. Both Node clients verify bigint target counters and repeated results. A temporary-file CLI smoke asserted nonzero target work in the profile JSON.

- Final `fastdb/scripts/check.sh` run passed formatting, Clippy and 287 Rust tests with one known ignored gate (`/tmp/fastdb-fetch-profile-final-check.log`). Its added Node fixture initially used the forbidden AsyncDatabase constructor; corrected to await AsyncDatabase.open().
- `fastdb/scripts/check-node.sh` then passed all 33 Node tests and strict TypeScript (`/tmp/fastdb-fetch-profile-node-final.log`). Production code did not change after the Rust checks.
- The initial development run hit the prior explicit profiling-FETCH rejection; it was removed with the snapshot scope before final Rust verification.

Counters exclude catalog/schema/savepoint helpers, decoding and transport. They count physical engine work, not unique logical documents. Detailed target plans, complete helper accounting and broader profiling/cancellation/platform qualification remain open.


## Installed Node target-counter qualification — 2026-09-07

`node --check fastdb/scripts/check-node-package.cjs` and `node fastdb/scripts/check-node-package.cjs` passed (`/tmp/fastdb-package-fetch-profile.log`). The tarball was installed offline into a temporary consumer; sync and worker fetch profiles verified bigint target counters and fetched values, and strict TypeScript checked the installed declarations. Existing reopen, vector, cancellation and addon-load failure probes also passed. Report: Linux x64, Node 24.19.0, eight runtime files, 59,564,850 packed bytes. Temporary files were removed by the harness. No production changes; prior 287 Rust / 33 Node scoped evidence remains applicable.


## Fetch-profile interruption qualification — 2026-09-07

`cargo test --locked -p fastdb --lib links::tests` passed all three tests (`/tmp/fastdb-fetch-profile-interrupt.log`). The new test counts VM progress for a 130-target/two-batch profile and injects one-shot interrupts at step 1, quarter/half/three-quarter progress and one step before measured completion, under autocommit and active transaction modes. Every case fired, returned FDB_CANCELLED, preserved state/data and allowed equal rows/counters on retry; outer rollback removed prior uncommitted work. Scoped frontend Clippy and formatting passed. Production code is unchanged; no full-suite or Node rebuild repeated. Current distinct Rust coverage is 288, with the existing known ignored gate. These progress thresholds do not prove fixed latency or every engine phase.


## Forward-fetch diagnostic — 2026-09-07

`node --check fastdb/scripts/bench-fetch.cjs` and `node fastdb/scripts/bench-fetch.cjs` passed. Report: benchmark-results/2026-09-07-linux-dev-fetch-1000.json, clean implementation 23f7030c9. Twelve workloads each passed one warmup and three measured samples, checking all returned values, duplicates, expected batch counts and repeatable counters. Target metrics also match between one/two projections. No production changes or scoped-suite rebuild; prior test evidence remains applicable. The single-process debug fixture does not qualify memory, cold caches, optimized builds or platform performance.


## Current SQL gate probes — 2026-09-07

Reviewed clean 41a2daf95 and ran isolated BEGIN/ROLLBACK probes through the local addon over matching collection/native integer rows 1,2,3. Correlated scalar comparison succeeds natively but collection lowering reports no such table: d. Leading-WITH UPDATE succeeds natively with RETURNING 12 but is explicitly rejected for collections. The scalar grouping alias/HAVING probe agrees at (1,2). Updated v1-gates.md with exact queries and candidate-lowering requirements; the prior native-membership gap description was stale. No production changes or redundant scoped tests in this audit. These probes direct the next implementation work and do not establish release completion.


## Initial WITH UPDATE/DELETE — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-with-writes-check.log`): formatting, Clippy, 289 Rust tests, 34 Node tests and strict TypeScript; one known trigger-interruption gate ignored. New Rust cases use native/collection CTE candidates and compare mutations to an equivalent native chosen-value CTE oracle. They verify RETURNING/affected counts, self-read candidate materialization, missing parameters, uniqueness failure with prior work, integrity, retry and rollback. Existing mutation interruption coverage now includes both WITH write forms. Sync/worker clients verify updates, deletes, index integrity and rollback.

The same-name chained CTE development probe still fails preparation with no such column: n; it is documented as an open qualification gap rather than included in supported-case assertions. A native oracle reading a collection CTE crosses a separate unsupported native-write path; the final oracle uses an equivalent ordinary-table CTE. No upstream implementation files changed.


## Same-name CTE write oracle — 2026-09-07

`cargo test --locked -p fastdb-tests --test with_writes` passed two tests (`/tmp/fastdb-same-name-write-oracle.log`). The new ordinary-table regression asserts the pinned difference between candidate SELECT (row 2) and UPDATE/DELETE (all three rows) using a target-named CTE; it verifies affected counts and rollback restoration. Test-package Clippy and formatting passed. No production change; prior 289 Rust / 34 Node scoped evidence plus this new regression yields 290 distinct Rust tests. The collection failure remains unresolved, with the required write-context semantics recorded in v1-gates.md.


## Write-target alias oracle — 2026-09-07

Extended the existing native same-name regression with UPDATE target aliases. `cargo test --locked -p fastdb-tests --test with_writes` passed both tests (`/tmp/fastdb-with-alias-oracle.log`): native-named CTE + AS target updates only 2; target-named CTE + AS target updates all three rows. A live derived-candidate SELECT probe retained SELECT's one-row behavior. Inspected pinned update/delete/planner code to identify separate target and CTE reference scopes. Test-package Clippy and formatting passed. No production change; distinct test count remains 290 Rust, with prior 34 Node evidence. Collection same-name resolution remains open.


## CTE-only SELECT guard correction — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-cte-guard-final-check.log`): formatting, Clippy, 292 Rust tests, 34 Node tests and strict TypeScript; one known ignored gate. Final direct guard regression rerun: `/tmp/fastdb-cte-reserved-final.log`, passed. Frontend Clippy rerun covers the final reserved-name handling. Simple/chained/derived CTE reads and profiles now work when a CTE shares a collection name. Direct guard tests retain actual schema and internal references, including self/forward and nested physical references.

An initial integration assertion incorrectly expected valid EXPLAIN collection lowering to fail; protection was instead tested directly at guard_native_sql. Redaction affects only the guard AST; original SQL executes unchanged. Expression qualifiers and deeper scopes remain conservative, and the separate same-name write-context fix remains open.


## Qualified CTE guard references — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-cte-qualifiers-check.log`): formatting, Clippy, 292 Rust tests, 34 Node tests and strict TypeScript; one known ignored gate. Expanded existing tests cover unaliased CTE-qualified fields, stars, filtering, ordering and grouping/HAVING, plus both Node clients. Direct guard checks retain schema-qualified and nested physical references. Qualification proofs come from current FROM sources and do not cross expression subquery boundaries. Original SQL executes unchanged; explicit alias/deeper-scope guarding and same-name writes remain open.


## CTE source aliases in native guarding — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-cte-alias-check.log`): formatting, Clippy, 292 Rust tests, 34 Node tests and strict TypeScript; one known ignored gate. Existing CTE integration tests now cover explicit/implicit source aliases, including aliases sharing collection names, through execute and profile_select. Direct guard tests reject physical-table aliases, internal/reserved aliases and nested physical references. Alias redaction applies only to proven CTE FROM sources in the inspection AST. Same-name write semantics and deeper scope coverage remain open.


## Installed Node WITH-write qualification — 2026-09-07

`node --check fastdb/scripts/check-node-package.cjs` and `node fastdb/scripts/check-node-package.cjs` passed (`/tmp/fastdb-package-with-writes.log`). The temporary offline consumer tests both clients' WITH UPDATE/DELETE, RETURNING/affected counts, fetch profiles of updated values, empty collection/index integrity after delete, rollback to int64 max and a collection-named CTE alias. Existing installed declarations, cancellation, vector, reopen and addon-load checks also passed. Result: Linux x64, Node 24.19.0, eight runtime files, 59,569,700 packed bytes. No production change; prior 292 Rust / 34 Node scoped evidence remains applicable.


## Named-window CTE guard qualification — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-cte-window-check.log`): formatting, Clippy, 293 Rust tests, 34 Node tests and strict TypeScript; one known ignored gate. The new multi-row CTE window regression compares execute/profile results to the equivalent native CTE name. Final direct guard checks passed (`/tmp/fastdb-cte-window-guard-final.log`), including nested physical and schema-qualified references in named windows. Only PARTITION BY/ORDER BY qualifier roles are newly recognized; original SQL and pinned frame/window capabilities remain unchanged.


## Aliased collection CTE writes — 2026-09-07

`cargo test --locked -p fastdb-tests --test with_writes` passed five tests (`/tmp/fastdb-aliased-with-writes.log`). The new test compares UPDATE/DELETE using a physical-name CTE and distinct target alias against native equivalents, with parameters, qualified expressions, RETURNING, affected counts, integrity and rollback. Test-package Clippy and formatting passed. Production code is unchanged; prior 293 Rust / 34 Node full scoped evidence plus this regression yields 294 distinct Rust tests. Unaliased target-identifier collision remains open and was reconfirmed by a live probe.


## UPDATE subquery assignments — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-update-subqueries-check.log`): formatting, Clippy, 295 Rust tests, 34 Node tests and strict TypeScript; one known ignored gate. Candidate-assignment validation now defers subquery nodes to SELECT lowering while checking surrounding scalar expressions. Regression coverage includes native/collection scalar sources, WITH scalar sources, EXISTS/IN uniqueness conflicts, missing parameters, prior work, atomic retry, typed boolean validation and outer aggregate rejection. Cancellation coverage includes scalar and WITH-scalar assignment writes. Both Node clients exercise CTE scalar assignments.

A development oracle initially updated its source before the collection query read it, producing unequal starting data; the final comparison executes the collection route before the equivalent native mutation. VALUES/RETURNING restrictions and correlated assignment support remain unchanged.


## Nested membership assignment operands — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-nested-assignment-check.log`): formatting, Clippy, 296 Rust tests, 34 Node tests and strict TypeScript; one known ignored gate. A live probe showed scalar-subquery left operands of IN/NOT IN were rejected by assignment validation. Preserving a parenthesized validation wrapper lets the walker visit the nested root. New native UPDATE comparisons cover scalar and nested membership operands, NULL/empty sets, affected counts and rollback; outer aggregate assignments remain rejected with stored data unchanged. Original executable expressions are not altered by validation.


## Installed subquery assignments — 2026-09-07

`node --check fastdb/scripts/check-node-package.cjs` and `node fastdb/scripts/check-node-package.cjs` passed (`/tmp/fastdb-package-subquery-assignments.log`). The temporary offline consumer exercises bound scalar and nested membership UPDATE assignments through both clients, RETURNING, index consistency and rollback to int64 max. Existing installed declarations, vectors, cancellation, reopen and loader-error checks also passed. Result: Linux x64, Node 24.19.0, eight runtime files, 59,579,238 packed bytes. No production changes; prior 296 Rust / 34 Node scoped evidence remains applicable.


## Self-read and late assignment validation — 2026-09-07

`cargo test --locked -p fastdb-tests --test with_writes` passed all eight tests (`/tmp/fastdb-assignment-atomicity-final.log`). The added regression checks native-equivalent self-read assignment results and a CASE assignment that writes an earlier row before a later NULL fails integer validation. An increased total_changes counter establishes that the failure follows a write; collection integrity, prior outer work, successful retry and final rollback are checked. Production code is unchanged. Prior full scoped evidence of 296 Rust / 34 Node tests plus this regression yields 297 distinct Rust tests; the known trigger-interruption gate remains ignored.

Test-package Clippy with warnings denied, formatting and `git diff --check` also passed.


## Qualified native predicate correlation — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-correlated-predicates-check.log`): formatting, Clippy, 301 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored. Four new Rust regressions cover per-row scalar/EXISTS results, empty sources, alias shadowing, JOIN predicates, bindings, UPDATE candidates and validation rollback, binary equality and declared collation. The scalar comparison matrix covers 576 native/logical query pairs (three declarations, six projections, eight operators and four operand/collation forms), each with five outer values. Mutation cancellation additionally covers correlated scalar UPDATE and EXISTS DELETE, in autocommit and outer transactions. Both Node clients test reads/profiles and bound correlated updates with integrity/rollback.

An initial collation comparison failed because standalone inner projection metadata carried NOCASE into a correlated outer comparison. The pinned engine treats that correlated result without projected collation; the corrected lowering passes the matrix while preserving explicit outer COLLATE. Final added wrapper cases comparing a correlated scalar with a literal and another correlated scalar passed separately (`/tmp/fastdb-correlation-final-wrappers.log`). No upstream implementation or persisted format changes. General correlation, volatile evaluation and remaining release gates stay open.


## Installed predicate correlation — 2026-09-07

`node --check fastdb/scripts/check-node-package.cjs` and `node fastdb/scripts/check-node-package.cjs` passed (`/tmp/fastdb-package-correlated-predicates.log`). The temporary offline consumer verifies bound correlated UPDATE, scalar profile results, empty-result NULL, correlated EXISTS DELETE, index integrity and rollback through both clients. Existing installed declarations, cancellation, vectors, reopen and loader-error checks also passed. Result: Linux x64, Node 24.19.0, eight runtime files, 59,601,946 packed bytes. No production change; prior full scoped 301 Rust / 35 Node evidence remains applicable.


## Correlated scalar evaluation counts — 2026-09-07

`cargo test --locked -p fastdb typed_between_evaluates_volatile_left_operand_once` passed (`/tmp/fastdb-correlation-evaluation.log`). The existing counter regression now includes seven correlated scalar forms with direct native row/count oracles, followed by execute and profile_select comparisons. It establishes 2/1/0 calls for two/one/no matching outer rows, two calls through arithmetic and either comparison orientation, and four for two explicit occurrences across two rows. No production change or additional test function; prior full scoped evidence remains 301 Rust / 35 Node tests with one ignored gate. Broader volatile-expression qualification remains open.

Frontend all-target Clippy with warnings denied and formatting also passed.


## Correlated native HAVING — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-correlated-having-check.log`): formatting, Clippy, 302 Rust tests, 35 Node tests and strict TypeScript; one known trigger-interruption gate remains ignored. A new regression compares aggregate/grouped scalar and EXISTS HAVING predicates with native tables through execute/profile, including HAVING projection aliases, local alias shadowing, parameters and empty results. It checks failed UPDATE rollback, prior work, integrity and corrected retry. Both Node clients additionally exercise a correlated HAVING alias read/profile. Initial live probes had rejected the collection forms while native queries returned per-row results; those cases now pass. GROUP BY expression correlation and broader nested/native/collection scope qualification remain open.


## Correlated native membership — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-correlated-membership-check.log`): formatting, Clippy, 305 Rust tests, 35 Node tests and strict TypeScript; one known trigger-interruption gate remains ignored. Three new regressions cover 32 NULL/empty/left-operand query pairs through execute/profile, a scalar affinity/collation matrix, parameter and atomic INSERT SELECT failure/retry, and binary/record identity. The final matrix includes a text-literal LHS (120 query pairs over five outer values); this expansion passed separately (`/tmp/fastdb-correlated-membership-final.log`). The existing native counter test verifies three RHS function calls across two correlated outer rows for both IN and NOT IN, matching native execute/profile behavior. Mutation cancellation includes a correlated membership predicate; both Node clients read and profile correlated membership.

A native probe showed an enclosing WITH source cannot resolve the correlated outer alias, while a local CTE inside the membership expression can. The lowerer therefore leaves uncorrelated sources shared and places correlated sources locally. Source evaluation stays in the engine. Inner collection/deeper scopes and broader volatile/planner/resource qualification remain open; no upstream implementation or persisted encoding changes.


## Installed correlated membership — 2026-09-07

`node --check fastdb/scripts/check-node-package.cjs` and `node fastdb/scripts/check-node-package.cjs` passed (`/tmp/fastdb-package-correlated-membership.log`). Both installed clients exercise bound correlated IN updates, profiled match/empty/NULL results, index integrity and rollback to int64 max. Existing installed declarations, cancellation, vectors, reopen and loader-error checks also passed. Result: Linux x64, Node 24.19.0, eight runtime files, 59,608,392 packed bytes. No production change; prior full scoped 305 Rust / 35 Node evidence remains applicable.


## Correlated nested paths and derived parents — 2026-09-07

`fastdb/scripts/check.sh` passed (`/tmp/fastdb-correlated-paths-qualified-check.log`): formatting, Clippy, 307 Rust tests, 35 Node tests and strict TypeScript; one known trigger-interruption gate remains ignored. New integration coverage compares two nested path depths across scalar/EXISTS/IN/NOT IN, HAVING and JOIN predicates, five parent/value cases, derived sources, direct nested projections, local alias shadowing and UPDATE integrity/rollback. Native scalar columns supply equivalent NULL/value oracles. A new direct accessor unit verifies NULL/scalar/array parents yield missing values only through the derived accessor, while stored accessors reject non-object roots and all accessors reject malformed encodings.

Live probes initially failed metadata preparation on nested outer paths. Qualifier recognition fixed those cases; derived-source qualification then exposed the existing accessor's object-root assumption. A dedicated nested-value accessor fixes that read behavior without relaxing physical document validation. The added direct projection uses explicit aliases to satisfy the existing unique-output-name contract. No upstream implementation or persisted-format changes; broader query scope/type/resource release gates remain open.


## Source-free deep paths, installed accessors and nested savepoint cancellation — 2026-09-07

`fastdb/scripts/check.sh` passed (/tmp/fastdb-unique-savepoints-check.log): formatting, Clippy, 308 Rust tests, 35 Node tests and strict TypeScript. The existing nested-path regression now includes source-free scalar, EXISTS and IN over direct/derived collections. The installed smoke initially exposed premature deep-path lowering without an outer scope; deferring that marker lets the enclosing correlation pass resolve it. The final installed-package smoke passed (/tmp/fastdb-package-nested-paths-fixed.log): Linux x64, Node 24.19.0, eight runtime files, 59,642,181 packed bytes. Both clients check nested object/NULL/scalar/array/missing parents, profiles, index integrity and rollback.

During scoped verification, a cancelled import retained two imported rows. A timing-based retry passed, but a new deterministic 48-boundary regression reproduced the root cause: reused savepoint names let cleanup stop at an unfinished inner frame. Unique names fix that failure. The Node transfer cancellation test passed three additional runs (/tmp/fastdb-transfer-cancel-repeated.log). See atomic-savepoints.md for exact before/after evidence and remaining RELEASE/top-level-open qualification. The separate known trigger-interruption gate remains ignored. No upstream implementation or persisted format changes; full V1 remains incomplete.


## Cancelled atomic opening — 2026-09-07

`fastdb/scripts/check.sh` passed (/tmp/fastdb-atomic-open-check.log): formatting, Clippy, 309 Rust tests, 35 Node tests and strict TypeScript; one known trigger-interruption gate remains ignored. The new opening sweep reproduced boundary 4 leaving an autocommit connection active without running its callback (/tmp/fastdb-atomic-open-expanded.log). Failed opens now roll back/release their unique frame, accepting only the pinned TxError for an absent generated frame. The regression verifies cancellation before callback execution in both initial transaction states, checks no named frame remains, preserves prior rows and retries a write successfully. Targeted opening/nested/import regressions also passed (/tmp/fastdb-atomic-open-qualified.log).

The persistent-interrupt export fixture intentionally prevents cleanup as well as work; it now expects FDB_ROLLBACK with an active outer transaction, while one-shot cancellation remains FDB_CANCELLED after successful cleanup. Other cleanup errors are not suppressed. Opening I/O failures, RELEASE/commit ambiguity and the separate trigger defect remain release qualification work. No upstream implementation or persisted format changes.


## RELEASE progress-boundary qualification — 2026-09-07

`cargo test --locked -p fastdb atomic_release_cancellation` passed (/tmp/fastdb-release-delivery.log). A new test sweeps 16 post-write thresholds in each transaction mode, verifies callback delivery, and asserts exact pinned outcomes: three restored FDB_CANCELLED cases per mode, one complete pending FDB_ROLLBACK case in the outer transaction, and success before delivery at later thresholds. It checks initial transaction mode, complete-versus-restored rows and explicit outer rollback. Initial diagnostic output is /tmp/fastdb-release-boundaries.log. No production changes; prior full scoped 309 Rust / 35 Node evidence plus this regression yields 310 distinct Rust tests. The known trigger gate and broader I/O/commit qualification remain open.

Frontend all-target Clippy with warnings denied, formatting and diff checks also passed.


## Installed nested import cancellation — 2026-09-07

`node --check fastdb/scripts/check-node-package.cjs` and `node fastdb/scripts/check-node-package.cjs` passed (/tmp/fastdb-package-savepoint-cancel.log). The temporary installed worker consumer cancels a 1,000-document JSON import with a 20 ms timer, checks FDB_CANCELLED/active state, prior rows, integrity and listener cleanup, then imports all 1,000 documents with a fresh operation and rolls back to the caller savepoint. Existing sync/worker, declaration, loader, vector, reopen and nested-path checks also pass. Result: Linux x64, Node 24.19.0, eight runtime files, 59,653,802 packed bytes. The timer does not prove a specific engine interruption phase. No production changes; prior Rust/Node evidence remains applicable.


## Completed 100,000 × 768 vector diagnostic (2026-09-07)

The seeded-vector benchmark at clean commit `bc88ad618b` exited successfully, checking filter counts/index use and all warmup/measured exact top-10 results against an independent float32-coordinate cosine reference. Debug CLI medians: 82.45 s unindexed filter, 1.12 s indexed filter, 388.11 s exact top-10; three measured samples per workload. Report identities, medians and repeated primary counters were verified. See [benchmarks.md](benchmarks.md) for the raw report, command and the nonmonotonic VmHWM accounting caveat. Optimized/real-distribution/platform/resource qualification remains open; this diagnostic does not close the vector release gate or full V1.


## Optimized vector diagnostic (2026-09-07)

The existing release-profile CLI built successfully with Rust 1.88.0 and completed the same seeded 100,000 × 768 diagnostic at clean commit `83583ecdd`. All warmup/sample count and cosine-reference checks passed. Medians were 9.48 s unindexed filter, 138.64 ms indexed filter and 47.12 s exact top-10. Reference data, engine counters and database size match the debug run; binary identity and medians were verified. See [benchmarks.md](benchmarks.md) for commands, comparison and recurring nonmonotonic VmHWM observations. This adds optimized-build evidence but leaves real workloads, broader platform/scale/resource qualification and full V1 incomplete.


## Combined vector-field conversion (2026-09-07)

Plain physical collection fields now bypass the intermediate tagged-value serialization/decode when passed to vector functions. Complete document validation and existing conversion errors remain enforced; other expressions retain generic conversion. Direct equivalence coverage includes five vector encodings, binary/text input, NULL/missing/scalar-parent paths and malformed stored data. Full scoped checks passed: 311 Rust tests, 35 Node tests, formatting, Clippy and strict TypeScript; one existing trigger gate remains ignored. Initial optimized diagnostic results and limitations are in [accessor-performance.md](accessor-performance.md). Full V1 and broader performance qualification remain open.


## Combined accessor large-vector verification (2026-09-07)

The release-profile 100,000 × 768 seeded run at clean `3cac94cae` passed all warmup/sample reference checks. Exact top-10 median was 32.71 seconds versus 47.12 seconds before; primary VM steps fell from 1,200,080 to 1,100,080 while all 100,000 vectors are still scanned. Binary identity, reference values, sample medians and repeated counters were checked. See [benchmarks.md](benchmarks.md) for report, command and measurement limitations. Substantial latency, whole-document decoding, real-workload/platform/resource qualification and full V1 remain open.


## Installed vector-field conversion smoke (2026-09-07)

The offline installed Node consumer now stores all five vector encodings, compares profiled field extraction with bound-parameter extraction, verifies typed round trips and collection integrity, and rolls the writes back through sync and dedicated-worker clients. A separate missing-vector rejection check retains the original document. The initial fixture attempted rollback after a rejected vector SELECT had left the pinned engine in autocommit; the corrected fixture checks successful-write rollback before exercising rejection. No production behavior changed for this fixture correction.

The complete offline package smoke passed on Linux x64 / Node 24.19.0: eight runtime files, 59,657,949 packed bytes. This is local installed-addon evidence; broader release/platform qualification and full V1 remain open.


## Vector conversion error transaction disposition

A differential regression now checks raw engine NULL, malformed stored bytes and a stored NULL field through both generic and combined vector-input accessors, from autocommit and an active outer transaction. Both paths return identical error text, leave autocommit, preserve committed rows, discard the pending outer write, and allow a subsequent write. Both vector-field unit tests passed. This pins the observed engine-abort behavior for these conversion failures; it does not promise statement-only rollback for every SELECT error. No production behavior changed. The prior full suite has 311 passing Rust tests; this adds one targeted passing regression.


## Qualified outer fields in native subquery projections (2026-09-07)

Simple native inner SELECT projections now lower qualified outer collection fields in addition to the existing predicate positions. Correlated logical projections retain typed results instead of being packed as native scalars; membership sources use the existing logical comparison conversion inside their correlated expression. Explicit CAST results, including parentheses/COLLATE wrappers, retain the native scalar route so outer comparisons preserve cast affinity. Local aliases continue to shadow outer aliases. Execution stays inside the engine and varies with the outer row.

Regression coverage includes scalar arithmetic and aggregates, EXISTS/IN, direct boolean/record/object/array values, NULL/empty sources, bound and missing parameters, typed membership, alias shadowing, profiled comparisons and atomic UPDATE uniqueness failure/retry/rollback with prior outer work. A seven-projection/five-operator/four-RHS comparison matrix uses native typeless columns as the collection-field oracle; declared native INTEGER columns have different affinity and are not that oracle. The matrix exposed and verified the CAST-affinity fix.

The complete scoped suite passed formatting, Clippy, 315 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored. Inner collection correlation, deeper/local-WITH/compound correlated scopes, correlation in grouping/window/order/limit expressions and broader volatile/type/resource qualification remain open. Full V1 is incomplete.


## Installed correlated-projection smoke (2026-09-07)

The offline installed Node consumer now runs a parameterized correlated-projection UPDATE, profiles record/integer projections and record membership, checks explicit CAST affinity and empty scalar NULL, audits the managed index and rolls back to int64 max through both sync and worker clients. The complete package smoke passed on Linux x64 / Node 24.19.0: eight runtime files and 59,662,439 packed bytes. Broader platform/release qualification and full V1 remain open.


## Qualified outer fields in native subquery ordering (2026-09-07)

Simple native inner SELECT ORDER BY expressions now rewrite qualified outer collection fields using the existing correlation scope. Disposable metadata probes replace those references with NULL; execution preserves the per-outer-row ordering inside the engine. Differential execute/profile coverage includes scalar, IN and EXISTS forms, ascending/descending expressions, NULLS LAST and multiple sort keys with LIMIT 1.

The complete scoped suite passed formatting, Clippy, 316 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored. Pinned native probes reject outer references in the tested GROUP BY and LIMIT positions; this change does not add those forms. Inner collection/deeper/local-WITH/compound correlation, broader ordering/type/alias cases and full V1 remain incomplete.


## Correlated typed projection sort reuse (2026-09-07)

Single-column correlated native subqueries now sort covered typed projection aliases and ordinals by logical values. Native scalar projections retain engine alias reuse; typed CASE/coalesce projections pass through a lazy local CTE with a flattening barrier, so sorting reuses the projected value. Mixed ordinary sort keys travel through that boundary as additional internal columns. LIMIT/OFFSET remain on the outer sort, including zero-limit short-circuiting.

Differential native-table tests cover negative and multi-digit integers, ascending/descending aliases and ordinals, parentheses, explicit BINARY collation, mixed keys and offsets. A registered volatile function verifies native evaluation counts through execute/profile for scalar arithmetic, typed CASE, mixed keys and LIMIT 0. Mixed DISTINCT ordering involving a typed projected alias and an additional ordinary key is explicitly unsupported: adding that key to DISTINCT would change duplicate elimination. General correlated alias expressions, DISTINCT/type semantics and broader scope/resource qualification remain open.

Verification: the complete scoped check passed formatting, Clippy, 318 Rust tests, 35 Node tests and strict TypeScript. One previously recorded trigger-interruption gate remains ignored. No upstream core files changed. Full V1 remains incomplete.


## Sorted correlated subquery consumers (2026-09-07)

Additional native differential coverage verifies sorted typed CASE/coalesce subqueries consumed by IN, NOT IN and EXISTS, including mixed numeric/text/NULL inputs, explicit NOCASE collation, alias/ordinal/mixed ordering, zero limits and offsets. Both execute and profile_select match the native table oracle. Separate assertions verify that sorted correlated record and boolean projections retain their logical values and membership identities. All 15 correlated scalar-subquery integration tests pass. This extends qualification of the existing lowering; broader correlated alias/DISTINCT/resource semantics remain open and V1 remains incomplete.


## Correlated sort alias expressions (2026-09-07)

Correlated typed projection aliases now expose logical scalar values inside covered ORDER BY arithmetic and function expressions, including `x+0` and `abs(x)`. Previously these expressions operated on the encoded projection and could choose 2 ahead of 10 in descending order. Root alias/ordinal sorting retains the existing projection-reuse boundary; expression aliases follow native evaluation behavior. Nested subquery scopes are excluded from this substitution.

Native differential tests cover alias expressions with LIMIT/OFFSET and name collisions across tables, views and inherited CTEs. The pinned engine gives projected aliases precedence over same-named input columns in these ORDER BY expressions. A volatile CASE projection confirms the native eight-call count through execute and profile_select. Mixed DISTINCT sorting and broader correlated scope/type/resource qualification remain open; full V1 is incomplete.

Verification: the complete scoped suite passed formatting, Clippy, 321 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored.


## Correlated sort alias write atomicity (2026-09-07)

A parameterized correlated CASE projection ordered through `abs(x)` now has write-path regression coverage. A multirow UPDATE that collides on a managed unique index restores both original values and index contents, preserves prior work, and reports unchanged transaction state in autocommit and explicit transactions. Retrying with a non-colliding parameter returns 11 and 12 and supports indexed lookup; outer rollback restores the original documents and removes the prior native-table insert. Integrity audits verify the failure, retry and rollback states. The focused real-engine regression passes; this test-only change does not establish broader correlated write or recovery qualification. Full V1 remains incomplete.


## Mixed DISTINCT correlated ordering (2026-09-07)

The previous rejection for mixed DISTINCT ordering of a single correlated typed projection is removed. DISTINCT now applies to the public projected value outside the lazy projection boundary, so additional internal sort columns do not participate in duplicate elimination. Ordering and pagination remain on that outer query.

Native differential execute/profile tests cover constant and varying typed CASE results, repeated inputs, alias/source-key order permutations, alias function expressions, LIMIT 0 and offsets past the distinct result set. Volatile projection and secondary-sort probes check native evaluation counts, including zero-limit short-circuiting. This supersedes the earlier mixed-ordering rejection; broader DISTINCT equality/collation/type semantics and correlated scope/resource qualification remain open. Full V1 is incomplete.

Verification: the complete scoped check passed formatting, Clippy, 322 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored.


## DISTINCT correlated consumer qualification (2026-09-07)

The mixed DISTINCT correlated-ordering matrix now compares scalar, IN, NOT IN and EXISTS consumers against native tables, including repeated NULL inputs, constant/varying CASE projections and empty pages after LIMIT/OFFSET. Execute and profile_select agree with the native oracle. Record and boolean assertions additionally verify that an offset past the single distinct value returns scalar NULL and false membership even when multiple native source rows exist. All 17 correlated scalar-subquery integration tests pass. This extends regression coverage of the existing implementation; broader DISTINCT type/collation semantics and full V1 remain open.


## Logical equality in correlated DISTINCT ordering (2026-09-07)

Sorted single-column correlated typed DISTINCT projections now group by the unwrapped logical SQL value while returning a typed representative, matching the existing collection DISTINCT strategy. Previously encoded integer 1 and real 1.0 survived as separate rows, causing OFFSET 1 to return 1.0 instead of 2. Hidden sort keys remain outside the grouping key.

Native differential execute/profile coverage includes both integer/real insertion orders, duplicate NULLs, ascending/descending and mixed sort keys, offsets through and beyond the result set, and scalar/membership consumers. The focused numeric-equality regression passes. Broader correlated DISTINCT collation/type semantics, unsorted correlated DISTINCT and general scope/resource qualification remain open; full V1 is incomplete.

Verification: the complete scoped suite passed formatting, Clippy, 323 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored.


## Explicit correlated DISTINCT collation probes (2026-09-07)

A native differential regression now covers BINARY/NOCASE on the outer CASE projection and within its selected branch, together with alias sorting, an explicit BINARY descending sort override, mixed source keys and offsets across the result set. Inputs include `a`, `A` and `b`. Execute and profile_select match the pinned native results for all covered combinations; no production change was needed. This records explicit-expression collation evidence, not general implicit-column or deeper-scope collation qualification. Full V1 remains incomplete.


## Bound pagination in sorted correlated projections (2026-09-07)

The installed-package smoke exposed bound LIMIT 0 returning a row in a sorted correlated DISTINCT projection. The pinned engine's row-value subquery lowering replaces non-literal limits with an implicit LIMIT 1 (`core/translate/subquery.rs`); native preparation can consequently discard the limit bind. FastDB now keeps the covered sorted typed projection's pagination inside a derived relation, leaving scalar cardinality outside that relation. No upstream files changed.

A parameter matrix compares bound limits 0, 1, 2 and -1 and offsets 0, 1 and 4 with literal native pagination through scalar, IN and EXISTS execute/profile consumers. The installed Node smoke additionally exercises alias-expression UPDATE/RETURNING, integer/real DISTINCT pagination, an empty record DISTINCT page, integrity and rollback through synchronous and worker clients. Other native/correlated pagination forms and full V1 remain open.

Verification: the complete scoped check passed formatting, Clippy, 325 Rust tests, 35 Node tests and strict TypeScript, with one known trigger-interruption gate ignored. The rebuilt offline installed-package smoke passed on Linux x64 / Node 24.19.0: eight runtime files and 59,695,955 packed bytes. Broader platform/release qualification remains open.


## Correlated pagination rejection and retry (2026-09-07)

A real-engine UPDATE regression now verifies that bound NULL, fractional, invalid-text and array LIMIT/OFFSET values reject the covered correlated DISTINCT source without changing documents, managed indexes, prior native-table work or observed transaction state. Missing parameters report FDB_PARAMETER. Both autocommit and explicit transactions permit a valid retry returning the expected updated values; outer rollback restores the original document IDs/values and removes prior pending work. Integrity audits cover rejection, retry and rollback states. The focused regression passes; this test-only qualification does not close broader pagination or recovery gates. Full V1 remains incomplete.


## Pagination across supported native correlation forms (2026-09-07)

The pagination boundary now also covers supported predicate-only/native scalar, CAST and typed CASE correlated sources, rather than only typed projection-alias sorting. Scalar and EXISTS consumers preserve requested pagination inside a relation. Native membership retains its existing correlation structure; adding another derived wrapper there produced wrong per-outer-row membership in a probe.

Integer LIMIT/OFFSET parameters in supported correlated sources are lowered to parsed SQL integer literals, avoiding a bound-counter reuse issue that made the second outer row miss an expected native membership match. Native ordinary SQL delegation is unchanged. Differential execute/profile tests compare scalar, IN and EXISTS with literal-native pagination across four projection forms, limits 0/1/2/-1 and offsets 0/1/2/4. These probes pass; other parameter types, complex pagination expressions and broader correlation/resource qualification remain open. Full V1 remains incomplete.

Verification: the complete scoped check passed formatting, Clippy, 327 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored.


## Reused correlated pagination bindings (2026-09-07)

A native differential regression verifies that lowering an integer pagination bind does not consume its use in a correlated WHERE predicate. Named `$count` and numbered `?1` parameters are reused across the predicate and LIMIT or OFFSET with values 0 through 3. Scalar, IN and EXISTS execute/profile consumers match literal-native results for both outer rows; omitted parameters still report FDB_PARAMETER. The focused real-engine regression passes. Other parameter types/expressions and full V1 remain open.


## Integral real pagination binds (2026-09-07)

Supported correlated pagination now normalizes integral real binds to integer literals as well as int64 binds. A real LIMIT 1.0 previously reused the bound counter and missed membership for the second outer row. Conversion follows the pinned engine's exact real-to-integer limits: fractional/non-finite values and both int64 endpoints are excluded, including the exactly representable negative endpoint. Those values continue to engine validation rather than being truncated or saturated.

Differential execute/profile coverage compares real and integer pagination through scalar, IN and EXISTS consumers, including zero/negative limits, offsets and large accepted values near both endpoints. Explicit endpoint probes retain rejection for scalar and membership sources. Other coercions and complex pagination expressions remain open; full V1 is incomplete.

Verification: the complete scoped check passed formatting, Clippy, 329 Rust tests, 35 Node tests and strict TypeScript. One known trigger-interruption gate remains ignored.


## Reused real pagination parameter identity (2026-09-07)

A regression verifies that normalizing integral real pagination binds does not change the same parameter's type elsewhere. Named `$count` and numbered `?1` binds are simultaneously projected, inspected with typeof, used in a correlated predicate and supplied to LIMIT. Values 0.0, 1.0 and 2.0 retain real projections/type names while producing the expected per-outer-row membership results through execute and profile_select. The focused real-engine test passes. Full V1 remains incomplete.


Negative-offset qualification (2026-09-07): the native correlated integer-pagination matrix and real/integer equivalence matrix now include OFFSET -2 through execute/profile scalar, IN and EXISTS consumers. Results match the native negative-offset behavior. All nine pagination integration tests pass, including scope, binding reuse and write failure/retry cases. Full V1 remains incomplete.


## Async worker startup failure qualification (2026-09-07)

The isolated transport fixture now covers error, early exit and message-decoding failure before the ready handshake. AsyncDatabase.open rejects with FDB_WORKER only after worker exit; a damaged response channel requests one close. A real-worker regression repeats failure to open a missing-parent path three times, then successfully creates, writes, closes and reopens another database. All 35 Node binding tests pass, including the transport fixture. This is startup lifecycle qualification; it does not establish native crash recovery or broader platform release readiness. Full V1 remains incomplete.


Worker close-failure qualification (2026-09-07): the isolated transport fixture now covers a failed close send and exit before close acknowledgement with pending cancellable work. Both operation and close reject with the same FDB_WORKER error, cancellation tokens/listeners are released, subsequent requests retain that failure, and close remains promise-idempotent. The fixture and its timeout/completion-marker wrapper pass. This establishes transport lifecycle behavior, not native interrupted-close durability. Full V1 remains incomplete.


## Public Node closed-handle errors (2026-09-07)

Public Database and AsyncDatabase operations now report FDB_CLOSED after close; worker submissions during closing use the same code. The error has no transaction field because no statement was submitted. Synchronous access checks the wrapper's closed state before invoking the native handle. Close remains idempotent, and an established worker failure retains FDB_WORKER precedence. JavaScript argument validation and constructor failures retain their existing contract.

Regression coverage exercises execute, row helpers, profiling, batches, integrity inspection, transfers and migrations across both closed clients, plus submissions while closing. All 37 Node binding/application tests and strict TypeScript pass. Native interrupted-close durability and broader release qualification remain open; full V1 is incomplete.

The offline installed Node package smoke also passed FDB_CLOSED assertions for both clients on Linux x64 / Node 24.19.0: eight runtime files and 59,805,368 packed bytes. No publishing occurred.


Closed-worker cancellation qualification (2026-09-07): the transport fixture verifies that an operation submitted during close with an AbortSignal returns FDB_CLOSED without another worker message or retained abort listener. An already-aborted signal submitted after close behaves the same way. The timeout/completion-marker test wrapper passes. Full V1 remains incomplete.


## Node cardinality error observations (2026-09-07)

Synchronous and worker exactlyOne helpers now retain RangeError while attaching FDB_CARDINALITY and the completed execute result's transaction observations. The code matches Rust's cardinality error. The helper checks rows after successful statement execution; it does not undo writes or roll back a transaction. Closed-handle and engine failures still propagate through execute.

Both-client regressions cover empty reads, autocommit INSERT RETURNING with two rows, an UPDATE RETURNING mismatch inside an explicit transaction, managed index integrity, explicit rollback and a successful single-row retry. All 38 Node/application tests and strict TypeScript pass. Broader error and release qualification remains open; full V1 is incomplete.

The offline installed-package cardinality assertions passed through both clients on Linux x64 / Node 24.19.0: eight runtime files and 59,805,622 packed bytes. No publishing occurred.


Cardinality-helper error precedence (2026-09-07): both Node clients retain FDB_CONSTRAINT and active transaction observations when exactlyOne executes a rejected unique write. A pre-aborted worker exactlyOne retains FDB_CANCELLED with active state, and a subsequent read succeeds. Index integrity remains valid after rollback. Both focused exactlyOne tests pass; full V1 remains incomplete.


## Typed Node error recognition (2026-09-07)

The Node package now exports FastDBError (Error with a string code and optional transaction observations) and isFastDBError(unknown), a runtime predicate and TypeScript type guard. It recognizes Error instances with FDB-prefixed identifier codes and validates any before/after observations as active/autocommit. Plain objects, uncoded errors and malformed transaction observations are rejected. The guard supports errors without observations, including FDB_CLOSED.

Both-client runtime tests cover execution, cardinality and closed-handle errors; strict TypeScript checks narrowing from unknown and optional transaction access. All 40 Node/application tests and strict TypeScript pass. This improves public error handling without making every constructor/argument error a database error; broader release qualification and full V1 remain open.

Installed-package runtime guard assertions and TypeScript narrowing passed on Linux x64 / Node 24.19.0: eight runtime files and 59,806,060 packed bytes. No publishing occurred.


Transport error-guard qualification (2026-09-07): the isolated worker fixture now verifies isFastDBError for shared FDB_WORKER failures and FDB_LIMIT queue rejections without transaction observations. Existing cancellation cleanup and timeout/completion-marker assertions pass. Full V1 remains incomplete.


Task-tracker initialization cleanup (2026-09-07): openTracker now preserves both migration and close failures in AggregateError, matching the example's existing transaction-cleanup policy. Successful cleanup rethrows the original migration error. A simulated-client regression verifies error identity/order and exactly one close attempt; both application tests pass, including real persistent atomic task completion. Native interrupted-close durability and full V1 remain open.


Task transaction cleanup qualification (2026-09-07): simulated statement failures verify that completeTask does not issue ROLLBACK after a rejected BEGIN, attempts cleanup after UPDATE/INSERT/COMMIT failures, and retains original plus rollback errors in order. All three application tests pass, including the real-engine persistence and atomic completion case. Simulated commit failure is control-flow evidence, not proof of a native commit outcome. Full V1 remains incomplete.


## Bundled normalization output overflow (2026-09-07)

A multirow collection UPDATE regression exercises NFKD expansion beyond the bundled output limit. The failure leaves documents and managed indexes intact, but the pinned engine rolls back the entire active transaction, including earlier ordinary-table writes; execute_report observes active → autocommit. An ordinary-table SELECT invoking the same helper confirms this transaction disposition. This is a helper-query comparison, not ordinary UPDATE namespace support. A valid retry in a new transaction succeeds, and explicit rollback restores the original collection values with a clean integrity audit.

The preceding complete scoped check passed 330 Rust tests and 42 Node/application tests, formatting, Clippy and strict TypeScript, with one known trigger-interruption gate ignored. The additional overflow regression passed separately with all three bundled integration tests and focused Clippy. Broader QuickJS runtime/platform/performance qualification and full V1 remain open.


## Correlated DISTINCT independent of projection sorting (2026-09-07)

Single-column correlated typed DISTINCT now uses logical-value grouping even when ORDER BY refers only to a native source column or is absent. Previously this boundary was enabled only by a sort naming the typed projection alias/ordinal, allowing integer 1 and real 1.0 to survive as separate rows and produce incorrect pagination. The existing lazy projection boundary retains typed representatives and excludes hidden sort keys from equality.

Native differential scalar/membership tests now cover ascending and descending source-column ordering with numeric equivalents and duplicate NULLs. An unordered scalar/IN/EXISTS regression checks exhaustion after all three logical values through execute and profile_select without asserting an unspecified row order. Broader correlation, collation/type/resource qualification and full V1 remain open.

Verification: the complete scoped check passed 332 Rust tests, 42 Node/application tests, formatting, Clippy and strict TypeScript. One known trigger-interruption gate remains ignored. No upstream core files changed.


Correlated source-sorted DISTINCT qualification (2026-09-07): a native differential matrix verifies integer and integral-real LIMIT/OFFSET bindings across scalar, IN and EXISTS consumers, including zero/unbounded limits and exhausted pages through execute/profile_select. A multirow UPDATE skips duplicate numeric equivalents, stores the expected values, retains managed-index integrity and restores the original records on explicit rollback. The focused regression, formatting and focused Clippy pass. This adds one test after the latest complete 332-Rust/42-Node run; broader V1 qualification remains open.


Correlated DISTINCT collation qualification (2026-09-07): the existing explicit BINARY/NOCASE projection and CASE-branch matrix now covers native source-column sorting, with a BINARY tie-breaker after NOCASE sorting. Scalar, IN (including explicit left-side NOCASE) and EXISTS results match the pinned native engine through execute and profile_select at each tested offset. NOCASE-only ties do not promise an order between a and A. The expanded focused regression, formatting and focused Clippy pass; this is additional coverage of the existing implementation, not complete collation or V1 release qualification.


Worker argument-byte budget qualification (2026-09-07): the isolated transport fixture fills the 128 MiB queue budget with sixteen shared 8 MiB UTF-8 payloads, verifies one-byte overflow rejection before listener registration or sending, and verifies that an aborted queued request retains its reservation until a response. A completed error and an injected send failure release byte capacity and cancellation resources; an equal-size replacement is accepted. Fatal-channel cleanup releases remaining tokens/listeners and closes the worker. The timeout-protected transport test passes with its completion marker. Payloads are shared in the mock, so this tests encoded-byte accounting rather than actual worker copies, native allocations or process memory limits. Full V1 remains incomplete.


Persistent migration contention qualification (2026-09-07): a file-backed two-connection regression holds a writer transaction while applying a pending migration. The runner returns a busy cause, restores autocommit, and leaves pending schema/data absent while the writer remains active. After writer commit, explicit retry applies only the pending version; reopen skips the exact applied history and preserves collection/native data and index integrity. Renaming an applied entry after reopen is rejected, and the unchanged plan remains usable. All four migration integration tests, formatting and focused Clippy pass. This adds one regression after the latest focused query additions; concurrent-runner stress, interrupted I/O/crash and full V1 qualification remain open.


Migration input-boundary qualification (2026-09-07): a new regression rejects 1,001 entries, a plan above 16 MiB, a script above 4 MiB, invalid positive-version requirements and empty/oversized/NUL-containing names before schema mutation. UTF-8 names are checked by bytes. An exactly 4 MiB script with a 255-byte multibyte name succeeds after those failures and skips on exact rerun, proving rejected plans did not record an applied prefix. All five migration integration tests, formatting and focused Clippy pass. Total runtime-memory, concurrent/crash and full V1 qualification remain open.


## Combined query, worker and migration verification (2026-09-07)

At clean implementation commit afbe25599, fastdb/scripts/check.sh passed formatting, Clippy with warnings denied, 335 Rust tests, 42 Node/application tests and strict TypeScript declarations. One known pinned trigger-interruption gate remains ignored. This combines the correlated DISTINCT ordering/collation/bound-write cases, worker UTF-8 queue-byte cleanup fixture, persistent migration contention/reopen and migration input-boundary regressions. The Node addon was rebuilt by the scoped script before client tests. This is local Linux evidence; platform distribution, interrupted I/O/recovery, remaining SQL/type/resource work and external application validation still prevent full V1 completion.


CLI migration source validation (2026-09-07): the loader now checks that each .sql source resolves to a regular file before opening it, preventing a stable named-pipe source from blocking the bounded content reader. Non-file errors identify the path; regular-file symlinks remain accepted. Both CLI migration integration tests pass, including rejection followed by an unapplied-prefix retry. A timeout-bounded Linux probe verifies FIFO rejection and symlink success. Formatting and CLI all-target Clippy pass. Concurrent path replacement, broader platform and full V1 qualification remain open.


CLI migration source diagnostics (2026-09-07): metadata/open/read, filename/version and file-budget failures now identify the migration path while retaining the underlying error text where available. The CLI retry regression additionally covers invalid UTF-8 content and malformed versions, asserting file-specific diagnostics and no applied prefix before a valid rerun. Both CLI migration tests, formatting and CLI all-target Clippy pass. Broader V1 tool qualification remains open.


Migration history diagnostics (2026-09-07): applied-prefix mismatch errors now distinguish version-sequence, name and exact SQL-source changes while retaining FDB_VALIDATION. Messages identify the supplied version and mention whitespace/comment sensitivity without printing stored SQL. Expanded regression assertions cover each mismatch and successful reuse of the original plan. All five migration integration tests, formatting and focused Clippy pass. History corruption/upgrade and broader V1 qualification remain open.


Migration ledger schema validation (2026-09-07): the runner now reuses managed-schema token comparison and dependency checks after creating or finding its ledger, inside the atomic scope and before history reads/pending scripts. Incompatible definitions and unexpected explicit indexes/triggers reject with FDB_STORAGE. Private corruption fixtures cover a missing primary key, an added index and an added trigger; pending schema/history remain untouched, autocommit is restored, and fixture-only external repair permits a valid apply-once retry. The new unit regression, all five migration integration tests, formatting and frontend all-target Clippy pass. No public repair API was added; broader corruption/upgrade/recovery and full V1 qualification remain open.


Applied migration ledger protection (2026-09-07): an additional private fixture adds a ledger trigger after an initial migration has committed. The runner rejects it before pending writes or trigger side effects; exact history rows, collection values, native audit data and managed-index integrity remain intact. Fixture-only trigger removal permits the pending version once and preserves the applied prefix on rerun. Both migration unit regressions, formatting and frontend all-target Clippy pass. This qualifies existing schema validation; broader external-corruption/recovery and full V1 work remain open.


## Combined ledger and distribution verification (2026-09-07)

At clean implementation commit 7c4858f7e, fastdb/scripts/check.sh passed formatting, Clippy, 338 Rust tests, 42 Node/application tests and strict TypeScript. One known trigger-interruption gate remains ignored. This includes migration source validation/diagnostics and ledger schema/dependency checks with applied-prefix preservation. The rebuilt addon also passed the offline installed-package runtime and declaration smoke on Linux x64 / Node 24.19.0: eight files, 59,806,951 packed bytes. No publishing occurred. Broader platform, recovery, SQL/type/resource and application release gates remain open; full V1 is incomplete.


Bounded migration history rows (2026-09-07): the ledger query now limits returned rows to the supplied plan length plus one, preserving omitted-history rejection without materializing every row from an oversized external ledger. A 1,100-row private fixture verifies validation rejection, unchanged ledger contents, no pending schema and restored autocommit. All three migration unit regressions, formatting and frontend all-target Clippy pass. Individual corrupted ledger-value sizes and total memory remain open, along with broader V1 gates.


Migration history value bounds (2026-09-07): a bounded metadata query now validates name/script storage types and byte lengths before fetching ledger text, within the same atomic scope. Per-name/per-script and aggregate script limits match input limits; oversize rejects with FDB_LIMIT and invalid types/empty names with FDB_STORAGE. New private fixtures cover multibyte oversized names, oversized scripts, aggregate history above 16 MiB and blob content; rejected runs restore autocommit and supported fixture repair permits retry. All five ledger unit tests, five migration integration tests, formatting and frontend all-target Clippy pass. This bounds frontend history text materialization, not engine page reads or temporary memory used by length/cast operations. Broader recovery/resource and full V1 gates remain open.


Maximum valid migration history qualification (2026-09-07): a file-backed integration regression applies four exactly 4 MiB scripts totaling 16 MiB, with multibyte UTF-8 comment padding and collection DDL. Reopening the database and rerunning the exact plan skips all four versions, preserves the created collections and returns autocommit. This confirms the new stored-history size checks accept the documented byte boundary. All six migration integration tests, formatting and focused Clippy pass. Broader resource/platform/recovery and full V1 qualification remain open.
