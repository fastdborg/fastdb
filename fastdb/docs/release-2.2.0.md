# FastDB / FastQL 2.2.0 release

Author: GLM (ZCode agent), 2026-10-02.

Status: **qualified candidate for Linux x64**, 2026-10-02. Language contracts
are in [the 2.2.0 working specification](v2.2-language.md), with scope decisions
in [the decision record](v2.2-decisions.md) and milestone evidence in
[the checklist](v2.2-tasks.md). The released 2.1.0 artifacts remain immutable.

2.2.0 is the language release authorized on 2026-10-01: every unfinished
[FastQL.md](../../../FastQL.md) item is resolved through a verified
implementation or a documented architecture/product-fit decision.

## Changes

### Reference expansion in ordinary SELECT

```sql
SELECT id, title, author.* AS writer
FROM articles
WHERE author = type::record('writers', $writer)
ORDER BY id;
```

An aliased single-field wildcard now expands through the existing bounded nested
projection resolver in ordinary SELECT lists, without braces, for unqualified and
source-qualified forms. Plain `author` still returns the stored typed reference,
and SQL source stars keep their meaning. See
[nested projections](nested-projections.md).

### Document operations

- `SELECT * OMIT path, ...` removes named object members from returned
  collection documents (S1).
- `UPDATE ... CONTENT` replaces whole documents and `MERGE` recursively merges
  nested objects; both preserve immutable IDs, validation and atomic index
  maintenance (S3).
- `UPDATE ... PATCH [...]` applies a bounded typed subset of JSON Patch
  (add/remove/replace/copy/move/test) with RFC 6901 pointers (S4).
- `SELECT * FROM ... FETCH path, ...` expands explicitly named finite reference
  paths inside the returned document (S2).
- Typed helpers: `array::len/distinct/flatten`, `array::contains`,
  `doc::keys/values/entries/from_entries` (S5).

### Query conveniences

- Array predicates `items[WHERE ...]` and object destructuring
  `profile.{city, country}` as bounded SELECT projections (S6).
- `array::unnest(...)` table function with typed values for SQL expansion (S7).
- Composite equality: arrays, objects and vectors in DISTINCT, UNION,
  INTERSECT, EXCEPT and IN/NOT IN; typed row membership
  `(n, tags) IN (SELECT n, tags ...)`; one collection-backed recursive CTE
  seed with a UNION ALL step.

### Schema rules

- `DEFAULT` constants and `READONLY` enforcement across every write path (S8).
- `DEFINE SCHEMA ON ... STRICT|FLEXIBLE` with atomic existing-data validation,
  and typed array elements such as `array<record<writers>>` (S9).
- Stored computed fields `VALUE (expr)` evaluated on write in dependency
  order (S10).

### Search and maintenance

- Full-text analyzer configuration (`tokenizer='raw|simple|whitespace|ngram'`)
  and `search::analyze('index', text)` inspection sharing the engine tokenizer (S11).
- Managed `REINDEX` rebuilding scalar/reference, spatial, full-text and vector
  indexes transactionally from source documents (S12).
- `doc::before()`, `doc::after()` and `doc::diff()` mutation snapshots in
  collection RETURNING (S13).
- Final `TIMEOUT <n>ms|s` statement deadlines composed with caller
  cancellation (S14).
- Optional fourth-argument typed-ID inclusion filters for `search::text` and
  `search::vector`, applied before top-k.
- F16 ANN graph precision (`quantization='f16'`) with float32 reranking.
- Compound collection indexes (`CREATE INDEX ... ON docs(a,b,...)`) and
  array-element indexes (`USING ARRAY` with `array::contains` seeks).
- Collection `INSERT ... ON CONFLICT(target) DO UPDATE/DO NOTHING` against
  `id` or declared unique indexes, including compound targets.

## Completed source and artifact checks

The complete scoped check passes on the version-aligned 2.2.0 source under the
local resource guard: formatting and scoped Clippy across eight packages,
**964 Rust tests**, **139 Node/application tests**, strict TypeScript and
**five C ABI tests**, with zero failed, ignored or skipped tests. Every adopted
feature carries focused real-engine regressions linked from
[the checklist](v2.2-tasks.md); indexed features prove native index use without
silent scan substitution.

Exact-artifact qualification of the Linux x64 CLI, all seven native clients,
cancellation, limits, previous-version upgrade/restore and bounded recovery
passes against the built candidate: **all 23 installed verification groups**,
covering the C ABI, the CLI (including both reference-wildcard example
statements), SQLite adoption, native ELF policy checks, Node 22.0.0/24.19.0,
CPython 3.10.21/3.12.3/3.14.7, PHP 8.3.6 with FFI, Go 1.27.1 with race
detection, Swift 6.4, .NET process ownership, and immutable 1.0.0/2.0.0/2.1.0
upgrade/restore with unchanged previous backups and rejected binary
downgrades. The 55-step 2.2.0 native-client fixture exercises every adopted
language feature through the Node, Python and C# clients; the standalone Rust
consumer covers the remaining Rust path against the same source identities.

## Distribution and compatibility

Linux x64 on the documented Ubuntu 24.04 baseline, with Rust, Node/TypeScript,
Python, PHP, Swift, C# and Go clients in the GitHub release bundle. No cloud,
browser/WASM, additional platform or graph-database product scope is added.

The release introduces managed catalog version 5: field rules (defaults,
read-only, computed values, element types), strict-schema metadata, compound
index layouts, array index entries, custom analyzer configurations and F16
graph identities. Older FastDB binaries reject version 5 metadata; preserve a
pre-upgrade backup rather than attempting an old-binary rollback. Legacy FTS
storage still migrates through explicit REINDEX during upgrade qualification.
2.1.0 and 2.0.0 databases upgrade in place; exact-artifact checks cover
candidate writes, reopens, unchanged previous backups, independent restores
and rejected downgrades. SQLite adoption is separate from FastDB version
upgrades.

| Immutable build identity | SHA-256 |
|---|---|
| Source commit | `c4d4d84a2c54a212cbeed6de9e47bf4f9e0cabe6` |
| Source archive | `f5d382266966d1b8f274ce8434967c4501f4f3ade71d6ad8569fbb5db3532907` |
| Cargo.lock | `9126da52f5870fa826648a7ae76718650026cc70bcf4d5623ea62d8a1aed962b` |
| Build manifest | `3c80f841a55656d794af98529a0f49529478115a729785cb52c303ccca18c484` |
| Build SHA256SUMS | `3a5b851eb77ec9e229789e281498faa3dd1a78da6f5e90f6162d893783b216f4` |

The local build manifest identifies the candidate; it is not modified to claim
publication. The qualification working receipts live under the release task
logs; the Record Links demo consumes the same qualified package.

## Completed release evidence

Release gates and their evidence are tracked in
[the 2.2.0 checklist](v2.2-tasks.md); this record is completed as each gate
passes, and later documentation commits never change the tagged build source.
