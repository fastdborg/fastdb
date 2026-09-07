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
- Dedicated-worker AsyncDatabase with ordered submissions, bounded request queues, graceful close, fatal transport cleanup and initial query AbortSignal cancellation; cancellation for other operation types and broader lifecycle qualification remain open.
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

The scoped test suite includes parser collision probes; persistent CRUD/reopen; mixed transaction rollback; failed unique inserts/updates/index builds; validation-definition rollback; typed round trips; numeric/index identity; a child process that exits without closing an active transaction; and differential ordinary SQL probes against the pinned engine. On 2026-09-07, scoped checks passed formatting, Clippy with warnings denied for the FastDB packages, and 280 Rust tests plus thirty-two Node tests (including the application template) (one additional trigger-interruption release-gate regression is ignored; see the latest entry) (including an isolated worker-transport fault test) and strict TypeScript declaration checks (Rust coverage includes a CLI recursive-input subprocess test, a schema-reprepare stack unit test, two CLI audit subprocess tests, five collection-content audit unit tests and a persistent audit integration test, two SELECT profiling tests, three reference-validation tests, three standalone-parameter tests, two numeric-precision tests, three interruption tests plus two catalog/index cancellation unit tests a mixed-CTE cancellation unit test and a compound/subquery source-evaluation cancellation unit test and a native-target subquery cancellation unit test, four migration tests, four transfer tests, three window tests, three derived-source tests, twenty-five scalar/EXISTS/IN-subquery tests, seven CTE tests, ten compound tests, two mixed-star tests, seven grouping tests, five bundled-runtime/function tests, ten vector tests, four forward-link tests, five batch tests, eight transaction-report tests, two CLI output-failure tests, a history-path collision unit test, a real-terminal subprocess test and seven CLI script/transaction subprocess tests, eight RETURNING tests, six INSERT SELECT tests, five CASE tests, eight SQL-helper tests, six document-expression tests, nineteen CHECK/upgrade tests, twelve catalog lifecycle/version/isolation/inspection tests plus a metadata-corruption unit test and four managed-schema unit tests, the typed BETWEEN evaluation-count test, the offline restore rehearsal, the process-kill stress test and its subprocess helper, the original subprocess helper, forty collection SELECT tests plus a lowering/execution unit test, and seven SQL-shaped write/constructor tests). This is local Linux evidence; hosted CI has not run. The process-exit test is a basic recovery smoke, not interrupted-checkpoint or power-loss certification.

## Next implementation work

See [V1 gate review](v1-gates.md) for the current evidence map and a reproduced native-membership query gap.

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

A two-connection test checks that a fetch inside an established read transaction sees the earlier snapshot after another connection commits a target update. Complete interleaving/crash/resource stress remains a release gate. The current EXPLAIN output describes the outer engine query, not target-batch details. The initial reference-count limit is now supplemented by the encoded-value budgets described below. Outer-query materialization, target-batch instrumentation and full resource budgets remain open.

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

## Initial collection IN/NOT IN subqueries (2026-09-07)

Supported one-column uncorrelated collection/logical-CTE sources now lower to scalar/record membership keys. Initial coverage preserves SQL NULL and empty-set behavior, numeric/text distinctions, record versus binary identity, native integer affinity and native BLOB left operands. It includes earlier CTE references, bound values, a scalar subquery on the left and collection INSERT SELECT filtering. A native-column branch initially evaluated its RHS twice; a shared materialized source fixes the observed duplication and a counter regression checks two source rows are evaluated once each. Both Node clients exercise record membership, NOT IN and NULL results. Correlated collection queries, row-value/composite membership, broader native-inner/collation/write-source behavior and resource/platform/release qualification remain unfinished.

The scoped membership check passed formatting, Clippy with warnings denied, 252 Rust tests, twenty-three Node tests and strict TypeScript declarations locally. No upstream-source, dependency or persisted-format changes. Full V1 remains incomplete.

## Membership affinity/collation correction (2026-09-07)

A broader IN/NOT IN matrix found that decoding a document field directly as a SQL function erased the distinction between typeless column affinity and expression affinity. A TEXT cast on the left consequently matched numeric RHS values that the native typeless-column query rejected. A derived column boundary after decoding fixes direct column-shaped projections while leaving unary plus and computed expressions without that boundary. The shared native-column materialization path also removes temporary column affinity for computed RHS projections, fixing a second mismatch exposed by unary plus. The matrix now compares 504 query pairs across seven native column declarations, six left operand forms, six RHS projections and both operators, each with ten numeric/text/NULL/BLOB inputs. Broader RHS casts, mixed native projection metadata, correlated sources and resource/platform/release qualification remain unfinished.

The final scoped affinity check passed formatting, Clippy with warnings denied, 253 Rust tests, twenty-three Node tests and strict TypeScript declarations. The matrix passed all 504 comparisons; both Node clients also cover the two observed failures. No upstream-source, dependency or persisted-format changes. Full V1 remains incomplete.

## Subquery write rollback qualification (2026-09-07)

A new regression forces late unique-index failures in collection INSERT SELECT sources using IN, NOT IN, EXISTS and scalar comparison predicates. It verifies original target IDs/values and prior transaction work, index-backed absence of partial rows, integrity audits, retry success and final outer rollback with the source unchanged. The scoped check passed formatting, Clippy with warnings denied, 254 Rust tests, twenty-three Node tests and strict TypeScript declarations. This is evidence for managed collection uniqueness failures, not a promise that all engine errors retain an outer transaction. Full V1 and broader cancellation/resource/platform qualification remain incomplete.

## Subquery cancellation qualification (2026-09-07)

The existing deterministic source-cancellation test now includes IN, NOT IN, EXISTS and scalar aggregate subqueries: 32 additional cases cover two source evaluation points, reads versus collection inserts and autocommit versus explicit transactions. All require FDB_CANCELLED, exact callback counts, preserved prior work/transaction state, empty audited targets, unchanged audited sources and exact successful retry. The full scoped check passed formatting, Clippy with warnings denied, 254 Rust tests, twenty-three Node tests and strict TypeScript declarations. No runtime or upstream changes. Per-operation client cancellation, broader native-target cancellation, hard deadlines and full V1 release qualification remain open.

## Native-target subquery cancellation (2026-09-07)

A new deterministic test compares collection-backed and native-only INSERT SELECT into a native unique table for IN, NOT IN, EXISTS and scalar aggregate predicates. Sixteen pairs cover two source callback boundaries and autocommit/explicit transactions. They require identical cancellation and transaction/prior-work observations, empty targets, unchanged audited collection sources, exact retry rows/counts and rollback when the transaction remains active. The focused test passed; interruption after partial native writes, per-operation client cancellation and broader resource/platform/release qualification remain open.

The full scoped native-target cancellation check passed formatting, Clippy with warnings denied, 255 Rust tests, twenty-three Node tests and strict TypeScript declarations locally. This slice changes test coverage and documentation only. Full V1 remains incomplete.

## Pinned trigger interruption defect — release gate (2026-09-07)

An AFTER INSERT trigger writes a side-effect row then increments a test counter; interrupting at that point returns FDB_BUSY rather than FDB_CANCELLED. The pinned OpProgram executor explicitly merges StepResult::Interrupt and Busy into LimboError::Busy. The first native/autocommit probe rolled back target and side-effect rows, but broader after-write cleanup/disposition remains unqualified. This is an observed V1 release gate, not completed cancellation support.

[The proposal and reproducer](trigger-interrupt.md) include a minimal core patch, checked for clean application but not applied or validated. Upstream core changes await the separately reviewed decision required by FastDB-Workflow.md. The new strict regression is ignored in routine checks and fails when run explicitly. Existing source-boundary cases now also verify trigger side effects during retry. Scoped formatting, Clippy, 255 Rust tests, twenty-three Node tests and strict TypeScript checks pass with that one known release gate ignored. Full V1 remains incomplete.

## Source-free typed nested SELECTs (2026-09-07)

Typed parameters and expanded FastQL helpers now bypass the native-only early return for source-free nested SELECTs. A new regression checks records, booleans, arrays, objects, binary and vectors through scalar/nested scalar queries, CTEs, derived tables and EXISTS, plus source-free record membership, numbered parameters and collection inserts. Native targets reject implicit array conversion. Binary-only CTE/derived queries retain their native route and existing column labels; binary expression subqueries opt into typed lowering. This fixes a compatibility failure found by the existing binary CTE/index-parameter test. Both Node clients exercise scalar and CTE typed roundtrips. The pinned trigger interruption release gate remains unresolved and its core proposal remains pending.

The final scoped source-free check passed formatting, Clippy with warnings denied, 256 Rust tests and twenty-three Node tests plus strict TypeScript checking; the one known trigger-interruption release gate remains ignored. No upstream-source, dependency or persisted-format changes. Full V1 remains incomplete.

## Nested anonymous parameter qualification (2026-09-07)

A new regression verifies statement-wide anonymous parameter numbering across outer/scalar/CTE queries using scalar, array and encoded-looking binary values. Missing/unused insert-source bindings preserve prior IDs/values, transaction state and index integrity; corrected retry and final rollback succeed. Both Node clients also verify anonymous scalar-subquery binding. Scoped formatting, Clippy, 257 Rust tests, twenty-three Node tests and strict TypeScript checks pass; the known trigger-interruption gate remains ignored and unresolved. Full V1 remains incomplete.

## Explicit typed projections over native nested sources (2026-09-07)

Native-source nested SELECTs now retain explicit typed-parameter/helper projections in scalar queries, EXISTS/IN, CTEs and derived tables. Regression coverage includes all logical value families, empty scalar results, binary predicates, record construction/membership and collection insertion; both Node clients exercise native-source scalar and CTE values. A native-only binary-filter oracle initially exposed lost INTEGER affinity, so native sources opt into logical lowering based on their projections rather than predicate-only parameters. The final scoped check passed formatting, Clippy, 258 Rust tests, twenty-three Node tests and strict TypeScript checking, with the known trigger-interruption gate still ignored. No upstream/dependency/encoding changes. Full V1 remains incomplete.

## Initial scalar-subquery pagination (2026-09-07)

Single-core collection SELECTs now lower supported uncorrelated logical scalar subqueries in LIMIT and OFFSET, including DISTINCT and collection INSERT SELECT. Pagination uses a separate scope without outer fields or projection aliases. Regression coverage compares positive, zero and negative limits and offsets with native SQL, preserves native-only limit queries, checks bound inner parameters, and verifies an empty scalar limit rejects an insert without target changes. Both Node clients exercise bound pagination with DISTINCT.

An outer CTE referenced inside LIMIT fails in both the pinned native engine and the logical route; this remains a limitation. Compound-query pagination subqueries, correlated queries and broader type/resource qualification remain unfinished. No upstream implementation, dependency or persisted-format changes.

The final scoped check passed formatting, Clippy, 259 Rust tests, twenty-three Node tests and strict TypeScript checking. The known trigger-interruption release gate remains ignored and unresolved. Full V1 remains incomplete.

## Compound scalar-subquery pagination (2026-09-07)

