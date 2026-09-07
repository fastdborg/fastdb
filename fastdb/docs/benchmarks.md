# Local benchmark harness

Run from the repository root with the pinned Rust toolchain:

```sh
cargo build --locked -p fastdb-cli
python3 fastdb/scripts/benchmark.py --rows 1000 --samples 3 --output /tmp/fastdb-smoke.json
python3 fastdb/scripts/benchmark.py --rows 100000 --samples 7 --output /tmp/fastdb-100k.json
python3 fastdb/scripts/benchmark.py --fixture seeded --rows 1000 --dimensions 768 --samples 3 --output /tmp/fastdb-seeded-768.json
```

This uses the ordinary dev build. Do not interpret these numbers as optimized release performance. The harness accepts up to 1,000,000 rows, 1,024 vector dimensions and 100 samples; larger runs are explicit local qualification work, not CI. Python 3, Unix pipe polling and the CLI binary are required. A statement has a 600-second timeout. The temporary database is removed after the run or a handled failure, and output is written only after successful verification.

The fixture has typed integer record IDs, 100 equally distributed scalar groups, titles and dense float32 vectors. It loads in one transaction with 100-row statements, measures an unindexed equality filter, creates a managed index, repeats the identical filter, then performs exact cosine top-10 with an explicit ID tie-breaker. Each workload has one unmeasured warmup. Assertions verify filter counts, index search in the reported plan, unique/in-range result IDs, the exact nearest vector, and distance/ID ordering. An independent reference rounds generated coordinates to float32, computes cosine to the basis-vector target using Python float64 fsum, and retains the best ten with a bounded heap. It runs outside the measured query samples. Reported distances must be within 2e-6 absolute error; returned records must lie within that tolerance of the reference cutoff and include reference records strictly below cutoff minus tolerance. This permits float32 accumulation/tie differences near the boundary. Reference results and tolerance are retained in new reports.

The default `--fixture cyclic` vectors are synthetic and 16-dimensional. Except for the designated nearest record, their values repeat every 997 keys. `--fixture seeded` uses a separate Python Random instance seeded by each integer record key, with the first coordinate fixed at one and the remaining coordinates drawn in [-1,1). It avoids the cyclic fixture's fixed 997-key period. Both retain the special nearest record [1,0,...]. Reports identify the fixture version and Python version; neither fixture represents real embedding distributions. Loading and index construction have separate elapsed times. Each query sample measures the complete CLI round trip, including frontend work and JSON encoding/transport. It does not isolate engine execution time.

Reports contain every latency sample, median, nearest-rank p95, plans, database size, binary SHA-256, source commit and dirty-worktree status. On Linux, process peak RSS comes from `/proc/PID/status`; it includes earlier work and is not isolated query allocation. On other platforms that measurement is null. Current runs retain each sample's primary engine counters in `engine_counters`; the top-level rows-read/fullscan fields repeat the first sample. These exclude frontend catalog/lowering helper statements and Rust decoding. Original baseline reports predate profiling and retain null counters; those nulls mean unmeasured, not zero. Workload cardinality must not be presented as measured scanned rows.

Remaining release evidence includes broader counter qualification, optimized-build qualification under the repository's approved workflow, cold/warm cache separation, representative high-dimensional embeddings, 100k–1m scaling, concurrency, repeated independent runs and regression thresholds. This harness does not satisfy the full benchmark release gate by itself.

## Initial Linux dev-build evidence — 2026-09-07

Measured with the local Rust 1.88.0 dev CLI at commit `d732de4bd`; binary hashes and dirty harness/document state are in the reports. The 1,000-row final smoke used three measured samples; the 100,000-row run used two. Both used 16-dimensional vectors and one warmup per workload. These small sample counts do not establish stable tail latency.

| Documents | Unindexed filter median | Indexed filter median | Exact vector top-10 median |
|---|---:|---:|---:|
| 1,000 | 79.7 ms | 4.08 ms | 297.6 ms |
| 100,000 | 5,844.4 ms | 321.2 ms | 29,416.9 ms |

The 100,000-row load took 310.6 seconds and index construction took 98.0 seconds. Its final process high-water RSS was 177,102,848 bytes and checkpointed database size was 76,652,544 bytes. The indexed plan used the named managed index. These original reports did not measure scan counters. This establishes a 100k synthetic debug baseline, not the full 100k–1m representative-vector gate.

