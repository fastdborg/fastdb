# Proposed pinned-engine trigger interruption fix

Status: observed V1 release gate; core patch proposed, not applied or validated. The project workflow requires a separately reviewed design decision for upstream core exceptions.

An AFTER INSERT trigger inserts into an effects table and invokes a test-only counter. A progress handler interrupts after that first callback. The pinned engine reports Busy instead of Interrupt. In the first native/autocommit probe both target and effects tables were empty afterward; explicit-transaction and remaining after-write cases must still be qualified after the error propagation fix.

The cause is `core/vdbe/execute.rs`, OpProgram's `step_subprogram()` handling: StepResult::Interrupt and StepResult::Busy share a branch that returns LimboError::Busy. The frontend row callback adapter cannot recover the original error kind after this conversion. Relabeling all Busy errors would misclassify real contention.

[Proposed patch](trigger-interrupt.patch) preserves the existing saved subprogram state and returns Interrupt for the Interrupt variant, retaining Busy for contention. It changes no encoding or public API. Error cleanup may differ because the engine distinguishes Busy from other failures; therefore rollback/transaction qualification is required before acceptance.

## Reproducer and required verification

Run from the repository with the pinned Rust toolchain/cache environment:

```sh
cargo test --locked -p fastdb --lib native_target_after_write_cancellation_requires_interrupt_propagation -- --ignored
```

This test intentionally remains ignored in routine checks while the pinned defect exists. It requires FDB_CANCELLED, empty target/effects tables, matching native/logical transaction and prior-work observations, intact collection sources and exact retry/rollback. It adds eight native/logical after-write pairs across four subquery forms and autocommit/explicit transactions. The existing source-boundary test remains enabled and now also checks trigger side effects during retry.

Before accepting a core exception: review/apply the isolated patch, enable the release-gate regression, run it and the scoped FastDB suite, run `cargo test --locked -p core_tester --test integration_tests trigger::` (the pinned tests/integration/trigger.rs suite), and record the exception in fastdb/UPSTREAM.md. The proposed patch has only been checked for clean application, not built or run.

Evidence: /tmp/fastdb-native-write-cancel-probe.log and /tmp/fastdb-native-write-cancel-state.log. The observed native/autocommit case returned FDB_BUSY after exactly one trigger callback, with autocommit state and empty target/effects tables.
