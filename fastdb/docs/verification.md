# Local foundation verification — 2026-09-06

- Upstream base: `046e9cbf67d22491e8ecc941ec2891b02a9f3cad` (v0.7.2).
- Development branch: `feat/embedded-foundation`.
- Platform: Linux x86_64; Rust 1.88.0; default turso_core features.
- Cargo.lock SHA-256: `a38e8a443a0f8e2e87a20246e6064062c13822c6f68530fd76d711019fb1f4c0`.
- `fastdb/scripts/check.sh`: passed scoped formatting, Clippy `--all-targets --no-deps -- -D warnings`, and 11 tests (7 persistence/recovery harness tests including the subprocess helper, 1 differential SQL test, 3 parser tests).
- Differential harness runs 14 sequential relational SQL probes against the raw pinned engine and FastDB, comparing column names and typed rows. It is compact smoke coverage, not the final SQL compatibility matrix.
- Recovery harness exits a child process without dropping its connection after a committed indexed document and uncommitted changes. Reopen preserves the committed record/index and discards uncommitted changes. Mid-commit/checkpoint fault injection remains untested.
- All 34 archived inherited workflows match their upstream bytes. Only fastdb-ci.yml remains in the executable workflow directory.
- Upstream source changes are restricted to workspace member/lockfile wiring and workflow relocation; no core/parser/bindings/CLI implementation edits.

No remote CI result, platform packaging evidence, release benchmark, restore/upgrade rehearsal, or V1 completion is claimed.

CLI smoke: the 10-statement `fastdb/examples/persistent.fastql` example ran successfully through `cargo run --locked -p fastdb-cli -- :memory:`; all output lines parsed as JSON, no errors were returned, and the post-rollback SELECT returned Alice.
