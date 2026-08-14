# Phase 7 clean-room graph observations

Status: independent black-box and public-document research, 2026-08-13

## Reference environment

- SurrealDB executable: unmodified official Linux x86-64 `v3.1.5`
- Version output: `3.1.5 for linux on x86_64`
- Download URL:
  `https://download.surrealdb.com/v3.1.5/surreal-v3.1.5.linux-amd64.tgz`
- Download SHA-256:
  `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`
- Executable SHA-256:
  `dd9b1395baa8b6af64eb97b85887490a3ad1882aeb910277e0489b072d6e2f9f`
- Backend: fresh embedded `memory` endpoint for each independent query batch
- Namespace/database: `fastdb` / `phase7`
- Output mode: CLI JSON

The queries below were independently written for FastDB. No SurrealDB source,
tests, fixtures, expected-output files, or fuzz corpus were inspected or copied.
Random edge IDs are normalized to `<edge-id>` and object ordering is normalized
for readability.

## Public sources

- `https://surrealdb.com/docs/reference/query-language/statements/define/table`
- `https://surrealdb.com/docs/reference/query-language/statements/relate`
- `https://surrealdb.com/docs/learn/schema-management/tables-and-fields/tables`
- `https://surrealdb.com/docs/learn/data-models/graph/graph-traversal`
- `https://surrealdb.com/docs/reference/query-language/statements/delete`

Online documentation is supporting research, not a moving compatibility
reference. The executable observations below pin behavior to `v3.1.5`.

## Relation table definition

Input:

```surql
DEFINE TABLE person TYPE NORMAL;
DEFINE TABLE post TYPE NORMAL;
DEFINE TABLE wrote SCHEMAFULL
  TYPE RELATION FROM person TO post ENFORCED;
DEFINE FIELD role ON wrote TYPE string;
INFO FOR TABLE wrote;
```

Observed normalized result:

```text
fields.in  = DEFINE FIELD in ON wrote TYPE record<person> PERMISSIONS FULL
fields.out = DEFINE FIELD out ON wrote TYPE record<post> PERMISSIONS FULL
fields.role = DEFINE FIELD role ON wrote TYPE string PERMISSIONS FULL
```

`FROM`/`TO` is accepted as the synonym of `IN`/`OUT`. A relation definition
may name endpoint tables before any endpoint records exist. FastDB can satisfy
its immutable catalog-ID model by atomically registering absent endpoint table
catalogs without creating endpoint records.

## RELATE shape, content, and return clauses

Input:

```surql
CREATE person:a;
CREATE post:x;
RELATE ONLY person:a->wrote->post:x
  SET role = "author" RETURN AFTER;
```

Observed normalized result:

```text
{
  id: wrote:<edge-id>,
  in: person:a,
  out: post:x,
  role: "author"
}
```

Without `ONLY`, the default/`RETURN AFTER` shape is a one-element array. With
`ONLY`, it is the edge object. `RETURN NONE` produces an empty array without
`ONLY`; `RETURN BEFORE` produces `[null]`; `CONTENT` and `SET` both attach user
fields. `LET` values holding record IDs are accepted for the two endpoints.

Phase 7 implements only `RETURN NONE`, `RETURN BEFORE`, and `RETURN AFTER`,
matching the already supported FastDB mutation return subset. `DIFF`, arbitrary
return projections, `VALUE`, timeout, `OR UPDATE`, arrays/cartesian products,
and explicit edge IDs remain rejected.

## Dangling and enforced endpoints

Input:

```surql
RELATE ONLY person:missing->likes->post:missing RETURN AFTER;
SELECT * FROM likes;
```

Observed: the edge is created and direct edge-table selection returns its
`id`, `in`, and `out`, even though neither endpoint record exists.

Input:

```surql
SELECT ->likes->post AS reached FROM ONLY person:missing;
CREATE person:missing;
SELECT ->likes->post AS reached FROM ONLY person:missing;
SELECT ->likes->post.* AS docs FROM ONLY person:missing;
```

