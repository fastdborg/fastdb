# FastDB Core release-readiness gates

Status: Phase 7 complete locally; Phase 8 planning is next; no alpha, beta, or
Core 1.0 release authorized

This checklist is the durable release gate. Phase plans and reports contain
execution details and evidence; completing a development phase does not itself
authorize a tag, package publication, signature, or artifact upload.

## Completed baseline decisions — 2026-08-13

- [x] FastDB Core is licensed under MIT in `LICENSE.md` and FastDB package
      manifests record `license = "MIT"`.
- [x] Turso copyright, MIT permissions, dependency notices, and mechanical
      provenance remain preserved.
- [x] The former BSL/commercial/change-license model and its special CLA and
      entity-approval blockers are retired for Core.
- [x] A future proprietary FastDB Cloud service is a separate product and does
      not alter Core's MIT terms.
- [x] The project owner approved the FastDB name and the precise
      “SurrealQL-compatible subset” wording, subject to `CLEAN_ROOM.md`.
- [x] SurrealDB `v3.1.5` is the immutable behavioral reference. Moving online
      documentation informs research but does not silently change the contract.
- [x] Core 1.0 is scoped to the embedded Rust API and CLI. Cloud, a network
      server, non-Rust SDKs, multiprocess access, recursive graph traversal,
      ANN, full analyzer compatibility, and complete SurrealQL are excluded.

## Phase 5 local technical baseline

- [x] Turso pin `977383ff40edc44ef410af062ed0d2322252a869` retained; no
      inherited engine/parser/WAL/yield-point change.
- [x] Format and dialect remain 1; migration fixtures and digests are committed.
- [x] FastDB packages remain `0.0.0` and `publish = false`.
- [x] Bounded parse/prepared caches preserve transaction and schema semantics.
- [x] Parser and structured CRUD fuzz targets exist with independent seeds.
- [x] Resource, model, injection, JSON, CLI, crash, public-I/O, fixture,
      filesystem, integrity, and index-plan coverage exists.
- [x] Public API/CLI, limitations, format/upgrade policy, clean-room record,
      benchmark, and compatibility documents are current for Phase 5.
- [x] The local command matrix and release benchmark passed; historical
      evidence is in `docs/phase5-report.md`.

The FastDB-only GitHub Actions workflow is intentionally absent from the Phase
5 baseline. Remote CI is not an alpha-candidate gate, but cross-platform CI is
mandatory for Core 1.0 in Phase 12.

## Ordered Core 1.0 development gates

### Phase 6 — Format 2 and multimodel foundation

- [x] Complete and publish the read-only audit of pinned translated-AST, FTS,
      vector, custom-index, explain/integrity, checkpoint, backup, and
      maintenance facilities plus the required upstream comparison, without
      merging or changing the retained pin.
- [x] Specify and transactionally implement format 1 to format 2 migration;
      preserve IDs, physical names, documents, B-tree expressions, and rollback.
- [x] Add the sealed internal provider/capability and hidden typed-column model;
      unknown providers, versions, encodings, and options fail before mutation.
- [x] Execute and evidence expression projections/aliases, structured
      `EXPLAIN`, and B-tree remove/rebuild through the public API and CLI.
- [x] Pass format fixtures, failure injection, crash/reopen, fuzz, integrity,
      unchanged Phase 5 tests/plans, and Phase 5 benchmark gates.

### Phase 7 — Graph records and bounded traversal

- [x] Characterize and implement the approved `v3.1.5` relation-table and
      `RELATE` subset with record literals and bound record parameters.
- [x] Prove immutable hidden endpoints, synthesized `id`/`in`/`out`, generated
      UUIDv7 edge IDs, two-way adjacency indexes, fixed-depth traversal, and
      schemafull edge validation.
- [x] Prove dangling-edge defaults, `ENFORCED` existence checks, atomic node
      cascade, failure rollback, reopen/crash recovery, and scan-free plans in
      both directions.
- [x] Keep cartesian targets, complex IDs, `OR UPDATE`, edge-path filters,
      recursive paths, and standalone traversal explicitly rejected.

### Phase 8 — Full-text search

- [ ] Normalize the characterized SurrealQL FTS subset and labeled FastDB/Turso
      extension into one versioned catalog provider; do not count extension
      syntax as SurrealQL compatibility.
- [ ] Limit Surreal analyzers to behaviorally equivalent configurations,
      beginning with `blank` and no pipeline; keep broader Turso tokenizers and
      weights extension-only.
