# Embedded FastDB and FastQL V2

Current direction: 2026-09-25. V1 is released. Cloud v0.2.0 remains a separate
workstream; no cloud implementation or deployment belongs in this checklist.
Preserve V1 behavior and upgrade paths, except documented approved corrections.
The release client set is native Rust, Node.js/TypeScript, Python, PHP, Swift,
C# and Go. Browser/WASM support is removed from the active checkout and outside release scope. V2 is not yet
complete or released.

## Checklist

- [x] V2-0: Replace stale active handoffs; retain release and historical evidence.
- [x] V2-S1: Spatial point construction, strict coordinate validation and a
  documented distance model through FastQL SELECT and document writes; test
  antimeridian/poles, errors, persistence and transaction rollback.
- [x] V2-S2: Managed spatial index and indexed radius search. Adopt the proposed
  `CREATE SEARCH INDEX ... USING SPATIAL` and `search::near` only with atomic
  build/write/drop, reopen/rollback, catalog versioning, integrity checks,
  deterministic distance/ID ordering, and measured index use. Compare against
  exhaustive distance evaluation at cell boundaries, poles and the antimeridian.
  Candidate filtering must have no false negatives; refine every candidate.
- [x] V2-S3: Cell aggregation with a documented grid/resolution contract; evaluate
  H3 against alternatives before committing dependencies or persisted cell IDs.
  [Design assessment](v2-cell-design.md) selects H3; h3o 0.9.4 without default
  features passes reference, geographic, grouping, persistence and client checks.
- [x] V2-Q1: Record brace projections, keeping zero-or-one rowset cardinality;
  [adopted contract](v2-record-projections.md) specifies object wildcard, one-hop
  forward fetch, naming and limits, with parser/engine/client qualification.
- [x] V2-Q2: Declared inverse relationships with required reference indexes,
  deterministic pagination and bounded expansion; no silent collection scan.
- [x] V2-F: Native indexed full-text search with ranking, filters, index lifecycle,
  explain plans and consistency tests. The separately approved backing-storage
  fix is integrated in `12109384a`; affected native suites, installed Python
  integrity/drop assertions and the complete V1 upgrade/restore rehearsal pass.
  See [integrated evidence](v2-upgrade-restore-evidence.md). Browser support is removed.
- [x] V2-A: Indexed ANN search; define dimensions/metric, deterministic ties,
  filters, lifecycle, recall/performance evidence and exact-search comparison.
- [x] V2-J: Sandboxed user JavaScript functions with bounded execution, no external
  I/O or database re-entry, explicit determinism/versioning and cancellation tests.
- [ ] V2-C: Qualify native Rust, Node.js/TypeScript, Python, PHP, Swift, C# and Go clients.
  Implementation and Linux development qualification: [client guide](native-language-clients.md).
  - [x] Shared C ABI with safe handle lifecycle, lossless values and transaction diagnostics.
  - [x] PHP FFI package and native integration checks.
  - [x] Swift Package Manager library and native integration checks.
  - [x] C#/.NET library and native integration checks.
  - [x] Go/cgo module and native integration checks.
  - [x] Native Python binding and initial installed-wheel qualification on Linux
    x86_64 / CPython 3.10, 3.12 and 3.14.
  - [x] Rebuild Python after approved scalar-error integration; eight installed
    tests pass on all three runtimes, including throwing JavaScript, nested
    savepoint recovery, failed-write atomicity and cancellation. See
    [current Python evidence](v2-python-scalar-evidence.md).
  - [x] Same installed native Node development archive passes synchronous/worker
    V2 lifecycle, persistence and TypeScript checks on Node 22.0.0 and 24.19.0;
    see [package evidence](v2-node-package-evidence.md). Final archive must include
    the subsequent scalar-error fix and current notices.
  - [x] Standalone Rust application builds/runs offline outside the workspace,
    using direct V2 APIs and persistence checks; see
    [Rust consumer evidence](v2-rust-client-evidence.md).
  - [ ] Qualify final versioned client artifacts and the supported native platform matrix.
- [ ] V2-R: Full scoped acceptance, V1 database upgrade/restore, client artifacts,
  language/docs/tooling parity, benchmarks and explicit supported-platform release.
  - [x] Integrate the separately approved first-commit FULL-mode WAL sync fix in
    isolated commit `fe2ccd404`; see [review](proposals/wal-first-commit-sync.md).
  - [x] Repeat the native sync probe and installed-browser durability/fault checks
    on the integrated source; see [evidence](v2-wal-durability-evidence.md).
  - [x] Initial V1 released-client database upgrade/new-write/reopen smoke against
    the rebuilt Node client; broader upgrade/restore qualification remains open.
  - [x] Include pinned repository notice supplements in offline-generated client
    bundles and verify the browser/Python packaged copies; see
    [notice evidence](v2-notice-evidence.md). Pinned Bon/DataSketches/Tantivy
    follow-up texts and crc32c inline attribution are also included; complete
    attribution remains open. Workspace license/core notice mappings are now
    checksummed and covered by 13 offline generator tests.
  - [x] Align the FastQL editor with implemented V2 spatial/H3 and replacement
    function syntax; qualify formatter/engine parity and the installed VSIX. See
    [tooling evidence](v2-language-tooling-evidence.md). No publication is claimed.
  - [x] Integrate the separately approved scalar-error and opt-in WASI FTS fixes
    in isolated commits; maintain [upstream replacement/removal criteria](core-exceptions.md).
  - [x] Combined acceptance passes 738 Rust and 115 Node/application tests, with
    formatting, Clippy and TypeScript; see [evidence](v2-core-integration-evidence.md).
  - [ ] Qualify final native artifacts, finish attribution and publish the release.
  - [x] Integrate the separately approved FTS backing-storage fix in isolated
    commit `12109384a`, preserving the reviewed patch exactly.
  - [x] Repeat native V1 upgrade/restore and affected suites on that integrated source.

