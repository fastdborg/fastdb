# Phase 8 clean-room full-text-search observations

Status: independent black-box, public-document, and pinned-engine research,
2026-08-13

## Reference environment

- SurrealDB executable: unmodified official Linux x86-64 `v3.1.5`
- Version output: `3.1.5 for linux on x86_64`
- Download URL:
  `https://download.surrealdb.com/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`
- Download SHA-256:
  `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`
- Executable SHA-256:
  `dd9b1395baa8b6af64eb97b85887490a3ad1882aeb910277e0489b072d6e2f9f`
- Installed outside the repository at
  `/home/tan/.cache/fastdb-tools/surrealdb-v3.1.5/surreal`
- Backend: fresh embedded `memory` endpoint for each independent query batch
- Namespace/database: `fastdb` / `phase8`
- Output mode: CLI JSON

The black-box queries below were independently written for FastDB. No
SurrealDB source, tests, fixtures, expected-output files, or fuzz corpus were
inspected, copied, translated, or adapted. Object ordering is normalized for
readability.

## Public sources

- `https://surrealdb.com/docs/reference/query-language/statements/define/analyzer`
- `https://surrealdb.com/docs/reference/query-language/statements/define/indexes`
- `https://surrealdb.com/docs/learn/data-models/full-text-search/overview`
- `https://surrealdb.com/docs/reference/query-language/operators`
- `https://surrealdb.com/docs/surrealql/functions/database/search`
- `https://docs.turso.tech/sql-reference/functions/fts`
- `https://docs.turso.tech/guides/vector-search`

Online SurrealDB documentation is supporting research, not a moving
compatibility reference. Only the executable observations below pin FastDB's
SurrealQL subset to `v3.1.5`. Turso documentation describes the labeled native
extension and its known transaction limitation.

## Analyzer and index normalization

Input:

```surql
DEFINE ANALYZER blankish TOKENIZERS blank;
DEFINE INDEX text_idx ON TABLE doc FIELDS text
  FULLTEXT ANALYZER blankish;
INFO FOR TABLE doc;
```

Observed normalized definitions:

```text
DEFINE ANALYZER blankish TOKENIZERS BLANK
DEFINE INDEX text_idx ON doc FIELDS text
  FULLTEXT ANALYZER blankish BM25(1.2,0.75)
```

Adding `HIGHLIGHTS` to the index retains `HIGHLIGHTS` in the normalized
definition. Defining an analyzer twice reports `The analyzer 'a' already
exists`. Phase 8 implements only one `blank` tokenizer and no function/filter
pipeline because that is the deliberately sealed equivalence boundary.

## Match operators, binding, and plan

Setup:

```surql
DEFINE ANALYZER blankish TOKENIZERS blank;
DEFINE INDEX text_idx ON doc FIELDS text
  FULLTEXT ANALYZER blankish HIGHLIGHTS;
CREATE doc:a SET text = 'Rust web programming';
CREATE doc:b SET text = 'web Rust internals';
CREATE doc:c SET text = 'rust web lowercase';
```

Observed:

```surql
SELECT id, text FROM doc WHERE text @@ 'Rust web';
-- doc:a and doc:b match; doc:c does not

LET $q = 'Rust web';
SELECT id FROM doc WHERE text @1@ $q;
-- doc:a and doc:b match

SELECT id FROM doc WHERE text @1@ 'Rust web' EXPLAIN FULL;
-- includes a FullTextScan naming text_idx and the query
```

The `blank` tokenizer is case-sensitive. Multiple query terms use AND
semantics independent of term order. Bound query strings are accepted. `@@`
and `@n@` are index-backed predicates; `@n@` associates the predicate with
reference-bearing `search::*` projection functions.

FastDB initially permits exactly one FTS predicate/reference per SELECT. This
avoids inventing association or boolean-composition behavior that has not yet
been characterized.

## Ranking and highlighting

With five independently created documents, a query term occurring in two
documents produced these observed scores:

```text
two occurrences in one document: 0.3710543215274811
one occurrence in one document:  0.31654953956604004
```

With the term appearing in two of three documents, both observed scores were
zero. This is consistent with the reference's non-negative/clamped inverse
document-frequency behavior; FastDB treats the numeric observations as the
contract rather than inferring an implementation.

A second controlled batch used `x x`, `x`, `a`, `b`, and `c` as the complete
five-document corpus and queried `x`. SurrealDB `v3.1.5` returned:

```text
document "x x": 0.3587977886199951
document "x":   0.36109215021133423
```

