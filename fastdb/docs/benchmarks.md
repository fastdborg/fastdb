# Local benchmark harness

Run from the repository root with the pinned Rust toolchain:

```sh
cargo build --locked -p fastdb-cli
python3 fastdb/scripts/benchmark.py --rows 1000 --samples 3 --output /tmp/fastdb-smoke.json
python3 fastdb/scripts/benchmark.py --rows 100000 --samples 7 --output /tmp/fastdb-100k.json
```

This uses the ordinary dev build. Do not interpret these numbers as optimized release performance. The harness accepts up to 1,000,000 rows, 1,024 vector dimensions and 100 samples; larger runs are explicit local qualification work, not CI. Python 3, Unix pipe polling and the CLI binary are required. A statement has a 600-second timeout. The temporary database is removed after the run or a handled failure, and output is written only after successful verification.

The fixture has typed integer record IDs, 100 equally distributed scalar groups, titles and dense float32 vectors. It loads in one transaction with 100-row statements, measures an unindexed equality filter, creates a managed index, repeats the identical filter, then performs exact cosine top-10 with an explicit ID tie-breaker. Each workload has one unmeasured warmup. Assertions verify filter counts, index search in the reported plan, result cardinality, the exact nearest vector and distance ordering.

The default 16-dimensional vectors are synthetic. Except for the designated nearest record, their values repeat every 997 keys. This is a reproducible functional baseline, not representative coverage of real high-dimensional embedding distributions. Loading and index construction have separate elapsed times. Each query sample measures the complete CLI round trip, including frontend work and JSON encoding/transport. It does not isolate engine execution time.

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
