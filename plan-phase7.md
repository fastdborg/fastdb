# FastDB Phase 7 — Graph Records and Bounded Traversal

Status: authoritative implementation plan

## 1. Objective and scope

Phase 7 adds a clean-room, characterized SurrealDB `v3.1.5` graph subset to
the completed format-2 foundation. It introduces typed relation tables,
single-edge `RELATE`, mandatory two-way adjacency indexes, fixed-depth
traversal projections, enforced endpoint validation, and atomic node cascade
deletion through the embedded Rust API and CLI.

The retained Turso engine SHA remains
`977383ff40edc44ef410af062ed0d2322252a869`. Phase 7 does not fetch, merge,
cherry-pick, or alter the engine pin and must not edit inherited Turso code.
Graph storage uses stable ordinary tables, hidden typed columns, B-tree
indexes, transactions, JSONB, and directly constructed translated AST.

The independent observations in `docs/compat-research/phase7.md` are the
behavioral contract. Current online documentation may inform research but does
not change the immutable `v3.1.5` reference.

## 2. Explicit compatibility boundary

Phase 7 supports:

```surql
DEFINE TABLE edge [SCHEMAFULL | SCHEMALESS]
  TYPE RELATION
  [IN source | FROM source]
  [OUT target | TO target]
  [ENFORCED];

RELATE [ONLY]
  source_record -> edge_table -> target_record
  [CONTENT object | SET field = value [, ...]]
  [RETURN NONE | RETURN BEFORE | RETURN AFTER];

SELECT ->edge->target [->edge->target ...] [.*] AS alias FROM ...;
SELECT <-edge<-source [<-edge<-source ...] [.*] AS alias FROM ...;
SELECT <->edge<->target [<->edge<->target ...] [.*] AS alias FROM ...;
```

`RELATE` endpoints may be record literals or parameters whose bound values are
typed record IDs. A source parameter, middle table identifier, and target
parameter remain three structurally distinct AST fields.

This phase rejects with a structured span before mutation:

- arrays, subqueries, ranges, and cartesian endpoint values;
- explicit or complex edge IDs and `OR UPDATE`;
- `TIMEOUT`, `RETURN DIFF`, `RETURN VALUE`, and arbitrary return projections;
- endpoint table unions, path filters, edge-field filters, wildcard edge
  tables, recursive idioms, and standalone traversal expressions;
- traversals outside SELECT projections or without an alias;
- relation records as endpoints, direct ordinary CREATE into relation tables,
  and use of a normal table as the middle of `RELATE`;
- storing or assigning top-level `id`, `in`, or `out` as user document fields.

`COMPAT.md` remains Unsupported until behavior executes through a public
boundary and its named evidence passes. Each promoted row is at most Partial;
Phase 7 does not claim complete graph compatibility.

## 3. Independent AST and parser contract

Add structural table-kind syntax to `DefineTableStatement`:

- `TableKind::Normal` for explicit `TYPE NORMAL` and the existing default;
- `TableKind::Relation(RelationType)` with optional spanned input/output table
  identifiers and an `enforced` span;
- `IN`/`FROM` and `OUT`/`TO` normalize to the same AST fields while source
  spelling remains in statement spans/definitions.

Add `RelateStatement` with:

- `only`, source endpoint expression, relation table identifier, target
  endpoint expression, `CreateData`, optional `ReturnClause`, and full span;
- endpoint expressions restricted during parsing to record literals or
  parameters; evaluation still validates parameter runtime type;
- no representation for deferred clauses.

Add `Expr::Traversal(TraversalExpr)` containing one or more `TraversalHop`
values. Each hop has one direction (`Forward`, `Reverse`, or `Bidirectional`),
one relation table identifier, and one endpoint table identifier. A final
materialization flag represents `.*`; edge materialization is not accepted.

Lexer recognition of `->`, `<-`, and `<->` occurs before their component
comparison/arithmetic tokens. Parser nesting, token, identifier, statement,
and input limits continue to apply. Add a fixed Phase 7 maximum of eight graph
hops and a maximum runtime frontier of 10,000 endpoint occurrences; Phase 10
will expose configurable lower bounds through `ResourceLimits`.

## 4. Catalog and hidden storage contract

Format version remains 2. No migration or public format bump occurs. Existing
format-2 normal tables and B-tree indexes reopen byte-for-byte unchanged.

Extend closed catalog values:

