# FastDB Phase 8 — Full-Text Search

Status: authoritative implementation plan

## 1. Objective and scope

Phase 8 adds one sealed, cataloged full-text-search provider behind two
explicitly different language surfaces: a characterized SurrealDB `v3.1.5`
subset and a labeled FastDB/Turso extension. Both surfaces maintain opaque
hidden TEXT columns and the pinned Turso FTS index in the same transaction as
the public JSON document.

The retained Turso engine SHA remains
`977383ff40edc44ef410af062ed0d2322252a869`. Phase 8 enables the pinned crate's
existing `fts` feature and custom-index option, but does not fetch, merge,
cherry-pick, change the pin, or edit inherited Turso code. FTS remains
unavailable on unsupported WASM targets.

The independent observations in `docs/compat-research/phase8.md` define the
SurrealQL behavior. Current online documentation informs the native extension
and limitations but cannot silently alter the immutable reference contract.

## 2. Explicit language surfaces

The initial SurrealQL subset is:

```surql
DEFINE ANALYZER name TOKENIZERS blank;
DEFINE INDEX name ON [TABLE] table FIELDS field
  FULLTEXT ANALYZER analyzer [HIGHLIGHTS];
SELECT ... FROM table WHERE field @@ query;
SELECT ... FROM table WHERE field @1@ query;
search::score(1)
search::highlight('<b>', '</b>', 1)
```

Only one field, one FTS predicate, and one match reference are accepted per
SELECT in this phase. A bare `@@` has the implicit reference zero. `@n@` uses
an unsigned decimal reference. Reference-bearing `search::*` calls must match
the query predicate in the same SELECT.

The FastDB extension is:

```sql
CREATE INDEX name ON [TABLE] table USING fts (field [, ...])
  [WITH (tokenizer = 'default|raw|simple|whitespace|ngram',
         weights = [number, ...])];
fts_match(field [, ...], query)
fts_score(field [, ...], query)
fts_highlight(field, before_tag, after_tag, query)
```

Extension syntax and behavior are documented as FastDB/Turso features and are
never counted as SurrealQL compatibility.

This phase rejects before mutation or query execution:

- Surreal analyzer functions, filters, comments, ngram options, multiple
  tokenizers, and tokenizers other than `blank`;
- Surreal multi-field FULLTEXT definitions, malformed or mismatched match
  references, multiple FTS predicates, and FTS under `OR` or `NOT`;
- `search::offsets`, `search::analyze`, `search::linear`, arbitrary score
  parameters, and all uncharacterized search functions;
- extension tokenizer or option names outside the closed set, duplicate
  options, invalid ngram configuration, non-finite/non-positive weights, and
  weight counts unequal to the indexed-field count;
- using Surreal analyzers from extension syntax or extension-only options from
  Surreal syntax;
- FTS indexes over nested collections, non-string indexed values, relation
  endpoint metadata (`id`, `in`, `out`), provider-owned hidden columns, or an
  unresolved table/field;
- FTS predicates outside SELECT filters, standalone search functions, and
  arbitrary user calls to unknown functions.

Deferred analyzer/pipeline syntax must fail explicitly rather than being
accepted and ignored. `COMPAT.md` remains Unsupported until each surface
executes through the public API and CLI with named conformance evidence.

## 3. Parser and independent AST

Add `DefineAnalyzerStatement` with a spanned logical analyzer identifier and
the exact normalized tokenizer configuration. Add a source-surface marker to
index definitions so Surreal FULLTEXT and the native extension cannot be
confused after parsing.

Add `BinaryOperator::FtsMatch(Option<u32>)`. The lexer recognizes `@@` and
`@<digits>@` atomically before identifier/operator fallback. The Pratt parser
places the operator at comparison precedence. Unsupported `@...@` forms carry
the exact source span in their diagnostic.

The existing independent namespaced function-call AST represents both
`search::*` and native FTS functions. No Turso AST types appear in public or
parser crates. The native `CREATE INDEX ... USING fts` production is distinct
from record `CREATE` and from Surreal `DEFINE INDEX`.