Logical compound SELECTs now lower supported uncorrelated scalar subqueries in LIMIT/OFFSET independently of arm fields and output aliases. Initial coverage includes UNION ALL, UNION, INTERSECT and EXCEPT, positive/zero/negative limits, offsets, bound values, native arms with a logical limit source, and successful/failed collection INSERT SELECT. Both Node clients exercise a bound UNION limit.

The pinned engine rejects the tested direct native compound LIMIT/OFFSET subquery form with datatype mismatch. Successful differential comparisons therefore use an equivalent native derived-table wrapper with outer pagination; the direct native rejection is retained separately. This is not a claim of complete native compound-subquery compatibility. Outer CTE visibility, correlation, broader types and resource/platform qualification remain open. No upstream implementation or persisted encoding changes.

Scoped formatting, Clippy, 260 Rust tests, twenty-three Node tests and strict TypeScript checks pass. The known trigger-interruption release gate remains ignored and unresolved. Full V1 remains incomplete.

## Pagination evaluation and failure qualification (2026-09-07)

A native test-only counter verifies one evaluation each of uncorrelated LIMIT and OFFSET scalar subqueries for plain SELECT, DISTINCT, UNION ALL and UNION. Four query shapes return the expected row counts with exactly two callback invocations per statement.

A transaction regression checks NULL, fractional, invalid-text and outer-field-dependent limits for plain, DISTINCT and UNION collection INSERT SELECT. Each failure preserves the active transaction, prior record IDs/values and managed-index integrity. A corrected insertion succeeds and final rollback removes all transaction-local target rows. This is bounded evaluation/atomicity evidence; cancellation during pagination, broader types/aliases and resource/platform coverage remain unqualified.

The final scoped check passed formatting, Clippy, 261 Rust tests, twenty-three Node tests and strict TypeScript checking. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Pagination cancellation qualification (2026-09-07)

The compound/subquery interruption regression now includes LIMIT and OFFSET scalar sources for plain and UNION queries. Thirty-two new cases cover read/collection-insert execution, callback thresholds two/four and autocommit/explicit transactions. Each requires FDB_CANCELLED at the exact threshold, no returned partial rowset, empty target/index state, intact source documents and prior transaction work, and exact successful retry; explicit transactions also verify final rollback. The complete matrix now covers ninety-six cases.

OFFSET probes use distinct row-dependent callback arguments to reach both cancellation thresholds; repeating an identical callback expression did not reach the fourth-call threshold in the diagnostic. This is source-phase cancellation evidence. Trigger after-write interruption remains a separate unresolved release gate, and per-operation cancellation, hard resource caps and platform qualification remain incomplete.

The final scoped check passed formatting, Clippy, 261 Rust tests, twenty-three Node tests and strict TypeScript checks. One known trigger-interruption release gate remains ignored; full V1 remains incomplete.

## Native scalar sources in logical queries (2026-09-07)

A native-only scalar SELECT can now supply a value inside a collection/logical SELECT without being traversed as an outer document expression. The frontend packs its native result at the typed boundary after selecting logical lowering; a native-only scalar source does not itself opt ordinary SQL into that route. Initial tests cover INTEGER, TEXT and encoded-looking BLOB values, empty-result NULL, named parameters, filtering, multi-column rejection and collection INSERT SELECT. Both Node clients project a native-table scalar result through a collection query.

This does not establish general native EXISTS/IN support inside logical expressions, correlated collection access, logical VALUES coverage or complete mixed-affinity/collation equivalence. Those and broader resource/platform qualification remain open. No upstream source or persisted-format changes.

A missing inner binding initially fell through as native NULL; the typed boundary now validates recorded native scalar parameters. The existing missing/unused-parameter transaction regression passes. Final scoped checks passed formatting, Clippy, 262 Rust tests, twenty-three Node tests and strict TypeScript checks. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Native EXISTS sources in logical queries (2026-09-07)

Native-only EXISTS/NOT EXISTS sources now remain native expressions inside collection/logical SELECTs. They do not independently opt ordinary SQL into logical lowering. The typed boundary validates their bound parameters before collection writes. Differential cases cover stars, multiple projections, empty sources, count aggregates and inner LIMIT/OFFSET. Missing-parameter insertion preserves prior IDs/values, index integrity and transaction state; corrected retry and final rollback succeed. A callback regression checks that unused native EXISTS projections are not evaluated, and both Node clients exercise a bound native EXISTS projection.

Native IN sources, correlated collection references, broader CTE/alias behavior and resource/platform qualification remain unfinished. No upstream implementation or persisted-format changes.

Final scoped checks passed formatting, Clippy, 263 Rust tests, twenty-three Node tests and strict TypeScript checking. The known trigger-interruption release gate remains ignored and unresolved; full V1 remains incomplete.

## Native scalar comparison affinity (2026-09-07)

Direct native scalar subqueries compared with typed document values now retain native affinity through a shared first-row result. A typeless document column boundary preserves the distinction between TEXT affinity and no affinity; native prepared result metadata restores declared collation with operand-order precedence. Equality keeps native BLOB and typed record/binary comparison identities separate, while range comparisons use the existing mixed-scalar conversion. Native-only statements keep their engine route.

The regression compares forty-eight query pairs (INTEGER, TEXT and TEXT COLLATE NOCASE sources; eight equality/range operators; both operand orders) over string, integer and NULL document values. Additional coverage checks native-only correlation, binary-versus-record identity and one native source evaluation across multiple outer rows. This is initial direct-scalar comparison coverage: explicit COLLATE wrappers, broader computed/compound/CTE metadata, volatile logical operands and resource/platform qualification remain open. No upstream implementation or encoding changes.

Final scoped checks passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checking. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Explicit collation around native scalars (2026-09-07)

Native scalar comparison recognition now sees through parentheses and outer COLLATE wrappers without losing native affinity. Explicit outer collation uses left-operand precedence when both sides specify one, and document operand wrappers retain their comparison role. The native scalar result remains shared rather than reevaluated for each comparison branch.

The existing affinity matrix now contains 768 native differential query pairs: three native column declarations, four inner projection collation forms, eight comparison operators and eight operand/wrapper arrangements, including conflicting BINARY/NOCASE/RTRIM collations. A callback case checks once-only native evaluation with an outer wrapper; both Node clients exercise a wrapped numeric scalar. Unary-plus/CAST and broader computed/compound/CTE metadata, volatile logical operands and resource/platform qualification remain open. No upstream implementation or encoding changes.

Final scoped checks passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checking. One known trigger-interruption release gate remains ignored and unresolved; full V1 remains incomplete.

## Trailing-space collation qualification (2026-09-07)

The native scalar comparison matrix now uses nine left-hand values, including mixed case, one/two trailing spaces, a trailing tab, empty text, integer and NULL. A native TEXT COLLATE RTRIM source joins the prior INTEGER/TEXT/NOCASE declarations. The matrix contains 1,024 query pairs across four declarations, four inner projection forms, eight operators and eight operand/wrapper arrangements (9,216 compared result cells per route). These inputs distinguish trailing-space trimming from case folding and preserve a tab as a separate test case. This expands behavioral evidence without changing production lowering.

Final scoped checks passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checking. One known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Computed native scalar projection affinity (2026-09-07)

The native scalar comparison matrix now includes +v, CAST(v AS TEXT) and CAST(v AS NUMERIC) inside the scalar SELECT. All 1,792 query pairs in the focused probe match native SQL over nine values: four native declarations, seven inner projection forms, eight operators and eight operand/wrapper arrangements (16,128 compared result cells per route). This verifies initial affinity removal and explicit type conversion within the native scalar source. It does not qualify unary-plus/CAST around the outer scalar expression or every computed/compound/CTE form. No production implementation or upstream changes.

Final scoped checks passed formatting, Clippy, 265 Rust tests, twenty-three Node tests and strict TypeScript checks. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Native scalar comparison write rollback (2026-09-07)

A new regression applies native INTEGER scalar comparisons to document string values in collection INSERT SELECT. Four predicate forms cover both operand orders and explicit COLLATE wrappers. A late unique-index conflict preserves prior IDs/values, active transaction state and index integrity, with no partially inserted indexed rows. Each corrected retry inserts the exact expected rows; final rollback leaves the target and its index empty. This qualifies the tested comparison source through the write/savepoint path without changing production code. Broader native-target conflict policies, cancellation during these comparisons and release/platform qualification remain open.

Final scoped checks passed formatting, Clippy, 266 Rust tests, twenty-three Node tests and strict TypeScript checking. The known trigger-interruption release gate remains ignored and unresolved; full V1 remains incomplete.

## Operation-scoped Rust cancellation tokens (2026-09-07)

Rust now exports CancellationToken with new/clone/cancel/is_cancelled and Connection::execute_cancellable/execute_report_cancellable. A token is sticky, connection-independent and used only by explicitly associated executions. A pre-cancelled execution fails before parsing/writes. Active execution polls at engine progress boundaries; one delivered interruption permits cleanup, and an RAII guard removes the handler on return/unwind. Calls remain serialized and cancellation is best effort without a fixed non-engine latency bound.

Tests cover a token cancelled from another thread before execution, prior transaction preservation, fresh-token retry, late cancellation not affecting later SQL, and deterministic interruption during collection reads/insert sources in autocommit and explicit transactions. They verify cancellation codes, callback counts, target/index integrity and retry/rollback. Node request-scoped AbortSignal integration, cancellation performance and broader runtime/platform qualification remain open. The trigger-interruption release gate is unchanged. No upstream source changes.

Final scoped checks passed formatting, Clippy, 268 Rust tests, twenty-three Node tests and strict TypeScript checks. The two new token tests cover pre-cancellation/late cancellation and four active read/insert transaction combinations. One known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Async query AbortSignal integration (2026-09-07)

AsyncDatabase execute/all/first/exactlyOne now accept an optional third argument with signal. Each signalled request uses a separate native CancellationToken registry entry, shared across the main thread and dedicated worker. Queued abortion is checked before SQL execution when the request reaches its turn; active abortion polls at engine progress boundaries. Results retain normal transaction reports and completion may win the race. Listeners and registry entries are released on success/error, send failure and worker failure. The registry is bounded at 16,384 entries across clients; accepted queue order and per-client bounds remain unchanged.

Initial tests cover queued and active query cancellation, following-request isolation, pre-aborted requests, prior transaction/index integrity, late abort, listener cleanup, invalid signals and transport-failure token release. Strict TypeScript declarations cover the options on all four query helpers. Profiling/audits/batches/migrations/transfers, deadlines, broader lifecycle/platform/performance qualification and the pinned trigger-interruption gate remain open. No upstream implementation changes.

Final scoped checks passed formatting, Clippy, 268 Rust tests, twenty-four Node tests and strict TypeScript checking. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Cancellable SELECT profiling (2026-09-07)

