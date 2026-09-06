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
- Collection GROUP BY scalar expressions/ordinals and HAVING aggregate predicates/projected aliases, with native SQL grouping and typed output projections.
- AST-lowered collection SELECT: typed field/document projections, scalar expressions, WHERE, explicit joins (including mixed relational/document joins), ORDER BY, LIMIT/OFFSET, named typed parameters, fixed record predicates, and EXPLAIN QUERY PLAN. Single-path equality filters on the leading collection use its managed index; id equality uses the physical primary-key index.
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

- ORDER BY resolves projected aliases inside supported arithmetic/scalar/helper expressions, including DISTINCT output reuse and mixed source inputs. Broader WHERE/GROUP alias rules remain unfinished.

- Ordinary SQL value literals in covered SELECT/write/schema/trigger-expression contexts no longer trigger the managed-name guard; single-quoted object references remain protected.

## Verification

The scoped test suite includes parser collision probes; persistent CRUD/reopen; mixed transaction rollback; failed unique inserts/updates/index builds; validation-definition rollback; typed round trips; numeric/index identity; a child process that exits without closing an active transaction; and differential ordinary SQL probes against the pinned engine. On 2026-09-06, `fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied for the FastDB packages, and all 130 Rust tests plus fourteen Node tests (including an isolated worker-transport fault test) and strict TypeScript declaration checks (Rust coverage includes three reference-validation tests, three standalone-parameter tests, two numeric-precision tests, three interruption tests, four migration tests, four transfer tests, three window tests, two mixed-star tests, four grouping tests, five bundled-runtime/function tests, six vector tests, four forward-link tests, four batch tests, three transaction-report tests, two CLI output-failure tests and two CLI subprocess tests, eight RETURNING tests, four INSERT SELECT tests, four CASE tests, five SQL-helper tests, six document-expression tests, seven CHECK/upgrade tests, seven catalog lifecycle/version tests, the subprocess helper, fourteen collection SELECT tests, and seven SQL-shaped write/constructor tests). This is local Linux evidence; hosted CI has not run. The process-exit test is a basic recovery smoke, not interrupted-checkpoint or power-loss certification.

## Next implementation work

1. Complete the SQL-shaped write contract (broader INSERT SELECT sources and further supported statement forms) and replace the remaining conservative managed-name guard. Complete collection read cases: subqueries/CTEs, grouping alias/type coverage and broader DISTINCT/window qualification, compound/derived typed expressions, and broader index planning. Preserve baseline parameter/alias forms and ordinary SQL errors. The fallback guard now distinguishes value literals in covered SELECT/write contexts; uncovered contexts still reject some harmless strings, and dependency/name authorization remains unfinished.
2. Finish expression type propagation through comparisons and remaining SQL expressions; broader CHECK eligibility, expanded inspection and index planning, stable errors/results, cancellation and resource limits. Namespace collisions, metadata format validation, multi-connection schema races, and managed object dependency access need full coverage.
3. Complete forward-link resource/planner coverage and upstream vector representation/operation coverage; qualify the initial bundled QuickJS catalog, limits, performance and platform packaging. All five pinned vector encodings now have initial validation; broader numerical/resource/platform and benchmark evidence remains pending.
4. Complete native Node/TypeScript APIs, cancellation/lifecycle and release packaging, plus Rust packaging; lossless cross-language wire encoding; CLI interactive/streaming UX, broader import/export coverage, migration qualification, schema/query-plan inspection.
5. Complete all master-plan/FastQL release gates: broad differential coverage, interrupted commits/checkpoints, restore/upgrade rehearsal, bounded crash/fuzz/stress, resource limits, benchmarks and platform packaging smoke tests. External pilots and business evidence are also not present.

Keep upstream implementation files unchanged. No cloud implementation or V2/V3 features have begun.

## SELECT lowering implementation notes

`frontend/src/select.rs` parses through the pinned SQLite AST and rewrites collection sources/field expressions. `frontend/src/functions.rs` registers static pure accessors on each private engine connection before exposing it. Scalar access rejects objects/arrays/vectors; typed projections decode the tagged value. Record ORDER BY uses canonical targets and signed integer ordering before string keys. Index candidates are selected only for simple equality/AND predicates with constant or bound keys, and the original predicate is retained for correctness. Other predicates remain engine-evaluated scans; no index use is claimed for them.

The current result metadata distinguishes direct typed field projections from ordinary SQL scalar expression results. Typed values flowing through arbitrary expressions, binary literals compared to typed binary fields, mixed record/scalar ordering, complete alias resolution, and metadata snapshot races still require work before V1 semantics can freeze. Scalar DISTINCT now has initial coverage; CTE/derived-table collection queries still fail instead of being advertised as implemented. This does not reduce the master-plan scope.

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

Collection INSERT with an explicit target column list now consumes the existing SELECT lowering subset, including typed collection fields/helpers, explicit relational projections, filters, joins, sorting and pagination. Target fields map by position, so repeated source projection names are accepted for insertion. Column-count mismatches fail even when the source returns no rows. All source values are materialized before mutation; self-inserts consume only the original source rows. Generated IDs, required/type/CHECK validation, unique constraints and managed indexes use the normal insert path inside one savepoint. A later row failure rolls back all inserted rows and index entries while retaining an existing outer transaction.

CTEs, grouping aliases, broader DISTINCT/window coverage, compound sources and other unsupported source shapes remain pending with broader SELECT work. Modified/compound VALUES sources fail explicitly instead of silently processing only their first VALUES component. This materializing implementation still needs bounded-memory/resource accounting and release-level concurrency verification.

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
