# FastDB Phase 9 — Exact Vector Search

Status: completed locally on 2026-08-13; see `docs/phase9-report.md`

## 1. Purpose and immutable baselines

Phase 9 adds fixed-dimension floating-point vector fields and exact bounded
K-nearest-neighbor search to the Phase 8 multimodel baseline. It is the
technical `0.1` alpha-candidate surface, but completion does not authorize a
tag, package publication, upload, or production-ready claim.

The immutable compatibility reference is the unmodified SurrealDB `v3.1.5`
binary. The engine remains Turso commit
`977383ff40edc44ef410af062ed0d2322252a869`; no upstream merge or inherited
source edit is planned. Format 2 remains current and gains only cataloged
vector hidden-column roles and canonical metadata.

Before implementation, preserve independently authored black-box observations
under `docs/compat-research/phase9.md` and audit the pinned engine's
`vector64`, `vector_distance_cos`, `vector_distance_l2`, serialization,
dimension limits, and query-plan behavior. Online documentation may guide
research but cannot silently change the `v3.1.5` contract.

## 2. Executable language boundary

Support this exact initial surface:

- `DEFINE FIELD embedding ON table TYPE array<float, N>` where `N` is a
  positive integer no greater than 65,536;
- exact KNN predicates `field <|K,COSINE|> expression` and
  `field <|K,EUCLIDEAN|> expression` in `SELECT ... WHERE`;
- literal query vectors within the existing collection limit and bound array
  parameters up to the field dimension;
- `vector::distance::euclidean(left, right)`,
  `vector::similarity::cosine(left, right)`, and
  `vector::distance::knn()` in SELECT projections;
- ordinary predicates combined with one KNN predicate by `AND`, with ordinary
  filtering applied before top-k selection;
- aliases and structured `EXPLAIN` for distance projections and exact scans.

`K` must be in `1..=10_000`. Query vectors and stored values must have exactly
the declared dimension and contain only finite numeric values. Integer inputs
normalize to public floating-point array values. Cosine search rejects a zero
query vector and any stored zero vector that would make distance undefined.

The first implementation accepts exactly one KNN predicate and one vector
field per SELECT. KNN under `OR`/`NOT`, KNN in mutations, dynamic K, standalone
KNN expressions, non-array query values, mixed dimensions, and unsupported
vector function paths fail explicitly before physical work.

Explicitly reject vector index definitions and provider names for HNSW,
DiskANN, and `toy_vector_sparse_ivf`. Phase 9 makes no ANN, approximate recall,
or vector-index GA claim.

## 3. Independent AST and validation

Extend the independent FastDB AST with:

- a fixed vector schema type carrying the dimension and complete source span;
- a KNN predicate carrying the field expression, K, metric, query expression,
  and operator span;
- recognized vector function paths, while retaining explicit errors for every
  unknown function.

The lexer/parser must keep the existing byte/token/nesting/collection limits.
The KNN operator is structural and cannot be reconstructed from source text in
the executor. Parser tests cover whitespace/case, both metrics, K boundaries,
malformed delimiters, overflow, nested boolean use, and deferred ANN syntax.

Semantic validation resolves the KNN field to one cataloged fixed vector rule,
checks all dimensions and finite values before mutation/query preparation, and
rejects ambiguous vector predicates or distance projections. Public arrays
remain ordinary `Value::Array` results; no vector-only public Rust value is
introduced.

## 4. Format-2 storage contract

Each fixed vector field owns one opaque catalog-managed nullable BLOB column
with hidden role `VECTOR64`, provider `BUILTIN_VECTOR_EXACT`, provider version
1, encoding version 1, and canonical options containing the logical path and
dimension. No public plugin/provider ABI is added.

The public JSONB document retains the normalized floating-point array. The
hidden BLOB is the pinned Turso `vector64` physical encoding: little-endian
IEEE-754 `f64` elements followed by the type byte `2`. FastDB validates the
encoding independently on reopen before allowing queries.

CREATE, RELATE, and UPDATE bind document JSONB and every vector BLOB in one
physical statement. Adding a vector field validates existing documents,
publishes the hidden-column catalog row, adds the physical BLOB column, and
backfills it within one schema transaction. Rollback/failure injection must
leave no partial document, column, catalog, or encoding state.

Catalog reload fails closed on unknown vector provider/version/role/options,
duplicate ownership, dimension mismatches, missing physical columns, malformed
stored BLOBs, or document/BLOB disagreement. Direct hidden-table edits remain
unsupported.

## 5. Exact scan and bounded top-k

Lower KNN into a direct Turso AST plan over the opaque physical table and
hidden BLOB. Bind the encoded query vector; never generate SQLite text from
FastDB input. Apply any supported ordinary predicate before distance ordering.