- table kind `RELATION`;
- index kind `GRAPH_ADJACENCY`;
- provider/capability `BUILTIN_GRAPH`, provider version 1, encoding version 1;
- hidden physical encodings `GRAPH_TABLE_ID` and `GRAPH_RID`;
- canonical hidden-column role options:
  `{"role":"in_table"}`, `{"role":"in_rid"}`,
  `{"role":"out_table"}`, and `{"role":"out_rid"}`;
- canonical adjacency index options:
  `{"direction":"forward"}` and `{"direction":"reverse"}`.

Every relation table owns exactly four hidden NOT NULL TEXT columns and two
non-unique ordinary B-tree indexes:

```text
forward: (in_table_id, in_rid, out_table_id, out_rid)
reverse: (out_table_id, out_rid, in_table_id, in_rid)
```

Names are deterministic opaque names derived from immutable catalog IDs.
Logical names and values never enter physical identifiers or generated SQL.
The hidden-column catalog owns the four columns; the index catalog owns both
adjacency indexes. Reserved internal logical index names cannot be addressed
by `REMOVE INDEX` or `REBUILD INDEX`; graph integrity/rebuild becomes an
operational provider action in Phase 10.

Relation metadata stores optional immutable endpoint table catalog IDs and the
`ENFORCED` flag. Defining constraints or relating dangling endpoints atomically
registers absent endpoint logical tables as empty NORMAL SCHEMALESS catalogs so
every stored endpoint always has a stable table ID. No endpoint record is
created by registration.

Catalog loading validates exact ownership, role cardinality, provider/version,
encoding, canonical options, physical column schema, endpoint table existence,
normal endpoint kinds, and both exact adjacency index definitions before
publishing a snapshot. Missing, duplicate, orphaned, or incompatible graph
storage is a format error; FastDB never returns incomplete graph results.

## 5. Relation definition and mutation

`DEFINE TABLE ... TYPE RELATION` uses the database schema mutex and one
transaction. It registers absent constrained endpoint tables, persists the
relation catalog, creates the physical relation table with all hidden columns,
creates both adjacency indexes, persists their ownership metadata and the
`BUILTIN_GRAPH` capability requirement, validates the candidate snapshot, and
publishes only after commit.

Redefinition of any existing table remains a logical constraint error. Phase 7
does not alter normal tables into relations or relations into normal tables.

`RELATE` resolves or atomically creates its middle table as RELATION
SCHEMALESS. It also registers absent endpoint tables as NORMAL SCHEMALESS. For
an existing relation it checks optional input/output table IDs. If `ENFORCED`,
it checks source record existence first and target existence second within the
same transaction before inserting the edge.

Each edge receives a UUIDv7 record ID. Its JSONB `doc` contains user content
only. The four hidden columns contain endpoint table IDs and the existing
canonical encoded RIDs. The single translated INSERT binds RID, document, and
all endpoint values, so document and adjacency state cannot diverge.

Schemafull validation applies to edge user content exactly as it does to
normal documents. Synthesized `id`, `in`, and `out` are authorized independently
of user field definitions and can never be declared or overwritten. Ordinary
user B-tree indexes on relation document fields remain supported.

Decoding an edge synthesizes typed `id`, `in`, and `out`. Direct SELECT,
UPDATE, and DELETE on a relation table preserve this shape. UPDATE may change
only user content; endpoint hidden columns are immutable.

Standalone statements are atomic. Explicit transaction errors poison and roll
back the complete transaction under the existing contract. Add failpoints
after relation catalog ownership, physical table creation, each adjacency
index, edge insertion, and each cascade direction.

## 6. Traversal execution

Traversal begins from each SELECT candidate's typed record ID. For every hop:

1. resolve the relation and expected endpoint table through the catalog;
2. issue a bound equality query using the forward, reverse, or both adjacency
   indexes;
3. project the opposite endpoint table ID/RID while preserving one occurrence
   per matching edge and per direction;
4. verify the requested endpoint table name matches the stored catalog ID;
5. stop with a resource error if the 10,000-occurrence frontier would be
   exceeded.

Forward/reverse queries are separate for bidirectional hops, preserving
duplicates exactly as separate walks. No implicit deduplication occurs.

Without `.*`, the final projection returns typed endpoint record IDs even when
target documents are absent, matching characterized dangling behavior. With
`.*`, absent endpoint documents are omitted and existing documents are decoded
with synthesized fields. Materialization of relation endpoints is outside the
phase because relation records cannot be graph endpoints.

Traversal arrays have no compatibility ordering guarantee without a supported
ordering surface. Tests compare ordered values only when the storage plan
provides a deterministic single result; otherwise they compare multisets.

`EXPLAIN` for a SELECT containing traversal returns the ordinary base SELECT
plan plus structured traversal plan rows naming only opaque relation/index
objects and direction. Test-only plan helpers assert that equality queries use
the mandatory forward and reverse indexes and contain no relation-table scan.

