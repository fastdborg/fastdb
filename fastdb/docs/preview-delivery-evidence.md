# Preview milestone: candidate delivery

P3 and P4 are complete. The retained local candidate is
`dist/fastdb-preview-linux-x64/`, built from source commit `dcce6c4f0`.
The directory is excluded from Git and has not been uploaded or published.

It contains the Linux x64 CLI, `fastdb-node-0.1.0.tgz`, the full pinned source
archive, an installed-package tracker example, quickstart, provenance manifest,
licenses/notices, separate CLI/Node dependency inventories and notice audits,
and SHA256SUMS covering artifacts and retained evidence. The binaries are debug
builds for evaluation, not performance-qualified release builds. Dynamic library
resolution passed on the build host; ldd outputs are retained.

## Acceptance evidence

- The quickstart ran in a fresh temporary directory using candidate artifacts:
  CLI creation/reopen; local package install; identical tracker results across
  repeated invocations; the documented export/restore snippet completed.
- Node 22.0.0 and 24.19.0 package smoke checks passed, including strict TypeScript
  consumers. Both installed the supplied candidate tarball after verifying its
  bytes match the current package. The tarball has 10 files and 60,832,147 bytes.
- The standalone Rust consumer passed on Rust 1.88.0, outside the workspace;
  244 registry/git package identities matched the pinned lockfile. It used the
  checkout represented by the source archive, not a published crate.
- All final candidate checksums were verified. Logs and dependency/notice records
  are retained in the candidate's `evidence/` directory.

The builder is `fastdb/scripts/build-preview.py`; it requires a clean checkout
and a new output directory. Use the pinned toolchain from UPSTREAM.md. Package
verification accepts `FASTDB_PACKAGE_TARBALL` to test the exact retained artifact.
The dependency inventory tool accepts `FASTDB_INVENTORY_PACKAGE` for the CLI;
its existing default remains fastdb-node.

No database implementation changed in this milestone. Application/storage
acceptance from P1/P2 therefore remains applicable. P5/P6 are next: the final
candidate acceptance run and a consolidated reviewable handoff. No additional
platform or speculative SQL matrices are required by this milestone.
