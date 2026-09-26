# Execution metering exception

Approved by the user on 2026-09-26 for Cloud 0.3.0. This work does not reopen the
completed native 2.1.0 release or change its historical acceptance record.

The released counters undercount seek-based reads. A forty-row indexed self-join
returned forty matches but reported rows_read=39 and btree_seeks=41. B-tree seeks
cannot simply be added to rows_read: missing keys and deferred-but-unused work
must not be billed as row visits.

The first isolated patch counts a completed successful rowid seek, successful
shared index/range positioning, and an actually executed deferred table seek.
It increments only after I/O completion, never on the yielded I/O path. Failed
lookups do not add a row. Existing query results and seek statistics are unchanged.
Generic engine counters contain no organization, price or cloud-specific state.

Regression: `cargo test --locked -p fastdb-tests --test profile` covers memory and
file-backed successful/missing point seeks, ascending/descending range seeks,
covering/non-covering indexes and the forty-row indexed join (eighty visits).
Existing scan-reduction and result-equivalence checks remain in that suite.
The private cloud has a separate native diagnostic reproducer and runtime suite.

A second isolated patch adds `execution_meter::ExecutionMeter`, attached through
`Connection::set_execution_meter`. Each program captures the optional Arc when
execution starts. Completed row-read/write events update it directly, so snapshots
survive statement errors, interruption, reset, rollback and drop. It preserves
physical write work on rollback; those counters must never be described as
committed logical mutations. All programs, including internal programs, contribute.
The existing statement and connection profiling APIs remain unchanged.

An optional VM-step budget reserves each dispatch atomically before execution.
Zero interrupts before dispatch, exactly enough permits completion, and exhaustion
uses the existing interrupt/abort path. The budget spans sequential statements;
replacing a meter while a root statement is active is rejected. Callers must
serialize replacement with execution and detach before unmetered cleanup. Final
snapshots require quiescent execution; concurrent field reads are provisional.
Waiting on unfinished I/O does not consume additional VM steps. Parsing, planning
and work within an opcode are not bounded by this facility.

`fastdb-tests --test execution_meter` covers memory/file-backed failure retention,
partial-result drop, reset/rebinding, explicit rollback, zero and exact VM limits,
interrupted physical writes with rollback, EXPLAIN modes, and deterministic queued
I/O polls/resumption using the existing upstream I/O test harness. Adjacent profile,
deadline, cross-thread interrupt, result-limit and transaction regressions pass.

A third isolated patch adds optional read budgets. Each completed instrumented
visit is retained, then an over-budget event interrupts immediately. The scope is
sticky: subsequent statements cannot resume work after exhaustion. In a serialized
execution the maximum overshoot is one counted visit, including zero-budget
execution that reaches an existing row. Missing rows and empty scans count zero.
Sharing a meter across concurrent VMs permits one crossing visit per active VM;
cloud admission must serialize each execution and include the documented bound.
The approved cloud plan explicitly permits documented bounded overshoot. Counts
are never clipped, and the ledger must settle the actual retained value.

With a meter attached, exact Count opcodes advance one row per VM dispatch rather
than reporting the whole count after an uninterruptible batch. The phase survives
I/O and resets with the statement. VM/deadline cancellation remains available
between advances. Default unmetered Count retains its optimized path. Tests cover
exact/short/zero read budgets for table scans, covering and deferred index reads,
Count results/reset/empty input, interrupted writes, and partial Count reads on
injected I/O failure. Waiting for pending I/O never repeats completed reads.

This bounds the existing instrumented visits; it does not establish complete
billable coverage or bound arbitrary work inside virtual-table extensions.
Complete cursor coverage (including hash joins), managed-write/linked-fetch
attribution, internal-work exclusion, DDL accounting and durable cloud settlement
remain required before paid activation.
Hash joins, sort/materialized paths, virtual/search indexes and suspended-I/O
edge cases require their own broader accounting audit; this patch does not claim
complete billable counters for all paths.

Retain this exception until upstream counts successful cursor positioning and
actually performed deferred table reads equivalently and these regressions pass
without the patch. Review counter and budget changes separately from upstream
merges; preserve ancestry and maintain an isolated commit.


## Hash-build duplicate accounting

