# FastDB Phase 14 — CRUD and query completeness

Status: technically complete locally on 2026-08-14; see
`docs/phase14-report.md`

Starting checkpoint: `d3ccf412e`

## 1. Purpose and boundary

Phase 14 implements the 26 currently Unsupported capabilities assigned to
Phase 14 in the locked SurrealDB `v3.1.5` inventory and preserves the 24
already Supported Phase 14 rows. It completes the non-graph CRUD/query
statement surface: INSERT, UPSERT, complete CREATE/UPDATE/DELETE data clauses
and return modes, richer SELECT projections and targets, aggregation/grouping,
split/omit/fetch, ordering/pagination, subqueries, and full explain/analyze.

The phase retains format 3, stable full-durability WAL, serialized writers,
the engine pin `977383ff40edc44ef410af062ed0d2322252a869`, and the official
unmodified SurrealDB `v3.1.5` behavioral reference. It does not publish Phase
15 scripting/schema/event statements, Phase 16 graph-completion syntax, Phase
18 principals/permissions, a network server, SDKs, multiprocess access,
parallel writers, geospatial/history/realtime behavior, GraphQL/GQL, a tag,
package publication, artifact upload, or a production-ready claim.

No on-disk format bump is planned. Documents remain authoritative. Complex
record identifiers use a backward-compatible format-3 RID codec extension;
record ranges are query targets and are never persisted as record IDs.

## 2. Locked capability target

The Phase 14 checkpoint must preserve the locked 50-row Phase 14 inventory and
promote these 26 currently Unsupported rows only after executable evidence:

- record-ID ranges and complex array/object record IDs;
- DIFF, FETCH, GROUP, MERGE, OMIT, complete ORDER/PAGINATION/WHERE, PATCH,
  REPLACE, RETURN BEFORE/VALUE, SPLIT, and UNSET clauses;
- full EXPLAIN/ANALYZE;
- aggregate, destructured, multi-target, and SELECT VALUE queries; and
- complete CREATE, UPDATE, and DELETE plus INSERT and UPSERT statements.

No Phase 14 Partial row remains at the checkpoint. A target can remain
Unsupported only with an architecture stop satisfying the roadmap's narrow
criteria; missing implementation time is not a stop. Inventory IDs,
granularity, phase assignment, reference version, and exclusions do not
change.

## 3. Clean-room characterization

Use public documentation and independently designed probes against the
verified external `v3.1.5` binary. Record version, checksum, date, exact input,
normalized result/error class, and public links in
`docs/compat-research/phase14.md`. Never inspect or adapt SurrealDB source,
tests, fixtures, expected-output files, or corpora.

Probe complete equivalence classes before implementing:

- every INSERT source form, omitted/default fields, duplicate IDs, IGNORE,
  relation-table behavior, multi-row failure, return modes, and any observed
  duplicate-key clause;
- UPSERT existence races, generated/explicit/complex IDs, each data clause,
  conditions, no-match behavior, and return modes;
- CREATE/UPDATE/DELETE ONLY/FROM variants, arrays/ranges/multiple targets,
  SET operators, CONTENT/MERGE/PATCH/REPLACE/UNSET, schema normalization,
  reserved fields, missing records, and duplicate assignment paths;
- RETURN NONE/BEFORE/AFTER/DIFF/VALUE expression shapes for zero, one, and
  many mutations;
- SELECT VALUE, aliases, wildcards/destructuring, OMIT, multiple heterogeneous
  targets, subqueries, FETCH nesting, SPLIT multiplicity, aggregate empty-set
  behavior, GROUP ALL and multiple group keys, ordering modifiers, randomized
  ordering, expression/parameter pagination, and ONLY cardinality;
- NONE/NULL/type/error propagation and clause pipeline order across WHERE,
  SPLIT, GROUP, projection, OMIT, ORDER, START/LIMIT, and FETCH; and
- EXPLAIN, EXPLAIN FULL, ANALYZE, and combined modes with and without executed
  mutations or provider queries.

