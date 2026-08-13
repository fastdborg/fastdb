# FastDB release-readiness checklist

Status: stop for release review

## Local technical candidate

- [x] Turso pin retained; no inherited engine/parser/WAL/yield-point change.
- [x] Format and dialect remain 1; migration fixtures and digests are committed.
- [x] FastDB packages remain `0.0.0` and `publish = false`.
- [x] Bounded parse/prepared caches preserve transaction and schema semantics.
- [x] Parser and structured CRUD fuzz targets exist with independent seeds.
- [x] Resource, model, injection, JSON, CLI, crash, public-I/O, fixture,
      filesystem, integrity, and index-plan coverage exists.
- [x] FastDB-only non-publishing Linux/macOS/Windows workflow is committed.
- [x] Public API/CLI, limitations, format/upgrade, clean-room, benchmark, and
      compatibility documents are current.
- [x] The complete local command matrix and release benchmark pass. Final
      evidence is recorded in `docs/phase5-report.md`; the benchmark timing
      gates passed but are host-sensitive and need a clean uncontended
      reconfirmation during release review.

## External release blockers

- [ ] The committed GitHub Actions workflow has passed on Linux, macOS, and
      Windows at the exact candidate commit.
- [ ] Counsel has approved the Community License/Additional Use Grant and
      commercial-license terms.
- [ ] Counsel has approved a CLA sufficient for the intended dual-license
      model; third-party contributions remain closed until then.
- [ ] The operating entity and ownership/assignment chain are approved.
- [ ] Trademark, product naming, and compatibility wording are approved.
- [ ] A human release reviewer has approved benchmark representativeness,
      known limitations, artifacts, notices, and signing/release procedure.

No publish, tag, release upload, third-party contribution acceptance, or
production-readiness claim is authorized while any external item is open.
