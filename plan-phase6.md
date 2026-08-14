# FastDB Phase 6 — Format 2 and Multimodel Foundation

Status: authoritative execution plan, 2026-08-13

## 1. Objective and boundary

Phase 6 starts from the completed Phase 5 baseline and prepares FastDB Core
for graph records, full-text search, exact vector search, and production
operations without claiming those later features early. It retains Turso pin
`977383ff40edc44ef410af062ed0d2322252a869`, SurrealDB `v3.1.5` as the
immutable behavioral reference, stable WAL/full durability, the independent
FastDB parser, and direct translated-Turso-AST execution.

The phase introduces transactional format 1 to format 2 migration, a closed
internal capability/provider model, catalog-managed hidden typed storage, and
the language/planner primitives required by Phases 7–10. It must preserve all
Phase 5 behavior and evidence. Graph execution, FTS execution, vector
execution, parallel writers, cloud/server work, a public extension ABI, and
release publication are outside this phase.

`COMPAT.md` changes only after a Phase 6 behavior is executable through the
public API and CLI and has independent conformance evidence. Merely parsing a
future graph, FTS, or vector construct does not make it Supported or Partial.

## 2. Mandatory pinned-engine audit

Before implementation, inspect the checked-out pinned source and record a
read-only audit under `docs/phase6-engine-audit.md`. Also perform the required
read-only comparison with then-current upstream through the repository's
upstream-sync workflow. Fetching for that comparison is allowed; merging,
cherry-picking, or changing the retained pin is not part of Phase 6. Record
exact source locations, feature flags, stability, transaction behavior, target
restrictions, and relevant tests for:

- translated AST preparation and expression/function construction;
- custom-index registration, lifecycle, planner selection, and maintenance;
- Turso FTS functions, tokenizers, index creation, optimization, transactional
  visibility, corruption behavior, and WASM availability;
- vector encoding, `vector64` functions, distance operations, limits, and the
  documented linear-scan execution path;
- schema inspection, `EXPLAIN`, integrity checking, checkpointing, backup, and
  index rebuild/optimization facilities.

The audit must classify every facility as usable unchanged, usable behind an
internal adapter, experimental/unfit, or absent. Phase 6 may use only stable
facilities that pass a focused unchanged-engine test. A Turso core change, a
pin change, or dependence on the toy sparse-IVF provider is a stop condition
requiring a design note and explicit approval.

## 3. Language, AST, and planner foundation

Keep FastDB AST types independent from Turso AST types. Add source-spanned
representations for:

- function calls with one or more identifier segments, including namespaced
  forms such as `search::score(...)` and `vector::distance::cosine(...)`;
- arbitrary supported expressions in `SELECT` projections and explicit
  `AS <alias>` aliases;
- index kind/provider plus provider-specific options as structured AST values,
  never as unparsed source fragments;
- `EXPLAIN <select>` with structured result rows;
- `REMOVE INDEX <name> ON [TABLE] <table>`;
- `REBUILD INDEX <name> ON [TABLE] <table>`.

Function resolution uses a closed internal registry with typed signatures,
capability versions, and lowering hooks. Phase 6 does not expose a public
registration API. Unknown functions, namespaces, index kinds, providers, and
options return a spanned error before opening a mutation transaction. Known
future-provider syntax may have an AST representation but must return an
explicit unavailable/unsupported error until its implementation phase.

Phase 6 makes expression projections and aliases executable for the existing
scalar expression set. It also makes `EXPLAIN`, `REMOVE INDEX`, and `REBUILD
INDEX` executable for existing B-tree indexes. `EXPLAIN` returns ordinary
structured FastDB values and must not expose a new public engine-plan type.
Remove and rebuild operations resolve catalog IDs before constructing static
internal DDL; logical names never enter generated SQL.

Parser, planner, API, and CLI tests must prove that unsupported clauses are
not silently discarded, aliases do not alter stored data, rebuild is atomic,
and a failed remove/rebuild leaves both the catalog and physical index
unchanged.

## 4. Format 2 catalog contract

Format 2 extends, rather than replaces, the format 1 document model. Existing
rows retain their canonical `rid` and `doc` bytes. Ordinary tables may have no
new physical columns after migration; hidden typed columns are added only by
a cataloged capability and are never part of the public document.

Persist the following semantic metadata using reserved, static internal
schema whose exact DDL is recorded in `docs/format-v2.md`:

- logical tables: existing fields plus `kind` (`NORMAL` or `RELATION`),
  optional endpoint table IDs, and the relation enforcement flag;
- analyzers: immutable analyzer ID, logical name, provider, provider version,
  canonical options, and original definition;
- indexes: existing identity/ownership fields plus index kind, provider,
  provider version, canonical options, lifecycle state, and encoding version;
- hidden typed columns: immutable column ID, owning table and optional owning
  index/field, opaque physical name, logical field path, provider and version,
  physical encoding, dimension/options, and rebuild state;
- capability requirements: the minimum provider/encoding versions required to
  open and mutate the database safely.

Provider names, option keys, and lifecycle states are closed enums at the
frontend boundary. Canonical provider options use a deterministic,
versioned encoding. They may not contain executable SQL or unresolved logical
identifiers. Committed provider-backed objects may be `READY` or
`REBUILD_REQUIRED`; construction uses transactional state and must never
publish a usable catalog object before its physical state is complete.

The migration maps every format 1 table to `NORMAL`, every format 1 index to
the built-in `BTREE` provider at version 1 with empty options and `READY`
state, and creates no analyzer, relation, hidden-column, or external
capability rows. It preserves immutable table/index IDs, physical names,
definitions, record encodings, and expression format versions.