Ambiguous behavior is resolved by a recorded probe, not a remembered current
manual. Moving online documentation does not alter the pin.

## 4. Independent AST and parser

Extend the FastDB AST structurally; do not encode new clauses as raw source or
Turso AST nodes.

- Add explicit `Insert` and `Upsert` statements with ordered source/data,
  target, condition, duplicate policy, and return nodes.
- Generalize mutation data into mutually exclusive CONTENT, MERGE, PATCH,
  REPLACE, SET, and UNSET forms. SET retains structured assignment operators;
  PATCH retains an expression whose evaluated value is a bounded patch array.
- Generalize query/mutation targets into an ordered target list containing
  tables, records, bounded record ranges, arrays/parameters where
  characterized, and structured subqueries where allowed.
- Extend `RecordIdPartKind`/public RID components with recursively bounded
  array/object values using the same canonical value order as Phase 13.
- Add structured SELECT VALUE, projection wildcards/destructuring, OMIT,
  SPLIT, GROUP, complete ORDER terms/modifiers, expression LIMIT/START, FETCH,
  and subquery-expression nodes.
- Add RETURN VALUE and DIFF nodes that retain their expression or mode and
  source spans.
- Add EXPLAIN FULL/ANALYZE flags and retain the exact nested statement node.

Clause order, duplicates, mutual exclusion, required aliases, aggregate
placement, nested query depth, target count, projection count, group/order
terms, and fetch/split paths are parser-limited. Unsupported variants fail
with precise spans; no clause is accepted and ignored. FastDB source never
reaches Turso's SQLite parser.

## 5. Complex identifiers and target resolution

Extend `RecordIdValue` with canonical array/object components built from the
public non-geospatial format-3 `Value` subset. Reject NONE, non-finite values,
records/ranges inside components where the reference rejects them, excessive
depth/elements/bytes, and noncanonical encodings. Existing string, integer,
and UUID RID bytes remain byte-for-byte stable.

The physical RID codec uses a new sealed tag/version branch with length-
delimited typed components; it does not concatenate source text. Decode must
round-trip, reject malformed/noncanonical bytes as format corruption, and
preserve collision freedom across every old/new component type. Public JSON
and SDK envelopes preserve typed complex IDs.

Resolve table, record, range, list, parameter, and subquery targets to a
bounded ordered set of immutable logical record IDs. Deduplicate repeated
targets according to characterized semantics. Range scans use encoded RID
ordering only after proving that encoding order matches the public canonical
RID order; otherwise materialize a bounded table candidate set and compare in
Rust. Multiple tables retain deterministic target and RID order until an
explicit ORDER clause changes it.

## 6. Mutation execution

Every standalone mutation is one transaction; every multi-row INSERT/UPSERT
and multi-target CREATE/UPDATE/DELETE is all-or-nothing. Explicit-transaction
errors poison and roll back the complete transaction. Never replay a
transaction implicitly.

- INSERT evaluates all input rows first, validates field lists/row arity,
  resolves/generates IDs, rejects duplicate IDs within the batch, and then
  publishes catalog registration, schema normalization, documents, hidden
  columns, graph adjacency, FTS, and vector state atomically.
- UPSERT resolves existence inside the write transaction and applies exactly
  the characterized create/update branch without a read/write race.
- CONTENT/REPLACE construct a replacement object; MERGE performs the recorded
  recursive or shallow merge; PATCH reuses the bounded Phase 13 value patch;
  SET evaluates against the before-image snapshot; UNSET removes canonical
  paths without permitting reserved `id`, `in`, or `out` mutation.
- UPDATE/DELETE target snapshots are fixed before writes, so one row's change
  cannot alter later membership. Node deletes retain Phase 7 atomic relation
  cascades and all provider cleanup.
- RETURN BEFORE/AFTER/VALUE/DIFF is materialized from immutable before/after
  images after successful validation but before commit publication. RETURN
  NONE avoids unnecessary document output. ONLY enforces characterized
  cardinality without allowing a partial write.

All logical values are bound. Static reviewed internal DDL is allowed only for
opaque physical objects; no user source, value, path, logical table, or field
name is rendered into SQLite text.

