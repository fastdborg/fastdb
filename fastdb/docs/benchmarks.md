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


## Forward-fetch batching diagnostic

Run `node fastdb/scripts/bench-fetch.cjs [positions]` after building the local addon; the default is 1,000 positions, bounded to 130–8,000. The in-memory fixture compares collection and relational targets, one/130/all distinct keys, and one/two fetched projections. Each workload has one warmup and three timed samples. Assertions check every fetched integer, duplicate results, expected 128-key batch count and stable engine counters. The [1,000-position report](benchmark-results/2026-09-07-linux-dev-fetch-1000.json) records implementation 23f7030c9, clean implementation paths, addon/harness hashes and all samples.

| Target | Distinct keys | One projection median | Two projections median | Target batches | Target rows read | Target VM steps |
|---|---:|---:|---:|---:|---:|---:|
| Collection | 1 | 107.47 ms | 185.39 ms | 1 | 2 | 28 |
| Collection | 130 | 153.21 ms | 225.35 ms | 2 | 259 | 1,975 |
| Collection | 1,000 | 310.65 ms | 407.30 ms | 8 | 1,999 | 15,103 |
| Relational | 1 | 65.18 ms | 116.30 ms | 1 | 1 | 26 |
| Relational | 130 | 88.29 ms | 137.85 ms | 2 | 130 | 1,716 |
| Relational | 1,000 | 200.04 ms | 250.75 ms | 8 | 1,000 | 13,104 |

Target counters were identical for one and two projections, confirming deduplicated target reads in these fixtures. Physical row reads include engine index/table operations and need not equal logical target counts. Elapsed time includes outer evaluation, frontend decoding/result construction and Node transport; duplicate output still costs time. These are synthetic debug-addon measurements in one process with warm caches, not isolated CPU attribution, memory evidence or release latency guarantees. Three samples cannot establish a stable p95.


## 100,000 seeded vectors at 768 dimensions (2026-09-07)

The [completed report](benchmark-results/2026-09-07-linux-dev-seeded-768-100000.json) measures clean commit `bc88ad618b39fb054a32c5ddec39fed0f917aded` on Linux x64/WSL2, using the unoptimized CLI built with Rust 1.88.0 (`cargo build --locked -p fastdb-cli`). Command: `python3 fastdb/scripts/benchmark.py --rows 100000 --dimensions 768 --samples 3 --fixture seeded --output fastdb/docs/benchmark-results/2026-09-07-linux-dev-seeded-768-100000.json`. The process exited successfully. Binary SHA-256 matches the report; harness SHA-256 is `5c2c24e8a5c4df2977834e8c204702e8e40132c6ef943b9f0c6f3ec670d2f682`.

| Workload | Median | Sample range | Primary rows read | Primary VM steps |
|---|---:|---:|---:|---:|
| Unindexed filter | 82.45 s | 82.42–82.65 s | 100,000 | 401,015 |
| Indexed filter | 1.12 s | 1.08–1.14 s | 2,000 | 18,025 |
| Exact cosine top-10 | 388.11 s | 387.93–389.52 s | 100,000 | 1,200,080 |

Loading took 966.79 seconds and index construction 179.27 seconds. The checkpointed database occupied 1,247,805,440 bytes. Every workload has one warmup and three measured CLI round trips. Filter counts matched 1,000 before and after indexing; the indexed plan names `docs_group` and records zero full-scan steps. Vector queries scan all documents and use a sorter. Warmup and measured top-10 results passed ordering, uniqueness, cutoff membership and per-distance checks against an independent float64 reference over rounded float32 coordinates, with absolute tolerance 2e-6. Repeated primary engine counters and reported medians were checked for consistency.

The recorded `/proc` VmHWM values range from 2,637,705,216 to 2,641,252,352 bytes (about 2.46 GiB), but the last value decreases despite the same child process. Preserve these raw observations; the platform/accounting inconsistency means they should not be treated as a reliable monotonic peak or evidence of a memory reduction. They also include earlier workloads and loading, not isolated query memory.

This establishes a completed synthetic 100,000 × 768 correctness/performance diagnostic. It does not qualify release latency: the build is unoptimized, the data are seeded synthetic vectors, there is one process with warm caches, and three samples cannot establish a stable p95. Real embedding distributions, optimized builds, platform coverage, larger scales, cold/concurrent workloads and complete resource limits remain open. These measured latencies warrant profiling before making performance claims.


## Optimized 100,000 × 768 comparison (2026-09-07)

