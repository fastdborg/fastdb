# Spatial workloads: FastDB and SurrealDB

Measured 2026-10-13 on branch `feat/spatial-upgrade`. This is one local sample,
not a latency or capacity guarantee. Raw results:
[spatial-surrealdb-comparison.json](spatial-surrealdb-comparison.json).

## Setup

| Item | FastDB | SurrealDB |
| --- | --- | --- |
| Version | 2.4.0 working tree with the spatial upgrade | 3.3.0, official Linux x64 release, SHA-256 verified |
| Storage | Embedded, file-backed, one spatial index | Server with the RocksDB file backend (`surreal start rocksdb://...`) |
| Commit durability | Default (synced commits) | Default `SURREAL_DATASTORE_SYNC_DATA=every` |
| Access | Node.js client in the same process | Node.js `fetch` over loopback HTTP, one sign-in, bearer token |
| Build | Node addon from the `fastdb-production` profile | Release binary |

Both ran inside one cgroup through `scripts/fastdb-heavy`: four CPUs, 24 GiB memory,
on an Intel Xeon E5-2673 v4 under WSL2, with Node.js 24.19.0.

Each dataset has 100,000 people and 500 stored zone polygons, generated from a
fixed seed. The **cities** dataset puts the people in 20 cities with about 5 km
spread; the **uniform** dataset spreads them evenly over the sphere. Attributes
are independent of location: one third `available`, `last_seen` uniform from
0 to 99, and a `fleet` of `'night'` for 0.1% of people. Each workload runs 25
query centers drawn from the data, after one warm-up query.

SurrealDB has no spatial index type. Its documentation recommends storing a
geohash in an indexed field for dense data, so it ran each workload two ways:

- **Scan:** geometry predicates only, such as `location INSIDE $polygon` or
  `geo::distance(location, $p) <= 5000`.
- **Geohash index:** computed fields `gh4` to `gh7`
  (`DEFINE FIELD ... VALUE geo::hash::encode(location, n)`), each with a standard
  index. The client picks the finest precision whose cover stays under 64 cells
  and adds `ghN IN [cells]`, which SurrealDB plans as a union of index scans.
  Nearest search grows the radius in rounds, as FastDB does.

FastDB used its documented forms: ordinary SQL (`ST_DWithin`, `ST_Covers`, the
nearest `ORDER BY ... LIMIT` form), `search::bbox` for viewports, and for the
5 km search also the `search::near` join. Zone lookup is a scan in both systems.

Every timed query was also compared across systems. Result sets matched for all
samples of all workloads in both datasets.

Two harness details mattered for fairness:

- HTTP Basic authentication made SurrealDB verify the root password hash on every
  request, adding about 60 ms. The harness signs in once and sends a bearer token,
  as SurrealDB's SDK sessions do; a trivial query then takes about 1 ms.
- The first FastDB run used the unoptimized test build of the Node addon. The
  results below use the `fastdb-production` build.

## Results

### Cities dataset

Times are p50 / p95 milliseconds seen by the client. SurrealDB server-reported p50 is in parentheses.

| Workload | Mean rows | FastDB ordinary SQL | FastDB explicit source | SurrealDB scan | SurrealDB geohash index |
| --- | --- | --- | --- | --- | --- |
| Available people within 5 km | 371.2 | 52.0 / 95.8 | 33.0 / 52.4 | 300.5 / 348.4 (290.2) | 37.8 / 276.2 (31.0) |
| Map viewport 0.1 x 0.1 degrees | 1353.8 | 8.0 / 12.9 | n/a | 78.6 / 105.7 (65.9) | 41.2 / 60.0 (30.8) |
| Points covered by a storm polygon | 3807.2 | 150.0 / 206.4 | n/a | 113.2 / 131.1 (91.1) | 67.8 / 84.9 (46.6) |
| Which of 500 stored zones contain a point | 0.3 | 20.2 / 20.5 | n/a | 4.5 / 6.3 (1.0) | n/a |
| 5 nearest available, fresh drivers | 5 | 35.0 / 61.7 | n/a | 84.4 / 93.7 (79.5) | 650.8 / 1002.7 (622.2) |
| 5 nearest drivers of a 0.1% fleet | 5 | 445.3 / 3339.4 | n/a | 55.4 / 64.9 (50.0) | n/a |
| Cell counts in a 0.5 degree viewport | 4998.5 | 159.1 / 217.8 | n/a | 101.5 / 124.5 (88.2) | 51.6 / 67.4 (42.5) |
| Move one driver (autocommit update) | 1 | 23.4 / 39.8 | n/a | 9.4 / 11.2 (5.2) | same writes, with geohash fields |

Mismatched result samples across all workloads: 0.

| Load step | FastDB | SurrealDB |
| --- | --- | --- |
| Bulk load 100,000 people and 500 zones | 93.74 s (NDJSON import) | 14.51 s (INSERT batches of 1,000) |
| Build indexes | 8.71 s (one spatial index) | 32.4 s (four geohash indexes) |
| Files on disk after the run | 92 MB | 72 MB |

### Uniform dataset

Times are p50 / p95 milliseconds seen by the client. SurrealDB server-reported p50 is in parentheses.