Rust Connection::profile_select_cancellable and AsyncDatabase.profileSelect(sql, parameters?, {signal}?) now share the operation-scoped token guard used by query execution. Native profiling retains its result/metrics shape and transaction observations under result.transaction. Cancellation rejects without partial metrics and does not cancel the next queued operation. The Node regression covers active and pre-cancelled profiling, preserved prior transaction data, complete retry metrics, listener disposal and late-abort isolation. TypeScript accepts the new options. Audits, batches, migrations, transfers, deadlines and broader runtime/platform/performance qualification remain open; the trigger-interruption gate is unchanged.

Final scoped checks passed formatting, Clippy, 268 Rust tests, twenty-five Node tests and strict TypeScript checking. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Cancellable collection integrity audits (2026-09-07)

Rust Connection::check_collection_integrity_cancellable and AsyncDatabase.checkCollectionIntegrity(table, limits?, {signal}?) now use operation-scoped cancellation. The worker/native bridge retains existing audit counters and transaction observations. The 1,000-document indexed regression checks active/pre-cancellation, following-query isolation, prior transaction data, exact 1,001-document/index-entry retry counts, listener disposal and final rollback. TypeScript accepts audit options; sync calls retain their existing signature.

A first 10,000-document fixture was deliberately stopped after over 80 seconds of CPU without completing. That uninstrumented workload includes population and audit phases; the expensive phase is not yet established. The routine fixture was reduced to keep CI bounded. Larger-scale population/audit performance remains an explicit investigation/release gap, alongside batches/transfers/migrations cancellation, deadlines and broader platform qualification. The pinned trigger-interruption gate remains unresolved. No upstream source changes.

Final scoped checks passed formatting, Clippy, 268 Rust tests, twenty-six Node tests and strict TypeScript checking. The known trigger-interruption gate remains ignored and unresolved; full V1 remains incomplete.

## Index audit scan scaling (2026-09-07)

The prior audit counted matching index entries separately for every document, producing repeated scans and poor scaling. It now walks each index once, resolves each referenced document by primary key, compares its expected key with native IS semantics and rejects duplicate IDs, stale keys, orphan entries or missing coverage. It retains at most one validated ID per audited document for each index pass; the existing document count/encoded-byte bounds limit retained valid IDs. The audit still runs in one snapshot and preserves cancellation/error cleanup.

A VM-step regression covers duplicate and NULL keys at 64/256 documents. Diagnostic timings at 1,000 documents improved from about 3.40 seconds to 0.79 seconds for auditing, with insertion near 2.5 seconds. The 10,000-row workload now completed: 27.7 seconds insertion and 8.65 seconds audit. The new maintainer script fastdb/scripts/bench-audit.cjs reproduces separate phases; these debug measurements do not complete release-scale performance qualification. Existing corruption and cancellation tests remain required. No upstream or persisted-schema changes.

Final scoped checks passed formatting, Clippy, 269 Rust tests and twenty-six Node tests plus strict TypeScript checking. One known trigger-interruption release gate remains ignored; full V1 remains incomplete.


## Operation-scoped batch cancellation (2026-09-07)

Rust execute_batch_cancellable/visit_batch_cancellable and AsyncDatabase.executeBatch(script, {signal}?) now support batch cancellation. Pre-cancellation rejects before splitting; after splitting, the affected statement produces the final error report with its byte offset and transaction state. Earlier successful statements retain their effects, later statements are skipped, and explicit transaction ownership remains with the caller. Each statement checks the sticky token and uses cooperative engine interruption; visitors and other non-engine work have no fixed cancellation latency.

Regression coverage checks cancellation between statements in autocommit and explicit transactions, active scalar/read and collection-insert cancellation, retained reports/UTF-8 offsets, prior indexed data, retry and rollback. The real-worker test additionally checks AbortSignal listener cleanup and following-request isolation. Migration/transfer cancellation, deadlines, broader platform/lifecycle qualification and the pinned trigger-interruption release gate remain open. No upstream source changes.

Final scoped checks passed formatting, Clippy, 270 Rust tests, twenty-seven Node tests and strict TypeScript checks. One known trigger-interruption regression remains ignored; full V1 remains incomplete.


## Rust document-transfer cancellation (2026-09-07)

Rust import_documents_cancellable and export_documents_cancellable now accept operation-scoped CancellationToken values. They reuse the transfer atomic scope and engine progress cancellation with cleanup permitted after interruption. Pre-cancellation rejects before parsing/catalog work; serialization and parsing have no fixed cancellation latency, and completion can win a race. Node transfer AbortSignal integration and broader active-token/resource/platform qualification remain open.

Final scoped checks passed formatting, Clippy, 271 Rust tests, twenty-seven Node tests and strict TypeScript checking. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Node document-transfer cancellation (2026-09-07)

AsyncDatabase.importDocuments and exportDocuments accept an optional final {signal} argument. The native bridge selects the Rust cancellable transfer API and retains existing error/transaction reporting, queue ordering and token/listener cleanup. Imports preserve the existing atomic scope; exports return complete strings or errors. Parsing/serialization latency and broader resource/platform qualification remain open, along with migration cancellation and the pinned trigger-interruption gate. No upstream or stored-format changes.

Final scoped checks passed formatting, Clippy, 271 Rust tests, twenty-eight Node tests and strict TypeScript checking. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Migration cancellation (2026-09-07)

Rust migrate_cancellable and AsyncDatabase.migrate(plan, {signal}?) now apply operation-scoped cancellation to the migration runner. Pending schema/data/history share the existing atomic scope. Cancellation errors retain migration context and expose FDB_CANCELLED, while other migration execution errors retain FDB_MIGRATION. Pre-cancellation precedes Rust validation; JavaScript plan validation still occurs before submission. Deadline, lifecycle/platform qualification and the pinned trigger-interruption gate remain open.

Final scoped checks passed formatting, Clippy, 271 Rust tests, twenty-nine Node tests and strict TypeScript checking. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Cancellation queue and close coverage (2026-09-07)

A real-worker regression now queues cancelled batches, profiles, audits, transfers and migrations behind an interrupted query, then closes the connection. It checks cancellation transaction observations, idempotent close, new-request rejection during closing, listener disposal, dead interrupt handle and file reopening with only committed data/indexes retained. A transport fixture fills all 256 request slots with signalled work, verifies rejected requests gain no listeners and aborted queued requests retain slots, then verifies failure releases all accepted tokens/listeners and closes the worker. This extends lifecycle evidence without claiming exhaustive interleavings, native crash recovery or platform qualification.

Final scoped checks passed formatting, Clippy, 271 Rust tests, thirty Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Native addon load diagnostics (2026-09-07)

The Node entrypoint and worker share a packaged native loader. Missing or unloadable addons throw FDB_NATIVE_LOAD with platform/architecture/Node identity, source-build guidance and the original cause. The installed-package smoke verifies the new runtime file inventory and missing/invalid-addon failures inside its temporary consumer installation. Automatic platform selection, prebuilds and broader platform/Node qualification remain open.

Installed-package smoke and full scoped checks passed: formatting, Clippy, 271 Rust tests, thirty Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Installed cancellation API coverage (2026-09-07)

The offline installed-package smoke now covers pre-aborted calls for every signalled async API, preserved active work, listener cleanup, fresh-token batch results and late cancellation isolation. All signal signatures compile against declarations in the installed tarball. The Linux x64/Node 24.19.0 smoke passed; this does not qualify other runtimes/platforms or release prebuilds.


## Native membership sources in logical queries (2026-09-07)

Initial native IN/NOT IN subqueries now lower inside collection/logical queries. The lowering retains native RHS affinity and resolved collation, compares raw BLOB values through collision-resistant keys, and computes match/NULL/empty-set outcomes from a materialized comparison source. Ordinary native-only statements remain on their native route. Consumed membership parameters are explicitly validated before execution.

Differential coverage includes INTEGER, TEXT, NOCASE/RTRIM TEXT and BLOB declarations, direct/unary-plus/CAST RHS projections, NULL-containing/non-NULL/empty sources and both membership operators. Additional checks cover native bytes shaped like an encoded record, missing parameters and unique-failure rollback/retry for collection INSERT SELECT. Broader LHS wrappers, correlation/CTE metadata, volatile evaluation counts, cancellation and planner/performance qualification remain open. This initial materializing implementation is not a performance certification. No upstream source changes.

Final scoped checks passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript checking. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Native membership operand affinity (2026-09-07)

The native membership lowering now restores CAST affinity on both operands and removes the artificial column affinity introduced when materializing a computed RHS. The expanded differential matrix covers seven LHS forms (field, unary plus, parentheses, three explicit collations and CAST TEXT), eight scalar/NULL/trailing-space values, five RHS declarations, three RHS projections, three source predicates and IN/NOT IN. This closes a reproduced CAST TEXT versus unary-plus RHS coercion mismatch. Broader computed/CTE/correlated metadata, volatile evaluation and planner/resource qualification remain open.

Final scoped checks passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Native membership cancellation checkpoints (2026-09-07)

The deterministic source-interruption matrix now includes native IN/NOT IN sources in logical queries. Sixteen added combinations cover reads/inserts, autocommit/explicit transactions and cancellation after two/four scalar source callbacks. Checks require FDB_CANCELLED, exact callback counts, preserved prior work/source documents, empty target/index state, exact retry rows and rollback. This tests selected engine-progress checkpoints, not all instruction boundaries or volatile evaluation equivalence.

Final scoped checks passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Shared native membership evaluation (2026-09-07)

Native membership sources now use a generated materialized CTE at the enclosing SELECT scope, and final comparison uses native IN/NOT IN execution. This replaces per-row aggregate comparisons that re-evaluated a native source for each collection row. The logical left operand is separately materialized once per outer evaluation; the BLOB branch converts RHS comparison keys while the scalar branch retains affinity/collation handling. Generated names avoid source-text collisions.

Deterministic callback probes compare native and collection routes for source evaluation and volatile left operands. Broader CTE/correlation/compound source semantics and planner/resource qualification remain open. No upstream source changes.

Final scoped checks passed formatting, Clippy, 273 Rust tests, thirty Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Compound native membership compiler crash correction (2026-09-07)

A native membership projection in a compound collection query triggered the pinned compiler's `No index cursor found for table` panic. The correlated left-value CTE now uses NOT MATERIALIZED, preventing premature materialization before its outer cursor exists. The shared uncorrelated RHS remains materialized. Tests cover all four set operators against an ordinary-table oracle; native/collection callback evaluation counts remain covered. Node sync/worker regressions include the former crashing UNION ALL and a native CTE membership source. Broader nested/correlated/CTE metadata and planner qualification remain open. No upstream source changes.

Final scoped checks passed formatting, Clippy, 274 Rust tests, thirty-one Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Outer CTE scope in membership compounds (2026-09-07)

Compound-arm native expression metadata preparation now receives enclosing CTE definitions. Generated arm CTEs are placed at the compound scope with arm-specific membership names, so native membership sources can resolve earlier outer CTEs during final preparation. Logical compound lowering also validates already-consumed CTE parameters before execution; native-only queries retain their existing route. The new differential regression combines a parameterized native CTE with both membership operators in all four compound forms and checks missing bindings. Broader nested scopes, recursive/correlated queries and metadata qualification remain open.

