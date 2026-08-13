# FastDB Phase 13 — Expressions, operators, and built-in functions

Status: authoritative implementation plan; implementation not started

Starting checkpoint: `9627c82cd`

## 1. Purpose and boundary

Phase 13 completes the locked `v3.1.5` expression/operator and built-in
function capabilities assigned to Phase 13. It builds one bounded FastDB
semantic evaluator shared by projection, predicate, mutation, closure, custom
schema, event, permission, and later protocol execution. Exact Rust semantics
remain authoritative where Turso cannot express the characterized behavior
without generated SQL.

The phase retains format 3, stable full-durability WAL, serialized writers, the
engine pin `977383ff40edc44ef410af062ed0d2322252a869`, and the official
unmodified SurrealDB `v3.1.5` behavioral reference. No inherited Turso source,
format migration, server, authentication policy, remote SDK, multiprocess
mode, parallel writer, geospatial value, history/changefeed/LIVE behavior,
GraphQL, GQL, tag, publication, artifact upload, or production-ready claim is
authorized.

Phase 14 statement-level query completeness, Phase 15 persisted custom
functions/events/schema expressions, and Phase 18 authenticated session state
remain outside this phase. Phase 13 may implement the bounded expression
primitive needed by a later phase, but must not silently publish that later
statement or security surface.

## 2. Clean-room characterization

Use public documentation and independently designed probes against the
verified external `v3.1.5` binary only. Record version, archive checksum,
observation date, exact input, normalized output/error class, and public links
in `docs/compat-research/phase13.md`. Do not inspect or adapt SurrealDB source,
tests, fixtures, expected outputs, or corpora.

Probe by semantic equivalence class rather than copying documentation examples:

- operand type matrices, NONE/NULL propagation, numeric overflow, ordering,
  negative indexes, omitted slice bounds, Unicode, empty collections, and
  malformed casts;
- function arity/type/error behavior, aliases, deterministic ordering,
  bounded output, non-finite prevention, and context-dependent results;
- random and password functions for shape/range/security properties rather
  than exact bytes;
- outbound functions through a FastDB-owned local adversarial test server that
  exercises redirect, DNS/address policy, timeout, size, and concurrency
  boundaries without depending on public services.

Moving online documentation does not change the pinned contract. Any
undocumented ambiguity must be resolved by a recorded probe before promotion.

## 3. AST and parser contract

Extend the engine-independent AST with explicit nodes for:

- array/string/set indexing and half-open slicing with omitted bounds;
- chained object/collection access without flattening it to an interpolated
  JSON path;
- casts carrying a structured Phase 12 schema/value type;
- range construction with explicit included/excluded/unbounded endpoints;
- the locked comparison, exact-equality, containment, all/any/none,
  null-coalescing, truthy-coalescing, modulo, and power operators;
- closure literals with an explicit ordered parameter list and bounded body;
- future blocks as inert structured expressions only where their Phase 13
  value semantics are independently characterized; and
- subquery-expression structure needed by the locked expression row, without
  broadening Phase 14 SELECT/CRUD syntax.

Pratt binding powers and associativity are independently tested at every
operator boundary. Chained postfix access has a dedicated parser loop.
Unsupported variants fail with source spans; no clause is accepted and ignored.
Parser limits cover closure parameters, access-chain length, and expression
nesting within existing byte/token/collection ceilings.

## 4. Bounded semantic evaluator

Create a FastDB-owned evaluator module with an explicit evaluation context and
budget. The budget counts expression steps, call depth, closure invocations,
visited collection elements, produced elements, string/byte output, regex
work, and nested evaluation. Checked arithmetic prevents counter overflow.
Default and hard ceilings integrate with `ResourceLimits`; a breach is
`ResourceLimit`, poisons an explicit transaction, and never exposes source or
parameter values.

Preserve the distinction between NONE and NULL. Define one canonical total
order shared by comparisons, sets, distinct/group helpers, min/max/sort, and
later query ordering. Equality and exact equality use separately characterized
numeric/type behavior. Logical and coalescing operators short-circuit. Integer,
decimal, duration, datetime, string, array, set, and range arithmetic is
enabled only for characterized combinations; overflow, division/modulo by
zero, non-finite results, incompatible bounds, and invalid casts fail exactly
as recorded rather than relying on host-language panics or lossy coercion.

