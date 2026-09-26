//! Optional counters retained independently of statement success and lifetime.
//!
//! These describe engine work, not logical committed writes or billable usage.
//! All programs started on an attached connection contribute, including internal
//! programs. Existing statement metrics and their coverage are unchanged.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionSnapshot {
    pub rows_read: u64,
    /// Physical row-write events, including work subsequently rolled back.
    pub rows_written: u64,
    /// Completed row mutation events, including no-op updates and replacement
    /// deletions. Retained after rollback; these are not committed writes.
    pub row_mutations: u64,
    /// VM dispatch attempts, including attempts that yield for I/O.
    pub vm_steps: u64,
    pub vm_budget_exhausted: bool,
    pub read_budget_exhausted: bool,
    pub mutation_budget_exhausted: bool,
}

/// Optional execution limits. Read limits interrupt immediately after the first
/// completed visit beyond the allowance, retaining that visit in the snapshot.
/// Overshoot is at most one counted visit per concurrently executing VM; callers
/// requiring the single-visit bound must serialize all users of this meter.
/// Mutation budgets likewise retain the crossing event and interrupt before the
/// statement completes. Finalize/reset interrupted writers before scope changes.
/// Mutation events survive rollback and must not be reported as committed writes.
/// This bounds instrumented work, not arbitrary work inside extensions.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecutionLimits {
    pub max_vm_steps: Option<u64>,
    pub max_rows_read: Option<u64>,
    pub max_row_mutations: Option<u64>,
}

/// A fresh meter can span several sequential statements. Retain its `Arc` to
/// inspect work after failure, rollback, reset, or connection destruction.
/// It cannot be reset; use a fresh instance for a new execution scope.
#[derive(Debug, Default)]
pub struct ExecutionMeter {
    rows_read: AtomicU64,
    rows_written: AtomicU64,
    row_mutations: AtomicU64,
    vm_steps: AtomicU64,
    max_vm_steps: Option<u64>,
    max_rows_read: Option<u64>,
    max_row_mutations: Option<u64>,
    vm_budget_exhausted: AtomicBool,
    read_budget_exhausted: AtomicBool,
    mutation_budget_exhausted: AtomicBool,
}

impl ExecutionMeter {
    /// `None` disables the VM budget; `Some(0)` interrupts before any dispatch.
    /// This does not bound parsing, planning, or work within a single opcode.
    pub fn new(max_vm_steps: Option<u64>) -> Self {
        Self::with_limits(ExecutionLimits {
            max_vm_steps,
            max_rows_read: None,
            max_row_mutations: None,
        })
    }

    pub fn with_limits(limits: ExecutionLimits) -> Self {
        Self {
            max_vm_steps: limits.max_vm_steps,
            max_rows_read: limits.max_rows_read,
            max_row_mutations: limits.max_row_mutations,
            ..Self::default()
        }
    }

    /// Fields are individually atomic. Read after execution has stopped for a
    /// consistent final snapshot; concurrent observations are provisional.
    pub fn snapshot(&self) -> ExecutionSnapshot {
        ExecutionSnapshot {
            rows_read: self.rows_read.load(Ordering::Relaxed),
            rows_written: self.rows_written.load(Ordering::Relaxed),
            row_mutations: self.row_mutations.load(Ordering::Relaxed),
            vm_steps: self.vm_steps.load(Ordering::Relaxed),
            vm_budget_exhausted: self.vm_budget_exhausted.load(Ordering::Relaxed),
            read_budget_exhausted: self.read_budget_exhausted.load(Ordering::Relaxed),
            mutation_budget_exhausted: self.mutation_budget_exhausted.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn record_rows_read(&self, count: u64) -> bool {
        let previous = self
            .rows_read
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(count))
            })
            .expect("unconditional update");
        if self
            .max_rows_read
            .is_some_and(|limit| previous.saturating_add(count) > limit)
        {
            self.read_budget_exhausted.store(true, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub(crate) fn record_row_mutation(&self) -> bool {
        let previous = self
            .row_mutations
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            })
            .expect("unconditional update");
        if self
            .max_row_mutations
            .is_some_and(|limit| previous.saturating_add(1) > limit)
        {
            self.mutation_budget_exhausted
                .store(true, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub(crate) fn is_budget_exhausted(&self) -> bool {
        self.read_budget_exhausted.load(Ordering::Relaxed)
            || self.vm_budget_exhausted.load(Ordering::Relaxed)
            || self.mutation_budget_exhausted.load(Ordering::Relaxed)
    }

    pub(crate) fn record_rows_written(&self, count: u64) {
        saturating_add(&self.rows_written, count);
    }

    /// Reserve before dispatch, so a shared meter cannot overrun its VM limit.
    pub(crate) fn take_vm_step(&self) -> bool {
        if self.is_budget_exhausted() {
            return false;
        }
        let result = self
            .vm_steps
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |steps| {
                if self.max_vm_steps.is_some_and(|limit| steps >= limit) {
                    None
                } else {
                    Some(steps.saturating_add(1))
                }
            });
        if result.is_err() {
            self.vm_budget_exhausted.store(true, Ordering::Relaxed);
            return false;
        }
        true
    }
}

fn saturating_add(counter: &AtomicU64, count: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(count))
    });
}
