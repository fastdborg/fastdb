# FastDB Cloud C0 — Phase 0 Decision Note

Scope: only decisions that affect the **local** Phase 0 format and frontend so
that later logging/sync is not made impossible. No cloud service is built in
Phase 0 (see `revised_plan.md` §10). This note is not on the Core feasibility
critical path.

## Database identity

- Phase 0 persists a `database_id` in `__fastdb_meta` (a 32-hex random u128)
  that is stable across reopen (proven by the vertical-slice test). This
  *demonstrates* a persistent database id.
- **Its encoding is not stable.** The Phase 0 format version is explicitly `0`
  and disposable (`plan-phase0.md` §2). A future cloud needs a **globally
  unique, immutable** database id; Phase 0's random u128 is neither globally
  coordinated nor promised forward-compatible. Do not treat it as the cloud
  identity format.

## Logical mutation log requirements (for later Cloud C0/C1)

A future durable log will need, at minimum:

- an **epoch / fencing token** so at most one unfenced writer owns a
  database/log epoch at a time;
- a **strictly ordered sequence** per database;
- an **idempotent mutation id** so retries cannot duplicate logical mutations;
- the **database format and dialect version** (Phase 0 records both as `0`)
  so a log record carries the format it was written under;
- a **checksum** per log record/segment.

## Does Phase 0 expose a deterministic logical mutation?

**No.** Phase 0 operations are physical SQL (catalog + `rid`/`doc`) executed
through the engine; there is no FastDB-level logical-mutation representation
yet. Determining whether the engine's own change capture (CDC) or a FastDB
emit-on-commit layer is the right source is **deferred to Cloud C0**. Phase 0
does not preclude either path: `rid` and `doc` are stable per-record handles,
and the catalog maps logical→physical deterministically.

## Pinned Turso log/sync modules to audit later (not endorsed)

These exist at the pin and *deserve later audit* for cloud reuse; Phase 0
makes **no claim** that they satisfy FastDB Cloud requirements:

- `sync/engine` — Turso sync engine (push/pull/checkpoint model).
- `core/mvcc/persistent_storage` (`DurableStorage`) — durable storage
  plumbing referenced by `OpenOptions`.
- The `aristo` logical-log surface (referenced via the `aristo-instr`
  `turso_core` feature and `core/json/cache.rs`) — instrumentation hooks only.

Any reuse requires its own audit of durability, fencing, idempotency,
checkpointing, and recovery against FastDB's invariants
(`revised_plan.md` §10.1).

## Undecided until Cloud C0 benchmarks

The following remain explicitly open and must not be fixed prematurely:

- object-storage provider and segment/block size;
- checkpoint interval and generation size;
- commit batching delay and maximum delay/isolation guarantees;
- retention, backup windows, and restore objectives;
- pricing quotas and tier definitions (the `$5/$20/$100` ladder is a
  hypothesis, `revised_plan.md` §1.2/§10.3).

## Phase 0 guardrail honored

Phase 0 keeps `database_id`, `format_version`, and `dialect_version` persisted
and version-refuses unknown future versions before mutation, so a later log
can rely on a versioned, identifiable database without re-deriving it.