## 7. Atomic node cascade

Deleting normal records discovers all relation tables from the immutable
catalog snapshot. For each deleted record and relation table it executes two
bound DELETE statements:

- forward equality on `(in_table_id, in_rid)`;
- reverse equality on `(out_table_id, out_rid)`.

Both are execution-plan tested against their corresponding adjacency index.
Edge cleanup and node deletion occur in the same standalone or explicit
transaction. Failure before, between, or after cascade directions rolls back
all edge and node mutations. Deleting an edge directly does not recurse.

The candidate node set is fixed before mutation under existing DELETE snapshot
semantics. Cascade work is bounded by the number of relation tables and deleted
records; Phase 10 adds user-configurable statement resource limits.

## 8. Verification matrix

Add independently authored parser, frontend, async API, CLI, model, failure,
reopen, crash, and corruption tests with stable `p7_*` evidence IDs.

Required coverage includes:

- exact ASTs and precise rejection spans for every supported/deferred form;
- relation definition synonyms, constraints, duplicate definitions, absent
  endpoint registration, schemafull fields, and normal/relation kind errors;
- literal and bound endpoints, UUIDv7 edge IDs, `ONLY`, `CONTENT`, `SET`, and
  supported return shapes;
- dangling endpoints, enforced source-first/target-second checks, mismatched
  endpoint table types, and transaction poisoning;
- synthesized and immutable `id`/`in`/`out` through sync frontend, async Rust
  API, CLI JSON, reopen, and direct relation CRUD;
- forward, reverse, bidirectional, fixed-depth, duplicate-preserving, dangling
  ID, and `.*` materialization traversal;
- adjacency plan selection in both directions before/after reopen;
- node cascade across multiple relation tables and both endpoint roles;
- failure injection at every relation publication, edge insert, and cascade
  boundary, plus abrupt exit and integrity checks;
- catalog corruption for missing/orphaned/duplicate hidden columns, endpoint
  ownership, index direction/options/version/encoding/state, physical columns,
  and adjacency indexes;
- model tests comparing random graph mutations/traversals/cascades with an
  independent in-memory multigraph;
- parser and structured graph fuzz targets with independent seeds;
- graph p95 latency/storage against an equivalent native Turso physical schema,
  with a provisional 2x ceiling preserved for the Phase 12 gate;
- all unchanged Phase 6 and Phase 5 gates.

At minimum run:

```sh
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy --locked -p turso_fastdb_parser -p turso_fastdb -p fastdb \
  -p fastdb-cli -p turso_fastdb_tests --all-targets
cargo test --locked -p turso_fastdb_parser
cargo test --locked -p turso_fastdb
cargo test --locked -p fastdb --all-targets
cargo test --locked -p fastdb-cli --all-targets
cargo test --locked -p turso_fastdb_tests
cargo test --locked --doc -p fastdb
(cd fastdb-parser && cargo +nightly fuzz run parse -- -max_total_time=300)
(cd fastdb-tests/fuzz && cargo +nightly fuzz run structured_crud -- -max_total_time=300)
cargo test --locked -p turso_core --lib
cargo test --locked -p core_tester --test integration_tests expression_index
cargo test --locked -p core_tester --test integration_tests without_mvcc
cargo test --locked -p core_tester --test integration_tests test_transaction_visibility
cargo test --locked -p turso_pg_tests
cargo test --locked -p turso_whopper
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
cargo build --locked --release -p fastdb-cli
cargo build --locked --release -p turso_fastdb_benchmarks
git diff --check
```

Preserve exact graph benchmark commands/raw samples and rerun the unchanged
Phase 5 release benchmark. Update `COMPAT.md`, `docs/format-v2.md`, fixtures,
release readiness, and `docs/phase7-report.md` only as evidence becomes real.

## 9. Definition of Done and stop conditions

Phase 7 is complete only when relation catalogs and physical ownership validate
on reopen, every edge write is document/endpoint atomic, both adjacency
directions prove index selection, traversal matches the characterized subset,
cascade deletion is crash/failure safe, all public boundaries agree, graph
model/fuzz/benchmark gates pass, and every earlier local gate remains green.

Stop before Phase 8 if graph correctness requires a Turso core/pin change;
hidden endpoint state can diverge from documents; any traversal or cascade plan
scans a relation table; a catalog inconsistency can yield partial results;
graph expansion is unbounded; a logical identifier reaches physical SQL; or
the clean-room `v3.1.5` evidence contradicts the implemented public contract.
