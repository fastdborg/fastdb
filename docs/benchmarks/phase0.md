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
  re-bound per iteration (the cached pure-engine baseline, no frontend). It
  uses the same canonical rid encoding, `SELECT rid, json(doc) ...`, rid/doc
  decoders, typed `Record`, and empty delete result as FastDB. It does not
  merely count rows.

Both paths run on the same pinned engine, same physical schema
(`rid TEXT PRIMARY KEY, doc BLOB`), same JSONB functions
(`jsonb(json_object(...))` to write; `json(doc)` / `json_extract(doc,...)` to
read), same bound values, same full-durability file-backed WAL, and equivalent
temp-directory lifetimes. Correctness is asserted inside every workload, and
the last deleted record is checked outside the timed region. The `delete`
workload times **only** the delete (the target record is created in untimed
setup); `cold_create` is bootstrap + implicit table creation + JSONB write +
typed result construction (the open is untimed, per P0.10).

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
The results below are from the full default-sampling command above: 3-second
warm-up, 100 samples, and Criterion's automatically selected measurement
duration. Criterion 0.5 records mean, median, slope, and confidence intervals,
but not p95/p99 latency percentiles; Phase 0 therefore reports median (p50) and
explicitly leaves tail-latency measurement to the later process-level harness.

## Results (median/p50 estimate; bracket = 95% confidence interval)

| Workload | FastDB | Native | FastDB / Native |
| --- | ---: | ---: | ---: |
| cold_create (bootstrap + table DDL + first write + typed result) | 1.497 ms [1.473–1.512] | 369.4 µs [362.7–383.7] | **4.1×** |
| steady_create (existing table, unique id, typed result) | 48.91 µs [48.69–48.99] | 9.694 µs [9.651–9.735] | **5.0×** |
| point_read (canonical rid + doc → typed record) | 29.60 µs [29.58–29.67] | 1.995 µs [1.954–2.002] | **14.8×** |
| indexed_filter (canonical rid + doc → typed record) | 30.06 µs [29.98–30.16] | 2.058 µs [2.056–2.059] | **14.6×** |
| delete (by canonical rid, delete-only) | 36.05 µs [35.78–36.28] | 7.810 µs [7.746–7.863] | **4.6×** |

## Observations and honest interpretation

- **The feasibility spike passes, but the current uncached frontend is far
  outside the future MVP gates.** The largest measured median ratio is 14.8×.
  Phase 0 has no pass ratio (`plan-phase0.md` P0.10), while the 1.5–2× targets
  in `revised_plan.md` §11 remain release gates that require profiling and
  caching work in Phases 1–3.
- **The overhead is concentrated and explainable.** Each FastDB
  `Connection::execute`, including reads, currently:
  1. parses the SurrealQL input (Phase 0 hand-written parser),
  2. performs **three engine round-trips** per existing-table read
     (`sqlite_schema` existence, metadata versions, then logical→physical
     catalog resolution),
  3. re-lowers and **re-prepares** the statement every call,
  4. decodes via `json(doc)` + serde.
  Native prepares **once** and skips 1–3 entirely; the catalog/version
  round-trips dominate the read ratios.
- **Concrete Phase 1–3 optimization targets** (not Phase 0 work): cache the
  catalog resolution (logical→physical + version) per connection, add a
  prepared- and lowered-statement cache keyed by FastDB source, and decode
  without an intermediate `json()` text round-trip. These bring the ratios
  toward the MVP gates.

The benchmark is reproducible, materially equivalent (including typed result
construction), and correctness-checked. It makes no tail-latency,
"production-ready", or competitive-performance claim.
