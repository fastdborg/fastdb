# FastDB Agent Guide

This file is the durable, cross-phase context for people and coding agents working in this repository. Keep it concise and current. Put detailed execution steps, transient status, and phase-specific evidence in the relevant phase plan or report instead.

## Start Here

Before changing code or repository structure:

1. Read this file completely.
2. Read [`revised_plan.md`](revised_plan.md) for the product and architecture contract.
3. Read the plan for the active phase. Phase 10 operational readiness is the
   completed implementation baseline. Phase 11 stopped at its mandatory MVCC
   audit; [`plan-phase11.md`](plan-phase11.md) and
   [`docs/phase11-mvcc-audit.md`](docs/phase11-mvcc-audit.md) preserve the stop
   evidence. Phase 12 begins the approved broad-compatibility pre-1.0 track;
   read [`plan-phase12.md`](plan-phase12.md) before executable changes. Earlier
   plans and reports remain historical evidence.
4. Inspect the actual repository state. The planning workspace may not yet have been converted into the Turso-derived monorepo.
5. After the Turso import, find and obey any more-specific `AGENTS.md` files below the directory being changed.
6. Use `cargo metadata` and the checked-out source instead of guessing current package names or APIs.
7. For any Turso release check, fetch, comparison, pin change, merge, cherry-pick, or upstream conflict, load and follow [`.claude/skills/upstream-sync/SKILL.md`](.claude/skills/upstream-sync/SKILL.md) before acting.

If instructions conflict, apply this precedence:

1. System, developer, and explicit current-user instructions.
2. The most-specific applicable `AGENTS.md`.
3. The active phase plan.
4. `revised_plan.md`.
5. `plan.md` was historical pre-planning input that was **not** carried into this monorepo (only `revised_plan.md` and the per-phase plans were preserved). Treat `revised_plan.md` and the active phase plan as the live planning sources; do not expect a `plan.md` file.

Do not silently resolve a material architectural contradiction. Record it and follow the stop/escalation process in the active phase plan.

## Project in One Paragraph

FastDB is a clean-room, SurrealQL-compatible document database frontend built on a pinned fork of Turso. It provides its own parser, AST, compatibility contract, execution frontend, embedded asynchronous Rust API, and CLI while using Turso for durable storage, transactions, WAL, JSONB, query execution, and indexes. The MVP intentionally supports a documented SurrealQL subset rather than claiming complete SurrealDB compatibility. FastDB Core is publicly developed as MIT-licensed open-source software. A future managed FastDB Cloud service may be developed separately as proprietary software.

## Current Baselines and State

- Initial Turso engineering baseline: commit `977383ff40edc44ef410af062ed0d2322252a869`.
- Behavioral compatibility reference: SurrealDB `v3.1.5`.
- Durability default: stable Turso WAL with full durability.
- Active delivery surfaces: the embedded Rust library and `fastdb` CLI, later
  extended by the Phase 19 server and Phase 21 Rust/TypeScript/Go/PHP SDKs.
- Current implementation stage: Phase 10 operational readiness is technically
  complete locally. Phase 11 stopped because no stable parallel-writer
  candidate qualified. Phase 12 is active. The implemented on-disk format
  remains version 2 until Phase 12 migration passes; serialized stable WAL
  remains the only supported concurrency mode through Phase 22.
- Phase 0 format version `0` is disposable and must not be presented as a stable format.

Before implementation, audit the then-current Turso `main` as required by the plans. Retain the baseline above unless a newer commit is deliberately audited and the pin, plans, reports, and CI evidence are updated together. Never build CI or releases from a floating branch.

## Non-Negotiable Architecture

The request path is:

```text
FastDB source
  -> independent FastDB lexer/parser/AST
  -> FastDB frontend plan
  -> directly constructed Turso AST plus bound values
  -> prepare_translated_stmt_with_options
  -> Turso execution and result decoding
```

Apply these invariants across phases:

- Do not parse FastDB input with Turso's SQLite parser.
- Do not translate user input into generated SQLite text. User values are bound parameters, and logical identifiers are resolved through catalogs.
- Keep FastDB AST types independent of Turso AST types.
- Keep Turso's `SqliteDialect` for persisted internal schema so `sqlite_schema` contains valid SQLite/Turso definitions.
- Static, reviewed internal DDL is allowed. It must not contain interpolated user data or logical user identifiers.
- Avoid Turso core changes. If one appears unavoidable, stop at the applicable gate and write a design note. Any later approved change must be isolated, independently tested, and suitable for upstream submission.
- Use the engine's stable facilities. Experimental multiprocess WAL is outside
  the active track. Parallel writers are prohibited until dormant Phase 23
  qualifies a new exact stable implementation; serialized stable WAL remains
  the default.
