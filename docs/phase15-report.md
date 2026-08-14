# FastDB Phase 15 report

Status: technically complete locally on 2026-08-14

Starting checkpoint: `a027a777d`

Authoritative plan commit: `ffb9c1573`

Ending implementation checkpoint: `9b69ad1fb`

Engine pin: `977383ff40edc44ef410af062ed0d2322252a869`

Compatibility reference: SurrealDB `v3.1.5`

Release authority: none

## Outcome

Phase 15 completes the characterized scripting, custom function and database
parameter, table and field schema, materialized view, synchronous event, and
matching ALTER/REMOVE/INFO lifecycle surface while retaining format 3, direct
Turso-AST lowering, bound values, stable-WAL serialized writers, and the
existing Rust/CLI contract. Scripts have lexical scope and bounded control
flow; custom functions share caller budgets and transaction state; schema
expressions use one normalization path; views and events update atomically
with their source documents; and every catalog definition is validated on
publication and reopen.

The locked Phase 15 inventory contains 42 rows. Twenty-nine are Supported
with named parser and API evidence, zero are Partial, and thirteen are
Unsupported with the narrow stops recorded in
`docs/phase15-architecture-stops.md`. The stops cover four non-transactional
sequence lifecycle capabilities, one Surrealism module capability, three
server-owned API lifecycle capabilities, four external bucket lifecycle
capabilities, and the mixed NORMAL/RELATION `TYPE ANY` table representation.
FastDB rejects each stopped form before catalog, document, or filesystem
mutation.

Across the complete locked inventory, 532 of 756 capabilities are Supported,
zero are Partial, and 224 are Unsupported. Unsupported rows include the
roadmap exclusions and later-phase capabilities; their status is not treated
as implementation evidence.

## Correctness and compatibility evidence

Independent black-box probes against the verified unmodified SurrealDB
`v3.1.5` binary characterize script scope/control flow, parameters, custom
functions, field rules, views, events, schema lifecycle, sequence rollback,
modules, buckets, APIs, and mixed table kinds. The binary and checksum
provenance plus exact normalized observations are in
`docs/compat-research/phase15.md`.

Thirty Phase 15 Rust API tests, thirteen parser tests, nine catalog tests, and
four catalog reopen/integrity tests cover the new surface, including explicit
transactions, poisoning, dependency validation, recursion and resource
limits, event ordering and rollback, view rebuild/reopen, derived graph/FTS/
vector state, and complete removal lifecycles. The full FastDB matrix passed,
including the format/migration, graph, FTS, vector, crash/model, backup,
check/rebuild, CLI, and API regression suites. The seven committed fixture
hashes match and `COMPAT.md` is byte-for-byte reproducible from the locked
inventory.

The parser fuzz gate found that its AST validator still modeled pre-Phase 14
CREATE/SELECT/UPDATE shapes and omitted Phase 15 statements and expressions.
The validator now walks all current structured nodes. Its instrumented nightly
build passed and a 30-second libFuzzer run completed without a crash. The
structured CRUD target was also rebuilt from its refreshed standalone lockfile
and completed a 30-second run without a crash.

The unchanged Turso core suite passed 2,286 tests with 17 ignored. The
PostgreSQL suite passed 412 tests. Whopper passed 37 unit tests, 12 regression
tests, and one cross-platform regression test. Scoped Clippy passed with
`-D warnings` and `--no-deps`; only the known inherited Turso unused-import
warning was emitted.

No file under `core/`, `sqlite/`, `postgres/`, inherited `tests/`, Whopper, or
the WAL/JSONB/optimizer implementation changed in the Phase 15 range.

## Resource and memory evidence

The release memory harness ran stable WAL with the `read-heavy` workload, 20
iterations, batches of 100, one connection, and a final checkpoint. RSS was
6,873,088 bytes at baseline, peaked at 21,618,688 bytes, and ended at
17,424,384 bytes. DHAT reported 310,310 peak live heap bytes and 47,540 final
live heap bytes across 198,317,918 allocated bytes. The checkpoint left a
274,432-byte database and no WAL/log sidecar. Phase 15 API tests separately
exercise script iteration, recursion, statement, output, and deadline limits;
this raw harness confirms that the inherited stable-WAL checkpoint path did
not retain an unbounded provider sidecar.

## Regression performance

The retained exact passing Phase 5 workload is committed as
`docs/benchmarks/phase15-phase5-regression-rerun3.json`. It used 5,000 seed
records, 200 warmups, and 200 samples on Linux x86-64. Every existing threshold
was retained:

| Gate | Ratio | Limit |
| --- | ---: | ---: |
| Point read p50 | 1.37393x | 1.5x |
| Point read p99 | 0.74813x | 2.0x |
| Indexed filter p95 | 0.71209x | 2.0x |
| Write p95 | 1.03054x | 2.0x |
| Checkpointed storage | 1.02263x | 1.5x |

Several precursor and otherwise identical runs were scheduler-sensitive and
failed at least one latency gate. Investigation found avoidable retained-path
work: catalog reads cloned the complete snapshot, ordinary statements scanned
function/event/view definitions even when those catalogs were empty, and
simple schema validation took the generalized Phase 15 expression path.
Commit `beff069f3` restores shared catalog snapshots and the corresponding
empty-provider/simple-statement fast paths. No sample count or threshold
changed. The optimized binary was rebuilt from that exact code before the
passing run above.

## Commands

```text
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy -p turso_fastdb_compat -p turso_fastdb_parser -p turso_fastdb \
  -p fastdb -p fastdb-cli -p turso_fastdb_tests -p turso_fastdb_benchmarks \
  --all-targets --no-deps -- -D warnings
cargo test -p turso_fastdb_parser -p turso_fastdb -p fastdb -p fastdb-cli \
  -p turso_fastdb_compat -p turso_fastdb_tests
cargo test -p turso_core --lib
cargo test -p turso_pg_tests
cargo test -p turso_whopper
cargo +nightly check --manifest-path fastdb-parser/fuzz/Cargo.toml --bin parse
cargo +nightly fuzz run parse --fuzz-dir fastdb-parser/fuzz -- \
  -max_total_time=30 -print_final_stats=1
cargo +nightly fuzz run structured_crud --fuzz-dir fastdb-tests/fuzz -- \
  -max_total_time=30 -print_final_stats=1
cargo build --locked --release -p fastdb-cli -p turso_fastdb_benchmarks
target/release/phase5-release-bench --records 5000 --warmups 200 \
  --samples 200 \
  --output docs/benchmarks/phase15-phase5-regression-rerun3.json
./target/release/memory-benchmark --mode wal --workload read-heavy \
  -i 20 -b 100 --connections 1 --checkpoint --format json
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
cargo run -p turso_fastdb_compat -- --check
git diff --check
```

## Remaining boundaries

Phase 15 does not publish Phase 16 graph completion or any later specialized-
index, authentication, server, control-plane, or SDK surface. Versioned
history, Realtime/LIVE queries, geospatial behavior, GraphQL/GQL, multiprocess
access, and parallel writers remain deliberately unavailable. Stable WAL with
one serialized writer remains the only supported concurrency mode.

This is a local pre-1.0 compatibility checkpoint. It does not authorize a tag,
package publication, artifact upload, production-ready claim, parallel
writers, or Core 1.0. Rollback is by explicit revert of the Phase 15 range
starting at `ffb9c1573`; completed Phase 0–14 history remains intact.
