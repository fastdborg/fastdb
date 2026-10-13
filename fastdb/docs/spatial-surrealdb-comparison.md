# Spatial workloads: FastDB and SurrealDB

Measured 2026-10-13 with the FastDB 2.5.0 release Node package. This is one local
sample, not a latency or capacity guarantee. Raw results:
[spatial-surrealdb-comparison.json](spatial-surrealdb-comparison.json), also
shipped in the release evidence.

## Setup

| Item | FastDB | SurrealDB |
| --- | --- | --- |
| Version | 2.5.0 release package | 3.3.0, official Linux x64 release, SHA-256 verified |
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
| Available people within 5 km | 371.2 | 31.6 / 58.1 | 20.9 / 38.5 | 272.2 / 326.2 (265.5) | 35.7 / 256.1 (29.6) |
| Map viewport 0.1 x 0.1 degrees | 1353.8 | 6.5 / 8.3 | n/a | 82.9 / 98.0 (74.4) | 42.6 / 57.6 (30.2) |
| Points covered by a storm polygon | 3807.2 | 130.1 / 172.6 | n/a | 105.4 / 122.3 (87.2) | 64.7 / 85.0 (45.0) |
| Which of 500 stored zones contain a point | 0.3 | 2.8 / 3.1 | n/a | 5.3 / 6.6 (1.3) | n/a |
| 5 nearest available, fresh drivers | 5 | 25.1 / 43.0 | n/a | 78.6 / 94.1 (73.8) | 608.0 / 994.0 (579.1) |
| 5 nearest drivers of a 0.1% fleet | 5 | 338.8 / 2528.9 | n/a | 49.7 / 63.9 (46.6) | n/a |
| Cell counts in a 0.5 degree viewport | 4998.5 | 143.9 / 179.2 | n/a | 92.8 / 105.5 (81.9) | 51.1 / 60.2 (41.1) |
| Move one driver (autocommit update) | 1 | 6.7 / 9.2 | n/a | 8.5 / 9.8 (4.8) | same writes, with geohash fields |

Mismatched result samples across all workloads: 0.

| Load step | FastDB | SurrealDB |
| --- | --- | --- |
| Bulk load 100,000 people and 500 zones | 14.76 s (NDJSON import) | 12.97 s (INSERT batches of 1,000) |
| Build indexes | 9.27 s (point and region index) | 30.79 s (four geohash indexes) |
| Files on disk after the run | 97 MB | 72 MB |

### Uniform dataset

Times are p50 / p95 milliseconds seen by the client. SurrealDB server-reported p50 is in parentheses.

| Workload | Mean rows | FastDB ordinary SQL | FastDB explicit source | SurrealDB scan | SurrealDB geohash index |
| --- | --- | --- | --- | --- | --- |
| Available people within 5 km | 0.3 | 2.1 / 4.9 | 1.6 / 2.0 | 278.7 / 326.8 (273.7) | 7.0 / 271.9 (2.3) |
| Map viewport 0.1 x 0.1 degrees | 1 | 1.4 / 1.5 | n/a | 62.5 / 78.1 (57.2) | 5.7 / 64.8 (1.9) |
| Points covered by a storm polygon | 1.1 | 2.1 / 2.4 | n/a | 69.7 / 82.6 (64.9) | 5.2 / 6.1 (1.4) |
| Which of 500 stored zones contain a point | 0 | 2.3 / 2.7 | n/a | 4.4 / 5.7 (1.0) | n/a |
| 5 nearest available, fresh drivers | 5 | 28.8 / 85.1 | n/a | 76.4 / 84.9 (71.2) | 2062.9 / 2467.1 (1992.9) |
| 5 nearest drivers of a 0.1% fleet | 5 | 1373.6 / 1620.7 | n/a | 53.5 / 65.0 (49.0) | n/a |
| Cell counts in a 0.5 degree viewport | 1.3 | 2.3 / 3.7 | n/a | 62.4 / 74.8 (57.0) | 6.0 / 7.5 (1.9) |
| Move one driver (autocommit update) | 1 | 7.3 / 9.8 | n/a | 9.0 / 10.4 (5.1) | same writes, with geohash fields |