The Phase 8 conformance test reproduces both values within `1e-6` using
single-precision arithmetic, BM25 parameters `1.2`/`0.75`, non-negative IDF,
and the independently derived logarithmic term-frequency behavior. This
tolerance accommodates the final `f32` representation without weakening the
observed numeric contract.

Input:

```surql
SELECT text,
       search::score(1) AS score,
       search::highlight('<b>', '</b>', 1) AS marked
FROM doc
WHERE text @1@ 'Rust web'
ORDER BY score DESC;
```

Observed: exact matching tokens are wrapped with the requested before/after
tags when the index was declared with `HIGHLIGHTS`. Without `HIGHLIGHTS`, the
highlight function returns the original value. Phase 8 supports this
characterized string result and defers offsets and other search functions.

## Churn visibility

Standalone CREATE, UPDATE, and DELETE operations were immediately reflected by
later FTS queries. The pinned Turso documentation separately warns that an FTS
query in a transaction after an indexed write can observe the pre-transaction
index. FastDB does not expose that stale view: it rejects an affected FTS read
after a dirty indexed write until the explicit transaction commits.

## Pinned Turso capability audit

Audit target: retained engine SHA
`977383ff40edc44ef410af062ed0d2322252a869`. The audit inspected only the
checked-out Turso-derived source already present in this repository.

- `core/database.rs` exposes `DatabaseOpts::with_index_method(true)`. FastDB
  must opt in explicitly; the feature is not silently enabled globally.
- `core/index_method/fts.rs` implements the provider with Tantivy data stored
  in Turso B-trees. The provider's `pre_commit` flush persists pending
  documents within engine commit handling, and rollback discards the
  uncommitted provider mutations.
- The provider accepts `default`, `raw`, `simple`, `whitespace`, and `ngram`
  tokenizers plus field weights. FastDB validates its closed option set before
  mutation because the pinned parser/provider does not reject every unknown
  `WITH` key itself.
- The checked-out provider maps `OPTIMIZE INDEX` to FTS segment merging. Phase
  8 can therefore lower `REBUILD INDEX` to the directly constructed Optimize
  AST without generated SQL.
- The provider exposes exact custom-index patterns for `fts_match` and
  `fts_score`, including score ordering and optional limits. Plan evidence
  must prove this path is selected; scalar fallback is not sufficient.
- Default provider budgets are 64 MiB for the writer, 64 MiB for the hot cache,
  and 128 MiB for the chunk cache. The chunk store uses 1 MiB pieces. These are
  engine bounds, not FastDB's complete query resource contract; Phase 10 adds
  public configurable limits.
- `PRAGMA integrity_check` and `quick_check` report `wrong # of entries` for
  the provider-owned `__turso_internal_fts_dir_<index>_key` index even on a
  clean native Turso database created and closed through the official CLI.
  The same native-only diagnostic appears after FastDB clean and abrupt
  shutdown, while provider queries and catalog validation succeed. Phase 8
  records this as a pinned engine check limitation rather than suppressing the
  row or claiming whole-engine integrity. Phase 10's supported check path must
  validate FTS internals separately and distinguish this exact known result
  from other integrity failures.
- The Cargo feature excludes unsupported WASM targets. FastDB must return an
  explicit capability error on those targets rather than expose parser-only or
  scan-fallback behavior.

No inherited Turso edit or pin update is required by this audit. The provider
stays internal to the translated-AST request path. FastDB supplies opaque
physical column/index names and bound values and never forwards a logical name
or generates SQLite text from source input.

## Provider mapping decision

Surreal `blank` maps to the pinned Turso `whitespace` tokenizer because both
split on whitespace and preserve case in the characterized surface. FastDB
keeps scoring/highlighting surface behavior separate: Surreal functions follow
the observations above, while `fts_match`, `fts_score`, and `fts_highlight`
retain pinned Turso semantics and are labeled extensions.

The document remains authoritative. Catalog-managed hidden TEXT columns store
the extracted indexed strings for native provider input. They are written in
the same translated mutation as `doc`; the FTS index is then maintained by the
pinned engine's transaction machinery.

## Deliberate subset boundaries

Phase 8 does not claim full analyzer compatibility, multi-field Surreal
FULLTEXT indexes, arbitrary boolean FTS composition, offset output, custom
functions, custom filters, custom scoring parameters, fuzzy/phrase grammar,
WASM FTS, or complete SurrealDB search behavior. The native Turso tokenizer and
weight options are FastDB extensions and do not expand `COMPAT.md`'s
SurrealQL-supported surface.
