# Phase 2 clean-room storage and schema characterization

Status: completed 2026-08-12 against the fixed SurrealDB `v3.1.5` behavioral reference.

## Method and provenance

This note uses public SurrealQL documentation and independently designed black-box queries against an unmodified official binary. No SurrealDB source, tests, fixtures, expected-output files, or fuzz corpus was read, copied, translated, or committed.

Public sources accessed 2026-08-12:

- [UUIDs](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/uuids)
- [Record IDs](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types/record-ids)
- [DEFINE TABLE](https://surrealdb.com/docs/reference/query-language/statements/define/table)
- [DEFINE FIELD](https://surrealdb.com/docs/reference/query-language/statements/define/field)
- [DEFINE INDEX](https://surrealdb.com/docs/reference/query-language/statements/define/indexes)
- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)

Reference environment:

- Asset: `surreal-v3.1.5.linux-amd64.tgz`
- Archive SHA-256: `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`
- `surreal version`: `3.1.5 for linux on x86_64`
- Backend: `mem://`; namespace `fastdb`; isolated databases per probe group
- CLI: `surreal sql --hide-welcome --json --log none`
- Probe date: 2026-08-12

Output below is exact strict JSON apart from terminal prompts and line wrapping. FastDB follows the fixed Phase 2 contract where it deliberately narrows or strengthens the observed behavior.

## UUID record IDs, objects, and reserved `id`

The public UUID page describes UUIDv4/v7 values and the adjacent `u"…"` prefix form. The v3.1.5 binary accepted both adjacent quote styles as RID components:

```surql
RETURN [
  person:u"a8f30d8b-db67-47ec-8b38-ef703e05ad1b",
  person:u'01954f80-0000-7000-8000-000000000001'
];
```

```json
[["person:u'a8f30d8b-db67-47ec-8b38-ef703e05ad1b'","person:u'01954f80-0000-7000-8000-000000000001'"]]
```

A backtick-string RID and UUID RID with the same UUID text coexisted as distinct records. FastDB likewise keeps string and UUID components distinct in its RID codec.

The reference also accepted `u'01954f80-0000-1000-8000-000000000001'`, despite the current public page describing v4/v7. FastDB follows the documented and phase-plan contract instead: only canonical lowercase hyphenated v4/v7 UUIDs enter the AST, and omitted CREATE IDs generate v7.

Duplicate object keys normalized last-value-wins:

```surql
RETURN {a: 1, a: 2};
```

```json
[{"a":2}]
```

Attempting `CREATE person:reserved CONTENT {id: person:other, name: "kept"}` failed with `Found person:other for the id field, but a specific record has been specified`. FastDB reserves top-level `id` more strictly and rejects it for every CREATE; decoded results synthesize `id` from the immutable RID instead of storing it in `doc`.

## Required, optional, null, and numeric coercion

The following schema was probed:

```surql
DEFINE TABLE typed SCHEMAFULL;
DEFINE FIELD required ON TABLE typed TYPE string;
DEFINE FIELD optional ON TABLE typed TYPE option<string>;
DEFINE FIELD i ON TABLE typed TYPE int;
DEFINE FIELD f ON TABLE typed TYPE float;
DEFINE FIELD n ON TABLE typed TYPE number;
```

Observed behavior:

| Input distinction | v3.1.5 result |
| --- | --- |
| omit `optional`, provide every plain field | success |
| omit `required` | expected `string`, found `NONE` |
| store `NULL` in `option<string>` | expected `none \| string`, found `NULL` |
| store integer `2` in `float` | success; result materialized `2.0` |
| store float `1.5` in `int` | expected `int`, found `1.5f` |
| store integer or float in `number` | success |

These observations match the FastDB Phase 2 rules: plain types are required, option permits absence rather than null, float normalizes integers, and number accepts both numeric representations.

## Nested paths and duplicate definitions

The reference accepted a declared descendant and synthesized its ancestor container:

```surql
DEFINE TABLE nested SCHEMAFULL;
DEFINE FIELD profile.age ON TABLE nested TYPE int;
CREATE nested:ok CONTENT {profile: {age: 7}};
```

It also accepted `{profile: {age: 8, extra: true}}` in this exact v3.1.5 setup, and later accepted a new required `profile.name` definition even though existing records lacked it. Current public documentation describes stricter schemafull object behavior. FastDB intentionally implements the Phase 2 production invariant instead: declared descendants authorize ancestor containers but not undeclared siblings, and a new field definition validates every existing row before commit.

Repeating `DEFINE TABLE nested SCHEMAFULL` returned `The table 'nested' already exists`; repeating `DEFINE FIELD profile.age ...` returned `The field 'profile.age' already exists`. FastDB maps the same duplicate-definition class to logical constraint errors and does not expose physical names.

## Unique null, missing, and composite indexes

For a unique index on `email`, v3.1.5 admitted two missing values and two explicit null values. It admitted the first non-null string and rejected the second equal string with an index-duplicate error. Defining a unique index after inserting two equal existing strings also failed.

For `DEFINE INDEX by_pair ON combo FIELDS a, b UNIQUE`, the reference rejected a duplicate complete tuple `[1, 'x']` but allowed repeated partial tuples where `b` was missing. Repeating the index definition returned `The index 'by_email' already exists`.

FastDB follows these observed null/missing and ordered-composite uniqueness rules. Its narrower Phase 2 index contract accepts only missing, null, bool, integer, finite float, and string path values; objects, arrays, and record-valued indexed paths fail with a schema error before physical index creation or record commit.

## Phase 2 interpretation

The probes establish UUID source spelling, value normalization, required/optional/null behavior, nested-path differences, duplicate definition behavior, and unique/composite semantics. They do not broaden Phase 2. In particular, FastDB deliberately enforces documented UUID versions, stronger schemafull validation, existing-row validation, scalar-only expression indexes, and the exact execution slice in `COMPAT.md`.
