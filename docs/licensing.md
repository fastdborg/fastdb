# Licensing Decision Record

Status: approved by the project owner on 2026-08-13

This decision supersedes the earlier proposed BSL/community, commercial, and
change-license model recorded in historical phase documents.

## FastDB Core

FastDB-authored Core code is open-source software under the MIT License in
`LICENSE.md`. FastDB package manifests identify that license with
`license = "MIT"`.

MIT permits use, copying, modification, merging, publication, distribution,
sublicensing, sale, and managed-service use subject to preserving the license
notice and disclaimer. These rights cannot later be withdrawn from copies
already distributed under MIT.

Core packages remain version `0.0.0` and `publish = false` until the project
owner deliberately packages the first public alpha. Those are release-safety
controls, not restrictions on the MIT rights in the source.

## Inherited Turso code

All inherited Turso files retain their existing MIT notices and permissions.
The repository license lists both the Turso and FastDB authors, and
`NOTICE.md` keeps inherited dependency notices intact. File provenance must
remain mechanically auditable during upstream synchronization and release
review.

## Contributions

Contributions accepted into FastDB Core must be provided on terms compatible
with MIT. The retired dual-license model's special relicensing CLA, commercial
license, Change License, and legal-entity approval are not Core release
blockers. A separate contribution guide or DCO may be adopted as project
governance before opening public contribution intake; it must not contradict
the MIT license already granted for Core.

## Proprietary FastDB Cloud boundary

A future FastDB Cloud service may be developed as closed-source proprietary
software. It may use FastDB Core under MIT, but it is a separate product
boundary. Cloud-only control-plane, orchestration, billing, operations,
customer, and infrastructure code or data need not be published.

Private Cloud components must not be required to build, test, use, modify, or
self-host Core. The proprietary service model does not alter, narrow, or
revoke anyone's MIT rights in Core, including the right to operate a competing
service using Core.

## Product name and compatibility wording

The project owner has approved the FastDB product name and the phrase
"SurrealQL-compatible subset" for the current development stage. That phrase
describes a tested behavioral target only. It does not imply sponsorship,
affiliation, certification, ownership of third-party marks, or complete
compatibility. `CLEAN_ROOM.md` remains normative for compatibility research.

## Release process

The former license, CLA, entity, trademark, and remote-CI approval items no
longer block Core feature development or the first public alpha. Local
verification is authoritative during this period. Before that alpha is cut,
the project must rerun the full local matrix and perform the human
artifact/documentation review in `docs/release-readiness.md`. GitHub Actions
may be reconsidered later, but is not an alpha gate.
