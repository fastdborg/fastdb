# Licensing Decision Record

Status: policy direction approved; final text pending qualified legal
counsel. This record summarizes the direction from `revised_plan.md`
section 1.3 and the constraints in force **now**, before counsel
finalizes terms. It is not license text.

## Intended model (subject to counsel)

FastDB-authored code is intended to be dual/triple-licensed:

1. **Community License** — Business Source License 1.1 (BSL 1.1) with a
   narrowly drafted Additional Use Grant permitting internal production
   use, self-hosting, modification, redistribution, and applications that
   use FastDB internally, while restricting offering FastDB as a competing
   hosted/managed database service.
2. **Commercial License** — for prohibited managed-service use, qualifying
   OEM redistribution, and customers needing other terms.
3. **Change License** — Apache License 2.0, taking effect no later than
   the maximum period BSL 1.1 permits. The initial preference is four
   years after each version's first public release. Counsel chooses the
   exact Change Date.

Until the Change Date, FastDB is **source-available, not OSI open
source**. The Open Source Definition does not permit discriminating
against a field of endeavor, so a current license cannot both be open
source and forbid competing cloud services. Documentation must use
"source-available" accurately and must not market pre-Change-Date
releases as open source.

## Inherited Turso code

All inherited Turso files retain their MIT notices and permissions
(`LICENSE.md`, `NOTICE.md`, `CONTRIBUTING.md` at the repository root).
The FastDB license governs FastDB-authored files and the combined
distribution; it cannot revoke permissions already granted for upstream
Turso code. File provenance stays mechanically auditable.

## Constraints in force NOW (before counsel finalizes terms)

Until final license files, CLA, legal entity, and trademark policy are
approved by counsel:

- New FastDB crates are `publish = false` in their manifests.
- FastDB-authored crates do **not** use `license.workspace = true`
  (the Turso workspace license is MIT; inheriting it would falsely state
  that new FastDB-authored files are MIT).
- No FastDB release is published.
- No material third-party contribution is accepted. Dual licensing
  requires sufficient relicensing rights, so a counsel-approved CLA is
  required (a DCO alone is insufficient). A DCO may be added in addition
  to the CLA, not instead of it.
- No informal license text is drafted in the repository. Counsel adapts
  the standard BSL parameters and publishes practical examples.
- No trademark or compatibility promises are made.

## Provenance and relicensing rights

Dual licensing requires the project to retain sufficient relicensing
rights. Contributors retain copyright but grant the FastDB legal entity
the licenses needed to distribute contributions under the Community,
Commercial, and Change Licenses (via the CLA above). The legal entity,
CLA, privacy terms, and trademark policy are established before accepting
material external code.

## What this means for Phase 0

Phase 0 crates carry `publish = false`, no `license`/`license-file` field
inheriting MIT, and clear provenance (FastDB-authored files live in new
crates). The Phase 0 report records this legal block. No Phase 0 artifact
makes a licensing, trademark, or compatibility promise.
