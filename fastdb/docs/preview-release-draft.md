# FastDB 0.1.0 — embedded V1 preview

An unpublished [GitHub draft prerelease](https://github.com/fastdborg/fastdb/releases/tag/untagged-2d5e3413f99424d2484d)
now contains both assets below. Its intended tag is `fastdb-v0.1.0-preview.1`.
GitHub reports the archive uploaded with the SHA-256 recorded below.
The release target must be the artifact source commit
`dcce6c4f09e98cd4173b54d7bcf8af0a10911436`, not a later documentation commit.

FastDB combines typed document collections with the pinned Turso SQL engine.
This preview is ready for local application evaluation on the tested Linux x64
host. It includes a CLI, native Node.js/TypeScript client, pinned Rust source,
and a task-tracker example with migrations, linked records, transactions and
atomic application export/restore.

## Assets to attach

- `fastdb-preview-0.1.0-linux-x64.tar.gz` — candidate bundle (approximately 128 MiB)
- `fastdb-preview-0.1.0-linux-x64.tar.gz.sha256` — adjacent archive checksum

Both files are retained in the checkout's `dist/` directory. Archive SHA-256:

```text
467a4f90a0c9bd3d603f8c28cbbe5222ac0dbfea7f2d01b2a922a92852147e8c
```

## Try the preview

After downloading both assets into the same directory:

```sh
sha256sum -c fastdb-preview-0.1.0-linux-x64.tar.gz.sha256
tar -xzf fastdb-preview-0.1.0-linux-x64.tar.gz
cd fastdb-preview-linux-x64
sha256sum -c SHA256SUMS
npm install --no-audit --no-fund --no-save ./fastdb-node-0.1.0.tgz
node tracker.cjs ./tracker.db
```

Follow the included `README.md` for CLI usage, persistent reopen, application
export/restore and Rust source builds. `manifest.json` identifies the source,
toolchain and tested host. Licenses, dependency notices and verification logs
are included.

## Tested scope

- Linux x64 under WSL2, glibc 2.39; Node 22.0.0 and 24.19.0; Rust 1.88.0.
- Fresh-directory CLI and application quickstart, including restore.
- Exact retained Node tarball installation and TypeScript consumers.
- Standalone Rust consumer with 244 pinned dependency identities.
- Final scoped check: 673 Rust tests and 101 Node/application tests passed;
  formatting, Clippy and TypeScript passed. One known trigger-cancellation test
  is ignored. All 31 embedded checksums passed after archive extraction.

## Limitations

This is an evaluation prerelease, not stable V1 or a production-readiness claim.
Binaries use the debug profile; do not use them to judge production performance.
Other Linux distributions, Windows and macOS are not qualified. Results and
snapshots materialize in memory; configured limits are not a global memory cap.
Cancellation of trigger-bearing writes is unsupported because of a known pinned
engine defect. Some collection SQL forms remain unsupported; use the documented
application workflow. No complete SQLite compatibility, power-loss guarantee or
untested binary-upgrade guarantee is made.

Public npm/crates.io installation, cloud, sync, inverse links, indexed ANN/FTS/
spatial search and user JavaScript are not part of this preview.

## Feedback requested

Report whether the quickstart worked on your environment, the application you
tried to build, and any query or API limitation that prevented that workflow.
For a bug, include a minimal reproduction, expected/actual behavior, runtime
versions and whether reopening changes the result. Do not include credentials or
private database contents. Report bugs at https://github.com/fastdborg/fastdb/issues.