- Unsupported syntax must fail explicitly. Never accept and ignore a clause.

The MVP parser is independently authored: a hand-written lexer, recursive-descent statement parser, and Pratt expression parser with source spans, nesting/token limits, and structured diagnostics.

## Storage and Transaction Invariants

Each logical table maps to one opaque hidden physical table, conceptually containing:

```sql
rid TEXT PRIMARY KEY,
doc JSONB NOT NULL
```

The SQL above is a storage illustration, not a user-input translation template.

- Physical table and index names are deterministic, opaque names derived from immutable catalog IDs. Never interpolate logical user identifiers into physical SQL names.
- `rid` contains the immutable canonical record identifier. `doc` contains user content only; synthesize the typed `id` field while decoding results.
- Format 2 may add opaque catalog-managed hidden typed columns for relation
  endpoints, FTS text, and native vectors. They are derived state, remain
  invisible in public documents, and must change atomically with `doc`.
- Catalogs and format metadata are versioned. Refuse unknown future versions before mutation.
- First-use bootstrap, implicit schemaless table registration, physical table creation, and the associated record mutation must be atomic.
- Schema changes use a database-level schema mutex while preserving transaction rollback.
- Nested access and mutation use safe, canonical JSON paths.
- Filters and expression indexes must use the exact same canonical JSON-expression builder so the optimizer can select the index.
- Direct external changes to hidden physical tables are unsupported even though the file uses a SQLite-compatible format.
- “Single file” means one durable `.fastdb` artifact after checkpoint and clean shutdown. WAL/shared-memory files may exist while open, and sync may add Turso-owned sidecars later.

Every standalone statement is atomic. Any error inside an explicit transaction poisons and rolls back the complete transaction.

## Compatibility and Clean-Room Rules

FastDB compatibility work may use:

- Public SurrealQL documentation.
- Independently designed black-box queries against an unmodified SurrealDB `v3.1.5` binary.
- Independently written observations that record input, output, version, date, and public source.

It must not copy, translate, adapt, or vendor SurrealDB source code, test files, fixtures, expected-output files, fuzz corpora, or implementation details. Keep behavioral research notes under `docs/compat-research/` and keep implementation tests independently authored.

`compat/surrealdb-v3.1.5.toml` is the locked machine-readable inventory and
`COMPAT.md` is its normative public view. Every atomic item must be labeled
supported, partial, unsupported, or planned and both representations must stay
mechanically synchronized. Partial is temporary during Phases 12–21; Phase 22
permits only Supported or an approved Unsupported architecture stop.
“SurrealQL-compatible subset” does not mean sponsorship, certification, or
complete compatibility.

## Licensing and Provenance

FastDB-authored Core code is licensed under the MIT License in `LICENSE.md`.
This is an open-source grant without a field-of-use restriction. A future
closed-source FastDB Cloud service may use Core under MIT, but that service is
a separate product boundary and does not alter or revoke Core's MIT terms.

All inherited Turso files retain their MIT notices and permissions. Preserve
every inherited copyright and license notice and keep upstream and
FastDB-authored file provenance mechanically auditable. Contributions to
FastDB Core must be compatible with MIT; the former BSL/commercial/change-
license model and its CLA/entity approval blockers no longer apply.

Keep new FastDB crates at version `0.0.0` and `publish = false` until an
explicit alpha packaging decision. These fields prevent accidental package
publication; they are not license restrictions. Record FastDB package
metadata as `license = "MIT"`. Do not publish, tag, or upload a release unless
the user explicitly authorizes that release operation.

## Repository and Upstream Policy

This workspace is to become one monorepo based on Turso's Git history. Do not create a nested Git repository, vendor a history-less engine copy, or use Turso as an unpinned submodule.

- Preserve the planning documents while importing the Turso history.
- Configure the official Turso repository as the `upstream` remote.
- Record the audited engine SHA in machine-readable project metadata and CI output.
- Keep FastDB Core crates in the same workspace. A future proprietary cloud
  service may live in a separate private repository and must not be required
  to build, test, or use Core.
- Regularly review upstream changes, but integrate only an exact audited SHA through a dedicated upstream-sync branch after relevant FastDB and unchanged Turso tests pass.
- Preserve public FastDB history with explicit upstream merge commits. Do not rebase or force-push shared branches to update the engine.
- Preserve unrelated user changes and never use destructive Git operations to simplify an import or update.

