# FastDB on-disk format version 2

Status: retained migration input and Phase 6–11 historical contract. Current
FastDB transactionally migrates a valid format-2 file to format 3 on open; see
[`format-v3.md`](format-v3.md). The remainder of this document preserves the
exact source-format contract used during migration validation.

Status: Phase 8 implementation contract; not frozen for Core 1.0

Format 2 extends format 1 with relation metadata and sealed provider-owned
derived storage. It does not change existing `rid`, JSONB `doc`, record-ID,
value-envelope, path, or ordinary B-tree expression encodings. Format 0 remains
disposable. Format 1 upgrades transactionally and has no downgrade path.

## Compatibility header and open rules

`__fastdb_meta` keeps its format-1 columns and singleton invariant. A format-2
database has:

| Field | Required value |
| --- | --- |
| `format_version` | `2` |
| `dialect_version` | `1` |
| `last_migration` | `2` |

Open reads `format_version` before interpreting any other catalog. It rejects
format 0 and unknown future formats without mutation. Format 1 is accepted only
through the migration path below. Format 2 must have the exact reviewed catalog
shape, one valid metadata row, known capability/provider versions, valid
ownership, and exact opaque physical objects before its catalog snapshot is
published.

## Exact catalog schema

All catalogs are `STRICT`. The SQL below specifies the exact column order,
types, nullability, defaults, primary/unique keys, and checks. The implementation
constructs equivalent Turso AST using `SqliteDialect`; it never executes these
blocks as user input.

```sql
CREATE TABLE __fastdb_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    format_version INTEGER NOT NULL,
    dialect_version INTEGER NOT NULL,
    database_id TEXT NOT NULL UNIQUE,
    creation_version TEXT NOT NULL,
    last_migration INTEGER NOT NULL
) STRICT;

CREATE TABLE __fastdb_tables (
    table_id TEXT PRIMARY KEY,
    logical_name TEXT NOT NULL UNIQUE,
    physical_name TEXT NOT NULL UNIQUE,
    mode TEXT NOT NULL CHECK (mode IN ('SCHEMALESS', 'SCHEMAFULL')),
    definition TEXT,
    kind TEXT NOT NULL DEFAULT 'NORMAL',
    relation_in_table_id TEXT,
    relation_out_table_id TEXT,
    relation_enforced INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE __fastdb_fields (
    table_id TEXT NOT NULL,
    path_key TEXT NOT NULL,
    type_ast TEXT NOT NULL,
    required INTEGER NOT NULL CHECK (required IN (0, 1)),
    definition TEXT NOT NULL,
    PRIMARY KEY (table_id, path_key)
) STRICT;

CREATE TABLE __fastdb_indexes (
    index_id TEXT PRIMARY KEY,
    table_id TEXT NOT NULL,
    logical_name TEXT NOT NULL,
    physical_name TEXT NOT NULL UNIQUE,
    paths_json TEXT NOT NULL,
    unique_flag INTEGER NOT NULL CHECK (unique_flag IN (0, 1)),
    expression_version INTEGER NOT NULL,
    definition TEXT NOT NULL,
    index_kind TEXT NOT NULL DEFAULT 'BTREE',
    provider TEXT NOT NULL DEFAULT 'BUILTIN_BTREE',
    provider_version INTEGER NOT NULL DEFAULT 1,
    options_json TEXT NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'READY',
    encoding_version INTEGER NOT NULL DEFAULT 1,
    UNIQUE (table_id, logical_name)
) STRICT;

CREATE TABLE __fastdb_analyzers (
    analyzer_id TEXT PRIMARY KEY,
    logical_name TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL,
    provider_version INTEGER NOT NULL,
    options_json TEXT NOT NULL,
    definition TEXT NOT NULL
) STRICT;

CREATE TABLE __fastdb_hidden_columns (
    column_id TEXT PRIMARY KEY,
    table_id TEXT NOT NULL,
    index_id TEXT,
    field_path_key TEXT,
    physical_name TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL,
    provider_version INTEGER NOT NULL,
    physical_encoding TEXT NOT NULL,
    dimension INTEGER,
    options_json TEXT NOT NULL,
    state TEXT NOT NULL,
    encoding_version INTEGER NOT NULL
) STRICT;

CREATE TABLE __fastdb_capabilities (
    provider TEXT PRIMARY KEY,
    min_provider_version INTEGER NOT NULL,
    min_encoding_version INTEGER NOT NULL
) STRICT;
```

