# Phase 14 Architecture Stops

Status: Phase 14 checkpoint evidence

Reference: unmodified SurrealDB `v3.1.5`

## Versioned CREATE clause

Capability: `STMT-CREATE-COMPLETE`.

The locked row combines the supported Phase 14 CREATE surface with a
`VERSION` clause. The immutable reference accepts a datetime-valued VERSION
clause, but that behavior belongs to versioned record history. Versioned
history, changefeeds, time-series retention, and historical reads are explicit
roadmap exclusions. FastDB format 3 therefore has no authoritative historical
record store against which this clause could execute.

FastDB executes single, batch, range, array, and bound-expression CREATE
targets; duplicate conflicts; CONTENT/SET; every supported return mode; and
bounded TIMEOUT. It deliberately rejects `CREATE ... VERSION ...` at parse
time. Accepting and ignoring VERSION would silently lose the caller's temporal
semantics, while adding an isolated timestamp field would not implement the
reference's versioned-history contract or its recovery/backup behavior.

Because inventory granularity is locked, the combined
`STMT-CREATE-COMPLETE` row remains Unsupported with this stop even though its
non-versioned component behaviors have executable evidence and are represented
by narrower Supported CREATE/value/clause rows. Reopening the row requires an
explicit roadmap change that brings versioned history into scope, a storage
format migration, historical-read semantics, retention/recovery evidence, and
new clean-room conformance probes. It must not be reopened by weakening or
renaming the row.
