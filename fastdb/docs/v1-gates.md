# Embedded V1 gate review — 2026-09-07

This is a navigation and prioritization aid, not a replacement for the parent FastDB.md and FastQL.md plans. The current implementation is not release-complete. Baseline reviewed: ab4bcb937; scoped evidence remains 271 Rust tests and 30 Node tests with one ignored trigger-cancellation gate. See verification.md for exact runs and limitations.

| Required area | Current evidence | What still prevents a completion claim |
|---|---|---|
| Pinned dialect and grammar | UPSTREAM.md; parser tests; tests/tests/sql_compat.rs and scalar_subqueries.rs | Mixed collection/native query coverage, correlation, remaining aliases/type propagation, managed-name authorization and broader baseline differential qualification |
| Documents, validation and indexes | CRUD, checks, catalog, returning, insert_select and transaction suites | Broader write forms and dependency access; complete concurrent schema/resource qualification |
| Value/query/result contract | contracts.md; types and expression tests; Node lossless value and transfer tests | Complete expression propagation, ordering/index equivalence across supported forms; stable release result/error contract |
| Transactions and cancellation | interrupt.rs regressions; Node operation tokens and queue/close tests | Ignored pinned trigger Interrupt-to-Busy defect; ambiguous/interrupted commit qualification, deadlines and full lifecycle/platform coverage |
| One-hop forward links | tests/tests/links.rs; documented snapshot and batching probes | Fetched-byte/outer-result resource bounds and target-plan instrumentation |
| Exact vectors | tests/tests/vectors.rs; benchmark reports | Broader numerical/platform qualification and representative high-dimensional 100k–1m evaluation, without a promised latency target |
| Bundled QuickJS functions | Fixed slugify/normalize implementation and function tests | Runtime/platform/resource/performance release qualification; user JS remains outside V1 |
| Recovery, backup and upgrades | Process-kill tests, offline restore rehearsal, legacy catalog fixtures | Interrupted I/O/commit/checkpoint matrix, previous released-binary upgrade/restore and advertised-platform evidence |
| CLI and developer tools | Script/terminal/inspection/migration/transfer tests | Row streaming, total resource accounting, terminal/platform and complete tool qualification |
| Rust and Node distribution | Offline Rust consumer; offline Node tarball consumer and declarations | Release artifacts/notices, platform/Node matrix, prebuild selection, registry readiness; no publishing has occurred |
| Application validation | Master-plan requirement | An initial task-tracker template is tested under examples/node-task-tracker; external developer/pilot evidence and broader tested agent-facing guidance remain missing |

Cloud beta requirements remain deferred until after embedded V1. They do not block embedded implementation. V2 inverse links, indexed ANN/FTS/spatial and user scripts are not substitutes for unfinished V1 work.

## Native membership gap and follow-up

A live probe against the current native addon produced:

```sql
CREATE TABLE docs;
INSERT INTO docs(n) VALUES (1),(2),(NULL);
CREATE TABLE native(n INTEGER);
INSERT INTO native VALUES (1),(NULL);
SELECT n,n IN (SELECT n FROM native) FROM native;
-- succeeds: (1,1), (NULL,NULL)
SELECT n,n IN (SELECT n FROM native) FROM docs;
-- FDB_UNSUPPORTED: this collection expression is not implemented for collections
```

The collection result should be qualified against an ordinary-table oracle containing the same left-hand values. The implementation prepass in frontend/src/select.rs caches native scalar/EXISTS sources but deliberately excludes native membership sources. Extending that route must preserve RHS affinity/collation, NULL/NOT IN behavior, raw binary versus typed record identity, parameter validation and single-source evaluation. Simply encoding all RHS values through a function would lose native column affinity. Tests should cover both reads and atomic collection INSERT SELECT with retry/rollback before marking the gap handled.

Initial implementation now handles the scalar native-membership cases described in status.md. Follow-up priority is broader operand/CTE/correlation and evaluation-count qualification, followed by remaining write/type semantics and resource accounting. Existing cancellation and packaging evidence should be reused unless a change affects it. The core trigger proposal remains separately reviewable under the project workflow; it is not applied by this audit.

The reproducer above records the pre-change failure. Initial native membership lowering and differential/read-write tests now exist; the wider semantic and performance requirements remain open.
