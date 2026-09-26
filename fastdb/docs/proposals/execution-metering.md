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

This is only the first part of the approved metering design. Complete failed-query
counters, an optional execution budget, managed-write/linked-fetch attribution,
DDL accounting and durable cloud settlement remain required before paid activation.
Hash joins, sort/materialized paths, virtual/search indexes and suspended-I/O
edge cases require their own broader accounting audit; this patch does not claim
complete billable counters for all paths.

Retain this exception until upstream counts successful cursor positioning and
actually performed deferred table reads equivalently and these regressions pass
without the patch. Review counter and budget changes separately from upstream
merges; preserve ancestry and maintain an isolated commit.