Detailed bootstrap instructions and the initial crate layout are in `plan-phase0.md`. The operational update procedure is in the mandatory `upstream-sync` skill; `UPSTREAM.md` remains the durable policy and pin record.

## Phase Boundaries

Keep work within the active phase unless the user explicitly changes scope.

| Phase | Purpose |
| --- | --- |
| 0 | Prove direct AST translation, atomic first mutation, reopen/delete, expression-index use, and baseline overhead without Turso core changes. |
| 1 | Build the independent parser/AST and explicit compatibility contract. |
| 2 | Build stable catalogs, storage translation, schema enforcement, and index definitions. |
| 3 | Complete the MVP CRUD, parameters, result, and transaction semantics. |
| 4 | Deliver the embedded Rust API and CLI. |
| 5 | Fuzz, crash-test, benchmark, document, and harden the MVP for release. |
| 6 | Transactionally migrate to format 2 and establish the sealed multimodel provider, AST, explain, and index-maintenance foundation. |
| 7 | Add typed relation records, fixed-depth traversal, two-way adjacency indexes, and atomic cascade behavior. |
| 8 | Add a characterized SurrealQL FTS subset plus a labeled FastDB/Turso FTS extension over one provider. |
| 9 | Add fixed-dimension native vector storage and exact bounded vector search; define the 0.1 alpha-candidate surface. |
| 10 | Add resource limits, deterministic close, backup/restore/check/rebuild tooling, and operational evidence. |
| 11 | Preserve the completed audit stop: no candidate qualified and no parallel-writer/API change was made. |
| 12 | Lock the compatibility inventory, migrate transactionally to format 3, and add richer collision-safe values. |
| 13 | Complete bounded expressions, operators, built-in functions, and deny-by-default external capabilities. |
| 14 | Complete CRUD, subqueries, aggregation, grouping, and richer query clauses. |
| 15 | Add scripting, custom schema behavior, views, functions, parameters, and atomic events. |
| 16 | Complete graph relations and bounded recursive/filtered traversal. |
| 17 | Complete analyzers/search and add FastDB-owned non-geospatial specialized indexes including ANN. |
| 18 | Add embedded authentication, principals, roles, and row/field/function authorization. |
| 19 | Add the secure single-database HTTP/WebSocket server over the FastDB API. |
| 20 | Add the multi-database control plane and read-only FastDB ATTACH/DETACH extension. |
| 21 | Add local/remote Rust, TypeScript, Go, and PHP SDKs through a versioned FastDB C ABI. |
| 22 | Resolve the locked matrix and pass broad-compatibility hardening without a production-ready claim. |
| 23 | Dormant Core 1.0 gate: re-audit parallel writers only when a new stable exact candidate exists. |

Phase 2 established stable format version 1, Phase 3 completed the synchronous
frontend contract, Phase 4 delivered the worker-backed asynchronous Rust API,
transaction guard, and CLI, Phase 5 completed local hardening, Phase 6
established format 2 plus the sealed provider and maintenance foundation, and
Phase 7 added graph records and bounded traversal, Phase 8 added dual-syntax
full-text search, Phase 9 added exact native vector scans, and Phase 10 added
bounded resource controls plus backup, restore, check, rebuild, lifecycle, and
observability tooling. Phase 11 completed its audit but stopped before
implementation. Phase 9 remains a historical alpha-candidate milestone and
Phase 11 remains a stopped audit, not a beta. Phases 12–22 form the approved
pre-1.0 compatibility track; Phase 23 is the dormant Core 1.0 gate. None
authorizes publishing. The proprietary cloud service remains separately scoped
and separately authorized.

## Cloud and Business Context

The planned business is a managed FastDB service at `cloud.fastdb.org`, but
cloud implementation is outside the active Phase 12–22 Core track and requires
a separately approved plan.