Raw reports: [1,000 documents](benchmark-results/2026-09-07-linux-dev-1000.json), [100,000 documents](benchmark-results/2026-09-07-linux-dev-100000.json).

## Instrumented 1,000-document smoke

The [instrumented report](benchmark-results/2026-09-07-linux-dev-profile-1000.json) records three samples per workload using the profiling implementation. All three samples had identical engine counters. The unindexed filter read 1,000 physical rows with 999 fullscan steps; the indexed filter read 20 physical rows with zero fullscan steps and 31 B-tree seeks. Exact-vector top-10 read 1,000 rows with 999 fullscan steps and one sort. Physical rows include index/table operations and must not be equated with returned documents.

The original 100,000-document artifact retains its unmeasured null counters. A later instrumented run is recorded below. The 1,000-document smoke validates reporting and scan reduction, not full release qualification.

## Parser stack protection smoke

The [parser-stack report](benchmark-results/2026-09-07-linux-dev-parser-stack-1000.json) measures the final same-thread stack guards with three samples per workload and 1,000 documents. Median CLI round trips were 57.4 ms unindexed, 4.27 ms indexed and 304.0 ms exact-vector top-10. The result and plan assertions passed, and scan counters retained the expected 1,000 versus 20 physical rows for unindexed/indexed filters.

Public execution/profiling/audit guards allow internal parsing and execution to reuse an auxiliary stack. An intermediate version with guards only on internal calls measured 84.0/6.16/316.8 ms in a separate small run; these few samples do not isolate overhead or establish a performance trend. The retained report is the final version, run after other validation processes completed. Full representative, optimized, large-scale and platform benchmarking remains open.

## Instrumented 100,000-document run

The [100,000-document instrumented report](benchmark-results/2026-09-07-linux-dev-profile-100000.json) was produced from clean commit `10af87419` with the local Rust 1.88 dev CLI, 16-dimensional synthetic vectors, one warmup and three measured samples per workload. All result and plan assertions passed. Each workload's engine counters were identical across the three samples.

| Workload | Median CLI round trip | Engine rows read | Fullscan steps | Sorts |
|---|---:|---:|---:|---:|
| Unindexed equality filter | 5,807.94 ms | 100,000 | 99,999 | 0 |
| Indexed equality filter | 284.38 ms | 2,000 | 0 | 0 |
| Exact cosine top-10 | 30,616.53 ms | 100,000 | 99,999 | 1 |

The filter returned the expected count of 1,000 matching documents in both cases. The indexed plan used docs_group and recorded 3,001 B-tree seeks. Rows read are primary engine physical operations, including index/table work, rather than logical matched-document counts. The vector query returned ten ordered distances with the expected zero-distance nearest record.

Loading took 324.99 seconds, index construction 95.70 seconds, and the checkpointed database occupied 76,652,544 bytes. Final process high-water RSS was 176,701,440 bytes, including loading and earlier workloads. The harness completed its TRUNCATE checkpoint and clean CLI exit. Binary identity, source state, all samples and query plans are retained in the report.

This supplies measured 100k scan/index evidence for the synthetic debug workload. Three samples cannot establish stable p95 latency; the reported p95 is the largest sample. The vectors repeat after 997 keys apart from the special nearest record. Representative higher dimensions, 1m-scale evaluation, optimized builds, cold caches, concurrency, platform coverage and release performance conclusions remain open.

## Seeded 768-dimensional fixture and independent reference

The enhanced harness passed both [cyclic 16-dimensional](benchmark-results/2026-09-07-linux-dev-reference-cyclic-1000.json) and [seeded 768-dimensional](benchmark-results/2026-09-07-linux-dev-seeded-768-1000.json) runs with 1,000 documents and three samples per workload. All samples passed the independent float32-coordinate cosine reference, including the cutoff/tolerance checks described above. Engine counters were identical across each workload's samples.

| Fixture | Unindexed median | Indexed median | Exact cosine top-10 median |
|---|---:|---:|---:|
| Cyclic, 16 dimensions | 85.44 ms | 4.42 ms | 293.56 ms |
| Seeded, 768 dimensions | 805.27 ms | 19.98 ms | 3,715.40 ms |