Final scoped checks passed formatting, Clippy, 275 Rust tests, thirty-one Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## CTE membership compound write atomicity (2026-09-07)

A new collection INSERT SELECT regression combines a parameterized outer native CTE, membership projection and UNION ALL. In both transaction modes it checks FDB_PARAMETER before writes, late uniqueness failure with prior target/index state intact, exact three-row retry and final explicit rollback. This qualifies a write path through the preceding scope fix; it does not establish all compound/write/correlation combinations.

Final scoped checks passed formatting, Clippy, 276 Rust tests, thirty-one Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Membership in derived queries and later CTEs (2026-09-07)

Derived-source lowering now receives the enclosing native CTE scope. CTE definitions are assembled in resolved order; generated definitions from a body without its own WITH are placed before that body's CTE, making preceding sources visible during final preparation. User-local WITH bodies keep their existing scope. Tests cover membership inside a derived query, a later CTE and a further CTE consuming its result. Broader deeply nested/local-shadowing/correlated metadata remains unqualified.

Final scoped checks passed formatting, Clippy, 277 Rust tests, thirty-one Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Same-name CTE resolution in nested membership (2026-09-07)

Native metadata probes now preserve inherited/local WITH levels instead of concatenating duplicate names. A direct raw-engine probe establishes that the pinned resolver chooses the enclosing same-name CTE in the tested derived-query and later-CTE forms. Logical lowering retains that definition before adding generated scopes, so collection and ordinary-table results agree with the raw engine. This is pinned behavior, not a claim of general SQLite lexical shadowing semantics. Broader recursive/deeper-scope qualification remains open.

Enclosing-definition preservation applies only to inherited native CTEs. Existing logical collection CTE shadowing retains its prior resolution and regression coverage.

Final scoped checks passed formatting, Clippy, 278 Rust tests, thirty-one Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Node task-tracker application template (2026-09-07)

fastdb/examples/node-task-tracker provides a runnable asynchronous Node storage-layer template with migrations, typed record/boolean parameters, field validation, a managed index, one-hop owner expansion and atomic task/event writes. Its smoke test forces the second transactional write to fail, verifies document/index rollback, retries, rejects duplicate completion, reopens through migration validation and exports NDJSON. The CLI demo was run twice against one temporary file without duplicating its seeded task. The example test is now included in check-node.sh. This supplies an initial tested template, not external pilot or production qualification.

Final scoped checks passed formatting, Clippy, 278 Rust tests, thirty-two Node tests including the template, and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## AI-assisted application guide (2026-09-07)

[AI application guide](ai-application-guide.md) provides a complete runnable task-tracker workflow, current syntax/type/result rules, transaction ownership, migration/cancellation/transfer behavior and suggested application-agent instructions. The JavaScript code block was extracted and executed from the repository root; its completed task and fetched owner were asserted, and the template smoke passed again. This is initial tested guidance, not external developer/pilot validation.


## Incremental document export encoding (2026-09-07)

Export now processes engine rows incrementally and serializes each document directly into an output writer that rejects bytes beyond the existing 64 MiB limit. It no longer retains a full engine rowset, decoded document vector and per-document JSON strings before returning. The API still returns one complete string; the current decoded row, vector capacity and engine allocations remain outside a hard total-memory bound. Existing snapshot/atomic scope and cancellation cleanup are retained. No stored or transfer format change.

Final scoped checks passed formatting, Clippy, 279 Rust tests, thirty-two Node tests and strict TypeScript checks. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## NDJSON import preflight and replay (2026-09-07)

NDJSON import no longer retains the full portable/document vectors. It validates header, each typed document and document count one line at a time, then replays the immutable input inside the existing atomic insert scope. JSON import retains its existing materializing path. Parsing twice is an explicit CPU tradeoff; input/current-document/engine memory and parsing latency are not hard bounded beyond existing limits. Late-invalid-entry tests inspect engine total_changes to prove preflight executes no writes, then verify valid retry and outer rollback.

Final scoped checks passed formatting, Clippy, 280 Rust tests, thirty-two Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Isolated transfer measurements (2026-09-07)

The maintainer bench-transfer.cjs harness now measures JSON/NDJSON import/export in separate Linux processes, records source/addon/harness identities, and validates content aggregates, index integrity and exact round trips. The stored 1,000-document/4,096-byte-text report passed correctness with about 4.3 MB payloads. Single-sample debug timings and similar RSS values do not establish comparative performance or memory guarantees; see benchmarks.md for limits.


## Incremental JSON import preflight and replay (2026-09-07)

JSON import now decodes one document at a time during validation and replays the immutable input inside the atomic insert scope. It no longer retains the full portable/document vectors. Envelope field order remains unrestricted; duplicate/unknown/missing fields, invalid versions, trailing input and late invalid entries are rejected before writes. The document-count limit is enforced during sequence decoding. Replay preserves database error codes and rolls back imported rows/indexes while retaining prior outer transaction work. Parsing twice trades CPU for lower retained document memory; this does not establish a total-memory or parsing-latency bound.

Scoped checks passed 282 Rust tests, thirty-two Node tests and strict TypeScript; the added JSON constraint/retry integration regression then passed in the four-test transfer suite, bringing distinct Rust coverage to 283. One trigger-interruption gate remains ignored. Full V1 remains incomplete.


## Repeated JSON replay transfer diagnostic (2026-09-07)

Three runs of the unchanged transfer harness against clean implementation 8e7b5893d passed all six format samples. JSON import ranged from 3.64 to 4.03 seconds with 131.9–133.3 MB current RSS after import. These overlap or remain close to the earlier single materializing-path sample, so no reliable speed or memory improvement is claimed. The retained-document-array removal is an implementation property, not a measured total-memory guarantee. Detailed reports and limitations are in benchmarks.md.


## Forward-fetch encoded-value budgets (2026-09-07)

Each forward resolver invocation now bounds retained fetched values and duplicate-expanded output separately to 64 MiB of tagged Value JSON bytes. Serialization counts into a writer without allocating encoded copies; output accounting completes before cloning documents. Nulls and repeated references count per output position. Exact-boundary tests cover collection/native targets, Unicode, duplicates, empty/null output, FDB_LIMIT, retained active work, successful retry and rollback. Reference keys, container overhead, current engine chunks and outer-query materialization are not included, so this is not a total-memory cap. All fetch projections in one lowered SELECT share a resolver invocation and its budgets.

Scoped checks passed formatting, Clippy, 284 Rust tests, thirty-two Node tests and strict TypeScript. One trigger-interruption gate remains ignored; full V1 remains incomplete.


## Incremental forward-fetch target reads (2026-09-07)

Collection and native target batches now use engine row callbacks instead of collecting every target row before decoding and charging the byte budget. A budget/decoding failure stops iteration and preserves its frontend error through statement drop and atomic cleanup. The prior full-chunk memory caveat is reduced to the current row, although engine allocations, reference/container overhead and outer-query materialization still prevent a total-memory guarantee. Existing tests exercise exact budget failures/retries in active transactions and the multi-batch order/duplicate/snapshot contracts.

Scoped checks passed formatting, Clippy, 284 Rust tests, thirty-two Node tests and strict TypeScript. One trigger-interruption gate remains ignored; full V1 remains incomplete.


## Fetch target evaluation-count regression (2026-09-07)

A test-only scalar counts actual native expression evaluations through the target-row visitor. Budgets accepting zero, one or two rows evaluate exactly one, two or three rows respectively, stop at the first over-budget row with FDB_LIMIT, preserve active transaction work and permit a complete three-row retry. Outer rollback removes prior writes. This qualifies early visitor termination on the tested scalar table scan, not all planner/materialization behavior or cancellation latency. Both link unit tests passed; the prior full 284 Rust / 32 Node baseline remains applicable, with one additional distinct Rust regression.


## Shared SQL fetch budget qualification (2026-09-07)

Inspection of execute_lowered_profiled confirms all fetched cells are flattened into one resolver call. Earlier wording claiming separate per-projection budgets was incorrect and is corrected above and in contracts.md. A SQL regression uses 4,096 positions and an 8,192-byte target text: two projections exceed the shared 64 MiB byte limit while staying below the 16,384 reference limit; a one-projection retry succeeds. It checks retained active work and outer rollback. This covers the lowered SELECT fetch path, not total outer-result memory.


## Incremental lowered SELECT decoding (2026-09-07)

Lowered SELECT execution now decodes directly from engine row callbacks, removing the full intermediate engine-value rowset. It checks the combined fetch-reference count before retaining each decoded row. The evaluation-count regression now scans 24,576 native positions with a fetched projection, expects FDB_LIMIT after 16,385 scalar evaluations and verifies active state, a two-row retry and rollback. Complete decoded results are still retained; ordinary result-byte budgets, engine materialization and whole-query memory remain open.

Scoped checks passed formatting, Clippy, 286 Rust tests, thirty-two Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Forward-fetch profiling counters (2026-09-07)

profile_select now accepts supported forward-fetch SELECT projections. Lowering, the outer statement and target reads share an atomic snapshot scope. Existing primary-statement counters retain their scope; new fetch_batches, fetch_rows_read and fetch_vm_steps separately attribute target SELECT batches and engine rows/instructions, excluding metadata/savepoint helpers and decoding. Node exposes the additions as bigint fetchBatches/fetchRowsRead/fetchVmSteps; CLI serialization includes the Rust names. No partial counters are returned after failures. Tests cover 130 distinct keys in each of collection/native targets, duplicate projections sharing four target batches, repeat-call stability, zero target counters for ordinary/missing-target queries, prior snapshots, budget failure and both Node clients. Detailed target plans and complete helper/time/memory accounting remain open.

Verification passed formatting, scoped Clippy, 287 Rust tests, thirty-three Node tests and strict TypeScript. A CLI fetch profile smoke passed. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Installed Node fetch profiling smoke (2026-09-07)

The offline Node package consumer now checks installed sync/worker fetch profiles: deduplicated target counters, bigint transport, lossless fetched int64 values and active transaction observations. Installed TypeScript declarations expose all three target counters as bigint. The complete package smoke passed on Linux x64 / Node 24.19.0 with eight runtime files and a 59,564,850-byte development tarball. This does not establish release prebuild or additional-platform support.


## Fetch-profile interruption sweep (2026-09-07)

A real-engine regression measures total VM progress for a 130-target/two-batch fetch profile, then interrupts at five progress thresholds in both autocommit and an existing transaction. All ten cases return FDB_CANCELLED, retain transaction state and prior collection data, and permit exact result/counter retry; outer rollback remains effective. Thresholds span the measured workflow and are not labels for specific target/compiler/cleanup phases or a cancellation-latency guarantee. All three link unit tests passed; prior 287 Rust / 33 Node scoped evidence remains, plus one new distinct Rust regression.


## Forward-fetch benchmark evidence (2026-09-07)

The maintainer bench-fetch.cjs harness passed twelve 1,000-position workloads with warmup and three samples each. Collection/relational targets, one/130/1,000 distinct keys and one/two projections all passed result and counter assertions. Duplicate projections share target counters while increasing observed elapsed time; timings and qualifications are in benchmarks.md. This supplies initial target-work evidence, not release performance or memory guarantees.