- [ ] Prove hidden TEXT/document atomicity, actual plan selection, ranking,
      highlighting, churn, rollback, reopen, abrupt exit, corruption handling,
      bounded memory, and rebuild/optimization.
- [ ] Reject affected FTS reads after an indexed write in an explicit
      transaction until commit; reject FTS explicitly on unsupported WASM.

### Phase 9 — Exact vector search / `0.1` alpha candidate

- [ ] Implement fixed-size `array<float, N>` with public arrays and hidden
      native `vector64` BLOBs; enforce finite values, dimensions <= 65,536, and
      `K <= 10,000`.
- [ ] Prove exact cosine/euclidean KNN, bound vectors, filtering before top-k,
      distance projection, bounded memory, structured explain output, and
      independent reference calculations.
- [ ] Explicitly reject HNSW, DiskANN, and `toy_vector_sparse_ivf`; make no ANN
      GA claim.
- [ ] Pass document/BLOB rollback/reopen and native-equivalent scan/storage
      benchmarks without regressing prior gates.

Passing Phase 9 freezes a candidate surface for a possible `0.1` alpha review;
it does not authorize publication.

### Phase 10 — Operational readiness

- [ ] Add bounded query/resource options, with-options methods, deterministic
      close, consistent backup, and CLI check/backup/restore/rebuild commands.
- [ ] Bound time, output rows/bytes, graph hops, vector dimensions, and FTS
      queries without logging source or parameters.
- [ ] Validate catalogs, hidden columns, adjacency indexes, FTS state, vector
      encodings, and engine integrity through one supported check path.
- [ ] Pass randomized backup/restore hashes, interrupted operations,
      document-derived provider rebuilds, busy/checkpoint behavior, safe limit
      failures, upgrade/rollback, and clean/abrupt shutdown retention tests.

### Phase 11 — Parallel writers / `0.9` beta candidate

- [ ] Perform the mandatory exact-implementation upstream audit. The current
      pinned MVCC implementation is experimental and not production-ready.
- [ ] Confirm a stable candidate has snapshot isolation, recovery, garbage
      collection, bounded memory, and acceptable checkpoint behavior. If not,
      stop the Core 1.0 roadmap at this gate.
- [ ] If qualified, integrate exactly one audited SHA through the mandatory
      upstream-sync workflow and update all pin/evidence records together.
- [ ] Add opt-in `ParallelWrites` while retaining `Serialized` as default; add
      retryable conflict errors without implicit transaction replay.
- [ ] Pass graph, B-tree, FTS, vector, backup, schema, conflict, long-reader,
      checkpoint, crash, starvation, and memory-growth suites in both modes.

Passing Phase 11 freezes a candidate surface for a possible `0.9` beta review;
it does not authorize publication.

### Phase 12 — Core 1.0 hardening

- [ ] Freeze format 2, Rust API, error categories, strict CLI JSON envelope,
      compatibility matrix, and migration/rollback policy.
- [ ] Pass Linux, macOS, and Windows CI plus sustained parser/graph/FTS/vector
      fuzzing, deterministic simulation, failure injection, recovery, integrity,
      migration, concurrency, and unchanged upstream regression suites.
- [ ] Preserve raw benchmark evidence proving graph, FTS, and vector p95
      overhead <= 2x equivalent native Turso physical workloads, plus actual
      execution-plan selection for every declared index.
- [ ] Publish a FastDB security policy and vulnerability-reporting process;
      complete dependency, license, notice, and provenance audits.
- [ ] Complete backup/restore drills, contribution policy, support window,
      release signing/checksums, artifact inventory, release rollback procedure,
      and exact-candidate documentation review.
- [ ] Verify all FastDB package versions and `publish` settings are intentional
      for the authorized release.

## Core 1.0 claim and release authorization

- [ ] Every Phase 6–12 Definition of Done and report is complete with no open
      stop condition or silently unsupported syntax.
- [ ] `COMPAT.md` reflects only executable conformance evidence and clearly
      separates SurrealQL compatibility from FastDB extensions.
- [ ] The exact candidate supports the production-ready claim with published
      recovery, operations, concurrency, security, and performance evidence.
- [ ] The project owner explicitly authorizes the version, tag, package
      publication, signatures, checksums, and release upload.
- [ ] Perform only the separately authorized release operations and record the
      rollback point.

Until every applicable checkbox passes, describe the project by its completed
phase and candidate status. Do not claim “production-ready FastDB Core 1.0.”
