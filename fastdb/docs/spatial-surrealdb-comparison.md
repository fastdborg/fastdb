# Spatial workloads: FastDB and SurrealDB

Measured 2026-10-13 with the FastDB 2.5.1 release Node package. This is one local
sample, not a latency or capacity guarantee. Raw results:
[spatial-surrealdb-comparison.json](spatial-surrealdb-comparison.json), also
shipped in the release evidence.

## Setup

| Item | FastDB | SurrealDB |
| --- | --- | --- |
| Version | 2.5.1 release package | 3.3.0, official Linux x64 release, SHA-256 verified |
| Storage | Embedded, file-backed, a point index and a region index | Server with the RocksDB file backend (`surreal start rocksdb://...`) |
| Commit durability | Default (synced commits) | Default `SURREAL_DATASTORE_SYNC_DATA=every` |
| Access | Node.js client in the same process | Node.js `fetch` over loopback HTTP, one sign-in, bearer token |
| Build | Release Node package (`fastdb-production` profile) | Release binary |

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
5 km search also the `search::near` join. Zone lookup uses a FastDB region index
(`USING SPATIAL WITH (geometry = 'region')`) and is a scan in SurrealDB.

Every timed query was also compared across systems. Result sets matched for all
samples of all workloads in both datasets.

Two harness details mattered for fairness:

- HTTP Basic authentication made SurrealDB verify the root password hash on every
  request, adding about 60 ms. The harness signs in once and sends a bearer token,
  as SurrealDB's SDK sessions do; a trivial query then takes about 1 ms.
- FastDB runs in the client process, while SurrealDB answers over loopback HTTP.
  The server-reported SurrealDB p50 in parentheses excludes that transport cost,
  which is about 4 ms for small queries.

## Results

### Cities dataset

Times are p50 / p95 milliseconds seen by the client. SurrealDB server-reported p50 is in parentheses.

| Workload | Mean rows | FastDB ordinary SQL | FastDB explicit source | SurrealDB scan | SurrealDB geohash index |
| --- | --- | --- | --- | --- | --- |
| Available people within 5 km | 371.2 | 30.5 / 53.5 | 19.4 / 34.3 | 292.7 / 343.4 (284.8) | 46.8 / 281.1 (40.6) |
| Map viewport 0.1 x 0.1 degrees | 1353.8 | 7.6 / 11.9 | n/a | 87.1 / 97.5 (74.7) | 48.0 / 59.7 (36.6) |
| Points covered by a storm polygon | 3807.2 | 111.5 / 143.3 | n/a | 125.9 / 145.9 (102.7) | 74.0 / 94.6 (51.8) |
| Which of 500 stored zones contain a point | 0.3 | 2.9 / 3.2 | n/a | 5.5 / 7.8 (1.1) | n/a |
| 5 nearest available, fresh drivers | 5 | 29.5 / 60.8 | n/a | 86.7 / 96.2 (81.0) | 666.8 / 927.0 (634.5) |
| 5 nearest drivers of a 0.1% fleet | 5 | 378.6 / 673.0 | n/a | 57.9 / 66.6 (52.9) | n/a |
| Cell counts in a 0.5 degree viewport | 4998.5 | 135.6 / 164.5 | n/a | 99.4 / 118.7 (88.6) | 52.1 / 78.7 (43.6) |
| Move one driver (autocommit update) | 1 | 6.9 / 9.5 | n/a | 8.5 / 9.8 (5.0) | same writes, with geohash fields |

Mismatched result samples across all workloads: 0.

| Load step | FastDB | SurrealDB |
| --- | --- | --- |
| Bulk load 100,000 people and 500 zones | 14.01 s (NDJSON import) | 14.94 s (INSERT batches of 1,000) |
| Build indexes | 9.45 s (point and region index) | 30.87 s (four geohash indexes) |
| Files on disk after the run | 97 MB | 72 MB |

### Uniform dataset

Times are p50 / p95 milliseconds seen by the client. SurrealDB server-reported p50 is in parentheses.

| Workload | Mean rows | FastDB ordinary SQL | FastDB explicit source | SurrealDB scan | SurrealDB geohash index |
| --- | --- | --- | --- | --- | --- |
| Available people within 5 km | 0.3 | 2.8 / 3.3 | 2.1 / 2.3 | 286.7 / 336.8 (280.7) | 7.3 / 362.4 (2.5) |
| Map viewport 0.1 x 0.1 degrees | 1 | 1.6 / 2.0 | n/a | 76.0 / 89.4 (69.9) | 6.1 / 68.0 (1.9) |
| Points covered by a storm polygon | 1.1 | 2.4 / 4.0 | n/a | 71.4 / 83.2 (65.5) | 5.3 / 6.5 (1.5) |
| Which of 500 stored zones contain a point | 0 | 2.8 / 3.1 | n/a | 4.7 / 6.3 (1.6) | n/a |
| 5 nearest available, fresh drivers | 5 | 32.5 / 78.2 | n/a | 72.8 / 92.0 (67.3) | 2298.9 / 2585.2 (2229.3) |
| 5 nearest drivers of a 0.1% fleet | 5 | 186.5 / 218.5 | n/a | 56.1 / 63.0 (51.4) | n/a |
| Cell counts in a 0.5 degree viewport | 1.3 | 2.2 / 3.1 | n/a | 70.0 / 76.4 (64.7) | 5.7 / 7.4 (1.9) |
| Move one driver (autocommit update) | 1 | 6.9 / 9.2 | n/a | 8.6 / 9.8 (4.9) | same writes, with geohash fields |

