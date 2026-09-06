# V1 implementation status

V1 is incomplete. The full scope is the FastDB.md master plan in the parent planning directory. This file records current evidence and work remaining; no milestone substitutes for the full V1 goal.

## Repository and tooling

- Local Git checkout: `turso/`, branch `feat/embedded-foundation` (local `main` is the baseline), upstream v0.7.2 at `046e9cbf67d22491e8ecc941ec2891b02a9f3cad`.
- Four workspace crates: fastql-parser, fastdb, fastdb-cli, fastdb-tests. All product crates are unpublished 0.1.0 prototypes.
- Scoped script: `fastdb/scripts/check.sh`; one Ubuntu CI YAML, read-only permissions, timeout and cancellation. Inherited workflow files moved unchanged to `.github/upstream-workflows/`.
- Remote FastDB fork owner is unresolved. No origin remote, push, PR, branch protection, or hosted FastDB CI result exists yet.
- Local toolchain is installed under `/tmp/fastdb-cargo` and `/tmp/fastdb-rustup`. Run with PATH prefixed by `/tmp/fastdb-cargo/bin`, CARGO_HOME and RUSTUP_HOME set accordingly, and RUSTUP_TOOLCHAIN=1.88.0. These temporary tools may need reinstalling on another machine/session.

## Implemented subset

- Bare collection CREATE TABLE, IF NOT EXISTS, fixed typed IDs, direct-record SELECT, object/DOCUMENT INSERT, target object UPDATE, target DELETE, RETURNING *.
- Nested objects/arrays, tagged persisted values, bool/int64/null/binary distinction, UUIDv7 automatic IDs.
- Field definitions (basic types, required/nullable, nested paths, overwrite) and single-path managed scalar/reference indexes with unique/nonunique variants. Rust APIs and initial FastQL declarations.
- Statement savepoints, transactional catalog/index maintenance, mixed ordinary SQL/document transactions.
- Rust query cardinality helpers and initial line-oriented stdin CLI with tagged JSON output.
- AST-lowered collection SELECT: typed field/document projections, scalar expressions, WHERE, explicit joins (including mixed relational/document joins), ORDER BY, LIMIT/OFFSET, named typed parameters, fixed record predicates, and EXPLAIN QUERY PLAN. Single-path equality filters on the leading collection use its managed index; id equality uses the physical primary-key index.
- SQL column-list VALUES inserts (including multiple rows), predicate-based multirow UPDATE/DELETE, nested SET/UNSET, and RETURNING *. Whole statements share a savepoint and evaluated candidates use pre-update values.
- SQL type::record constructors and typed record expression projections, including standalone SELECT; named and numbered/anonymous value binding through the Rust parameter map.
- ID-based object UPSERT and direct-target UPSERT, transactional field removal, collection/index drop, logical INFO FOR DB/TABLE/INDEX, and non-converting IF NOT EXISTS behavior.
- Field CHECK validation uses deterministic candidate-only SQL expressions, validates existing data before publishing definitions, and applies to every supported document write path.
- Rust transaction_state()/execute_report() and CLI transaction.before/after expose observed autocommit/active state on success and failure.
- Object/DOCUMENT INSERT, object UPDATE/UPSERT, predicate patches and direct DELETE support typed RETURNING projections and empty-result metadata.
- SQL-shaped INSERT/UPDATE/DELETE support typed RETURNING projections from final/deleted document snapshots, including empty-result column metadata.
- Column-list INSERT SELECT copies typed source rows through normal validation/index maintenance and materializes sources before writing, including self-inserts.
- Searched and simple CASE preserve typed branch results in SQL SELECT/SET/VALUES and object-write expressions, with lazy branch evaluation.
- SQL SELECT/SET/VALUES support typed record extractors, array::new/append, doc::get/has, collection doc::row, parentheses and lazy coalesce/ifnull.
- Object writes evaluate scalar operators and nested function calls with typed record/array/document helpers; predicate object patches and UPSERT expressions read pre-update values.
- Catalog writes use version 2 for CHECK semantics; version 1 and unversioned prototype metadata remain readable, and unknown/inconsistent versions are rejected.
- Ordinary SQL delegation outside the conservative collection-name guard, which still protects unimplemented forms.

