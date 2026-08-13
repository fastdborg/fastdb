//! Closed, crate-private index-provider registry.
//!
//! Phase 6 registers only the ordinary B-tree adapter. The trait is not
//! exported, accepts no SQL callbacks, and resolves physical work exclusively
//! from catalog definitions whose names are already opaque.

use crate::catalog::{
    IndexDefinition, IndexKind, Provider, ProviderState, BUILTIN_BTREE_ENCODING_VERSION,
    BUILTIN_BTREE_PROVIDER_VERSION,
};
use crate::error::{FastDbError, Result};
use turso_parser::ast::Stmt;

pub(crate) trait IndexProviderAdapter: Sync {
    fn validate_definition(&self, index: &IndexDefinition) -> Result<()>;
    fn create_statement(&self, index: &IndexDefinition, physical_table: &str) -> Result<Stmt>;
    fn drop_statement(&self, index: &IndexDefinition) -> Result<Stmt>;
    fn rebuild_statement(&self, index: &IndexDefinition) -> Result<Stmt>;
}

struct BuiltinBtreeProvider;

impl IndexProviderAdapter for BuiltinBtreeProvider {
    fn validate_definition(&self, index: &IndexDefinition) -> Result<()> {
        if index.kind != IndexKind::Btree || index.provider != Provider::BuiltinBtree {
            return Err(FastDbError::format(
                "B-tree index has an incompatible kind or provider",
            ));
        }
        if index.provider_version != BUILTIN_BTREE_PROVIDER_VERSION {
            return Err(FastDbError::format(format!(
                "unsupported built-in B-tree provider version {}",
                index.provider_version
            )));
        }
        if index.options_json != "{}" {
            return Err(FastDbError::format(
                "built-in B-tree options are not canonical",
            ));
        }
        if index.state != ProviderState::Ready {
            return Err(FastDbError::format(
                "built-in B-tree index cannot require provider rebuild",
            ));
        }
        if index.encoding_version != BUILTIN_BTREE_ENCODING_VERSION {
            return Err(FastDbError::format(format!(
                "unsupported built-in B-tree encoding version {}",
                index.encoding_version
            )));
        }
        Ok(())
    }

    fn create_statement(&self, index: &IndexDefinition, physical_table: &str) -> Result<Stmt> {
        self.validate_definition(index)?;
        crate::lower::physical_index_ddl(
            &index.physical_name,
            physical_table,
            &index.path_keys,
            index.unique,
        )
    }

    fn drop_statement(&self, index: &IndexDefinition) -> Result<Stmt> {
        self.validate_definition(index)?;
        crate::lower::physical_drop_index_ddl(&index.physical_name)
    }

    fn rebuild_statement(&self, index: &IndexDefinition) -> Result<Stmt> {
        self.validate_definition(index)?;
        crate::lower::physical_rebuild_index_stmt(&index.physical_name)
    }
}

static BUILTIN_BTREE: BuiltinBtreeProvider = BuiltinBtreeProvider;

pub(crate) fn index_provider(index: &IndexDefinition) -> Result<&'static dyn IndexProviderAdapter> {
    match index.provider {
        Provider::BuiltinBtree => {
            BUILTIN_BTREE.validate_definition(index)?;
            Ok(&BUILTIN_BTREE)
        }
    }
}

#[cfg(feature = "testing")]
trait DerivedStorageProviderAdapter: Sync {
    fn derive(&self, encoded_document: &str) -> Result<i64>;
}

/// A deliberately small provider used only to prove the Phase 6 atomic
/// document/derived-storage contract. It is absent from the persisted
/// provider registry and cannot be selected by user syntax.
#[cfg(feature = "testing")]
struct TestDocumentLengthProvider;

#[cfg(feature = "testing")]
impl DerivedStorageProviderAdapter for TestDocumentLengthProvider {
    fn derive(&self, encoded_document: &str) -> Result<i64> {
        i64::try_from(encoded_document.len())
            .map_err(|_| FastDbError::Engine("test provider document is too large".into()))
    }
}

#[cfg(feature = "testing")]
static TEST_DOCUMENT_LENGTH: TestDocumentLengthProvider = TestDocumentLengthProvider;

#[cfg(feature = "testing")]
pub(crate) fn derive_test_hidden_value(encoded_document: &str) -> Result<i64> {
    TEST_DOCUMENT_LENGTH.derive(encoded_document)
}