Both filters read 1,000/20 physical rows before/after indexing, with 999/zero fullscan steps. Exact search read 1,000 rows with 999 fullscan steps and one sort. The seeded database occupied 12,500,992 bytes after checkpoint, and final process high-water RSS was 63,053,824 bytes. Loading took 8.43 seconds and index construction 1.33 seconds. The two fixture sizes and distributions differ, so this table does not isolate dimensionality's effect. These are small synthetic dev-build runs, not real embedding distributions or high-dimensional 100k–1m qualification.

## Collection audit diagnostic

After building the local Node addon, run `node fastdb/scripts/bench-audit.cjs` for 100/300/1,000 rows or pass explicit counts up to 10,000. It uses an in-memory collection and one unique integer index, prints insertion and audit phases separately, and validates document/index-entry counts. It is maintainer-run rather than part of routine CI.

On this Linux development machine, before the index-audit change the 100/300/1,000-row audit times were about 89/431/3,397 ms; after the change they were about 82/253/792 ms. Insertion at 1,000 rows remained about 2.5 seconds. A subsequent 10,000-row run completed insertion in 27.7 seconds and auditing in 8.65 seconds. These are single-run debug-addon diagnostics, not release throughput or latency guarantees. The prior 10,000-row cancellation fixture stopped before completion and supplies no complete before-timing at that size.

The audit now scans each index once, checks the referenced document through its primary key, compares expected keys using native IS semantics and tracks bounded unique IDs to reject duplicate/missing coverage. A VM-step regression with duplicate and NULL keys checks that quadrupling rows from 64 to 256 uses less than six times the engine work. This guards against per-document index scans; it does not certify all workload scaling or memory usage.

## Isolated document transfer diagnostic

Run `node fastdb/scripts/bench-transfer.cjs [rows] [text-bytes]` after building the addon. Defaults are 1,000 rows and 4,096 repeated ASCII text bytes per document. Inputs are bounded to 10,000 rows and an estimated 16 MiB fixture. Each format runs in a fresh Linux child process with a unique integer index; the harness records import/export time, encoded sizes, current RSS and process peak RSS. It verifies count, numeric sum, text lengths, index integrity and exact export/import/export equality after measurements. Temporary fixtures are removed. This is a maintainer diagnostic, not routine CI.

The [1,000-document report](benchmark-results/2026-09-07-linux-dev-transfer-1000.json) records the clean implementation commit, addon hash and harness hash on Linux x64/Node 24.19.0:

| Format | Encoded input | Import | Export | RSS after import | RSS after export |
|---|---:|---:|---:|---:|---:|
| JSON | 4,283,846 bytes | 3,708 ms | 326 ms | 133,619,712 bytes | 151,314,432 bytes |
| NDJSON | 4,283,822 bytes | 3,621 ms | 322 ms | 131,842,048 bytes | 150,847,488 bytes |

These are one debug-addon sample per format with no forced garbage collection. The input string remains live during export, and peak RSS includes all earlier work in that child, including import; it is not export-only peak memory. Engine pages, native allocators, JavaScript memory and buffer capacities all contribute. The small observed RSS difference does not establish a general memory or speed advantage. No pre-change binary was measured, so this is not before/after evidence for the incremental transfer implementation. Larger/diverse fixtures, repeated samples and optimized builds remain qualification work.


## JSON preflight/replay measurements

The [three-run report](benchmark-results/2026-09-07-linux-dev-transfer-json-replay-1000.json) measures clean implementation `8e7b5893d` with the unchanged transfer harness, 1,000 documents and 4,096 text bytes per document. All six fresh-process format samples passed content aggregates, index integrity and exact round trips. Source, addon and harness identities are retained and were checked against the current files.

| Format | Import median (range) | Export median (range) | RSS after import range |
|---|---:|---:|---:|
| JSON | 3,889 ms (3,642–4,028) | 403 ms (342–428) | 131,858,432–133,308,416 bytes |
| NDJSON | 3,681 ms (3,563–3,842) | 325 ms (320–330) | 131,842,048–132,767,744 bytes |

The earlier JSON materializing implementation had one sample at 3,708 ms import and 133,619,712 bytes RSS after import. Its timing falls inside the new range; the small RSS difference does not establish a repeatable reduction. Export implementation was unchanged, yet its timing also varied. This is not a controlled causal comparison or optimized-build result. The new import implementation removes the retained decoded document array and parses twice; this workload does not show that tradeoff dominates whole-process costs. Larger/diverse fixtures, controlled repeated baseline samples and release builds remain open.
