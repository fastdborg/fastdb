# Embedded V1 gate review — 2026-09-07

This is a navigation and prioritization aid, not a replacement for the parent FastDB.md and FastQL.md plans. The current implementation is not release-complete. Latest scoped evidence: 301 passing Rust tests with one ignored trigger-cancellation gate, 35 passing Node tests, formatting, Clippy and strict TypeScript. Installed-package evidence is recorded separately. See verification.md for exact runs and limitations.

| Required area | Current evidence | What still prevents a completion claim |
|---|---|---|
| Pinned dialect and grammar | UPSTREAM.md; parser tests; tests/tests/sql_compat.rs and scalar_subqueries.rs | Mixed collection/native query coverage, correlation, remaining aliases/type propagation, managed-name authorization and broader baseline differential qualification |
| Documents, validation and indexes | CRUD, checks, catalog, returning, insert_select and transaction suites | Broader write forms and dependency access; complete concurrent schema/resource qualification |
| Value/query/result contract | contracts.md; types and expression tests; Node lossless value and transfer tests | Complete expression propagation, ordering/index equivalence across supported forms; stable release result/error contract |
| Transactions and cancellation | interrupt.rs regressions; Node operation tokens and queue/close tests | Ignored pinned trigger Interrupt-to-Busy defect; ambiguous/interrupted commit qualification, deadlines and full lifecycle/platform coverage |
| One-hop forward links | tests/tests/links.rs; documented snapshot and batching probes | Initial per-resolver encoded-value bounds exist; target batch/row/VM counters exist; total query/outer-result memory accounting and detailed target plans remain open |
| Exact vectors | tests/tests/vectors.rs; benchmark reports | Broader numerical/platform qualification and representative high-dimensional 100k–1m evaluation, without a promised latency target |
| Bundled QuickJS functions | Fixed slugify/normalize implementation and function tests | Runtime/platform/resource/performance release qualification; user JS remains outside V1 |
| Recovery, backup and upgrades | Process-kill tests, offline restore rehearsal, legacy catalog fixtures | Interrupted I/O/commit/checkpoint matrix, previous released-binary upgrade/restore and advertised-platform evidence |
| CLI and developer tools | Script/terminal/inspection/migration/transfer tests | Row streaming, total resource accounting, terminal/platform and complete tool qualification |
| Rust and Node distribution | Offline Rust consumer; offline Node tarball consumer and declarations | Release artifacts/notices, platform/Node matrix, prebuild selection, registry readiness; no publishing has occurred |
| Application validation | Master-plan requirement | An initial task-tracker template and ai-application-guide.md workflow are tested; external developer/pilot evidence and broader application-pattern qualification remain missing |

Cloud beta requirements remain deferred until after embedded V1. They do not block embedded implementation. V2 inverse links, indexed ANN/FTS/spatial and user scripts are not substitutes for unfinished V1 work.

## Current SQL probes and implementation priorities

Probed the locally built addon after 41a2daf95 using matching `docs` collection and `native(n INTEGER)` rows 1, 2, 3. Each probe ran inside BEGIN/ROLLBACK, so UPDATE probes did not affect later comparisons.

| Probe | Ordinary table | Collection |
|---|---|---|
| Correlated scalar below | (1,NULL), (2,1), (3,2) | FDB_ENGINE: no such table: d |
| Leading-WITH UPDATE below | RETURNING 12; affected 1 | FDB_UNSUPPORTED: this collection UPDATE clause |
| `SELECT n%2 AS parity,count(*) AS total FROM source GROUP BY parity HAVING total>1 ORDER BY parity` | (1,2) | (1,2) |

```sql
SELECT d.n,(SELECT max(n) FROM native WHERE n<d.n) AS prior
FROM docs d ORDER BY d.n;

WITH chosen AS (SELECT 2 AS n)
UPDATE docs SET n=n+10 WHERE n IN (SELECT n FROM chosen) RETURNING n;
```

Replace `docs` with `native` for the ordinary-table forms. These probes are narrow current behavior evidence, not complete grouping or correlation qualification.

The leading-WITH rejection above is a pre-change probe. Initial collection UPDATE/DELETE support now carries CTE scope into candidate SELECT lowering and preserves pre-mutation materialization and atomic writes. Tests cover native/collection CTE membership, self-reads, RETURNING, uniqueness failure/retry/rollback and mutation cancellation. Remaining work includes same-name CTE resolution (the chained target-name fixture fails preparation), broader assignments/RETURNING, validation/type coverage and other write clauses. Correlated scalar lowering remains a separate gap.

The original correlated scalar probe above now passes for native inner SELECT predicates: metadata preparation substitutes outer references in a disposable probe, and runtime predicates lower qualified outer fields. Scalar/EXISTS WHERE and JOIN ON forms have initial coverage; inner collection sources, correlated IN, deeper scopes and non-predicate correlation remain open. Preserve correlation inside engine execution rather than pre-executing a correlated source once or caching it as an uncorrelated value. Test per-row native oracles, NULL/empty sources, alias shadowing, type/affinity/collation, parameter use and atomic writes.

Recent resource work adds incremental transfer parsing/encoding, incremental lowered SELECT decoding, fetch reference/encoded-byte budgets, and target batch/row/VM profiling. Public results still materialize; these are not complete memory budgets. Further resource work remains necessary but does not replace unfinished SQL semantics.

The earlier native membership reproducer is now covered by implemented lowering and differential/read-write/CTE tests. Reuse that evidence; do not treat its old failure as current behavior. The core trigger proposal remains separately reviewable under the workflow and has not been applied. Full embedded scope, packaging/recovery/platform gates and external application validation remain intact.


## Same-name CTE write oracle

A current pinned native regression uses `native(n INTEGER)` rows 1,2,3 and the prefix `WITH native AS (SELECT 2 AS n), chosen AS (SELECT n FROM native)`. A qualified candidate `SELECT n FROM main.native WHERE n IN (SELECT n FROM chosen)` returns only 2. The corresponding UPDATE increments all three rows to 11,12,13, and DELETE returns all three original rows. Both writes affect 3. This is measured pinned behavior; it is not general SQLite shadowing semantics.

Consequently, making the existing collection candidate SELECT prepare successfully is insufficient: it could silently select a different write set. The next same-name fix must preserve the native write-context binding when lowering to candidates, and qualify aliases, nested scopes and parameters. Current collection chained same-name forms still fail preparation. The related native SELECT guard gap now has initial CTE declaration/unqualified-FROM redaction, with simple/chained/derived-source tests. Qualified expression and deeper scope coverage remains open.


Alias qualification extends the oracle: with `UPDATE native AS target`, a CTE named `native` yields only 12, while a CTE named `target` yields 11,12,13. The collision follows the exposed target identifier, not merely the physical table name. A derived wrapper around the candidate SELECT still yields only 2 and is not a solution.

Relevant pinned engine code: core/translate/update.rs builds target_table with tbl_name.identifier(), plans FROM/CTE references separately, then prepends the target into read_scope_tables; core/translate/delete.rs adds the target before plan_ctes_as_outer_refs; core/translate/planner.rs retains CTE ASTs for replanning and resolves current CTE definitions separately from outer references. A frontend fix must preserve this lookup context and avoid blanket replacement of a target-named CTE: aliases change which name collides, and original CTE validation/parameter behavior must remain accounted for. No core changes were made.


The predicate-correlation regression matrix additionally checks native scalar affinity and pinned correlated-result collation behavior. This advances the dialect gate without closing general correlation or the other embedded release gates. See status.md and verification.md for current test counts.
