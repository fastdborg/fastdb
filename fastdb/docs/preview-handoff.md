# FastDB V1 preview handoff

This is the local Linux x64 evaluation candidate. It is not a production/stable
V1 release and has not been uploaded or published to a registry.

## Candidate

- Directory: `dist/fastdb-preview-linux-x64/`
- Source: `dcce6c4f09e98cd4173b54d7bcf8af0a10911436`
- Entry point: candidate `README.md` (CLI, Node tracker, export/restore and Rust)
- Identity and platform: `manifest.json`
- Integrity: `SHA256SUMS`; run `sha256sum -c SHA256SUMS` in the candidate directory
- Retained evidence: candidate `evidence/`

The candidate includes the CLI, native Node tarball, pinned source archive,
application example, licenses/notices and dependency inventories. Node 22.0.0
and 24.19.0 installed the exact retained tarball successfully. Rust 1.88.0's
standalone consumer passed with 244 dependency identities matching the lockfile.
The quickstart ran from a fresh directory, including persistent reopen and
application export/restore.

## What the preview delivers

Typed document collections beside relational SQL; validation and managed indexes;
transactions and typed results; one-hop forward links; exact vectors and bundled
string functions; migrations and JSON/NDJSON transfer; CLI and Rust/Node APIs.
The tracker demonstrates the complete application path and atomic restoration
of linked documents plus relational events into a fresh database.

See [application/storage acceptance](preview-storage-evidence.md),
[candidate delivery](preview-delivery-evidence.md) and the
[finite checklist](preview-release.md).

## Known limitations

- Debug binaries for functionality evaluation, not production performance.
  Tested on Linux x64 under WSL2 with glibc 2.39; other distributions/platforms
  are not qualified.
- Cancellation of trigger-bearing writes is unsupported. The pinned engine
  converts interruption to Busy in one known path; the associated test is
  explicitly ignored. Ordinary tracker transactions do not use triggers.
- Unsupported collection query forms, including ON CONFLICT and some joined
  UPDATE/index-hint forms, remain documented limitations.
- Results and snapshots materialize; available limits are not a total memory
  cap. No power-loss or untested binary-upgrade guarantee is made.
- No public npm/crates.io package, cloud, sync, inverse links, indexed ANN/FTS/
  spatial search or user JavaScript.

Preview acceptance does not declare the full stable-V1 goal complete. The next
product decision is where to distribute this candidate and collect application
feedback; speculative SQL combinations do not reopen the preview checklist.

## Final acceptance

P0–P6 are complete for this preview. The final FastDB-scoped check passed with
673 Rust tests and 101 Node/application tests; formatting, Clippy and strict
TypeScript passed. One known trigger-cancellation test remains ignored as
classified above. The tested checkout was `1b6b66d40`, whose only differences
from candidate source `dcce6c4f0` were delivery documentation. The implementation,
lockfile and test scripts match. Full output is retained as
`evidence/final-scoped-check.log` in the candidate.
