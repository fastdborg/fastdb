# Bounded SELECT results

Rust `Connection::select_with_limits(sql, params, ResultLimits)` and
`profile_select_with_limits` now enforce explicit `max_rows` and
`max_payload_bytes` on native and typed SELECT results. Existing APIs retain
unbounded result behavior. Overflow returns `FDB_LIMIT`, discards the result,
and stops collection at the first rejected row. Column metadata is charged even
for empty results. Non-SELECT statements are rejected by these APIs.

One-hop FETCH shares the final payload budget. Primary collection charges row
counts and non-FETCH columns; reference placeholders are excluded. The resolver
counts reference occurrences, then charges each found target with its output
multiplicity before retaining it in the target cache. Repeated reads of the same
stored identity are charged once. Missing targets and null references are charged
after target resolution. All final payload checks precede output cloning. Its independent
16,384-reference and 64 MiB tagged-JSON workspace/output limits still apply.
Temporary references and the deduplicated target cache are not covered by the
final payload budget. Fetch queries retain their atomic snapshot scope.

Payload accounting uses UTF-8 byte lengths for column names, strings, object
keys and record table/string keys; null and booleans cost one byte; numbers and
integer record keys cost eight bytes. Binary and encoded vector bytes count
verbatim. Arrays and objects sum their contents, with no container overhead.
Arithmetic is checked. This bounds retained logical payload, not allocator
usage, the temporary decoded row, engine working memory, or execution time.

Node clients expose `selectWithLimits` and `profileSelectWithLimits` with
required bigint `maxRows` and `maxPayloadBytes`; worker calls accept AbortSignal.
RETURNING, INSERT SELECT candidates and transfer remain separate integration
work. This does not close the V1 resource gate.

## Atomic write-result policy

Rust `Connection::write_with_result_limits(sql, params, ResultLimits)` accepts
one SQL INSERT/UPDATE/DELETE or supported object write. It runs inside an
operation savepoint, checks the completed returned result with the same row and
payload accounting, and releases only on success. A result limit failure rolls
back that operation, including data and managed indexes, while preserving prior
pending work when savepoint recovery succeeds. Existing engine/rollback errors
retain their own error handling and transaction disposition.

This is a final-result acceptance policy, **not a bound on write candidate or
RETURNING materialization memory**. It must not be used as a process memory cap.
Transaction control, schema operations, reads and multi-statement input are
rejected. Writes without RETURNING can succeed with zero budgets; affected rows
are not returned rows. Empty RETURNING results still charge column metadata.
Node exposure, cancellation qualification and progressive write collection
remain unfinished.

The original integration inventory follows; its next-step language describes
the design preceding the initial Rust implementation.

## Result budgets — implementation design

Status of the inventory below: design notes retained for remaining integration work; the Rust SELECT subset above is implemented.

## Current execution paths

| Path | Current owner | Accounting concern |
|---|---|---|
| Ordinary SQL results and profiling | `Connection::native_profiled` in `frontend/src/lib.rs` | Collects engine rows, then constructs public values; transient and public representations can overlap. |
| Collection SELECT and profiling | `Connection::execute_lowered_profiled` in `frontend/src/select.rs` | Decodes typed values per row, then appends to a public result vector. |
| Forward FETCH | SELECT collector plus `frontend/src/links.rs` | The outer result survives while target records are resolved. Existing 16,384-reference and 64 MiB resolver limits do not bound the complete result. |
| INSERT SELECT | `Connection::insert_select` in `frontend/src/select.rs` | Source candidates are materialized before writes. They are statement workspace, not simply returned rows. |
| Collection writes and RETURNING | `frontend/src/write.rs`, `update.rs`, and object statements in `lib.rs` | A result limit reached after mutation must not leave a partial statement reported as a harmless read failure. |
| Internal catalog/validation reads | `Connection::run` and shared `collect_rows` | User result limits must not silently truncate these rows or change constraint decisions. |
| Transfers | `frontend/src/transfer.rs` | Existing incremental encoding and transfer bounds are separate from general SELECT result budgeting. |

The common `collect_rows` function is therefore not a sufficient single enforcement point. Applying a global row cap there would affect internal database operations while missing typed SELECT collection and FETCH expansion.

## First implementation step

Introduce an explicit bounded SELECT operation using the existing SELECT-only preparation checks in `profile_select_inner`. Share one collector budget between native and logical SELECT execution, and expose the same policy to profiling. Reject non-SELECT statements before running them. Keep existing APIs and their current behavior until the broader policy is decided and implemented.

Define separate maximum returned-row and logical-payload-byte counters. Count column names and recursively count owned value payloads, including object keys, record table/key strings, arrays, binary and vector data. Use checked arithmetic. Document that these counters bound returned logical data, not Rust allocator overhead or the engine's sort/group/CTE working memory. Check the next row before retaining it, and discard the result on overflow with `FDB_LIMIT`; never return a truncated successful rowset.

Forward FETCH requires the same final-result budget before retaining resolved values. A read-only first step can reject FETCH until this accounting is integrated, but that restriction is unfinished work rather than completion of the V1 link/resource contract.

## Required verification

- Native and collection results: zero limits, exact row/byte boundaries, one-over boundaries, empty rows and empty result metadata.
- Exact payload accounting for multibyte text, nested objects/arrays, records, binary and each vector encoding; overflow-safe arithmetic.
- A volatile callback confirms collection stops at the first rejected row and no later row executes. Planning executes no callbacks.
- Error provenance remains `FDB_LIMIT`; engine `Interrupt`, `Busy` and transaction observations retain their own meanings.
- Rejected read results preserve prior pending writes and allow a valid query retry. Non-SELECT input performs no mutation.
- Sync and worker clients use the same limits, with cancellation and lifecycle checks, strict TypeScript and installed-package coverage.
- FETCH, write candidates and RETURNING receive explicit integration and atomicity tests before claiming general query budgeting.

## Remaining V1 resource work

Returned-result limits are only one part of the gate. Engine working memory, statement candidate buffers, deadlines, interrupted I/O and commit/checkpoint outcomes, transfer peak memory, and platform qualification remain required. A green bounded-SELECT test cannot establish a total-memory guarantee.
