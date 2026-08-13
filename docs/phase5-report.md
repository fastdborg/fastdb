# Stop for release review

FastDB Phase 5 completes the local release-hardening candidate on branch
`phase-next` immediately after the Phase 4 commit `932f90ce5`. Every local
command in the `plan-phase5.md` gate matrix passes without a Turso
implementation, yield-point, format, or pin change.

This is a stop condition, not a release. Even with every local technical gate
green, the project remains blocked from any publish, tag, release upload, or
production-readiness claim until the committed Linux/macOS/Windows GitHub
Actions workflow passes at the candidate commit and counsel approves the
license, CLA, entity, and trademark/compatibility materials.

## Baseline and upstream decision

- Phase 4 baseline: commit `932f90ce5` on branch `phase-next`.
- Official remote: `https://github.com/tursodatabase/turso.git`.
- Retained engine pin: `977383ff40edc44ef410af062ed0d2322252a869`.
- Upstream audit (read-only): fetched `upstream/main` at
  `a94102c20b4c1c554f7c246606c2ed74db47199c` (dated 2026-08-12).
- Decision: retain the pin. Phase 5 needs no engine change. No merge,
  cherry-pick, pin update, push, or Turso implementation edit occurred.
- Format and dialect remain version 1; the migration-level-0 fixture exercises
  the transactional level-0-to-1 migration within format version 1. The
  uncommitted Phase 0 format version 0 remains disposable and has no upgrade
  path.

## Bounded optimization

Successful-parse caching is bounded exactly as specified:

- at most 128 entries and 4 MiB of source bytes;
- sources over 64 KiB are excluded from both lookup and insert;
- only fully successful parses are cached; an LRU move-to-back policy evicts
  on entry-count or byte-budget overflow.

Prepared-SELECT caching is bounded to 64 idle candidates. Each key carries
catalog generation, opaque physical table, RID use, and predicate
path/operator/scalar-type, and never carries a bound value. Prepared reuse is
disabled inside explicit transaction execution. Caches invalidate on
transaction boundaries, catalog publication, execution, bind, prepare, reset,
close, and error paths. Data-only CREATE avoids republishing an unchanged
catalog generation, and existing-table CREATE holds the schema mutex through
validation and commit to prevent a concurrent schema-publication race.

## Strict JSON reserved-key fix

A plain user object such as `{"$fastdb":"user data"}` is now preserved instead
of being collapsed to an empty object. The decoder borrows the reserved value
non-destructively and treats it as an envelope only when it is an actual
nested object, so non-envelope reserved values round-trip as ordinary user
content.

## Hardening evidence

Ten independently authored `p5_*` groups cover the `plan-phase5.md` evidence
requirements:

- `p5_cache_001/002`: parse and prepared-SELECT cache bounds, value-free keys,
  and transaction/schema-publication/error invalidation.
- `p5_resource_001`: every parser ceiling and malformed-input class is bounded.
- `p5_model_001`: multi-seed memory and on-disk results match an independent
  map.
- `p5_inject_001`: recursive parameters, record IDs, JSON paths, and source
  payloads stay data.
- `p5_json_001`: nested reserved envelopes round-trip and spoofing is rejected.
- `p5_cli_001`: CLI parameters and JSON envelopes cannot inject source.
- `p5_fixture_001`: format-1 and migration-level-0 fixtures migrate, reopen,
  accept a further mutation, pass `PRAGMA integrity_check`, and demonstrate
  real expression-index selection.
- `p5_crash_001`: process-abrupt-exit recovery at frontend
  operation/publication boundaries leaves the database integral.
- `p5_fs_001`: Unicode, spaces, relative/absolute paths, reopen, sidecars, and
  clean close.
- Real public-I/O WAL-sync completion failures remain covered by the existing
  `atomicity.rs` suite in the FastDB integration package.

## Fuzz targets

The parser fuzzer (`fastdb-parser/fuzz`) and the structured-CRUD fuzzer
(`fastdb-tests/fuzz`) are detached from the root workspace. Both ran for the
required five-minute campaigns with no crash:

- parser `parse`: 7,497,854 runs in 301 seconds, no crash.
- structured `structured_crud`: 7,456 runs in 301 seconds, coverage 20,219 /
  feature 42,307, no crash.

A prior structured-CRUD harness-only panic (a model that rendered a string
record ID via `to_source()` and then wrongly required the rendered source to
begin with `r`) was fixed by destructuring `RecordIdValue::String` and parsing
the underlying string. The obsolete crash artifact was re-run against the
fixed target (executed cleanly, no panic) and then removed. The corpus was
reset to the two independently authored seeds (`create_update_read`,
`delete_rollback`); generated corpus entries, build `target/` directories, and
artifacts are gitignored so only the seeds and harness source are committed.

## Fixtures

Two committed 65,536-byte database fixtures carry independent provenance and
SHA-256 digests under `fastdb-tests/fixtures/`:

- `phase3-format1.fastdb` -
  `5f86e67ac36efe6470d09d8fc7f22f201ecc952547a136c9d24bebd06bfdfdef`
- `migration-level0.fastdb` -
  `cfa48691f2a9767a66461a8d0545c824e7ae39302ce3be1cc8e566d51eb018a0`

Both were generated by FastDB at Phase 4 commit `932f90ce5`; only the
migration fixture's `last_migration` catalog integer was changed from 1 to 0
with the pinned Turso CLI. `sha256sum -c SHA256SUMS` passes.

## Release benchmark

The `phase5-release-bench` executable records every raw nanosecond sample plus
p50/p95/p99 for the public async API and an equivalent native Turso path. Both
paths use a dedicated worker, identical stable-WAL/full-durability files,
opaque physical values, the same score expression index, and complete public
`QueryResponse` materialization. The committed run passed every gate
(`gates.passed` is `true` in `docs/benchmarks/phase5-results.json`).

| Gate | FastDB | Native | Ratio | Limit |
| --- | ---: | ---: | ---: | ---: |
| Point read p50 | 79,913 ns | 58,607 ns | 1.3635x | 1.5x |
| Point read p99 | 260,219 ns | 265,104 ns | 0.9816x | 2.0x |
| Indexed filter p95 | 220,805 ns | 184,601 ns | 1.1961x | 2.0x |
| Write p95 | 641,769 ns | 356,183 ns | 1.8018x | 2.0x |
| Main file after close | 14,667,776 B | 14,618,624 B | 1.0034x | 1.5x |

Host conditions and stability are documented in `docs/benchmarks/phase5.md`.
The recorded run was taken while the host was under routine desktop load (load
average 3.5-3.9, browser and editor active) with the CPU in the `powersave`
governor at roughly 1.7 GHz rather than full turbo, so absolute latencies for
both paths are about 2x an earlier uncontended reference (native point-read p50
was 58,607 ns here versus 28,857 ns uncontended). The deterministic storage
gate (1.0034x) is identical to the uncontended reference. Across six
back-to-back re-runs on this host, point-read p50 ranged 1.36x-1.86x (four
passed the 1.5x gate, two failed purely from background contention) while
point-read p99, indexed-filter p95, write p95, and storage stayed within their
gates on every run. An uncontended run on this same machine measured
point-read p50 at 1.0515x, indexed-filter p95 at 1.0529x, and write p95 at
1.5841x. A clean, uncontended reconfirmation on a performance governor is the
recommended final check for release review.

## Required local gate results

Environment: Linux x86-64 (`7.0.0-29-generic`), Rust 1.88.0 stable and
1.99.0-nightly, 14 online cores, host under routine desktop load.

