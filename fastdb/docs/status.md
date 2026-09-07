# V1 implementation status

V1 is incomplete. The full scope is the FastDB.md master plan in the parent planning directory. This file records current evidence and work remaining; no milestone substitutes for the full V1 goal.

## Repository and tooling

- Local Git checkout: `turso/`, branch `feat/embedded-foundation` (local `main` is the baseline), upstream v0.7.2 at `046e9cbf67d22491e8ecc941ec2891b02a9f3cad`.
- Five workspace crates: fastql-parser, fastdb, fastdb-cli, fastdb-tests, fastdb-node. All product crates are unpublished 0.1.0 prototypes.
- Scoped script: `fastdb/scripts/check.sh`; one Ubuntu CI YAML, read-only permissions, timeout and cancellation. Inherited workflow files moved unchanged to `.github/upstream-workflows/`.
- Remote FastDB fork owner is unresolved. No origin remote, push, PR, branch protection, or hosted FastDB CI result exists yet.
- Local toolchain is installed under `/tmp/fastdb-cargo` and `/tmp/fastdb-rustup`. Run with PATH prefixed by `/tmp/fastdb-cargo/bin`, CARGO_HOME and RUSTUP_HOME set accordingly, and RUSTUP_TOOLCHAIN=1.88.0. These temporary tools may need reinstalling on another machine/session. Native checks also require Node and `npm ci --prefix fastdb/bindings/node --ignore-scripts`; CI pins Node 24.19.0.

## Implemented subset

- Bare collection CREATE TABLE, IF NOT EXISTS, fixed typed IDs, direct-record SELECT, object/DOCUMENT INSERT, target object UPDATE, target DELETE, RETURNING *.
- Recursive record-target validation applies independently of indexes, including reserved names and NUL rejection.
- Nested objects/arrays, tagged persisted values, bool/int64/null/binary distinction, binary64 round-trip decoding, UUIDv7 automatic IDs.
- Field definitions (basic types, required/nullable, nested paths, overwrite) and single-path managed scalar/reference indexes with unique/nonunique variants. Rust APIs and initial FastQL declarations.
- Statement savepoints, transactional catalog/index maintenance, mixed ordinary SQL/document transactions, and deterministic partial-write/index-build interruption coverage.
- Weak Rust interrupt handles and out-of-queue Node interruption, with distinct cancellation errors and callback row collection preserving engine Interrupt vs Busy.
- Dedicated-worker AsyncDatabase with ordered submissions, bounded request queues, graceful close and fatal transport cleanup; per-operation cancellation and broader lifecycle qualification remain open.
- Initial native synchronous Node client with TypeScript declarations, bigint/typed-value conversion, query/cardinality methods, transaction errors, explicit close, script batches, migrations and JSON/NDJSON document transfers.
- Forward migration runner and CLI directory loading, exact-source history checks, and atomic pending runs.
- Versioned typed JSON/NDJSON collection import/export through Rust APIs and CLI, with atomic validated inserts and decimal-string int64 encoding.
- Rust query cardinality helpers, execute_batch with byte offsets and stop-on-error reports, and a multiline script CLI with tagged JSON output.
- Initial native scalar windows over collections: inline/named partitions and ordering, row_number, sum and count, including INSERT SELECT.
- Relational table/view stars in mixed collection queries, with source-order unqualified star expansion and native column metadata.
- Collection GROUP BY scalar expressions/ordinals (including parentheses, unary plus and collation) and HAVING aggregate predicates/projected aliases, with native SQL grouping and typed output projections.
- AST-lowered collection SELECT: typed field/document projections, scalar expressions, WHERE, explicit joins (including mixed relational/document joins), ORDER BY, LIMIT/OFFSET, named typed parameters, fixed record predicates, and EXPLAIN QUERY PLAN. Eligible equality and constant IN-list filters on the leading collection use its managed index; id equality uses the physical primary-key index.
- SQL column-list VALUES inserts (including multiple rows), predicate-based multirow UPDATE/DELETE, nested SET/UNSET, and RETURNING *. Whole statements share a savepoint and evaluated candidates use pre-update values.
- Standalone source-free SELECT preserves composite/record/vector/boolean parameter projections and resolves scalar alias predicates.
- SQL type::record constructors and typed record expression projections, including standalone SELECT; named and numbered/anonymous value binding through the Rust parameter map.
- ID-based object UPSERT and direct-target UPSERT, transactional field removal, collection/index drop, logical INFO FOR DB/TABLE/INDEX, and non-converting IF NOT EXISTS behavior.
- Field CHECK validation uses deterministic candidate-only SQL expressions, validates existing data before publishing definitions, and applies to every supported document write path.
- Fixed bundled QuickJS string::slugify and string::normalize functions with isolated per-call runtimes and initial input/output, memory, stack and cooperative execution limits.
- Dense32/dense64/sparse32/quantized8/bit vector values, vector<N> validation, native distances/extraction and typed slice/concat operations.
- Top-level SELECT record::fetch and Rust fetch_records resolve one-hop collection/relational references in batched snapshot reads.
- Rust transaction_state()/execute_report() and CLI transaction.before/after expose observed autocommit/active state on success and failure.
- Object/DOCUMENT INSERT, object UPDATE/UPSERT, predicate patches and direct DELETE support typed RETURNING projections and empty-result metadata.
- SQL-shaped INSERT/UPDATE/DELETE support typed RETURNING projections from final/deleted document snapshots, including empty-result column metadata.
- Column-list INSERT SELECT copies typed source rows through normal validation/index maintenance and materializes sources before writing, including self-inserts.
- Searched and simple CASE preserve typed branch results in SQL SELECT/SET/VALUES and object-write expressions, with lazy branch evaluation.
- SQL SELECT/SET/VALUES support typed record extractors, array::new/append, doc::get/has, collection doc::row, parentheses and lazy coalesce/ifnull.
- Object writes evaluate scalar operators and nested function calls with typed record/array/document helpers; predicate object patches and UPSERT expressions read pre-update values.
- Catalog writes use version 2 for CHECK semantics; version 1 and unversioned prototype metadata remain readable, and unknown/inconsistent versions are rejected.
- Ordinary SQL delegation outside the conservative collection-name guard, which still protects unimplemented forms.

- CLI script reports are emitted and flushed between statements; output failures stop later execution. Full scripts and individual result sets remain materialized.

- Qualified collection paths support up to 64 field segments in SELECT, SQL-shaped writes and RETURNING, retaining typed projections and managed scalar-index resolution.

- Scalar DISTINCT preserves typed representatives while grouping by native comparison values, with aggregate/window evaluation before deduplication and ordering/pagination after it.

- ORDER BY resolves projected aliases inside supported arithmetic/scalar/helper expressions, including DISTINCT output reuse and mixed source inputs. WHERE/GROUP BY/JOIN ON source-expression alias substitution is also supported; broader alias scope remains unfinished.

- Ordinary SQL value literals in covered SELECT/write/schema/trigger-expression contexts no longer trigger the managed-name guard; single-quoted object references remain protected.

- Initial interactive CLI with terminal auto-detection, multiline completeness, active-transaction prompts, .clear/.quit controls and recovery after statement errors; piped script mode remains available.

- CLI SQL input buffers have a configurable 16 MiB default byte limit; oversized input emits FDB_LIMIT and stops before submitting a truncated prefix.

- Controlled multi-connection document/index snapshots and index-build contention have persistent tests; native Busy/BusySnapshot errors expose FDB_BUSY/FDB_BUSY_SNAPSHOT through Rust and Node.

- Same-build offline backup/restore rehearsal covers a checkpointed file copy, typed values, schema, indexes, migration history and subsequent writes. See [the procedure and limits](backup-restore.md).

