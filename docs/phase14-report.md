# FastDB Phase 14 report

Status: technically complete locally on 2026-08-14

Starting checkpoint: `d3ccf412e09da97781b3fec36fd74e2da5e55f51`

Authoritative plan commit: `68da72cf9`

Ending implementation checkpoint: `6d06d0120`

Engine pin: `977383ff40edc44ef410af062ed0d2322252a869`

Compatibility reference: SurrealDB `v3.1.5`

Release authority: none

## Outcome

Phase 14 completes the characterized non-graph CRUD and query surface while
retaining format 3, direct Turso-AST lowering, bound values, stable-WAL
serialized writers, and the public Rust/CLI contract. It adds collision-safe
complex record IDs and typed record ranges; INSERT, UPSERT, batch/multi-target
mutations, all supported data and return modes; SELECT VALUE, destructuring,
OMIT, SPLIT, GROUP/aggregates, FETCH, multiple targets, subqueries, ordering,
pagination; and structured EXPLAIN FULL/ANALYZE. Statement timeouts compose
with request deadlines and every multi-record mutation remains atomic.

The locked Phase 14 inventory has 50 rows. Forty-nine are Supported with named
parser and API evidence, zero are Partial, and one is Unsupported:
`STMT-CREATE-COMPLETE`. The stopped `CREATE ... VERSION` clause is versioned
history, which the approved roadmap explicitly excludes; the exact boundary
is recorded in `docs/phase14-architecture-stops.md`. FastDB rejects that syntax
explicitly rather than accepting and ignoring it.

## Correctness and compatibility evidence

Independent black-box probes against the verified unmodified SurrealDB
`v3.1.5` binary characterize complex IDs, ranges, mutation data/return modes,
timeouts, query-pipeline clauses, multiple targets, and the excluded VERSION
form. The binary and checksum provenance plus exact normalized observations
are in `docs/compat-research/phase14.md`.

Fourteen Phase 14 Rust API tests and six parser tests cover the new surface,
including disk reopen, explicit-transaction poisoning, relation adjacency,
atomic target arrays, bound statement timeouts, empty aggregation, and Phase
13 predicates. The full FastDB matrix also passed after updating historical
negative tests to use syntax that remains unsupported and making keyword
tokens contextual where namespaced functions, fields, and table identifiers
permit them. All format/migration, graph, FTS, vector, crash/model, backup,
check/rebuild, CLI, and API regression suites remained green. The seven
committed fixture hashes match and `COMPAT.md` is byte-for-byte reproducible
from the locked inventory.

The unchanged Turso core suite passed 2,286 tests with 17 ignored. The
PostgreSQL suite passed 412 tests. Whopper passed 37 unit tests, 12 regression
tests, and one cross-platform regression test. Scoped Clippy passed with
`-D warnings` and `--no-deps`; only the known inherited Turso unused-import
warning was emitted.

No file under `core/`, `sqlite/`, `postgres/`, inherited `tests/`, Whopper, or
the WAL/JSONB/optimizer implementation changed in the Phase 14 range.

## Regression performance

The optimized retained Phase 5 workload is committed as
`docs/benchmarks/phase14-phase5-regression.json`. It used 5,000 seed records,
200 warmups, and 200 samples on Linux x86-64. The passing run retained every
existing threshold:

| Gate | Ratio | Limit |
| --- | ---: | ---: |
| Point read p50 | 1.42566x | 1.5x |
| Point read p99 | 1.33872x | 2.0x |
| Indexed filter p95 | 1.32536x | 2.0x |
| Write p95 | 1.85368x | 2.0x |
| Checkpointed storage | 1.02263x | 1.5x |

Five preceding identical 200-sample runs failed at least one latency gate.
Investigation found that the generalized query pipeline did unnecessary row
ordering setup for queries without ORDER and retained its richer `QueryRow`
path for simple reads. Commits `96cdd5b65` and `6d06d0120` remove the no-op
ordering work and restore direct projection when SPLIT/GROUP/OMIT/FETCH/ORDER
are absent. No sample count or gate changed. The rebuilt optimized binary then
passed all thresholds above.

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
cargo build --locked --release -p fastdb-cli -p turso_fastdb_benchmarks
target/release/phase5-release-bench --records 5000 --samples 200 \
  --output docs/benchmarks/phase14-phase5-regression.json
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
cargo run -p turso_fastdb_compat -- --check
git diff --check
```

## Remaining boundaries

Phase 14 does not publish Phase 15 scripting/schema/views/events or any later
graph-completion, specialized-index, authentication, server, control-plane, or
SDK surface. Versioned history remains deliberately unavailable. Stable WAL
with one serialized writer remains the only supported concurrency mode.

This is a local pre-1.0 compatibility checkpoint. It does not authorize a tag,
package publication, artifact upload, production-ready claim, parallel
writers, or Core 1.0. Rollback is by explicit revert of the Phase 14 range
starting at `68da72cf9`; completed Phase 0–13 history remains intact.