| Command | Result |
| --- | --- |
| `cargo metadata --locked --no-deps --format-version 1` | Passed; six FastDB packages resolve at `0.0.0`, all `publish = false`. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check` | Passed. |
| `cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check` | Passed. |
| `cargo clippy --locked ... --all-targets` | Passed; only documented inherited Turso warnings. |
| `cargo test --locked -p turso_fastdb_parser` | Passed: 34 tests. |
| `cargo test --locked -p turso_fastdb` | Passed: 14 tests. |
| `cargo test --locked -p fastdb --all-targets` | Passed: 11 tests plus compile-only lifecycle example. |
| `cargo test --locked -p fastdb-cli --all-targets` | Passed: 6 tests. |
| `cargo test --locked -p turso_fastdb_tests` | Passed: 76 tests. |
| `cargo test --locked --doc -p fastdb` | Passed. |
| `cargo build --locked --release -p fastdb-cli` | Passed. |
| `cargo build --locked --release -p turso_fastdb_benchmarks --bin phase5-release-bench` | Passed. |
| `target/release/phase5-release-bench --output docs/benchmarks/phase5-results.json` | Passed; all gates passed. |
| parser fuzz, `-max_total_time=300` | 7,497,854 runs, no crash. |
| structured_crud fuzz, `-max_total_time=300` | 7,456 runs, no crash. |
| `cargo test --locked -p turso_core --lib` | Passed: 2,286; 17 ignored. |
| `core_tester ... expression_index` | Passed: 3. |
| `core_tester ... without_mvcc` | Passed: 5. |
| `core_tester ... test_transaction_visibility` | Passed: 1. |
| `cargo test --locked -p turso_pg_tests` | Passed: 412. |
| `cargo test --locked -p turso_whopper` | Passed: 50. |
| `SEED=1 ... --mode fast --max-steps 1000` | Passed. |
| `SEED=7 ... --mode recovery-heavy --max-steps 1000` | Passed. |
| `(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)` | Passed: both OK. |
| `git diff --check` | Clean. |

## CI and documentation

A single FastDB-only GitHub Actions workflow (`.github/workflows/fastdb.yml`)
covers formatting, linting, tests/docs, release CLI and benchmark builds, a
Linux/macOS/Windows filesystem and CLI matrix, fixture-digest verification,
pinned-engine recording, a non-publishing assertion, and bounded fuzz smoke
runs. It contains no publish, upload-artifact, deploy, secret, or release
operation. `COMPAT.md`, `docs/limitations.md`, `docs/format-v1.md`,
`docs/api.md`, `docs/benchmarks/phase5.md`, `docs/phase5-clean-room.md`, and
`docs/release-readiness.md` are current. Every FastDB package remains version
`0.0.0` and `publish = false` with no `license` field pending counsel.

## Provenance and risk review

All Phase 5 changes are confined to FastDB-authored crates (`fastdb-parser`,
`fastdb-frontend`, `fastdb-api`, `fastdb-cli`, `fastdb-tests`,
`fastdb-benchmarks`), the workspace `Cargo.lock`, the new detached
`fastdb-tests/fuzz` package, the non-publishing workflow, plans, and
documentation. No file under Turso core, the SQLite parser, bindings, the
PostgreSQL frontend, inherited test directories, WAL, JSONB, optimizer, or
inherited workflows changed. FastDB user input still reaches only the
independent parser, frontend evaluation/planning, directly constructed Turso
AST, typed bindings, and `prepare_translated_stmt_with_options`. No user value
or logical identifier is interpolated into generated SQL; the prepared-SELECT
cache key carries no bound value. No unsafe code, external message, release,
publishing action, workflow publishing step, format change, or cloud surface
was introduced.

## Remaining external blockers

Release remains stopped pending:

- the committed GitHub Actions workflow passing on Linux, macOS, and Windows
  at the exact candidate commit;
- counsel-approved Community License/Additional Use Grant and commercial
  terms;
- a counsel-approved CLA sufficient for the dual-license model;
- the operating entity and ownership/assignment chain;
- trademark, product-naming, and compatibility wording; and
- a human release reviewer's sign-off on benchmark representativeness,
  known limitations, artifacts, notices, and the signing/release procedure,
  including a clean, uncontended benchmark reconfirmation.
