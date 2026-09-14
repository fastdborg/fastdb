# S6 optimized Linux review bundle

Built from clean source 7e7f376fcb510b3b093a4044277958fd37c6a3bd on 2026-09-14:

```sh
python3 fastdb/scripts/build-preview.py dist/fastdb-v1-review-linux-x64 \
  --release --label v1-review-linux-x64
```

Build completed successfully with Rust 1.88.0, release optimization and debug
information on Linux x64/WSL2, glibc 2.39. Log: /tmp/fastdb-v1-review-build.log.
Every entry in the bundle SHA256SUMS passed verification. Source archive,
CLI/Node dependency inventories, notice audits and generated notice collections
are retained in the bundle. Inventory audits identify their own scope; they are
not proof of every statically linked component's attribution by themselves.

The exact fastdb-node-0.1.0.tgz passed check-node-package.cjs with
FASTDB_PACKAGE_TARBALL set to that file, on Node 22.0.0 and 24.19.0. The checker
requires byte equality to the current packed package, installs it offline in a
separate consumer, exercises both clients including collection CRUD/rollback,
and checks TypeScript usage. Both runs reported 10 files and 23,386,884 packed
bytes. Logs: /tmp/fastdb-v1-package22.log and /tmp/fastdb-v1-package24.log.

| Artifact | SHA-256 |
|---|---|
| Node package | de6aecda3f410531620a20cb798a053262275a661062928f442e09b2c9e849d3 |
| CLI | 8420c566895c56f50a75d7b40bde5f5c89977e1b98dd10b5a493fcdbdcc02702 |

An additional installation at /tmp/fastdb-v1-review-consumer was used as the
new package in check-preview-upgrade.cjs, with the published preview.2 installed
package as the old package. Creation, upgrade and reopen phases all passed:
/tmp/fastdb-v1-review-upgrade.log. This replaces a development-addon-only claim
with an exact optimized-package upgrade check for this source.

The bundled CLI was also executed against a temporary fresh database: object
INSERT automatically created notes, SELECT returned its typed record/document,
and a second CLI process returned the same saved text. Both processes exited 0.
This verifies the candidate quickstart's persistence path using the retained
optimized CLI, independently of the Node addon.

## Remaining release work

This is a local review bundle, not stable V1 or a registry publication. Its Node
package version remains 0.1.0 and its included preview README/limitations retain
preview framing. Final packaging must use the chosen stable version and current
handoff documentation, recheck affected final artifacts, and publish only after
the remaining requirements are closed. The reviewed trigger core exception is
still pending approval; this bundle deliberately records the current unpatched
source rather than claiming that S3 is complete. Linux x64 and Node 22/24 are the
measured package targets; no other platform is established by this evidence.
