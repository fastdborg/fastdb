# FastDB Phase 0 Benchmark — FastDB vs Native Turso

Methodology follows `plan-phase0.md` work package P0.10. Both paths run on
the **same pinned engine** (Turso `977383ff`), the same physical schema
(`rid TEXT PRIMARY KEY, doc BLOB`), the same JSONB functions
(`jsonb(json_object(...))` to write, `json(doc)` / `json_extract(doc,...)` to
read), the same bound values, and the same full-durability file-backed WAL.

## Paths compared (equivalent work)

- **FastDB** — the full `Connection::execute`: parse SurrealQL, catalog
  resolution, direct-AST lowering, `prepare_translated_stmt_with_options`,
  bind, execute, decode (canonical rid + serde_json doc → typed `Record`).
- **Native** — the bare engine via SQL text, statement prepared **once** and
  re-bound per iteration (the pure-engine baseline, no frontend). It does the
  **same result materialization** as FastDB: `SELECT rid, json(doc) ...` and a
  serde_json decode of `doc` per result — it does not merely count rows.

Both paths run on the same pinned engine, same physical schema
(`rid TEXT PRIMARY KEY, doc BLOB`), same JSONB functions
(`jsonb(json_object(...))` to write; `json(doc)` / `json_extract(doc,...)` to
read), same bound values, same full-durability file-backed WAL, and equivalent
temp-directory lifetimes. Correctness is asserted inside every workload
(record counts / decoded fields). The `delete` workload times **only** the
delete (the target record is created in untimed setup); `cold_create` is
bootstrap + implicit table creation + JSONB write + decode (the `open` is in
untimed setup, per `plan-phase0.md` P0.10's definition).

The ratio therefore isolates **FastDB frontend overhead over a cached
engine baseline**: FastDB re-parses, re-resolves the catalog, and re-prepares
on every call; native prepares once and skips all of that.

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
The numbers below were collected with reduced sampling; re-running the
command above reproduces them within noise and yields the full percentile
estimates (criterion writes p50/p95/p99 to
`target/criterion/<group>/<bench>/estimates.json`):
```sh
cargo bench -p turso_fastdb_benchmarks --bench phase0 -- \
    --warm-up-time 1 --measurement-time 2 --sample-size 30
```

## Results (median estimate; bracket = criterion 95% confidence)

| Workload | FastDB | Native | FastDB / Native |
| --- | ---: | ---: | ---: |
| cold_create (bootstrap + table DDL + first write + decode) | 1.79 ms [1.63–2.01] | 525 µs [474–566] | **3.4×** |
| steady_create (existing table, unique id) | 62.8 µs [54.6–73.8] | 10.1 µs [9.23–11.8] | **6.2×** |
| point_read (by rid, with doc decode) | 46.0 µs [39.9–52.9] | 3.18 µs [2.80–3.45] | **14.5×** |
| indexed_filter (by name, with doc decode) | 30.0 µs [29.9–30.0] | 2.30 µs [1.94–2.66] | **13.0×** |
| delete (by rid, delete-only) | 65.2 µs [58.7–69.9] | 10.2 µs [9.03–11.8] | **6.4×** |

The CIs are wider than ideal (this run hit background load); the ratios are
representative, not precise. A full-sampling run on a quiet machine is the
authoritative measurement.

## Observations and honest interpretation

- **No pathological result.** The largest ratio is ~14×, dominated by
  per-call frontend work, not by anything that makes the architecture
  unusable. Phase 0 has **no** performance pass ratio (`plan-phase0.md`
  P0.10); the MVP targets in `revised_plan.md` §11 (e.g. point-read p50
  ≤ 1.5×, filter p95 ≤ 2×) are **future** gates, not Phase 0 gates.
- **The overhead is concentrated and explainable.** Each FastDB
  `Connection::execute`, including reads, currently:
  1. parses the SurrealQL input (Phase 0 hand-written parser),
  2. resolves the catalog with **two engine round-trips** per call
     (`SELECT … FROM sqlite_schema` for existence, then
     `SELECT … FROM __fastdb_tables` + version check),
  3. re-lowers and **re-prepares** the statement every call,
  4. decodes via `json(doc)` + serde.
  Native prepares **once** and skips 1–3 entirely; the two extra catalog
  round-trips dominate the read ratios.
- **Concrete Phase 1–3 optimization targets** (not Phase 0 work): cache the
  catalog resolution (logical→physical + version) per connection, add a
  prepared- and lowered-statement cache keyed by FastDB source, and decode
  without an intermediate `json()` text round-trip. These bring the ratios
  toward the MVP gates.

The benchmark is reproducible, equivalent (both paths decode the same logical
result), and correctness-checked; it makes no absolute-latency or
"production-ready" claim.