## Initial leading-WITH collection UPDATE/DELETE (2026-09-07)

Collection writes now retain the leading CTE scope in pre-mutation candidate selection. Tests cover parameterized native/collection CTE membership, self-read candidates, RETURNING/affected counts, uniqueness failure, prior outer work, retry/rollback and both Node clients. The interrupted-mutation matrix includes WITH UPDATE/DELETE. Main qualification prevents an unqualified candidate target reference, but deeper same-name CTE resolution remains open: `WITH docs AS (SELECT $n AS n), chosen AS (SELECT n FROM docs) UPDATE docs SET n=n+10 WHERE n IN (SELECT n FROM chosen) RETURNING n` currently fails preparation with no such column: n. Assignment/RETURNING subqueries and other previously unsupported write clauses remain unqualified.

Scoped checks passed formatting, Clippy, 289 Rust tests, thirty-four Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Same-name write-context baseline (2026-09-07)

A pinned native regression demonstrates that same-name CTE resolution differs between UPDATE/DELETE and a candidate SELECT in the tested chained form: writes affect all three base rows, while SELECT matches only the CTE's value 2. Both with_writes tests passed, including rollback restoration. Collection lowering must preserve this write-context behavior; the existing same-name preparation failure remains open. This evidence rules out simply treating successful SELECT rewriting as sufficient correctness.


## Native CTE names in SELECT guarding (2026-09-07)

The native guard now redacts known nonrecursive CTE declarations and unqualified FROM references in its inspection-only AST. CTE-only SELECT/profile queries sharing a collection name pass in simple, chained and derived-source forms; original SQL is unchanged. Direct guard tests retain schema-qualified, internal/reserved, self/forward and nested physical references. Reserved CTE names are never redacted. Qualified expression aliases and deeper scope coverage remain conservative; the same-name UPDATE/DELETE gap is separate and still open.

Scoped checks passed formatting, Clippy, 292 Rust tests, thirty-four Node tests and strict TypeScript. The final reserved-name guard regression was rerun separately. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Qualified CTE references in native guarding (2026-09-07)

Qualified fields and stars from proven unaliased CTE FROM sources can now pass the native guard when the CTE shares a collection name. Tests cover projection, filter, ordering, grouping/HAVING and both Node clients. Direct guard regressions keep qualified physical references and nested expression scopes visible. This extends the previous declaration/source redaction without changing executed SQL. Explicit aliases, named windows and deeper scope qualification remain open; same-name collection writes remain a separate gap.

Scoped checks passed formatting, Clippy, 292 Rust tests, thirty-four Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Aliased CTE sources in native guarding (2026-09-07)

Proven CTE FROM sources now support explicit/implicit aliases in guard inspection, including an alias that matches a collection name. Expanded regressions cover qualified projections/stars, filtering/ordering and aliases of collection-named CTEs. Direct guard tests retain physical-table aliases, reserved/internal aliases and nested physical references. Same-name write resolution and deeper/named-window scope coverage remain open.

Scoped checks passed formatting, Clippy, 292 Rust tests, thirty-four Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Installed Node WITH-write smoke (2026-09-07)

The offline installed-package smoke now exercises CTE-driven UPDATE/DELETE in synchronous and worker clients, typed RETURNING, fetched values after update, empty index/data state after delete and rollback to the original int64 value. A collection-named CTE alias also executes through the installed client. The complete smoke passed on Linux x64 / Node 24.19.0 with eight runtime files and a 59,569,700-byte development tarball. Release/platform qualification remains open.


## Named-window CTE qualifiers (2026-09-07)

Named-window partition/order expressions now recognize proven CTE qualifiers in guard inspection. A multi-row sum window over a collection-named native CTE agrees with an equivalent ordinary CTE in execute and profile_select. Direct guard tests retain nested physical and schema-qualified window references. This removes a false rejection without changing executed SQL or extending pinned frame/window support. Frame expressions, deeper scopes and same-name writes remain open.

Scoped checks passed formatting, Clippy, 293 Rust tests, thirty-four Node tests and strict TypeScript. Final direct guard checks passed separately. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Aliased collection-named CTE writes (2026-09-07)

After the guard corrections, a collection-named CTE with a distinct write-target alias passes for UPDATE and DELETE. A new regression compares parameterized, qualified candidates and RETURNING to equivalent native queries, checks affected counts/index integrity and rolls back both routes. All five with_writes tests passed. The unaliased target-identifier collision still fails preparation; automatic alias insertion would change pinned semantics and is not the fix.


## UPDATE subquery assignment candidates (2026-09-07)

The assignment validator now delegates subquery nodes to candidate SELECT lowering while retaining surrounding scalar checks. Tests compare native/collection scalar sources, CTE scalar values and EXISTS/IN assignments; verify typed boolean propagation into field validation; and exercise missing parameters, uniqueness failure, prior outer work, corrected retry and rollback. The mutation-cancellation matrix includes scalar and WITH-scalar assignment forms. VALUES/RETURNING restrictions and unsupported correlation remain unchanged.

Scoped checks passed formatting, Clippy, 295 Rust tests, thirty-four Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Nested membership assignment validation (2026-09-07)

The candidate validator now preserves a parenthesized wrapper when inspecting the left operand of IN/NOT IN subqueries. This lets the AST walker visit a nested scalar-subquery root instead of leaving it rejected by the surrounding scalar validator. Differential assignment tests cover scalar-subquery operands, nested membership, NULL/empty sets and retained rejection of outer aggregate assignments. The executable expression remains unchanged.

Scoped checks passed formatting, Clippy, 296 Rust tests, thirty-four Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Installed subquery-assignment smoke (2026-09-07)

The installed Node consumer now uses a parameterized scalar UPDATE assignment and a nested scalar-IN assignment in both sync and worker clients. It checks RETURNING/affected counts, fetched values, managed index entries and rollback to int64 max. The complete offline smoke passed on Linux x64 / Node 24.19.0, with eight runtime files and a 59,579,238-byte development tarball. Release/platform qualification remains open.


## Self-read assignment atomicity (2026-09-07)

A new regression compares a multi-row self-read scalar UPDATE with native results, then forces a typed validation failure after an earlier candidate has written. The engine total_changes counter confirms a write occurred; data and managed indexes return to their pre-statement state while prior outer-transaction work survives. A corrected retry succeeds and outer rollback restores the original rows. All eight with_writes tests passed. Full V1 remains incomplete.


## Qualified native subquery predicates (2026-09-07)

Native scalar and EXISTS subqueries now support qualified outer collection fields in a simple SELECT's WHERE and JOIN ON predicates. The native inner query may use ordinary table sources or no FROM source. Local source aliases shadow outer aliases; unqualified inner names retain native resolution. Metadata preparation substitutes outer references only in a disposable probe. Executable predicates use the existing typed comparison lowering and remain inside the engine statement, so different outer rows receive different results. Scalar projections retain native affinity; the pinned engine's correlated scalar result does not propagate its projected collation into an outer comparison, while explicit outer COLLATE remains effective.

Covered consumers include projections, filters, profile_select and pre-mutation UPDATE candidates. Tests cover empty results, parameters, alias shadowing, native JOIN predicates, binary keys, a scalar affinity/collation matrix, validation rollback and both Node clients. This is initial predicate correlation support: inner collection sources, unqualified outer references, correlated IN, nested/compound/CTE-local scopes, and correlation in projection/group/window/order/limit expressions remain open. General correlation and volatile-expression/resource qualification are not complete.

Scoped checks passed formatting, Clippy, 301 Rust tests, thirty-five Node tests and strict TypeScript. Final explicit-collation wrapper cases passed separately. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Installed predicate-correlation smoke (2026-09-07)

The offline installed Node consumer now exercises bound correlated UPDATE, profiled scalar reads, empty-result NULL and EXISTS DELETE through sync and worker clients. It checks affected rows, index cleanup and rollback to int64 max. The complete smoke passed on Linux x64 / Node 24.19.0, with eight runtime files and a 59,601,946-byte development tarball. Release/platform qualification remains open.


## Correlated scalar evaluation counts (2026-09-07)

The existing test-only native function counter now compares seven correlated scalar forms against ordinary tables through execute and profile_select. Two matching outer rows cause two calls, one matching row causes one, empty predicates cause zero, and two explicit scalar occurrences cause four. Arithmetic and comparisons retain the native counts. The targeted regression passed; this qualifies these forms without claiming complete volatile-expression behavior or closing general correlation.


## Correlated native HAVING predicates (2026-09-07)

Qualified outer collection fields now lower inside native scalar/EXISTS HAVING predicates using the same metadata/runtime separation as WHERE and JOIN ON. Differential coverage includes scalar aggregates, projection aliases in HAVING, grouped first-row selection, EXISTS, local alias shadowing, parameters, execute/profile results and atomic UPDATE failure/retry/rollback. Both Node clients exercise a grouped scalar read with a HAVING projection alias. Correlation in GROUP BY expressions, inner collection sources, correlated IN and deeper scopes remain open.

Scoped checks passed formatting, Clippy, 302 Rust tests, thirty-five Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Correlated native membership predicates (2026-09-07)

IN/NOT IN sources now use qualified outer collection fields in the supported native predicate scopes. An uncorrelated native RHS retains its enclosing shared materialized CTE; a correlated RHS stays in a local materialized CTE inside the membership expression. Native left operands retain native IN execution with the rewritten RHS. Logical left operands retain binary comparison keys and the existing affinity/collation conversions. Sources are evaluated by the engine for their outer rows, without frontend pre-execution.

Tests compare NULL/empty sources, native literal and typed left operands, unary plus/CAST/COLLATE, INTEGER/NOCASE/RTRIM RHS declarations and binary-versus-record identity. Missing parameters, failed INSERT SELECT with prior work, integrity and corrected retry are covered. The native function counter checks per-row source evaluation for IN and NOT IN through execute/profile. Inner collection sources, deeper correlated scopes, non-predicate correlation and broader volatile/planner/resource behavior remain unfinished.

Scoped checks passed formatting, Clippy, 305 Rust tests, thirty-five Node tests and strict TypeScript. Final text-literal collation cases passed separately. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Installed correlated membership smoke (2026-09-07)

The offline installed Node consumer now exercises a parameterized correlated IN UPDATE and profiles matching, empty NOT IN and NULL membership results through sync and worker clients. It verifies affected rows, managed index integrity and rollback to int64 max. The complete smoke passed on Linux x64 / Node 24.19.0 with eight runtime files and a 59,608,392-byte development tarball. Release/platform qualification remains open.


## Nested outer fields in correlated predicates (2026-09-07)

The correlation detector now recognizes nested outer document paths, including the parser's deep-path helper, and substitutes the complete path only in metadata probes. Runtime lowering uses the existing typed field accessor and comparison rules. Scalar, EXISTS, IN/NOT IN, HAVING and JOIN predicate regressions compare shallow/deep nested paths with native scalar-column oracles, including NULL, missing paths and scalar parents. Derived collection sources retain nested typed access; local aliases continue to shadow the outer source. A correlated UPDATE checks affected rows, index integrity and rollback. Deeper query scopes and inner collection sources remain unfinished.

