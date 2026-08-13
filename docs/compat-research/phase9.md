# Phase 9 vector observations

Date: 2026-08-13
Reference: unmodified SurrealDB `v3.1.5` Linux x86_64 binary
Binary SHA-256: `dd9b1395baa8b6af64eb97b85887490a3ad1882aeb910277e0489b072d6e2f9f`
Public references: [SurrealDB vector search overview](https://surrealdb.com/docs/learn/data-models/vector-search/overview), [Turso vector search](https://docs.turso.tech/guides/vector-search)

These are independently authored black-box observations. No SurrealDB source,
tests, fixtures, or expected-output files were inspected or copied.

## Fixed vector fields

The reference accepted:

```surql
DEFINE TABLE item SCHEMAFULL;
DEFINE FIELD embedding ON item TYPE array<float, 2>;
CREATE item:a SET embedding = [1, 0];
```

The returned document normalized integer elements to floating-point values.
Arrays of length one or three failed the fixed-length field validation.

## Exact KNN syntax and prefilter order

With three records whose vectors were `[1,0]`, `[0,1]`, and `[0.9,0.1]`, the
reference accepted both operator forms:

```surql
SELECT id, vector::distance::knn() AS distance
FROM item
WHERE embedding <|2,COSINE|> [1,0];

SELECT id, vector::distance::knn() AS distance
FROM item
WHERE embedding <|2,EUCLIDEAN|> [1,0];
```

`vector::distance::knn()` returned the distance associated with the KNN
predicate. A bound vector introduced with `LET $q = [1,0]` was accepted in the
right operand.

When the nearest unfiltered record had `active = false`, this query returned
the two nearest records among the `active = true` set:

```surql
SELECT id FROM item
WHERE active = true AND embedding <|2,COSINE|> [1,0];
```

This establishes filter-before-top-k behavior for the characterized subset.

## Scalar vector functions

The reference returned `1.0` for:

```surql
RETURN vector::distance::euclidean([1,0], [1,1]);
```

It returned approximately `0.7071067811865475` for:

```surql
RETURN vector::similarity::cosine([1,0], [1,1]);
```

`vector::distance::cosine` was not accepted as a function path in `v3.1.5`.
FastDB therefore exposes cosine as similarity, while the KNN `COSINE` metric
continues to project its associated distance through
`vector::distance::knn()`.

## Phase 9 interpretation

FastDB implements this fixed-field, exact COSINE/EUCLIDEAN, one-predicate
subset. It deliberately rejects KNN under `OR`/`NOT`, dynamic K, mismatched or
non-finite vectors, and ordinary predicates it cannot prove are physically
applied before top-k. HNSW, DiskANN, and Turso's pinned
`toy_vector_sparse_ivf` are explicitly outside the contract. No ANN or
complete SurrealDB vector-compatibility claim is made.