- SQL range comparisons with two preserved typed operands, and BETWEEN with three, now compare record integer keys numerically, matching object-expression ordering; broader comparison propagation remains pending.

- Relational INFO includes native table/index metadata and distinguishes views; database INFO lists views separately, with reopen and transactional index visibility coverage.

- Single-source null predicates can filter compact managed-index entries before document lookup; outer-join pushdown remains disabled and native null-key seeks are not available on the pinned engine.

- Node packaging has an explicit runtime file inventory and an offline tarball-install smoke covering synchronous/worker queries, persistence and installed TypeScript declarations on Linux x64/Node 24.19.0. Cross-platform prebuilds and release qualification remain pending.

- An offline Rust consumer outside the workspace builds and exercises the public client without the checkout build configuration, checking resolved package identities against the pinned lockfile. Registry distribution remains pending.

## Verification

The scoped test suite includes parser collision probes; persistent CRUD/reopen; mixed transaction rollback; failed unique inserts/updates/index builds; validation-definition rollback; typed round trips; numeric/index identity; a child process that exits without closing an active transaction; and differential ordinary SQL probes against the pinned engine. On 2026-09-07, scoped checks passed formatting, Clippy with warnings denied for the FastDB packages, and all 250 Rust tests plus twenty-three Node tests (including an isolated worker-transport fault test) and strict TypeScript declaration checks (Rust coverage includes a CLI recursive-input subprocess test, a schema-reprepare stack unit test, two CLI audit subprocess tests, four collection-content audit unit tests and a persistent audit integration test, two SELECT profiling tests, three reference-validation tests, three standalone-parameter tests, two numeric-precision tests, three interruption tests plus two catalog/index cancellation unit tests a mixed-CTE cancellation unit test and a compound source-evaluation cancellation unit test, four migration tests, four transfer tests, three window tests, three derived-source tests, three scalar/EXISTS-subquery tests, seven CTE tests, ten compound tests, two mixed-star tests, seven grouping tests, five bundled-runtime/function tests, ten vector tests, four forward-link tests, four batch tests, eight transaction-report tests, two CLI output-failure tests, a history-path collision unit test, a real-terminal subprocess test and seven CLI script/transaction subprocess tests, eight RETURNING tests, six INSERT SELECT tests, five CASE tests, eight SQL-helper tests, six document-expression tests, nineteen CHECK/upgrade tests, twelve catalog lifecycle/version/isolation/inspection tests plus a metadata-corruption unit test and four managed-schema unit tests, the typed BETWEEN evaluation-count test, the offline restore rehearsal, the process-kill stress test and its subprocess helper, the original subprocess helper, forty collection SELECT tests plus a lowering/execution unit test, and seven SQL-shaped write/constructor tests). This is local Linux evidence; hosted CI has not run. The process-exit test is a basic recovery smoke, not interrupted-checkpoint or power-loss certification.

## Next implementation work

1. Complete the SQL-shaped write contract (broader INSERT SELECT sources and further supported statement forms) and replace the remaining conservative managed-name guard. Complete collection read cases: subqueries/CTEs, grouping alias/type coverage and broader DISTINCT/window qualification, compound/derived typed expressions, and broader index planning. Preserve baseline parameter/alias forms and ordinary SQL errors. The fallback guard now distinguishes value literals in covered SELECT/write contexts; uncovered contexts still reject some harmless strings, and dependency/name authorization remains unfinished.
2. Finish expression type propagation through comparisons and remaining SQL expressions; broader CHECK eligibility, expanded inspection and index planning, stable errors/results, cancellation and resource limits. Namespace collisions, metadata format validation, multi-connection schema races, and managed object dependency access need full coverage.
3. Complete forward-link resource/planner coverage and upstream vector representation/operation coverage; qualify the initial bundled QuickJS catalog, limits, performance and platform packaging. All five pinned vector encodings now have initial validation; broader numerical/resource/platform and benchmark evidence remains pending.
4. Complete native Node/TypeScript APIs, cancellation/lifecycle and release packaging, plus Rust packaging; lossless cross-language wire encoding; CLI broader signal handling and terminal/platform/resource qualification and row streaming, broader import/export coverage, migration qualification, schema/query-plan inspection.
5. Complete all master-plan/FastQL release gates: broad differential coverage, interrupted commits/checkpoints, broader restore and previous-version upgrade rehearsal, bounded crash/fuzz/stress, resource limits, benchmarks and platform packaging smoke tests. External pilots and business evidence are also not present.

Keep upstream implementation files unchanged. No cloud implementation or V2/V3 features have begun.

## SELECT lowering implementation notes

`frontend/src/select.rs` parses through the pinned SQLite AST and rewrites collection sources/field expressions. `frontend/src/functions.rs` registers static pure accessors on each private engine connection before exposing it. Scalar access rejects objects/arrays/vectors; typed projections decode the tagged value. Record ORDER BY uses canonical targets and signed integer ordering before string keys. Index candidates are selected only for simple equality/AND predicates with constant or bound keys, and the original predicate is retained for correctness. Other predicates remain engine-evaluated scans; no index use is claimed for them.

The current result metadata distinguishes direct typed field projections from ordinary SQL scalar expression results. Typed values flowing through arbitrary expressions, binary literals compared to typed binary fields, mixed record/scalar ordering, complete alias resolution, and metadata snapshot races still require work before V1 semantics can freeze. Scalar DISTINCT now has initial coverage; Nonrecursive collection CTEs and aliased typed derived-table sources are now supported as described below. This does not reduce the master-plan scope.

## SQL-shaped write notes

`frontend/src/write.rs` dispatches parsed collection writes and delegates unchanged relational statements. `update.rs` normalizes collection path assignments before the stock SQL parser; paths preserve quoted segments, and duplicate/overlapping paths fail before mutation. Missing SET parents become objects; non-object parents fail. UNSET removes absent paths as a no-op and still validates the final document. Direct record SET/UNSET targets lower to an immutable-ID predicate. All candidate assignment values are collected before applying any row. Validation or uniqueness failures roll back the whole statement while retaining an existing outer transaction when the engine permits it.

Direct typed parameters and copied document fields retain their logical types. Ordinary SQL scalar expressions retain engine scalar types: SQL TRUE/FALSE become integer 1/0, so boolean validators require typed Boolean parameters or document literals rather than implicit coercion. The Rust map uses `?1`, `?2`, etc. to bind numbered or anonymous statement slots. Pinned Turso v0.7.2 rejects `$name::suffix`; a differential test preserves that exact engine error instead of reinterpreting it.

Current write limits include INSERT SELECT limited to the current source-query subset, no UPDATE FROM/CTE/tuple assignments, and incomplete expression type propagation. The full V1 scope remains unchanged. Resource limits and catalog concurrency still need release-level verification.

## Catalog lifecycle notes

`frontend/src/catalog.rs` owns logical lifecycle operations. DROP INDEX now removes both the index storage and metadata in the same savepoint; it no longer falls through to a raw engine drop that could leave stale metadata. DROP TABLE on a collection removes its documents, validators and managed indexes atomically, and never cascades through weak references. REMOVE FIELD removes validation metadata only. Field definitions and index builds reject incompatible scalar/object paths even when the collection is empty. IF NOT EXISTS never converts an existing data model or changes an existing index.

UPSERT requires an explicit typed id, inserts missing records, and shallow-patches existing records. Direct-target bodies must omit id. Required-field validation and other unique conflicts still apply; statement errors roll back all changes. INFO reports logical model/fields/index paths and selected capabilities without exposing collection storage names. The prototype INFO shape is not a frozen V1 wire contract. New catalog entries and metadata updates use version 2. Old version-1 entries (including the unversioned prototype) remain readable. CHECK-bearing entries require version 2; unknown or inconsistent versions fail closed. Full upgrade/restore rehearsal and corruption detection remain release gates.

