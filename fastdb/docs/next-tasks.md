# Next tasks toward stable embedded V1

The preview checklist P0–P6 is closed. This list tracks the next milestones;
it does not reopen preview acceptance or replace the parent FastDB.md scope.

| ID | Task | Done when | State |
|---|---|---|---|
| N1 | Ship preview.2 with automatic collections | Updated assets pass exact-package checks and are published with source identity and checksums | Complete: [release](https://github.com/fastdborg/fastdb/releases/tag/fastdb-v0.1.0-preview.2) |
| N2 | Integrate one real Node application | A selected application uses FastDB for one complete persistent workflow, with restart and transaction/error handling checked | Awaiting application path/repository |
| N3 | Resolve application blockers | Reproduced blockers from N2 are fixed with focused regressions; nonblocking limitations are documented | After N2 |
| N4 | Freeze stable V1 behavior | A concise supported contract covers SQL/documents, validation/indexes, values, transactions, links, bundled functions/vectors, tools and clients; each remaining scope gap is named | Open |
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
