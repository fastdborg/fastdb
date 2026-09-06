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