## CHECK implementation notes

`frontend/src/check.rs` parses CHECK as a single SQL expression, rejects reads/parameters/unsafe functions, replaces field references with bound candidate values, and executes only a scalar SELECT through Turso. False and SQL NULL fail. Missing optional and allowed-null fields skip their attached check. Definition changes validate all existing documents before saving; ordinary document writes, SQL-shaped writes, UPSERT and UNSET validate the final candidate and roll back on failure. INFO exposes the expression.

The current eligible function list is length, lower, upper, trim/ltrim/rtrim, substr/substring, abs, round, coalesce/ifnull/nullif, typeof, unicode, instr, replace, and multi-argument scalar min/max. Arithmetic, comparisons, CASE, BETWEEN, IN lists, null tests, GLOB, unsized built-in scalar CASTs and built-in BINARY/NOCASE/RTRIM collations are supported. LIKE (connection-setting dependent), REGEXP/MATCH, custom casts/collations, aggregates, windows, clock/random functions and database reads are not eligible. Broader eligibility and expression/resource limits remain release work.

CHECK adds a validation guarantee older readers must not ignore, so catalog writes now use version 2. A tested legacy-catalog fixture upgrades atomically when a definition is published, with rollback restoring the prior version. The prior version-aware prototype rejects version 2; downgrade to earlier unversioned prototypes is unsupported. A full previous-binary upgrade/restore rehearsal remains a release gate.

## Document expression notes

Object INSERT/UPDATE/UPSERT bodies support field reads, parentheses, scalar arithmetic/comparison/boolean operators, and nested calls. Scalar work uses the pinned engine. Objects/arrays/records retain their types through construction, typed parameters, lazy coalesce/ifnull, and the implemented helpers: type::record, record::id/table, array::new/append, doc::get/has. Document paths support object keys and nonnegative array positions, including double-quoted keys. Missing reads return null; has distinguishes stored null from absence. Generic scalar operations reject composite values; record comparisons use logical target/key identity and numeric integer-key ordering.

Predicate object UPDATE gathers and evaluates all candidates before writing and shares a statement savepoint. Target UPDATE and UPSERT read existing fields before applying the shallow patch. UPSERT can use coalesce for absent fields on insertion. Expression tree depth is capped at 64, including flat operator chains. This is not full query resource accounting.

These helpers also have an initial SQL SELECT/SET/VALUES path described below. BETWEEN/LIKE and other broader object expression grammar, additional helper functions, and full resource/concurrency verification remain open. No arbitrary JavaScript execution is introduced.

## Typed SQL helper notes

Collection SELECT, SET and column-list VALUES share typed lowering for record::id/table, array::new/append, doc::get/has, and doc::row(collection_alias). Namespaced standalone SELECT calls also work. Helpers preserve typed nested arguments; doc::row returns null for an unmatched outer-join source. Parentheses and lazy coalesce/ifnull retain the selected value's type, including arrays and booleans. Scalar contexts unwrap helper outputs, while projections decode them. Typed helper ORDER BY uses logical record ordering and rejects composite sort keys. Generated projection aliases are quoted, and default helper labels use public names.

Typed parameter occurrences are currently encoded as hex literals in the per-execution lowered AST; other occurrences retain ordinary binding. This avoids confusing user binary data with internal values, but larger values amplify lowered SQL size. Prepared-plan caching, parameter allocation/resource accounting, broader type propagation, record/composite scalar behavior, and comprehensive expression-error contracts remain release work. Typed accessors are statically linked Rust functions; string::slugify and string::normalize call the fixed bundled QuickJS catalog. Runtime qualification remains a V1 release requirement.

## Conditional expression notes

Both searched CASE (WHEN predicates) and simple CASE (one base value matched against WHEN values) now preserve the selected branch's type. SQL lowering retains engine CASE control flow while encoding result branches; object expressions evaluate the base once and evaluate conditions in order until a match. An omitted ELSE returns null. Nested CASE, helper composition, scalar predicate contexts and typed ordering work through the existing lowering. Writes validate final candidates and retain whole-statement rollback. SQL simple-CASE comparisons still use the current scalar lowering; broader record/composite comparison semantics remain release work.

## INSERT SELECT notes

Ordinary relational targets accept the supported collection SELECT subset through one native INSERT statement. Tests cover grouping, DISTINCT, duplicate projection names, explicit record-key extraction, binary payloads, ABORT/FAIL/IGNORE/REPLACE, UPSERT, RETURNING, triggers, rollback and close/reopen. Target scalar affinity and native conflict dispositions apply; composite values and fetched source projections are rejected. Internal names remain protected, including single-quoted function/table references; ordinary string values may contain managed-looking text.

Collection INSERT with an explicit target column list now consumes the existing SELECT lowering subset, including typed collection fields/helpers, explicit relational projections, filters, joins, sorting and pagination. Target fields map by position, so repeated source projection names are accepted for insertion. Column-count mismatches fail even when the source returns no rows. All source values are materialized before mutation; self-inserts consume only the original source rows. Generated IDs, required/type/CHECK validation, unique constraints and managed indexes use the normal insert path inside one savepoint. A later row failure rolls back all inserted rows and index entries while retaining an existing outer transaction.

CTEs, grouping aliases, broader DISTINCT/window coverage, compound sources and other unsupported source shapes remain pending with broader SELECT work. Typed compound SELECT/VALUES sources are now described below; unsupported modified standalone VALUES still fail explicitly instead of processing only their first component. This materializing implementation still needs bounded-memory/resource accounting and release-level concurrency verification.

## RETURNING projection notes

SQL-shaped collection INSERT (VALUES/SELECT), UPDATE and DELETE now return named scalar/typed projections, aliases and full-document stars. INSERT/UPDATE projections use final documents; DELETE projections use the deleted documents. The projection source is a synthetic row containing the document and typed ID, so projection evaluation does not refetch deleted data. Empty writes still prepare an empty snapshot to return column metadata and validate projection shape. All projection evaluation occurs inside the statement savepoint; no partial rowset is returned on failure.

Subqueries, aggregates and windows are rejected in RETURNING. Aggregate detection tracks the pinned engine's built-in aggregate list and needs review during upgrades. Object INSERT/UPDATE/UPSERT, predicate object patches and direct document DELETE use the same typed RETURNING projection path. Projection snapshot encoding/materialization still needs resource accounting and prepared-plan reuse.

A runtime helper failure can make pinned Turso abort the outer transaction as well as the current write; tests now demonstrate this and successful reuse of the connection afterward. This is within the prototype's engine-abort exception, not a guarantee that every expression failure preserves an outer transaction. Rust and CLI now expose observed transaction state as described below; finer error taxonomy remains release work.

Object RETURNING grammar retains the projection text for the pinned SQL AST parser. A projection is a list of result expressions, not a new query: FROM/WHERE/compound clauses, subqueries, aggregates, windows and managed internal names are rejected. Object insertion/deletion and projection evaluation now share an outer statement savepoint, so invalid projection syntax cannot leave a successful mutation behind. Quoted fields, string tokens and nesting are distinguished when separating predicate patches from their RETURNING clause; a semicolon inside a string remains data.

## Transaction observation notes

Connection::transaction_state() samples the pinned engine autocommit flag without issuing SQL. execute_report() retains the original Result<QueryResult> and reports transaction_before/transaction_after as Autocommit or Active. Active includes an explicit transaction or outer savepoint; it is not proof that a future COMMIT will succeed. Autocommit does not identify whether earlier work committed or rolled back. Callers must serialize operations on each connection to attribute observations to one execution.

The CLI adds transaction.before/after (snake_case state strings) to each success/error JSON line while retaining existing result/error fields. Tests distinguish Rust validation failures that leave a transaction active from runtime helper failures that abort it and discard earlier uncommitted work. Savepoint transitions, independent connections and close/reopen after a retained transaction commit are covered. This is observational reporting, not a complete error-disposition, durability or cross-language wire contract.