Qualification exposed a derived-accessor error for NULL/scalar parents. A separate nested-value accessor now returns missing fields for valid non-object parents while physical document access stays strict; a direct accessor regression checks malformed encodings and stored-root rejection.

Scoped checks passed formatting, Clippy, 307 Rust tests, thirty-five Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Source-free correlated deep paths (2026-09-07)

A deep-path parser helper alone no longer opts a source-free expression subquery into standalone logical lowering before its outer field scope exists. It remains available to the enclosing correlation pass. Regression coverage extends nested paths to source-free scalar, EXISTS and IN forms through direct and derived outer collections. Other explicit logical expressions and typed parameters retain their existing routing; broader mixed-expression correlation remains open.


## Nested savepoint cancellation and installed deep paths (2026-09-07)

The Node cancellation suite exposed partial import data surviving FDB_CANCELLED. A deterministic 48-boundary nested atomic sweep reproduced the failure at boundary 4. Atomic operations now have unique per-connection savepoint names, so outer cleanup cannot accidentally target an interrupted inner frame. See [atomic-savepoints.md](atomic-savepoints.md) for evidence and remaining savepoint/commit boundaries.

The complete scoped suite passed formatting, Clippy, 308 Rust tests, thirty-five Node tests and strict TypeScript. The Node cancelled-import test passed three additional isolated runs. The installed-package smoke passed direct/derived nested values and source-free correlated profiles across object, NULL, scalar, array and missing parents in both clients, including index integrity and rollback. Result: Linux x64 / Node 24.19.0, eight runtime files, 59,642,181 packed bytes. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## Cleanup after cancelled atomic opening (2026-09-07)

Cancellation after a SAVEPOINT opened could leave an unintended active transaction even though the operation callback never ran. Atomic opening errors now clean up their unique frame, accepting only the pinned engine's exact missing-frame error when it never opened. Other cleanup failures remain FDB_ROLLBACK. A boundary sweep checks autocommit and active outer transactions, absence of orphan savepoints, prior rows and successful retry. See atomic-savepoints.md for remaining I/O and RELEASE/commit qualification.

Scoped checks passed formatting, Clippy, 309 Rust tests, thirty-five Node tests and strict TypeScript. One known trigger-interruption gate remains ignored; full V1 remains incomplete.


## RELEASE cancellation disposition (2026-09-07)

A new 32-case boundary sweep distinguishes actual interrupt delivery from completion before delivery. Six delivered interrupts restore rows with FDB_CANCELLED; one outer-transaction RELEASE boundary returns FDB_ROLLBACK with the complete write set pending, which explicit outer rollback removes. Other thresholds finish before delivery. Transaction modes remain consistent and no partial set is accepted. See atomic-savepoints.md. This is test qualification without a production-code change; interrupted I/O and broader commit outcomes remain open.


## Installed import cancellation and caller savepoints (2026-09-07)

The offline installed Node smoke now requests cancellation during a 1,000-document import inside an active transaction and caller savepoint. It verifies FDB_CANCELLED/active state, prior rows and integrity, listener cleanup, a complete fresh retry and rollback to the caller savepoint. The complete smoke passed on Linux x64 / Node 24.19.0 with eight runtime files and a 59,653,802-byte development tarball. Timing determines the delivery point; Rust boundary sweeps provide deterministic interruption evidence. Release/platform qualification remains open.


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


Migration source-location qualification (2026-09-07): a regression verifies FDB_MIGRATION carries the pending version, exact UTF-8 byte offset after multibyte comments and the underlying FDB_VALIDATION cause. Earlier pending DDL rolls back, later statements remain absent, and correcting the failed source applies only the pending nonconsecutive version before exact rerun skips it. All seven migration integration tests, formatting and focused Clippy pass. Full V1 remains incomplete.


Migration preflight syntax context (2026-09-07): script splitting/tokenization errors now include the migration version while retaining FDB_SYNTAX and script-relative UTF-8 byte offsets. A regression checks an unterminated quote after multibyte comments, no earlier migration mutation and a successful valid retry. All eight migration integration tests, formatting and frontend/tests all-target Clippy pass. Full V1 remains incomplete.


## Combined migration resource and diagnostic verification (2026-09-07)

At clean implementation commit fe4856c19, fastdb/scripts/check.sh passed formatting, Clippy, 344 Rust tests, 42 Node/application tests and strict TypeScript. One known trigger-interruption gate remains ignored. This combines bounded ledger rows/text, maximum-size persistent history, execution/preflight UTF-8 diagnostic tests and the rebuilt Node addon/client paths. Installed-package evidence remains the separately recorded earlier run. Broader platform, recovery, SQL/type/resource and application release gates remain open; full V1 is incomplete.


Node migration diagnostic qualification (2026-09-07): a both-client regression verifies preflight FDB_SYNTAX includes the exact migration version above JavaScript's safe-integer range and the UTF-8 byte offset after multibyte comments, with autocommit observations and no earlier schema mutation. Corrected plans apply, name/source/version mismatches retain FDB_VALIDATION reasons and transaction state, and the unchanged plan remains reusable. The focused synchronous/worker test passes using the addon rebuilt by the preceding full scoped check. Broader V1 release qualification remains open.


Filtered DISTINCT aggregate qualification (2026-09-07): native differential collection tests cover count/sum/avg DISTINCT with FILTER, numeric equivalents, NULLs, empty filtered groups and HAVING aliases through execute/profile_select. A grouped INSERT SELECT validation failure preserves prior outer work and index integrity; a corrected filter succeeds and explicit rollback restores empty committed contents. All eight grouping integration tests, formatting and focused Clippy pass. This qualifies existing scalar aggregate behavior; broader SQL/type/resource and full V1 gates remain open.


Typed aggregate FILTER qualification (2026-09-07): record-literal and record-parameter predicates select the expected aggregate values through execute/profile_select. COUNT and COUNT DISTINCT of record::id skip invalid argument values on excluded rows; including an invalid value rejects, and a valid query can run afterward. All nine grouping integration tests, formatting and focused Clippy pass. A separate local probe found COUNT(array::append(tags,1)) rejects even for an included valid array because aggregate lowering still requires scalar/record index values. Composite aggregate arguments remain an explicit SQL/type gap; this qualification does not close it or full V1.


## COUNT of composite document values (2026-09-07)

Non-DISTINCT COUNT arguments now use the existing nullable typed-value conversion rather than scalar unwrapping. Arrays, objects and composite helper results count as non-null values; null and missing values remain excluded. FILTER still skips argument evaluation on excluded rows, while included invalid helper inputs reject. COUNT DISTINCT retains its existing scalar comparison path; composite DISTINCT equality remains open.

The new regression covers arrays/objects/booleans, null/missing values, helper/coalesce results, profiling, excluded invalid arguments and successful reuse after errors. The complete scoped check passed formatting, Clippy, 347 Rust tests, 43 Node/application tests and strict TypeScript; one known trigger-interruption gate remains ignored. No upstream core files changed. Broader aggregate/type/resource and full V1 qualification remain open.


Composite COUNT window/write qualification (2026-09-07): arrays, objects, records and nulls produce matching cumulative, partitioned and whole-window counts against native non-null/blob fixtures through execute/profile_select. Grouped INSERT SELECT validation rejects atomically with prior work and index integrity preserved; a HAVING-filtered retry succeeds and explicit rollback removes pending results. All eleven grouping integration tests, formatting and focused Clippy pass. The native oracle compares null presence only, not composite ordering/equality. COUNT DISTINCT composites and broader V1 gates remain open.


Node composite COUNT qualification (2026-09-07): both clients count stored array/object/record/binary/boolean values while excluding null/missing fields, preserve bigint result counts through exactlyOne/profileSelect, and count typed parameters including empty binary values correctly. Cumulative window counts retain the same null-presence semantics. The focused synchronous/worker regression passes with the previously rebuilt addon. Composite DISTINCT equality and broader V1 qualification remain open.


Installed composite COUNT qualification (2026-09-07): the offline package consumer now checks plain composite/null/missing counts, typed object parameters, FILTER helper evaluation, cumulative windows and profile results through synchronous and worker clients inside rollback-scoped fixtures. Runtime and declaration checks passed on Linux x64 / Node 24.19.0: eight files, 59,805,933 packed bytes. The addon is the build from the preceding combined COUNT implementation check. No publishing occurred; broader platform, composite DISTINCT and full V1 gates remain open.


COUNT contract clarification (2026-09-07): contracts.md now records non-DISTINCT COUNT null-presence behavior for composite values, parameters, filters, windows and grouped writes. Its existing Scalar DISTINCT section explicitly leaves generic array/object/vector equality unsupported in V1. Recent notes calling composite COUNT DISTINCT “open” describe an unsupported form, not an added standalone release requirement; no equality semantics or V1 scope have been changed. The remaining SQL/type/resource and release gates still apply. This documentation change was checked against the implementation and the preceding COUNT tests.


COUNT scalar compatibility qualification (2026-09-07): a native differential matrix checks COUNT, COUNT ALL and COUNT DISTINCT over mixed null/numeric/boolean/text/binary inputs, explicit text/blob casts, CASE, NULLIF, NOCASE and literals. Empty and internal-prefix-looking blobs remain ordinary binary inputs. Execute/profile_select match native counts across the matrix. All twelve grouping tests, formatting and focused Clippy pass. This supplements the composite COUNT change without defining composite DISTINCT equality; broader V1 gates remain open.


## Combined COUNT qualification (2026-09-07)

At clean commit ae564d1ff, fastdb/scripts/check.sh passed formatting, Clippy, 349 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-interruption gate remains ignored. This combines plain composite COUNT with scalar/native compatibility, FILTER, window/grouped-write and both-client coverage. The addon was rebuilt before client tests. Installed-package evidence remains the separately recorded COUNT smoke. Generic composite DISTINCT remains unsupported under the existing contract; remaining SQL/type/resource, recovery, distribution and application gates still prevent full V1 completion.


Correlated composite COUNT qualification (2026-09-07): native null-presence comparisons cover outer composite fields in COUNT, CASE and coalesce with full, correlated-filtered and empty native inner sources through execute/profile_select. A multirow collection UPDATE consumes these counts, retains managed-index integrity and restores original records on rollback. The focused regression, formatting and focused Clippy pass. The native fixture uses blobs only to represent non-null presence; it does not establish composite equality. Broader correlation/type/resource and full V1 gates remain open.


## COUNT presence marker (2026-09-07)

Non-DISTINCT COUNT lowering now uses a private count-value helper that fully decodes/validates its typed argument and returns SQL NULL or integer 1. This removes the prior nullable helper's re-encoding of non-null composites into result blobs. Input decoding and document reads remain; no end-to-end latency or total-memory improvement is claimed without measurement. DISTINCT comparisons retain their existing path.

The complete scoped check passed formatting, Clippy, 350 Rust tests, 44 Node/application tests and strict TypeScript, including scalar/composite/filter/window/correlated COUNT and write qualification. One known trigger-interruption gate remains ignored. No upstream files changed; broader V1 work remains open.


