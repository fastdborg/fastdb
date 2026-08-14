# Phase 12 clean-room compatibility observations

Behavioral reference: official, unmodified SurrealDB `v3.1.5`

Observation date: 2026-08-13

This note records independently designed black-box probes and public
documentation used to lock the Phase 12 capability inventory and value model.
No SurrealDB source, tests, fixtures, fuzz corpus, or implementation details
were inspected or copied.

## Reference binary

- Download URL:
  `https://github.com/surrealdb/surrealdb/releases/download/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`
- Downloaded outside the repository to
  `/home/tan/.cache/fastdb-reference/surreal-v3.1.5/`.
- Archive SHA-256:
  `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
- `surreal version`: `3.1.5 for linux on x86_64`.
- Test server: loopback-only, ephemeral `memory` datastore, fresh namespace
  `fastdb` and database `phase12`. The test-only password was not committed.

## Public references

- [Data types](https://surrealdb.com/docs/reference/query-language/language-primitives/data-types)
- [Operators](https://surrealdb.com/docs/reference/query-language/language-primitives/operators)
- [Statements](https://surrealdb.com/docs/reference/query-language/statements/overview)
- [Database function families](https://surrealdb.com/docs/reference/query-language/functions/database-functions)
- [RPC protocol](https://surrealdb.com/docs/reference/rest-api/rpc-protocol)

These current pages identify capability names and syntax candidates. The
binary observations below, not moving documentation, pin Phase 12 value
behavior to `v3.1.5`.

## Value normalization probe

Input statements were independently authored:

```surql
RETURN NONE;
RETURN NULL;
RETURN <bytes>'Aé';
RETURN d'2026-08-13T12:34:56.123456789+07:00';
RETURN 1y2w3d4h5m6s7ms8us9ns;
RETURN 1.2300dec;
RETURN <set>[3,1,3,2];
RETURN 1..=3;
RETURN /a+b?/;
RETURN u'018f47f2-5f10-7cc7-98b2-001122334455';
RETURN type::table('person');
```

Observed normalized results:

```text
NONE
NULL
b"41C3A9"
d'2026-08-13T05:34:56.123456789Z'
1y2w3d4h5m6s7ms8µs9ns
1.23dec
{1, 2, 3}
1..=3
/a+b?/
u'018f47f2-5f10-7cc7-98b2-001122334455'
person
```

The observation establishes distinct NONE/NULL values, byte preservation,
UTC datetime normalization with nanoseconds, nonnegative duration
normalization, canonical decimal trailing-zero removal, set deduplication and
ordering, typed ranges, regexes, UUIDs, and table values.

## Missing values, sets, and ranges

Input:

```surql
RETURN { a: NONE, b: NULL, c: [NONE, NULL] };
RETURN <set>[1, 1.0f, 1dec, NULL, NONE, '1'];
RETURN ..3;
RETURN 1..;
RETURN 1>..=3;
```

Observed:

```text
{ a: NONE, b: NULL, c: [NONE, NULL] }
{NONE, NULL, 1, '1'}
..3
1..
1>..=3
```

NONE remains representable inside stored composite values and differs from
NULL. Numerically equal int/float/decimal set members deduplicate. A range may
be unbounded on either side and independently marks exclusion or inclusion of
present bounds.

Additional mixed-number probes showed `0.1f = 0.1dec`, while decimal
`9007199254740993dec` remains distinct from float
`9007199254740992f`. A mixed set retained the two large adjacent values but
deduplicated the exactly represented integer/float member. FastDB therefore
compares a float's canonical decimal rendering against decimals and retains an
exact integer/float boundary comparison instead of collapsing every numeric
value through `f64`.

## Boundary probes

Observed duration and datetime boundaries:

```text
duration::max
=> 584942417355y3w5d7h15s999ms999µs999ns

0ns
=> 0ns

d'0001-01-01T00:00:00Z'
=> d'0001-01-01T00:00:00Z'

d'9999-12-31T23:59:59.999999999Z'
=> d'9999-12-31T23:59:59.999999999Z'
```

Decimal probes showed a 96-bit unsigned coefficient ceiling with sign and
scale:

```text
79228162514264337593543950335dec
=> 79228162514264337593543950335dec

-79228162514264337593543950335dec
=> -79228162514264337593543950335dec

79228162514264337593543950336dec
=> parse error: overflow from too many digits

0.0000000000000000000000000001dec
=> 0.0000000000000000000000000001dec

0.00000000000000000000000000001dec
=> 0dec
```

FastDB therefore uses a checked coefficient no greater than
`2^96 - 1`, scale `0..=28`, and canonical zero/sign/trailing-zero handling.
It does not use an unbounded decimal string.

Additional scale-29 probes established midpoint-away-from-zero rounding at
the 28-digit scale boundary:

```text
1.23456789012345678901234567894dec
=> 1.2345678901234567890123456789dec

1.23456789012345678901234567895dec
=> 1.234567890123456789012345679dec

9.99999999999999999999999999995dec
=> 10dec

-1.23456789012345678901234567895dec
=> -1.234567890123456789012345679dec
```

## File-value disposition

The probe `RETURN f'bucket:/folder/file';` was rejected by the official binary
unless its experimental files feature is enabled. The general value inventory
retains a distinct file capability because `v3.1.5` exposes the syntax and
file function family. Phase 12 supports the value only as a validated opaque
reference through bound values, schema enforcement, documents, JSON, reopen,
and backup/restore. It does not dereference the value. File and outbound
resource functions remain separately capability-gated Phase 13 inventory
items and are deny-by-default.

## Typed collection schema probe

Input:

```surql
DEFINE TABLE item SCHEMAFULL;
DEFINE FIELD xs ON item TYPE array<int, 2>;
CREATE item:one SET xs = [1];
CREATE item:two SET xs = [1, 2];
CREATE item:three SET xs = [1, 2, 3];

DEFINE TABLE thing SCHEMAFULL;
DEFINE FIELD ys ON thing TYPE set<int, 2>;
DEFINE FIELD span ON thing TYPE range;
CREATE thing:one SET ys = <set>[1], span = 1..=3;
CREATE thing:two SET ys = <set>[1, 2], span = 1..3;
CREATE thing:three SET ys = <set>[1, 2, 3], span = 1..=3;
```

Observed behavior: the array and set declarations accepted only collections
of exactly two elements. Lengths one and three failed coercion. The range type
is written as unparameterized `range`; `range<int>` was rejected by the parser.
The successful records retained `[1, 2]`, the typed set with members 1 and 2,
and the exclusive-upper-bound range `1..3`.
