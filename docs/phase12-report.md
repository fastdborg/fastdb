# FastDB Phase 12 report

Status: technically complete locally on 2026-08-13

Starting roadmap checkpoint: `aec1da0aa`

Authoritative plan commit: `d5681a557`

Ending implementation checkpoint: `6d193d816`

Engine pin: `977383ff40edc44ef410af062ed0d2322252a869`

Compatibility reference: SurrealDB `v3.1.5`

Release authority: none

## Outcome

Phase 12 starts the serialized-writer pre-1.0 compatibility track without
reopening the stopped Phase 11 MVCC gate. It locks 756 atomic capabilities in
`compat/surrealdb-v3.1.5.toml` and mechanically renders `COMPAT.md`. The
checkpoint contains 89 Supported and 667 Unsupported entries, with all 24
Phase 12 targets Supported and no Partial entry. The checker rejects stale
Markdown, unknown fields, changed locked IDs, invalid phase/disposition/status
combinations, and missing evidence.

Format 3 is now the current on-disk format. Bootstrap creates it directly;
format 1 and format 2 migrate to it in one immediate transaction, with source
validation inside that transaction and the format header published last. The
migration retains documents and every B-tree, graph, FTS, and vector physical
object. It adds seven sealed catalogs for functions, parameters, views, events,
permissions, users, and accesses plus versioned provider auxiliary metadata.
All injected format-3 boundaries restore the format-2 fixture byte-for-byte,
and a clean format-3 reopen is byte-stable.

The value contract now has collision-safe representations for NONE, bytes,
datetime, duration, decimals, UUIDs, sets, ranges, regexes, table values, file
references, and typed arrays/sets. Documents and public JSON use envelope
version 2 while losslessly readable version-1 record/object envelopes remain
accepted. Decode limits cover nesting, scalar bytes, arrays, and collections;
malformed, noncanonical, oversized, duplicate-set, unknown-version, and
unknown-kind representations fail closed. Documents remain authoritative and
are rewritten to v2 only by an ordinary successful mutation.

No inherited Turso source, engine pin, concurrency mode, server surface, tag,
package publication, upload, or production-ready claim changed.

## Clean-room reference provenance

The official unmodified Linux x86-64 binary was installed outside the
repository at `/home/tan/.cache/fastdb-reference/surreal-v3.1.5/surreal`.
It reports `3.1.5 for linux on x86_64`; the downloaded archive SHA-256 is
`f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
Independent probes, exact inputs, normalized observations, public links, and
the 2026-08-13 observation date are recorded in
`docs/compat-research/phase12.md`. No SurrealDB source, test, fixture, expected
output, or corpus was inspected or copied.

## Storage and functional evidence

Independent tests prove in-memory and on-disk bound-value round trips,
schema-enforced typed collections, close/reopen, backup/restore equality,
format-1/2 migration, format-3 idempotent reopen, byte-exact failpoint rollback,
sealed-catalog and version refusal without mutation, a committed format-3
fixture, engine integrity, and retained index selection. Existing crash tests
also exercise the format-1 migration through format 3 before an abrupt process
exit and prove the recovered index plan.

The committed format-3 fixture is
`fastdb-tests/fixtures/phase12-format3.fastdb`, SHA-256
`6ef2751296c551ba236dce0f2f5ad42913506cca42d36b311bce388d055057b6`.
All seven historical/current fixture hashes passed.

The full FastDB integration package passed, including catalog/transaction
failure injection, model tests, abrupt-exit recovery, backup/check/restore,
graph adjacency and cascade, FTS transaction/churn/corruption cases, vector
document/BLOB agreement, resource limits, API, and CLI compatibility.

## Regression and performance evidence

The unchanged Turso core library suite passed 2,286 tests with 17 upstream
ignored tests and no failures. The PostgreSQL suite passed 412 tests. Whopper
passed 37 unit tests, 12 regression tests, and one cross-platform regression
test. Existing inherited warnings remained limited to Turso sources; all
changed FastDB targets passed scoped Clippy with `-D warnings` and `--no-deps`.

The unchanged Phase 5 optimized workload is committed as
`docs/benchmarks/phase12-phase5-regression.json`. It used 5,000 seed records,
200 warmups, and 200 samples on Linux x86-64 and passed every retained gate:

| Gate | Ratio | Limit |
| --- | ---: | ---: |
| Point read p50 | 1.49379x | 1.5x |
| Point read p99 | 0.83022x | 2.0x |
| Indexed filter p95 | 1.01655x | 2.0x |
| Write p95 | 1.22787x | 2.0x |
| Checkpointed storage | 1.02263x | 1.5x |

## Commands

```text
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy -p turso_fastdb_compat -p turso_fastdb_parser -p turso_fastdb \
  -p fastdb -p fastdb-cli -p turso_fastdb_tests -p turso_fastdb_benchmarks \
  --all-targets --no-deps -- -D warnings
cargo test -p turso_fastdb_compat
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p fastdb
cargo test -p fastdb-cli
cargo test -p turso_fastdb_tests
cargo test -p turso_core --lib
cargo test -p turso_pg_tests
cargo test -p turso_whopper
cargo build --locked --release -p fastdb-cli -p turso_fastdb_benchmarks
target/release/phase5-release-bench --records 5000 --samples 200 \
  --output docs/benchmarks/phase12-phase5-regression.json
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
git diff --check
```

## Remaining boundaries

Phase 12 does not implement Phase 13 operators/functions or any later roadmap
surface. New typed values are deliberately not promoted to ordinary indexable
scalars until their exact index representation and comparison semantics are
implemented and proven. File values are opaque values; no resource is fetched.
The inventory still has 667 Unsupported entries assigned to later phases or
explicit exclusions, and status promotion remains evidence-gated.

Serialized stable WAL remains the only supported writer mode. This checkpoint
is not a pre-1.0 package release and does not authorize a production-ready or
Core 1.0 claim. Rollback is by explicit revert of the Phase 12 commit range
starting at `d5681a557`; completed Phase 0–11 history is preserved.
