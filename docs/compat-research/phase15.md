# Phase 15 clean-room compatibility observations

Behavioral reference: official, unmodified SurrealDB `v3.1.5`

Observation date: 2026-08-14

This note records independently designed black-box probes and public
documentation used for Phase 15 scripting and schema behavior. No SurrealDB
source, tests, fixtures, expected-output files, or implementation details were
inspected or copied.

## Reference binary

- Download URL:
  `https://github.com/surrealdb/surrealdb/releases/download/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`.
- Installed outside the repository under
  `/home/tan/.cache/fastdb-reference/surreal-v3.1.5/`.
- Archive SHA-256:
  `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
- `surreal version`: `3.1.5 for linux on x86_64`.
- Backend: ephemeral `memory`; namespace `fastdb`; isolated Phase 15
  databases per probe group.

The moving public starting points were the SurrealDB pages for
[statements](https://surrealdb.com/docs/reference/query-language/statements/overview),
[LET](https://surrealdb.com/docs/reference/query-language/statements/let),
[FOR](https://surrealdb.com/docs/reference/query-language/statements/for), and
[DEFINE](https://surrealdb.com/docs/reference/query-language/statements/define).
They supplied syntax candidates only; the fixed binary determined the
observations below.

## Script scope and result boundaries

The following independently authored probe established lexical block scope:

```surql
LET $x = 1;
IF true { LET $x = 2; };
RETURN $x;
```

The ordered results were `NONE`, `NONE`, and `1`. A binding introduced or
shadowed inside an IF/FOR body did not replace the outer binding after that
block returned.

RETURN supplied the value of its immediate statement/block boundary rather
than terminating the rest of the top-level request:

```surql
IF true { RETURN 1; };
RETURN 2;
```

The results were `1` and `2`. An IF branch without a RETURN exposed the result
of its last executed statement. For example, an IF containing CREATE returned
the created record array, while a later statement in the same block was not
executed after RETURN.

False IF without ELSE and blocks ending in LET returned `NONE`. ELSE IF was
evaluated lazily in source order.

## FOR, BREAK, and CONTINUE

An array iteration probe used BREAK and CONTINUE inside nested IF blocks:

```surql
FOR $x IN [1, 2, 3] {
    IF $x = 2 { CONTINUE };
    IF $x = 3 { BREAK };
    CREATE item CONTENT { n: $x };
};
SELECT VALUE n FROM item ORDER BY n;
```

The FOR result was `NONE` and the selected values were `[1]`. Thus ordinary
body statement results are not exposed as the FOR result. A direct RETURN in
the body stopped the loop and became the FOR result; a subsequent top-level
RETURN still executed. Array order was preserved. FastDB additionally accepts
the locked bounded set and integer-range surfaces and rejects unbounded or
non-collection iterables.

BREAK and CONTINUE outside a loop were rejected. Their control signals crossed
nested IF blocks but did not escape the enclosing FOR statement.

## THROW and statement atomicity

Two CLI requests in one in-memory session were:

```surql
CREATE item:kept SET n = 1; THROW 'stop';
SELECT * FROM item:kept;
```

The first request returned the created record followed by an error result, and
the second request found `item:kept`. Therefore THROW stops subsequent script
execution but does not retroactively roll back earlier standalone statements.
FastDB follows that boundary. Within an explicit transaction guard, its
existing poison-on-error contract rolls back all guarded writes. FastDB redacts
the THROW payload from public error detail so user values cannot enter logs or
metadata-only hooks.

## SLEEP and bounded execution

This probe returned `NONE` and then `"awake"`:

```surql
SLEEP 1ms;
RETURN 'awake';
```

FastDB accepts only duration values, caps a single SLEEP at five seconds,
composes it with the request deadline, and checks cooperative interruption in
bounded intervals. Script statements, loop iterations, expression collection
sizes, API output, vector dimensions, FTS query bytes, and graph hops retain
their existing independent ceilings.

## Parameters and initial function shapes

The following definitions were accepted and persisted by the reference:

```surql
DEFINE PARAM $answer VALUE 42;
RETURN $answer;
INFO FOR DB;

