# FastDB Phase 11 — Parallel-writer qualification gate

Status: stopped at the mandatory audit on 2026-08-13

## 1. Purpose

Phase 11 may add opt-in parallel writers only after one exact Turso SHA proves
production-suitable snapshot isolation, recovery, garbage collection, bounded
memory, checkpoint behavior, conflict reporting, and crash safety. Serialized
stable WAL remains the compatibility default. This phase is a stop gate, not
permission to enable an experimental journal mode.

## 2. Audit candidates

Audit, without merging:

- retained pin `977383ff40edc44ef410af062ed0d2322252a869`;
- newest fetched release tag `v0.8.0-pre.4` at
  `ce276e4218d631ab7b2e15bfa277f1509be1cf82`;
- fetched upstream `main` at
  `069b5431e86779d70df3940711bb61f8601db069`.

The audit follows `.claude/skills/mvcc/SKILL.md`,
`.claude/skills/memory-benchmark/SKILL.md`, and
`.claude/skills/upstream-sync/SKILL.md`. Fetching and comparison do not
authorize a merge, pin update, push, or release.

## 3. Qualification requirements

One exact candidate must satisfy all of these before implementation starts:

1. MVCC is no longer documented or configured as experimental or unsuitable
   for critical data.
2. Snapshot isolation, non-conflicting concurrent writes, write conflicts,
   schema conflicts, long readers, indexes, savepoints, provider-derived state,
   and statement abandonment have passing non-ignored coverage.
3. Logical-log and interrupted-checkpoint recovery fail closed and preserve all
   acknowledged commits under deterministic failure injection.
4. Garbage collection bounds version memory under sustained update churn and a
   pinned long reader; startup does not require unbounded eager loading.
5. Checkpoint latency and exclusion are acceptable. A stop-the-world checkpoint
   that blocks readers and writers does not qualify; an experimental passive
   path with unresolved snapshot hazards does not qualify.
6. Stable engine errors distinguish retryable write/schema conflicts. FastDB
   can return them after rollback without implicit user-transaction replay.
7. Graph, B-tree, FTS, vector, backup, check, schema publication, checkpoint,
   crash, starvation, and memory suites pass in serialized and candidate modes.

## 4. Audit result and stop condition

No audited candidate qualifies. The retained pin and current upstream provide
substantial recovery and GC work, but upstream still labels MVCC experimental,
warns that queries may be incorrect or panic, documents eager memory loading
and blocking checkpoints, and keeps fundamental concurrency/cursor correctness
tests ignored for known failures. The newest release is a prerelease and does
not remove those limitations; current `main` does not contain a qualifying
MVCC change relative to the pin.

Therefore Phase 11 stops before any public API, error category, format, engine,
or concurrency-mode change. Phase 12 and the “production-ready Core 1.0” claim
remain blocked. A future resumed Phase 11 must fetch and audit a new exact
stable candidate from scratch.

## 5. Definition of done

For this stopped phase, the completed deliverable is the exact-SHA audit and
durable stop record in `docs/phase11-mvcc-audit.md`, committed as the isolated
Phase 11 rollback point. It is not a beta candidate and does not satisfy the
Phase 11 implementation exit gate.
