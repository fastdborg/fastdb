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
    /// After format-2 table metadata columns are added.
    AfterFormat2TableColumns,
    /// After format-2 index metadata columns are added.
    AfterFormat2IndexColumns,
    /// After the new format-2 catalogs are created.
    AfterFormat2Catalogs,
    /// After migrated format-2 ownership validates, before header publication.
    AfterFormat2Validation,
    /// After the logical-table catalog row is inserted.
    AfterCatalogRow,
    /// After the hidden physical table is created.
    AfterPhysicalDdl,
    /// After graph hidden-column rows are persisted.
    AfterGraphHiddenCatalog,
    /// After the forward graph adjacency index is persisted and created.
    AfterGraphForwardIndex,
    /// After the reverse graph adjacency index is persisted and created.
    AfterGraphReverseIndex,
    /// After an edge document and all hidden endpoints are inserted.
    AfterGraphEdgeInsert,
    /// After a Surreal analyzer and FTS capability are cataloged.
    AfterFtsAnalyzerCatalog,
    /// After FTS hidden-column ownership rows are cataloged.
    AfterFtsHiddenCatalog,
    /// After one FTS hidden physical TEXT column is added.
    AfterFtsPhysicalColumn,
    /// After existing documents are backfilled into FTS hidden columns.
    AfterFtsBackfill,
    /// After the custom FTS provider index is created.
    AfterFtsProviderIndex,
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
    /// After the physical B-tree is dropped, before its catalog row is removed.
    AfterIndexRemovePhysical,
    /// After the B-tree catalog row is removed, before commit.
    AfterIndexRemoveCatalog,
    /// After the engine rebuilds a B-tree, before commit.
    AfterIndexRebuild,
    /// After a test provider updates `doc`, before its derived column update.
    AfterTestProviderDocument,
    /// After the record INSERT is prepared, before it is executed.
    AfterRecordPrepare,
    /// After the record is inserted, before COMMIT.
    AfterRecordInsert,
    /// After UPDATE candidates validate, before the first physical mutation.
    BeforeUpdateMutations,
    /// After one physical UPDATE mutation, before statement completion.
    AfterUpdateMutation,
    /// After DELETE candidates are fixed, before the first physical mutation.
    BeforeDeleteMutations,
    /// After one physical DELETE mutation, before statement completion.
    AfterDeleteMutation,
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
    after_format2_table_columns: AtomicBool,
    after_format2_index_columns: AtomicBool,
    after_format2_catalogs: AtomicBool,
    after_format2_validation: AtomicBool,
    after_catalog_row: AtomicBool,
    after_physical_ddl: AtomicBool,
    after_graph_hidden_catalog: AtomicBool,
    after_graph_forward_index: AtomicBool,
    after_graph_reverse_index: AtomicBool,
    after_graph_edge_insert: AtomicBool,
    after_fts_analyzer_catalog: AtomicBool,
    after_fts_hidden_catalog: AtomicBool,
    after_fts_physical_column: AtomicBool,
    after_fts_backfill: AtomicBool,
    after_fts_provider_index: AtomicBool,
    after_field_validation: AtomicBool,
    after_field_catalog_row: AtomicBool,
    after_index_validation: AtomicBool,
    after_index_physical_ddl: AtomicBool,
    after_index_catalog_row: AtomicBool,
    after_index_remove_physical: AtomicBool,
    after_index_remove_catalog: AtomicBool,
    after_index_rebuild: AtomicBool,
    after_test_provider_document: AtomicBool,
    after_record_prepare: AtomicBool,
    after_record_insert: AtomicBool,
    before_update_mutations: AtomicBool,
    after_update_mutation: AtomicBool,
    before_delete_mutations: AtomicBool,
    after_delete_mutation: AtomicBool,
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
            Failpoint::AfterFormat2TableColumns => {
                self.after_format2_table_columns.load(Ordering::SeqCst)
            }
            Failpoint::AfterFormat2IndexColumns => {
                self.after_format2_index_columns.load(Ordering::SeqCst)
            }
            Failpoint::AfterFormat2Catalogs => self.after_format2_catalogs.load(Ordering::SeqCst),
            Failpoint::AfterFormat2Validation => {
                self.after_format2_validation.load(Ordering::SeqCst)
            }
            Failpoint::AfterCatalogRow => self.after_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.load(Ordering::SeqCst),
            Failpoint::AfterGraphHiddenCatalog => {
                self.after_graph_hidden_catalog.load(Ordering::SeqCst)
            }
            Failpoint::AfterGraphForwardIndex => {
                self.after_graph_forward_index.load(Ordering::SeqCst)
            }
            Failpoint::AfterGraphReverseIndex => {
                self.after_graph_reverse_index.load(Ordering::SeqCst)
            }
            Failpoint::AfterGraphEdgeInsert => self.after_graph_edge_insert.load(Ordering::SeqCst),
            Failpoint::AfterFtsAnalyzerCatalog => {
                self.after_fts_analyzer_catalog.load(Ordering::SeqCst)
            }
            Failpoint::AfterFtsHiddenCatalog => {
                self.after_fts_hidden_catalog.load(Ordering::SeqCst)
            }
            Failpoint::AfterFtsPhysicalColumn => {
                self.after_fts_physical_column.load(Ordering::SeqCst)
            }
            Failpoint::AfterFtsBackfill => self.after_fts_backfill.load(Ordering::SeqCst),
            Failpoint::AfterFtsProviderIndex => {
                self.after_fts_provider_index.load(Ordering::SeqCst)
            }
            Failpoint::AfterFieldValidation => self.after_field_validation.load(Ordering::SeqCst),
            Failpoint::AfterFieldCatalogRow => self.after_field_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterIndexValidation => self.after_index_validation.load(Ordering::SeqCst),
            Failpoint::AfterIndexPhysicalDdl => {
                self.after_index_physical_ddl.load(Ordering::SeqCst)
            }
            Failpoint::AfterIndexCatalogRow => self.after_index_catalog_row.load(Ordering::SeqCst),
            Failpoint::AfterIndexRemovePhysical => {
                self.after_index_remove_physical.load(Ordering::SeqCst)
            }
            Failpoint::AfterIndexRemoveCatalog => {
                self.after_index_remove_catalog.load(Ordering::SeqCst)
            }
            Failpoint::AfterIndexRebuild => self.after_index_rebuild.load(Ordering::SeqCst),
            Failpoint::AfterTestProviderDocument => {
                self.after_test_provider_document.load(Ordering::SeqCst)
            }
            Failpoint::AfterRecordPrepare => self.after_record_prepare.load(Ordering::SeqCst),
            Failpoint::AfterRecordInsert => self.after_record_insert.load(Ordering::SeqCst),
            Failpoint::BeforeUpdateMutations => self.before_update_mutations.load(Ordering::SeqCst),
            Failpoint::AfterUpdateMutation => self.after_update_mutation.load(Ordering::SeqCst),
            Failpoint::BeforeDeleteMutations => self.before_delete_mutations.load(Ordering::SeqCst),
            Failpoint::AfterDeleteMutation => self.after_delete_mutation.load(Ordering::SeqCst),
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
            Failpoint::AfterFormat2TableColumns => self
                .after_format2_table_columns
                .store(true, Ordering::SeqCst),
            Failpoint::AfterFormat2IndexColumns => self
                .after_format2_index_columns
                .store(true, Ordering::SeqCst),
            Failpoint::AfterFormat2Catalogs => {
                self.after_format2_catalogs.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterFormat2Validation => {
                self.after_format2_validation.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterCatalogRow => self.after_catalog_row.store(true, Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.store(true, Ordering::SeqCst),
            Failpoint::AfterGraphHiddenCatalog => self
                .after_graph_hidden_catalog
                .store(true, Ordering::SeqCst),
            Failpoint::AfterGraphForwardIndex => {
                self.after_graph_forward_index.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterGraphReverseIndex => {
                self.after_graph_reverse_index.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterGraphEdgeInsert => {
                self.after_graph_edge_insert.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterFtsAnalyzerCatalog => self
                .after_fts_analyzer_catalog
                .store(true, Ordering::SeqCst),
            Failpoint::AfterFtsHiddenCatalog => {
                self.after_fts_hidden_catalog.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterFtsPhysicalColumn => {
                self.after_fts_physical_column.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterFtsBackfill => self.after_fts_backfill.store(true, Ordering::SeqCst),
            Failpoint::AfterFtsProviderIndex => {
                self.after_fts_provider_index.store(true, Ordering::SeqCst)
            }
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
            Failpoint::AfterIndexRemovePhysical => self
                .after_index_remove_physical
                .store(true, Ordering::SeqCst),
            Failpoint::AfterIndexRemoveCatalog => self
                .after_index_remove_catalog
                .store(true, Ordering::SeqCst),
            Failpoint::AfterIndexRebuild => self.after_index_rebuild.store(true, Ordering::SeqCst),
            Failpoint::AfterTestProviderDocument => self
                .after_test_provider_document
                .store(true, Ordering::SeqCst),
            Failpoint::AfterRecordPrepare => {
                self.after_record_prepare.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterRecordInsert => self.after_record_insert.store(true, Ordering::SeqCst),
            Failpoint::BeforeUpdateMutations => {
                self.before_update_mutations.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterUpdateMutation => {
                self.after_update_mutation.store(true, Ordering::SeqCst)
            }
            Failpoint::BeforeDeleteMutations => {
                self.before_delete_mutations.store(true, Ordering::SeqCst)
            }
            Failpoint::AfterDeleteMutation => {
                self.after_delete_mutation.store(true, Ordering::SeqCst)
            }
            Failpoint::CommitFailure => self.commit_failure.store(true, Ordering::SeqCst),
            Failpoint::RollbackFailure => self.rollback_failure.store(true, Ordering::SeqCst),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn disarm(&self, fp: Failpoint) {
        match fp {
            Failpoint::AfterBootstrap => self.after_bootstrap.store(false, Ordering::SeqCst),
            Failpoint::AfterMigration => self.after_migration.store(false, Ordering::SeqCst),
            Failpoint::AfterFormat2TableColumns => self
                .after_format2_table_columns
                .store(false, Ordering::SeqCst),
            Failpoint::AfterFormat2IndexColumns => self
                .after_format2_index_columns
                .store(false, Ordering::SeqCst),
            Failpoint::AfterFormat2Catalogs => {
                self.after_format2_catalogs.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterFormat2Validation => {
                self.after_format2_validation.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterCatalogRow => self.after_catalog_row.store(false, Ordering::SeqCst),
            Failpoint::AfterPhysicalDdl => self.after_physical_ddl.store(false, Ordering::SeqCst),
            Failpoint::AfterGraphHiddenCatalog => self
                .after_graph_hidden_catalog
                .store(false, Ordering::SeqCst),
            Failpoint::AfterGraphForwardIndex => self
                .after_graph_forward_index
                .store(false, Ordering::SeqCst),
            Failpoint::AfterGraphReverseIndex => self
                .after_graph_reverse_index
                .store(false, Ordering::SeqCst),
            Failpoint::AfterGraphEdgeInsert => {
                self.after_graph_edge_insert.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterFtsAnalyzerCatalog => self
                .after_fts_analyzer_catalog
                .store(false, Ordering::SeqCst),
            Failpoint::AfterFtsHiddenCatalog => {
                self.after_fts_hidden_catalog.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterFtsPhysicalColumn => self
                .after_fts_physical_column
                .store(false, Ordering::SeqCst),
            Failpoint::AfterFtsBackfill => self.after_fts_backfill.store(false, Ordering::SeqCst),
            Failpoint::AfterFtsProviderIndex => {
                self.after_fts_provider_index.store(false, Ordering::SeqCst)
            }
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
            Failpoint::AfterIndexRemovePhysical => self
                .after_index_remove_physical
                .store(false, Ordering::SeqCst),
            Failpoint::AfterIndexRemoveCatalog => self
                .after_index_remove_catalog
                .store(false, Ordering::SeqCst),
            Failpoint::AfterIndexRebuild => self.after_index_rebuild.store(false, Ordering::SeqCst),
            Failpoint::AfterTestProviderDocument => self
                .after_test_provider_document
                .store(false, Ordering::SeqCst),
            Failpoint::AfterRecordPrepare => {
                self.after_record_prepare.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterRecordInsert => self.after_record_insert.store(false, Ordering::SeqCst),
            Failpoint::BeforeUpdateMutations => {
                self.before_update_mutations.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterUpdateMutation => {
                self.after_update_mutation.store(false, Ordering::SeqCst)
            }
            Failpoint::BeforeDeleteMutations => {
                self.before_delete_mutations.store(false, Ordering::SeqCst)
            }
            Failpoint::AfterDeleteMutation => {
                self.after_delete_mutation.store(false, Ordering::SeqCst)
            }
            Failpoint::CommitFailure => self.commit_failure.store(false, Ordering::SeqCst),
            Failpoint::RollbackFailure => self.rollback_failure.store(false, Ordering::SeqCst),
        }
    }

    #[cfg(feature = "testing")]
    pub(crate) fn disarm_all(&self) {
        for fp in [
            Failpoint::AfterBootstrap,
            Failpoint::AfterMigration,
            Failpoint::AfterFormat2TableColumns,
            Failpoint::AfterFormat2IndexColumns,
            Failpoint::AfterFormat2Catalogs,
            Failpoint::AfterFormat2Validation,
            Failpoint::AfterCatalogRow,
            Failpoint::AfterPhysicalDdl,
            Failpoint::AfterGraphHiddenCatalog,
            Failpoint::AfterGraphForwardIndex,
            Failpoint::AfterGraphReverseIndex,
            Failpoint::AfterGraphEdgeInsert,
            Failpoint::AfterFtsAnalyzerCatalog,
            Failpoint::AfterFtsHiddenCatalog,
            Failpoint::AfterFtsPhysicalColumn,
            Failpoint::AfterFtsBackfill,
            Failpoint::AfterFtsProviderIndex,
            Failpoint::AfterFieldValidation,
            Failpoint::AfterFieldCatalogRow,
            Failpoint::AfterIndexValidation,
            Failpoint::AfterIndexPhysicalDdl,
            Failpoint::AfterIndexCatalogRow,
            Failpoint::AfterIndexRemovePhysical,
            Failpoint::AfterIndexRemoveCatalog,
            Failpoint::AfterIndexRebuild,
            Failpoint::AfterTestProviderDocument,
            Failpoint::AfterRecordPrepare,
            Failpoint::AfterRecordInsert,
            Failpoint::BeforeUpdateMutations,
            Failpoint::AfterUpdateMutation,
            Failpoint::BeforeDeleteMutations,
            Failpoint::AfterDeleteMutation,
            Failpoint::CommitFailure,
            Failpoint::RollbackFailure,
        ] {
            self.disarm(fp);
        }
    }
}
