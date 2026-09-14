# Trigger interruption core exception for review

Candidate based on 9a82a36ea; isolated from main. Requirement S3: preserve
cancellation errors and transaction cleanup when a native trigger is interrupted.

The OpProgram executor currently maps both StepResult::Interrupt and Busy to
LimboError::Busy. The patch preserves saved subprogram state and returns Interrupt
for an interrupt; genuine contention continues returning Busy. No storage format
or public API changes. Frontend relabeling cannot safely distinguish the two.

Validation on Rust 1.88.0, Linux x64:
- Previously ignored after-write regression: passed, then enabled in the candidate.
- Pinned upstream trigger suite: 98 passed, zero failed.
- Full FastDB scoped suite: 675 Rust tests passed, zero ignored; 103 Node tests
  passed; formatting, Clippy and strict TypeScript passed.

Logs: /tmp/fastdb-trigger-review.log, /tmp/fastdb-trigger-upstream.log,
/tmp/fastdb-trigger-scoped.log. Dev/test debug symbols and incremental compilation
were disabled; test scope and debug assertions were preserved.

This validates the named defect and the required surrounding suites. It is not
an interrupted commit/checkpoint or arbitrary cancellation-boundary guarantee.
Acceptance requires recording the core exception in UPSTREAM.md and integrating
this isolated candidate. Main and published release artifacts remain unchanged.
