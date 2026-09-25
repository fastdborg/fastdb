# Engine provenance

- Repository: https://github.com/tursodatabase/turso (remote `upstream`).
- Baseline: release `v0.7.2`, full commit `046e9cbf67d22491e8ecc941ec2891b02a9f3cad`.
- FastDB fork: `git@github.com:fastdborg/fastdb.git` (remote `origin`). Upstream ancestry is preserved; inspect current refs rather than relying on historical branch names.
- Toolchain: upstream Rust 1.88; locally tested with 1.88.0 on Linux x86_64.
- Engine dependency: in-workspace `turso_core` with default features plus `conn_raw_api` for engine WAL hooks and `fts` for V2 full-text search. V2 opens opt into index methods; managed search owns the public surface.
- This release exposes `Connection::prepare`, `prepare_stmt`, and blocking statement execution. It predates the `postgres/frontend` directory in the plans; the same separate-frontend boundary is used without copying or modifying core.

Upstream evidence inspected through the GitHub check-runs API for this exact SHA:
[Linux native Node DB bindings](https://github.com/tursodatabase/turso/actions/runs/30547750865/job/90892023910) succeeded;
[Windows native Node DB bindings](https://github.com/tursodatabase/turso/actions/runs/30547750865/job/90892023935) succeeded.
This supports the baseline only. Broad engine conformance and our combined build still require their own evidence; these upstream jobs are not FastDB tests.

Local integration changes:

1. Eight explicit FastDB workspace members and corresponding lockfile entries; upstream default-members unchanged.
2. FastDB implementation, tests, scripts, and documentation under `fastdb/`.
3. Inherited workflow YAML files moved unchanged into `.github/upstream-workflows/` so GitHub cannot execute them. Only `.github/workflows/fastdb-ci.yml` remains active. Audit this directory on every upstream merge before pushing.
4. Approved engine exceptions are recorded below; upstream parser, bindings and CLI implementation files remain unchanged. The frontend also directly depends on the pinned workspace parser/extension crates for AST lowering and statically linked pure accessors; registration uses the documented unsafe startup extension context API, which is freed before exposing the connection.

## Approved core exception: trigger interruption

The user explicitly approved candidate 660a61afb947121fa8abf346ed600c780dfe123c
on 2026-09-14. It was integrated unchanged as ded389aea. The isolated patch in
core/vdbe/execute.rs preserves StepResult::Interrupt through OpProgram while
retaining Busy for actual contention and saving subprogram state in both cases.
The frontend cannot safely distinguish these outcomes after the old mapping.
No storage format or public API changes are introduced.

The previously ignored after-write trigger-cancellation regression is enabled.
The isolated candidate passed 98 pinned upstream trigger tests and its full
FastDB scoped suite; see [review evidence](docs/trigger-interrupt-review.md).
Combined-current-source verification is recorded separately after integration.
On upstream upgrades, check whether the fix is present before removing this
exception; preserve the regression and revalidate affected trigger behavior.

The root planning directory is not a Git repository. The ancestry-preserving checkout lives in `turso/`; product source is `turso/fastdb/`.

The separate FastDB native addon reuses pinned napi 3.8.3, napi-derive 3.5.2 and napi-build 2.3.1 from the existing lockfile. It depends on FastDB rather than the upstream Node binding and enables FTS through the FastDB frontend rather than the upstream Node binding. The frontend additionally embeds pinned rquickjs 0.12.2 for its fixed bundled string catalog.

The frontend enables serde_json's float_roundtrip feature to prevent one-bit numeric changes when reading stored tagged values. This changes a feature of the combined build, without upgrading the dependency or changing the stored format.

The FastDB CLI directly depends on Rustyline 15.0.0 already pinned in the upstream lockfile, with default features disabled and file history enabled. The lockfile adds only that dependency edge; upstream CLI source and dependency versions are unchanged.

The CLI's Unix SIGINT listener also uses signal-hook 0.3.18 already pinned in the upstream lockfile. Its dependency edge is FastDB-only; no upstream dependency versions or implementation files change.

The frontend directly uses stacker 0.1.22, already pinned in the upstream lockfile, to give SQL execution, profiling, audits, parser/preparation and row-execution calls an auxiliary stack when the caller has less than 16 MiB available. Growth requests 32 MiB on the same thread. Only the FastDB dependency edge is added; engine features, upstream implementation files and dependency versions are unchanged. This addresses observed debug-build parser stack exhaustion before the upstream depth guard, not general execution memory qualification.

## Approved core exception: FTS cache snapshot isolation

Approved by the user on 2026-09-25 after review of
[the design and control/fix evidence](docs/proposals/fts-cache-snapshot.md).
`core/index_method/fts.rs` checks pager identity before reusing its shared
Tantivy directory cache. A cache belonging to another connection is reloaded
through the requesting pager; same-pager reuse retains rollback validation.
This prevents uncommitted hit membership and scores from escaping the writer
and preserves reader snapshots across commit. No storage format or API changes.

The isolated change includes `test_fts_cache_preserves_connection_snapshots`
in the existing native index-method integration suite. Alternating connections
may reload the directory more often. On upstream upgrades, retain the regression
and check for an equivalent cache ownership fix before removing this exception.

V2 dependencies: h3o 0.9.4 without default features implements H3 cells;
Tantivy 0.26.1 and query grammar 0.26.0 use the existing workspace pins. The
frontend's FTS feature and query-grammar dependency add no version upgrades.

V2 ANN qualification adds USearch 2.26.2 (default features disabled) through its
Rust/C++ API and SHA-256 graph checksums with sha2 0.10.9. Graph snapshots and
redo records use ordinary engine transactions; no ANN core hook or external
index sidecar is added. C++ compilation, platform support and source attribution
remain part of the V2 release checks. See `docs/v2-ann.md`.

The V2 Python binding adds a sixth FastDB workspace member. It reuses PyO3 0.29.0
already pinned upstream, with its CPython 3.10 stable ABI and extension-module
features. Only the new FastDB package is added to Cargo.lock; no existing crate
version changes. Maturin 1.12.6 is pinned in its Python build metadata. Native
source remains under `fastdb/bindings/python`; upstream Python bindings are
unchanged. See `docs/v2-client-assessment.md` for installed-wheel evidence and
the browser feasibility probe.

The threaded-WASI probe adds a target-specific rquickjs-sys 0.12.2 bindgen feature
under the frontend, pinned bindgen 0.72.1 in Cargo.lock, and no existing crate
version upgrades. Native features remain unchanged. WASI SDK 33.0 and libclang
18.1.1 are external build tools; C++ exceptions require the SDK's exception-enabled
libraries. Core and vendor sources remain unchanged. See `docs/v2-wasm-probe.md`
for real-browser dependency checks and the remaining full-text/worker/I/O gates.

The browser client adds `fastdb-protocol` and `fastdb-browser` as explicit workspace
members. Python and browser use the same request/typed-value protocol; the Python
extension retains its existing PyO3 lifecycle and cancellation boundary. These
two packages add no external registry/git package identities, versions or checksums
relative to the WASI probe lockfile. Historical browser-stage Cargo.lock SHA-256:
`0dcbf5a9c5ead00c327c86d2a3fca97881ed3de9004e0d30f14a7b6b2f90f5b8`.
The browser uses WASI SDK 33 with a development Rust profile at optimization level
1, assertions retained, no LTO and no debug symbols. JavaScript support libraries
are pinned separately by the browser package's pnpm lockfile.

The browser qualification also reproduced partial ordinary SQL writes after
cancellation inside a caller transaction. The frontend now supplies a private
savepoint for ordinary INSERT/UPDATE/DELETE in existing caller scopes, rolling
back on cancellation and preserving native conflict-error dispositions. Native
oracle checks include rows, transaction state, changes() and last_insert_rowid().
This change uses public engine APIs and does not modify upstream sources. The scalar-error transaction and WASI FTS exceptions were subsequently approved
and integrated; see the maintenance register below. See [browser client evidence](docs/v2-browser-client.md).

The subsequent browser OPFS adapter implements the existing public engine `IO`
and `File` traits and uses `Database::open_with_io`. Dedicated browser workers own
exclusive database/WAL handles, map sync to flush, and return synchronous I/O
completions to Rust. No core/storage-format change or new dependency is added.
See [OPFS qualification](docs/v2-browser-opfs.md) and its remaining fault/platform
gates; WASI FTS is now integrated and undergoing combined qualification.

## Approved core exception: first-commit FULL-mode WAL sync

The OPFS fault matrix found a native FULL-mode first-commit sync omission. The
user approved the [separate review](docs/proposals/wal-first-commit-sync.md) on
2026-09-25. Isolated commit `fe2ccd404e005a198fac5365712cacaa54d47a2c` applies the
reviewed patch unchanged: the pager includes unpublished prepared frames in its
FULL-mode sync decision, with a native WAL regression. Its two-file diff SHA-256
is `72ed0afb451277c9406cc0a9b65b7867e7e8d47105296ce1c011476dee994f2d`.
No public API or storage format changes. NORMAL/OFF behavior remains unchanged.
Keep the regression and check for an equivalent fix during upstream upgrades.
Candidate evidence: 86 native passes, one existing ignored test, and 11 OPFS
fault cases in each browser. The integrated native sync probe and installed
browser package also pass, including all 11 fault cases in each browser; see
[active-source evidence](docs/v2-wal-durability-evidence.md). The scalar-error
and WASI FTS exceptions are now integrated as recorded below.

## Approved core exception: FTS backing-storage integrity and cleanup

V1 upgrade qualification reproduces false row/index-count comparisons for FTS
backing trees and unreclaimed backing pages after table/index teardown. The
[separate candidate](docs/proposals/fts-integrity.md) preserves physical checks,
fixes teardown enumeration, and passes affected native suites plus the complete
V1 upgrade/restore rehearsal. Its three-file patch SHA-256 is
`0a769b063076b4d110d6e75e0fcd334254663e89bb4dbe2e055da585ed9c474b`.
The user separately approved it on 2026-09-25. Isolated commit
`12109384ad868dd673a572305d45a816baa2e545` applies the exact reviewed patch; its
three-file diff has the same SHA-256. The integrated integrity, index-method and
drop-table suites pass (64 distinct tests), as does the complete native V1
upgrade/restore rehearsal. Core Clippy with warnings denied also passes. See
[active evidence](docs/v2-upgrade-restore-evidence.md).
This approval was separate from the later scalar-error and WASI FTS approvals. Existing orphaned pages from older development builds are not repaired
automatically; preserve the physical-corruption and drop/rollback regressions
when reviewing future upstream replacements.

## Approved core exceptions: scalar read errors and WASI FTS

The user explicitly approved both prepared proposals after reviewing the options.
The exact scalar-error patch is isolated in `f26014f04de4077a49268fd94c37ff9ad6ad6425`;
the exact opt-in WASI FTS core patch is isolated in
`2ef619c0704025512d9f4d4f5290dec3940a7361`. The separately reviewed frontend WASI
feature wiring is applied under `fastdb/frontend/Cargo.toml`. No dependency version
or storage-format changes are introduced by these patches. Candidate evidence is
in the respective reviews; integrated acceptance is recorded separately.

**Maintenance register:** [core exceptions](docs/core-exceptions.md) lists all
local exceptions, regression gates, and conditions for replacing or removing them.
Review every entry on every upstream sync. An upstream release note or a clean
patch application alone is not sufficient evidence to retain or remove a patch.

## Native-only release scope and browser removal

The user selected Rust, Node.js/TypeScript and Python clients and explicitly
requested removing browser support to reduce overhead. Isolated commit
`ae6777a17` reverts the WASI FTS core feature/cfg exception. The browser workspace
member, WASI QuickJS bindgen wiring, browser source/build/probe code and browser
release gates are removed. `fastdb-protocol` stays because Python depends on it.
Five active core exceptions remain. Earlier browser sections above are historical,
not current build instructions; see [removal evidence](docs/browser-removal.md).

Current Cargo.lock SHA-256: `eccecb92bd14841e0f0980680eb051f365db9454cbade2212a8ffdd80d3c6443`.
The removal drops `fastdb-browser` and bindgen 0.72.1, without adding/upgrading any
package identity. Native dependency declarations and notices are regenerated.

## Additional native language clients

The user requested PHP, Swift, C# and Go in place of browser support. The new
`fastdb-c` member is a native ABI over the FastDB frontend and shared protocol;
all wrapper code stays under `fastdb/bindings/`. No upstream core source or
third-party Rust package changed. Current lock SHA-256:
`1f64d71cc6cb57e691be4a7bc9b5a39ef4d00721515831813d1442aca2941aaa`.
See [client contracts and qualification](docs/native-language-clients.md).
