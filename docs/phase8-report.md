# FastDB Phase 8 report

Status: technically complete locally on 2026-08-13; Phase 9 implementation has
not started

Phase 8 adds one sealed, cataloged FTS provider behind a characterized
SurrealDB `v3.1.5` subset and a separately labeled FastDB/Turso extension. It
retains format 2 and the pinned Turso engine. This is engineering evidence, not
an alpha release authorization or a production-ready claim.

## Engine and clean-room decision

- Retained engine SHA: `977383ff40edc44ef410af062ed0d2322252a869`.
- No upstream fetch, merge, cherry-pick, pin update, or inherited Turso source
  edit was required. FastDB enables the pinned optional `fts` feature and
  custom-index option only on non-WASM targets.
- The official SurrealDB `v3.1.5` Linux x86-64 executable remains installed
  outside the repository. Executable SHA-256:
  `dd9b1395baa8b6af64eb97b85887490a3ad1882aeb910277e0489b072d6e2f9f`.
- Independently authored queries, outputs, scoring observations, public-source
  links, and provider audit evidence are in
  `docs/compat-research/phase8.md`. No SurrealDB source, tests, fixtures,
  expected-output files, or fuzz corpus were inspected or copied.

## Executable surface and storage

The Surreal subset executes `DEFINE ANALYZER ... TOKENIZERS blank`, one-field
`FULLTEXT ANALYZER` indexes, `@@`/`@n@`, `search::score`, and
`search::highlight`. Blank matching is case-sensitive whitespace AND. Match
references must agree within one SELECT. Functions/filters, other tokenizers,
multi-field Surreal indexes, multiple predicates, and FTS under OR/NOT fail
explicitly.

The extension executes `CREATE INDEX ... USING fts (...) WITH (...)`, the
pinned default/raw/simple/whitespace/ngram tokenizers, positive field weights,
and `fts_match`/`fts_score`/`fts_highlight`. It is not counted as SurrealQL
compatibility.

Every FTS field owns an opaque catalog-managed nullable TEXT column. CREATE,
RELATE, and UPDATE bind the document and every derived column in one physical
mutation; DELETE uses engine index maintenance. Existing-row validation,
hidden-column creation, backfill, provider publication, capability publication,
and catalog publication are one schema transaction. Reopen validation fails
closed on malformed analyzer/provider/options/state/capability/ownership or
missing physical state.

FTS candidate reads are capped at 10,000 rows and query text at 65,536 UTF-8
bytes. Structured EXPLAIN proves the opaque custom index method. REBUILD maps
to provider optimization. FTS removal remains explicitly rejected until
physical-column reclamation is proven. An indexed write marks an explicit
transaction dirty; a subsequent affected FTS read returns Transaction and
poisons/rolls back the transaction instead of exposing the pinned provider's
pre-commit view.

## Correctness and recovery evidence

Parser, synchronous frontend, async Rust API, CLI JSON, reopen, graph-edge
fields, cascade, rollback, failure injection, corruption, provider rebuild,
ranking, highlighting, case sensitivity, bound queries, option validation,
and abrupt-exit recovery tests pass. Controlled score results match the
`v3.1.5` observations within `1e-6`.

The committed provider fixture is:

```text
fastdb-tests/fixtures/phase8-format2-fts.fastdb
4a47192b286f457cc175be7881997fde2c4816a50320565a41e43be02ffa5372
```

It reopens, selects the opaque FTS index, accepts further mutations, and
returns the newly indexed document without rebuilding the provider.

The structured FTS model compares document churn and bound blank-token queries
with an independent `BTreeMap` model. The committed corpus contains minimized
model seeds. The final sanitizer-backed campaigns were:

| Target | Budget | Executions | Final corpus | Peak RSS | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| parser | 300 s | 4,502,894 | 5,505 | 753 MB | Passed |
| structured CRUD | 300 s | 6,176 | 350 | 568 MB | Passed |
| structured graph | 300 s | 5,194 | 269 | 561 MB | Passed |
| structured FTS | 300 s | 2,081 | 273 | 436 MB | Passed |

One pinned-engine diagnostic limitation is deliberately visible:
`PRAGMA integrity_check` and `quick_check` report `wrong # of entries` for
`__turso_internal_fts_dir_<index>_key` on an equivalent clean native Turso FTS
database. FastDB clean and abrupt-exit databases return the same provider-only
result while queries and catalog validation succeed. Phase 8 tests accept only
that exact result; Phase 10 must provide a supported FTS-aware check path. This
report does not claim ordinary whole-engine integrity checking covers FTS.

## Performance evidence

Raw samples are in `docs/benchmarks/phase8-fts.json`. The release workload used
1,000 documents, 100 warm samples, identical native hidden TEXT storage,
identical whitespace FTS indexing, identical result fields, and complete result
materialization.

| Metric | FastDB/native ratio | Provisional limit | Result |
| --- | ---: | ---: | --- |
| FTS query p95 | 1.8568x | 2.0x | Passed |
| checkpointed main-file storage | 1.4444x | 2.0x | Passed |

The unchanged Phase 5 workload was rerun with 5,000 records and 100 samples;
raw data is in `docs/benchmarks/phase8-phase5-regression.json`. Its aggregate
gate passed: point p50 1.2326x, point p99 1.1248x, indexed filter p95 1.5765x,
write p95 1.5578x, and storage 1.0118x.

## Verification and phase decision

The complete required parser/frontend/API/CLI/integration suites, locked fuzz
workspace build, formatting, FastDB-only strict clippy, inherited core and
regression suites, release builds, fixture hashes, four 300-second fuzz
campaigns, and final diff/provenance review passed as the Phase 8 commit gates.

No Phase 8 stop condition remains: document/provider state is atomic,
transaction-stale reads are rejected, plans select the provider, candidate
materialization is bounded, catalogs fail closed, unsupported WASM is explicit,
and no logical identifier or value is interpolated into generated SQL.

Core remains pre-alpha and is not production-ready. Phase 9 requires a new
authoritative plan and its own commit; Phases 10–12 and separate release
authorization remain required.
