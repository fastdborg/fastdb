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
    /// After the no-op migration row update, before commit.
    AfterMigration,
    /// After the logical-table catalog row is inserted.
    AfterCatalogRow,
    /// After the hidden physical table is created.
    AfterPhysicalDdl,
    /// After existing rows pass a new field definition.
    AfterFieldValidation,
    /// After a field catalog row is written.
    AfterFieldCatalogRow,
    /// After existing rows pass a new index definition.
    AfterIndexValidation,
    /// After physical index DDL is executed.
    AfterIndexPhysicalDdl,
    /// After an index catalog row is written.
    AfterIndexCatalogRow,
    /// After the record INSERT is prepared, before it is executed.
    AfterRecordPrepare,
    /// After the record is inserted, before COMMIT.
    AfterRecordInsert,
    /// After the body succeeds and the (optional) real COMMIT would run.
    /// Simulates a COMMIT-time failure so the commit-failure rollback path is
    /// exercised deterministically. A separate integration test wraps Turso's
    /// public `IO` interface to fail an actual WAL sync completion.
    CommitFailure,
    /// Before transaction cleanup issues `ROLLBACK`. Used to prove that the
    /// original and cleanup failures are reported together.
    RollbackFailure,
}

#[derive(Default)]
pub struct Failpoints {
    after_bootstrap: AtomicBool,
    after_migration: AtomicBool,
    after_catalog_row: AtomicBool,
    after_physical_ddl: AtomicBool,
    after_field_validation: AtomicBool,
    after_field_catalog_row: AtomicBool,
    after_index_validation: AtomicBool,
    after_index_physical_ddl: AtomicBool,
    after_index_catalog_row: AtomicBool,
    after_record_prepare: AtomicBool,
    after_record_insert: AtomicBool,
    commit_failure: AtomicBool,
    rollback_failure: AtomicBool,
}

impl Failpoints {
    /// If the given failpoint is armed, return a Transaction-category error
    /// that drives the real rollback path. Never panics.
    pub fn check(&self, fp: Failpoint) -> Result<()> {
        let armed = match fp {
            Failpoint::AfterBootstrap => self.after_bootstrap.load(Ordering::SeqCst),
            Failpoint::AfterMigration => self.after_migration.load(Ordering::SeqCst),
            Failpoint::AfterCatalogRow => self.after_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.load(Ordering::SeqCst),
            Failpoint::AfterFieldValidation => self.after_field_validation.load(Ordering::SeqCst),
            Failpoint::AfterFieldCatalogRow => self.after_field_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterIndexValidation => self.after_index_validation.load(Ordering::SeqCst),
            Failpoint::AfterIndexPhysicalDdl => {
                self.after_index_physical_ddl.load(Ordering::SeqCst)
            }
            Failpoint::AfterIndexCatalogRow => self.after_index_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterRecordPrepare => self.after_record_prepare.load(Ordering::SeqCst),
            Failpoint::AfterRecordInsert => self.after_record_insert.load(Ordering::SeqCst),
            Failpoint::CommitFailure => self.commit_failure.load(Ordering::SeqCst),
            Failpoint::RollbackFailure => self.rollback_failure.load(Ordering::SeqCst),
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
            Failpoint::AfterMigration => self.after_migration.store(true, Ordering::SeqCst),
            Failpoint::AfterCatalogRow => self.after_catalog_row.store(true, Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.store(true, Ordering::SeqCst),
            Failpoint::AfterFieldValidation => {
                self.after_field_validation.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterFieldCatalogRow => {
                self.after_field_catalog_row.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterIndexValidation => {
                self.after_index_validation.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterIndexPhysicalDdl => {
                self.after_index_physical_ddl.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterIndexCatalogRow => {
                self.after_index_catalog_row.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterRecordPrepare => {
                self.after_record_prepare.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterRecordInsert => self.after_record_insert.store(true, Ordering::SeqCst),
            Failpoint::CommitFailure => self.commit_failure.store(true, Ordering::SeqCst),
            Failpoint::RollbackFailure => self.rollback_failure.store(true, Ordering::SeqCst),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn disarm(&self, fp: Failpoint) {
        match fp {
            Failpoint::AfterBootstrap => self.after_bootstrap.store(false, Ordering::SeqCst),
            Failpoint::AfterMigration => self.after_migration.store(false, Ordering::SeqCst),
            Failpoint::AfterCatalogRow => self.after_catalog_row.store(false, Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.store(false, Ordering::SeqCst),
            Failpoint::AfterFieldValidation => {
                self.after_field_validation.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterFieldCatalogRow => {
                self.after_field_catalog_row.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterIndexValidation => {
                self.after_index_validation.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterIndexPhysicalDdl => {
                self.after_index_physical_ddl.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterIndexCatalogRow => {
                self.after_index_catalog_row.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterRecordPrepare => {
                self.after_record_prepare.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterRecordInsert => self.after_record_insert.store(false, Ordering::SeqCst),
            Failpoint::CommitFailure => self.commit_failure.store(false, Ordering::SeqCst),
            Failpoint::RollbackFailure => self.rollback_failure.store(false, Ordering::SeqCst),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn disarm_all(&self) {
        for fp in [
            Failpoint::AfterBootstrap,
            Failpoint::AfterMigration,
            Failpoint::AfterCatalogRow,
            Failpoint::AfterPhysicalDdl,
            Failpoint::AfterFieldValidation,
            Failpoint::AfterFieldCatalogRow,
            Failpoint::AfterIndexValidation,
            Failpoint::AfterIndexPhysicalDdl,
            Failpoint::AfterIndexCatalogRow,
            Failpoint::AfterRecordPrepare,
            Failpoint::AfterRecordInsert,
            Failpoint::CommitFailure,
            Failpoint::RollbackFailure,
        ] {
            self.disarm(fp);
        }
    }
}