Existing parser input, token, nesting, collection, identifier, and statement
limits remain in force. FTS query text and highlight tags are runtime values,
never generated SQL fragments.

## 4. Catalog and provider contract

Format version remains 2. Add closed catalog values:

- analyzer provider `BUILTIN_FTS_SURREAL_BLANK`, version 1;
- index kind `FTS`, provider `BUILTIN_FTS`, provider version 1, encoding
  version 1, and state `READY`;
- hidden-column encoding `FTS_TEXT_UTF8` and role `fts_text`;
- canonical provider options containing the source surface, tokenizer,
  ordered field paths, optional weights, and Surreal highlights flag.

Every FTS index owns exactly one hidden nullable TEXT column per indexed field.
Ownership is `(table_id, index_id, canonical_field_path, ordinal)`. Physical
names are opaque deterministic derivatives of catalog IDs. Logical names and
user values never enter a physical identifier or generated SQL.

Analyzer and index creation take the schema mutex and one engine transaction.
Index creation allocates catalog rows and hidden columns, adds the physical
columns, backfills them from existing documents, creates the Turso FTS index,
persists the exact capability requirement, validates the candidate catalog,
and publishes the snapshot only after commit.

Catalog loading validates analyzer definitions, provider/version/encoding,
canonical options, hidden ownership and ordinal cardinality, indexed field
agreement, physical column declarations, and the exact provider index. Any
missing, duplicate, orphaned, unknown, stale, or incompatible component fails
closed before user queries can run.

Provider registration remains internal and closed. Phase 8 adds no public
plugin ABI, dynamic native code, arbitrary provider name, generated user SQL,
or logical-name interpolation.

## 5. Derived storage and mutation atomicity

The document remains authoritative. On CREATE, RELATE, UPDATE, and document
replacement, the frontend derives each indexed hidden value from the same
post-mutation object that is encoded into `doc`. A missing or null path stores
SQL NULL; a string stores its exact UTF-8 value; any other value is a
constraint error. The translated INSERT/UPDATE binds the document and every
derived hidden column in one statement.

DELETE removes the document row and provider state atomically through the
pinned engine. Relation-edge FTS fields follow the same derivation rules while
the immutable graph columns remain unchanged. Failure injection covers
analyzer publication, hidden-column ownership, physical column addition,
backfill, provider-index creation, and document/derived-column writes.

Existing documents are validated before provider publication. A failed
backfill or incompatible value rolls back every catalog and physical change.
Ordinary B-tree and graph storage remain byte-for-byte unchanged when no FTS
index exists.

## 6. Query lowering and evaluation

Resolve exactly one supported FTS predicate from the SELECT condition. The
Surreal surface resolves a single-field Surreal FTS index by canonical field
path. The extension resolves one native FTS index whose ordered fields match
the function arguments exactly. Ambiguous or absent resolution is an explicit
query error.

Lower the resolved predicate to a directly constructed Turso SELECT AST over
the hidden physical columns, with all query text and tags bound. The physical
predicate invokes `fts_match`; score projection invokes `fts_score`. Remaining
ordinary predicates are applied to the matched document candidates without
weakening the documented filter semantics. Candidate materialization is
bounded to 10,000 rows in Phase 8; configurable lower limits arrive in Phase
10.

Surreal blank tokenization is case-sensitive whitespace tokenization and
requires every query token, independent of order. Its score/highlight behavior
matches the characterized `v3.1.5` observations. Native functions preserve
the pinned Turso provider semantics. Projection aliases may be used by ORDER
BY so score ordering does not require a public engine-plan type.

`EXPLAIN` returns ordinary structured FastDB result values. The physical plan
must name only opaque objects and prove the custom FTS index is selected; an
index-definition test without plan selection is insufficient.

## 7. Transaction visibility and maintenance

The pinned Turso FTS provider exposes a pre-transaction index view after a
writer changes an indexed document. FastDB must not return that stale view.
Each explicit transaction tracks the FTS indexes/tables dirtied by indexed
writes or FTS index creation. A later query that touches a dirty FTS index is
rejected with the existing transaction error path, poisoning and rolling back
the full explicit transaction. Commit publishes document and provider state
together; rollback publishes neither.