Unique conflict targets, OMIT, SPLIT, strict schemas and array-element indexes
remain FastQL ergonomic candidates requiring separate semantics review; they are
not silently promoted into required scope. Broader graph traversal, advanced
geometry, changefeeds/sync and procedural scripting remain V3.

## Current milestone

Complete V2-C/V2-R client and release qualification. The V2-J milestone below is
now complete; remaining release gates above stay open.

V2-J: implement sandboxed user JavaScript functions with persisted typed
signatures and versioned definitions. Reuse the bounded QuickJS runtime, define
isolation/determinism and lifecycle, and qualify invocation through FastQL,
mutations and clients. Functions must have no host I/O or database re-entry. Persisted
signatures/lifecycle, isolated execution, typed conversions and FastQL invocation
are implemented in development. A reproduced engine scalar-error transaction bug
has an explicitly approved [core fix](proposals/udf-error-transaction.md), now
integrated in `f26014f04`. Combined-current-source acceptance now passes 738 Rust and 115 Node/application
tests with formatting, Clippy and TypeScript; see [integrated evidence](v2-core-integration-evidence.md).
V2-J is closed under its documented sandbox contract. V2-C/V2-R still own final
platform and artifact qualification.

Spatial functions, managed radius search, H3 aggregation, record brace
projections, indexed inverse relationships and native ANN are complete under
their documented contracts. Native full-text storage qualification now also
passes after the approved fix in `12109384a`, closing the reopened V2-F gate.
Its cache exception remains isolated in `fb246a8e4`; ANN adds no further core edits.
[ANN acceptance](v2-ann-evidence.md) passes 730 scoped Rust tests and 113
Node/application tests, plus formatting, Clippy, strict TypeScript and CLI checks.
All eight FastDB crates and client package metadata now use `2.0.0`; publication
remains pending. Go uses the major-version module path ending in `/v2`.
[Client assessment](v2-client-assessment.md) records the
Python implementation and browser build gaps. V2-C and V2-R still own final
platform/artifact qualification.

Milestone evidence: [V2 spatial foundation verification](v2-verification.md).
V2-S2 evidence: [spatial index verification](v2-spatial-index-evidence.md).
V2-S3/Q1 evidence: [cell/projection verification](v2-cell-projection-evidence.md).
V2-Q2 evidence: [inverse relationship verification](v2-relations-evidence.md).
V2-F evidence: [full-text verification](v2-fulltext-evidence.md).
V2-A evidence: [ANN verification](v2-ann-evidence.md).
Keep all remaining V2 release requirements above open.

## Removed browser support

The user removed browser support, then added PHP, Swift, C# and Go to the native
Rust, Node.js/TypeScript and Python client set.
Browser code, build/probe scripts and the browser-only WASI core exception are
removed. Firefox/Chromium/OPFS/Vite qualification does not gate the release.
Historical reviews and evidence remain; a verified source archive is outside the
checkout. See [the removal record](browser-removal.md). Python's shared protocol
is retained. No browser artifact will be published with V2.

## Final release delivery checklist

- [x] Align all FastDB package identities to 2.0.0, including Go's /v2 module path.
- [x] Prepare a reproducible bundle containing CLI, Node archive, Python wheel,
  shared native library, PHP/Swift/Go sources, C# package, source, notices and checksums.
- [x] Resolve remaining attribution gaps and include notices in every native package.
  See [Linux rehearsal evidence](v2-linux-release-evidence.md); final committed
  artifacts must repeat exact-package qualification.
- [ ] Verify the exact versioned artifacts, V1 upgrade/restore and advertised platforms.
- [ ] Commit the complete intended source, run scoped CI and verify provenance.
- [ ] Publish the versioned release and verify downloadable artifacts and checksums.

Package metadata is being prepared at 2.0.0; this does not imply publication.
The user confirmed Linux for V2. Binary scope is Linux x64, continuing the V1
release platform. macOS and Windows are outside this release.

Versioned-source verification passed: 738 Rust tests, 115 Node/application tests,
3 C ABI tests, formatting, Clippy and strict TypeScript. The log is
`/tmp/fastdb-v2-versioned-acceptance.log`. The bundle build and exact-artifact
verification scripts are `build-v2.py` and `check-v2-bundle.py`; an uncommitted
source rehearsal must use `--development` and is never publication eligible.
Attribution preparation now covers every native dependency declaration; native
runtime linkage and inclusion in the final artifacts remain to be verified.