DEFINE FUNCTION fn::double($x: int) { RETURN $x * 2; };
RETURN fn::double(4);
INFO FOR DB;
```

The calls returned `42` and `8`. Canonical INFO output included `PERMISSIONS
FULL` for both definitions.

`DEFINE PARAM IF NOT EXISTS` retained an existing definition, while `DEFINE
PARAM OVERWRITE` replaced it. `ALTER PARAM` accepted VALUE, PERMISSIONS, or
both and rejected an absent parameter. `REMOVE PARAM IF EXISTS` was an
idempotent no-op. The fixed binary exposed parameter definitions through the
`params` object returned by `INFO FOR DB`; it rejected `INFO FOR PARAM`, so
FastDB follows the database-information surface rather than inventing a
per-parameter target.

A top-level LET shadowed a database parameter only for that request. FastDB
also gives an explicitly bound request parameter precedence for that request;
catalog mutation does not silently replace an existing request/LET binding.
Stored values use the collision-safe format-3 value envelope and remain
authoritative after reopen; their canonical definition is independently
parsed and ownership-checked during catalog loading.

## Custom functions

The typed `fn::` definition above was extended with independent probes for
lifecycle and execution contexts. `DEFINE FUNCTION IF NOT EXISTS` retained the
existing body, while `DEFINE FUNCTION OVERWRITE` replaced it. `ALTER FUNCTION
fn::f PERMISSIONS NONE` preserved the body and changed the canonical entry in
the `functions` object returned by `INFO FOR DB`. `REMOVE FUNCTION` removed the
call target; a later invocation returned a not-found error. Untyped arguments,
default argument syntax, wrong arity, and calls to an absent function were
rejected by the fixed binary.

Function bodies could read a database parameter and the ambient LET binding,
and arguments shadowed those values in the call frame. Calls worked inside
larger expressions and SELECT row projections. This probe also established
that a function invoked for a SELECT row may execute a data statement:

```surql
DEFINE FUNCTION fn::write($x: int) {
    CREATE sink CONTENT { n: $x };
    RETURN $x;
};
CREATE source:a SET n = 1;
SELECT fn::write(n) AS v FROM source;
SELECT * FROM sink;
```

The projection returned `{ v: 1 }` and the final query found one sink record.
FastDB consequently routes function data statements through the same frontend
and active transaction in script, projection, predicate, and CREATE-content
contexts. A standalone statement containing a custom call receives one
implicit statement transaction, so a later body error rolls back earlier body
writes. An existing explicit transaction remains authoritative and is never
implicitly replayed.

FastDB stores typed arguments, the independently parsed body, AST and limit
versions, permissions, and the public definition in the sealed format-3
function catalog. Reopen validates canonical encodings and ownership before
execution. Calls share a 10,000-call ceiling, a depth limit of 32, the caller's
script/deadline budget, typed argument normalization, and redacted THROW
behavior. Replacement and removal validate stored function call sites so they
cannot publish a dangling or wrong-arity dependency.

## Table and field schema

Public `DEFINE TABLE`, `DEFINE FIELD`, `ALTER TABLE`, `ALTER FIELD`, `REMOVE`,
and `INFO` documentation supplied clause candidates. Independent probes then
fixed the behavior used by the implementation. In `v3.1.5`, an omitted table
type canonicalized as `TYPE ANY`, while explicit `TYPE NORMAL` and `TYPE
RELATION` remained distinct. FastDB currently reports mixed `TYPE ANY` as an
explicit provider limitation because its Phase 7 storage contract gives normal
and relation records different immutable physical representations; it does not
silently map ANY to NORMAL.

The fixed binary accepted `DROP`, schema mode, permissions, and comments on a
table. DROP retained existing records and allowed deletion but rejected create
and update. `INFO FOR DB` returned canonical table definitions, while `INFO FOR
TABLE item` returned the five structured `events`, `fields`, `indexes`, `lives`,
and `tables` objects. `ALTER TABLE IF EXISTS` was an idempotent no-op for a
missing table. FastDB stores this metadata in the table definition owned by the
opaque table catalog row, parses and ownership-checks it on reopen, and keeps
permission enforcement reserved for Phase 18's non-Owner sessions.

The following independently authored field family established mutation order
and lifecycle behavior:

```surql
DEFINE TABLE item SCHEMAFULL TYPE NORMAL;
DEFINE FIELD count ON item TYPE int DEFAULT ALWAYS 1 ASSERT $value >= 0;
DEFINE FIELD stamp ON item TYPE number VALUE count + 1;
DEFINE FIELD code ON item TYPE string READONLY;
CREATE item:a SET code = 'fixed';
UPDATE item:a SET count = 4;
UPDATE item:a SET code = 'changed';
UPDATE item:a SET count = NONE;
ALTER FIELD code ON item DROP READONLY;
REMOVE FIELD stamp ON item;
INFO FOR TABLE item;
```

CREATE produced `count: 1` and `stamp: 2`; updating count to four recomputed
stamp as five; the readonly update failed without mutation; setting count to
NONE reapplied DEFAULT ALWAYS and recomputed stamp as two. Dropping READONLY
allowed the later update. REMOVE FIELD removed only schema metadata: the
already stored stamp value remained visible. A separate probe showed that
defining a required defaulted field after an older record did not backfill that
record in the reference; the next update failed type coercion while the
document remained unchanged. An untyped field permitted the field to be
missing in a schemafull table; FastDB's collision-safe `any` field rule follows
that optional, unrestricted value behavior.

FastDB applies defaults, computed VALUE expressions, type normalization,
readonly checks, assertions, and all derived graph/FTS/vector maintenance in
one statement transaction. Stored expression source and AST are reparsed and
ownership-checked on reopen. Simple REFERENCE metadata is accepted only for
record-valued fields; ON DELETE actions remain explicit until the dependency
provider is implemented. Field removal refuses live index dependencies and
removes catalog-owned native-vector columns and the last vector capability
atomically. Union/literal types, FLEXIBLE, permission predicates, full
reference actions, mixed table ANY, and views remain Partial rather than being
accepted without behavior.

## Synchronous events

The public [DEFINE EVENT](https://surrealdb.com/docs/reference/query-language/statements/define/event)
and [ALTER EVENT](https://surrealdb.com/docs/reference/query-language/statements/alter/event)
pages supplied syntax candidates. The fixed `v3.1.5` binary then established
the synchronous subset independently. A definition with no WHEN clause was
reported by `INFO FOR TABLE` with `WHEN true`; block, parenthesized, and bare
single-statement actions all executed. A definition without THEN was rejected.
Phase 15 rejects ASYNC, RETRY, and MAXDEPTH explicitly instead of silently
changing their separate-transaction semantics.

Two events were deliberately defined in reverse name order. Both appended
their name to the same record when a source record was created:

```surql
DEFINE EVENT z ON item WHEN $event = 'CREATE'
  THEN { UPSERT event_order:state SET marks += ['z'] };