`REBUILD INDEX name ON table` maps a ready FTS provider to the pinned engine's
verified optimization/segment-maintenance AST. B-tree rebuild behavior remains
unchanged. Ordinary `REMOVE INDEX` removes the provider index and owned hidden
state only if the pinned physical-column removal path is proven atomic and
reopen-safe; otherwise Phase 8 explicitly rejects FTS removal and records the
deferred operational path for Phase 10.

On unsupported WASM targets, analyzer/index definition and FTS execution
return an explicit capability error. Parsing remains deterministic and cannot
silently fall back to a scan with different semantics.

## 8. Verification matrix

Add independently authored parser, frontend, async API, CLI, model, recovery,
failure, corruption, fuzz, and benchmark evidence with stable `p8_*` IDs.
Required coverage includes:

- exact ASTs and rejection spans for both surfaces and all deferred clauses;
- analyzer creation, duplicate/unknown definitions, case-sensitive blank
  behavior, bound queries, match-reference validation, scoring, highlighting,
  aliases, ordering, and projection results;
- native tokenizer/weight validation and pinned `fts_match`, `fts_score`, and
  `fts_highlight` behavior;
- existing-row backfill, create/update/delete churn, null/missing fields,
  relation-edge FTS fields, incompatible value rollback, and document/hidden
  atomicity;
- explicit transaction stale-read rejection, poisoning, commit/rollback,
  reopen, abrupt exit, and injected failure between every publication step;
- exact physical plan selection before/after reopen and after rebuild;
- corruption cases for analyzer/provider/version/options/state, hidden
  ownership/ordinal/encoding, physical columns, and provider indexes;
- bounded 10,000-row candidate materialization, FTS query-length/token bounds,
  and memory measurement under index churn and broad matches;
- parser, structured CRUD/graph, and structured FTS fuzz targets;
- FTS p95 latency/storage against an equivalent native Turso physical
  workload, with the provisional 2x Phase 12 ceiling;
- every unchanged Phase 7, Phase 6, and Phase 5 gate.

At minimum run:

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

Run every committed fixture hash check, the exact Phase 8 recovery/corruption
commands, the three existing 300-second fuzz campaigns, the new structured FTS
300-second campaign, and release-mode FTS plus unchanged Phase 5 benchmark
gates. Preserve commands, environment, raw measurements, and ratios in the
Phase 8 report.

`--no-deps` keeps `-D warnings` scoped to the listed FastDB packages. Enabling
the pinned `fts` feature exposes a pre-existing unused benchmark re-export in
the inherited Turso crate; inherited Turso correctness is verified by its
unchanged test suites instead of changing pinned source or weakening FastDB
lints.

## 9. Stop conditions

Stop Phase 8 and record evidence instead of weakening the design if:

- enabling the pinned provider requires an inherited Turso source change;
- FTS state cannot be committed and recovered atomically with the document;
- stale explicit-transaction reads cannot be rejected deterministically;
- logical identifiers or values would need generated SQL interpolation;
- the provider plan cannot be proven or catalogs cannot detect missing/stale
  physical state;
- bounded candidate/memory behavior cannot be enforced;
- Surreal blank behavior is not equivalent to the sealed physical tokenizer;
- any unchanged Phase 5–7 durability, graph, compatibility, or performance
  gate regresses beyond its documented limit.

## 10. Definition of done

Phase 8 is complete only when both documented surfaces execute through the
embedded Rust API and CLI; catalog/hidden/provider state is atomic and
fail-closed; actual FTS plans, ranking, highlighting, churn, rebuild, limits,
rollback, reopen, abrupt exit, and corruption evidence pass; prior phase gates
remain green; `COMPAT.md`, `FORMAT.md`, API/CLI documentation, release
readiness, independent research, fixtures, raw benchmarks, and a Phase 8
report are current; and the complete Phase 8 diff is committed as its own
rollback point.

Completion does not authorize an alpha tag, package publication, artifact
upload, or a production-ready claim. Phase 9 remains required for the first
alpha-candidate surface, and Phases 10–12 remain required for Core 1.0.
