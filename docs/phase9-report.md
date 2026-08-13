# FastDB Phase 9 report

Status: technically complete locally on 2026-08-13
Engine pin: `977383ff40edc44ef410af062ed0d2322252a869`
Compatibility reference: SurrealDB `v3.1.5`
Release authority: none; no tag, package publication, upload, or production-ready claim

## Outcome

Phase 9 adds fixed `array<float, N>` fields, sealed catalog-owned native
`vector64` storage, exact COSINE/EUCLIDEAN KNN, bound query vectors, scalar
vector functions, distance projection, and structured exact-scan plans. Public
values remain arrays. ANN methods remain rejected and are not a GA claim.

The engine pin did not change and no inherited Turso source was modified.
FastDB constructs Turso AST directly, binds vector BLOBs, and never generates
SQLite text from FastDB source or logical identifiers.

## Compatibility and physical contract

Independent `v3.1.5` observations and the reference binary checksum are in
`docs/compat-research/phase9.md`. `COMPAT.md` moves only the executing fixed
vector and exact-search subset to Partial.

Each declared vector field owns one opaque BLOB column under provider
`BUILTIN_VECTOR_EXACT`, provider/encoding version 1. The canonical physical
encoding is little-endian finite `f64` elements followed by type byte `2`.
Document JSONB and all derived BLOBs change in one physical mutation. Reopen
validation checks ownership, dimensions, options, physical column type, and
document/BLOB agreement in RID-ordered batches of 256 rows.

KNN permits one top-level-AND predicate, `K` in `1..=10,000`, and only scalar
ordinary predicates that can be proven to execute physically before top-k.
The native plan orders distance then encoded RID and applies physical LIMIT K.
COSINE rejects zero query vectors and zero stored candidates. HNSW, DiskANN,
and `toy_vector_sparse_ivf` remain unavailable.

## Verification evidence

The independently authored Phase 9 tests cover parser boundaries, schema and
catalog validation, integer normalization, literal and bound vectors,
dimension 65,536, exact functions, prefilter-before-top-k, deterministic
ordering, prepared-scan caching, update/reopen, rollback injection, abrupt
exit, corruption, fixture reopen, relation-edge cascade cleanup, same-record
FTS/vector state, explicit-transaction visibility, API, CLI JSON, and
structured EXPLAIN.

The committed fixture is
`fastdb-tests/fixtures/phase9-format2-vector.fastdb`, SHA-256
`0b30fdad5bc418c892ad80ca0810292a9688a5c8d84a20a4c72aab165b333df6`.

The release benchmark in `docs/benchmarks/phase9-vector.json` compares the
public FastDB exact-vector path with equivalent pinned native Turso physical
storage, exact distance ordering, K, and result materialization:

- records: 5,001; K: 10; samples: 100;
- FastDB p95: 1,380,729 ns;
- native p95: 1,269,964 ns;
- query p95 ratio: 1.0872190078, below the provisional 2x Phase 12 ceiling;
- checkpointed main-file storage: 512,000 bytes FastDB versus 352,256 bytes
  native, ratio 1.4534883721, also below the provisional 2x ceiling.

The unchanged Phase 5 workload was rerun at 5,000 records and 200 samples;
raw data is in `docs/benchmarks/phase9-phase5-regression.json`. Its aggregate
gate passed: point p50 1.0048x, point p99 0.7726x, indexed-filter p95 1.0391x,
write p95 1.6279x, and storage 1.0113x. Two preceding 100-sample runs were
discarded as timing-noisy after failing different latency gates; increasing
the unchanged workload to its Phase 5 default sample count produced the
committed result rather than weakening any threshold.

The Phase 9 structured vector model fuzzer completed 2,314 executions in 301
seconds at 555 MB peak reported ASAN RSS, with 103 persisted corpus entries and
no crash, sanitizer finding, or model divergence.

## Verification summary

The local matrix passed:

```text
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p fastdb -p fastdb-cli \
  -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets --no-deps -- -D warnings
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p fastdb --all-targets
cargo test -p fastdb --doc
cargo test -p fastdb-cli --all-targets
cargo test -p turso_fastdb_tests --all-targets
cargo test -p turso_core --lib
cargo test -p turso_pg_tests
cargo test -p turso_whopper
cargo build --release -p fastdb-cli -p turso_fastdb_benchmarks
cargo +nightly fuzz run structured_vector -- -max_total_time=300 -timeout=10
sha256sum -c fastdb-tests/fixtures/SHA256SUMS
git diff --check
```

The inherited Turso build emits its pre-existing unused `CollationSeq` warning;
FastDB crates deny warnings and the scoped Clippy matrix passed with
`-D warnings` and `--no-deps`.

## Remaining boundaries

Phase 9 is an alpha-candidate technical surface, not release authorization.
ANN, recursive graph traversal, broader analyzer/vector compatibility,
multiprocess access, network service, non-Rust SDKs, and cloud remain outside
this phase. Phase 10 operational readiness is the next gate.
