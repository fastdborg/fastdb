# FastDB pre-1.0 and Core 1.0 release-readiness gates

Status: Phase 11 stopped at its mandatory MVCC audit. Phases 12–15 are
technically complete locally; Phase 16 planning is next on the serialized
stable-WAL pre-1.0 track. No alpha, beta, pre-1.0 package publication, or Core
1.0 release is authorized.

This checklist is the durable release gate. Phase plans and reports preserve
execution details and evidence. Completing a development phase does not itself
authorize a tag, package publication, signature, or artifact upload.

## Preserved baseline

- [x] FastDB Core is MIT licensed and inherited Turso notices/provenance remain
      mechanically auditable.
- [x] SurrealDB `v3.1.5` is the immutable behavioral reference. Moving online
      documentation informs research but does not silently change the contract.
- [x] Phases 0–5 established and hardened the original embedded Rust/CLI MVP.
- [x] Phase 6 transactionally established format 2 and the sealed provider,
      explain, projection, and index-maintenance foundation.
- [x] Phase 7 added the characterized bounded graph subset with two-way
      adjacency and atomic cascade behavior.
- [x] Phase 8 added the characterized Surreal FTS subset and labeled
      FastDB/Turso extension.
- [x] Phase 9 added fixed-dimension native vector storage and exact bounded
      vector search.
- [x] Phase 10 added resource limits, deterministic close, backup, restore,
      check, provider rebuild, lifecycle, and operational evidence.
- [x] Phase 11 audited the retained pin, `v0.8.0-pre.4`, and fetched upstream
      `main`; no exact candidate qualified for parallel writers. No pin, API,
      format, or concurrency mode changed. See `docs/phase11-mvcc-audit.md`.

The completed Phase 0–11 plans/reports are historical evidence and must not be
rewritten to imply the new roadmap existed when they ran.

## Active compatibility contract

- [x] Lock `compat/surrealdb-v3.1.5.toml` before executable Phase 12 expansion
      and keep `COMPAT.md` mechanically synchronized with it.
- [x] Assign every atomic non-excluded `v3.1.5` capability to Phases 12–20.
- [ ] Promote an entry to Supported only after execution and conformance
      evidence. Partial is allowed only while a phase is in progress.
- [ ] At Phase 22, resolve every non-excluded entry to Supported or an approved
      architecture stop report; no Partial entry remains.
- [x] Use only public documentation and independently authored black-box probes
      against the unmodified official `v3.1.5` binary. Do not inspect or adapt
      SurrealDB source, tests, fixtures, expected outputs, or fuzz corpora.

The active target excludes geospatial/geometry; versioned history,
changefeeds, time-series retention, and Realtime/LIVE queries; GraphQL/GQL;
multiprocess access; and parallel writers. General datetime/duration values,
indexes, and `time::*` functions remain in scope. Native FTS and read-only
ATTACH/DETACH are labeled FastDB extensions and do not count as SurrealQL
compatibility.

## Ordered pre-1.0 gates

### Phase 12 — Inventory and format 3

- [x] Commit the authoritative Phase 12 plan before executable changes.
- [x] Lock the atomic compatibility inventory and mechanical matrix check.
- [x] Transactionally migrate format 2 to format 3 while preserving format 1
      and 2 fixtures, IDs, documents, physical objects, and provider state.
- [x] Add collision-safe non-geospatial value representations for bytes,
      datetime, duration, decimal-compatible numbers, sets, ranges, and richer
      typed collections.
- [x] Pass migration rollback, reopen, backup/restore, unknown-version,
      corruption, failure-injection, and unchanged Phase 6–10 gates.

### Phase 13 — Expressions and functions

- [x] Complete bounded indexing/slicing, casts, collection/range operations,
      null/none semantics, subexpressions, and characterized pure functions.
- [x] Keep external-resource and ambient-context functions unavailable where
      the current evaluator cannot meet SSRF, DNS, redirect, cancellation,
      authorization, time, size, concurrency, and isolation gates; record each
      stop and require its later owning phase to reopen it with executable
      evidence rather than publish a weaker provider.

### Phase 14 — CRUD and query completeness

- [x] Add INSERT, UPSERT, richer mutations/returns, subqueries, aggregation,
      grouping, split/omit/fetch, multiple targets, and analyze forms.
- [x] Preserve bound values, direct translated AST, authoritative FastDB
      semantics, transaction poisoning, and proven-safe index pushdown.

### Phase 15 — Scripting, schema, views, and events

- [x] Add bounded control flow, custom functions/parameters, views, events,
      defaults, assertions, computed/readonly fields, and matching INFO/REMOVE.
