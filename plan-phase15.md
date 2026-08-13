# FastDB Phase 15 — Scripting, schema, views, and events

Status: authoritative implementation plan; implementation not started

Starting checkpoint: `a027a777d`

## 1. Purpose and boundary

Phase 15 implements the 41 currently Unsupported capabilities assigned to
Phase 15 in the locked SurrealDB `v3.1.5` inventory and preserves the two
already Supported basic table/field capabilities. It adds bounded SurrealQL
scripting, canonical database parameters and custom functions, richer table
and field definitions, materialized views, synchronous events, sequences, and
matching ALTER/REMOVE/INFO lifecycle operations.

The phase retains format 3, the engine pin
`977383ff40edc44ef410af062ed0d2322252a869`, direct Turso-AST lowering, bound
values, full-durability stable WAL, and one serialized writer. It does not
publish Phase 16 graph completion, Phase 17 specialized indexes, Phase 18
authorization, Phase 19 server execution, Phase 20 namespace/database control,
Phase 21 SDKs, multiprocess or parallel writers, geospatial/history/realtime
behavior, GraphQL/GQL, a tag, a package, an artifact, or a production-ready
claim.

Phase 15 may parse server-owned API, namespace, database, and USE statement
shapes only where needed to lock the independent AST. Their execution remains
unavailable until their owning phases. Bucket definitions are metadata-only
unless a bounded storage capability already exists; WASM modules require a
sealed provider and may stop with an architecture report rather than loading
arbitrary native or unbounded code.

## 2. Locked capability target

The Phase 15 target is the locked 43-row set:

- eight ALTER rows: API, BUCKET, EVENT, FIELD, FUNCTION, PARAM, SEQUENCE, and
  TABLE;
- nine DEFINE rows: BUCKET, EVENT, basic/complete FIELD, FUNCTION, MODULE,
  PARAM, SEQUENCE, basic/complete TABLE, and VIEW (the two basic rows are
  already Supported);
- eight INFO rows: API, BUCKET, EVENT, FIELD, FUNCTION, PARAM, SEQUENCE, and
  TABLE;
- eight complete REMOVE rows: API, BUCKET, EVENT, FIELD, FUNCTION, PARAM,
  SEQUENCE, and TABLE; and
- BREAK, CONTINUE, FOR, IF/ELSE, LET, RETURN, SLEEP, and THROW.

Inventory IDs, granularity, reference version, phase assignment, and roadmap
exclusions remain locked. No Phase 15 Partial row remains at the completion
checkpoint. A target may remain Unsupported only with a narrow architecture
stop allowed by the roadmap; implementation effort or missing time is not a
stop.

## 3. Clean-room characterization

Use public documentation and independently designed black-box probes against
the verified external unmodified SurrealDB `v3.1.5` binary. Record the binary
version/checksum, date, exact input, normalized output or error class, and
public source links in `docs/compat-research/phase15.md`. Do not inspect or
adapt SurrealDB source, tests, fixtures, expected outputs, or corpora.

Characterize at least:

- script block delimiters, statement/value result shapes, lexical variable
  scope and shadowing, forward references, IF truthiness, FOR iteration order,
  BREAK/CONTINUE legality, RETURN propagation, THROW payloads, and SLEEP
  cancellation/deadlines;
- custom-function naming, typed/default arguments, captures, recursion,
  replacement rules, returns, error propagation, and definition rendering;
- database-parameter names, value evaluation time, type behavior, replacement,
  visibility, persistence, and interaction with bound request parameters;
- each table/field DEFAULT, ASSERT, VALUE/computed, READONLY, REFERENCE, DROP,
  COMMENT, literal/union/collection type, and alteration/removal behavior;
- view creation/backfill, source dependencies, projection/filter/group behavior,
  source writes, view writes, schema changes, rebuild/reopen, and removal;
- event WHEN/THEN before/after images, mutation ordering, nested writes,
  recursion, multiple events, failures, relations/providers/views, and returns;
- sequence bounds, start/increment/cycle behavior, transactional allocation,
  rollback, exhaustion, alteration, and reopen; and
- canonical INFO shapes plus IF EXISTS/IF NOT EXISTS, OVERWRITE, dependency,
  and FORCE/CASCADE spelling for every characterized lifecycle statement.

Reference behavior that conflicts with an explicit roadmap exclusion is
recorded but not implemented. Ambiguity is resolved by a probe, not by moving
online documentation or memory.

## 4. Independent AST and parser

Extend only FastDB-owned lexer/parser/AST types. User source never reaches
Turso's SQLite parser and no new node stores a Turso AST.

