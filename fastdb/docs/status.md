# FastDB implementation status

FastDB and FastQL V1 are released. See [release 1.0.0](release-1.0.0.md),
[behavior history](contracts.md), and [engine provenance](../UPSTREAM.md).
Cloud v0.2.0 is separate and outside this workstream.

## Current release scope

Release embedded FastDB V2, V2 spatial and FastQL V2 with native **Rust,
Node.js/TypeScript, Python, PHP, Swift, C# and Go** clients. The user explicitly clarified that
browser/WASM is not needed for this release. Browser code, WASI build/probe wiring and the WASI-only core exception have
been removed. Historical evidence is retained; its unfinished browser gates
must not block native V2. See [the removal record](browser-removal.md). Follow [the current checklist](v2-tasks.md).
The four additional [PHP, Swift, C# and Go clients](native-language-clients.md)
now share a native C ABI with Linux development qualification. Final versioned
packages and platform qualification remain open. The user confirmed Linux x64
as the V2 binary platform; macOS and Windows are outside this release.
No V2 artifact has been published.

Implemented and qualified under the linked contracts:

- [Spatial point/radius indexes](v2-spatial.md) and [H3 cells](v2-cell-design.md).
- [Record brace projections](v2-record-projections.md) and
  [indexed inverse relationships](v2-relations.md).
- [Native full-text search](v2-fulltext.md) and [ANN search](v2-ann.md).
- [Sandboxed JavaScript functions](v2-user-functions.md), including the approved
  scalar-read transaction fix. Managed failures restore statement changes and
  retain earlier caller work. This correction differs from V1's scalar-error
  transaction-wide rollback behavior; inspect transaction reports.

## Current evidence and remaining release work

[Combined integrated acceptance](v2-core-integration-evidence.md) passes **738
Rust tests and 115 Node/application tests**, zero failures/ignored/skipped, plus
formatting, Clippy and TypeScript. Scalar-error core commit `f26014f04` and native
FTS/WAL fixes are integrated; the optional WASI feature commit `2ef619c07` was
reverted in `ae6777a17`. Review [all core exceptions](core-exceptions.md)
on every upstream sync before retaining, updating or removing local patches.

The rebuilt [Python wheel](v2-python-scalar-evidence.md) passes eight installed
checks on CPython 3.10, 3.12 and 3.14, including failed/cancelled JavaScript,
nested savepoints, FTS integrity/drop and persistence. It remains a local
Linux x86_64 development artifact.

[Node installed-package checks](v2-node-package-evidence.md) cover Node 22/24;
[standalone Rust checks](v2-rust-client-evidence.md) exercise direct V2 APIs.
Those earlier artifacts predate the latest scalar fix and must be refreshed for
final delivery. [V1 upgrade/restore](v2-upgrade-restore-evidence.md) passes after
the approved FTS backing-storage fix. Final artifacts must repeat that rehearsal.

[FastQL editor evidence](v2-language-tooling-evidence.md) covers catalog/formatter
parity and an installed VSIX. [Notice evidence](v2-notice-evidence.md) records
collected texts and unresolved attribution. Finish native artifact versions,
supported-platform qualification, complete notices, release records and delivery.
V2-C and V2-R remain open; do not publish development artifacts as the final release.

## Retained history

The [V1 implementation log](v1-implementation-history.md) and individual V2
milestone documents retain their exact source/artifact scopes. Browser source is archived outside the checkout; historical evidence is
retained in [browser evidence](v2-browser-client.md) and
[OPFS evidence](v2-browser-opfs.md). Its unexplained shutdown timeout remains a
parked browser issue. Do not infer an active release requirement from historical
checklists or pending-state descriptions. Preserve unrelated working-tree changes.
