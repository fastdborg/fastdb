# WASM dependency runtime evidence

2026-09-25. This closes an initial build/runtime feasibility step, not V2-C or a
browser release. The active core is unchanged. The standalone frontend probe
passes in Node WASI and in actual browser workers:

| Runtime | Result |
| --- | --- |
| Node 24.19.0 WASI preview1 | Passed |
| Chromium 149.0.7827.55, headless Linux | Passed |
| Firefox 151.0, headless Linux | Passed |

The same artifact checks a thrown C++ error crossing the CXX result boundary,
graph reuse afterward, managed ANN creation/search with exact expected ID and
distance, H3 cell `87658b314ffffff` for `(100,13)` at resolution 7, and a typed
JavaScript function returning the expected string. It uses an in-memory database.
The corresponding native Linux probe also passes; package Clippy/all targets
with warnings denied and Rust formatting pass. These are focused checks, not
full V2 acceptance or JavaScript failure/transaction qualification.

## Reproduction

- Rust 1.88.0, target `wasm32-wasip1-threads`.
- WASI SDK **33.0**, Linux x86_64 archive SHA-256
  `0ba8b5bfaeb2adf3f29bab5841d76cf5318ab8e1642ea195f88baba1abd47bce`,
  verified against its official release asset digest.
- libclang **18.1.1** for bindgen, installed in an isolated build environment.
- Frontend target-specific rquickjs-sys 0.12.2 `bindgen` feature; lockfile adds
  bindgen 0.72.1 and no existing dependency version upgrades. Native builds do
  not enable this feature or depend on bindgen 0.72.1.
- Browser harness pins Playwright 1.61.0, @tybys/wasm-util 0.10.1 and
  @emnapi/wasi-threads 1.1.0; pnpm lockfile is checked in.

```sh
WASI_SDK=/path/to/wasi-sdk-33.0 LIBCLANG_PATH=/path/to/libclang \
  bash fastdb/scripts/build-wasm-probe.sh
cd fastdb/docs/probes/wasm-browser
pnpm install --frozen-lockfile --ignore-scripts
node check.mjs /absolute/path/to/wasm_probe.stripped.wasm chromium
node check.mjs /absolute/path/to/wasm_probe.stripped.wasm firefox
```

The build script uses the development profile with Rust optimization level 1,
development assertions retained, no debug symbols and no LTO. C/C++ dependencies
use O2. It explicitly selects the threaded WASI C++ target, exception-enabled
libraries, standard WASM exception instructions, the unwind library and WASI's
memory-map emulation. The latter backs USearch allocation; this probe does not
use file-backed USearch mappings. A 32 MiB linear-memory stack, 64 MiB initial
memory and 2 GiB maximum are probe settings, not a final browser memory promise.

The browser harness serves only allowlisted local files on loopback and supplies
COOP/COEP headers. Both tested workers confirm `crossOriginIsolated`. It rejects
unexpected native imports instead of supplying dummy exception handlers.

Artifact SHA-256:
`3347bc5311a873f3e89a0b54e7382e656c4a93b0527dbe7b10c9a7e22640407f`.
This is a stripped optimized-development probe, not a distributed client binary.
Logs: `/tmp/fastdb-wasm-browser-optimized-build.log`,
`/tmp/fastdb-wasm-chromium-optimized.log`,
`/tmp/fastdb-wasm-firefox-optimized.log`,
`/tmp/fastdb-wasm-probe-native-control.log`,
`/tmp/fastdb-wasm-native-clippy.log`.

## Findings that changed the implementation