DEFINE EVENT a ON item WHEN $event = 'CREATE'
  THEN { UPSERT event_order:state SET marks += ['a'] };
CREATE item:a;
SELECT * FROM event_order:state;
```

The resulting array was `['a', 'z']`, establishing deterministic lexical event
name order rather than definition order. An `ALTER EVENT ... DROP WHEN DROP
THEN` probe retained `WHEN true` and the prior action in `INFO FOR TABLE`; the
fixed reference does not publish an action-less synchronous event.

A separate CREATE/UPDATE/DELETE probe captured `$event`, `$before`, `$after`,
`$value`, and `$input`. CREATE exposed no before image, the complete after/value
record, and the input document without the synthesized ID. UPDATE exposed both
complete images, used after for value, and exposed the update input. DELETE
exposed the complete before/value record and no after/input. Event actions ran
inside the triggering transaction, and an action error rolled back the source
mutation and earlier event writes together.

FastDB persists a canonical definition plus independently parsed WHEN/action
source in the sealed format-3 event catalog. Reopen checks table ownership,
AST version, recursion policy, sources, and canonical definition before any
query executes. CREATE, INSERT, RELATE, UPDATE, UPSERT, DELETE, and graph node
cascade deletion all invoke the same frontend event path. Event bodies share
the request deadline and script-step limits, have a 10,000-invocation ceiling
and depth limit of 16, reject transaction-control statements, and detect a
same-event/same-record recursion cycle. Event actions never bypass graph,
schema, FTS, vector, or transaction maintenance.

A three-record CREATE probe had each event query the table count. The event
observed one, then two, then three records, establishing that the reference
finishes a record's synchronous event actions before applying the next source
record. A multi-record UPDATE probe selected all candidates first: an event on
the first record changed the third record to `100`, but the outer statement
later applied its already selected input image and left the third record at
`1`. A multi-record DELETE probe made the first record's event conflict with a
later record; the conflict rolled the entire statement back. FastDB follows
these boundaries: source candidates are fixed before UPDATE/DELETE mutation,
while each record and all of its event actions complete before the next record.
Standalone data-mutation statements that may fire an event use the same
internal transaction path as explicit transactions, so any source or event
failure rolls back the complete statement.

## Sequence and module stop probes

The fixed reference canonicalized `DEFINE SEQUENCE basic` as `DEFINE SEQUENCE
basic BATCH 1000 START 0`. Two calls to `sequence::nextval('basic')` returned
zero and one; `sequence::next('basic')` was rejected as an invalid function
path. `IF NOT EXISTS` retained a sequence, `OVERWRITE` replaced its definition,
`ALTER SEQUENCE ... TIMEOUT` changed only the timeout, and missing-object
`ALTER/REMOVE ... IF EXISTS` were no-ops.

A `BATCH 1 START 10` transaction probe established the important durability
boundary: an allocation followed by `CANCEL` was still burned, so the next
call returned 11. Another allocation in a transaction stopped by `THROW` was
also burned, and the next call returned 13. An independent pinned-Turso
stable-WAL probe returned 10 both inside the rolled-back transaction and on
the following call. The resulting architecture stop is recorded in
`docs/phase15-architecture-stops.md`; FastDB does not substitute rollbackable
or process-local semantics.

The fixed reference also rejected `DEFINE MODULE mod::demo AS
f\"files:/demo.surli\"` because the experimental `surrealism` capability was
not enabled. Public documentation identifies the source as a bucket-hosted
WASM artifact. FastDB has no sealed bounded WASM provider and rejects the
definition before filesystem or catalog access, as detailed in the same stop
report.