COUNT marker validation qualification (2026-09-07): a private-helper regression verifies valid null returns SQL NULL and false/large composite values return only integer 1. Malformed/truncated encodings, an invalid typed-vector payload and unencoded text reject; a subsequent valid null call succeeds. The focused unit test, formatting and frontend all-target Clippy pass. This confirms full decoding/validation remains in the optimized helper; total resource/performance and broader V1 gates remain open.


CLI migration/transfer output errors (2026-09-07): these modes now use fallible writes and explicit stdout flushing instead of panic-on-error print macros. A Linux /dev/full regression confirms migration reporting failure returns an error without panic and reopening retains the committed applied history. Timeout-bounded export/import probes confirm clean export rejection and committed import data despite report failure. All CLI tests, formatting and CLI all-target Clippy pass. Output errors do not imply write rollback; broader platform/recovery and full V1 gates remain open.


CLI transfer reporting-failure regression (2026-09-07): Linux /dev/full coverage now runs in the CLI integration suite for both JSON and NDJSON. Export/import output failures return nonzero without panic; re-export after reopening proves imports committed despite report failure, duplicate retries reject and source contents remain unchanged. Both transfer integration tests, formatting and focused Clippy pass. Broader platform/output/recovery qualification and full V1 remain open.


CLI help output handling (2026-09-07): --help and -h now use fallible stdout writes and explicit flush, removing the last print macro from CLI source. A rebuilt CLI passes normal-output and Linux /dev/full probes for both aliases, returning an error rather than panicking when output fails. Formatting and CLI all-target Clippy pass. Broader platform and V1 qualification remain open.


## Initial inner-collection correlation (2026-09-07)

Qualified outer collection fields now lower before a simple inner collection expression subquery, retaining encoded values across the recursive lowering boundary. This fixes silent record-identity non-matches in EXISTS and scalar COUNT, and missing outer-field bindings in IN predicates. Source lookup is lazy; local aliases shadow outer aliases. Regression coverage includes execute/profile, scalar outer record/array/object/nested projections, numeric predicates, multirow UPDATE, managed-index integrity and rollback. A former unsupported-query assertion now verifies matching correlated scalar results.

The complete scoped check passed formatting, Clippy, 355 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. The rebuilt synchronous and worker Node clients also pass the original EXISTS/COUNT/IN probes. The pass is limited to direct table sources without local WITH or compounds; derived outer sources, deeper scopes and broader correlation/type/resource qualification remain open. Full V1 is not complete. No upstream files changed.


## Derived outer fields in collection correlation (2026-09-07)

Inner collection expression subqueries now retain typed fields from aliased derived SELECTs and enclosing CTEs. A compiler-only marker carries encoded-value metadata across recursive lowering and is removed before execution. Coverage includes record and nested predicates, scalar record/array/object projections, COUNT, EXISTS, IN, ordering, execute/profile and binary parameters. A failed INSERT SELECT preserves prior work, an integer retry succeeds and rollback removes both writes. Unaliased native derived queries retain native routing; their existing compound-pagination regression passes.

The complete scoped check passed formatting, Clippy, 357 Rust tests, 44 Node/application tests and strict TypeScript, with the known trigger-cancellation gate still ignored. No upstream files changed. Local inner WITH, compounds, deeper correlation scopes and broader V1 release gates remain open.


## HAVING with unprojected document keys (2026-09-07)

Fixed silently lost groups when a collection HAVING predicate references a document group key omitted from the projection. HAVING uses the equivalent typed-accessor/unwrap form, avoiding the pinned engine's unavailable expression-key result. Native SQL and upstream files remain unchanged. Regressions cover record keys, NULL/numeric/text/NOCASE keys against projected native-key references, execute/profile, direct and derived outer correlation, EXISTS, indexed UPDATE and rollback.

The complete scoped check passed formatting, Clippy, 360 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. The workaround specifically addresses document scalar accessors; broader grouping and volatile-expression qualification remain open. An additional aggregate ORDER BY expression probe fails with a missing-column error while its output ordinal works; see contracts.md for the reproducer. Full V1 remains incomplete.


## Aggregate ORDER BY projection matching (2026-09-07)

Fixed the documented missing-column failure for ORDER BY sum(n) over a collection. Native-valued expression matches now reuse their translated projection even without DISTINCT; logical field arguments no longer remain in the engine AST. Differential execute/profile tests cover SUM/AVG/MIN/MAX/COUNT, expressions, parentheses, aliases, ordinals and descending order. The original unaliased count/sum/HAVING reproducer also passes through the rebuilt Node addon.

The complete scoped check passed formatting, Clippy, 361 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. No upstream files changed. Broader grouping, correlation, type/resource and full V1 release qualification remain open.


Ordered grouped INSERT SELECT qualification (2026-09-07): a regression verifies SUM-expression sorting and a CHECK failure in the final sorted group for both typed collection and ordinary SQL targets. Prior transaction work survives the rejected statement; a filtered retry inserts the two valid groups, managed collection indexes remain consistent and rollback removes all transaction writes. The focused regression, formatting and focused Clippy pass. This is additional focused evidence after the recorded 361-Rust/44-Node combined check, not a new combined run. Broader write/grouping and V1 release gates remain open.


## Function-valued HAVING group keys (2026-09-07)

Extended the nonprojected document group-key workaround to native scalar function arguments. Private helper aliases use the existing document-scalar and SQL-scalar implementations, preserving binary payload behavior and replacing the earlier direct-key decode/unwrap sequence. Regressions compare lower/hex/length/typeof over text, binary and NULL through direct and derived sources against projected native-key references. A plan assertion confirms projected aggregates, including those nested in output arithmetic, keep one aggregate step. Group keys nested inside a different projected aggregate are also covered.

The final complete scoped check passed formatting, Clippy, 364 Rust tests, 44 Node/application tests and strict TypeScript; one known trigger-cancellation gate remains ignored. No upstream files changed. Broader grouping/volatile-expression, correlation, resource and full V1 release qualification remain open.


Projected aggregate evaluation qualification (2026-09-07): a test-only nondeterministic scalar callback counts actual engine evaluations. Direct and arithmetic aggregate projections, HAVING aliases/repeated aggregate expressions and alias/expression ordering match native results with exactly one callback per input row through execute and profile_select. This strengthens the previous aggregate-step plan assertion without claiming general volatile-expression qualification. The focused unit regression, formatting and frontend lib/test Clippy pass. Latest full scoped evidence remains the separately recorded 364-Rust/44-Node check; full V1 gates remain open.


Distinct grouped pagination qualification (2026-09-07): differential execute/profile coverage combines SUM/arithmetic/COUNT, DISTINCT, ascending/descending aggregate ordering and page offsets through empty pages, including NULL and integer/real equivalents. Numeric equivalents are compared logically because the native plan may retain a different integer/real representative; no exact representative-type guarantee is added. A grouped DISTINCT INSERT SELECT CHECK failure preserves prior work, a corrected retry succeeds, index integrity passes and rollback removes all writes. The focused regression, formatting and focused Clippy pass. This extends focused evidence after the recorded 364-Rust/44-Node combined run; full V1 remains incomplete.


## Case-insensitive projected aggregate reuse (2026-09-07)

Fixed duplicate aggregation when a projected SUM and HAVING sum differ only in the aggregate name's case. Reuse keys normalize only the aggregate name, preserving its argument AST and the pinned engine's argument-equivalence behavior. The callback-count regression now covers mixed aggregate spelling with aliases, repeated expressions, ordering and profiling, retaining one callback per input row like the native reference. A probe changing nested callback spelling showed that the pinned native engine itself can use separate aggregates there; this change does not broaden that equivalence.

The complete scoped check passed formatting, Clippy, 366 Rust tests, 44 Node/application tests and strict TypeScript, with one known trigger-cancellation gate ignored. No upstream files changed. Broader grouping, correlation, resource and full V1 release gates remain open.


Filtered aggregate evaluation qualification (2026-09-07): the runtime callback counter now covers bound FILTER predicates with partially included and all-excluded inputs. HAVING aliases/repeated filtered aggregates and aggregate ordering match native results and callback counts through execute/profile; excluded inputs do not evaluate the aggregate argument and reuse adds no calls. The expanded focused unit test, formatting and frontend lib/test Clippy pass. Latest full scoped evidence remains the recorded 366-Rust/44-Node run; broader volatile/grouping and full V1 gates remain open.


## Inner derived collection correlation (2026-09-07)

Aliased inner derived sources now participate in outer-field correlation resolution, fixing silent record non-matches in EXISTS and scalar COUNT. Regressions cover direct/derived outer sources, derived inner filters and limits, COUNT, EXISTS, IN, typed array projections and execute/profile. Local alias handling uses the existing source metadata. Local inner WITH, compounds and deeper scope qualification remain open.

The complete scoped check passed formatting, Clippy, 367 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. No upstream files changed; full V1 remains incomplete.


Inner-derived correlated write qualification (2026-09-07): a multirow UPDATE uses an inner derived source for scalar assignments and another for correlated EXISTS filtering. A CHECK failure preserves prior transaction work and managed-index integrity; correcting the source permits retry, and rollback restores both source and target collections. The focused regression, formatting and focused Clippy pass. Latest complete scoped evidence remains the recorded 367-Rust/44-Node run; broader correlation/write and full V1 gates remain open.


Pinned outer GROUP BY reference qualification (2026-09-07): native SQL and collection probes both reject inner grouping keys referencing an outer row, including direct/derived outer sources. The focused regression verifies FDB_ENGINE through execute/profile/UPDATE, preservation of prior transaction work, successful predicate-correlated reads afterward, index integrity and rollback. Formatting and focused Clippy pass. contracts.md records this pinned native limitation; it does not establish a new collection-only implementation requirement. Latest combined evidence remains 367 Rust/44 Node tests; full V1 remains incomplete.


## Local collection CTE consumer correlation (2026-09-07)

Fixed false record non-matches when a nonrecursive local WITH exposes collection fields to a correlated consuming SELECT. A lowering-only probe determines whether local CTE output is logical, including when a CTE shadows a collection name, before rewriting qualified outer typed fields. Tests cover single/chained/shadowing CTEs, direct/derived outer sources, COUNT, EXISTS, IN and execute/profile. This adds planning work without frontend execution of the inner SQL query. Correlation within CTE definitions, recursive CTEs, compounds and broader scope/resource qualification remain open.

The final complete scoped check passed formatting, Clippy, 370 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. No upstream files changed; full V1 remains incomplete.


Local CTE correlated write qualification (2026-09-08): a parameterized multirow UPDATE uses local CTE consumers in scalar assignments and EXISTS filters. An invalid bound value rejects the write while preserving prior transaction work and managed indexes; a corrected binding permits retry and rollback restores original rows. The focused regression, formatting and focused Clippy pass. Latest full scoped evidence remains the recorded 370-Rust/44-Node run; broader CTE/correlation/resource and full V1 gates remain open.