- Add structured script statements and blocks for LET, RETURN, IF/ELSE IF/
  ELSE, FOR, BREAK, CONTINUE, THROW, and SLEEP. Blocks retain spans and ordered
  statements; control-flow keywords cannot appear in invalid contexts.
- Add structured DEFINE/ALTER/REMOVE/INFO nodes for functions, parameters,
  views/tables, fields, events, sequences, buckets, modules, and API metadata.
- Represent custom-function arguments, field types/clauses, event predicates/
  actions, view SELECTs, sequence options, comments, overwrite/if-exists flags,
  and dependency modes explicitly. No clause is accepted and ignored.
- Parse server-owned namespace/database/API execution syntax to typed dormant
  nodes only when its owning inventory row requires Phase 15 parsing; execution
  returns a precise Capability/Unsupported error until Phase 19 or 20.
- Preserve complete canonical definitions from AST printers that quote logical
  names and literals safely. The printer is for catalog evidence and INFO,
  never for engine execution or reparsing through SQLite.

Enforce parser ceilings for block depth, statements per block, clauses,
function arguments, field union members, event actions, view dependencies,
identifier/source bytes, and nested expressions. Duplicate, out-of-order, and
mutually exclusive clauses fail with exact spans.

## 5. Script evaluator and execution budgets

Run scripts through a FastDB-owned interpreter over the Phase 13 evaluator and
Phase 14 statement executors. Use a lexical environment distinct from bound
request parameters and catalog parameters. Variable resolution order is
explicit and characterized; request values remain immutable and are never
rendered into source.

Represent BREAK, CONTINUE, RETURN, and THROW as internal typed control-flow
signals, not public error strings. A signal may cross only its valid scope.
Condition branches and loop bodies execute lazily. FOR snapshots a bounded
array/set/range before iteration, preserves characterized order, and enforces
iteration, statement, nesting, allocation, output, and wall-clock limits.

Every standalone script uses the same statement-level atomicity contract
characterized from the reference. Inside an explicit transaction, any ordinary
error or uncaught THROW poisons and rolls back the transaction. Control flow
does not accidentally poison. SLEEP is asynchronous at the public API worker
boundary, interruptible, capped, and composed with the smaller request/
statement deadline; it never holds the schema mutex and cannot create an
unbounded worker backlog.

## 6. Canonical functions and parameters

Use the sealed format-3 `__fastdb_functions` and `__fastdb_parameters`
catalogs. Logical names resolve to immutable catalog IDs; stored definitions,
argument AST, bodies, values, versions, and limits are canonical and validated
on reopen before user execution.

- DEFINE FUNCTION validates the complete body before catalog mutation and
  installs it atomically. Invocation binds arguments by value, uses a fresh
  lexical frame, routes every statement through the FastDB frontend, and
  enforces recursion, call-count, statement, time, memory, output, and mutation
  budgets shared with the caller.
- Custom functions cannot access ambient filesystem/network/process state,
  bypass authorization-ready visibility hooks, commit independently, install
  plugins, call inherited SQL, or dynamically evaluate source.
- DEFINE PARAM evaluates its value once according to characterized semantics,
  stores the collision-safe format-3 representation, and exposes an immutable
  database-scoped value. Bound request parameters never overwrite catalog
  parameters silently.
- ALTER/REMOVE validate call sites and dependencies before publication. INFO
  returns ordinary structured values and canonical public definitions without
  opaque physical names or hidden values.

Function and parameter catalog changes take the database schema mutex, are
transactional, invalidate relevant parse/definition caches only after commit,
and restore the previous snapshot on rollback or failure injection.

## 7. Complete table and field schema

Extend cataloged field schemas with the characterized non-geospatial format-3
type expressions, unions/literals/collections, DEFAULT, ASSERT, VALUE/computed,
READONLY, REFERENCE, COMMENT, and later-authorization permission metadata.
Documents remain authoritative; computed values and provider representations
are derived and rebuildable.

Use one deterministic normalization pipeline for CREATE, INSERT, UPSERT,
UPDATE, event writes, graph edges, and view maintenance:

1. Resolve schema and immutable before image.
2. Apply declared defaults only where the reference does.
3. Evaluate computed/VALUE expressions with bounded dependency ordering.
4. Normalize field types without lossy conversion.
5. Enforce readonly/reference and assertion rules.
6. Maintain hidden graph/FTS/vector state and views atomically.

