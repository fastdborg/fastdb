# Phase 13 clean-room compatibility observations

Behavioral reference: official, unmodified SurrealDB `v3.1.5`

Observation date: 2026-08-14

This note records independently designed black-box probes and public
documentation used for Phase 13 expression behavior. No SurrealDB source,
tests, fixtures, expected-output files, or implementation details were
inspected or copied.

## Reference binary

- Download URL:
  `https://github.com/surrealdb/surrealdb/releases/download/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`
- Downloaded outside the repository to
  `/home/tan/.cache/fastdb-reference/surreal-v3.1.5/`.
- Archive SHA-256:
  `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
- `surreal version`: `3.1.5 for linux on x86_64`.
- Backend: ephemeral `memory`; namespace `fastdb`; database `phase13`.

Public reference: [SurrealQL operators](https://surrealdb.com/docs/reference/query-language/operators).
The online page supplied syntax candidates only; the fixed binary observations
below determine FastDB's compatibility behavior.

## Binding and associativity

Independently authored input:

```surql
RETURN 0 ?? 1 + 2;
RETURN (0 ?? 1) + 2;
RETURN 0 ?? (1 + 2);
RETURN 2 ** 3 ** 2;
RETURN -2 ** 2;
RETURN 2 * 3 ** 2;
RETURN 0 ?? 1 = 0;
RETURN 0 ?? true AND false;
RETURN 0 ?? false OR true;
RETURN 0 ?: 1 + 2;
RETURN 1 ?: 1 + 2;
```

Observed results, in order:

```text
0
2
0
64
4
18
0
0
0
3
1
```

Both coalescing operators bind below arithmetic, comparisons, `AND`, and
`OR`. Power is left associative, while unary operators bind above power.
Logical and coalescing operators return an operand and short-circuit.

## Access, slices, equality, and containment

Array indexing is zero-based. A negative or out-of-bounds direct index returns
NONE. Array slices accept `start..end`, `start..=end`, `..end`, and `start..`;
negative, reversed, or out-of-bounds limits return NONE. Direct string index
and slice expressions returned NONE in these probes and are not inferred to be
Unicode access support.

The reference returned true for the characterized `CONTAINS`, `CONTAINSALL`,
`CONTAINSANY`, `CONTAINSNONE`, `INSIDE`, `ALLINSIDE`, `ANYINSIDE`, and
`NONEINSIDE` array forms. Object containment with a string tests for a key.
The empty collection satisfies the vacuous all-equal form.

Integer, float, and decimal values that are numerically equal compare equal
under both `=` and `==` in the observed forms. NONE equals NONE, NULL equals
NULL, and NONE does not equal NULL.

## Arithmetic boundaries

Integer remainder by zero produced an error; integer division by zero produced
NULL. String and array addition concatenate. Duration add/subtract and scaling
by an integer were accepted, while duration modulo was rejected. Negative
power exponents and integer power overflow produced errors. FastDB uses
checked arithmetic and bounded allocations for the corresponding supported
forms.