Observed:

```text
before source table/record exists: table/record error
after source exists: reached = [post:missing]
after source exists, target document absent: docs = []
```

Creating `post:missing` makes `docs` contain its complete document. Therefore
endpoint-ID traversal follows adjacency even for a dangling target, while `.*`
materialization filters absent documents.

Input:

```surql
DEFINE TABLE linked TYPE RELATION IN person OUT post ENFORCED;
RELATE person:a->linked->post:x;
CREATE person:a;
RELATE person:a->linked->post:x;
```

Observed errors, in order:

```text
The record 'person:a' does not exist
The record 'post:x' does not exist
```

After both records exist, relation creation succeeds. Endpoint table-type
mismatches are errors independently of `ENFORCED`; enforcement controls record
existence, not type constraints.

## Relation immutability and table kind

Creating an ordinary record directly in a `TYPE RELATION` table fails because
the record is not a relation. Using a `TYPE NORMAL` table as the middle table
of `RELATE` likewise fails. An absent middle table is implicitly registered by
`RELATE` and accepts relation records.

Updating `in` on an existing `v3.1.5` edge leaves the endpoint unchanged. The
FastDB subset makes this immutability explicit by returning a constraint error
for attempts to store or assign top-level `id`, `in`, or `out`; it never
silently accepts an ignored mutation.

## Traversal

Setup:

```surql
CREATE person:a SET name = "A";
CREATE person:b SET name = "B";
CREATE person:c SET name = "C";
CREATE post:x SET title = "X";
CREATE post:y SET title = "Y";
RELATE person:a->wrote->post:x;
RELATE person:a->wrote->post:y;
RELATE person:b->wrote->post:x;
RELATE person:a->likes->person:b;
RELATE person:b->likes->person:c;
RELATE person:c->likes->person:a;
```

Observed queries, with array order treated as unspecified:

```surql
SELECT ->wrote->post AS ids,
       ->wrote->post.* AS docs
FROM ONLY person:a;
-- ids = {post:x, post:y}; docs materialize both post documents

SELECT <-wrote<-person AS authors FROM ONLY post:x;
-- authors = {person:a, person:b}

SELECT ->likes->person->likes->person AS two_hops FROM ONLY person:a;
-- two_hops = [person:c]

SELECT <->likes<->person AS both FROM ONLY person:a;
-- both = [person:c, person:a, person:a, person:b] in observed order
```

The bidirectional result demonstrates that `<->` combines both endpoint roles
and preserves duplicates, including the starting record when reached through
the opposite endpoint. FastDB does not promise traversal array order without
an ordering surface, but it preserves multiplicity.

Phase 7 accepts graph paths only as SELECT projection expressions, requires an
alias under the existing expression-projection rule, and bounds manually
spelled fixed-depth paths. Standalone and recursive paths remain unsupported.

## Cascade deletion

With the setup above, input:

```surql
DELETE person:b RETURN BEFORE;
SELECT * FROM likes;
SELECT * FROM wrote;
```

Observed:

- `person:b` is returned before deletion;
- both `likes` edges connected to `person:b` are removed;
- `person:c->likes->person:a` remains;
- `person:b->wrote->post:x` is removed;
- both edges from `person:a` remain.

FastDB implements this as one transaction. It issues separately indexed
forward and reverse edge deletions for every deleted normal record so neither
direction requires a table scan. Failure at any cascade boundary rolls back
the edge deletions and node deletion together.

## Deliberate subset boundaries

The following are explicitly outside Phase 7 even where `v3.1.5` supports
them: multiple endpoint table alternatives, arrays/cartesian targets, explicit
complex edge IDs, `OR UPDATE`, relation-path filters, recursive idioms,
standalone traversal expressions, arbitrary traversal return projections,
edge records used as graph endpoints, and mutation of synthesized endpoint
fields.