## Script execution notes

The shared parser splitter preserves semicolons in quoted strings/identifiers, comments, nested documents/arrays and SQL trigger bodies (including CASE END). It returns UTF-8 byte offsets. Rust execute_batch tokenizes/splits the full script before executing, then records each executed statement and stops at the first execution error. The final statement need not have a semicolon, and empty/comment-only statements are skipped. A lexical splitting failure runs nothing; a later parse/execute failure leaves earlier statements subject to their explicit transaction controls. No implicit batch transaction or bound batch-parameter map is introduced.

CLI stdin defaults to semicolon-delimited script mode, emits one JSON result/error per executed statement with its offset, and returns nonzero on error. --line retains the prior line-at-a-time, continue-after-error mode, but now also reports failure via exit status. Script input and reports are materialized in memory; bounded/streaming execution, interactive prompts, streaming transfers and migration qualification remain V1 tool work.

## Forward-link notes

Top-level SELECT record::fetch(reference) is lowered to a typed reference projection followed by frontend-managed target reads, under the same statement savepoint/snapshot. No UDF or QuickJS callback reads the database. Rust fetch_records exposes the same batched resolver. References are canonicalized and deduplicated per target, with IN-list reads of at most 128 keys and at most 16,384 reference positions per call/query. Results preserve input order/duplicates; null or missing records/tables yield null. Reference-looking strings and arrays of references are errors. Returned documents retain their own reference fields without recursively fetching them.

Relational targets require exactly one explicit TEXT or INTEGER primary key and a matching reference-key type; implicit rowids, composite keys, views and other key declarations are unsupported. Native relational row values are returned in an Object without converting their primary keys to records. Fetch is restricted to top-level SELECT projections; nested fetches, fetched-alias filters/orderings, RETURNING and write expressions are rejected. Qualify a stored source field when its name also names a fetched projection alias.

A two-connection test checks that a fetch inside an established read transaction sees the earlier snapshot after another connection commits a target update. Complete interleaving/crash/resource stress remains a release gate. The current EXPLAIN output describes the outer engine query, not target-batch details. Reference-count limits do not yet bound total fetched bytes or outer-query materialization; planner instrumentation and full resource budgets remain open.

## Initial dense vector notes

Value::vector32/vector64 construct validated typed dense values; vector_dimensions reports their dimension. Value validation rejects empty/misaligned encodings, non-finite components and dimensions above 65,536. Float32 accepts the pinned engine's untagged dense bytes or its explicit trailing type 1; float64 uses trailing type 2. Field vector<N> enforces dimensions on existing documents at definition time and on every supported write. INFO displays dimensions; scalar indexes reject vector fields. Generic scalar/index operations do not treat vector payloads as scalar blobs.

Object expressions and collection SQL lowering retain vector32/vector64 constructor results as Vector values, including typed parameters/projections and RETURNING. vector_distance_cos/l2/dot and vector_extract unwrap typed operands for the pinned native functions. Ordinary SQL outside collection lowering retains native Blob results and behavior. This is exhaustive search using upstream floating-point algorithms, not ANN or exact arithmetic; a test compares a collection distance with native engine output and allows floating-point error against the mathematical ideal.

Dense support is extended by the representation/operation work below. Broader malformed-input/zero-norm/numerical/platform coverage, full resource accounting and performance benchmarks remain open. Catalog version stays 2: the new FieldType enum variant is rejected by older prototype readers rather than silently ignored. Earlier opaque Vector values that do not meet the new validation contract are no longer accepted; no downgrade or release upgrade guarantee is implied.

## Vector representations and operations

Vector validation now also handles sparse float32 (trailing tag 9), quantized float8 (tag 4), and packed bit vectors (tag 3), in addition to dense32/64. It validates sparse index ordering/uniqueness/range, dimensions, finite values, quantization scale/shift and reconstructed components, metadata lengths, and padding before native binary parsing. All-zero sparse vectors with positive dimensions are valid. Unknown types and malformed metadata fail without entering the native vector parser.

vector32_sparse, vector8 and vector1bit now retain typed results in object/collection SQL expressions. vector_slice/vector_concat return typed vectors, and Jaccard joins the native distance functions. Native restrictions on format combinations remain: slice/concat on float8/bit vectors are unsupported, and empty vectors remain outside the FastDB value contract.

The pinned sparse concat implementation appends right-hand indexes without shifting them. The FastDB typed frontend corrects this by offsetting those indexes by the left dimension and validating the result; dense concat uses the native operation. Ordinary SQL outside FastDB lowering retains baseline behavior. Upstream files are unchanged; review this workaround during upstream syncs. Tests assert the actual concatenated coordinates, not just dimensions. Full numerical/fuzz/stress/platform and performance release coverage remains open.


## Binary ordering correction

Binary ORDER BY keys now use payload bytes, correcting the tagged-encoding order that placed X'0A' before X'02'. Typed range and BETWEEN comparisons also use payload bytes, with preserved blob literal bounds. Collection/native-BLOB differential coverage includes DISTINCT, empty/prefix values, indexed IN ordering and DELETE/RETURNING rollback. Persisted values and index keys are unchanged; managed range seeks and comprehensive binary expression/CHECK propagation remain open.


## Typed membership correction

IN/NOT IN with a preserved typed left operand now normalizes all list operands, including native function results, to collision-resistant scalar keys. Blob literals on the left match typed fields and parameters; binary payloads still cannot impersonate record IDs. Differential tests cover NULL/empty lists and DELETE rollback/index restoration. Native-function left operands now receive the same conversion, with explicit collation preserved. Differential probes cover blob/numeric/text functions and unchanged CAST affinity. Cast and scalar membership operands now receive comparison keys while retaining cast affinity, explicit collation and unary-plus behavior; differential probes cover both list sides. Native scalar operator payload handling is covered below; broader type/affinity propagation and subquery/aggregate qualification remain release work.


## Native scalar operator payloads

Arithmetic, concatenation, bitwise/shift and logical operators now receive payload bytes for preserved binary inputs. Unary plus preserves typed values. Collection/native-BLOB differential probes cover numeric/nonnumeric/empty/NULL values, nested expressions and UPDATE/RETURNING rollback with index restoration. CHECK operators receive the corresponding correction below; broader predicate/type propagation still requires work; persisted encoding is unchanged.


## CHECK operator correction and resource finding

CHECK scalar operators now use binary payload bytes, with unary plus preserving the enclosing binding mode. A persistent regression covers existing-data validation, rejected INSERT/UPDATE/UPSERT, retained transaction/index state and reopened enforcement. Reapply affected prototype definitions to revalidate stored data; opening alone does not scan documents.

An initial version of `binary_check_operators_validate_payloads_atomically_after_reopen` combined all four constraint groups into one left-associated AND expression and overflowed the default Rust test thread stack (Rust 1.88.0, unoptimized Linux build). Instrumentation located the failure after FastDB parsing/lowering and during engine preparation. CHECK lowering now balances homogeneous AND/OR chains, retaining operand order and explicit serialization parentheses. The original combined constraint and a long OR/NULL chain pass in `long_check_boolean_chains_prepare_and_validate`, including invalid-update rollback/index checks. This fixes the observed preparation failure without an upstream core change. General expression depth/stack accounting, arbitrarily large input and non-CHECK SQL still require resource-limit work.


## Binary predicate truth conversion

SQL-shaped WHERE/HAVING/JOIN ON, aggregate FILTER and searched CASE now pass preserved binary payloads to native truth conversion. Differential tests cover empty/nonnumeric/NULL values, inner/left joins, grouped HAVING, aggregate filters and DELETE rollback/index restoration. CHECK truth contexts receive the corresponding correction below; simple CASE and broader equality propagation remain unfinished.


