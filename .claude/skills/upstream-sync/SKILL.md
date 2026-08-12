---
name: upstream-sync
description: Safely inspect, audit, and integrate updates from the official Turso repository into the FastDB monorepo. Use whenever a task involves checking Turso releases or commits, fetching or comparing upstream, changing the pinned engine SHA, merging or cherry-picking a Turso fix or feature, resolving an upstream conflict, or preparing an upstream-sync pull request.
---

# Sync Turso Upstream

Preserve FastDB's pinned, auditable engine baseline while importing selected Turso fixes and features. Treat every pin change as an engine upgrade, not routine dependency refresh.

## Read the Policy First

Read, in order:

1. The root `AGENTS.md` and any more-specific instructions.
2. `UPSTREAM.md`.
3. The active phase plan and current engine audit/report.
4. `reviews.md` when Phase 0 remediation is unfinished.

If these disagree about the pin, branch, or release state, stop and report the conflict before changing Git history.

## Choose the Task Mode

- For **monitoring or advice**, fetch/read refs if authorized, compare candidates, and report. Do not merge, edit pins, or push.
- For an **engine update**, create a dedicated integration branch, merge one exact audited SHA, run all gates, and prepare a reviewable diff.
- For an **urgent patch**, prefer an audited upstream release/candidate containing the fix. Cherry-pick only when the user explicitly authorizes the narrower patch.

Never interpret “check upstream” as permission to merge or push.

## 1. Establish a Safe Baseline

Run read-only checks first:

```bash
git status --short
git branch -vv
git remote -v
git log --oneline --decorate --max-count=20
git merge-base --is-ancestor <current-pin> HEAD
```

Then:

- Identify the current pin from all existing machine-readable metadata, `UPSTREAM.md`, `AGENTS.md`, the engine audit, and CI. Do not guess which record is authoritative when they differ.
- Confirm `upstream` points to `https://github.com/tursodatabase/turso.git`.
- Start from the user-designated FastDB integration branch, normally `main` after the current phase is accepted. Do not sync onto an unfinished topic branch unless explicitly requested.
- Preserve a dirty worktree. Do not stash, reset, restore, or overwrite user changes to obtain a clean base. Report the conflicting paths and wait if they prevent a safe sync.

## 2. Fetch and Select an Exact Candidate

Fetch without merging:

```bash
git fetch upstream --tags --prune
```

Select an immutable commit SHA. Prefer a Turso release/tag commit when it satisfies the needed fix; use an audited `main` commit only deliberately. Never pin a branch name.

Inspect the range before creating an integration merge:

```bash
git log --oneline --decorate <current-pin>..<candidate-sha>
git diff --stat <current-pin>..<candidate-sha>
git diff --name-status <current-pin>..<candidate-sha>
```

Pay particular attention to:

- `core/` transaction, WAL, pager, I/O, JSONB, optimizer, and file-format changes.
- `sqlite/parser/` and translated-statement API changes.
- `postgres/` frontend changes that demonstrate the shared AST integration seam.
- Default feature or `DatabaseOpts` changes, especially MVCC, multiprocess WAL, index methods, FTS, encryption, and sync.
- Rust toolchain, workspace dependency, license, notice, and release changes.

Record why this candidate was selected and which upstream changes FastDB needs.

## 3. Merge Without Rewriting FastDB History

Use a short-lived integration branch and an explicit merge commit:

```bash
git switch <fastdb-base-branch>
git switch -c upstream-sync/<date>-<short-sha>
git merge --no-ff <candidate-sha>
```

Do not rebase or force-push shared FastDB history. The monorepo already contains Turso ancestry; merge commits keep imported provenance and pin transitions auditable.

Do not resolve conflicts automatically. First list them:

```bash
git diff --name-only --diff-filter=U
```

Review every conflict. Known root integration points require special care:

