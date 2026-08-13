# FastDB Phase 12 — Roadmap reset, compatibility inventory, and format 3

Status: authoritative implementation plan; implementation not started

Starting checkpoint: `aec1da0aa`

## 1. Purpose and boundaries

Phase 12 starts the approved pre-1.0 compatibility track after Phase 11 stopped
at its mandatory parallel-writer audit. It locks an atomic SurrealDB `v3.1.5`
capability inventory, makes that inventory the source for `COMPAT.md`, and
introduces transactional format 3 plus the non-geospatial value encodings
needed by later phases.

Stable Turso WAL and serialized writers remain mandatory through Phase 22. The
engine stays pinned to `977383ff40edc44ef410af062ed0d2322252a869` and the
behavioral reference stays pinned to the official, unmodified SurrealDB
`v3.1.5` binary. This phase does not add query operators, broad built-in
functions, authentication, a server, remote protocols, SDKs, parallel writers,
multiprocess access, geospatial values, history, changefeeds, LIVE queries,
GraphQL, or GQL.

No inherited Turso source change, upstream sync, generated SQLite text, public
plugin ABI, tag, package publication, artifact upload, push, or production-ready
claim is authorized.

## 2. Clean-room reference procedure

Capability discovery may use only public SurrealDB documentation and
independently designed black-box queries against the official `v3.1.5` binary.
If the binary is absent, install it outside the repository in a user cache or a
uniquely owned temporary directory. Record its download URL, version output,
SHA-256 digest, platform, observation date, exact input, and normalized output
in `docs/compat-research/phase12.md`.

Do not inspect, copy, translate, adapt, vendor, or derive names or expected
outputs from SurrealDB source, tests, fixtures, fuzz corpora, or implementation
details. Public online documentation may locate a feature, but moving online
behavior never changes the pinned `v3.1.5` contract without a new recorded
black-box observation.

## 3. Locked capability inventory

Create `compat/surrealdb-v3.1.5.toml` as the immutable-granularity inventory.
It has one metadata table and one ordered array of atomic capability tables.
Each capability contains exactly these fields:

- `id`: stable uppercase identifier, never reused;
- `area`: stable presentation group;
- `title`: concise public capability name;
- `phase`: integer `12..=20`, or `0` for an excluded/dormant capability;
- `disposition`: `target`, `excluded`, or `dormant`;
- `status`: `Supported`, `Partial`, or `Unsupported`;
- `syntax`: exact accepted or rejected surface summary;
- `provenance`: one or more public-doc or clean-room note references;
- `parser_evidence`: independently authored parser test IDs, possibly empty
  only for protocol or API capabilities with an explicit reason;
- `execution_evidence`: executable conformance IDs, empty for unsupported
  capabilities;
- `stop_report`: empty unless a targeted capability has an approved
  architecture-stop report.

Inventory granularity is locked by the Phase 12 checkpoint. Later phases may
change status, syntax precision, evidence, and an approved stop report, but may
not delete, merge, split, or renumber capabilities merely to improve coverage.
A genuinely omitted capability requires a separately reviewed inventory
amendment that explains the omission and keeps the prior IDs.

The inventory must cover all characterized non-geospatial `v3.1.5` values,
expressions, operators, functions, statements, schema objects, graph forms,
search/index forms, security objects, RPC methods, namespace/database behavior,
and SDK-facing protocol capabilities. It also records every roadmap exclusion:
geospatial/geometry, versioned history/changefeeds/time-series retention,
Realtime/LIVE/KILL, GraphQL, GQL, multiprocess access, and parallel writers.
FastDB-native FTS and ATTACH/DETACH entries are marked extensions and do not
count toward SurrealQL coverage.

While Phase 12 is active, only Phase 12 target rows may be `Partial`. Future
phase targets remain `Unsupported` until their phase begins. At each phase
checkpoint no row from a completed phase may remain `Partial`. By Phase 22 no
row may remain `Partial`; every targeted unsupported row must cite an approved
architecture-stop report.

## 4. Mechanical `COMPAT.md` synchronization

Replace the hand-maintained matrix body with deterministic Markdown rendered
from the locked inventory. Keep the explanatory preamble, clean-room warning,
status legend, exclusions, extension labels, and evidence links in the rendered
output. Add a repository tool that:

1. parses TOML with unknown-field rejection;
2. validates unique IDs, allowed enum values, phase/disposition combinations,
   ordering, nonempty syntax/provenance, evidence/status rules, and Partial-row
   ownership by the active phase;
3. renders byte-for-byte deterministic `COMPAT.md`;
4. supports `--check` without mutation and fails when the checked-in Markdown
   differs; and
5. is exercised by normal FastDB tests and CI.

The renderer is repository tooling only. It does not generate SQL, database
input, executable compatibility tests, or black-box expected outputs.

## 5. Public value contract

