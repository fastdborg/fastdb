# V1 implementation status

V1 is incomplete. The full scope is the FastDB.md master plan in the parent planning directory. This file records current evidence and work remaining; no milestone substitutes for the full V1 goal.

## Repository and tooling

- Local Git checkout: `turso/`, branch `feat/embedded-foundation` (local `main` is the baseline), upstream v0.7.2 at `046e9cbf67d22491e8ecc941ec2891b02a9f3cad`.
- Four workspace crates: fastql-parser, fastdb, fastdb-cli, fastdb-tests. All product crates are unpublished 0.1.0 prototypes.
- Scoped script: `fastdb/scripts/check.sh`; one Ubuntu CI YAML, read-only permissions, timeout and cancellation. Inherited workflow files moved unchanged to `.github/upstream-workflows/`.
- Remote FastDB fork owner is unresolved. No origin remote, push, PR, branch protection, or hosted FastDB CI result exists yet.
- Local toolchain is installed under `/tmp/fastdb-cargo` and `/tmp/fastdb-rustup`. Run with PATH prefixed by `/tmp/fastdb-cargo/bin`, CARGO_HOME and RUSTUP_HOME set accordingly, and RUSTUP_TOOLCHAIN=1.88.0. These temporary tools may need reinstalling on another machine/session.

## Implemented subset

- Bare collection CREATE TABLE, IF NOT EXISTS, fixed typed IDs, direct-record SELECT, object/DOCUMENT INSERT, target object UPDATE, target DELETE, RETURNING *.
- Nested objects/arrays, tagged persisted values, bool/int64/null/binary distinction, UUIDv7 automatic IDs.
- Field definitions (basic types, required/nullable, nested paths, overwrite) and single-path managed scalar/reference indexes with unique/nonunique variants. Rust APIs and initial FastQL declarations.
- Statement savepoints, transactional catalog/index maintenance, mixed ordinary SQL/document transactions.
- Rust query cardinality helpers and initial line-oriented stdin CLI with tagged JSON output.
- Ordinary SQL delegation outside the conservative collection-name guard.

## Verification

The scoped test suite includes parser collision probes; persistent CRUD/reopen; mixed transaction rollback; failed unique inserts/updates/index builds; validation-definition rollback; typed round trips; numeric/index identity; a child process that exits without closing an active transaction; and differential ordinary SQL probes against the pinned engine. On 2026-09-06, `fastdb/scripts/check.sh` passed formatting, Clippy with warnings denied for the FastDB packages, and all 11 tests (including the subprocess helper). This is local Linux evidence; hosted CI has not run. The process-exit test is a basic recovery smoke, not interrupted-checkpoint or power-loss certification.

## Next implementation work

1. Replace conservative managed-name token rejection with AST-aware SQL lowering and authorization. Implement collection SELECT projections, predicates, joins, indexed planning, sorting/pagination, and SQL-shaped writes. Preserve baseline parameter/alias forms and ordinary SQL errors. Current guard rejects some harmless strings and is not a final compatibility/security boundary.
2. Finish expressions and dynamic records; document functions, SET/UNSET, UPSERT, CHECK, remove/drop/inspection, full index lifecycle, stable errors/results, cancellation and resource limits. Namespace collisions, metadata format validation, multi-connection schema races, and managed object dependency access need full coverage.
3. Add snapshot-consistent batched one-hop links, bundled bounded QuickJS functions, and verified exact upstream vectors. Vector bytes currently have no validated public constructor/field validator; do not advertise vector support yet.
4. Native Node/TypeScript client and complete Rust packaging; lossless cross-language wire encoding; CLI multiline/batch UX, import/export, migrations, schema/query-plan inspection.
5. Complete all master-plan/FastQL release gates: broad differential coverage, interrupted commits/checkpoints, restore/upgrade rehearsal, bounded crash/fuzz/stress, resource limits, benchmarks and platform packaging smoke tests. External pilots and business evidence are also not present.

Keep upstream implementation files unchanged. No cloud implementation or V2/V3 features have begun.