- `AGENTS.md` is FastDB's durable guide. Incorporate relevant new Turso engineering instructions without replacing FastDB policy.
- `README.md` is FastDB-owned. Review upstream documentation changes separately.
- Root `COMPAT.md` is FastDB's SurrealQL matrix. Port upstream Turso SQLite compatibility changes into `docs/upstream-turso-sqlite-compat.md`.
- Root `Cargo.toml` must retain all upstream workspace changes and FastDB workspace members.
- Regenerate `Cargo.lock` through Cargo after resolving manifests; do not hand-merge dependency entries blindly.
- Preserve `LICENSE.md`, `NOTICE.md`, inherited file headers, and mechanically auditable provenance.

If a conflict reveals private FastDB edits inside Turso core, stop and inventory that fork delta before continuing.

## 4. Audit Behavior Before Updating the Pin

Keep the documented pin unchanged until the candidate passes review. Verify at least:

```bash
cargo fmt --all -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb \
  -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets
cargo test -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests
cargo test -p turso_core --lib
cargo test -p core_tester --test integration_tests
cargo test -p turso_pg_tests
cargo bench -p turso_fastdb_benchmarks --bench phase0
```

Adapt package names only after checking `cargo metadata`. Record every filtered, skipped, ignored, or pre-existing failure accurately.

Also verify:

- FastDB uses `SqliteDialect` and `prepare_translated_stmt_with_options` as intended.
- Stable WAL/full durability remains the selected mode.
- Experimental facilities did not become enabled through changed defaults.
- The actual translated FastDB filter still selects every declared expression index.
- Catalog/DDL/data rollback, reopen, integrity, and commit-failure tests pass.
- Corrected benchmarks compare identical physical work, durability, cache state, and result materialization.
- Stable-format fixtures created by the previous released pin reopen correctly once format version 1 exists.

Do not waive a failed correctness, recovery, format, or provenance gate because the upstream release is newer.

## 5. Update the Pin Atomically

Only after the candidate passes:

- Update every existing machine-readable pin record.
- Update `UPSTREAM.md`, `AGENTS.md` when its baseline changes, the engine audit, CI output/assertions, and the active phase/release report.
- Record the old SHA, new SHA, Turso tag if any, reason, relevant upstream range, conflicts, commands, results, benchmark deltas, and known limitations.
- Confirm the new SHA is an ancestor of the integration branch.
- Review the complete FastDB delta against the new pin.

Keep the merge, integration fixes, generated lockfile, and pin/evidence changes easy to review. Follow the repository's atomic-commit guidance; do not mix unrelated FastDB features into the sync.

Never push, publish, or merge the FastDB pull request unless the user has authorized that external action.

## Urgent Cherry-Pick Exception

Use cherry-pick only for a narrow security or data-correctness fix when waiting for a full audited merge is unacceptable and the user approves it.

Before cherry-picking:

1. Inspect the fix and all prerequisite commits.
2. Prove it applies to the pinned version.
3. Record the upstream SHA and why a normal merge was deferred.
4. Run the same correctness gates as an engine update.
5. Track the patch until a later upstream merge contains or supersedes it.

Never maintain an undocumented private patch stack.

## Stop Conditions

Stop and report rather than improvising when:

- The base branch or current pin is ambiguous.
- User changes overlap the sync.
- A conflict affects Turso core behavior, file format, WAL, transaction semantics, or licensing.
- A previously passing FastDB or relevant unchanged Turso test fails.
- A database created by a supported previous pin cannot reopen safely.
- The candidate enables an experimental facility or changes durability defaults.
- The update requires destructive Git operations or rewriting shared history.
- Pin documentation, CI, and the checked-out engine cannot be made consistent in one reviewed change.

## Handoff

Lead the final report with one result: **adopt**, **reject**, or **blocked**. Include the exact old/new SHAs, candidate rationale, conflicts, files changed, verification commands/results, performance deltas, format/recovery impact, remaining risks, and whether any external push or PR action remains.
