# FastDB Phase 11 MVCC audit

Decision: **reject and stop**
Audit date: 2026-08-13
Retained engine pin: `977383ff40edc44ef410af062ed0d2322252a869`
Fetched release: `v0.8.0-pre.4` at `ce276e4218d631ab7b2e15bfa277f1509be1cf82`
Fetched upstream `main`: `069b5431e86779d70df3940711bb61f8601db069`

## Outcome

No exact candidate qualifies for FastDB Core 1.0 parallel writers. No upstream
commit was merged or cherry-picked, no pin record changed, and no
`ConcurrencyMode`, conflict API, format, or engine behavior was added.
Serialized stable WAL remains the only supported mode.

The retained source is more advanced than the old short MVCC guide implied: it
contains logical-log recovery, interrupted-checkpoint reconciliation,
checkpoint-time and inline garbage collection, conflict variants, Hermitage
tests, memory workloads, and extensive deterministic failure tests. Those are
positive engineering signals, not production qualification.

## Exact-source findings

| Gate | Evidence | Result |
| --- | --- | --- |
| Stable status | The current upstream manual still says MVCC is experimental and not production-ready, and warns features may panic or return incorrect results. `v0.8.0-pre.4` is a prerelease. | Fail |
| Snapshot isolation | The implementation has snapshot/conflict tests and `WriteWriteConflict`, but `core/mvcc/mod.rs` still lists phantom/read-skew/write-skew TODOs. A cursor/B-tree MVCC model test is ignored because it fails constantly. | Fail |
| Concurrent writes | A basic non-overlapping concurrent-insert test remains ignored for an unresolved write-busy defect; overlapping writes are documented as sporadic; the large concurrent-writes stress test is ignored. | Fail |
| Recovery | Logical-log recovery, durable replay boundaries, torn-tail handling, interrupted checkpoint reconciliation, and many failure tests exist. One first-bootstrap torn-header case still lacks its required harness and remains ignored. | Partial, not disqualifying alone |
| GC and memory | GC and thresholds exist. Local update-churn results did not show heap-at-exit growth across the sampled scale, but the manual still documents eager loading and high memory use, and no sustained FastDB long-reader bound was proven. | Insufficient |
| Checkpoint | The supported truncate path is stop-the-world and blocks readers and writers. Passive checkpointing is explicitly experimental; source documentation records snapshot-safety constraints and unresolved soak needs. | Fail |
| Correct record IDs | The AUTOINCREMENT/update-rowid test is ignored because the MVCC allocator does not track rowid changes from UPDATE. | Fail |
| Conflict surface | Engine variants exist, but candidate qualification fails before FastDB can freeze `ErrorCategory::Conflict`. No user transaction replay was added. | Not reached |

The upstream range from the retained pin to current `main` changes only a small
part of the focused MVCC surface (`core/mvcc/database/mod.rs` and its tests have
minor edits). It does not remove the experimental status, blocking checkpoint,
or ignored correctness tests. Broader changes in `core/database.rs` and stable
WAL do not constitute a qualified MVCC release.

## Local evidence

The unchanged pinned core regression passed during Phase 10: 2,286 tests
passed and 17 were ignored. PostgreSQL (412 tests) and Whopper unit/regression
suites also passed. This proves the retained serialized baseline, not MVCC GA.

The most direct ignored test was run manually once:

```text
cargo test -p turso_core --lib \
  mvcc::tests::test_non_overlapping_concurrent_inserts -- --ignored --nocapture
```

It passed once (1 passed; 2,302 filtered). It remains ignored upstream with an
explicit known write-busy defect, so a single successful run cannot convert it
into production evidence.

The repository memory harness was run in release mode with MVCC, four
connections, update churn, 100 operations per batch, and a 16 KiB checkpoint
threshold:

Raw measurements are preserved in
`docs/benchmarks/phase11-mvcc-memory.json`.

| Iterations | Peak RSS | Final RSS | Net RSS growth | Heap peak | Heap at exit | Log file |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 50 | 29,790,208 | 25,563,136 | 18,714,624 | 2,469,217 | 1,598,744 | 16,289 |
| 200 | 31,006,720 | 26,775,552 | 19,808,256 | 3,159,704 | 1,237,640 | 0 |

These samples show GC/checkpoint activity and no proportional retained-heap
growth in this narrow workload. They do not cover a pinned long reader,
sustained production duration, startup of a large database, provider indexes,
or the known correctness gaps, so they cannot waive the failed gate.

## Commands and provenance

```text
git status --short
git branch -vv
git remote -v
git merge-base --is-ancestor 977383ff40edc44ef410af062ed0d2322252a869 HEAD
git fetch upstream --tags --prune
git log/diff 977383ff40edc44ef410af062ed0d2322252a869..069b5431e...
git grep against the retained pin, v0.8.0-pre.4, and upstream/main
cargo test -p turso_core --lib
cargo test -p turso_pg_tests
cargo test -p turso_whopper
cargo test -p turso_core --lib mvcc::tests::test_non_overlapping_concurrent_inserts \
  -- --ignored --nocapture
cargo run --release -p memory-benchmark -- --mode mvcc \
  --workload update-churn -i 50 -b 100 --connections 4 \
  --mvcc-checkpoint-threshold 16384 --format json
target/release/memory-benchmark --mode mvcc --workload update-churn \
  -i 200 -b 100 --connections 4 --mvcc-checkpoint-threshold 16384 --format json
```

The audit preserved the existing dirty worktree entries
(`revised_plan.md` user reference and `.codex/config.toml`) and made no
destructive Git operation.

## Resume criteria

Resume Phase 11 only when Turso publishes or identifies a new exact candidate
that removes the production warning and closes the concurrency, cursor,
rowid, long-reader memory, and checkpoint gates with non-ignored recovery and
stress evidence. Re-run the complete upstream-sync audit; do not assume a
future branch or moving documentation silently qualifies.