Local CTE correlation evaluation qualification (2026-09-08): a test-only nondeterministic callback checks a materialized local CTE consumed by a correlated scalar aggregate. LIMIT 0 produces no calls through execute/profile, and active collection queries match native results and evaluation counts. This gives runtime evidence that the lowering probe does not add source evaluation for the covered shape. The focused unit test, formatting and frontend lib/test Clippy pass. Latest full scoped evidence remains the recorded 370-Rust/44-Node run; broader CTE/volatile/resource and full V1 gates remain open.


Local CTE materialization-mode evaluation qualification (2026-09-08): the correlated callback regression now covers MATERIALIZED, NOT MATERIALIZED and default CTE planning. Each mode matches native results and actual evaluation counts through execute/profile, including zero calls with LIMIT 0. The expanded focused test, formatting and frontend lib/test Clippy pass. This does not freeze optimizer behavior across engine upgrades or establish general volatile-expression qualification. Latest combined evidence remains the recorded 370-Rust/44-Node run; full V1 gates remain open.


## Collection-backed CTE definition correlation (2026-09-08)

Factored collection correlation into a helper that visits nonrecursive CTE definitions before their consuming SELECT. Collection-backed definitions now bind qualified outer record and numeric fields; single/chained definitions and direct/derived outer sources pass execute/profile regressions. The helper skips work when no outer source is logical. This extends consumer-only CTE support; recursive CTEs, native-only definitions, compounds, deeper shadowing and total planning/resource qualification remain open.

The complete scoped check passed formatting, Clippy, 373 Rust tests, 44 Node/application tests and strict TypeScript, including existing callback-count and CTE write regressions. One known trigger-cancellation gate remains ignored. No upstream files changed; full V1 remains incomplete.


## Correlated CTE EXISTS preparation fix (2026-09-08)

Fixed a pinned-engine cursor-lookup panic exposed by an UPDATE combining correlated CTE definitions in a scalar assignment and EXISTS filter. Logical EXISTS with a local WITH now remains native EXISTS inside a scalar SELECT wrapper, providing the required outer-cursor preparation order. Regressions verify local aliases, failed validation with prior work, corrected-source retry, managed-index integrity and rollback of source/target collections. Callback counts cover EXISTS/NOT EXISTS first-match evaluation and ignored output projections through execute/profile.

The complete scoped check passed formatting, Clippy, 374 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. No upstream files changed; broader planner/scope/resource and full V1 release qualification remain open.


Correlated CTE EXISTS pagination qualification (2026-09-08): execute/profile comparisons cover EXISTS/NOT EXISTS with ordinary and aggregate projections, LIMIT 0/1, offsets and empty inner inputs. The native reference uses a scalar SELECT wrapper to avoid its direct-EXISTS preparation defect; the collection query uses the supported direct form. The focused regression, formatting and focused Clippy pass. An additional user-written scalar nesting level still reaches the documented deeper-correlation gap; this qualification does not claim that scope. Latest full evidence remains 374 Rust/44 Node tests; full V1 gates remain open.


## Source-free scalar wrapper correlation (2026-09-08)

Source-free scalar SELECT wrappers without local WITH or compound arms now pass the enclosing logical scope into projected subqueries. Regressions cover one/two wrapper levels, correlated COUNT and CTE-backed EXISTS, direct/derived outer sources and execute/profile. This fixes the extra-wrapper failure recorded during CTE EXISTS pagination qualification. Wrappers with table sources and broader scope/resource combinations remain open.

The complete scoped check passed formatting, Clippy, 376 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. No upstream files changed; full V1 remains incomplete.


Scalar-wrapper correlated write qualification (2026-09-08): a parameterized multirow UPDATE uses a source-free scalar wrapper around its assignment subquery and another around a correlated CTE EXISTS filter. Invalid bindings preserve prior work and managed-index integrity; valid bindings allow retry and rollback restores original rows. The focused regression, formatting and focused Clippy pass. Latest combined evidence remains 376 Rust/44 Node tests; broader correlation/write/resource and full V1 gates remain open.


## Source-free scalar WHERE correlation (2026-09-08)

Extended source-free scalar wrapper scope propagation to WHERE subqueries, fixing false NULL results for matching outer record references. Logical EXISTS consistently uses a scalar SELECT wrapper to avoid the pinned source-free semi-join preparation panic while retaining native EXISTS behavior. Execute/profile regressions cover direct/derived outer sources, correlated counts and direct/CTE-backed EXISTS filters.

The complete scoped check passed formatting, Clippy, 378 Rust tests, 44 Node/application tests and strict TypeScript, including existing callback and write-rollback regressions. One known trigger-cancellation gate remains ignored. No upstream files changed; broader scope/resource and full V1 release qualification remain open.


## Logical source-free projection binding (2026-09-08)

Source-free scalar queries selected for logical lowering now bind qualified outer document fields in projections and filters. The regression combines array::append with correlated EXISTS: excluded invalid values are not evaluated, admitted invalid values reject, and correcting the filter permits retry. Ordinary native scalar routing remains unchanged.

The complete scoped check passed formatting, Clippy, 379 Rust tests, 44 Node/application tests and strict TypeScript. One known trigger-cancellation gate remains ignored. No upstream files changed; broader expression/scope/resource and full V1 release qualification remain open.


Derived-source logical scalar qualification (2026-09-08): expanded the filtered array projection regression to an outer derived SELECT carrying id, n and v. Execute/profile preserve the encoded array, skip excluded invalid values, reject admitted invalid values, and permit execution retry after correcting the filter. The expanded regression, package formatting and focused Clippy pass. Latest combined evidence remains 379 Rust/44 Node tests; broader expression/scope/resource and V1 release gates remain open.


Typed-parameter scalar correlation qualification (2026-09-08): a new regression covers source-free scalar queries routed by boolean and record parameters, with direct and derived outer document sources. Boolean filters include/exclude typed record projections; changing a record parameter selects each corresponding outer record. Execute and profile agree, including NULL for excluded scalar rows. The focused regression, package formatting and focused Clippy pass. Latest complete scoped evidence remains 379 Rust/44 Node tests; this additional test has not received a new combined run. Broader scope/resource and full V1 release gates remain open.


## Source-free membership operand correlation (2026-09-08)

Fixed false NULL results when a logical scalar filter uses a qualified outer record as the left operand of IN/NOT IN. The correlation walker now visits that operand in its enclosing scope before handling the SELECT source. Regressions cover direct/derived outer sources, direct/coalesce operands, and NULL members through execute/profile.

The complete scoped check passed formatting, Clippy, 381 Rust tests, 44 Node/application tests and strict TypeScript. This includes the preceding typed-parameter regression. One known trigger-cancellation gate remains ignored. No upstream files changed; broader scope/resource and full V1 release qualification remain open.


Scalar membership write qualification (2026-09-08): a parameterized multirow UPDATE now has a regression for a source-free assignment with an outer record IN a collection SELECT and a typed boolean filter. A failing CHECK preserves prior transaction work and managed-index integrity; corrected bindings update both intended rows, and rollback restores the originals. The focused regression, package formatting and focused Clippy pass. Latest complete scoped evidence remains 381 Rust/44 Node tests; this additional test has not received a new combined run. Broader write/scope/resource and full V1 release gates remain open.


## Collection-reading scalar membership correlation (2026-09-08)

Fixed false NULL results for matching outer records on the left of IN/NOT IN in collection-reading scalar queries. The correlation walker binds the left operand in its enclosing scope while preserving the right-hand SELECT boundary and local alias shadowing. Execute/profile regressions cover direct/derived outer sources, direct/coalesce operands, positive/negative membership and a locally shadowed alias.

The complete scoped check passed formatting, Clippy, 383 Rust tests, 44 Node/application tests and strict TypeScript, including the preceding membership write-atomicity regression. One known trigger-cancellation gate remains ignored. No upstream files changed; broader query scopes, resource qualification and full V1 release gates remain open.


Collection scalar membership NULL qualification (2026-09-08): a native differential regression covers IN/NOT IN with nullable outer operands, nonempty RHS inputs, RHS NULL members and empty RHS inputs. Direct/derived collection sources agree with pinned native scalar-query results through execute/profile. The focused regression, package formatting and focused Clippy pass. Latest complete scoped evidence remains 383 Rust/44 Node tests; this additional test has not received a new combined run. Broader scope/resource and full V1 release gates remain open.


Nested membership operand qualification and remaining gap (2026-09-08): the source-free membership regression now includes `(SELECT d.id WHERE record::id(d.id) IS NOT NULL)` as its left operand. Direct/derived outer sources, IN/NOT IN and RHS NULL members pass execute/profile, formatting and focused Clippy. Latest combined evidence remains 383 Rust/44 Node tests.

A separate probe exposed an unresolved native-routing gap: with docs rows `{id:docs:a,n:1,v:[]}` / `{id:docs:b,n:2,v:[]}` and links `{owner:docs:a}`, `SELECT n,(SELECT array::append(d.v,2) WHERE (SELECT d.id WHERE true) IN (SELECT owner FROM links)) FROM docs d ORDER BY n` fails with `FDB_ENGINE: no such table: d`. The nested SELECT lacks an explicit logical helper/typed parameter, unlike the passing regression. This remains required implementation work; the passing typed form does not close native scalar routing or general scope qualification. Full V1 release gates remain open.


## Inherited logical scalar correlation (2026-09-08)

Fixed the preceding nested membership reproducer: source-free child SELECTs now inherit logical binding context from an already logical source-free parent. `(SELECT d.id WHERE true)` binds its qualified outer record without requiring a helper in the child. The expanded membership regression covers direct/derived sources, positive/negative membership and RHS NULL members through execute/profile. Native entry routing and table-bearing scope rules remain unchanged.

The complete scoped check passed formatting, Clippy, 384 Rust tests, 44 Node/application tests and strict TypeScript, including native scalar affinity, callback and write regressions. One known trigger-cancellation gate remains ignored. No upstream files changed; broader scope/resource and full V1 release qualification remain open.


Inherited scalar value qualification (2026-09-08): a new regression passes stored values through a native-shaped source-free child SELECT into an array helper in its logical parent. All ten public Value variants are represented, including binary bytes beginning with FDB and a float32 vector; direct/derived outer sources preserve exact values through execute/profile. The fixture uses array::new(), preserving ordinary SQL bracket-quoting rules. The focused regression, package formatting and focused Clippy pass. Latest complete scoped evidence remains 384 Rust/44 Node tests; this additional test has not received a new combined run. Broader scope/resource and full V1 release gates remain open.


## Source-free scalar ordering correlation (2026-09-08)

Extended source-free scalar correlation to ORDER BY expressions, fixing an unresolved outer numeric field in a logical array projection. A pinned native comparison establishes support for the scalar ordering form; direct/derived collection regressions pass execute/profile.

The complete scoped check passed formatting, Clippy, 386 Rust tests, 44 Node/application tests and strict TypeScript, including the preceding inherited-value regression. One known trigger-cancellation gate remains ignored. No upstream files changed; broader scope/resource and full V1 release qualification remain open.
