# Dependency security review for 2.1

Reviewed 2026-09-25 for the Linux x64 candidate. This is a scoped advisory and
applicability review, not a source-code security audit or a claim that the
application has no vulnerabilities. Maintainer responsibilities and private
reporting are in [operations](operations.md).

## Rust dependency closure

The [machine-readable receipt](dependency-security-rustsec.json) records the
exact lockfile checksum, source revision, package identities, advisory database
revision, findings and excluded workspace findings. The source checkout had
uncommitted candidate changes during review; the lockfile checksum, not the
pre-existing HEAD alone, identifies the reviewed dependency versions.

Tool: official RustSec `cargo-audit 0.22.2` Linux x86_64 binary. Download archive
SHA-256: `ab28a1bdb54db4d5d8ad5981cf1f959410370b3d28250dbd35f6a44248620e39`.
Advisory repository: `https://github.com/RustSec/advisory-db`, pinned commit
`913a741345c1df04dd8ee83f4304f439caa30ccc`, 1269 advisories. Network refresh and
yanked-package checks are disabled during the pinned scan.

`cargo tree --locked` selects normal and build dependencies for `fastdb-cli`,
`fastdb-node`, `fastdb-python` and `fastdb-c` on
`x86_64-unknown-linux-gnu`. These include the shared FastDB frontend used by Rust
and the C library used by PHP/Swift/C#/Go. The union contains 354 package/version
identities, including local packages. Package inclusion is conservative evidence
of possible reachability, not proof that every function is linked or called.
Dev-only and other-target dependencies are excluded. Unrelated upstream
workspace findings remain visible in the receipt and are not silently ignored.

After the targeted updates below, the shipping closure has **zero RustSec
vulnerability findings** and one informational unsoundness finding with a
documented non-applicability decision. This statement is limited to the pinned
database and dependency closure.