SDK 24 compiled a WASM artifact but left `__cxa_allocate_exception`/`__cxa_throw`
as unresolved imports. The checker rejects that artifact. SDK 33 supplies the
exception-enabled C++ runtime; see the official
[WASI C++ exception guidance](https://github.com/WebAssembly/wasi-sdk/blob/wasi-sdk-33/CppExceptions.md).
Compilation also requires overriding cc-rs's `-fno-exceptions` and using the
threaded target headers. Vendor sources were not modified.

Unoptimized QuickJS exceeded the existing 100 ms function budget in repeated
WASI runs. Optimizing its C code resolves the probe without changing the budget.
The initial Chromium run then hit the host call-stack limit in unoptimized Rust
code during H3 evaluation. The optimized development build passes the same
assertions in Chromium and Firefox. The failed controls remain in
`/tmp/fastdb-wasm-sdk24-control.log`, `/tmp/fastdb-wasm-repeat-control.log` and
`/tmp/fastdb-wasm-chromium.log`.

## Asynchronous reactor and candidate full-text support

The temporary command runner stalled at CREATE SEARCH INDEX because its JS
thread coordinator was blocked. A second-instance controller failed a Rust
pointer precondition during initialization. The asynchronous reactor resolves
both host issues: link Rust's `crt1-reactor.o`, export and call `_initialize`,
then dispatch database work onto a Rust thread while JS handles nested worker
requests. Omitting reactor initialization caused the initial asynchronous run to
time out. No checks or function time limits were relaxed.

`frontend/examples/wasm_async_probe.rs` is built with:

```sh
WASI_SDK=/path/to/wasi-sdk-33.0 LIBCLANG_PATH=/path/to/libclang \
  bash fastdb/scripts/build-wasm-probe.sh wasm_async_probe
```

An unapproved, narrowed [core proposal](proposals/fts-wasi.md) enables FTS only
with an explicit `fts_wasi` feature on WASI. It supersedes the broad historical
[compilation experiment](probes/fts-wasm-experiment.patch). Both remain outside
the active core; passing candidate tests does not authorize integration.

The shared `wasm_fulltext_probe.rs` passes natively and with the candidate in
**Chromium 149.0.7827.55 and Firefox 151.0**. It checks index creation, ordered
results, insert/update/delete maintenance, nested savepoint/transaction rollback,
reader snapshots and exact scores across a writer commit, integrity audit,
index drop/recreation and collection drop. The base ANN/H3/JavaScript/C++ checks
also pass in the same reactor artifact.

The strict pool of four workers failed with a segment-merging thread allocation
error. A pool of eight passes both browsers with peak eight active Rust threads
and zero remaining at completion. The coordinator waits for the reporting
thread's cleanup event before releasing the pool. This bounds the fixture;
it does not establish the capacity of a public concurrent-query client.

```sh
# Candidate core and WASI-only frontend feature wiring must be applied separately
# in the temporary checkout; these commands alone do not enable active-core FTS.
node check.mjs /absolute/path/to/candidate.wasm chromium 8 fulltext
node check.mjs /absolute/path/to/candidate.wasm firefox 8 fulltext
```

Candidate artifact `/tmp/fastdb-wasm-fts-reactor-qualified.wasm`: 25,293,870 bytes,
SHA-256 `fe9fe9ea2b3429ec9753d997ea235e41fe4c4d7b7e826f0ba0748ca9933204ad`.
Logs: `/tmp/fastdb-wasm-fts-reactor-qualified-build.log`,
`/tmp/fastdb-wasm-fts-reactor-qualified-chromium.log`,
`/tmp/fastdb-wasm-fts-reactor-qualified-firefox.log` and
`/tmp/fastdb-wasm-fulltext-native-fixture.log`.
Earlier failed controls remain in `/tmp/fastdb-wasm-fts-runtime-exports.log`,
`/tmp/fastdb-wasm-fts-controller.log`, `/tmp/fastdb-wasm-async-chromium.log`,
and `/tmp/fastdb-wasm-fts-async-{chromium,firefox}.log`.

The current active-core asynchronous artifact also passes the base checks in
Chromium and Firefox, with one peak worker and zero left active. Its full-text
control fails explicitly with `unknown module name 'fts'`, then cleans up the
worker. This confirms that the passing candidate result depends on the proposed
FTS feature and that the active tree does not claim it.
Artifact `/tmp/fastdb-wasm-reactor-active.wasm`: 21,485,631 bytes, SHA-256
`bfe9f85783e019022dd7c4b3f711d23883da2bab8003428480dd54d5bf9b8be7`.
Logs: `/tmp/fastdb-wasm-reactor-active-{build,chromium,firefox}.log` and
`/tmp/fastdb-wasm-reactor-active-fts-control.log` (expected failure).

The narrowed candidate also passes 27 native engine FTS tests and all six managed
full-text integration tests. Active frontend Clippy/all targets with warnings
denied, Rust formatting and browser harness syntax checks pass. Exact commands
and logs are recorded in the [proposal](proposals/fts-wasi.md).

Remaining V2-C work includes the public browser binding, capability contract,
complete worker lifecycle and resource limits, OPFS durability/reopen/recovery,
cancellation and catalog interoperability. V2-R owns final artifacts, platform
coverage, complete notices and installed-client acceptance.