## Verification

The scoped test suite includes parser collision probes; persistent CRUD/reopen; mixed transaction rollback; failed unique inserts/updates/index builds; validation-definition rollback; typed round trips; numeric/index identity; a child process that exits without closing an active transaction; and differential ordinary SQL probes against the pinned engine. On 2026-09-06, `fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied for the FastDB packages, and all 67 tests (including three transaction-report tests and one CLI subprocess test, seven RETURNING tests, four INSERT SELECT tests, four CASE tests, five SQL-helper tests, six document-expression tests, seven CHECK/upgrade tests, seven catalog lifecycle/version tests, the subprocess helper, five collection SELECT tests, and seven SQL-shaped write/constructor tests). This is local Linux evidence; hosted CI has not run. The process-exit test is a basic recovery smoke, not interrupted-checkpoint or power-loss certification.

## Next implementation work

1. Complete the SQL-shaped write contract (broader INSERT SELECT sources and further supported statement forms) and replace the remaining conservative managed-name guard. Complete collection read cases: subqueries/CTEs, grouping/DISTINCT/window semantics, arbitrary-depth paths, compound/derived typed expressions, and broader index planning. Preserve baseline parameter/alias forms and ordinary SQL errors. The old fallback guard still rejects some harmless strings and is not a final compatibility/security boundary.
2. Finish expression type propagation through comparisons and remaining SQL expressions; broader CHECK eligibility, expanded inspection and index planning, stable errors/results, cancellation and resource limits. Namespace collisions, metadata format validation, multi-connection schema races, and managed object dependency access need full coverage.
3. Add snapshot-consistent batched one-hop links, bundled bounded QuickJS functions, and verified exact upstream vectors. Vector bytes currently have no validated public constructor/field validator; do not advertise vector support yet.
4. Native Node/TypeScript client and complete Rust packaging; lossless cross-language wire encoding; CLI multiline/batch UX, import/export, migrations, schema/query-plan inspection.
5. Complete all master-plan/FastQL release gates: broad differential coverage, interrupted commits/checkpoints, restore/upgrade rehearsal, bounded crash/fuzz/stress, resource limits, benchmarks and platform packaging smoke tests. External pilots and business evidence are also not present.

Keep upstream implementation files unchanged. No cloud implementation or V2/V3 features have begun.

## SELECT lowering implementation notes

`frontend/src/select.rs` parses through the pinned SQLite AST and rewrites collection sources/field expressions. `frontend/src/functions.rs` registers static pure accessors on each private engine connection before exposing it. Scalar access rejects objects/arrays/vectors; typed projections decode the tagged value. Record ORDER BY uses canonical targets and signed integer ordering before string keys. Index candidates are selected only for simple equality/AND predicates with constant or bound keys, and the original predicate is retained for correctness. Other predicates remain engine-evaluated scans; no index use is claimed for them.

The current result metadata distinguishes direct typed field projections from ordinary SQL scalar expression results. Typed values flowing through arbitrary expressions, binary literals compared to typed binary fields, mixed record/scalar ordering, complete alias resolution, and metadata snapshot races still require work before V1 semantics can freeze. Unsupported DISTINCT/CTE/group/window/derived-table collection queries fail instead of being advertised as implemented. This does not reduce the master-plan scope.

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

Typed parameter occurrences are currently encoded as hex literals in the per-execution lowered AST; other occurrences retain ordinary binding. This avoids confusing user binary data with internal values, but larger values amplify lowered SQL size. Prepared-plan caching, parameter allocation/resource accounting, broader type propagation, record/composite scalar behavior, and comprehensive expression-error contracts remain release work. Helpers are statically linked Rust functions; bundled QuickJS remains a separate unimplemented V1 requirement.

## Conditional expression notes

Both searched CASE (WHEN predicates) and simple CASE (one base value matched against WHEN values) now preserve the selected branch's type. SQL lowering retains engine CASE control flow while encoding result branches; object expressions evaluate the base once and evaluate conditions in order until a match. An omitted ELSE returns null. Nested CASE, helper composition, scalar predicate contexts and typed ordering work through the existing lowering. Writes validate final candidates and retain whole-statement rollback. SQL simple-CASE comparisons still use the current scalar lowering; broader record/composite comparison semantics remain release work.

## INSERT SELECT notes

Collection INSERT with an explicit target column list now consumes the existing SELECT lowering subset, including typed collection fields/helpers, explicit relational projections, filters, joins, sorting and pagination. Target fields map by position, so repeated source projection names are accepted for insertion. Column-count mismatches fail even when the source returns no rows. All source values are materialized before mutation; self-inserts consume only the original source rows. Generated IDs, required/type/CHECK validation, unique constraints and managed indexes use the normal insert path inside one savepoint. A later row failure rolls back all inserted rows and index entries while retaining an existing outer transaction.

CTEs, grouping/DISTINCT/windows, compound sources, relational stars and other unsupported source shapes remain pending with broader SELECT work. Modified/compound VALUES sources fail explicitly instead of silently processing only their first VALUES component. This materializing implementation still needs bounded-memory/resource accounting and release-level concurrency verification.

## RETURNING projection notes

SQL-shaped collection INSERT (VALUES/SELECT), UPDATE and DELETE now return named scalar/typed projections, aliases and full-document stars. INSERT/UPDATE projections use final documents; DELETE projections use the deleted documents. The projection source is a synthetic row containing the document and typed ID, so projection evaluation does not refetch deleted data. Empty writes still prepare an empty snapshot to return column metadata and validate projection shape. All projection evaluation occurs inside the statement savepoint; no partial rowset is returned on failure.

Subqueries, aggregates and windows are rejected in RETURNING. Aggregate detection tracks the pinned engine's built-in aggregate list and needs review during upgrades. Object INSERT/UPDATE/UPSERT, predicate object patches and direct document DELETE use the same typed RETURNING projection path. Projection snapshot encoding/materialization still needs resource accounting and prepared-plan reuse.

A runtime helper failure can make pinned Turso abort the outer transaction as well as the current write; tests now demonstrate this and successful reuse of the connection afterward. This is within the prototype's engine-abort exception, not a guarantee that every expression failure preserves an outer transaction. Rust and CLI now expose observed transaction state as described below; finer error taxonomy remains release work.

Object RETURNING grammar retains the projection text for the pinned SQL AST parser. A projection is a list of result expressions, not a new query: FROM/WHERE/compound clauses, subqueries, aggregates, windows and managed internal names are rejected. Object insertion/deletion and projection evaluation now share an outer statement savepoint, so invalid projection syntax cannot leave a successful mutation behind. Quoted fields, string tokens and nesting are distinguished when separating predicate patches from their RETURNING clause; a semicolon inside a string remains data.

## Transaction observation notes

Connection::transaction_state() samples the pinned engine autocommit flag without issuing SQL. execute_report() retains the original Result<QueryResult> and reports transaction_before/transaction_after as Autocommit or Active. Active includes an explicit transaction or outer savepoint; it is not proof that a future COMMIT will succeed. Autocommit does not identify whether earlier work committed or rolled back. Callers must serialize operations on each connection to attribute observations to one execution.

The CLI adds transaction.before/after (snake_case state strings) to each success/error JSON line while retaining existing result/error fields. Tests distinguish Rust validation failures that leave a transaction active from runtime helper failures that abort it and discard earlier uncommitted work. Savepoint transitions, independent connections and close/reopen after a retained transaction commit are covered. This is observational reporting, not a complete error-disposition, durability or cross-language wire contract.