The [release-profile report](benchmark-results/2026-09-07-linux-release-seeded-768-100000.json) completed successfully at clean commit `83583ecdda91aa3f595dcef7d39545523520ac47`. Build command: `cargo build --locked --release -p fastdb-cli`, with Rust 1.88.0 and the same dependency lockfile. The existing release profile uses thin LTO, four code-generation units, abort-on-panic and line-table debug information; compilation took 2m48s. This is the `release` profile, not `release-official` or a published artifact.

Run command: `python3 fastdb/scripts/benchmark.py --binary target/release/fastdb-cli --rows 100000 --dimensions 768 --samples 3 --fixture seeded --output fastdb/docs/benchmark-results/2026-09-07-linux-release-seeded-768-100000.json`. The unchanged harness hash is recorded in the preceding diagnostic. The binary SHA-256 was verified against the report: `80d383d176fb1729041b5da09fd63601a5eebd132d27b746f2e1d0d676e54193`.

| Workload | Optimized median | Optimized sample range | Debug median |
|---|---:|---:|---:|
| Unindexed filter | 9.48 s | 9.33–9.52 s | 82.45 s |
| Indexed filter | 138.64 ms | 132.74–148.26 ms | 1,124.82 ms |
| Exact cosine top-10 | 47.12 s | 47.03–47.29 s | 388.11 s |

Loading took 190.51 seconds and index construction 13.03 seconds. The checkpointed size was identical at 1,247,805,440 bytes. All filter and vector reference assertions passed for warmups and measured samples. Independent reference values, per-workload primary engine counters and database size match the debug report exactly; medians and current binary identity were independently checked. The intervening commit contains only diagnostic documentation and its report, so product code and harness were unchanged.

Reported VmHWM observations are 2,622,300,160 then 2,620,866,560 bytes (about 2.44 GiB). The decreasing accounting caveat from the debug run recurs; these observations do not establish a reliable monotonic peak or a meaningful memory improvement. Source code, build configuration, platform and measurement scope are explicit, but these sequential three-sample synthetic runs do not establish general speedup, stable p95, cold-cache behavior or concurrent performance. The 47-second full-scan top-10 latency remains a concrete performance limitation for this workload. Profiling document decoding and vector execution, real embedding distributions, broader scales/platforms and complete resource qualification remain open.


## Combined vector-field accessor at 100,000 × 768 (2026-09-07)

The [post-change report](benchmark-results/2026-09-07-linux-release-vector-field-768-100000.json) completed at clean implementation commit `3cac94cae`, using the same release profile, Rust 1.88.0, seeded fixture and unchanged harness. Command: `python3 fastdb/scripts/benchmark.py --binary target/release/fastdb-cli --rows 100000 --dimensions 768 --samples 3 --fixture seeded --output fastdb/docs/benchmark-results/2026-09-07-linux-release-vector-field-768-100000.json`. The rebuilt binary hash matches `cb9e8b99209bf3cef1c589a78113414740407181e7faa1c5b5cec0fcf71dc081`.

| Workload | Median before | Median after | After sample range |
|---|---:|---:|---:|
| Unindexed filter | 9.48 s | 9.26 s | 9.16–9.44 s |
| Indexed filter | 138.64 ms | 117.97 ms | 114.72–122.31 ms |
| Exact cosine top-10 | 47.12 s | 32.71 s | 32.60–32.76 s |

Warmup and all measured results passed count, ordering, uniqueness, cutoff membership and per-distance reference checks. The independent cosine reference and checkpointed database size (1,247,805,440 bytes) match the baseline. Loading took 192.68 seconds and index construction 12.36 seconds. Report medians, binary/commit identity and repeated counters were independently checked.

Vector primary VM steps fell from 1,200,080 to 1,100,080, consistent with one removed scalar-function call per document. Vector rows read remain 100,000 with 99,999 full-scan steps and one sort. Filter counters remain unchanged. Median top-10 time is 30.6% lower in these sequential runs; unchanged filter timings also vary, so avoid treating the comparison as a universal speedup. The query still takes over 32 seconds on this synthetic workload.

VmHWM observations are 2,622,386,176 then 2,620,030,976 bytes, retaining the earlier platform-accounting decrease. No memory improvement is established. Three warm samples, one machine, synthetic vectors and a single process do not establish stable p95, real-workload performance, cold/concurrent behavior or complete resource qualification. Whole-document decoding and broader V1 gates remain open.

## Node result transport memory diagnostic

Run `node fastdb/scripts/bench-results.cjs 1000 4096` after building the local addon.
The Linux-only harness records three fresh processes per sync/worker and
execute/profile combination, baseline RSS, process peak RSS, wall time and source,
addon and harness hashes. It validates every returned integer and binary byte after
measurement. Parameters cap the diagnostic at 32 MiB of binary payload.

