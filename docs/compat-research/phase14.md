# Phase 14 clean-room compatibility observations

Behavioral reference: official, unmodified SurrealDB `v3.1.5`

Observation date: 2026-08-14

This note records independently designed black-box probes and public
documentation used for Phase 14 CRUD/query behavior. No SurrealDB source,
tests, fixtures, expected-output files, or implementation details were
inspected or copied.

## Reference binary

- Download URL:
  `https://github.com/surrealdb/surrealdb/releases/download/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`.
- Installed outside the repository under
  `/home/tan/.cache/fastdb-reference/surreal-v3.1.5/`.
- Archive SHA-256:
  `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
- `surreal version`: `3.1.5 for linux on x86_64`.
- Backend: ephemeral `memory`; namespace `fastdb`; isolated Phase 14
  databases per probe group.

Public syntax references were the SurrealDB pages for
[data types](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types)
and [statement overviews](https://surrealdb.com/docs/reference/query-language/statements/overview).
Those moving pages supplied candidates only; the fixed binary determined the
observations below.

## Record targets and identifiers

The following independently authored forms selected integer record IDs using
canonical typed order:

```surql
SELECT * FROM person:1..3;
SELECT * FROM person:1..=3;
SELECT * FROM person:..3;
SELECT * FROM person:2..;
```

Exclusive and inclusive end bounds behaved as spelled, and either bound could
be omitted. Array and object record components were addressable after reopen;
object key order did not change identity. An array/object component remained
distinct from a string containing the same source-looking text, and
`record::id` returned the typed component.

## Mutation forms

CREATE accepted omitted data, CONTENT, SET, ONLY, generated IDs, explicit IDs,
integer batch/range targets, array targets, all observed return modes, and a
trailing TIMEOUT after RETURN. UPDATE/UPSERT accepted CONTENT, MERGE, PATCH,
REPLACE, SET assignment operators, UNSET, WHERE, ONLY, return modes, and the
same trailing TIMEOUT position. DELETE accepted optional FROM, arrays, ONLY,
WHERE, return modes, and TIMEOUT.

One combined target/timeout probe was:

```surql
CREATE person:a SET n = 1 RETURN AFTER TIMEOUT 1s;
UPDATE [person:a, person:b] SET x = true RETURN AFTER TIMEOUT 1s;
DELETE [person:a, person:b] RETURN BEFORE TIMEOUT 1s;
```

The returned arrays retained target order. Duplicate or overlapping target
membership mutated a logical record once. Errors in a multi-record mutation
prevented the statement's earlier records from remaining published.

INSERT accepted one object, an array of objects, and field-list VALUES rows.
`IGNORE` suppressed a duplicate input, while `ON DUPLICATE KEY UPDATE` exposed
the proposed row through `$input`. `INSERT RELATION` accepted typed `in`/`out`
record values. UPSERT created a missing explicit record and updated an existing
one through the same data/return forms.

`RETURN BEFORE` exposed the immutable before image, `RETURN AFTER` the after
image, `RETURN VALUE <expr>` one evaluated value per mutation, and `RETURN
DIFF` an ordered patch-like change collection. `RETURN NONE` suppressed result
materialization without changing the mutation count.

## Query pipeline

Independent probes covered SELECT VALUE; expression aliases; star plus
expression projection; nested destructuring; OMIT; SPLIT ON; GROUP ALL and
GROUP BY one or more keys; count/sum-style aggregates; multiple table, record,
array, object, and subquery targets; ORDER BY multiple terms with COLLATE or
NUMERIC; ORDER BY RAND(); expression/parameter LIMIT BY and START AT; FETCH;
and `SELECT * FROM ONLY <table> LIMIT 1`.

Observed pipeline details used by FastDB are:

- WHERE filters authoritative records before SPLIT and grouping.
- SPLIT emits one row per collection member and preserves a row for an empty
  collection in the characterized form.
- GROUP ALL on an empty input still emits one aggregate row, with `count()`
  equal to zero.
- Numeric ordering places an `item2` string before `item10`; ordinary string
  ordering does not infer numeric segments.
- START is applied before LIMIT to the ordered output.
- FETCH replaces a one-level record value with its current document and leaves
  a dangling reference as characterized by the fixed binary.
- Multiple targets preserve target order until an explicit ORDER clause; an
  array target contributes its members in array order.

EXPLAIN, EXPLAIN FULL, EXPLAIN ANALYZE, and the combined ANALYZE/FULL JSON form
returned structured plan/operation data. FastDB exposes a stable logical
subset rather than leaking reference-internal or Turso physical identifiers.

## Explicit version boundary

The reference accepted:

```surql
CREATE person:b SET n = 1 VERSION d'2024-01-01T00:00:00Z';
```

That clause is part of versioned record history. The approved FastDB roadmap
explicitly excludes versioned history, changefeeds, time-series retention, and
historical reads. FastDB therefore rejects VERSION rather than accepting it as
an ignored annotation. The locked combined capability row and reopening rule
are recorded in `docs/phase14-architecture-stops.md`.
