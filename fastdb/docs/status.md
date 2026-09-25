# FastDB implementation status

FastDB and FastQL **2.0.0 are released for Linux x64**. See the
[release record](release-2.0.0.md), [public download receipt](release-2.0.0-download.json),
[completed V2 checklist](v2-tasks.md) and [engine provenance](../UPSTREAM.md).
Cloud v0.2.0 remains separate; it was not changed or deployed by this release.

Release clients are Rust, Node.js/TypeScript, Python, PHP, Swift, C# and Go.
Browser/WASM is removed. macOS, Windows, ARM and Apple mobile are outside V2.
Packages are distributed in the GitHub Linux bundle, not language registries.
The bundle uses the required development build profile with debug symbols stripped;
read the release record for Linux library requirements and performance limits.

## Post-release work

[Nested projection paths](nested-projections.md) add SurrealDB-style wildcard
record fetching plus FastQL dot and negative array indexes. This work is separate
from the published 2.0.0 artifacts.

## Implemented V2 behavior

- [Spatial point/radius indexes](v2-spatial.md) and [H3 cells](v2-cell-design.md).
- [Record brace projections](v2-record-projections.md) and
  [indexed inverse relationships](v2-relations.md).
- [Native full-text search](v2-fulltext.md) and [ANN search](v2-ann.md).
- [Sandboxed JavaScript functions](v2-user-functions.md), including the approved
  correction preserving caller transactions after scalar read errors.
- [Seven native clients](native-language-clients.md), CLI and FastQL editor parity.
  Editor evidence does not imply a new marketplace publication.

## Release verification

Source `4d118ab1f819ff5deb32aa29dfe739ca9b086785` passes hosted scoped CI:
738 Rust tests, 115 Node/application tests, three C ABI tests, formatting, Clippy
and TypeScript. Exact installed packages pass Node 22/24, Python 3.10/3.12/3.14,
PHP/Swift/C#/Go and Go race checks. A standalone Rust consumer passes from the
extracted source archive. V1 upgrade, V2 writes/reopen, downgrade rejection and
both V1/V2 backup restores pass. Public download and all 56 internal checksums
are verified. Preserve a V1 backup before adopting V2 catalog features.

Review [all five maintained core exceptions](core-exceptions.md) on each upstream
sync before retaining, adapting or removing them. The WASI-only exception was
reverted and must not be reapplied for this native release.

## History and future work

[V1 release](release-1.0.0.md), [V1 implementation history](v1-implementation-history.md)
and earlier V2 evidence retain their exact source/artifact scopes. The final
release record supersedes their pending states. Historical browser evidence does
not create a current gate; see [browser removal](browser-removal.md).
Broader graph traversal, advanced geometry, changefeeds/sync and procedural
scripting remain future proposals. Begin further implementation only under a
new requested scope. Preserve unrelated working-tree changes.
