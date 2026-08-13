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

## Initial parameter and function shapes

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
FULL` for both definitions. These observations establish candidates only;
FastDB does not mark the locked parameter/function rows Supported until their
catalog lifecycle, reopen, rollback, limits, and executable conformance tests
pass.