## 7. Query pipeline and bounded relational semantics

Use one explicit FastDB query pipeline whose stage order is fixed by the
reference probes. Candidate acquisition may use direct Turso AST and indexes;
all semantics not proven equivalent execute in bounded Rust over decoded
values.

1. Resolve ordered targets and safe index/provider candidate plans.
2. Apply authorization-ready record visibility hook and authoritative WHERE.
3. Apply SPLIT expansion with a multiplicative result ceiling.
4. Form GROUP ALL/key groups using Phase 13 canonical equality/order.
5. Evaluate aggregates and ordinary/destructured/VALUE projections.
6. Apply OMIT and bounded FETCH dereferencing.
7. Apply stable complete ORDER semantics.
8. Evaluate checked START/LIMIT and enforce ONLY/result limits.

Aggregate calls are explicit AST/evaluator nodes in aggregate context rather
than overloaded row-wise scalar calls. Group accumulation is deterministic,
checked, and bounded by candidate, group, per-group, aggregate-state, byte,
and output limits. Empty-set and numeric overflow behavior follows the
recorded reference. Invalid aggregate/nonaggregate mixtures fail before
result publication.

FETCH dereferences only FastDB record IDs through catalog-resolved tables and
the current snapshot. It has depth, record, byte, and cycle limits, preserves
dangling-reference behavior, and routes every dereference through the future
authorization visibility hook. It never interpolates a table or RID into
SQLite text. SPLIT expansion, projection destructuring, and OMIT operate on
owned bounded values and preserve reserved synthesized record fields.

Subqueries execute through the same FastDB frontend with a reduced nested
budget, the active transaction snapshot, lexical parameters, cancellation,
and recursion limits. They cannot commit independently or reach Turso SQL
parsing.

## 8. Pushdown, plans, and explain

Extend safe predicate pushdown only where direct Turso AST comparison has the
same Phase 13 NONE/NULL/type/error semantics. Candidate reads remain bounded
when post-filtering is required. Composite range/equality and provider queries
retain executed-plan assertions proving actual index selection before and
after reopen.

EXPLAIN returns ordinary structured FastDB values. EXPLAIN FULL/ANALYZE may
execute only the characterized nested operation, under normal transaction,
authorization-ready, cancellation, and resource gates. Plans expose logical
operators, selected logical indexes/providers, estimated/actual row counts,
timing, and bounded post-processing stages without leaking opaque physical
names, hidden columns, source text, or parameter values. Explain never grants
a separate engine-plan public type.

## 9. Resource limits, cancellation, and API/CLI

Extend `ResourceLimits` with hard-ceiling fields for scanned candidates,
targets, mutation rows, split output, groups, aggregate state, fetch depth and
records, subquery depth, and explain events. Defaults remain conservative and
validation happens before execution. Existing timeout, output-row/byte,
graph-hop, vector-dimension, and FTS-query limits still apply.

Check cancellation/deadlines between candidate batches and at every nested,
split, group, aggregate, fetch, sort, mutation, provider-maintenance, and
return-materialization boundary. A limit/cancellation error is value-free,
poisons an explicit transaction, and leaves no partial catalog/document/index
state.

The Rust API and CLI expose every supported statement through the existing
query/execute paths and typed JSON envelope. No new transport is introduced.
Prepared/parse caches remain bounded and contain neither values nor results.

## 10. Implementation order and commit boundaries

1. Commit this plan alone.
2. Characterize Phase 14 and commit AST/parser nodes plus negative grammar.
3. Commit complex RID encoding and target resolution with codec/migration
   compatibility evidence.
4. Commit RETURN/data-clause semantics and complete CREATE/UPDATE/DELETE.
5. Commit atomic multi-row INSERT and UPSERT.
6. Commit the bounded query pipeline in focused SPLIT/GROUP/aggregate,
   projection/OMIT/FETCH/subquery, and order/pagination batches.