Mismatched result samples across all workloads: 0.

| Load step | FastDB | SurrealDB |
| --- | --- | --- |
| Bulk load 100,000 people and 500 zones | 14.12 s (NDJSON import) | 14.13 s (INSERT batches of 1,000) |
| Build indexes | 8.32 s (point and region index) | 30.72 s (four geohash indexes) |
| Files on disk after the run | 97 MB | 72 MB |

### Change from the development run

The first comparison used the development branch before the three open items
below were implemented. FastDB p50 on the cities dataset:

| Workload | Development run | 2.5.0 |
| --- | --- | --- |
| Bulk load | 93.74 s | 14.76 s |
| Move one driver | 23.4 ms | 6.7 ms |
| Zone lookup | 20.2 ms | 2.8 ms |
| Available people within 5 km | 52.0 ms | 31.6 ms |
| 5 nearest available, fresh drivers | 35.0 ms | 25.1 ms |
| Storm polygon | 150.0 ms | 130.1 ms |
| Cell counts | 159.1 ms | 143.9 ms |
| 5 nearest of a 0.1% fleet | 445.3 ms | 338.8 ms |

## Findings

**Where FastDB is faster.** Selective spatial searches, zone lookups, single
updates and index builds:

- Map viewports in dense cities take 6.5 ms, about 6 times faster than SurrealDB's
  geohash pattern and 13 times faster than its scan.
- Zone lookup through the region index takes 2.3 to 2.8 ms in the client, against
  4.4 to 5.3 ms for SurrealDB over HTTP. SurrealDB's server-side time is 1.0 to
  1.3 ms, so the two are close once transport is excluded.
- Nearest available, fresh drivers take 25 to 29 ms, against 76 to 79 ms for a
  SurrealDB scan and 0.6 to 2.1 s for its geohash rounds.
- A single-driver update takes 6.7 to 7.3 ms in FastDB with a synced commit,
  against 8.5 to 9.0 ms in the SurrealDB client and 4.8 to 5.1 ms on its server.
- Bulk import now matches SurrealDB at about 14 s, and FastDB builds both spatial
  indexes in 8 to 9 s, against 31 s for SurrealDB's four geohash indexes.

**Where SurrealDB is faster.** Workloads that return or evaluate thousands of
documents, and very selective filters far from the index:

- Storm polygons with about 3,800 matches: 130 ms against 65 ms.
- Dashboard grouping over about 5,000 rows: 144 ms against 51 ms.
- Nearest search for a rare fleet: 0.34 to 1.4 s against 50 to 54 ms. The fleet
  is so rare that FastDB's rounds grow to a global radius and fall back to a
  scan with per-document filters.
- FastDB files are about 35% larger.

## Changes since the development run

The three open items from the development comparison are implemented in 2.5.0:

- **Per-document cost.** Field accessors decode one field from the stored
  encoding instead of the whole document, the stored-value nesting check is
  faster, updates and deletes maintain scalar and point-index entries by key and
  skip unchanged keys, each operation uses one stack segment, and imports insert
  without a savepoint and catalog read per document.
- **Join-shaped rewrite.** Ordinary rewrites restrict the collection with a
  semijoin on the stored ID column instead of a typed `id IN (...)` list.
- **Reverse geofencing.** Region indexes store up to 32 quadtree cells per zone
  and test candidate shapes exactly, used by `search::containing` and by
  `ST_Covers(shape, point)`.

## Remaining gaps

- Large result sets and grouping still cost FastDB several microseconds per
  document in evaluation and result conversion.
- Nearest search with a filter that matches very few documents far from most
  query points falls back to a scan.

## Reproduce

```sh
scripts/fastdb-heavy node fastdb/scripts/bench-spatial-surreal.cjs PACKAGE_DIR SURREAL_BINARY WORKDIR OUTPUT.json 100000 25
```

`PACKAGE_DIR` is the `@fastdb/node` directory installed from the release
package. `BENCH_ONLY`, `BENCH_DATASETS` and `BENCH_PROFILE` select workloads and
datasets and print FastDB profiles; `BENCH_LABEL` labels the FastDB build in the
output.
