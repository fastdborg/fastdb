# Upstream (Turso) Policy

FastDB is a clean-room frontend built on a pinned fork of Turso. This
document records the engine pin, how to audit and update it, the merge
policy, notice preservation, and how to isolate any change
that is intended for upstream contribution.

## Pinned baseline

| Item | Value |
| --- | --- |
| Engine | Turso (`tursodatabase/turso`) |
| Pinned commit | `977383ff40edc44ef410af062ed0d2322252a869` |
| Configured remote | `upstream` → `https://github.com/tursodatabase/turso.git` |
| Behavioral reference (not the engine) | SurrealDB `v3.1.5` |

The pinned commit is recorded here, in `docs/phase0-engine-audit.md`,
and must be recorded in CI output. The pinned commit is an ancestor of
the FastDB development branch (verified with
`git merge-base --is-ancestor <sha> HEAD`).

## Repository shape

This workspace is **one** monorepo. Turso's full history is part of
FastDB's Git ancestry. We do **not**:

- create a nested Git repository,
- vendor a history-less engine copy,
- use Turso as an unpinned submodule, or
- build CI or releases from a floating branch.

## Auditing a new pin

Before moving the pin to a newer Turso commit:

1. Read the most-specific applicable `AGENTS.md` files.
2. Re-read this file and `docs/phase0-engine-audit.md`.
3. Check out the candidate commit and run the relevant unchanged Turso
   suites (JSONB, expression index, transactions, WAL, reopen, and the
   PostgreSQL frontend tests).
4. Run the FastDB Phase 0 tests and benchmarks.
5. Update the pin, `docs/phase0-engine-audit.md`, this file, and CI in
   **one** coordinated change. Never leave the pin and the documentation
   out of sync.
6. Confirm experimental facilities (MVCC, multiprocess WAL, experimental
   index methods, FTS, encryption, sync) remain disabled unless a phase
   plan explicitly and auditedly enables them.

## Merge policy

- Review upstream changes regularly.
- Integrate one exact candidate SHA on a dedicated `upstream-sync/<date>-<sha>`
  branch. Use an explicit merge commit so Turso provenance and FastDB pin
  transitions remain auditable.
- Do not rebase or force-push shared FastDB history to update Turso.
- Merge only after the relevant FastDB tests and unchanged Turso tests pass
  on the candidate commit.
- Preserve unrelated user changes. Never use destructive Git operations
  (`reset --hard`, force-push to shared branches, `filter-branch`) to
  simplify an import or update.
- When upstream paths conflict with FastDB files, stop and report the
  exact paths before resolving (as done for the root `AGENTS.md` and
  `README.md` during the initial import — resolved in favor of FastDB).

The required read-only audit, candidate selection, conflict checklist,
verification commands, urgent-patch exception, atomic pin update, and handoff
format are defined in [`.claude/skills/upstream-sync/SKILL.md`](.claude/skills/upstream-sync/SKILL.md).
Agents must load that skill before any upstream fetch, comparison, pin change,
merge, cherry-pick, or conflict resolution.

## Notice preservation and provenance

- Every inherited Turso file retains its MIT notice and permissions.
  `LICENSE.md`, `NOTICE.md`, and `CONTRIBUTING.md` at the repository root
  are inherited from Turso and preserved.
- File provenance stays mechanically auditable: inherited files are
  unchanged FastDB-vs-upstream diffs; FastDB-authored files live in new
  crates/directories.
- See `docs/licensing.md` for the FastDB licensing direction and the
  block on publishing and third-party contributions pending counsel.
- One upstream root doc was relocated to avoid a name collision with the
  FastDB normative matrix: Turso's SQLite compatibility doc was moved
  `COMPAT.md` → `docs/upstream-turso-sqlite-compat.md` (content and
  history preserved via `git mv`). FastDB's `COMPAT.md` is the FastDB
  SurrealQL feature matrix. This is a documentation relocation, not an
  engine change. Future upstream merges should account for the rename.

## Isolating upstreamable engine changes

Phase 0 must not modify Turso core, parser, WAL, JSON, optimizer,
bindings, or existing frontends. If a required public API is missing:

1. Prove the gap with the smallest test or compiler error.
2. Search the pinned source for a supported alternative.
3. Write a design note (`docs/phase0-blocker-<topic>.md`) describing the
   minimal potential upstream API.
4. Stop at the Phase 0 gate. Do not patch private internals or fork core
   behavior.

If a later phase approves an engine change, it must be:

- isolated to the smallest possible diff,
- independently tested,
- documented in a design note, and
- shaped for upstream submission (not a private fork behavior).

## What counts as "no Turso core change"

Calling public, documented `turso_core` / `turso_parser` APIs from new
FastDB crates is allowed and expected. Editing any file under `core/`,
`sqlite/parser/`, `bindings/`, `postgres/`, or upstream test directories
is a core change and is disallowed in Phase 0. Root workspace manifests
(`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`) may change only to
register FastDB crates and dependencies.
