# Phase 13 Architecture Stops

Status: Phase 13 checkpoint evidence

Reference: unmodified SurrealDB `v3.1.5`

These stops apply only to the atomic Phase 13 capabilities listed below. They
do not broaden a capability, turn parser acceptance into support, or prevent a
later owning phase from reopening the row after the required execution and
security context exists. Every listed function remains unavailable; unknown
or stopped calls fail before mutation.

## Statement-owned expressions

Capabilities: `EXPR-FUTURE`, `EXPR-OBJECT-PICK`, `EXPR-SUBQUERY`.

Future blocks and subqueries require bounded re-entry through the complete
FastDB statement frontend, transaction poisoning, cancellation, and result
materialization. Object pick syntax is coupled to Phase 14 projection and
destructuring semantics. Publishing scalar-evaluator shortcuts would either
bypass the frontend or create semantics that cannot meet the atomicity and
resource gates. The rows remain stopped until their Phase 14/15 statement
owners integrate them without generated SQLite text.

## Immutable-reference mismatches

Capabilities: `FN-ARRAY-EVERY`, `FN-ARRAY-INCLUDES`,
`FN-ARRAY-INDEX-OF`, `FN-ARRAY-SOME`,
`FN-STRING-DISTANCE-OSA-DISTANCE`, `FN-STRING-ENDSWITH`,
`FN-STRING-STARTSWITH`, every `FN-STRING-IS-NESTED-*` row,
`FN-RAND-GUID`, `FN-TYPE-THING`, and `FN-META-TABLE`.

Independent probes against the fixed binary returned an unknown-function
error for these exact catalog spellings and identified different `v3.1.5`
spellings where applicable. Claiming these aliases as SurrealDB-compatible
would contradict the immutable behavioral reference. Some aliases are
defensively recognized internally, but the locked compatibility rows remain
Unsupported and are not evidence for another function.

## Values outside the public format contract

Capabilities: `FN-TIME-MINIMUM`, `FN-TIME-MAXIMUM`, `FN-MATH-INF`,
`FN-MATH-INFINITY`, `FN-MATH-NEG-INF`, `FN-MATH-NEG-INFINITY`.

The reference time constants fall outside FastDB's format-3 year
`1..=9999` datetime domain. The math constants are non-finite, while format 3,
bound-value validation, JSON encoding, indexes, and the public `Value`
contract deliberately reject non-finite numbers. Adding either group in this
phase would require an unplanned format/public-contract migration and would
invalidate existing storage and client invariants.

## Database and graph context

Capabilities: `FN-RECORD-EXISTS`, `FN-RECORD-IS-EDGE`, `FN-RECORD-REFS`.

These functions require catalog resolution, a transaction snapshot, graph
metadata, and—after Phase 18—permission-filtered visibility. Implementing
them as context-free scalar helpers would leak record existence or bypass
authorization; issuing nested engine queries would also violate the direct
frontend execution boundary. Phase 16/18 must provide an authorization-aware
evaluation context before these rows can be reopened.

## Request, response, and session context

Capabilities: `FN-API-INVOKE`, every `FN-API-REQ-*` and `FN-API-RES-*` row,
and every `FN-SESSION-*` row.

The embedded Phase 13 connection has no authenticated principal, selected
namespace/database, peer address, HTTP request, or response builder. Inventing
ambient or process-global values would break session isolation and create a
confused-deputy path. Phase 18 owns principals and authorization; Phase 19
owns isolated request/WebSocket session contexts. These functions remain
unavailable until those contexts are threaded through the same frontend.

## Ambient resource authority

Capabilities: all `FN-HTTP-*` and `FN-FILE-*` rows.

No Phase 13 API grants network or filesystem capabilities. A direct host HTTP
client or arbitrary path implementation would introduce prohibited ambient
authority and could not satisfy DNS-rebinding, redirect, SSRF, symlink,
timeout, response-size, concurrency, redaction, and cancellation gates.
Calls therefore fail before external work. A later implementation must use
explicit opaque capabilities and adversarial provider tests; inherited Turso
bindings are not an alternative.

## Re-entrant and transactional side effects

Capabilities: `FN-EVAL-SURQL`, `FN-SEQUENCE-NEXT`,
`FN-SEQUENCE-NEXTVAL`, and `FN-SLEEP`.

Dynamic evaluation must reparse and execute through FastDB with reduced nested
budgets and recursion protection. Sequences require format-3 catalog state,
schema serialization, and participation in the caller's transaction. Sleep
must be asynchronous, cancellation/deadline-aware, and must not block the
serialized database worker. The current scalar evaluator cannot supply those
properties. Ad-hoc parsing, engine calls, process-global counters, or blocking
sleeps would violate the architecture and resource gates, so these rows remain
stopped for their Phase 15/API owners.

## Reopening rule

A later phase may remove a stop only in the same commit that adds executable
conformance evidence and all context-specific atomicity, authorization,
resource, and recovery tests. The inventory ID, reference version, and row
granularity remain locked.
