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

## Collection and object function batch

The first function batch was probed one call per statement so an unknown name
could not hide later results. Representative inputs included:

```surql
RETURN array::add([1,2],2);
RETURN array::at([1,2],-1);
RETURN array::difference([2,1],[2,3]);
RETURN array::repeat([1,2],2);
RETURN array::range(1,4);
RETURN array::slice([1,2,3],1,2);
RETURN object::entries({b:2,a:1});
RETURN object::extend({a:1},{b:2,a:3});
RETURN set::difference(<set>[2,1],<set>[2,3]);
RETURN set::slice(<set>[1,2,3],1,2);
```

Observed results established duplicate suppression for `array::add`,
from-the-end negative `array::at`, symmetric `difference`, whole-value
repetition (`[[1,2],[1,2]]`), an exclusive range end, an exclusive slice end,
lexicographically ordered object entries, right-biased object extension,
canonical set ordering, and set slice output. `array::union` preserves the
left collection's order before new right-side members; set functions require a
typed set rather than accepting an array.

The fixed binary returned “unknown function” for `array::includes` and
`array::index_of`, although those atomic names occur in the locked inventory
derived from the public function catalog. FastDB implements both as bounded
closed-registry collection helpers but does not use their availability as
evidence about other `v3.1.5` functions. This reference/catalog mismatch must
be resolved before the Phase 13 checkpoint decides their final compatibility
classification.

## Math functions and constants

Constants were confirmed to be namespaced values without parentheses:

```surql
RETURN math::pi;
RETURN math::e;
RETURN math::sqrt_2;
```

The fixed binary returned finite floats. Calling `math::pi()` was not accepted
as the constant form, so FastDB represents namespaced constants separately
from function calls in its AST and registry.

Representative scalar and aggregate probes produced:

```text
math::abs(-2)          => 2
math::ceil(1.2)        => 2f
math::sign(-2)         => -1
math::sqrt(4)          => 2f
math::max([1,2.5])     => 2.5f
math::sum([1,2,3])     => 6
math::product([2,3])   => 6
math::mean([1,2])      => 1.5f
math::spread([1,4])    => 3
math::clamp(5,1,3)     => 3
math::lerp(0,10,0.25)  => 2.5f
math::log(8,2)         => 3f
math::pow(2,3)         => 8
```

FastDB preserves exact integer results where the reference does, returns
finite floats for transcendental operations, and rejects domain errors or
non-finite output rather than persisting an invalid numeric value.

## Type and record helpers

Type conversion probes established that conversion is checked rather than a
general truthiness or truncation operation. `type::bool('true')` succeeds but
`type::bool(0)` fails; `type::int(1.0)` succeeds but `type::int(1.9)` fails.
Typed sets convert to canonically ordered arrays, UTF-8 strings convert to
bytes, and two-element arrays convert to exclusive-end ranges.

The reference returned lower-case type names from `type::of`, the raw record
component from `record::id`/`meta::id`, and the table string from
`record::table`. `type::string(person:one)` produced `person:one` without
unnecessary quoting. FastDB follows these result types while retaining typed
record IDs and table values for the actual conversion functions.

The fixed `v3.1.5` binary rejected `type::thing` and `meta::table` and suggested
`type::record` and `meta::tb`, respectively. FastDB implements the locked
catalog names defensively but leaves those two inventory rows Unsupported
until the reference/catalog mismatch is resolved.

## Duration and time families

Composite duration literals concatenate nonnegative unit components. The
reference reported total whole units from extractors (for example,
`duration::hours(1d2h) => 26`) and constructors returned exact typed
durations (`duration::from_millis(1500) => 1s500ms`). `duration::max` is a
constant without parentheses and corresponds to the full unsigned
seconds-plus-nanoseconds domain used by FastDB's typed duration.

UTC datetime extraction from
`2024-02-29T12:34:56.123456789Z` returned year 2024, month 2, day 29, hour 12,
minute 34, second 56, Sunday-based weekday 4, ISO week 9, one-based year day
60, and integer Unix seconds/milliseconds/microseconds/nanoseconds. Timestamp
constructors use Unix epoch units. `time::epoch` is a constant, while
`time::timezone()` is a context function and returned the process-local
offset.

Time setters reject invalid calendar results (setting the leap day to 2023
failed). For a `56.789` second value with a one-second quantum, floor/group
returned second 56 and ceil/round returned second 57. FastDB performs this
arithmetic with checked signed nanoseconds and rejects a zero quantum.

The reference `time::minimum` and `time::maximum` constants are outside
FastDB's currently declared year `1..=9999` datetime domain. They remain
Unsupported until the datetime domain is deliberately expanded with storage,
migration, and client-format evidence; the supported `time::min` and
`time::max` functions operate on in-domain datetime collections.

## Encoding and digest functions

The reference encoded `<bytes>'hello'` as unpadded standard base64
`aGVsbG8`, while its decoder accepted the padded form `aGVsbG8=`. JSON
encoding of `{a:1,b:[2]}` produced the compact object
`{"a":1,"b":[2]}`. JSON and CBOR round trips retained typed FastDB values;
FastDB therefore routes both formats through its collision-safe format-3
public value mapping and applies a 16 MiB input/output ceiling.

Public digest vectors for the UTF-8 string `hello` matched MD5, SHA-1,
SHA-256, SHA-512, and BLAKE3 reference outputs. `crypto::joaat('hello')`
returned the unsigned 32-bit value `3372029979`. The same functions accept
bytes without string re-encoding, reject other value categories, and do not
include source or input values in errors.
