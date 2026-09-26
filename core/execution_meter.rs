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
    /// VM dispatch attempts, including attempts that yield for I/O.
    pub vm_steps: u64,
    pub vm_budget_exhausted: bool,
}

/// A fresh meter can span several sequential statements. Retain its `Arc` to
/// inspect work after failure, rollback, reset, or connection destruction.
/// It cannot be reset; use a fresh instance for a new execution scope.
#[derive(Debug, Default)]
pub struct ExecutionMeter {
    rows_read: AtomicU64,
    rows_written: AtomicU64,
    vm_steps: AtomicU64,
    max_vm_steps: Option<u64>,
    vm_budget_exhausted: AtomicBool,
}

impl ExecutionMeter {
    /// `None` disables the VM budget; `Some(0)` interrupts before any dispatch.
    /// This does not bound parsing, planning, or work within a single opcode.
    pub fn new(max_vm_steps: Option<u64>) -> Self {
        Self {
            max_vm_steps,
            ..Self::default()
        }
    }

    /// Fields are individually atomic. Read after execution has stopped for a
    /// consistent final snapshot; concurrent observations are provisional.
    pub fn snapshot(&self) -> ExecutionSnapshot {
        ExecutionSnapshot {
            rows_read: self.rows_read.load(Ordering::Relaxed),
            rows_written: self.rows_written.load(Ordering::Relaxed),
            vm_steps: self.vm_steps.load(Ordering::Relaxed),
            vm_budget_exhausted: self.vm_budget_exhausted.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn record_rows_read(&self, count: u64) {
        saturating_add(&self.rows_read, count);
    }

    pub(crate) fn record_rows_written(&self, count: u64) {
        saturating_add(&self.rows_written, count);
    }

    /// Reserve before dispatch, so a shared meter cannot overrun its VM limit.
    pub(crate) fn take_vm_step(&self) -> bool {
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
