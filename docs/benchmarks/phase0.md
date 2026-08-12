# FastDB Phase 0 Benchmark — FastDB vs Native Turso

Methodology follows `plan-phase0.md` work package P0.10. Both paths run on
the **same pinned engine** (Turso `977383ff`), the same physical schema
(`rid TEXT PRIMARY KEY, doc BLOB`), the same JSONB functions
(`jsonb(json_object(...))` to write, `json(doc)` / `json_extract(doc,...)` to
read), the same bound values, and the same full-durability file-backed WAL.

## Paths compared

- **FastDB** — the full `Connection::execute`: parse SurrealQL, catalog
  resolution, direct-AST lowering, `prepare_translated_stmt_with_options`,
  bind, execute, decode.
- **Native** — the bare engine via SQL text, statement prepared **once** and
  re-bound per iteration (the pure-engine baseline, no frontend).

The ratio therefore isolates FastDB frontend overhead over the engine.
Correctness is asserted before and after every workload (`assert_eq!` on
record counts / decoded content).

## Environment

| Item | Value |
| --- | --- |
| Turso SHA | `977383ff40edc44ef410af062ed0d2322252a869` |
| Rust toolchain | `1.88` (`rust-toolchain.toml`), `--release` |
| OS | Linux 7.0.0-29-generic x64 |
| CPU | 14 cores |
| Harness | `criterion 0.5`, file-backed tempdir per workload |

## Command

Full run (default sampling):
```sh
cargo bench -p turso_fastdb_benchmarks --bench phase0
```
The numbers below were collected with reduced sampling to fit this report;
re-running the command above reproduces them within noise and yields the
full percentile estimates (criterion writes p50/p95/p99 to
`target/criterion/<group>/<bench>/estimates.json`):
```sh
cargo bench -p turso_fastdb_benchmarks --bench phase0 -- \
    --warm-up-time 1 --measurement-time 2 --sample-size 30
```

## Results (median estimate; bracket = criterion 95% confidence)

| Workload | FastDB | Native | FastDB / Native |
| --- | ---: | ---: | ---: |
| cold_create (open + bootstrap/DDL + first write + decode) | 1.57 ms [1.53–1.61] | 305 µs [298–317] | **5.1×** |
| steady_create (existing table, unique id) | 45.3 µs [44.8–45.8] | 9.41 µs [9.38–9.44] | **4.8×** |
| point_read (by rid) | 22.5 µs [22.4–22.7] | 1.78 µs [1.77–1.80] | **12.6×** |
| indexed_filter (by name) | 23.3 µs [23.2–23.5] | 1.81 µs [1.81–1.81] | **12.9×** |
| delete (by rid) | 114.8 µs [113.3–116.8] | 26.5 µs [26.3–26.8] | **4.3×** |

## Observations and honest interpretation

- **No pathological result.** The largest ratio is ~13×, well short of
  anything that makes the architecture unusable. Phase 0 has no performance
  pass ratio (`plan-phase0.md` P0.10); the MVP targets in `revised_plan.md`
  §11 (e.g. point-read p50 ≤ 1.5×) are **future** gates, not Phase 0 gates.
- **The overhead is concentrated and explainable.** Each FastDB
  `Connection::execute`, including reads, currently:
  1. parses the SurrealQL input (Phase 0 hand-written parser),
  2. resolves the catalog with **two engine round-trips** per call
     (`SELECT … FROM sqlite_schema` for existence, then
     `SELECT … FROM __fastdb_tables` for the logical→physical name),
  3. re-lowers and **re-prepares** the statement every call,
  4. decodes via `json(doc)` + serde.
  The native baseline prepares **once** and skips steps 1–3 entirely. The
  two extra catalog round-trips per read dominate the point-read / filter
  ratios.
- **Concrete Phase 1–3 optimization targets** (not Phase 0 work): cache the
  catalog resolution (logical→physical) per connection, add a prepared- and
  lowered-statement cache keyed by FastDB source, and decode without an
  intermediate `json()` text round-trip. These are exactly the wins that
  bring the ratios toward the MVP gates.

The benchmark is reproducible, equivalent, and correctness-checked; it does
not make any absolute-latency or "production-ready" claim.