## CHECK truth correction

CHECK root expressions and searched CASE conditions now use candidate binary payloads for native truth conversion. Persistent tests cover positive/negative numeric bytes, rejected zero/empty/nonnumeric/NULL bytes, nullable-constraint skipping to isolate searched CASE, transaction/index preservation and reopened enforcement. Existing prototype definitions need reapplication to revalidate data; comparison/literal propagation and general resource limits remain unfinished.


## Simple CASE binary comparisons

SQL-shaped simple CASE now uses shared comparison keys for its base and WHEN values, fixing binary literal/field/function matches while retaining casts, collation, native CASE structure and typed result branches. Differential probes cover NULL, cast affinity and binary results; UPDATE/RETURNING rollback restores indexes. CHECK simple CASE and broader equality/type/resource qualification remain open.


## CHECK simple CASE correction

CHECK simple CASE now uses candidate comparison keys for binary fields, literal/function results and nested CASE branches. Persistent tests cover definition over stored values, reversed operands, casts, failed updates, retained transaction/index state and reopened enforcement. A direct ordinary-table probe confirms that the pinned engine does not coerce text '1' to match an integer CASE base; tests preserve that behavior. Reapply affected prototype definitions to revalidate stored data. Broader CHECK equality/membership and resource qualification remain open.


## CHECK equality and membership correction

CHECK equality/IS and IN/NOT IN now normalize fields, literals and function results through candidate comparison keys. Known numeric/text results avoid unnecessary wrappers; the long boolean-chain preparation regression remains covered. Regression coverage includes binary/record separation, NULL membership, casts/collation, invalid writes, transaction/index preservation and reopen. Existing prototype definitions need reapplication for data revalidation. Range/literal comparison propagation and general resource qualification remain open.


## CHECK literal range correction

CHECK range comparisons and BETWEEN now accept typed field/literal operands, including binary and signed numeric bounds. Regression coverage verifies inclusive/reversed/negative ranges, NULL rejection, failed-update transaction/index preservation and reopen. Cast/function/collation range propagation and resource qualification remain open; affected prototype definitions need reapplication for existing-data validation.


## SQL-shaped equality expression correction

Equality/IS operators now normalize both supported expression operands through shared comparison keys. Direct fields keep their existing accessors so ID equality retains a primary-key SEARCH plan. Differential tests cover blob-returning functions, casts, NULLs, collation, arithmetic and CASE results, plus DELETE rollback/index restoration. Mixed ordinary-column/typed-field and untyped-alias propagation and general resource qualification remain unfinished; persisted encodings and index candidate shapes are unchanged.


## Binary pattern correction

SQL-shaped LIKE/GLOB operands and ESCAPE now use payload bytes, with differential empty/NULL/binary-pattern coverage and DELETE rollback. CHECK's existing GLOB support receives the same candidate conversion, verified through failed updates, transaction preservation and reopened index/enforcement checks. CHECK LIKE/REGEXP/MATCH remain ineligible; affected prototype definitions need reapplication for stored-data validation. General resource and broader expression qualification remain open.


## JSON arrow payload correction

SQL-shaped and CHECK JSON arrow operators now receive binary payload bytes. Differential tests cover extraction/chaining, missing/NULL values, native errors for binary paths and UPDATE rollback. Persistent CHECK coverage verifies rejected updates, retained transaction state and reopened enforcement/index contents. This adds no implicit object-to-JSON conversion or new CHECK functions; general resource and expression qualification remain open.


## Native alias comparison correction

Native expression aliases now receive comparison keys without re-lowering their generated inputs. Differential tests cover binary alias equality/membership in HAVING, casts/collation and ORDER BY equality. Direct ordinary-column aliases retain existing affinity handling. WHERE/GROUP BY/JOIN ON aliases are described below; mixed-column type propagation and general resource qualification remain unfinished.


## Collection GROUP BY aliases

Named projection aliases now resolve in GROUP BY, including supported expression/collation wrappers and integer constants. Collection aliases take precedence over same-named stored fields; qualified paths retain stored-field meaning. Ordinary relational column-first precedence remains native. Tests cover source-expression substitution, collisions, typed binary/record keys and aggregate-alias rejection. WHERE/JOIN ON alias resolution is described below; broader grouping/derived-query/resource work remains open.


## Collection WHERE aliases

WHERE now resolves explicit projection aliases before choosing managed index candidates. The collection alias-first rule matches GROUP BY, with qualified fields retaining stored-field meaning. Tests cover expressions/constants/membership, field collisions, index SEARCH, aggregate misuse and collection-target INSERT SELECT rollback. Collection-source INSERT SELECT into an ordinary target is now supported as described above. JOIN ON aliases now receive the same source-expression substitution before typed predicate lowering. Inner/left joins, binary/record comparisons, native-target INSERT SELECT rollback and invalid aggregate/fetched predicates have regression coverage. Broader derived-query/type/resource work remains open.

## Quoted managed-name guards

Collection reads, SQL-shaped writes and object-write RETURNING now share an original-token guard that recognizes single-quoted internal function names and qualifiers. A regression checks SELECT predicates/order, VALUES/SELECT inserts, UPDATE/DELETE, SQL/object RETURNING, failure rollback and harmless managed-looking text mixed with FastQL. Broader dependency authorization is still unfinished.

## Mixed native-column equality

Mixed equality/inequality/IS/IS NOT now normalize native BLOB column values to binary comparison keys while preserving scalar native-column affinity. Differential tests cover operand order, NULL, numeric affinity, declared/explicit collation and left joins. Native BLOB bytes are never inferred to be typed records. Native-target INSERT SELECT rollback is covered. Mixed ranges/membership, derived/alias propagation and index-seek qualification remain open.

## Mixed native-column membership

Native-column IN/NOT IN now uses binary comparison keys for BLOBs and preserves the native left-hand affinity for scalar values. Differential tests cover NULLs/empty lists, operand directions, affinity, collation and HAVING aliases. Additional regressions confirm existing mixed native HAVING aliases retain projection identity across stored-field collisions. Mixed ranges, subquery membership, broader derived typing and planner qualification remain unfinished.

## Process-kill commit/checkpoint recovery probes

A bounded subprocess harness now kills repeated mixed document/relational transactions after document rewrites, near COMMIT and near TRUNCATE checkpoint calls, using zero-, one- and five-millisecond delays after flushed phase markers. Each of nine fresh databases is reopened twice. Every transaction inserts a new batch, updates half of the preceding batch (including unique indexed names and binary payloads), and deletes the other half, with matching relational state. Checks cover native integrity, every acknowledged commit, at most one additional unacknowledged durable transaction, contiguous audit history, exact surviving documents and relational rows, new index entries, and absence of stale/deleted/uncommitted document and index entries.

The markers identify call windows, not the exact engine instruction running at termination. These are local process-failure probes, not deterministic mid-I/O fault injection, power-loss certification or cross-platform release qualification. Broader crash/fuzz/resource and previous-version upgrade gates remain open.

## Catalog mutation cancellation

Test-only one-shot engine progress callbacks now interrupt DROP INDEX, collection DROP TABLE and DEFINE FIELD OVERWRITE after change counters advance. Regression checks cover catalog/physical schema restoration, required integer validation, index usability, prior outer-transaction work, retry and explicit rollback. A separate transactional index replacement test proves that cancelling CREATE leaves the earlier DROP pending until outer ROLLBACK restores the old index; retry/commit installs the replacement. Broader failure-point and persistent fault-injection qualification remains open.

## Catalog identity validation

