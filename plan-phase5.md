# FastDB Phase 5 — Release Hardening

Status: authoritative execution plan, 2026-08-13

## 1. Objective and boundary

Phase 5 starts from Phase 4 commit `932f90ce5` and hardens the local MVP for
release review. It retains Turso pin
`977383ff40edc44ef410af062ed0d2322252a869`, format/dialect version 1,
stable WAL/full durability, the independent FastDB parser, and direct
translated-Turso-AST architecture.

The upstream audit is read-only. A pin change, Turso core/yield-point change,
Cloud C0/C1, server, SDK, broader SurrealQL, publishing, or public release is
outside this phase and is a stop condition. Remote CI and legal approvals are
external release gates, not local implementation work.

## 2. Bounded optimization

- Cache only fully successful parses, at most 128 entries and 4 MiB of source
  bytes; never cache a source over 64 KiB.
- Cache at most 64 idle prepared SELECT candidates. Keys contain catalog
  generation, opaque physical table, RID use, predicate structure/operator,
  and scalar type, never bound values.
- Do not use prepared SELECT caching inside explicit transactions. Drop idle
  statements at transaction boundaries and invalidate on catalog publication,
  execution/reset/bind failure, close, or an unusable cache state.
- Data-only mutation must not publish an unchanged catalog generation.

## 3. Hardening evidence

Maintain independently authored `P5-*` groups for cache bounds/invalidation,
malformed/resource-ceiling input, multi-seed model equivalence in memory and
on disk, recursive injection boundaries, strict JSON, CLI input, format-1 and
migration-0 fixtures, process-abrupt-exit recovery, real public-I/O WAL
failure, filesystem paths, reopen, integrity, and actual index selection.

Keep parser and structured CRUD libFuzzer targets detached from the root
workspace. Exercise Turso's existing simulator/yield/failure machinery
unchanged. Do not add, remove, or reorder an engine yield point. FastDB crash
helpers may stop only at test-only frontend operation/publication boundaries.

Committed database fixtures must carry independent provenance and SHA-256
digests. They must reopen, migrate where applicable, accept a new mutation,
pass `PRAGMA integrity_check`, and demonstrate expression-index selection.

## 4. Release benchmark

The `phase5-release-bench` executable records every raw nanosecond sample plus
p50/p95/p99 for the public async API and an equivalent native Turso path. Both
paths use a dedicated worker, identical stable-WAL/full-durability files,
physical values, indexes, result materialization, warmup, and cache policy.
The native path bypasses only FastDB parsing/planning/catalog resolution.

Required gates:

- point read: FastDB/native <= 1.5x p50 and <= 2.0x p99;
- indexed filter: <= 2.0x p95;
- write: <= 2.0x p95;
- checkpointed main database bytes: <= 1.5x native.

Preserve the JSON result and environment/methodology under
`docs/benchmarks/`.

## 5. CI and documentation

Add one FastDB-only GitHub Actions workflow covering formatting, linting,
tests/docs, release CLI/benchmark builds, Linux/macOS/Windows filesystem and
CLI behavior, fixture digests, and bounded fuzz smoke runs. It must contain no
publishing, upload, deployment, secret, or release operation.

Finalize `COMPAT.md`, limitations, format/upgrade policy, benchmark evidence,
clean-room record, and release-readiness checklist. Every FastDB package stays
version `0.0.0` and `publish = false`.

## 6. Local verification gates

```sh
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy --locked -p turso_fastdb_parser -p turso_fastdb -p fastdb \
  -p fastdb-cli -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets
cargo test --locked -p turso_fastdb_parser
cargo test --locked -p turso_fastdb
cargo test --locked -p fastdb --all-targets
cargo test --locked -p fastdb-cli --all-targets
cargo test --locked -p turso_fastdb_tests
cargo test --locked --doc -p fastdb
cargo build --locked --release -p fastdb-cli
cargo build --locked --release -p turso_fastdb_benchmarks --bin phase5-release-bench
target/release/phase5-release-bench --output docs/benchmarks/phase5-results.json
(cd fastdb-parser && cargo +nightly fuzz run parse -- -max_total_time=300)
(cd fastdb-tests/fuzz && cargo +nightly fuzz run structured_crud -- -max_total_time=300)
cargo test --locked -p turso_core --lib
cargo test --locked -p core_tester --test integration_tests expression_index
cargo test --locked -p core_tester --test integration_tests without_mvcc
cargo test --locked -p core_tester --test integration_tests test_transaction_visibility
cargo test --locked -p turso_pg_tests
cargo test --locked -p turso_whopper
SEED=1 cargo run --locked -p turso_whopper -- --mode fast --max-steps 1000
SEED=7 cargo run --locked -p turso_whopper -- --mode recovery-heavy --max-steps 1000
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
git diff --check
```

## 7. Definition of Done and release stop

The phase is locally complete only when all bounded-cache semantics,
hardening groups, fuzz targets, fixtures, crash/recovery checks, unchanged
Turso regressions/simulators, benchmark ratios, documentation, provenance,
and non-publishing CI checks pass without a Turso implementation change.

`docs/phase5-report.md` must lead with `Stop for release review`. Even after
local technical gates pass, release remains blocked until the committed remote
Linux/macOS/Windows workflow succeeds and counsel approves the license, CLA,
entity, and trademark/compatibility materials.

The Phase 5 upstream audit retained the pin. Fetched `upstream/main` was
`a94102c20b4c1c554f7c246606c2ed74db47199c`; no merge or pin change is part
of Phase 5.
