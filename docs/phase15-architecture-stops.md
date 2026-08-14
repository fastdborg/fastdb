# Phase 15 architecture stops

Status: active Phase 15 evidence

Behavioral reference: official, unmodified SurrealDB `v3.1.5`

These stops are deliberately narrow. They do not turn rejected syntax into
support, broaden another compatibility row, or permit fallback to generated
SQLite text, experimental MVCC, ambient host authority, or a sidecar file.

## Sequence definitions and allocation

Capabilities: `SCHEMA-DEFINE-SEQUENCE`, `SCHEMA-ALTER-SEQUENCE`,
`SCHEMA-INFO-SEQUENCE`, and `SCHEMA-REMOVE-SEQUENCE-COMPLETE`.

The fixed reference canonicalized a default definition as `BATCH 1000 START
0`, exposed only `sequence::nextval` (not the inventoried `sequence::next`
alias), and burned allocations across both `CANCEL` and a failed explicit
transaction. A `BATCH 1 START 10` probe allocated inside `BEGIN; ...; CANCEL`
and the next standalone call returned `11`. A second failed transaction burned
`12`, and the following call returned `13`.

The pinned Turso engine has a native sequence AST and durable sequence state,
but its supported stable-WAL mode participates in the caller transaction. An
independent native probe ran `CREATE SEQUENCE s START WITH 10; BEGIN; SELECT
nextval('s'); ROLLBACK; SELECT nextval('s');` and returned `10` twice. Its
autonomous sequence transaction is tied to the experimental MVCC facility
that failed the Phase 11 production audit.

FastDB cannot emulate the reference rule through a second stable-WAL
connection once an outer data transaction owns the single writer. Committing
the outer transaction early would violate statement atomicity; an in-memory
counter would lose acknowledged uniqueness on crash; a sidecar would violate
the single-artifact and backup contracts; generated SQL would violate the
frontend architecture; and enabling experimental MVCC would violate the Phase
11 stop. Batch reservation cannot close the boundary for `BATCH 1` when an
allocation follows an already-written outer transaction. These four schema
rows therefore remain Unsupported until a stable engine primitive can durably
burn an allocation without publishing or replaying the caller transaction.

## Surrealism modules

Capability: `SCHEMA-DEFINE-MODULE`.

The fixed reference rejected `DEFINE MODULE` unless its experimental
`surrealism` capability was enabled. The public surface loads a `.surli` WASM
artifact through a file bucket and can expose scripting, arbitrary-query,
network, and attached-filesystem capabilities. FastDB has no sealed module
provider that validates artifact provenance and ABI version while enforcing
deterministic imports, fuel, memory, stack, deadline, output, re-entrancy, and
authorization budgets.

Accepting a file pointer without execution would publish a dangling function
namespace. Loading through WASI or a native dynamic library would introduce
ambient authority and an arbitrary plugin ABI. Implementing the runtime in
Turso core would be an unqualified engine change. `DEFINE MODULE` therefore
remains Unsupported and fails before catalog or filesystem mutation. A future
provider may reopen the row only with a FastDB-owned versioned ABI and
adversarial sandbox/recovery evidence.

## Server API lifecycle

Capabilities: `SCHEMA-ALTER-API`, `SCHEMA-INFO-API`, and
`SCHEMA-REMOVE-API-COMPLETE`.

The corresponding `SCHEMA-DEFINE-API` row belongs to Phase 19 because an API
definition is meaningful only with authenticated request routing, method/path
matching, `$request` isolation, response limits, and authorization. Phase 15
has no server, selected remote session, or API definition owner to alter,
inspect, or remove. Persisting inert API metadata now would create definitions
whose security envelope cannot be validated and whose lifecycle could diverge
from the Phase 19 route catalog.

These three lifecycle rows remain Unsupported at the embedded Phase 15
boundary and reject before mutation. Phase 19 must reopen all four API rows
together when definition, routing, authentication, authorization, collision,
and malformed-request tests share one catalog and transaction contract.

## Bucket lifecycle

Capabilities: `SCHEMA-DEFINE-BUCKET`, `SCHEMA-ALTER-BUCKET`,
`SCHEMA-INFO-BUCKET`, and `SCHEMA-REMOVE-BUCKET-COMPLETE`.

The fixed reference exposes experimental memory, local-file, and global bucket
backends. A memory backend cannot satisfy reopen, backup, or acknowledged
durability. A local/global backend stores mutable bytes outside the one
`.fastdb` artifact, so a catalog transaction cannot atomically publish file
content, rollback it, include it in `backup_to`, or prove crash recovery. The
format-3 catalogs also deliberately have no bucket owner: adding inert metadata
would create file pointers without the Phase 18 authorization and Phase 19
request capability boundaries.

FastDB therefore rejects every bucket lifecycle form before catalog or
filesystem mutation. This is not a denial of future object storage. A later
sealed provider must define versioned content identity, transactional staging,
rollback/recovery, backup/restore, canonical-directory or object-store policy,
authorization, quotas, and no ambient filesystem authority. Arbitrary storage
plugins and host paths remain prohibited.

## Mixed TYPE ANY tables

Capability: `SCHEMA-DEFINE-TABLE-COMPLETE`.

The fixed reference uses `TYPE ANY` for a table that may contain both ordinary
and relation records. FastDB format 3 assigns one immutable physical kind to
each table: NORMAL rows have only document state, while RELATION rows have
mandatory immutable endpoint columns and two adjacency indexes. Mapping ANY to
NORMAL would silently reject relation records; mapping it to RELATION would
invent endpoints for ordinary records. Allowing nullable endpoint state in the
current provider would invalidate the Phase 7 catalog and adjacency invariants
for every existing format-2/3 fixture.

Phase 15 freezes format 3 and cannot reinterpret an existing table kind during
reopen or backup/restore. A qualifying implementation therefore needs a later
transactional format migration and a versioned mixed-record graph provider,
including normal/relation decode discrimination, adjacency maintenance,
cascade behavior, integrity checking, rebuild, and failure injection. Until
that representation exists, explicit `TYPE ANY` remains Unsupported and fails
before mutation. Explicit NORMAL and RELATION tables and every other
characterized Phase 15 table clause remain executable; the locked complete-row
status is nevertheless Unsupported because its atomic capability includes
mixed ANY.