Reject cycles in computed/default dependencies, mutation of synthesized
`id`/`in`/`out`, nondeterministic or context-unsafe schema expressions, and
references whose semantics cannot be enforced. ALTER and REMOVE validate all
existing documents and dependent indexes/events/views/functions before the
catalog changes. DROP/table lifecycle behavior must preserve or remove opaque
physical objects only after dependency and rollback gates pass.

Permissions are parsed and stored canonically but enforcement is not claimed
until Phase 18. Trusted local Owner behavior remains unchanged.

## 8. Views and dependency graph

Implement characterized table views as FastDB-owned materialized derived
tables, not SQLite views and not generated SQL. Store a canonical structured
SELECT definition and immutable dependency IDs in `__fastdb_views`; assign the
view an opaque physical table and derived row identity strategy proven by
reference probes.

View creation evaluates and validates the complete source query before
publication, backfills inside one transaction, and fails without a catalog or
physical orphan. Source mutations compute all affected view deltas from the
same before/after snapshots used by indexes and events. View documents,
indexes, FTS/vector columns, graph-compatible fields, and downstream views
change atomically. Direct writes to read-only views are rejected explicitly.

Build a bounded acyclic dependency graph for tables, views, functions,
parameters, fields, and events. Reject cycles and unsupported nondeterministic
queries before mutation. ALTER/REMOVE operations either reject live
dependencies or apply only the exact characterized explicit dependency mode.
Reopen/check validates definitions, IDs, physical objects, dependency edges,
and derived rows; rebuild deterministically derives views from authoritative
documents.

## 9. Synchronous events

Use the sealed `__fastdb_events` catalog with immutable table/event IDs,
canonical WHEN and THEN AST source, expression version, and explicit limits.
Definitions compile through the FastDB parser before publication and are
revalidated on reopen.

For each mutation, evaluate matching events in characterized deterministic
order against immutable before/after context. Execute THEN statements through
the same frontend and active transaction. Event-created mutations may trigger
other events only within a global recursion/statement/mutation/time/output
budget and deterministic cycle detector. Event results are not leaked into the
outer result unless characterized.

Any event parse, evaluation, schema, constraint, resource, interrupt, provider,
or I/O error rolls back the outer document, graph adjacency, FTS/vector state,
views, sequence allocations, and event side effects together. Failure
injection covers every boundary before catalog publication and between each
derived-state mutation.

## 10. Sequences, buckets, modules, and API metadata

Implement sequences only with bounded integer state, checked increment,
explicit minimum/maximum/start/cycle semantics, and transactionally durable
allocation. Sequence reads/writes use opaque catalog-managed state and never
Turso's unrelated PostgreSQL frontend. Rollback must not publish an allocation
unless that is the independently characterized `v3.1.5` behavior.

Bucket and API definitions are closed canonical metadata capabilities. They do
not grant filesystem/network access or server routes in Phase 15. Any later
resource use must pass the Phase 13 capability policy and Phase 18/19
authorization boundaries.

DEFINE MODULE is supported only if a FastDB-owned sealed WASM provider can
enforce deterministic imports, fuel, memory, stack, time, output, provenance,
and versioned persistence without a Turso core change. Native dynamic loading,
WASI ambient authority, arbitrary plugins, and source-to-SQL translation are
prohibited. If no qualifying provider exists, write a row-specific architecture
stop and reject the syntax before mutation.

## 11. Catalog publication, caches, and recovery

Every schema object creation, alteration, removal, backfill, dependent-object
change, and first mutation is one transaction under the schema mutex. Catalog
IDs and opaque physical names are collision-safe and never interpolate logical
names into internal SQL. Static reviewed internal DDL may reference only
validated opaque identifiers.

Publish a new immutable catalog snapshot and invalidate definition/query
caches only after successful commit. Rollback, transaction poisoning, commit
failure, dropped futures, and abrupt process exit leave the previous snapshot
and physical schema usable. Reopen refuses malformed, noncanonical, unknown-
version, dangling, cyclic, or physical-object-mismatched definitions before
mutation. Backup/restore/check cover all Phase 15 catalogs and derived state.

No on-disk format bump is planned because format 3 reserved the required
catalogs. If characterization proves that collision-safe durable state cannot
fit the sealed schema without ambiguity, stop and write a migration design;
do not silently alter format 3.

## 12. Resource limits, API, and CLI

Extend `ResourceLimits`/`QueryOptions` with validated hard ceilings for script
statements, block/call/loop/event/view depth, loop iterations, custom-function
calls, event mutations, schema dependencies, backfill candidates, sleep
duration, sequence work, and derived output bytes. Defaults are conservative
and upper bounds are compile-time capped.