SQL checks intentionally cover storage-local scalar validity. Cross-row and
cross-catalog rules are validated by FastDB before publication because SQLite
foreign keys or cascades must not become an independently mutable ownership
model for hidden objects.

## Closed values and canonical options

Phase 8 accepts these committed values:

| Field | Accepted value |
| --- | --- |
| table `kind` | `NORMAL`, `RELATION` |
| index `index_kind` | `BTREE`, `GRAPH_ADJACENCY`, `FTS` |
| index `provider` | `BUILTIN_BTREE`, `BUILTIN_GRAPH`, `BUILTIN_FTS` |
| index `provider_version` | `1` |
| index `options_json` | exactly `{}` for B-tree; `{"direction":"forward"}` or `{"direction":"reverse"}` for graph adjacency |
| index `state` | `READY` |
| index `encoding_version` | `1` |

Graph hidden columns use provider `BUILTIN_GRAPH`, provider/encoding version
`1`, state `READY`, and physical encodings `GRAPH_TABLE_ID` or `GRAPH_RID`.
Their canonical role options are `{"role":"in_table"}`,
`{"role":"in_rid"}`, `{"role":"out_table"}`, and
`{"role":"out_rid"}`. A graph database contains the exact capability row
`BUILTIN_GRAPH,1,1`.

An FTS analyzer uses provider `BUILTIN_FTS_SURREAL_BLANK`, provider version
`1`, and canonical options `{"tokenizer":"blank"}`. An FTS index uses
provider `BUILTIN_FTS`, provider/encoding version `1`, state `READY`, and a
canonical JSON options object containing, in struct serialization order,
`surface`, `tokenizer`, `weights`, `analyzer`, and `highlights`. The Surreal
surface requires `whitespace`, one weight of `1.0`, one defined analyzer, and
one field. The FastDB extension allows the closed native tokenizer set and one
finite positive weight per ordered field, with no analyzer or Surreal
highlight flag.

Each FTS input owns one hidden nullable TEXT column using provider
`BUILTIN_FTS`, encoding `FTS_TEXT_UTF8`, an index ID, a canonical field path,
and role options `{"role":"fts_text","ordinal":N}`. An FTS database contains
the exact capability row `BUILTIN_FTS,1,1`. Vector provider values remain
unavailable. Unknown providers, newer versions, unknown encodings,
noncanonical options, and invalid lifecycle states fail open before catalog
publication.

Provider options are canonical JSON objects with lexicographically sorted keys,
no duplicate keys, and a provider-version-specific value schema. Empty B-tree
options encode as the two bytes `{}`. An options object is catalog data, never
SQL or a source fragment.

`READY` means physical derived state is complete and queryable.
`REBUILD_REQUIRED` is a reserved known state for later rebuildable providers;
ordinary B-tree indexes cannot commit in that state. Construction state is
transaction-local and is never a committed catalog value.

## Ownership invariants

- `table_id`, `index_id`, `analyzer_id`, and `column_id` are immutable 128-bit
  lowercase hexadecimal catalog IDs.
- Physical names are deterministic opaque names derived from immutable IDs.
- A `NORMAL` table has null relation endpoint IDs and
  `relation_enforced = 0`.
- A `RELATION` table may constrain either endpoint by immutable normal-table
  ID. Endpoint names are never persisted as ownership keys.
- Every relation table owns exactly four graph hidden columns and exactly two
  non-unique graph adjacency indexes. The forward order is in-table, in-rid,
  out-table, out-rid; the reverse order is out-table, out-rid, in-table,
  in-rid. Their internal logical names are `__graph_forward` and
  `__graph_reverse` and cannot be addressed through public index maintenance.
- Every index and hidden column owns an existing table. Optional `index_id`
  must name an index owned by the same table. Optional `field_path_key` uses the
  canonical path codec.
