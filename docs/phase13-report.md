# FastDB Phase 13 report

Status: technically complete locally on 2026-08-14

Starting checkpoint: `9627c82cd`

Authoritative plan commit: `9f1c1bbdc`

Ending implementation checkpoint: `f07501778`

Engine pin: `977383ff40edc44ef410af062ed0d2322252a869`

Compatibility reference: SurrealDB `v3.1.5`

Release authority: none

## Outcome

Phase 13 adds the shared bounded expression evaluator and context-neutral
built-in function surface without changing Turso core, the format-3 storage
contract, stable-WAL serialized writers, or the public release boundary.
Independent parser nodes now preserve postfix access, indexing and slicing,
casts, ranges, closures, the characterized operators, NONE/NULL distinctions,
and namespaced values/functions. Evaluation uses checked arithmetic, finite
numbers, bounded collections/strings, lexical closure bindings, explicit
short-circuiting, and transaction poisoning on every failure.

The closed registry implements the characterized array, object, set, value,
bytes, encoding, digest, password, random, duration, time, math, parsing,
string, validation, semantic-version, type, record, vector, and context-safe
projection helpers. `value::diff` and `value::patch` are bounded, ordered, and
apply to a clone before publication. Password functions retain the reference
Argon2id, bcrypt, PBKDF2, and scrypt work factors and redact inputs. Random
functions use the process CSPRNG. JSON/CBOR typed values use the collision-safe
format-3 public mapping.

The locked matrix now contains 479 Supported, zero Partial, and 277
Unsupported rows. Of the 487 Phase 13 targets, 408 are Supported and 79 have
the explicit stop analysis in `docs/phase13-architecture-stops.md`. The stops
cover immutable-reference spelling mismatches, values outside the public
format contract, and functions requiring statement, database/graph,
authenticated session/request, network/filesystem capability, or asynchronous
cancellation context. No weaker ambient-authority implementation is exposed.
Later owning phases may reopen a row only with executable evidence and the
required atomicity, authorization, resource, and recovery tests.

## Correctness and compatibility evidence

The full FastDB matrix passed after correcting two stale integration
assumptions found by the gate: the public API resource walker now descends into
closure bodies, and the historical Phase 6 test recognizes that promoted
`search::score` is a known function with a schema/context error rather than
unknown syntax. All Phase 0–12 migration, model, crash, atomicity, transaction,
API, CLI, graph, FTS, vector, backup/check, and fixture tests remained green.

Supported inventory evidence is mechanically checked against executable test
names. Every Phase 13 Unsupported row has a nonempty stop report, no Partial
row remains, and `COMPAT.md` is byte-for-byte reproducible from the locked
inventory. The seven committed fixture hashes still match.

The unchanged Turso core suite passed 2,286 tests with 17 ignored. The
PostgreSQL suite passed 412 tests. Whopper passed 37 unit tests, 12 regression
tests, and one cross-platform regression test. FastDB scoped Clippy passed
with `-D warnings` and `--no-deps`; only inherited Turso unused-import warnings
were emitted.

No file under `core/`, `sqlite/`, `postgres/`, inherited `tests/`, Whopper, or
the WAL/JSONB/optimizer implementation changed in the Phase 13 range.

## Dependency review

Phase 13 pins memory-safe Rust libraries for cryptographic primitives,
password hashing, CBOR, URL parsing, Unicode segmentation/normalization,
semantic versions, HTML sanitization, fuzzy matching, and diff-match-patch.
They execute behind FastDB's closed registry and hard input/output limits; no
plugin ABI, native dynamic code, user-generated SQLite text, logical-name
interpolation, or inherited Turso SQL binding was added.

## Regression performance

The unchanged optimized Phase 5 workload is committed as
`docs/benchmarks/phase13-phase5-regression.json`. It used 5,000 seed records,
200 warmups, and 200 samples on Linux x86-64. The retained run passed every
existing gate without changing a threshold:

| Gate | Ratio | Limit |
| --- | ---: | ---: |
| Point read p50 | 1.40587x | 1.5x |
| Point read p99 | 1.01741x | 2.0x |
| Indexed filter p95 | 1.42282x | 2.0x |
| Write p95 | 1.05638x | 2.0x |
| Checkpointed storage | 1.02263x | 1.5x |

Three preceding unchanged 200-sample runs failed different timing gates on
the shared workstation (point-read p50, then point-read p99, then multiple
latency gates). They were discarded as timing-noisy; the sample count and all
thresholds remained unchanged, matching the established release-benchmark
procedure.

## Commands

```text
cargo metadata --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy -p turso_fastdb_compat -p turso_fastdb_parser -p turso_fastdb \
  -p fastdb -p fastdb-cli -p turso_fastdb_tests -p turso_fastdb_benchmarks \
  --all-targets --no-deps -- -D warnings
cargo test -p turso_fastdb_parser -p turso_fastdb -p fastdb -p fastdb-cli \
  -p turso_fastdb_compat -p turso_fastdb_tests
cargo test -p turso_core --lib
cargo test -p turso_pg_tests
cargo test -p turso_whopper
cargo build --locked --release -p fastdb-cli -p turso_fastdb_benchmarks
target/release/phase5-release-bench --records 5000 --samples 200 \
  --output docs/benchmarks/phase13-phase5-regression.json
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
cargo run -p turso_fastdb_compat -- --check
git diff --check
```

## Remaining boundaries

Phase 13 does not publish Phase 14 CRUD/query forms or any later scripting,
graph-completion, specialized-index, authentication, server, control-plane, or
SDK surface. The resource/context stops are deliberately unavailable, not
partial implementations. Serialized stable WAL remains the only supported
writer mode.

This checkpoint is local pre-1.0 compatibility evidence. It does not authorize
a tag, package publication, artifact upload, production-ready claim, parallel
writers, or Core 1.0. Rollback is by explicit revert of the Phase 13 range
starting at `9f1c1bbdc`; completed Phase 0–12 history remains intact.