Evaluation never renders or executes SQLite text. Safe predicate pushdown is
allowed only when the direct Turso AST has the same NONE/NULL/type/error
semantics; otherwise FastDB materializes bounded candidates and evaluates the
authoritative expression in Rust. Every pushed expression has an equivalence
test and every claimed index still has an executed-plan assertion.

## 5. Built-in registry

Add a closed, typed registry keyed by canonical lower-case namespace segments.
Each entry declares aliases, arity, evaluation class, argument/output limits,
context requirements, and implementation version. Unknown functions and wrong
arity are explicit errors before mutation. There is no plugin ABI or dynamic
native code.

Implement and characterize the locked Phase 13 families in focused batches:

1. array, object, set, value, bytes, base64, JSON, and CBOR helpers;
2. numeric operators plus math constants, scalar math, and bounded aggregate
   math helpers;
3. duration and time extraction/construction/rounding/formatting functions;
4. strings, parsing, validation, semantic-version, similarity, regex, and
   Unicode-safe access helpers;
5. type conversion/introspection, record/meta helpers, and context-neutral
   session helpers;
6. cryptographic digests, secure password hashing/comparison, and bounded
   cryptographically secure random values; and
7. capability-gated API, HTTP, file, eval, sequence, sleep, and other
   resource/context functions.

Closures are lexical, cannot mutate captured values, and use the same budget.
Collection helpers preserve characterized order and duplicate semantics.
Hash/password comparison is constant-time where the selected primitive permits.
Password functions use bounded work parameters and never log inputs or encoded
hashes. Random functions use an operating-system CSPRNG, enforce size/range
ceilings, and are tested by invariants rather than deterministic output.

`encoding::json::*` uses the collision-safe public value mapping rather than a
lossy host JSON shortcut. CBOR has a strict FastDB-owned value mapping and
bounded decoder. Regex compilation and matching are bounded and reject
unsupported constructs; no unbounded backtracking engine is introduced.

## 6. Outbound capability policy

Outbound/resource behavior is disabled by default. Extend builder/query
options with a closed capability policy that can independently allow HTTP(S),
file buckets, API invocation, dynamic evaluation, sleep, and sequences. A
missing capability returns `Capability` before external work.

HTTP(S) policy must:

- permit only configured schemes, ports, methods, host patterns, and response
  content types;
- resolve every hop, reject credentials in URLs, fragments where irrelevant,
  non-canonical hosts, and private, loopback, link-local, multicast,
  documentation, benchmark, unspecified, or otherwise denied addresses unless
  that exact range is administratively allowed for testing;
- connect only to an address from the validated resolution set, revalidate
  redirects from scratch, cap redirects, and prevent DNS rebinding;
- bound connect/read/overall time, request/response headers and bodies,
  decompressed bytes, and concurrent requests; and
- redact authorization, cookies, URL userinfo/query secrets, bodies, and
  parameters from errors, tracing, and hooks.

File functions operate only through registered opaque bucket capabilities.
They never accept an arbitrary host path. Bucket implementations canonicalize
keys, prevent traversal/symlink escape, enforce read/write/byte/concurrency
limits, use no-overwrite/atomic-publication semantics where named, and are
disabled until explicitly registered. `eval::surql` reparses through the
FastDB parser with reduced nested budgets and never reaches Turso SQL parsing.
Sleep consumes the query deadline and has a small hard ceiling. Sequence
operations are serialized FastDB catalog operations and join the active
transaction; they are not inherited Turso SQL bindings.

If these guarantees cannot be met without a prohibited core change or an
unsafe dependency, the affected atomic capability receives an architecture
stop report; no weaker network or filesystem behavior is published.

## 7. Inventory and evidence rules

At Phase 13 start, set inventory `active_phase = 13` and mark only capabilities
under current implementation as Partial. A row becomes Supported only when its
parser and executable evidence names pass in normal tests. Unknown or deferred
behavior remains Unsupported, never accepted as a no-op.