- Analyzer logical names are database-wide. Index logical names remain unique
  within a table.
- A capability row is required only when safely opening the database depends on
  a provider/encoding beyond the built-in format-2 B-tree baseline.
- No catalog row may point to a missing, extra, differently encoded, or
  differently named reserved physical object.
- Every FTS index owns exactly one hidden TEXT column per ordered indexed
  field. Ordinals are contiguous from zero; ownership, canonical paths, and
  physical column order must agree with the index definition.

## Format 1 to format 2 migration

Migration holds the process-local schema mutex and one engine immediate
transaction:

1. Validate the complete known format-1 metadata, exact catalog DDL, catalog
   ownership, records, B-tree definitions, and physical objects before mutation.
2. If `last_migration = 0`, include the existing level-0-to-1 metadata step in
   the same transaction. Format 1 accepts no other migration level.
3. Add the table metadata columns with canonical `NORMAL`/null/`0` defaults.
4. Add the index metadata columns with canonical
   `BTREE`/`BUILTIN_BTREE`/`1`/`{}`/`READY`/`1` defaults.
5. Create the analyzer, hidden-column, and capability catalogs. They are empty.
6. Re-read and validate row counts, IDs, names, definitions, paths, and all
   backfilled values inside the transaction.
7. Set `last_migration = 2` and then `format_version = 2` only after the prior
   checks pass. Commit once.
8. Re-read format-2 metadata/schema/capabilities before publishing the database
   handle or shared catalog generation.

Failure or abrupt exit before commit leaves a complete format-1 database.
Success leaves a complete format-2 database. A mixed schema/header is format
corruption. Reopening format 2 is a no-op and never rewrites documents or
renames physical objects.

Downgrade is unsupported. Back up or copy a database before upgrade when a
format-1 executable must remain usable.

## Physical document and derived columns

Migrated physical record tables remain exactly:

```sql
rid TEXT PRIMARY KEY,
doc BLOB NOT NULL
```

Format 2 allows later providers to append catalog-managed hidden typed columns
using opaque names derived from `column_id`. A hidden value is a deterministic
derivative of the same logical document or immutable relation endpoint. It is
maintained in the same transaction as `doc`, is never returned as a document
field, and can be validated/rebuilt from cataloged logical state.

Phase 7 adds no hidden column to an ordinary physical record table. A relation
physical table appends four opaque `TEXT NOT NULL` columns in canonical role
order:

```text
rid TEXT PRIMARY KEY,
doc BLOB NOT NULL,
<in_table> TEXT NOT NULL,
<in_rid> TEXT NOT NULL,
<out_table> TEXT NOT NULL,
<out_rid> TEXT NOT NULL
```

Table IDs are canonical 32-byte lowercase catalog-ID hex. Endpoint RIDs use
the existing version-1 RID codec. One edge insert binds `rid`, `doc`, and all
four endpoint values in a single physical INSERT. The document never stores
`id`, `in`, or `out`; FastDB synthesizes them from immutable physical state.
Catalog loading verifies exact column ownership, role cardinality, endpoint
table ownership, capability versions, index direction/order, and exact
physical DDL before publishing a snapshot.

An FTS-enabled table appends nullable opaque `TEXT` columns after any graph
columns. Missing or null logical values encode as SQL NULL; strings retain
their exact UTF-8 bytes. The document and every derived FTS column are bound by
one physical INSERT or UPDATE. Provider index maintenance participates in the
same engine transaction. Catalog loading validates the exact custom index,
column list, tokenizer/weight options, provider versions, ownership, and
physical table declaration before publishing a snapshot.

## Fixtures and freeze policy

Keep every committed format-1 and migration-level-0 fixture and digest. Phase 6
adds a format-2 fixture only after migration and new bootstrap tests pass. Each
fixture must reopen, accept a mutation, pass engine integrity, and demonstrate
ordinary B-tree selection.

Format 2 was evolved through Phases 7–10 and retained unchanged by the stopped
Phase 11 audit. Phase 12 superseded it with format 3 while keeping every
format-1/2 fixture as a validated migration input.