Extend the public `Value` model with owned, validated representations for:

- `Bytes(Vec<u8>)`;
- `Datetime`, normalized to a UTC instant with nanosecond precision and a
  canonical RFC 3339 rendering;
- nonnegative `Duration`, represented as checked seconds plus nanoseconds;
- `Decimal`, stored as a canonical decimal string within the characterized
  `v3.1.5` precision/range, with no exponent, redundant leading/trailing zero,
  or negative-zero ambiguity;
- `Set`, held in canonical FastDB total-value order with duplicates removed;
- `Range`, containing optional boxed bounds and explicit inclusive/exclusive
  flags; and
- richer typed collections in schema metadata, while runtime vectors remain
  ordinary arrays as required by the existing contract.

Constructors and decoders reject invalid UTF-8 where text is required,
noncanonical decimal encodings, non-finite numeric values, invalid datetime or
duration ranges, duplicate/noncanonical set encodings, and malformed ranges.
Resource accounting includes all new value payloads with checked arithmetic.

This phase makes literals and bound values round-trip where the current grammar
already has an unambiguous form. Broader casts, access, comparison, containment,
arithmetic, slicing, and function behavior belong to Phase 13. Unsupported
source forms continue to fail explicitly.

## 6. Collision-safe document and wire encoding

Format 3 introduces value-envelope version 2 for documents and public JSON.
The existing one-key `$fastdb` escape rule remains collision-safe: a user object
whose only or any key could be mistaken for an envelope is encoded as an
explicit escaped-object envelope. New tagged kinds use exact field sets and
canonical payloads. Unknown versions, unknown kinds, extra fields, invalid
payloads, and noncanonical encodings fail closed.

Format-1/2 value-envelope version 1 remains readable. A document is rewritten
to envelope version 2 only by an ordinary successful document mutation; the
format migration does not scan or rewrite user documents. This preserves
bounded migration time and keeps `doc` authoritative. API/CLI JSON always emits
the current public envelope version and accepts the documented previous version
only where lossless decoding is possible.

Bytes use base64url without padding in JSON envelopes. Datetimes, durations,
and decimals use canonical strings. Sets encode an ordered array of encoded
values. Ranges encode explicit bound-presence and inclusivity fields so
unbounded and null bounds cannot collide. Record IDs retain their existing
typed payload and gain no lossy string-only representation.

## 7. Format 3 catalogs

Format 3 retains every format-2 physical record table, opaque physical name,
provider column, index, and authoritative document. It extends catalogs for
later schema, function/event, permission, security, and provider metadata by
adding sealed tables with opaque immutable IDs:

- `__fastdb_functions` for canonical custom-function definitions and limits;
- `__fastdb_parameters` for database parameters and encoded values;
- `__fastdb_views` for canonical view definitions and dependency metadata;
- `__fastdb_events` for canonical event definitions and recursion metadata;
- `__fastdb_permissions` for owner scope, action, canonical predicate, and
  expression versions;
- `__fastdb_users` and `__fastdb_accesses` for security definitions only (no
  plaintext credentials or signing secrets are introduced in Phase 12); and
- provider metadata columns sufficient to version future FastDB-owned
  specialized auxiliary state.

Every new catalog is `STRICT`, has exact checked columns, uses catalog IDs for
ownership, and is empty after a format-1/2 migration. Definitions are retained
only as canonical FastDB source plus independently versioned AST/expression
metadata; they are never interpolated into generated SQLite text. Phase 12
catalog loading validates exact DDL, row shape, ownership, versions, and
capabilities before publishing a snapshot. Executing the new cataloged objects
belongs to their owning later phases.

`format_version`, `last_migration`, and the document envelope version become
`3`, `3`, and `2` respectively. Dialect and existing expression/provider
versions change only if the implementation actually changes those contracts.
Unknown future format, catalog, expression, provider, or encoding versions are
refused before mutation.

## 8. Transactional migration and bootstrap

Open reads only the minimal metadata header before deciding whether to migrate.
Format 1 and format 2 migrate directly to format 3 while holding the
database-level schema mutex and one immediate transaction:

1. validate the complete source header, exact catalog DDL, ownership,
   capabilities, physical objects, and provider state;
2. for format 1, perform the already proven format-1-to-2 steps inside the same
   transaction without publishing an intermediate catalog snapshot;
3. create every new exact format-3 catalog and add provider metadata columns;
4. validate empty-row and backfill invariants inside the transaction;
5. set `last_migration = 3`, then publish `format_version = 3` last;
6. commit once, reopen/validate the complete format-3 state, and only then
   publish the shared catalog snapshot.

Bootstrap creates format 3 directly in one transaction. Reopening format 3 is
a read-only no-op. Failpoints cover every DDL/backfill/header/publication
boundary. Any injected error or abrupt process exit leaves either a complete
source format or a complete format 3 database; a mixed header/schema fails as
corruption. Migration never deletes source data and has no downgrade path.