Direct lookup and catalog enumeration now validate decoded metadata identity and structure before use: catalog/collection name agreement, deterministic storage names, unique canonical index identities, valid field/index paths, duplicate/reserved field rejection, reference names, vector dimensions and field/index compatibility. A corruption regression verifies FDB_STORAGE, blocks destructive storage redirection and preserves an unrelated native table. Valid legacy version-1 entries and case-insensitive Rust record targets remain supported. Physical-layout/content corruption checks, repair tooling and broader resource/dependency qualification remain open.

## Statement delimiter-depth preflight

FastQL parse/execute now rejects input beyond 64 open delimiters before dispatch to recursive expression/native SQL parsing. Token-aware checks ignore quoted content and comments. Parser tests cover 64/65 boundaries, byte offsets and 10,000 parentheses; a real-engine regression verifies rejected native/document writes preserve existing transaction work and permit subsequent use. This is an initial shared input bound, not full AST-depth, allocation, generated-expression or result-budget qualification.

## Direct and stored CHECK depth guards

Direct Rust CHECK definitions now receive the shared delimiter preflight before parsing. Catalog decoding rejects stored CHECK text that exceeds the same bound, so write-time CHECK parsing cannot bypass it through persisted metadata. A direct-API regression verifies definition/index/transaction preservation and later rollback; the corruption test now covers stored CHECK depth. Broader AST and allocation budgets remain unfinished.

## Reusable SELECT lowering stage

Collection query lowering now returns an internal single-execution plan containing the rewritten command, logical output metadata, fetch flags and consumed-parameter names. A separate execution stage prepares/binds the command, decodes typed results and expands fetched references. Native-target INSERT SELECT uses the same execution stage while retaining native result decoding and affected counts. The existing fetch transaction wrapper still surrounds lowering and execution together.

A frontend regression checks typed output metadata/results and proves that lowering a native insert does not execute it. This establishes an internal boundary needed for nested-source lowering; nonrecursive CTE and derived-table support is described below, while scalar subqueries remain unfinished. Plans may embed typed parameter values and are not a public reusable prepared-statement API.

## Typed derived-table queries

Aliased collection SELECT subqueries now work as FROM/join sources, preserving logical output types through nested queries and expanding derived stars into named columns. Tests cover projected objects/deep paths, arrays, records, booleans/binary values, named/positional parameters, helper/native expression columns, empty metadata, grouping, DISTINCT, pagination, left joins and write-source validation/rollback. Native-only derived queries retain delegation. Duplicate output names, unaliased collection sources and fetched inner projections remain unsupported; CTE/compound/scalar/correlated queries and broader planner/resource qualification remain open.

## Nonrecursive typed CTE queries

WITH SELECT now supports collection CTEs, references to earlier definitions, explicit column names, grouping/DISTINCT, typed projections, native CTE joins and nested WITH scope. Definitions remain engine CTEs with their materialization hints intact; a MATERIALIZED volatile projection test verifies shared values across references. Tests cover collection-name shadowing with main-qualified access, top-level forward fetch, empty metadata and source-form INSERT validation/RETURNING/rollback. Recursive/forward collection references, fetched inner projections, compounds, leading-WITH writes and broader scope/resource qualification remain unfinished or unsupported. Native recursive rejection is preserved.

## Leading WITH inserts

Leading WITH ... INSERT SELECT now supports collection and native targets through the existing CTE source lowering. A new regression covers typed RETURNING, inner parameters, validation rollback, explicit rollback, native IGNORE and native-only delegation. Native UPSERT/RETURNING subqueries and table membership are rejected on the managed-source route to preserve the CTE scope boundary; a same-named physical table regression verifies that no write runs with a misresolved reference. Leading-WITH UPDATE/DELETE and broader cross-clause typing remain open; VALUES support is described below.

## CTE and derived-source cancellation

The controlled interruption matrix now includes leading-WITH and derived-table collection INSERT SELECT after mutation has begun. A new unit test interrupts mixed native/collection CTE reads and inserts at three early engine-operation counts, verifying FDB_CANCELLED, preserved outer work, empty target/index state, retry and rollback. These are specific deterministic interruption points, not comprehensive cancellation/deadline coverage for every lowering or engine stage.

## Mixed native-column ranges

Collection/native-column `<`, `<=`, `>` and `>=` now use raw binary payloads, preserve native numeric affinity and text collation, and reject non-null record/scalar ordering. Regression coverage includes both operand orders, native column wrappers, NULLs, left joins, typed derived/CTE sources and native INSERT SELECT with explicit rollback. Other mixed BETWEEN forms, arbitrary native expressions and broader logical-operand wrappers remain to be qualified.

## Native-column BETWEEN document bounds

Native-column BETWEEN and NOT BETWEEN with document bounds now compare raw binary payloads and preserve native affinity, collation and NULL behavior. The regression covers either/both typed bounds, reversed/NULL bounds, numeric/text comparisons, CTE bounds, a left join, record/scalar rejection and native target constraint rollback. The controlled counter test also checks each typed bound evaluates once. The logical-left/native-column-bounds extension is described below; wider expression forms and volatile native views/derived columns remain unqualified.

## CLI terminal editing and history

Unix terminal sessions now use pinned Rustyline 15, with multiline history recall, prompt Ctrl-C to clear pending input without rolling back a transaction, and optional `--history PATH` persistence. Default history stays in memory. History size/entry limits, leading-whitespace exclusion, private new files and database/sidecar collision checks are implemented. The terminal submission limit is checked after editing; live editor memory and broader signal/platform qualification remain unfinished. Piped and redirected-stderr sessions retain plain input and JSON output stays separate. A Python Unix pseudo-terminal harness is part of the scoped CLI tests.

## Interactive CLI query cancellation

Edited Unix terminal sessions now route SIGINT through a regular signal-listener thread to the weak engine interrupt handle. Ctrl-C during engine work reports FDB_CANCELLED, stops the submitted batch and returns to the prompt; command failure still makes eventual process exit nonzero. The listener closes and joins on exit. The PTY harness now checks read cancellation with retained prior outer-transaction work, native write rollback, skipped batch tails and connection reuse. Nonterminal signals, complete deadlines, broader delivery timing and other platforms remain unqualified.

## Local benchmark harness

`fastdb/scripts/benchmark.py` loads a temporary persistent database and measures warm CLI round trips for unindexed/indexed document equality and exact cosine top-10. Reports retain samples, query plans, index-build/load time, Linux process peak RSS, binary identity and source state. Result and plan assertions guard the run. The initial reports have unmeasured scan counters; the profiler below supplies them for new runs. The synthetic low-dimensional fixture, debug builds and a single local run do not satisfy the complete benchmark gate. See benchmarks.md for commands and limitations.

## Measured SELECT counters

Rust profile_select and the CLI .profile command now expose primary engine statement counters while preserving typed query results. Tests compare measured scans before/after a managed index, repeat calls to verify counters reset, exercise a CTE and native bindings, and reject writes/multiple statements/managed calls/FETCH. The benchmark harness retains each sample's measured counters. These are physical engine operations, excluding metadata/lowering queries and Rust work; FETCH profiling and broader profiling qualification remain open.

## Node SELECT profiling

Synchronous and async Node clients now expose profileSelect with typed query results, transaction observations and bigint engine counters. Decimal-string native transport avoids numeric narrowing. The worker allowlist includes profiling, and both clients retain their existing queue, close and error behavior. Tests compare scans before/after an index, typed binary/record/boolean output, native parameters, counter reset and write/closed-connection rejection.

## Managed schema validation on connect

New connections now check the catalog plus referenced collection/index DDL and managed-table dependencies in one snapshot. Missing/incompatible objects, changed uniqueness and unexpected explicit indexes/triggers fail with FDB_STORAGE before the connection is exposed. Tests cover healthy reconnect, a mutation matrix, restored uniqueness and persistent reopen after external ALTER TABLE. This does not scan documents/index contents or revalidate every later operation; post-connect external modification remains outside this check. Reserved-prefix orphan checks are described below.