## 5. Transactional migration and open rules

Run migration while holding the database schema mutex and before exposing a
connection for queries:

1. Read and validate format/dialect metadata and the complete known format 1
   catalog before mutation; refuse unknown future or structurally incompatible
   formats.
2. Begin one engine transaction and apply only reviewed static internal DDL.
3. Add and backfill format 2 metadata deterministically without rebuilding
   user documents or renaming physical objects.
4. Validate catalog ownership, opaque physical objects, B-tree expressions,
   provider requirements, and row counts inside the transaction.
5. Write `last_migration = 2` and `format_version = 2` only after every prior
   check succeeds, then commit.
6. Re-read format and capability metadata before publishing the database
   handle.

Any injected error, panic boundary, I/O failure, validation failure, or
unsupported provider rolls back the whole migration. Reopening must observe a
complete format 1 database or a complete format 2 database, never a mixed
catalog. Re-running open on format 2 is a no-op. Downgrade is not supported;
backup/export before upgrade is documented.

Open may read a database with a known provider in `REBUILD_REQUIRED` state
only when ordinary document access remains sound and all affected indexed
queries fail explicitly. An unknown provider, newer provider version, unknown
encoding, or inconsistent hidden storage fails before mutation. Phase 6
creates no best-effort fallback that could return incomplete indexed results.

## 6. Internal provider contract

Implement the smallest private interface needed by later phases. A provider
owns typed option validation, required hidden storage, encoding/version
checks, document-to-derived-value maintenance, index create/drop/rebuild,
planner matching, explain annotations, integrity validation, and result
decoding. Provider-maintained document and derived state change in the same
transaction.

The interface is crate-internal and sealed. Do not add dynamic loading,
native-code plugins, SQL source callbacks, public trait stability promises, or
provider discovery from the database file. Every provider and supported
version is compiled into FastDB and explicitly registered. Physical names are
derived from immutable catalog IDs.

Phase 6 registers only the existing B-tree behavior. FTS, graph adjacency,
and vector encodings may use test-only fake providers to exercise rollback and
compatibility refusal, but no public syntax may create them successfully.

## 7. Verification evidence

Add independently authored `P6-*` test groups and a
`docs/phase6-report.md`. Cover at least:

- committed format 1 fixtures migrating in memory and on disk, reopening as
  format 2, accepting new CRUD/index work, and passing engine integrity;
- failure injection at every migration publication boundary, followed by
  reopen and digest/catalog comparison;
- idempotent format 2 reopen and refusal of future formats, providers,
  provider versions, encodings, options, and invalid lifecycle states;
- unchanged Phase 5 CRUD, schemafull validation, transactions, parameters,
  CLI JSON, crash/recovery, cache invalidation, and B-tree plan selection;
- expression projections/aliases and structured explain results through both
  the Rust API and CLI;
- atomic B-tree remove/rebuild before and after reopen, including duplicate or
  invalid catalog/physical states;
- provider-derived hidden state maintained or rolled back with `doc` under a
  test provider, with no logical-name interpolation or generated user SQL;
- resource-limit and fuzz coverage for nested/namespaced calls, option lists,
  projection aliases, and new statements.

Review the final diff for changes under inherited Turso core. None are
expected. Preserve raw audit findings, commands, fixture hashes, plan output,
and failure-injection results in the Phase 6 report.

## 8. Local verification gates

Discover package names with `cargo metadata` and adjust only if the checked-in
workspace proves a command stale. At minimum run:

```sh
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy --locked -p turso_fastdb_parser -p turso_fastdb -p fastdb \
  -p fastdb-cli -p turso_fastdb_tests --all-targets
cargo test --locked -p turso_fastdb_parser
cargo test --locked -p turso_fastdb
cargo test --locked -p fastdb --all-targets
cargo test --locked -p fastdb-cli --all-targets
cargo test --locked -p turso_fastdb_tests
cargo test --locked --doc -p fastdb
(cd fastdb-parser && cargo +nightly fuzz run parse -- -max_total_time=300)
(cd fastdb-tests/fuzz && cargo +nightly fuzz run structured_crud -- -max_total_time=300)
cargo test --locked -p turso_core --lib
cargo test --locked -p core_tester --test integration_tests expression_index
cargo test --locked -p core_tester --test integration_tests without_mvcc
cargo test --locked -p core_tester --test integration_tests test_transaction_visibility
cargo test --locked -p turso_pg_tests
cargo test --locked -p turso_whopper
(cd fastdb-tests/fixtures && sha256sum -c SHA256SUMS)
cargo build --locked --release -p fastdb-cli
cargo build --locked --release -p turso_fastdb_benchmarks \
  --bin phase5-release-bench
target/release/phase5-release-bench \
  --output docs/benchmarks/phase6-phase5-regression.json
git diff --check
```

Re-run the unchanged Phase 5 release benchmark and require every original
ratio and storage gate to remain within its recorded threshold. Phase 6 does
not substitute a new benchmark for a regressed MVP baseline.

## 9. Definition of Done and stop conditions

Phase 6 is complete only when the pinned-engine audit, format 2 documentation,
migration fixtures, rollback/crash evidence, provider refusal tests, AST and
B-tree maintenance behavior, full local verification matrix, and unchanged
Phase 5 benchmark gates pass. `docs/phase6-report.md` must record exact results
and any unavailable target capability.

Stop before Phase 7 if migration can expose mixed format state, provider
absence can yield incomplete results, an existing B-tree plan changes, an
unknown option reaches mutation, a logical identifier is interpolated into
SQL, or implementation requires a Turso core/pin change. Resolve such a stop
through a reviewed design note and explicit approval; do not weaken the
architecture or release gates.
