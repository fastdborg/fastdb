# FastDB Phase 7 report

Status: technically complete locally on 2026-08-13; Phase 8 implementation has
not started

Phase 7 adds the characterized SurrealDB `v3.1.5` relation-record and bounded
graph-traversal subset without changing FastDB format 2 or the pinned Turso
engine. This report is local engineering evidence. It is not an alpha release
authorization or a claim that FastDB Core is production-ready.

## Engine and clean-room decision

- Retained engine SHA: `977383ff40edc44ef410af062ed0d2322252a869`.
- No upstream fetch, merge, cherry-pick, pin update, or inherited Turso source
  edit was required in Phase 7.
- The immutable reference executable is the official SurrealDB `v3.1.5` Linux
  x86-64 binary, installed outside the repository under the user cache. Its
  executable SHA-256 is
  `dd9b1395baa8b6af64eb97b85887490a3ad1882aeb910277e0489b072d6e2f9f`.
- Independently authored black-box inputs, normalized observations, source
  links, tool hashes, and compatibility decisions are recorded in
  `docs/compat-research/phase7.md`. No SurrealDB source, tests, fixtures,
  expected outputs, or fuzz corpus were inspected or copied.

## Graph catalog and physical model

Format 2 now uses its previously reserved closed catalog values for relation
tables and the built-in graph provider. A relation table owns four opaque,
catalog-managed `TEXT NOT NULL` columns containing the immutable input/output
table IDs and encoded record IDs. It also owns exactly two provider indexes:
forward `(in_table, in_rid)` and reverse `(out_table, out_rid)` adjacency.

Catalog reopen validation proves the capability, table kind, endpoint table
ownership, hidden-column roles and encodings, provider/version/options/state,
index direction and order, and exact physical table/index definitions before a
catalog snapshot can be published. Missing, orphaned, duplicated, or altered
graph metadata fails closed. The public document stores only edge user content;
typed `id`, `in`, and `out` record IDs are synthesized during decoding.

Provider-owned graph indexes cannot be removed or rebuilt through ordinary
public B-tree maintenance, and relation endpoint fields cannot be redefined or
mutated as user fields. No public plugin ABI, dynamic code loading, generated
user SQL, or logical-name interpolation was introduced.

## Executable language surface

The independent parser and frontend implement:

- `DEFINE TABLE ... TYPE RELATION [IN|FROM table] [OUT|TO table]
  [ENFORCED]`, with `FROM`/`TO` normalized to `IN`/`OUT`;
- `RELATE [ONLY] record->relation->record [CONTENT object|SET assignments]
  [RETURN NONE|BEFORE|AFTER]` using literal or bound record-ID endpoints;
- UUIDv7 edge identifiers and atomic auto-registration of absent schemaless
  endpoint/relation catalogs;
- dangling edges by default and source-first then target existence checks for
  `ENFORCED` relations;
- chained, fixed-depth `->`, `<-`, and `<->` traversal in aliased `SELECT`
  projections, with endpoint-ID results or final `.*` document materialization;
- duplicate-preserving bidirectional traversal, bounded to eight source hops
  and a 10,000-record intermediate frontier;
- atomic deletion of every connected relation edge when deleting a normal
  record, including self-loop deduplication;
- direct relation reads, updates, and deletes with synthesized endpoint values;
  direct `CREATE` into a relation table and updates to `id`/`in`/`out` fail
  explicitly.

Arrays/cartesian RELATE targets, explicit complex edge IDs, `OR UPDATE`, edge
path filters, recursive paths, standalone traversal expressions, arbitrary
RELATE return projections, and relation timeout clauses remain explicitly
rejected. `COMPAT.md` marks only the executable subset Partial.

## Atomicity, recovery, and corruption evidence

Failure injection covers graph hidden-column catalog publication, forward and
reverse index publication, edge insertion, ordinary catalog/physical DDL
publication, and node/cascade deletion. Every injected error rolls back the
catalog, physical objects, document, typed endpoints, adjacency state, and node
mutation together.

Disk tests cover explicit rollback, reopen, abrupt process exit after graph
write, abrupt exit after node cascade, concurrent schemaless relation
registration, schemafull edge validation, dangling endpoints, immutable
endpoints, multigraph duplicates, self-loops, and both traversal directions.
The committed graph fixture is:

```text
fastdb-tests/fixtures/phase7-format2-graph.fastdb
00f33f003db352981becf501519263cc7ad757916b5a601d69defb69cb799e54
```

