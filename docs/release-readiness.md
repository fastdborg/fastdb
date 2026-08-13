# FastDB release-readiness checklist

Status: Core feature development unblocked; final public alpha not yet cut

## Decisions completed on 2026-08-13

- [x] FastDB Core is licensed under MIT in `LICENSE.md` and FastDB package
      manifests record `license = "MIT"`.
- [x] Turso copyright, MIT permissions, dependency notices, and mechanical
      provenance remain preserved.
- [x] The former BSL/commercial/change-license model and its special CLA and
      entity-approval blockers are retired for Core.
- [x] A future proprietary FastDB Cloud service is a separate product and does
      not alter Core's MIT terms.
- [x] The project owner approved the FastDB product name and the precise
      "SurrealQL-compatible subset" wording, subject to the disclaimers in
      `CLEAN_ROOM.md`.
- [x] The FastDB-only GitHub Actions workflow is removed for now; verification
      remains local while the compatibility surface expands.

## Phase 5 local technical baseline

- [x] Turso pin retained; no inherited engine/parser/WAL/yield-point change.
- [x] Format and dialect remain 1; migration fixtures and digests are committed.
- [x] FastDB packages remain `0.0.0` and `publish = false`.
- [x] Bounded parse/prepared caches preserve transaction and schema semantics.
- [x] Parser and structured CRUD fuzz targets exist with independent seeds.
- [x] Resource, model, injection, JSON, CLI, crash, public-I/O, fixture,
      filesystem, integrity, and index-plan coverage exists.
- [x] Public API/CLI, limitations, format/upgrade, clean-room, benchmark, and
      compatibility documents are current for the Phase 5 baseline.
- [x] The Phase 5 local command matrix and release benchmark passed. Historical
      evidence is recorded in `docs/phase5-report.md`.

## Deferred final-alpha gates

- [ ] Freeze the intended alpha feature and compatibility surface.
- [ ] Rerun the complete local test, fuzz, fixture, crash/recovery, integrity,
      release-build, and benchmark matrix on the exact alpha candidate.
- [ ] Review package versions, `publish` settings, artifacts, checksums,
      notices, documentation, limitations, signing, and rollback procedure.
- [ ] Decide whether the alpha accepts external contributions. If it does,
      add a FastDB-specific MIT contribution guide and clearly distinguish the
      inherited Turso `CONTRIBUTING.md` instructions.
- [ ] Publish a FastDB vulnerability-reporting contact/process; the inherited
      monorepo currently has no FastDB security policy.
- [ ] Explicitly authorize and perform the tag, package publication, and
      release upload operations.

None of the deferred final-alpha gates blocks additional Core feature work.
They become release blockers only after the alpha feature surface is frozen.
GitHub Actions and remote CI are explicitly not gates for this alpha; they may
be reconsidered after the alpha release.