Mismatched result samples across all workloads: 0.

| Load step | FastDB | SurrealDB |
| --- | --- | --- |
| Bulk load 100,000 people and 500 zones | 14.51 s (NDJSON import) | 14.63 s (INSERT batches of 1,000) |
| Build indexes | 10.37 s (point and region index) | 31.39 s (four geohash indexes) |
| Files on disk after the run | 97 MB | 72 MB |

### Change across releases

FastDB on the cities dataset. The development run preceded the 2.5.0 open items; 2.5.0 and 2.5.1 are release packages. Other values are p50.

| Workload | Development run | 2.5.0 | 2.5.1 |
| --- | --- | --- | --- |
| Bulk load | 93.74 s | 14.76 s | 14.01 s |
| Move one driver | 23.4 ms | 6.7 ms | 6.9 ms |
| Zone lookup | 20.2 ms | 2.8 ms | 2.9 ms |
| Available people within 5 km | 52.0 ms | 31.6 ms | 30.5 ms |
| 5 nearest available, fresh drivers | 35.0 ms | 25.1 ms | 29.5 ms |
| Storm polygon | 150.0 ms | 130.1 ms | 111.5 ms |
| Cell counts | 159.1 ms | 143.9 ms | 135.6 ms |
| 5 nearest of a 0.1% fleet, p50 | 445.3 ms | 338.8 ms | 378.6 ms |
| 5 nearest of a 0.1% fleet, p95 | 3339.4 ms | 2528.9 ms | 673.0 ms |

On the uniform dataset, the 0.1% fleet query fell from 1374 / 1621 ms in 2.5.0 to 187 / 219 ms (p50 / p95). Run-to-run variation on this host is about 10 to 20% for the faster workloads.

## Findings

**Where FastDB is faster.** Selective spatial searches, zone lookups, single
updates and index builds:

- Map viewports in dense cities take 7.6 ms, about 6 times faster than SurrealDB's
  geohash pattern and 11 times faster than its scan.
- Zone lookup through the region index takes 2.8 to 2.9 ms in the client, against
  4.7 to 5.5 ms for SurrealDB over HTTP. SurrealDB's server-side time is 1.1 to
  1.6 ms, so the two are close once transport is excluded.
- Nearest available, fresh drivers take 30 to 33 ms, against 73 to 87 ms for a
  SurrealDB scan and 0.7 to 2.3 s for its geohash rounds.
- A single-driver update takes about 7 ms in FastDB with a synced commit, against
  8.5 ms in the SurrealDB client and 5 ms on its server.
- Bulk import matches SurrealDB at about 14 s, and FastDB builds both spatial
  indexes in 9 to 10 s, against 31 s for SurrealDB's four geohash indexes.

**Where SurrealDB is faster.** Workloads that return or evaluate thousands of
documents, and very selective filters without an index:

- Storm polygons with about 3,800 matches: 112 ms against 74 ms.
- Dashboard grouping over about 5,000 rows: 136 ms against 52 ms.
- Nearest search for a 0.1% fleet without a fleet index: 187 to 379 ms against
  56 to 58 ms. Rounds stop once the match rate shows that a scan is cheaper, and
  FastDB's scan costs more per document. With an index on the fleet field, the
  local evaluation in the [spatial evidence](spatial-upgrade-evidence.md#251-follow-up)
  answers the same query in about 7 ms.
- FastDB files are about 35% larger.

## Changes since the development run

2.5.0 implemented the three open items from the development comparison:

- **Per-document cost.** Field accessors decode one field instead of the whole
  document, updates and deletes maintain scalar and point-index entries by key and
  skip unchanged keys, each operation uses one stack segment, and imports insert
  without a savepoint and catalog read per document.
- **Join-shaped rewrite.** Ordinary rewrites restrict the collection with a
  semijoin on the stored ID column instead of a typed `id IN (...)` list.
- **Reverse geofencing.** Region indexes store up to 32 quadtree cells per zone
  and test candidate shapes exactly, used by `search::containing` and by
  `ST_Covers(shape, point)`.

2.5.1 then made field reads stop at the requested field and return stored bytes,
ended nearest rounds early for selective filters, and used scalar indexes on such
filters when they select fewer documents.

## Remaining gaps

- Large result sets and grouping still cost FastDB more per document than
  SurrealDB in evaluation and result conversion.
- Nearest search with a very selective filter and no index on that field ends in
  a scan, which is several times slower per document than SurrealDB's. Index such
  fields.

## Reproduce

```sh
scripts/fastdb-heavy node fastdb/scripts/bench-spatial-surreal.cjs PACKAGE_DIR SURREAL_BINARY WORKDIR OUTPUT.json 100000 25
```

`PACKAGE_DIR` is the `@fastdb/node` directory installed from the release
package. `BENCH_ONLY`, `BENCH_DATASETS` and `BENCH_PROFILE` select workloads and
datasets and print FastDB profiles; `BENCH_LABEL` labels the FastDB build in the
output.