The Phase 13 checkpoint has no Phase 13 Partial row. Every Phase 13 target is
Supported or has an architecture stop report satisfying the roadmap's narrow
criteria. Granularity, IDs, phase assignment, and exclusions remain locked;
`COMPAT.md` is regenerated only by the checked repository tool.

## 8. Implementation order and commit boundaries

1. Commit this plan alone.
2. Commit AST/parser/operator structure and the evaluator budget.
3. Commit operators, postfix access, casts, ranges, closures, and their
   conformance matrix.
4. Commit deterministic built-in batches, with inventory promotion only after
   each batch's executable evidence passes.
5. Commit secure random/crypto behavior and separately reviewed dependencies.
6. Commit deny-by-default capability configuration and hardened resource
   providers/functions.
7. Run the complete gate, write `docs/phase13-report.md`, update durable status,
   and commit `phase 13 complete` with start/end SHAs and rollback range.

No Phase 14 plan or behavior starts before the Phase 13 checkpoint. Rollback is
by explicit Git revert of the recorded Phase 13 range.

## 9. Verification matrix

At minimum, independently authored tests cover:

- every operator's precedence, short-circuiting, supported type matrix,
  NONE/NULL distinction, exact/value equality, bounds, overflow, and errors;
- positive/negative indexing, Unicode scalar slicing, omitted/clamped bounds,
  nested access, missing paths, casts, set/range containment, and canonical
  ordering in memory/on disk, standalone/in transactions, and after reopen;
- each supported function and alias with arity/type/limit/error evidence, plus
  table-driven cross-checks against recorded clean-room observations;
- closure capture/arity/depth/step/output limits and cancellation;
- hash vectors from public primitive standards, password generation/compare
  success/failure/work bounds/redaction, and random shape/range/uniqueness
  smoke tests without flaky distribution claims;
- strict JSON/CBOR/base64 malformed-input and allocation bounds;
- regex/Unicode/URL/email/semver adversarial inputs and no panics;
- deny-by-default external calls, allowlisted local success, every forbidden IP
  class, mixed DNS answers, rebinding, redirects, compression bombs, slow
  responses, oversized headers/bodies, concurrency saturation, cancellation,
  bucket traversal/symlink races, and secret-free errors/hooks;
- transaction poisoning for evaluation/resource failures and no partial
  document/index/provider/catalog mutation;
- direct-AST/index-plan preservation for every safely pushed expression;
- deterministic `COMPAT.md`, no completed Phase 13 Partial row, fixtures,
  backup/restore, check/rebuild, abrupt exit, and unchanged Phase 6–12 behavior;
  and
- full FastDB formatting/lint/test/fuzz/release-build/benchmark gates plus the
  relevant unchanged Turso core, PostgreSQL, and Whopper suites.

## 10. Stop conditions

Stop the affected capability and write an architecture report rather than
weakening the contract if implementation would require generated SQLite text,
logical identifier interpolation, arbitrary native plugins, copied SurrealDB
material, an unqualified Turso core change, unbounded evaluation/regex/decode,
ambient filesystem/network authority, secret-bearing diagnostics, or semantics
that cannot be made atomic and authorization-ready.

Stop the entire phase if the shared evaluator cannot preserve Phase 3
transaction poisoning, Phase 7 graph, Phase 8 FTS, Phase 9 vector, Phase 10
limits/backup/check, Phase 12 format/value behavior, or acknowledged stable-WAL
durability.

## 11. Definition of done

Phase 13 is complete only when the locked Phase 13 inventory has no Partial
row, every Supported row names passing parser/execution evidence, every
remaining Unsupported Phase 13 target has an approved architecture stop, the
deny-by-default resource boundary passes adversarial security tests, and the
complete regression/performance matrix passes with exact commands and raw
measurements in `docs/phase13-report.md`.

Completion is a local pre-1.0 compatibility checkpoint only. It does not
authorize Phase 14, a package release, parallel writers, or a production-ready
Core 1.0 claim.