## Orphan managed storage detection

Connection validation now inventories reserved collection/index storage names and rejects objects absent from metadata, including case variants and views. It also rejects a physical index table referenced by multiple collections. Unit cases cover removed collection/index metadata and unexpected reserved objects while verifying original physical rows survive rejection. A persisted-file test drops the catalog externally and verifies reconnect rejects the surviving collection storage after open recreates the empty catalog. This is detection, not metadata recovery or data repair; arbitrary renamed objects and content corruption remain unqualified.

## Explicit collection content audit

Rust check_collection_integrity streams ID/document rows in one snapshot, validates IDs and fields/CHECKs, and verifies each expected index entry plus total index counts. Defaults bound processed document count and encoded ID/document bytes; they are not engine memory/time limits. Unit coverage detects invalid encoding/IDs/types/CHECK values, missing/stale/duplicate/extra entries, preserves outer work on limit failures and checks cancellation classification. Persistent typed-vector/binary and exact byte-limit coverage is included. No data is repaired; broader physical/resource qualification remains open.

## Node collection audit

Both Node clients now expose checkCollectionIntegrity with optional bigint limits, lossless bigint report counters and transaction observations. The async path uses the existing bounded worker queue. Tests cover empty audits with zero limits, counts, exact byte bounds, limit failure with retained outer work, invalid/out-of-range options, rollback, missing collections and closed connections.

## CLI collection audit

The standalone --check-collection command audits an existing database collection with optional unsigned document/encoded-byte limits. It emits one JSON report, exposes transaction observations after connection creation and exits nonzero on failure. Subprocess tests verify counts after reopening, exact limits, failure without partial counts, retained documents, and missing-file/conflicting-mode rejection before file creation. This uses normal database opening/recovery and does not repair data; broader physical/resource qualification remains open.

## Collated collection range resolution

Scope-aware native-column recognition fixes spurious missing-column errors for ranges such as a < b COLLATE NOCASE on collection fields. Differential coverage compares text/NULL results with ordinary SQL for both operand positions, four operators and direct/derived/CTE sources. Parenthesized/unary-plus logical operands were already handled by the existing helper; expanded mixed-native range/BETWEEN cases retain that behavior. Broader expression/type/collation propagation remains open.

## Document BETWEEN native column bounds

The lowerer now converts a preserved logical left operand for native-column BETWEEN/NOT BETWEEN bounds, retaining raw binary payloads and native affinity/collation. A generated scalar subquery isolates physical document collation while retaining the engine's one-evaluation left operand behavior. Record/scalar rejection remains null-aware. Mixed typed/native bounds, arbitrary expressions and volatile native bound evaluation remain qualification work.

## Profiling and audit consumer qualification

The standalone Rust and installed Node package smoke scripts now exercise the public profiling and collection-audit APIs outside the workspace/package source tree. Coverage includes exported result/limit types, typed query results, lossless counters, indexed profiling, repeated metrics, audit limits and retained transaction work, and close/reopen. These remain local path-dependency/offline-tarball checks; registry publishing, prebuilds, platform coverage and release notices remain open.

## Tokenizer resource bounds

The shared tokenizer rejects input above 16 MiB before scanning and more than 262,144 tokens before copying an excess token. Direct SQL, batch splitting, completeness checking and internal tokenizer calls share these limits. Exact-boundary/parser tests and real-engine mutation/transaction checks cover rejection without executing an input prefix. These are lexer bounds; complete AST depth, generated expansion, execution deadlines and result-memory limits remain open.

## Native parser stack protection

SQL execution, profiling, audits and parser/preparation/row-execution callbacks now use same-thread auxiliary stack space when needed, allowing the pinned native depth guard to reject recursive input instead of aborting the process. Regression coverage includes CLI unary/CASE/arithmetic inputs, native/collection paths on a two-MiB Rust thread, Node sync/workers and reprepare after a schema change. The frontend adds only a dependency edge to already-pinned stacker 0.1.22. Broader caller-stack, frontend recursion, memory and platform qualification remain open.

## Native fallback syntax diagnostics

Malformed first statements reaching native fallback now return the pinned parse error through FDB_ENGINE before unresolved managed names can hide it. Tests compare collection/native SELECT, CTE and INSERT SELECT errors with the raw engine, retain active work and single-statement restrictions, and update CLI/Rust/Node recursive-input expectations to the precise depth diagnostic. FastQL write guards retain their separate unexpanded-syntax fallback.

## Instrumented 100k benchmark evidence

A clean-source 100,000-document run now retains measured engine counters, three samples per workload, plans and binary identity. All result/plan assertions passed. Unindexed/indexed filtering read 100,000/2,000 physical rows with 99,999/zero fullscan steps; exact-vector top-10 read 100,000 rows with one sort. Counters matched across all samples. Median CLI times were 5.81 seconds, 284 ms and 30.62 seconds respectively. See benchmarks.md for raw evidence, loading/index costs and memory. This closes the missing-counter gap for the existing synthetic 16-dimensional dev workload; representative dimensions, 1m-scale, optimized/platform/concurrent and full release qualification remain open.

## Seeded vector benchmark reference

The harness now offers a per-record seeded vector fixture alongside the unchanged cyclic default and records fixture/Python versions. Its independent float32-coordinate cosine reference validates distances, top-10 cutoff membership, mandatory nearer records and distance/ID ordering with an explicit roundoff tolerance. Cyclic 16-dimensional and seeded 768-dimensional 1,000-document runs passed all assertions; raw reports are retained. This extends numerical/dimensional benchmark coverage, while real embedding distributions and large high-dimensional release workloads remain open.

## Typed VALUES and CTE insert sources

VALUES cells with logical helpers or boolean/record/composite/vector parameters now retain typed records, booleans, binary, objects and vectors through standalone results, CTEs and derived sources. This fixes encoded record bytes leaking from VALUES CTEs. Leading-WITH collection INSERT VALUES uses the same CTE/source path, with source materialization and atomic validation/index writes. Tests cover typed values and nested access, native delegation, RETURNING, unique/CHECK failure rollback, outer work retention and native targets. Compound operators are described below; scalar/correlated cells, recursive CTEs and broader cross-clause typing remain open.

## VALUES boundary qualification

Additional regressions verify heterogeneous rows, encoded-looking Binary versus record identity, positional parameters, and one evaluation per MATERIALIZED VALUES cell across repeated CTE references. Independent arity, missing-parameter and unused-parameter failures preserve outer work. Both synchronous and worker Node clients retain mixed logical values and pass collection/index audits after a failed multirow unique insert and explicit rollback. The scoped check passes 232 Rust and nineteen Node tests plus strict TypeScript; broader VALUES/subquery and V1 release qualification remain open.

## Typed UNION ALL

Initial UNION ALL support lowers supported SELECT/VALUES arms as one engine query, preserving duplicates and heterogeneous logical values. First-arm output names, positional/name ordering, explicit collation, direction/NULL placement, combined LIMIT/OFFSET, typed CTE/derived consumers, profiling and EXPLAIN are covered. Generated arm definitions share the user CTE scope; a controlled callback checks shared MATERIALIZED evaluation across arms. Collection inserts materialize before atomic document/index writes, including VALUES-first compounds; native targets keep a single engine INSERT and its conflict policies. Ordinary native-only compounds retain delegation.

Three new integration regressions cover types, native/document arm order, nested sources, ordering against native SQL, empty metadata, parameters, rollback/index integrity and native IGNORE. The existing function-counter and both Node client tests include UNION ALL. Pinned native compound ORDER BY rejects direct COLLATE, so that scalar-ordering oracle uses an outer SELECT. Set-operator equality is described below; arbitrary-expression ordering, fetched arms, scalar/correlated subqueries and broader compound planner/resource/cancellation qualification remain unfinished. Full V1 remains incomplete.