Check interruption and deadlines at every interpreter step, loop iteration,
function/event call, schema expression, dependency traversal, backfill batch,
derived-state write, and result materialization. Errors identify only the
object/category and span; source, bound values, passwords/tokens, THROW
payloads, and stored documents are not logged.

The existing Rust API and CLI expose supported statements through ordinary
query/execute and typed JSON envelopes. INFO returns ordinary structured
values. No server endpoint, unsafe FFI, or separate public engine-plan/schema
type is introduced.

## 13. Implementation order and commit boundaries

1. Commit this plan alone.
2. Characterize Phase 15 and commit script/schema AST plus negative grammar.
3. Commit the bounded script interpreter, lexical environment, control-flow
   signals, deadlines, and API/CLI integration.
4. Commit canonical database parameters and custom functions with reopen and
   recursion evidence.
5. Commit complete field/table normalization and lifecycle operations.
6. Commit sequences and closed metadata objects; stop MODULE only if the
   provider gate cannot qualify.
7. Commit materialized views and dependency validation.
8. Commit synchronous events and cross-derived-state failure injection.
9. Commit ALTER/REMOVE/INFO completion and catalog/check/rebuild integration.
10. Promote inventory rows only with named executable evidence, run the full
    gate, write `docs/phase15-report.md`, update durable status, and commit
    `phase 15 complete` with exact start/end SHAs and rollback range.

No Phase 16 plan or implementation begins before the Phase 15 checkpoint.
Rollback uses explicit Git reverts of the recorded Phase 15 range.

## 14. Verification matrix

At minimum, independently authored tests cover:

- every script statement in nested/invalid contexts, lexical shadowing,
  control-flow propagation, collection iteration order, below/at/above limits,
  interruption, timeout, explicit transaction poisoning, and reopen;
- function/parameter define/alter/remove/info, typed/default arguments,
  recursion, captures, dependency refusal, canonical persistence, malformed
  catalog rows, request-bind collisions, and concurrent schema publication;
- every complete field/table clause across CREATE/INSERT/UPSERT/UPDATE,
  defaults/assertions/computed ordering, readonly/reference behavior, existing
  row backfill, rollback, relations, FTS, vectors, indexes, and reopen;
- sequence boundary/exhaustion/cycle/concurrency/rollback/recovery behavior;
- view backfill and incremental deltas for insert/update/delete, grouping,
  provider fields, dependency cycles, rebuild, corruption, failure injection,
  backup/restore, and abrupt exit;
- event WHEN/THEN ordering, before/after context, nested writes, recursion,
  rollback at every document/adjacency/provider/view boundary, and no result or
  diagnostic leakage;
- exact INFO shapes and every lifecycle IF/OVERWRITE/dependency form;
- MODULE/bucket/API capability denials before mutation and without ambient
  authority where a capability is stopped or dormant;
- deterministic inventory/COMPAT synchronization, no Phase 15 Partial row,
  fixture hashes, check/rebuild, backup/restore, fuzz/model/crash coverage, and
  all unchanged Phase 6–14 behavior; and
- formatting, scoped Clippy, all FastDB suites, release build, retained and
  Phase 15 performance/memory gates, plus unchanged Turso core, PostgreSQL,
  and Whopper suites.

## 15. Stop conditions

Stop the affected capability and write an architecture report rather than
weakening it if it requires generated SQLite text, logical-name interpolation,
copied SurrealDB material, arbitrary native plugins, ambient WASI authority,
an unqualified Turso core change, unbounded or non-interruptible execution,
authorization-hostile visibility, non-atomic derived state, nondeterministic
recovery, or value/secret-bearing diagnostics.

Stop the entire phase if scripting/schema/events cannot preserve explicit
transaction poisoning, stable-WAL acknowledged durability, format-1/2/3
reopen/migration, graph cascade, FTS/vector atomicity, backup/check/rebuild,
bounded worker behavior, or the existing Rust/CLI result contract.

## 16. Definition of done

Phase 15 is complete only when all 43 locked Phase 15 rows are Supported or a
remaining target has an approved narrow architecture stop; no Phase 15 Partial
row remains. Every Supported row names passing parser and execution evidence.
All catalog, storage, crash, resource, API/CLI, compatibility, regression,
fuzz/model, memory, and performance gates above pass and exact commands/raw
measurements are recorded in `docs/phase15-report.md`.

Completion is a local pre-1.0 compatibility checkpoint only. It does not
authorize Phase 16, a server, package publication, parallel writers, or a
production-ready/Core 1.0 claim.