It reopens, accepts further mutations, selects both adjacency indexes, and
passes engine integrity checking. Five independently induced catalog/physical
corruption classes fail during open instead of returning partial graph data.

## Public boundary and planning evidence

Parser, synchronous frontend, asynchronous Rust API, and strict CLI JSON tests
agree on typed relation values and traversal results. Structured `EXPLAIN`
tests prove forward and reverse traversal select the opaque adjacency indexes;
the equivalent indexed lookup builder is used by cascade discovery.

The graph model fuzzer compares real FastDB node/edge mutations, dangling-edge
creation, traversals, and cascades with an independent in-memory multigraph.
The existing structured CRUD model was rerun to protect earlier document and
transaction semantics.

## Verification evidence

The authoritative local matrix completed on x86-64 Linux. Warnings shown by
inherited Turso crates were unchanged; the FastDB clippy matrix passed.

| Command or gate | Result |
| --- | --- |
| `cargo metadata --locked --no-deps --format-version 1` | Passed. |
| all three required formatting checks; `git diff --check` | Passed. |
| FastDB package clippy matrix, all targets | Passed. |
| parser package and compatibility checker | Passed, including 4 Phase 7 parser tests. |
| frontend package | Passed: 15 tests. |
| async Rust API, all targets, and doc tests | Passed, including Phase 7 public-boundary coverage. |
| CLI all targets | Passed, including strict Phase 7 JSON coverage. |
| FastDB integration package | Passed, including 9 Phase 7 graph tests. |
| parser fuzz, `-max_total_time=300` | 4,490,614 runs in 301 seconds; no failure; peak RSS 715 MB. |
| structured CRUD fuzz, `-max_total_time=300` | 2,669 runs in 301 seconds; no failure; peak RSS 553 MB. |
| structured graph fuzz, `-max_total_time=300` | 3,856 runs in 301 seconds; no failure; 164 new units; peak RSS 548 MB. |
| `turso_core --lib` | Passed: 2,286; 17 ignored. |
| core expression-index filter | Passed: 3. |
| core stable-WAL/no-MVCC filter | Passed: 5. |
| core transaction-visibility filter | Passed: 1. |
| inherited PostgreSQL suite | Passed: 412. |
| inherited Whopper package suites | Passed. |
| fixture SHA-256 verification | Passed for all four committed fixtures. |
| release CLI and benchmark package builds | Passed. |

## Performance evidence

Raw graph samples are in `docs/benchmarks/phase7-graph.json`. The run used
1,000 edges and 100 measured traversals over the same logical graph. Native
Turso used the equivalent physical table, typed endpoint columns, and two
adjacency indexes.

| Metric | FastDB | Native | Ratio | Provisional limit | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| traversal p95 | 184,059 ns | 148,094 ns | 1.2429x | 2.0x | Passed |
| checkpointed main-file storage | 589,824 B | 430,080 B | 1.3714x | 2.0x | Passed |

The graph ratio is Phase 7 evidence toward the Phase 12 gate, not a general
performance claim.

The unchanged Phase 5 benchmark was rerun after the graph implementation. Raw
samples are in `docs/benchmarks/phase7-phase5-regression.json`; its
machine-readable aggregate gate is `true`.

| Phase 5 regression gate | Ratio | Limit | Result |
| --- | ---: | ---: | --- |
| point read p50 | 1.1053x | 1.5x | Passed |
| point read p99 | 1.2096x | 2.0x | Passed |
| indexed filter p95 | 1.3724x | 2.0x | Passed |
| write p95 | 1.5331x | 2.0x | Passed |
| checkpointed main-file storage | 1.0059x | 1.5x | Passed |

## Provenance and phase decision

Implementation changes are confined to FastDB-authored crates, tests, fuzz
targets/corpus, fixtures, benchmarks, plans, compatibility/format/release
documentation, and the FastDB crash helper. No inherited Turso core, SQLite
parser, PostgreSQL frontend, binding, WAL, optimizer, JSONB, or inherited test
source changed.

Every Phase 7 definition-of-done gate passes locally. No stop condition was
encountered: edge documents and hidden endpoints remain atomic, both adjacency
directions use indexes, catalog inconsistencies fail closed, graph expansion is
bounded, opaque names remain enforced, and the retained pin required no change.

Core remains pre-alpha and is not production-ready. Phase 8 requires a new
authoritative plan before FTS implementation; Phases 8 through 12 and separate
release authorization remain required.
