# Phase 3 clean-room execution characterization

Status: completed 2026-08-12 against the fixed SurrealDB `v3.1.5` behavioral reference.

## Method and provenance

This note records independently designed black-box queries run against an unmodified official SurrealDB binary. No SurrealDB source, tests, fixtures, expected-output files, or fuzz corpus was read, copied, translated, or committed.

Public sources accessed 2026-08-12:

- [CREATE](https://surrealdb.com/docs/reference/query-language/statements/create)
- [SELECT](https://surrealdb.com/docs/reference/query-language/statements/select)
- [UPDATE](https://surrealdb.com/docs/reference/query-language/statements/update)
- [DELETE](https://surrealdb.com/docs/reference/query-language/statements/delete)
- [BEGIN](https://surrealdb.com/docs/reference/query-language/transactions/begin)
- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)

Reference environment:

- Asset: `surreal-v3.1.5.linux-amd64.tgz`
- Archive SHA-256: `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`
- `surreal version`: `3.1.5 for linux on x86_64`
- Backend: `mem://`; namespace `fastdb`; isolated databases per probe group
- CLI: `surreal sql --hide-welcome --json --log none`
- Probe date: 2026-08-12

Outputs below are exact strict JSON apart from the CLI's blank lines and diagnostic prompts. FastDB keeps the independent Phase 1 grammar and implements only the exact subset in `COMPAT.md`.

## Expressions and truthiness

The reference uses symbolic `!`; FastDB's frozen MVP grammar spells the equivalent unary operator `NOT`. This probe characterized evaluator behavior rather than broadening syntax:

```surql
RETURN [
  !none, !null, !false, !0, !'', ![], !{},
  0 OR 7, 2 AND 3, 7 / 2, 7 / 0,
  9007199254740993 = 9007199254740992.0
];
```

```json
[[true,true,true,true,true,true,true,7,3,3,null,false]]
```

The probe establishes falsey missing, null, false, numeric zero, and empty strings, arrays, and objects; operand-returning `AND` and `OR`; truncating integer division; null on division by zero; and exact mixed integer/float equality beyond JavaScript's exact-integer range. Separate independently authored probes compared arrays, objects, strings, booleans, record IDs, and missing projections to fix the recursive equality and type-order rules recorded in the Phase 3 plan.

FastDB adds checked integer arithmetic and rejects overflow or non-finite results with a schema error. Those are safety requirements of the FastDB API, not claims about accepting every SurrealQL numeric form.

## Result and projection shapes

The following statements were submitted to one in-memory reference session:

```surql
CREATE ONLY person:a SET profile.age = 42, name = 'A';
SELECT profile.age, name AS id FROM ONLY person:a;
CREATE person:b SET name = 'B' RETURN BEFORE;
```

```json
[{"id":"person:a","name":"A","profile":{"age":42}}]
[{"id":"A","profile":{"age":42}}]
[[null]]
```

This confirms the scalar `ONLY` contract, nested unaliased projection shape, top-level aliases overwriting the virtual `id`, and the null pre-create value. FastDB exposes these through typed Rust results: `Value` for an `ONLY` statement, `Rows` for ordinary CRUD, and one null row for `CREATE ... RETURN BEFORE`.

Missing projection and record probes produced:

```surql
SELECT absent, n AS id FROM ONLY thing:a;
SELECT * FROM ONLY thing:missing;
CREATE thing:b SET n = 3 RETURN NONE;
UPDATE thing:missing SET n = 1 RETURN BEFORE;
DELETE thing:missing RETURN BEFORE;
```

```json
[{"absent":null,"id":2}]
[null]
[[]]
[[]]
[[]]
```

FastDB therefore materializes a missing projection as null, a missing `ONLY` target as `Value::Null`, and missing ordinary mutations as empty rows. Schema and transaction statements have the separate `StatementResult::None` shape.

## SET evaluation and conflicting assignments

After creating `thing:a` with `n = 1`, the reference evaluated:

```surql
UPDATE thing:a SET n = n + 1, doubled = n * 2 RETURN AFTER;
UPDATE thing:a SET x = 1, x = 2 RETURN AFTER;
```

```json
[[{"doubled":2,"id":"thing:a","n":2}]]
[[{"doubled":2,"id":"thing:a","n":2,"x":2}]]
```

All right-hand sides observe the pre-update document, while writes are applied in source order and a later conflicting assignment wins. FastDB applies the same rule to UPDATE, and evaluates CREATE SET expressions against an empty pre-create document. Assigning an internal missing value removes the leaf without pruning empty ancestors.

## Return modes, ordering, and missing targets

The reference returned the pre-delete record for `DELETE ... RETURN BEFORE`, an empty result for the default missing DELETE, and no error for missing UPDATE/DELETE record targets. Pagination was observed after ordering and filtering. The reference additionally requires an ORDER expression to be available to its projection in some SELECT shapes; FastDB deliberately follows its fixed Phase 1 projection grammar and applies `WHERE`, `ORDER BY`, `START`, `LIMIT`, then projection.

Top-level `id` is virtual in FastDB: it is available to expressions, filtering, ordering, and projection, but cannot be stored, assigned, declared, or indexed.

## Explicit transactions

Submitting one script, rather than separate CLI requests, produced:

```surql
BEGIN; CREATE tx:a SET n = 1; CANCEL; SELECT * FROM tx;
BEGIN; CREATE tx:b SET n = 2; COMMIT; SELECT * FROM tx;
```

```json
[null,"The query was not executed due to a cancelled transaction",null,"The table 'tx' does not exist",null,[{"id":"tx:b","n":2}],null,[{"id":"tx:b","n":2}]]
```

This establishes the script-level transaction boundary and rollback/commit visibility. FastDB strengthens failure cleanup: any error while active immediately attempts a full rollback, discards private catalog state, and poisons the connection transaction state. Only `CANCEL` clears poison; rollback cleanup failure makes the connection broken. This is an explicit FastDB reliability contract.

## Parameter-map contract

The SurrealDB CLI cannot reproduce an embedded Rust parameter map with FastDB's validation boundary. Phase 3 therefore treats parameter validation as an explicit FastDB API contract: names are case-sensitive and omit `$`; values are recursively bounded, finite, and record-ID-valid; extra entries are ignored; every referenced entry must exist before its statement runs; and parameters can occupy value positions only. Independently authored `P3-PARAM-*` tests cover repeated names, Unicode, recursive validation, invalid records, and source/path/clause injection attempts.

## Phase 3 interpretation

The observations fix the semantics of the already parsed MVP expression, CRUD, result, script, and transaction forms. They do not add functions, casts, subqueries, traversal, richer mutation operators, arbitrary clauses, or other SurrealQL statement families. Those forms remain explicitly unsupported.