## Compound name and parameter scope

UNION ALL ORDER BY now searches output names across arms from left to right, matching the pinned compound resolver while retaining first-arm result labels. Differential tests cover later aliases, leftmost precedence, quoted name casing, three arms and CTE consumers. Boundary tests also verify whole-statement anonymous parameter numbering through LIMIT and a shared encoded-looking Binary value in native and indexed collection predicates, in both arm orders. Arbitrary compound ordering expressions remain unfinished; set operators are described below; full V1 is still open.

## UNION ALL interruption qualification

The controlled interruption matrix now includes plain and MATERIALIZED-CTE UNION ALL collection inserts after mutations begin, in autocommit and explicit transactions. A new source-evaluation test interrupts compound reads and collection inserts after two or four scalar evaluations, requires FDB_CANCELLED with no result prefix, audits source and target document/index integrity, checks prior outer work, retries the exact statement and verifies all six expected values, then checks explicit rollback. These are selected deterministic points; full compound cancellation/resource qualification, native-target interruption behavior and per-operation deadlines remain open.

## Typed set operations

UNION, INTERSECT and EXCEPT now combine supported typed SELECT/VALUES arms, including left-associated mixed chains with UNION ALL. Native scalar-key set operations establish membership; materialized rows and grouped joins recover typed representatives without repeating volatile projections. Numeric/NULL/text-collation equality, canonical record identity and encoded-looking Binary separation are covered. Representatives among equivalent values are unspecified; composite set equality remains unsupported.

Four new integration tests cover scalar/native differential results, mixed chains, multi-column NULL equality, pagination, empty derived metadata, typed CTE/native and collection insert sources, unique-index rollback, collation, records and binary payloads. Both Node clients exercise the new operators. A controlled callback verifies each arm's volatile projection evaluates once. The composite-error regression explicitly observes outer transaction abort on the pinned engine's scalar-function error. Materialization costs, broader cancellation/planner/resource/platform qualification and full V1 remain open.

## Set-collation boundary coverage

A new differential regression checks 48 two-arm collation/operator combinations and 64 three-arm shared-collation chains against the pinned native engine. Fixtures cover NULL, ASCII case differences, trailing spaces and duplicate multiplicity. Result comparison normalizes equivalent text representations while retaining duplicate counts, consistent with the unspecified representative contract. Mixed-collation chains beyond two arms, broader character/collation coverage and full V1 qualification remain open.

## Distinct-set interruption coverage

The existing source-interruption test now covers all four compound operators across reads/inserts, two exact evaluation counts and both transaction modes: 32 combinations. Each case checks FDB_CANCELLED, discarded result rows, intact source/target documents and indexes, prior outer work and exact-statement retry with operator-specific results. The partial-write matrix adds UNION/INTERSECT/EXCEPT and VALUES-first UNION inserts in both transaction modes. Broader cancellation points, native-target interruption, deadlines and full V1 qualification remain open.

## Rust vector constructors

The public Rust Value API now constructs all five pinned vector representations. New vector32_sparse, vector8 and vector1bit methods convert dense float32 inputs through the upstream conversion code and validate the resulting encoding. Dimension bounds are checked before constructor output allocation, including dense vector32/vector64. Two integration tests cover native byte equality, zero/constant/mixed-sign and packing-boundary inputs, persistence/audits, non-finite/empty/oversized rejection and the maximum dimension. The standalone consumer now calls all five constructors. Broader numerical/platform and full V1 qualification remain open.

## Node vector construction

The Node Vector class now exposes float32, float64, sparse32, quantized8 and bit1 factories, accepting numeric arrays and float typed arrays. A bounded binary64 buffer delegates conversion to Rust's validated constructors without opening a connection or using JSON numeric transport. TypeScript exports VectorComponents and matching static methods. Regression coverage includes native byte equality, both clients, audits, dimension/type/finiteness boundaries, float32 overflow, negative zero, ownership and malformed direct-addon input. The packed consumer exercises all five factories and their declarations. Broader numerical/platform and full V1 qualification remain open.

## Vector precision boundaries

Additional Node factory coverage verifies exact IEEE-754 bits at float32 rounding ties, subnormal underflow and negative zero, and binary64 precision/range boundaries. Non-dense factories match native conversion of the checked float32 bytes. Rust and Node reject quantizer scale overflow from finite extreme endpoints while accepting an equal-maximum constant vector. The local Node constructor error preserves active database work. Broader numerical/platform and full V1 qualification remain open.

## Sparse entry constructor (2026-09-07)

Rust Value::vector32_sparse_entries accepts declared dimensions and sorted index/value pairs without a dense intermediate. It validates every index and component before allocating output, omits zeros and preserves trailing-zero dimensions. Two new integration tests cover dense-constructor byte equality, native extraction, typed binding and field validation, persistence/audit, invalid inputs and maximum dimensions. Scoped formatting, Clippy, 247 Rust tests, twenty-one Node tests and strict TypeScript checking pass locally. Broader vector numerical/platform and full V1 release qualification remain open.

## Node sparse entry constructor (2026-09-07)

Vector.sparse32Entries now exposes sparse index/value construction through Node, with readonly SparseVectorEntry tuples in TypeScript. A bounded binary transport calls the Rust constructor with independent native validation and no dense intermediate. New coverage exercises both clients, finite/float32/index/pair/dimension boundaries, copied inputs, empty and maximum-dimension encodings, direct-addon malformed inputs, collection binding/audits and local failure within a transaction. Scoped formatting, Clippy, 247 Rust tests, twenty-two Node tests and strict TypeScript checks pass. Broader numerical/platform and full V1 release qualification remain open.

## Initial typed scalar subqueries (2026-09-07)

Scalar SELECTs over collections and logical CTEs now retain one typed output through supported outer SELECT expressions and collection/native INSERT SELECT. Nested scalar queries, bound inner parameters, arrays/objects/records/binary values, empty-result NULL, ordered first-row selection, scalar arithmetic/comparison and validation rollback have regression coverage. Correlated collection references remain rejected in the current probe; EXISTS/IN subqueries, native-only scalar subqueries inside logical expressions, LIMIT/OFFSET and broader alias/window/planner/resource qualification remain unfinished.

Evaluation stays in the engine statement. Counter tests show one evaluation for an uncorrelated scalar subquery used across multiple outer rows. They also establish a pinned native limitation: a subquery in an unused CASE branch still executes once; the typed route matches the native probe. This does not change ordinary CASE branch behavior without scalar subqueries. Full V1 remains incomplete.

The final local scoped check for this slice passed formatting, Clippy with warnings denied, 249 Rust tests, twenty-three Node tests and strict TypeScript declarations. Both Node clients preserve scalar-subquery records/arrays/NULL and scalar comparison results. No upstream-source, dependency or persisted-format changes.

## Initial collection EXISTS support (2026-09-07)

EXISTS/NOT EXISTS now use the expression-subquery plan cache for supported uncorrelated collection and logical CTE sources. Results retain native SQL integer 0/1. The inner query can project multiple columns or collection stars and retains filtering, grouping, compounds and pagination. Integration coverage compares fourteen positive/negative cases with native SQL and exercises outer filtering, CTE parameters and both insert targets. Native differential counters check ignored projection evaluation and first-match filtering; both Node clients check typed-input existence predicates. Correlated collection references, IN subqueries and broader native-inner/cross-clause/resource qualification remain unfinished. Full V1 remains incomplete.

The local scoped EXISTS check passed formatting, Clippy with warnings denied, 250 Rust tests, twenty-three Node tests and strict TypeScript declarations. No upstream-source, dependency or persisted-format changes.