Use the pinned native functions:

- COSINE distance = `vector_distance_cos(hidden_blob, bound_blob)`;
- EUCLIDEAN distance = `vector_distance_l2(hidden_blob, bound_blob)`.

The physical plan orders ascending by distance and applies `LIMIT K`, so
engine result materialization is bounded by K. FastDB also keeps a bounded
top-k fallback/reference implementation for conformance tests and refuses any
path that would collect an unbounded candidate set. Tie order is deterministic
by encoded RID after distance.

`vector::distance::knn()` returns the metric distance associated with the KNN
predicate. `vector::distance::euclidean` returns L2 distance;
`vector::similarity::cosine` returns `1 - cosine_distance`. Results use public
finite `Value::Float` values.

`EXPLAIN` returns ordinary structured rows and must identify an exact vector
scan, metric, K, opaque physical table/column, native distance function,
ordinary prefilter, bounded sort, and limit. It must not imply ANN or an index.

## 6. Transactions and cross-feature behavior

Vector hidden state follows existing standalone and explicit transaction
atomicity and poisoning rules. Reads inside a transaction see the transaction's
current document/BLOB state; unlike the Phase 8 FTS provider, exact scalar
vector functions have no documented pre-commit visibility exception.

Cross-feature tests cover vector fields on normal and relation tables,
FTS/vector fields on the same records, graph cascade deletion, schemafull
validation, B-tree/ordinary prefilters, rollback, reopen, abrupt exit, and
failure between document and vector-derived maintenance.

## 7. Verification and evidence

Add independently authored parser, frontend, API, CLI, integration, fixture,
failure-injection, corruption, crash, fuzz, and benchmark evidence. At minimum
prove:

- exact cosine/euclidean results against independent Rust calculations,
  including ties, negatives, integer normalization, and bound vectors;
- ordinary predicate filtering occurs before top-k;
- dimensions 1 and 65,536 work through bound parameters, while 0/65,537,
  non-finite values, dimension mismatch, K 0/10,001, and zero-vector cosine
  fail safely;
- document/hidden state is atomic through create/update/backfill/rollback,
  reopen, abrupt exit, and every failure-injection boundary;
- malformed vector catalog/options/BLOB/document disagreement fails closed;
- graph-edge vector fields and cascade cleanup remain correct;
- EXPLAIN proves the native exact scan, prefilter, bounded order, and limit;
- the public async API and CLI JSON retain arrays and return distance fields;
- a structured vector model fuzzer stays bounded and detects no divergence;
- release vector query/storage overhead is no worse than the provisional 2x
  Phase 12 ceiling against an equivalent native Turso physical workload;
- the unchanged Phase 5 release benchmark and every Phase 6–8 gate pass.

Run at minimum:

```sh
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p fastdb -p fastdb-cli \
  -p turso_fastdb_tests -p turso_fastdb_benchmarks \
  --all-targets --no-deps -- -D warnings
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
git diff --check
```

Run all committed fixture hashes, exact vector recovery/corruption commands,
the parser and structured CRUD/graph/FTS/vector 300-second fuzz campaigns, the
Phase 9 vector benchmark, and the unchanged Phase 5 release benchmark. Preserve
exact commands, counts, environment, raw measurements, and ratios in
`docs/phase9-report.md`.

## 8. Stop conditions

Stop and record evidence rather than weakening the architecture if:

- exact vector storage or distance requires an inherited Turso source change;
- format-2 catalog validation cannot detect malformed BLOB/document state;
- document and native vector state cannot commit/recover atomically;
- ordinary filtering cannot be proven to occur before bounded top-k;
- a query path must interpolate logical identifiers or values into SQL text;
- exact search cannot stay bounded by K or exceeds the provisional 2x native
  workload ceiling after equivalent materialization;
- any Phase 5–8 compatibility, durability, graph, FTS, or performance gate
  regresses beyond its documented limit.

## 9. Definition of done

Phase 9 is complete only when fixed vector schema/storage, both exact KNN
metrics, vector functions, bound queries, prefiltered bounded top-k, distance
projection, structured EXPLAIN, atomic/recovery/corruption evidence, API/CLI,
fuzzing, fixtures, benchmarks, and all prior gates pass; `COMPAT.md`, format,
API/CLI, release-readiness, clean-room research, and the Phase 9 report are
current; and the complete diff is committed as the isolated Phase 9 rollback
point.

Completion makes the tree only a technical alpha candidate. Publishing remains
separately authorized, and Phases 10–12 remain required before any
production-ready Core 1.0 claim.
