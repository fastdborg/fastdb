# Turso 0.8.1 integration checklist

Scope: user-requested embedded upgrade from 0.7.2; preserve current FastDB work,
SQL/document contracts, native clients and approved engine exceptions. Cloud
source/deployment and release publication are excluded.

- [x] Read release notes, fetch tag and inspect clean source checkout.
- [x] Create `sync/turso-0.8.1` from `ab3a465f7` with an ancestry-preserving merge.
- [x] Resolve upstream integration and review all eight engine exceptions.
- [x] Adapt engine values, parser AST and execution APIs to 0.8.1.
- [x] Qualify explicit concurrent transactions and transactional managed FTS.
- [x] Expose/test new SQL language and optimizer capabilities through FastDB.
- [x] Verify old databases, FTS migration, rollback, cancellation and recovery.
- [x] Run scoped formatting, Clippy, native tests and binding smoke checks.
- [x] Record exact provenance, exception decisions and test evidence; audit CI.

Upstream tag object: `664022bdc83c2c1a5d245605388843693e3d0142`.
Release commit: `8549c16595d2faf1bdd6ee24aee0be8bfabb3d4a`.
The original checkout branch is `feat/cloud-cli-0.3`; all its committed work is
included, including metering and CLI work. Only FastDB CI may remain active.

## Validation environment and receipts

Linux x86_64; Rust 1.88.0; Node 24.19.0; Python 3.12; pnpm 11.23.0.
Default debug profiles and the manifests' selected features; no workspace-wide,
all-features, release or browser qualification. Lockfile SHA-256:
`16c3d2397f7d50bf03966506f42c13f77aa946516d02542966c5a51187bf1061`.

| Check | Result / receipt |
|---|---|
| Scoped format, all eight FastDB packages | Passed (`/tmp/fastdb-upgrade-format-final.log`) |
| Scoped Clippy, all eight packages, all targets, no deps, deny warnings | Passed (`/tmp/fastdb-upgrade-clippy2.log`) |
| Full eight-package Rust test run | 830 passed; two frontend unit follow-ups below (`/tmp/fastdb-upgrade-full3.log`) |
| Frontend unit sweep after callback-oracle update, single test thread | 89 passed; all 832 scoped Rust tests resolved (`/tmp/fastdb-upgrade-unit4.log`) |
| Native index-method suite | 131 passed, including FTS concurrent writers, snapshot isolation, recovery and the new savepoint-pin regression |
| Native trigger / integrity / external API suites | 98 / 36 / 14 passed |
| Native statement lifecycle / savepoint filter / first FULL commit | 46 / 12 / 1 passed |
| Standalone Rust consumer, offline path dependency | Passed; 341 registry/git identities remain within the workspace lockfile (`/tmp/fastdb-upgrade-rust-consumer4.log`) |
| Node sync/worker, ownership and task-tracker suites | 121 passed in full run; final bounded-source fixture test passed separately: 122 resolved (`/tmp/fastdb-upgrade-node2.log`, `node-write-retry2.log`) |
| pnpm-packed Node consumer installation, offline | Passed (`/tmp/fastdb-upgrade-node-package3.log`); development addon only, not release packaging qualification |
| Node declaration checking | Passed (`/tmp/fastdb-upgrade-types2.log`) |
| C ABI / Python / PHP | 5 tests / 8 tests / 42 contract steps passed |
| Raw engine probes | WAL first-write sync, FTS integrity/drop lifecycle, and 18 scalar-error transaction scenarios passed |
| FastDB package identity check | Eight crate identities remain consistent; engine manifest points to the new SHA with development status |

The full Rust run's two follow-ups were a test-only callback-count oracle that
assumed identical relational/document plans and a JavaScript setup deadline hit
during parallel compilation. Callback tests still compare native results,
require no evaluation during EXPLAIN, check LIMIT 0, and compare normal/profile
execution counts. The final serial unit sweep verifies the complete unit suite.
The native integrity harness retains the newer rusqlite security baseline; its
SQLite oracle now accepts the exact datatype constraint that newer SQLite emits
while evaluating an invalid STRICT generated column. Turso assertions are intact.

Full scoped command:

```sh
cargo test --locked -p fastql-parser -p fastdb -p fastdb-cli -p fastdb-tests \
  -p fastdb-node -p fastdb-python -p fastdb-protocol -p fastdb-c --no-fail-fast -j 8
cargo test --locked -p fastdb --lib -j 8 -- --test-threads=1
```

Native engine commands use `cargo test --locked -p core_tester --test
integration_tests <filter>` with `index_method::`, `trigger::`,
`integrity_check::`, `external_apis::`, `savepoint` and `first_full`; lifecycle
uses `cargo test --locked -p turso_core --lib statement_lifecycle_tests`.
Core receipts are `/tmp/fastdb-upgrade-core-{fts2,trigger,integrity2,external_apis,
savepoint,first_full,lifecycle2}.log`.

Go, Swift and .NET toolchains were unavailable here. Their adapters are unchanged;
C ABI/protocol checks cover the shared boundary, but do not substitute for those
language runtime tests. Binary release qualification/publication is separate.

## Dependency and workflow review

Preserved earlier security updates: rustls 0.23.45, aws-lc-rs 1.18.1,
aws-lc-sys 0.45.0, crossbeam-epoch 0.9.20, memmap2 0.9.11, rusqlite 0.40.2,
and rquickjs 0.13.0. Tantivy advances to upstream 0.26.2. No retained package
was silently downgraded to the release tag's older security baseline.

Only `fastdb-ci.yml` remains active. Newly imported inherited
`publish-serverless-python.yml` and `serverless.yml` were relocated to
`.github/upstream-workflows/` before push. Exact upstream CI links and the
per-exception retain/adapt/remove decisions are in [UPSTREAM.md](../UPSTREAM.md)
and [the exception register](core-exceptions.md).

The old database fixture comes from the actual published FastDB 2.1.0 binary;
[its receipt](../tests/fixtures/README.md) records both archive and database hashes.
The user selected upstream rejection of trailing DML LIMIT and separately
approved [the exact FTS cursor patch](proposals/turso-081-fts-savepoint.md).