The recorded debug run at `47252916f` returned 1,000 rows containing an integer and
4,096-byte binary value each (4,104,000 logical cell bytes). Twelve samples took
1,007–1,319 ms; peak process RSS ranged from about 216 to 232 MiB. Raw evidence:
[Node results diagnostic](benchmark-results/2026-09-08-linux-debug-node-results-1000.json).
These peaks include process startup, setup, addon and worker memory; they are not
query-exclusive allocation measurements. There is no baseline-build comparison,
release performance guarantee or inferred memory cap. The large observed footprint
keeps client/transport peak memory an explicit remaining resource requirement.

### Direct Node JSON response comparison

Repeating the unchanged 1,000 x 4,096-byte harness at `f3c70dd16` passed all 12
correctness samples. Median results across three isolated samples per workload:

| Client / operation | Previous peak RSS MiB | Direct JSON peak RSS MiB | Previous ms | Direct JSON ms |
|---|---:|---:|---:|---:|
| Sync execute | 217.1 | 129.4 | 1108.1 | 1039.0 |
| Sync profile | 216.3 | 124.6 | 1033.0 | 954.9 |
| Worker execute | 230.8 | 153.9 | 1065.7 | 1045.1 |
| Worker profile | 230.9 | 162.5 | 1174.7 | 991.2 |

Raw results: [direct JSON response run](benchmark-results/2026-09-08-linux-debug-node-direct-json-1000.json).
The prior run above used `47252916f`; both record addon and unchanged harness hashes.
Median process peaks decreased about 30–42%. These are sequential local debug runs,
not randomized release-build trials; peak RSS includes startup/setup/runtime and
JS decoding. Timing differences have only three samples per workload. The result
supports this serialization change while leaving total-memory bounds, larger and
more varied workloads, release timing and platform qualification unfinished.

The follow-up in-place JS decoding run at `060bd5469` also passed all 12 samples:
[raw results](benchmark-results/2026-09-08-linux-debug-node-inplace-1000.json).
Median ms / peak RSS MiB were sync execute 1071.1 / 130.9, sync profile
1088.5 / 127.9, worker execute 998.8 / 157.1 and worker profile 1000.9 / 157.7.
These mixed values do not demonstrate an additional peak-RSS or timing improvement
relative to direct JSON alone. The change avoids a second row-array structure,
but this workload's whole-process peaks and three samples cannot isolate that
allocation saving. The native addon hash is unchanged; the JS wrapper differs.

The result diagnostic now accepts a third argument for the binary fill byte
(0..255, default 0). `node fastdb/scripts/bench-results.cjs 1000 4096 255` passed
all 12 samples; [raw results](benchmark-results/2026-09-08-linux-debug-node-binary255-1000.json).
For sync execute/profile and worker execute/profile respectively, median ms / peak
RSS MiB were 1190.5 / 148.4, 1080.7 / 138.0, 1186.1 / 185.4 and 1166.7 / 199.6.
The logical cell payload remains 4,104,000 bytes. Decimal JSON byte arrays have
content-dependent text size. Nonzero fill uses a hex SQL literal while zero fill
retains zeroblob, so this is broader workload evidence rather than a controlled
attribution of the timing/RSS difference. Logical payload limits are not process
memory limits. The harness hash identifies this extension.

Response-buffer reuse at `a54277854` passed the same 12 nonzero-byte samples:
[raw results](benchmark-results/2026-09-08-linux-debug-node-reuse255-1000.json).
Median ms / peak RSS MiB were sync execute 1171.4 / 137.4, sync profile
1085.3 / 138.8, worker execute 1268.4 / 184.2 and worker profile 1201.9 / 183.4.
Compared with the preceding nonzero run, memory and timing results are mixed;
sync execute and worker profile peaks are lower, but no uniform latency or memory
improvement is established by three samples. Both runs use the same harness and
payload expression. Whole-process RSS and debug-run limitations still apply.


Direct cell writing at `ee79e6789` passed all 12 samples of
`node fastdb/scripts/bench-results.cjs 1000 4096 255`:
[raw results](benchmark-results/2026-09-08-linux-debug-node-writer255-1000.json).
Median ms / peak RSS MiB were sync execute 1223.7 / 137.7, sync profile
1211.5 / 140.0, worker execute 1205.6 / 182.0 and worker profile 1105.4 / 183.0.
The per-cell JSON String and append copy are removed, but this run does not show
a consistent whole-process memory or latency improvement versus response-buffer
reuse alone. Both use the same harness and workload; three sequential debug
samples per operation, runtime/setup memory and allocator variance limit the
comparison. This does not establish a release performance guarantee or memory cap.