- Local and self-hosted Core use is available under the MIT License.
- Do not assume a permanent free managed tier; the initial hypothesis is bounded `$5`, `$20`, and `$100` plans, with a capped trial or one-time credit if economical.
- Pricing is a hypothesis, not a promise. Model storage, requests, compute, egress, backups, support, and abuse before publishing prices.
- Object storage does not make the service cheap by merely uploading live SQLite files. A safe design needs immutable blocks/segments, manifests, conditional publication, caching, recovery, compaction, and garbage collection.
- The initial `cloud.fastdb.org` target is Cloudflare: a Worker API gateway routes each database through a container-backed Durable Object to native FastDB/Turso running in a Cloudflare Container, with R2 holding ordered recovery artifacts and immutable generations.
- Worker temporary files and Container disks are ephemeral and never the sole durable copy of acknowledged data. Do not run the active mutable database directly on an R2 FUSE mount.
- Cloud evolves through research and staged architectures: C0 proves Cloudflare/R2 recovery and economics, C1 ships an eager-hydration native-container alpha, C2 adds lazy R2 segments and bounded disposable caches, and C3 hardens the object-native service.
- Keep storage/recovery behind a FastDB-owned object-store interface so AWS S3 and other compatible backends remain possible; Cloudflare types must not leak into FastDB Core.
- Future sync follows Turso's central-authority model: explicit push/pull/checkpoint, authoritative remote schema, and offline record-data mutations. Do not invent peer-to-peer replication independently of Turso.

## Extension Direction

Plan extensions around explicit capabilities and typed physical representations, not arbitrary loadable native code in the MVP.

- Ordinary scalar features may remain JSONB-only.
- Frequently queried typed values may use catalog-managed hidden/generated columns plus ordinary B-tree indexes.
- Specialized features may use a dedicated provider with typed encoding, lowering hooks, index lifecycle, planner support, and result decoding.
- Phase 8 FTS and Phase 9 exact vectors must use sealed cataloged providers and
  native hidden representations while preserving the public document model.
- Phase 9 uses stable Turso vector functions for exact scans. Phase 17 may add
  FastDB-owned HNSW/MTREE providers over catalog-derived state, but must not
  expose the pinned toy sparse-IVF method, Turso's experimental provider ABI,
  or an unqualified Turso core change. Public vectors remain arrays; hidden
  exact storage uses native `vector64`.
- A focused geospatial subset may later use canonical geometry/WKB plus selected predicates and spatial indexes.
- Full PostGIS compatibility is a separate major project and must not be implied by Turso's PostgreSQL syntax frontend.

## Engineering and Verification Standards

- Inspect the pinned source before coding; do not rely on remembered Turso APIs.
- Prefer the smallest implementation that proves the active phase without weakening production invariants.
- Add tests with every supported behavior and every rejected/unsupported syntax path.
- Test injection boundaries: parameters, identifiers, JSON paths, malformed input, depth/token limits, and transaction failures.
- Storage changes require reopen, rollback, integrity, and relevant crash/recovery evidence.
- Every declared index requires an execution-plan test demonstrating actual selection; index existence alone is insufficient.
- Run relevant unchanged Turso suites whenever frontend work crosses JSONB, optimizer, transaction, WAL, or I/O behavior.
- Benchmarks compare equivalent physical schemas, values, durability, and result materialization. Preserve commands, environment, raw measurements, and ratios.
- Do not claim “production-ready,” complete compatibility, cloud readiness, ACID certification, or absolute performance without published evidence.
- Keep Phase 11's failed audit as historical evidence. Do not promote
  experimental MVCC on product documentation alone; dormant Phase 23 must
  repeat the exact-SHA audit from scratch when a new stable candidate exists.

Use the exact commands required by the active phase plan. Discover package names with `cargo metadata`; at minimum, finish applicable formatting, linting, targeted tests, integration tests, unchanged upstream regression tests, and release-mode benchmarks.

## Agent Workflow and Handoff

For each task:

1. State the phase and the narrow deliverable.
2. Inspect existing code, worktree changes, applicable instructions, and pinned upstream APIs.
3. Implement without broadening the compatibility surface.
4. Update `COMPAT.md`, design notes, research notes, or format documentation when behavior changes.
5. Run verification proportional to the risk and preserve evidence required by the active phase.
6. Review the diff for generated SQL, provenance, accidental core edits, silent syntax acceptance, and unrelated changes.
7. Report the outcome, commands run, remaining risks, and any stop condition.

Phase completion requires every checkbox in that phase's Definition of Done,
not merely working happy-path code. Phase 5's original release stop is retained
as historical evidence in its report and superseded by the MIT/private-cloud
decision recorded in `docs/licensing.md` and the current gates in
`docs/release-readiness.md`. The phrase “production-ready FastDB Core 1.0” is
reserved until dormant Phase 23 and its subsequent release gate pass; tagging,
publishing, signing, and uploading remain separately authorized operations.

Update this file only when durable project-wide decisions change. Put detailed implementation recipes in phase plans, observed results in reports, and temporary work status in normal task tracking.