- [x] Prove event atomicity, recursion/resource limits, rollback, and reopen;
      retain explicit architecture stops for the 13 rows that cannot meet the
      format-3, stable-WAL, server-security, or sealed-provider contracts.

### Phase 16 — Graph completion

- [ ] Add the remaining characterized RELATE and traversal forms, including
      bounded recursive/filtered paths with deterministic cycle behavior.
- [ ] Preserve immutable endpoints, two-way adjacency, scan-free plans, schema
      enforcement, and atomic cascade failure behavior.

### Phase 17 — Search and specialized indexes

- [ ] Complete behaviorally equivalent analyzers, multi-field/boolean FTS,
      remaining exact-vector forms, and non-geospatial specialized indexes.
- [ ] Add FastDB-owned HNSW/MTREE providers without Turso core changes or its
      experimental toy provider.
- [ ] Pass transactional maintenance, rebuild, crash/reopen, corruption,
      memory, plan-selection, exact-fallback, and recall gates.

### Phase 18 — Authentication and authorization

- [ ] Add embedded principals, database and record users, signup/signin,
      JWT/session behavior, roles, reserved auth context, and permissions.
- [ ] Enforce authorization before graph, FTS, vector, aggregation, ordering,
      event, or ordinary record candidates can influence observable results.
- [ ] Pass Argon2id work-factor, expiration/revocation, non-disclosure,
      redaction, audit, and capability/JWKS security gates.

### Phase 19 — Secure single-database server

- [ ] Add `fastdb-server` and `fastdb serve` strictly over the FastDB API.
- [ ] Implement the applicable clean-room `v3.1.5` HTTP/WebSocket RPC contract
      and negotiated encodings; exclude LIVE/KILL subscriptions and GQL.
- [ ] Pass authentication, TLS, malformed protocol, session isolation,
      backpressure, resource/rate-limit, busy-writer, graceful shutdown, and
      acknowledged-write recovery tests.

### Phase 20 — Multi-database control plane and ATTACH

- [ ] Map opaque namespace/database IDs to one `.fastdb` file each and add
      scoped users, routing, lifecycle, quotas, USE, INFO, DEFINE, and REMOVE.
- [ ] Add maximum-ten, validated, connection-local read-only ATTACH/DETACH.
- [ ] Reject attached writes, cross-file relations/transactions, and remote
      paths outside an explicit canonical-directory allowlist.

### Phase 21 — Rust, TypeScript, Go, and PHP SDKs

- [ ] Add a versioned FastDB C ABI with opaque handles and isolated unsafe FFI.
- [ ] Preserve the Rust Builder API and add path/`file://`/`mem://`/HTTP/WS
      endpoint routing.
- [ ] Pass one independently authored local/remote typed-value, query,
      transaction, authentication, error, cancellation, and close corpus across
      Rust, Node/TypeScript, Go, and PHP. Browsers remain remote-only.

### Phase 22 — Broad-compatibility hardening

- [ ] Resolve the locked inventory with no Partial entries.
- [ ] Pass cross-platform migration, fuzzing, deterministic simulation,
      failure/crash, provider rebuild, authorization, protocol, opaque-client
      differential, SDK, backup/restore, dependency, provenance, and rollback
      suites on the exact candidate.
- [ ] Complete TLS/security review, vulnerability policy, denial-of-service and
      secret scanning, artifact/checksum preparation, support policy, and
      release rollback drills.
- [ ] Produce a pre-1.0 report that does not make a production-ready or Core
      1.0 claim.

## Dormant Phase 23 Core 1.0 gate

- [ ] Begin only when Turso publishes or identifies a new exact stable
      parallel-writer candidate.
- [ ] Repeat the complete upstream-sync/MVCC audit from scratch; do not
      reinterpret the stopped Phase 11 evidence.
- [ ] Require snapshot isolation, retryable conflicts after rollback, recovery,
      garbage collection, checkpoint progress, bounded long-reader memory,
      providers, security, server, and SDK suites in serialized and parallel
      modes.
- [ ] Complete a separate final release gate before using the phrase
      “production-ready FastDB Core 1.0.”

## Commit and release authorization

- [x] Commit the roadmap reset as an isolated rollback point.
- [ ] Commit every authoritative `plan-phaseN.md` before implementing that
      phase.
- [ ] Finish every phase with a report/checkpoint commit recording its starting
      SHA, ending SHA, commands, evidence, stop conditions, and rollback range.
- [ ] Preserve Phase 0–11 history; do not squash or rebase completed phases.
- [ ] Verify all versions and `publish` settings are intentional before any
      separately authorized artifact operation.
- [ ] Obtain explicit authorization before tagging, publishing packages,
      signing checksums, uploading artifacts, or pushing a release.
