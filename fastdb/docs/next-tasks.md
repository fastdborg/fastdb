# Next tasks toward stable embedded V1

The preview checklist P0–P6 is closed. This list tracks the next milestones;
it does not reopen preview acceptance or replace the parent FastDB.md scope.

| ID | Task | Done when | State |
|---|---|---|---|
| N1 | Ship preview.2 with automatic collections | Updated assets pass exact-package checks and are published with source identity and checksums | Complete: [release](https://github.com/fastdborg/fastdb/releases/tag/fastdb-v0.1.0-preview.2) |
| N2 | Deliver a typed document SDK | Collection CRUD methods work in both Node clients, with TypeScript examples and transaction/error regression checks | Complete: [SDK contract](node-sdk.md), both-client CRUD regression and strict TypeScript checks |
| N3 | Verify SDK consumption and resolve usage blockers | Installed packages expose collection CRUD and strict TypeScript declarations on supported Node versions; reproduced blockers are resolved | Complete for the delivered SDK: Node 22/24 package checks passed; further actual bugs remain actionable |
| N4 | Freeze stable V1 behavior | A concise supported contract covers the full embedded scope and resolves its named remaining gaps | [Target contract and finite requirements](v1-release-contract.md) written; S1/S3 scope reconciliation remains open; S5 resource scope is reconciled |
| N5 | Close stable release evidence and ship | Required master-plan evidence below is linked to a fixed candidate, remaining failures are resolved, and supported artifacts are published | Open |

N5 evidence covers the master plan's seven required areas: pinned SQL differential
checks; document/index consistency; null/missing/reference/serialization semantics;
crash and interrupted commit/checkpoint behavior; backup and version upgrades;
execution limits and client packaging; document/vector latency, memory, scanned
records and index use (including the desired 100k–1m vector evaluation without
a performance promise). Use existing evidence where it applies. Each missing
item gets a concrete bounded task before execution, not a generic qualification loop.

Application validation follows the master plan: one integration starts the work;
it does not stand in for the three external pilots and approximately ten target
developers. Outreach requires explicit authorization. Cloud remains deferred.

During a milestone, use focused tests. Run the full scoped suite at its code
acceptance boundary; package/platform checks only for affected artifacts.
Do not add speculative query combinations or platforms. Record actual application
limitations and fix those that block the selected workflow.

N5 progress: [preview.1 → preview.2 released-binary upgrade rehearsal](preview-upgrade-evidence.md) passed.

User direction: no existing application is available. N2 now develops the SDK using SurrealDB as a reference; external application recruitment is not required to start this work.