| Component | Change | Reason |
|---|---|---|
| crossbeam-epoch | 0.9.18 → 0.9.20 | [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204.html), invalid pointer dereference in pointer formatting; pulled through engine/FTS dependencies. |
| rustls | 0.23.35 → 0.23.45 | [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html), TLS 1.3 encryption-level validation; CLI HTTP support uses reqwest/rustls. |
| memmap2 | 0.9.5 → 0.9.11 | [RUSTSEC-2026-0186](https://rustsec.org/advisories/RUSTSEC-2026-0186.html), range validation in mapping advice/flush operations. Tantivy brings this dependency; update avoids retaining the affected library even though FastDB uses its own FTS directory. |
| rusqlite / libsqlite3-sys | 0.37.0 / 0.35.0 → 0.40.2 / 0.38.2 | The newly shipped CLI importer needs SQLite's current fixes; bundled SQLite changes 3.50.2 → 3.53.2. |
| rquickjs / core / sys | 0.12.2 → 0.13.0 | Bundled QuickJS-NG changes 0.15.1 → 0.16.2, including the security fixes below. |

The rustls update also resolves aws-lc-rs 1.18.1, aws-lc-sys 0.45.0 and
rustls-webpki 0.103.15. The rusqlite update resolves hashlink 0.12.2 plus
non-Linux SQLite WASM support packages in the workspace lockfile; those WASM
packages are absent from the Linux shipping closure. This does not reinstate a
FastDB browser target. No upstream core implementation files were changed.

Declared minimum Rust versions are at most 1.87 for the updated rquickjs crate,
1.85 for hashlink, 1.71 for the TLS dependencies, 1.65 for memmap2 and 1.61 for
crossbeam-epoch. Rusqlite's [0.40.2 release](https://github.com/rusqlite/rusqlite/releases/tag/v0.40.2)
explicitly restores Rust 1.88 support. Actual compilation and tests use the
project's pinned Rust 1.88.0; declarations alone are not compilation evidence.

### Retained informational finding: lru 0.16.4

[RUSTSEC-2026-0253](https://rustsec.org/advisories/RUSTSEC-2026-0253.html) requires
`LruCache::pop()` to unwind through a key's panicking `Drop` implementation, then
further access to the damaged cache. The only `lru::LruCache` instantiation in
pinned Tantivy 0.26.1 is `LruCache<usize, Block>` in `src/store/reader.rs`.
It calls `get` and `put`, not `pop`; `usize` has no destructor that can panic.
The FastDB/engine sources contain no direct use of this external `lru` crate.
The engine's separately implemented `LruCache` in `core/index_method/fts.rs` is
not this dependency.

The advisory's required trigger is therefore unavailable in this shipping
integration. Retain 0.16.4 without modifying upstream manifests. The advisory is
explicitly accepted only as informational in the scan command; vulnerability
findings cannot be accepted through that flag. FastDB maintainers must revisit
this analysis whenever Tantivy, cache key types/call sites or the lru dependency
changes, and at each release/upstream sync. Patched upstream lru starts at
0.18.2; remove this exception when the pinned dependency moves to a fixed version.

## Bundled native libraries

The [native review receipt](dependency-security-native.json) records retrieved
advisory identities/ranges, source URLs and exact bundled versions.

QuickJS-NG 0.15.1 was affected by published RegExp allocation-failure memory
corruption, JSON rope-indentation disclosure, and error-stack/Promise OOM
use-after-free issues. Memory-limited UDF execution does not make allocation
failure irrelevant. Rquickjs 0.13.0 bundles QuickJS-NG 0.16.2, verified from
`rquickjs-sys-0.13.0/quickjs/quickjs.h`; the listed upstream fixes are in 0.16.0.
All ten published repository advisories were inspected, including older
array/proxy/typed-array fixes and their version/architecture restrictions.
See [QuickJS-NG advisories](https://github.com/quickjs-ng/quickjs/security/advisories).

The new `patched_runtime_preserves_rope_json_indentation` regression exercises
the actual JSON indentation disclosure trigger and checks the runtime reported
by `INFO FOR FUNCTION`. All five `user_functions` integration tests pass under
Rust 1.88.0, including bounded heap/stack failure, cancellation, preserved caller
transactions and snapshot/definition persistence. This is frontend integration
evidence, not an ASan reproduction of every upstream memory-corruption case.

The CLI's SQLite 3.53.2 includes fixes for corrupt-FTS5 CVE-2025-7709 and the
FTS5/defensive-mode CVE-2026-11822 and CVE-2026-11824. The importer additionally
restricts accepted schemas and applies defensive settings. The source ID and
amalgamation checksum are in [the SQLite notice](sqlite-notice.md). This SQLite
library is specific to adoption; FastDB query execution remains on the pinned
Turso engine. See [SQLite's vulnerability table](https://sqlite.org/cves.html).

USearch remains 2.26.2. Its public repository advisory endpoint returned an empty
list on the review date. That is recorded evidence of the inspected channel,
not proof of absence of vulnerabilities. Continue reviewing dependency and
upstream changes before each release.

`pnpm --dir fastdb/bindings/node audit --json` returned no advisories for the
package's sole development dependency, TypeScript 5.8.3; it declares no Node
runtime dependencies. This result does not cover the native addon. Rust
toolchain binaries, Maturin, application-selected language runtimes and
dynamically linked Ubuntu libraries remain separate maintained prerequisites,
not packages certified by this crate scan.

## Repeatable scan

Use the configured Rust 1.88 toolchain and a clean checkout of the desired
advisory database revision. Run from the repository root:

```sh
python3 fastdb/scripts/check-dependency-advisories.py \
  --database /path/to/advisory-db \
  --cargo-audit /path/to/cargo-audit \
  --allow-informational RUSTSEC-2026-0253 \
  --output fastdb/docs/dependency-security-rustsec.json
```

The command fails for any unresolved finding in the shipping closure. Run again
if dependencies change before packaging, and include the receipt with release
evidence. Regenerate all four native dependency inventories/notices after any
lockfile change; published artifacts retain their own immutable provenance.
