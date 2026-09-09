# Collection UPDATE FROM implementation design

Status: initial single-source, inner-join and left-join collection UPDATE FROM is implemented,
with encoded typed-ID duplicate resolution and initial LIMIT/OFFSET support after
deduplication. A leading RIGHT ON join is normalized to a reversed LEFT ON join,
including when followed by inner or left joins. USING/NATURAL/FULL and non-leading
RIGHT source joins remain explicitly rejected. Ordinary relational UPDATE FROM delegates to the
pinned engine. The remaining design below requires no engine changes or new
storage format.

The native oracle in `tests/tests/writes.rs` exercises a primary-key target and
an unindexed source with duplicate matches. Reversing source insertion order
changes the chosen assignment; in that plan the last match wins. Each target
is changed once, and LIMIT 1 still chooses the final assignment for that target.
This is plan-specific evidence, not a portable deterministic winner promise.

## Candidate construction

Extend `write_candidates` with an explicit source description containing WITH
and FROM, rather than another positional argument. Retain the current path for
writes without FROM. Project the target document explicitly using its exposed
alias; do not use an unqualified star once joined sources exist. Follow that
snapshot with the existing assignment projections, retaining their typed values
and direct-parameter validation.

Keep the original FROM tree grouped so its JOIN ON/USING scope is preserved.
Combine it with the actual update target using the same exposed target name as
the original UPDATE. Resolve self-joins and target-named CTEs using write-context
binding; do not blanket-rename source references. Existing CTE flattening and
correlation logic needs differential checks in this new context.

Joined updates resolve source CTEs before exposing the write target. A CTE named
like the target table or alias therefore retains its source binding. The native
write-context rebinding used for UPDATE/DELETE without FROM must not apply to
joined updates. Differential fixtures cover chained CTEs, materialization hints,
target aliases, self-sources and pagination; self-source assignments read the
original candidate snapshots before mutation.

Materialize the candidate query before any mutation, with the existing candidate
row and byte budgets. Resolve repeated targets using encoded typed record IDs
(`Value::Record(...).encode()`), never a stringified key that conflates integer
and string IDs. Retain one original snapshot and the chosen assignment tuple per
target. A later source match replaces assignments, rather than applying another
mutation to a previously changed document. The distinct-target buffer and lookup
map also need explicit resource accounting.

Apply LIMIT/OFFSET after duplicate resolution. Preserve parameter validation and
native scalar coercion; do not add a second ad hoc numeric parser. Evaluate
pagination expressions once in their valid SQL scope, including empty-source
behavior. Only then pass candidates to the current validation/index/savepoint
loop and produce RETURNING. Limit zero and runtime failures must preserve the
existing transaction-report behavior.

The current pagination position-list builder and candidate-selection loop poll
the engine progress handler once per candidate, including rows discarded by
OFFSET. This extends cooperative cancellation into these host phases. String
allocation/copying, individual value destruction and remaining-buffer cleanup
are not hard-deadline bounded; total workspace accounting remains open.

## Required verification

- Native/collection source and target comparisons with unique and duplicate
  matches, reversed insertion order, source indexes and explicit aliases.
- One mutation and one RETURNING row per target; unmatched targets unchanged;
  original-snapshot evaluation for scalar and tuple assignments.
- LIMIT/OFFSET after duplicate resolution, bound expressions, zero/empty cases,
  missing bindings and invalid pagination values.
- Local/chained/materialized CTEs, self-joins, target/alias collisions, grouped
  source joins, derived sources and correlated assignment subqueries.
- Typed record/object/array/binary values, integer versus string record keys,
  CHECK/unique failures, exact index restoration, valid retry and rollback.
- Candidate and deduplication buffer limits; cancellation before and during
  materialization and mutation; no partial result advertised as success.
- Both Node clients, standalone Rust consumer and persistent reopen coverage,
  followed by the full scoped check.

Do not enable FROM merely by forwarding raw joined rows to the existing mutation
loop: that would update a target repeatedly and paginate source matches instead
of targets. If planner variations expose different native duplicate selection,
record that evidence and resolve the compatibility policy before claiming parity.


## UPDATE FROM pagination evaluation oracle — 2026-09-09

A native regression verifies an overflowing assignment in a later joined
candidate fails UPDATE FROM even with LIMIT 0 or LIMIT 1, preserving target
rows. LIMIT 2 produces the same error. This confirms the tested pinned path
materializes assignment candidates before pagination, unlike ordinary limited
UPDATE; applying the limit before candidate evaluation would change behavior.
All 45 write tests passed; formatting and all-target fastdb-tests Clippy with
warnings denied passed. Logs: `/tmp/fastdb-update-from-limit-evaluation.log`,
`/tmp/fastdb-from-limit-evaluation-test.log` and
`/tmp/fastdb-from-limit-evaluation-clippy.log`. Only tests/docs changed from
`1f606bcd6`; collection FROM pagination is still unimplemented. Full V1 remains open.
