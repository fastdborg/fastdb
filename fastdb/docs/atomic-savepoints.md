# Atomic savepoint identity and cancellation

FastDB atomic operations now allocate a distinct reserved savepoint name per connection operation, using a checked monotonic counter. This applies to nested operations such as importing multiple documents. Exhausting the counter returns FDB_LIMIT before opening a savepoint.

Previously every frame was named __fastdb_statement. If interruption was reported after an inner SAVEPOINT had opened but before its callback began, the inner atomic call returned early. Outer cleanup then rolled back to the newly opened inner frame with the same name, leaving earlier writes in place while returning FDB_CANCELLED.

A deterministic regression sweeps 48 engine progress boundaries after the first write in nested atomic operations. Before the fix, boundary 4 returned FDB_CANCELLED while retaining the first write. Unique names make outer cleanup target its own frame, including any unfinished nested frames. The regression verifies that FDB_CANCELLED preserves prior rows and the active outer transaction, successful operations contain both writes, and rollback errors never leave only a partial write set in the covered cases.

The failure was discovered by the existing Node cancelled-import test: two imported rows survived cancellation. A timing-based rerun passed, so the deterministic regression is the primary evidence for the cause and fix. Targeted before/after logs are /tmp/fastdb-nested-atomic-cancel.log and /tmp/fastdb-nested-atomic-cancel-fixed.log.

This does not claim complete cancellation or commit-outcome qualification. A cancellation reported around RELEASE may produce FDB_ROLLBACK and a completed or restored write set; callers must inspect transaction/error reports. Top-level savepoint-open boundaries, interrupted I/O/commit behavior and the separate pinned trigger Interrupt-to-Busy defect remain qualification work. No upstream engine files or persisted data encodings changed.

Verification of the combined working tree passed fastdb/scripts/check.sh: 308 Rust tests, 35 Node tests, formatting, Clippy and strict TypeScript (/tmp/fastdb-unique-savepoints-check.log). The Node transfer cancellation test then passed three additional isolated runs (/tmp/fastdb-transfer-cancel-repeated.log). The known trigger-interruption gate remains ignored.
