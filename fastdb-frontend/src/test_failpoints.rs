//! Deterministic, test-only failure injection.
//!
//! The [`Failpoints`] registry always exists on a connection so that
//! `execute.rs` can call [`Failpoints::check`] uniformly; in non-test builds
//! no flag is ever set, so checks are no-ops. The arming API is gated behind
//! the `testing` Cargo feature so it cannot leak into release builds.

use crate::error::{FastDbError, Result};
use std::sync::atomic::{AtomicBool, Ordering};

/// A failure-injection point in the CREATE transaction algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failpoint {
    /// After catalog bootstrap (meta + tables tables + metadata row).
    AfterBootstrap,
    /// After the logical-table catalog row is inserted.
    AfterCatalogRow,
    /// After the hidden physical table is created.
    AfterPhysicalDdl,
    /// After the record INSERT is prepared, before it is executed.
    AfterRecordPrepare,
    /// After the record is inserted, before COMMIT.
    AfterRecordInsert,
}

#[derive(Default)]
pub struct Failpoints {
    after_bootstrap: AtomicBool,
    after_catalog_row: AtomicBool,
    after_physical_ddl: AtomicBool,
    after_record_prepare: AtomicBool,
    after_record_insert: AtomicBool,
}

impl Failpoints {
    /// If the given failpoint is armed, return a Transaction-category error
    /// that drives the real rollback path. Never panics.
    pub fn check(&self, fp: Failpoint) -> Result<()> {
        let armed = match fp {
            Failpoint::AfterBootstrap => self.after_bootstrap.load(Ordering::SeqCst),
            Failpoint::AfterCatalogRow => self.after_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.load(Ordering::SeqCst),
            Failpoint::AfterRecordPrepare => self.after_record_prepare.load(Ordering::SeqCst),
            Failpoint::AfterRecordInsert => self.after_record_insert.load(Ordering::SeqCst),
        };
        if armed {
            return Err(FastDbError::Transaction(format!(
                "injected failure: {fp:?}"
            )));
        }
        Ok(())
    }

    #[cfg(feature = "testing")]
    pub(crate) fn arm(&self, fp: Failpoint) {
        match fp {
            Failpoint::AfterBootstrap => self.after_bootstrap.store(true, Ordering::SeqCst),
            Failpoint::AfterCatalogRow => self.after_catalog_row.store(true, Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.store(true, Ordering::SeqCst),
            Failpoint::AfterRecordPrepare => {
                self.after_record_prepare.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterRecordInsert => self.after_record_insert.store(true, Ordering::SeqCst),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn disarm(&self, fp: Failpoint) {
        match fp {
            Failpoint::AfterBootstrap => self.after_bootstrap.store(false, Ordering::SeqCst),
            Failpoint::AfterCatalogRow => self.after_catalog_row.store(false, Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.store(false, Ordering::SeqCst),
            Failpoint::AfterRecordPrepare => {
                self.after_record_prepare.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterRecordInsert => self.after_record_insert.store(false, Ordering::SeqCst),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn disarm_all(&self) {
        for fp in [
            Failpoint::AfterBootstrap,
            Failpoint::AfterCatalogRow,
            Failpoint::AfterPhysicalDdl,
            Failpoint::AfterRecordPrepare,
            Failpoint::AfterRecordInsert,
        ] {
            self.disarm(fp);
        }
    }
}
