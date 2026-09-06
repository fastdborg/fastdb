# Engine provenance

- Repository: https://github.com/tursodatabase/turso (remote `upstream`).
- Baseline: release `v0.7.2`, full commit `046e9cbf67d22491e8ecc941ec2891b02a9f3cad`.
- Local development branch: `feat/embedded-foundation`; `main` is the pinned baseline, preserving upstream history. Remote FastDB fork and branch protection are pending owner/account configuration; no remote has been created or pushed.
- Toolchain: upstream Rust 1.88; locally tested with 1.88.0 on Linux x86_64.
- Engine dependency: in-workspace `turso_core` with its default features. No FTS or experimental engine features enabled by FastDB.
- This release exposes `Connection::prepare`, `prepare_stmt`, and blocking statement execution. It predates the `postgres/frontend` directory in the plans; the same separate-frontend boundary is used without copying or modifying core.

Upstream evidence inspected through the GitHub check-runs API for this exact SHA:
[Linux native Node DB bindings](https://github.com/tursodatabase/turso/actions/runs/30547750865/job/90892023910) succeeded;
[Windows native Node DB bindings](https://github.com/tursodatabase/turso/actions/runs/30547750865/job/90892023935) succeeded.
This supports the baseline only. Broad engine conformance and our combined build still require their own evidence; these upstream jobs are not FastDB tests.

Local integration changes:

1. Five explicit FastDB workspace members and corresponding lockfile entries; upstream default-members unchanged.
2. FastDB implementation, tests, scripts, and documentation under `fastdb/`.
3. Inherited workflow YAML files moved unchanged into `.github/upstream-workflows/` so GitHub cannot execute them. Only `.github/workflows/fastdb-ci.yml` remains active. Audit this directory on every upstream merge before pushing.
4. No upstream engine, parser, bindings, or CLI implementation changes. The frontend also directly depends on the pinned workspace parser/extension crates for AST lowering and statically linked pure accessors; registration uses the documented unsafe startup extension context API, which is freed before exposing the connection.

The root planning directory is not a Git repository. The ancestry-preserving checkout lives in `turso/`; product source is `turso/fastdb/`.

The separate FastDB native addon reuses pinned napi 3.8.3, napi-derive 3.5.2 and napi-build 2.3.1 from the existing lockfile. It depends on FastDB rather than the upstream Node binding and does not enable that binding's FTS feature. The frontend additionally embeds pinned rquickjs 0.12.2 for its fixed bundled string catalog.