A fourth isolated correction removes the read event from HashBuild. That opcode
copies keys/payload already read into registers and uses the currently positioned
source rowid; it does not advance or seek the source cursor. Its Rewind/Next/seek
already recorded the visit. A verified non-spilling 100-by-100 hash join reported
300 visits with zero B-tree seeks and 198 full-scan steps; the correct total is
200 (two initial positions plus 198 advances).

Memory and file-backed regressions assert the HashBuild/HashProbe plan, unchanged
results for unique and duplicate keys (100 and 1,000 matches), zero source seeks,
and exactly 200 visits. A retained-meter regression succeeds at a 200-read budget
and interrupts at 199 with the crossing 200th visit retained. Hash-grace spill
paths, temporary/materialized traversal and virtual/search indexes still need
broader attribution qualification; this correction is not evidence for those paths.


## Read-budget interruption and caller transactions

Frontend integration exposed a distinction between engine error and cooperative
interruption cleanup: a read-budget error could roll back prior work in an
explicit transaction. The fifth isolated correction routes an Interrupt from an
already-interrupting program through the existing cooperative abort path, matching
VM/deadline cancellation. Ordinary engine errors retain their existing handling.
A raw-engine regression proves retained crossing reads, an active caller transaction,
prior writes still visible after interruption, and explicit ROLLBACK restoring the
original rows. Frontend tests also cover result-budget and linked-fetch failure.

## Checked frontend read adapter

`Connection::select_metered` accepts checked native/collection SELECTs and direct
record SELECTs, result limits and optional read/VM work limits. Its result retains
`ReadWork` alongside the success or error outcome. A connection-local scope shares
one meter across primary statements and forward/inverse linked target statements.
The meter is attached only for those statement executions. Catalog discovery,
preparation probes and savepoint cleanup remain outside it. A callback failure is
reset before detachment; cleanup failures remain explicit rollback errors. Calls
on a connection must remain serialized, as for existing frontend APIs.

Direct records use a checked primary-key lookup with LIMIT 1, preserving the
existing document result shape while avoiding an unnecessary next-index boundary
visit. Read/result-budget failures retain visits and permit later operations.
Tests prove failed-expression retention, metadata exclusion despite additional
catalog entries, direct-record missing/found behavior, result limits, zero VM
budget, mutation rejection, native/collection forward fetches, inverse fetches,
and preservation of prior transaction work across budget failures.

This adapter is a prerequisite for cloud receipts, not proof of complete billing
coverage. Physical counter coverage for spills/materialization/search still needs
qualification. Logical writes, DDL/import attribution, internal execution retries,
durable receipts and organization settlement remain separate release gates.

## Retained row mutations and mutation budgets

The next isolated continuation adds `row_mutations` and an optional
`max_row_mutations` limit. Ordinary B-tree inserts, matched updates (including
no-op updates) and deletes emit one event. Moving an updated row emits one event;
REPLACE emits a separate event for each conflicting row removed. Trigger writes
share the meter. SQL `changes()` and `total_changes()` retain their existing
semantics. Index opcodes, schema/ephemeral rows, internal sequence maintenance
and change-capture inserts do not add mutation events.

The crossing event is retained and interrupts immediately, with at most one
extra event per serialized VM. Exhaustion is sticky. Trigger budget failures
propagate through cooperative cancellation, undoing the interrupted statement
while preserving prior caller transaction work. Reset/drop an interrupted writer
before replacing or detaching its meter; the API rejects replacement while a
root statement or writer remains active. Waiting on pending I/O cannot repeat a
completed event.

These are completed mutation events, **not committed writes**. Rollback preserves
the diagnostic count. Cloud must verify transaction outcome before reporting
committed writes and must separately exclude managed-document index maintenance,
which uses ordinary hidden SQL tables. Virtual-table writes, materialized-view
attribution and DDL/import billing remain unqualified. This patch does not expose
a checked write adapter or enable cloud paid billing.

Five `mutation_meter` regressions cover memory/file operation counts, conflict
replacement, no-op/rowid updates, RETURNING, index/temporary-work exclusion,
zero/exact/short budgets, explicit rollback retention, trigger rollback and caller
transaction preservation, change-capture modes, and deterministic queued I/O.
The selected integration suites pass 167 distinct tests (the broad run contains
166; the final five-test mutation suite adds the queued-I/O regression), and the
final frontend library run passes 92 tests. No runtime artifact is rebuilt by
this engine-only milestone.