| Workload | Mean rows | FastDB ordinary SQL | FastDB explicit source | SurrealDB scan | SurrealDB geohash index |
| --- | --- | --- | --- | --- | --- |
| Available people within 5 km | 0.3 | 6.8 / 7.5 | 1.7 / 2.3 | 248.4 / 315.8 (242.9) | 6.8 / 339.1 (2.2) |
| Map viewport 0.1 x 0.1 degrees | 1 | 1.4 / 1.8 | n/a | 74.9 / 87.9 (68.0) | 6.0 / 77.6 (2.0) |
| Points covered by a storm polygon | 1.1 | 6.1 / 7.1 | n/a | 87.6 / 94.5 (81.3) | 6.2 / 7.9 (1.9) |
| Which of 500 stored zones contain a point | 0 | 18.5 / 20.1 | n/a | 5.5 / 7.2 (1.3) | n/a |
| 5 nearest available, fresh drivers | 5 | 48.2 / 110.7 | n/a | 69.8 / 86.8 (64.9) | 2151.1 / 2979.5 (2076.4) |
| 5 nearest drivers of a 0.1% fleet | 5 | 1881.6 / 2253.5 | n/a | 55.3 / 68.7 (50.2) | n/a |
| Cell counts in a 0.5 degree viewport | 1.3 | 2.7 / 4.1 | n/a | 62.1 / 73.7 (56.6) | 5.7 / 6.8 (1.8) |
| Move one driver (autocommit update) | 1 | 23.8 / 37.1 | n/a | 9.0 / 10.6 (5.2) | same writes, with geohash fields |

Mismatched result samples across all workloads: 0.

| Load step | FastDB | SurrealDB |
| --- | --- | --- |
| Bulk load 100,000 people and 500 zones | 95.12 s (NDJSON import) | 14.67 s (INSERT batches of 1,000) |
| Build indexes | 8.04 s (one spatial index) | 32.71 s (four geohash indexes) |
| Files on disk after the run | 92 MB | 72 MB |
## Findings

**Where FastDB is faster.** Selective spatial searches use the two-dimensional
index directly and return in a few milliseconds:

- Map viewports in dense cities take 8 ms, about 5 times faster than SurrealDB's
  geohash pattern and 10 times faster than its scan. On uniform data they are
  about 4 times faster than the geohash pattern.
- Nearest available, fresh drivers take 35 to 48 ms, against 70 to 84 ms for a
  SurrealDB scan. SurrealDB's geohash rounds take 0.65 to 2.2 s, because each
  round is a separate request that rereads its cells.
- On uniform data, selective searches cost FastDB 1.4 to 7 ms. SurrealDB's
  geohash pattern matches that at the median, but its p95 reaches 77 to 339 ms
  for the radius and viewport searches.
- FastDB builds its one spatial index in 8 to 9 s, against 32 s for SurrealDB's
  four geohash indexes.

**Where SurrealDB is faster.** Workloads that evaluate many documents, scan, or
write are faster on SurrealDB:

- Storm polygons with about 3,800 matches: 150 ms against 68 ms.
- Dashboard grouping over about 5,000 rows: 159 ms against 52 ms.
- Zone lookup over 500 stored polygons: 18 to 20 ms against 4.5 to 5.5 ms.
- Nearest search for a rare fleet: 0.45 to 1.9 s against 55 ms.
- Single-driver updates: 23 ms against 9 ms.
- Bulk load: 94 s against 15 s.

These gaps come mostly from FastDB's general document path, not from spatial
search. Measured separately on 20,000 documents, a FastDB scan with a simple
filter costs about 5.4 µs per document, returning a typed ID row to Node costs
about 12 µs, and a spatial function call adds about 8 µs per row. SurrealDB
evaluated `geo::distance` over 100,000 documents in about 77 ms, under 1 µs per
row. When no index can prune, as with a rare fleet far from most centers, FastDB
falls back to a scan and inherits that per-document cost.

## Changes made during this comparison

The first runs exposed problems in the spatial upgrade, fixed before these results:

- A literal `LIMIT 5` disabled the nearest rewrite (literal numbers arrive as
  floating point); a test now covers literal limits.
- Exactly equivalent conditions (`ST_Covers`, `ST_Contains`, `ST_Within`,
  `ST_Intersects`, `ST_DWithin`, `<=` distance comparisons, and the nearest
  form's `IS NOT NULL` guard) are now replaced by the indexed candidate set
  instead of being evaluated again per row. Dense storm polygons fell from
  453 ms to 150 ms on data generated the same way.
- Ordinary nearest statements first find the radius holding `k` unfiltered
  points with cheap index rounds, grow by the observed shortfall, jump to a
  global round beyond 2,000 km, and fall back to the ordinary plan when only a
  global round could finish. In an earlier run whose generator left half the
  cities without fresh drivers, one nearest query read up to 155,000 index
  entries over 10 rounds and took 3.5 s; these changes bound that case.
- The new spatial index covers its key, coordinates and ID, so candidates are
  refined without reading table rows.
- Prepared regions are cached per thread in a 4,096-entry LRU, so repeated zone
  lookups do not re-validate stored polygons.

## Open items

- FastDB's per-document evaluation and result conversion cost dominates large
  result sets, scans and writes. Improving it is outside the spatial upgrade.
- The ordinary rewrite uses typed `id IN (...)` membership, which cost 1.6 times
  a join in a 1,835-row test. A join-shaped rewrite needs `SELECT *` and name
  scoping work.
- Indexed lookup of stored zones by a point (reverse geofencing) remains deferred.

## Reproduce

```sh
cargo build --locked --profile fastdb-production -p fastdb-node
scripts/fastdb-heavy node fastdb/scripts/bench-spatial-surreal.cjs PACKAGE_DIR SURREAL_BINARY WORKDIR OUTPUT.json 100000 25
```

`PACKAGE_DIR` is a copy of `fastdb/bindings/node` with `fastdb.node` replaced by
`target/fastdb-production/libfastdb_node.so`. `BENCH_ONLY`, `BENCH_DATASETS` and
`BENCH_PROFILE` select workloads and datasets and print FastDB profiles.
