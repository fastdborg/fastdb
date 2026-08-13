# FastDB on-disk format version 3

Status: current pre-1.0 embedded format, introduced by Phase 12.

Format 3 retains every format-2 logical table, opaque physical name, document,
hidden provider column, and provider index. It adds collision-safe value
envelope version 2, sealed catalogs for later compatibility features, and an
explicit auxiliary-state version on provider-owned catalog rows. Documents
remain authoritative and are not rewritten during migration.

## Metadata

`__fastdb_meta` remains a singleton strict table. Its current values are:

- `format_version = 3`;
- `dialect_version = 1`;
- `last_migration = 3`; and
- `document_encoding_version = 2`.

The document encoding version is constrained by the catalog DDL. Unknown
format, dialect, migration, document, expression, provider, encoding, or
auxiliary-state versions fail before a catalog snapshot is published.

## Value encoding

The document codec uses a one-key `$fastdb` envelope. Version-2 tags cover
NONE, record IDs, escaped user objects, bytes, datetime, decimal, duration,
file references, ranges, regexes, sets, table values, and UUIDs. Tags have
exact member sets. Unknown versions/kinds, extra members, noncanonical
payloads, oversized values, excessive nesting, and malformed values are format
corruption.

Bytes use unpadded base64url. Datetimes use canonical UTC RFC 3339 with up to
nanosecond precision. Decimals use normalized checked strings. Durations use
checked seconds and nanoseconds. Sets encode their deterministic total-order
sequence after numeric-equivalence deduplication. Ranges encode each bound as
unbounded, included, or excluded, with a nested value only for present bounds.

Version-1 record/object envelopes remain readable. An ordinary successful
document mutation re-encodes the mutated document with current tags; migration
does not scan or rewrite physical record tables.

## Existing multimodel catalogs

Format 3 retains the exact format-2 table, field, analyzer, capability, hidden
column, and provider ownership contracts. `__fastdb_indexes` and
`__fastdb_hidden_columns` append `auxiliary_version INTEGER NOT NULL DEFAULT 1`.
The loader requires version 1 and continues to validate all graph, FTS, vector,
and B-tree physical ownership before publication.

## Sealed future catalogs

Bootstrap and migration create these empty strict catalogs:

- `__fastdb_functions`;
- `__fastdb_parameters`;
- `__fastdb_views`;
- `__fastdb_events`;
- `__fastdb_permissions`;
- `__fastdb_users`; and
- `__fastdb_accesses`.

Their reviewed DDL reserves immutable IDs, logical definitions, independently
versioned AST/expression metadata, ownership, limits, and provider/security
options. Phase 12 rejects nonempty rows because execution ownership belongs to
later phases. No credentials, password hashes, signing keys, or executable
definitions are introduced by the migration.

## Bootstrap and migration

Pristine bootstrap creates all fourteen catalogs and the singleton header in
the same immediate transaction as the first logical mutation.

Format 1 migrates directly to format 3 in one immediate transaction: FastDB
first performs and validates the established format-1→2 DDL steps without
publishing an intermediate header, then applies the format-3 additions.
Format 2 follows only the latter steps. In both paths FastDB:

1. validates the complete source header, exact DDL, catalog ownership,
   provider state, and physical objects;
2. adds the document/provider version columns and sealed catalogs;
3. validates empty future catalogs and unchanged existing ownership;
4. publishes `last_migration = 3` and `format_version = 3` last; and
5. commits once before publishing the process-local snapshot.

Failure injection covers every DDL, validation, and header boundary. Rollback
leaves the exact source fixture; recovery yields either a complete source
format or a complete format 3 file. There is no downgrade path.

## Fixtures and operations

Committed format-1 and format-2 fixtures remain immutable source inputs with
recorded digests. Phase 12 tests migration, reopen, mutation, integrity,
provider plans, failure rollback, backup/restore, and unknown-version refusal.
The supported check and backup paths validate format 3 through the same catalog
loader. A clean, checkpointed database remains one `.fastdb` artifact; active
WAL sidecars remain engine-owned recovery state.
