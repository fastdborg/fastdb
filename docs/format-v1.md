# FastDB on-disk format version 1

Status: stable Core format introduced by Phase 2. Format 0 was a disposable prototype and is never upgraded in place.

## Compatibility header and open rules

`__fastdb_meta` contains exactly one row selected by `singleton = 1`:

| Column | Storage and invariant |
| --- | --- |
| `singleton` | INTEGER primary key, always `1` |
| `format_version` | INTEGER, exactly `1` |
| `dialect_version` | INTEGER, exactly `1` |
| `database_id` | unique TEXT, 32 lowercase hexadecimal digits |
| `creation_version` | nonempty TEXT package version |
| `last_migration` | INTEGER, currently `0` or `1` while opening and `1` after migration |

All catalog and physical tables are `STRICT`. Open first inspects `sqlite_schema`. A truly empty database is accepted without creating any object. A nonempty database without `__fastdb_meta` is rejected. If metadata exists, FastDB reads only `format_version` first; format 0 and unknown versions are rejected before the remaining catalogs are interpreted or mutated.

After the version gate, FastDB requires the exact reviewed catalog DDL, a single well-typed metadata row, dialect 1, a known migration level, valid ownership, deterministic physical names, and an exact physical table/index definition for every catalog entry. Missing, mismatched, duplicate, or orphan `__fastdb_` objects are format corruption.

Migration level `0` to `1` is a transactional no-op over user data and catalogs; it changes only `last_migration`. A failed commit leaves level 0 intact. Reopening level 1 performs no migration.

## Catalogs

`__fastdb_tables`:

| Column | Meaning |
| --- | --- |
| `table_id TEXT PRIMARY KEY` | immutable 128-bit ID as 32 lowercase hex digits |
| `logical_name TEXT UNIQUE NOT NULL` | user table name, always bound |
| `physical_name TEXT UNIQUE NOT NULL` | `__fastdb_t_` plus the table ID |
| `mode TEXT NOT NULL` | checked `SCHEMALESS` or `SCHEMAFULL` |
| `definition TEXT NULL` | original DEFINE source; null means implicit schemaless registration |

`__fastdb_fields` has `table_id`, canonical `path_key`, canonical `type_ast`, checked `required` integer, and original `definition`. `(table_id, path_key)` is the primary key. Ownership and parent/descendant type consistency are validated when loading.

`__fastdb_indexes` has immutable `index_id`, owner `table_id`, logical and physical names, `paths_json` containing an ordered array of canonical paths, checked `unique_flag`, `expression_version = 1`, and original `definition`. `(table_id, logical_name)` and every physical name are unique. Physical names are `__fastdb_i_` plus the 128-bit index ID.

Logical names and definitions are bound data. Opaque physical identifiers are derived only from validated immutable IDs.

## Physical records and record IDs

Each logical table owns one exact physical table:

```sql
rid TEXT PRIMARY KEY,
doc BLOB NOT NULL
```

The table is strict. `doc` is Turso JSONB produced from bound canonical JSON; it never stores a top-level `id`. Result decoding synthesizes a typed logical ID from `rid`.

RID component encoding is prefix-free and versioned:

```text
v1:s:<UTF-8-byte-length>:<text>
v1:i:<canonical-i64>
v1:u:<lowercase-hyphenated-UUID>
```

String length counts UTF-8 bytes. Integer spellings are canonical decimal, and UUIDs must be canonical v4/v7. Unknown prefixes, wrong lengths, noncanonical integers/UUIDs, and unsupported UUID versions are format errors.

## Values and reserved envelopes

Frontend values are null, bool, integer, finite float, UTF-8 string, array, lexicographically ordered object, and typed record ID. Duplicate input object keys normalize last-value-wins. Integer and float remain distinct representations.

An embedded record ID is stored as:

```json
{"$fastdb":{"v":1,"t":"rid","table":"person","id":"v1:s:5:tracy"}}
```

A user object containing the reserved `$fastdb` key is escaped as a version-1 `t:"object"` envelope whose `value` member contains the encoded user object. Decoding is recursive. An unknown version/tag, missing or extra tag member, invalid embedded RID, non-finite/out-of-range number, invalid document root, or stored top-level `id` is format corruption.

## Paths, schema, and indexes

Every path segment is JSON-escaped and dot-quoted. For example, segments `profile`, `age` encode as `$."profile"."age"` (rendered normally as `$."profile"."age"`). Quotes, dots, brackets, backslashes, control characters, and Unicode inside a segment are data, not syntax.

The same canonical path and `json_extract(doc, path)` AST builders are used for reads, filters, validation, and expression-index DDL. SET uses the same canonical path with `json_set`. Expression version 1 indexes are ordered tuples of these expressions.

Indexable path values are missing, null, bool, integer, finite float, and string. Missing/null entries may repeat in unique indexes; equal complete non-null tuples may not. Existing documents are decoded and validated before index DDL, and future writes validate every cataloged index path before insertion.

## Atomicity and cache visibility

Catalog bootstrap, implicit table registration, physical table creation, record insertion, migration, field validation/catalog insertion, and index validation/DDL/catalog insertion run in immediate transactions under the shared schema mutex. The process-local catalog cache is write-locked through commit and replaced only after a successful commit. Rollback, injected commit failures, and real WAL-sync completion failures leave persistent catalog ownership and the shared cache unchanged.

Direct external changes to catalogs or hidden objects are unsupported. They are diagnosed on reopen when structural validation can identify them; malformed stored records are diagnosed as format corruption when decoded.

## Fixture and upgrade policy

Format 1 is the only stable Core format in this release candidate. Unknown
future format, dialect, expression, or migration versions are refused before
mutation; format 0 is disposable and has no upgrade path. A release that
changes physical layout or semantics must add an explicit transactional
migration, retain the prior fixture, add a new fixture and provenance digest,
and prove reopen, rollback, integrity, and index-plan behavior before changing
the supported-version constants.

The committed Phase 3 format-1 and migration-level-0 artifacts live under
`fastdb-tests/fixtures/`. Their SHA-256 provenance is verified separately from
behavioral open/migrate/mutate/reopen tests. Empty `-wal` placeholders can
remain after a clean checkpoint in the pinned engine; nonempty sidecars are
engine-owned recovery state and must never be deleted manually.
