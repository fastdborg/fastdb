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

Read budgets are still required: one opcode can report a batch of reads (notably
Count), so a VM budget is not a row-quota substitute. Complete cursor coverage,
managed-write/linked-fetch attribution, internal-work exclusion, DDL accounting
and durable cloud settlement remain required before paid activation.
Hash joins, sort/materialized paths, virtual/search indexes and suspended-I/O
edge cases require their own broader accounting audit; this patch does not claim
complete billable counters for all paths.

Retain this exception until upstream counts successful cursor positioning and
actually performed deferred table reads equivalently and these regressions pass
without the patch. Review counter and budget changes separately from upstream
merges; preserve ancestry and maintain an isolated commit.