## 9. Backup, restore, and compatibility preservation

The Phase 10 supported backup/check/restore path must understand format 3 and
validate every new catalog. Backups created before migration remain valid
format-1/2 migration inputs. A format-3 backup restores and reopens without
mutation, preserves logical hashes, and reports the current format.

All Phase 6–10 graph, FTS, vector, B-tree, limits, lifecycle, CLI, backup,
restore, check, rebuild, abrupt-exit, and benchmark gates remain unchanged.
Format-1 and format-2 fixtures and their recorded hashes remain historical
inputs; add a format-3 fixture only after migration and new-value tests pass.

## 10. Implementation order and commits

1. Commit this authoritative plan by itself.
2. Add and lock the inventory, deterministic renderer/checker, generated
   `COMPAT.md`, and clean-room Phase 12 research note.
3. Add the public value types, strict canonical codecs, resource accounting,
   parser/parameter/CLI round trips that are owned by this phase, and focused
   tests.
4. Add exact format-3 catalogs, direct-AST bootstrap/migration, failpoints,
   catalog validation, backup/check integration, fixtures, and format docs.
5. Run the full verification matrix, write `docs/phase12-report.md`, update
   release readiness with observed evidence only, and commit the isolated
   `phase 12 complete` checkpoint with starting and ending SHAs.

Focused commits are allowed, but no Phase 13 implementation starts before the
Phase 12 checkpoint passes. Rollback uses explicit Git reverts of the recorded
Phase 12 commit range.

## 11. Verification matrix

At minimum, independently authored tests must prove:

- every inventory ID is unique, atomic, assigned, and deterministically
  rendered; deletion, split/merge-like replacement, invalid status, evidence
  omission, and stale `COMPAT.md` fail the checker;
- new values round-trip through in-memory and on-disk documents, nested arrays
  and objects, parameters, Rust API JSON, CLI JSON, backup/restore, reopen, and
  abrupt exit without collisions with adversarial `$fastdb` user objects;
- canonical encoding is deterministic; malformed, noncanonical, oversized,
  deeply nested, duplicate-set, invalid-range, non-finite, overflow, and
  unknown-tag inputs fail safely;
- new schema field types enforce their declared shape while all existing field
  and fixed-vector behavior remains unchanged;
- pristine bootstrap creates exact format 3; committed format-1 and format-2
  fixtures migrate and reopen; an already format-3 file is byte-stable across
  reopen after checkpoint;
- each migration failpoint rolls back byte-for-byte to the source fixture or
  recovers to one complete version after abrupt exit;
- unknown future format and every unknown catalog/provider/encoding version are
  refused before mutation;
- provider-derived graph/FTS/vector state, ordinary index plans, backup hashes,
  restore validation, integrity checks, and rebuilds survive migration; and
- unchanged Phase 5–10 functional, crash, fuzz, Turso regression, and
  performance gates still pass.

Discover exact package names with `cargo metadata`. The final evidence includes
format checks, `git diff --check`, clippy for every changed FastDB target,
parser/frontend/API/CLI/integration tests, structured fuzzing, migration crash
helpers, unchanged relevant Turso suites, fixture digests, release builds, and
equivalent native-workload benchmarks. Long-running commands and raw benchmark
measurements are recorded in `docs/phase12-report.md`.

## 12. Stop conditions

Stop and write an architecture report rather than weakening the contract if:

- a value cannot be represented collision-safely and losslessly through the
  public API, document codec, and supported transports;
- migration requires interpolating a logical identifier, generating SQLite
  from user input, rewriting every document, publishing a mixed catalog, or
  editing inherited Turso core;
- exact source-format validation or rollback cannot distinguish complete
  format 1/2/3 states after failure;
- inventory coverage can be made tractable only by coarse merged rows, copied
  SurrealDB materials, or mutable row granularity;
- new resource accounting can overflow or accepts unbounded decode/allocation;
  or
- any acknowledged-write, provider atomicity, backup, integrity, security
  boundary, or Phase 5–10 compatibility/performance gate regresses.

## 13. Definition of done

Phase 12 is complete only when the locked machine-readable inventory and
mechanically synchronized `COMPAT.md` pass; all Phase 12 capabilities are
Supported or have an approved architecture-stop report; format 3 and the
collision-safe non-geospatial values pass migration, rollback, reopen,
backup/restore, abrupt-exit, integrity, limits, and unchanged regression gates;
and `docs/phase12-report.md` records the exact commands, fixtures, binary
provenance, starting SHA, ending SHA, and remaining risks.

Completion is a pre-1.0 compatibility checkpoint only. It does not authorize
Phase 13 behavior, a release, publication, parallel writers, or a
production-ready Core 1.0 claim.