7. Commit EXPLAIN FULL/ANALYZE and API/CLI/resource-limit integration.
8. Promote inventory rows only with passing named executable evidence.
9. Run the full gate, write `docs/phase14-report.md`, update durable status,
   and commit `phase 14 complete` with start/end SHAs and rollback range.

No Phase 15 plan or implementation begins before the Phase 14 checkpoint.
Rollback uses explicit Git reverts of the recorded Phase 14 range.

## 11. Verification matrix

At minimum, independently authored tests cover:

- every complex RID type, collision pair, boundary, malformed/noncanonical
  encoding, JSON/public round trip, bind, disk reopen, backup/restore, and old
  fixture unchanged behavior;
- every INSERT/UPSERT input/data/duplicate/return form in memory/on disk,
  single/multi-row, standalone/explicit transactions, with schema, unique,
  graph, FTS, vector, reopen, abrupt exit, and failure injection;
- complete CREATE/UPDATE/DELETE target and data clauses, before/after/value/
  diff returns, snapshot membership, ONLY cardinality, reserved-field
  rejection, transaction poisoning, and node-edge cascade cleanup;
- SELECT clause pipeline cross-products, empty/single/multi groups, aggregate
  type/overflow/ordering behavior, split multiplicity, fetch cycles/dangling
  records, omit/destructure/value shapes, multiple targets, subquery nesting,
  stable ordering, bound pagination, and no source/value leakage;
- every resource limit at below/at/above boundaries, deadline/cancellation,
  output bytes/rows, no panics, no unbounded allocation, and connection reuse;
- direct-AST plan selection for B-tree, graph adjacency, FTS, and exact vector
  paths plus bounded post-filter explanations and reopen assertions;
- deterministic EXPLAIN/FULL/ANALYZE shapes with no physical identifier,
  hidden-column, source, or parameter leakage;
- parser/frontend structured fuzzing and model tests comparing independently
  maintained CRUD/query state in memory and on disk;
- deterministic inventory/COMPAT synchronization, no Phase 14 Partial row,
  fixture hashes, check/rebuild, backup/restore, crash recovery, and all
  unchanged Phase 6–13 behavior; and
- formatting, scoped Clippy, all FastDB suites, release build, retained and
  Phase 14 performance gates, plus unchanged Turso core, PostgreSQL, and
  Whopper suites.

## 12. Stop conditions

Stop the affected capability and write an architecture report rather than
weakening it if implementation would require generated SQLite text, logical
identifier interpolation, arbitrary native plugins, copied SurrealDB
material, an unqualified Turso core change, an unbounded query/mutation stage,
authorization-hostile FETCH/candidate semantics, non-atomic derived state, or
secret/value-bearing diagnostics.

Stop the entire phase if the generalized pipeline cannot preserve explicit
transaction poisoning, stable-WAL acknowledged durability, format-1/2/3
reopen/migration, graph cascade, FTS/vector derived-state atomicity, backup/
check/rebuild, or the public Rust/CLI result contract.

## 13. Definition of done

Phase 14 is complete only when all 50 locked Phase 14 rows are Supported or a
remaining target has an approved narrow architecture stop; no Phase 14 Partial
row remains. Every Supported row names passing parser and execution evidence.
All storage, crash, resource, API/CLI, compatibility, regression, fuzz/model,
and performance gates above pass and exact commands/raw measurements are
recorded in `docs/phase14-report.md`.

Completion is a local pre-1.0 compatibility checkpoint only. It does not
authorize Phase 15, a server, package publication, parallel writers, or a
production-ready/Core 1.0 claim.

## 14. Completion checkpoint

The implementation ended at `6d06d0120`. All 50 locked Phase 14 rows are
resolved: 49 are Supported by named executable evidence and
`STMT-CREATE-COMPLETE` is Unsupported by the approved versioned-history stop
in `docs/phase14-architecture-stops.md`. No Phase 14 Partial row remains.

The complete command log, compatibility evidence, regression counts,
performance measurements, and rollback boundary are preserved in
`docs/phase14-report.md`. Phase 15 must still begin with its own authoritative
plan commit before implementation.
