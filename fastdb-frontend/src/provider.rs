//! Closed, crate-private index-provider registry.
//!
//! Phase 6 registers only the ordinary B-tree adapter. The trait is not
//! exported, accepts no SQL callbacks, and resolves physical work exclusively
//! from catalog definitions whose names are already opaque.

use crate::catalog::{
    FtsIndexOptions, IndexDefinition, IndexKind, Provider, ProviderState,
    BUILTIN_BTREE_ENCODING_VERSION, BUILTIN_BTREE_PROVIDER_VERSION, BUILTIN_FTS_ENCODING_VERSION,
    BUILTIN_FTS_PROVIDER_VERSION, BUILTIN_GRAPH_ENCODING_VERSION, BUILTIN_GRAPH_PROVIDER_VERSION,
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
struct BuiltinGraphProvider;
struct BuiltinFtsProvider;

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

impl IndexProviderAdapter for BuiltinGraphProvider {
    fn validate_definition(&self, index: &IndexDefinition) -> Result<()> {
        if index.kind != IndexKind::GraphAdjacency || index.provider != Provider::BuiltinGraph {
            return Err(FastDbError::format(
                "graph index has an incompatible kind or provider",
            ));
        }
        if index.provider_version != BUILTIN_GRAPH_PROVIDER_VERSION
            || index.encoding_version != BUILTIN_GRAPH_ENCODING_VERSION
        {
            return Err(FastDbError::format(
                "graph index has an unsupported provider or encoding version",
            ));
        }
        if !matches!(
            index.options_json.as_str(),
            "{\"direction\":\"forward\"}" | "{\"direction\":\"reverse\"}"
        ) {
            return Err(FastDbError::format(
                "graph index direction options are not canonical",
            ));
        }
        if index.unique || index.paths.len() != 4 || index.paths.iter().any(|path| path.len() != 1)
        {
            return Err(FastDbError::format(
                "graph index must contain four direct non-unique hidden columns",
            ));
        }
        if index.state != ProviderState::Ready {
            return Err(FastDbError::format("graph index is not ready"));
        }
        Ok(())
    }

    fn create_statement(&self, index: &IndexDefinition, physical_table: &str) -> Result<Stmt> {
        self.validate_definition(index)?;
        let columns = index
            .paths
            .iter()
            .map(|path| path[0].clone())
            .collect::<Vec<_>>();
        crate::lower::physical_graph_index_ddl(&index.physical_name, physical_table, &columns)
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

impl IndexProviderAdapter for BuiltinFtsProvider {
    fn validate_definition(&self, index: &IndexDefinition) -> Result<()> {
        if index.kind != IndexKind::Fts || index.provider != Provider::BuiltinFts {
            return Err(FastDbError::format(
                "FTS index has an incompatible kind or provider",
            ));
        }
        if index.provider_version != BUILTIN_FTS_PROVIDER_VERSION
            || index.encoding_version != BUILTIN_FTS_ENCODING_VERSION
            || index.state != ProviderState::Ready
        {
            return Err(FastDbError::format(
                "FTS index has an unsupported version or state",
            ));
        }
        if index.unique
            || index.paths.is_empty()
            || index.paths.len() != index.physical_columns.len()
        {
            return Err(FastDbError::format(
                "FTS index must have matching logical and hidden input columns",
            ));
        }
        let options = FtsIndexOptions::parse_canonical(&index.options_json)?;
        if !matches!(
            options.tokenizer.as_str(),
            "default" | "raw" | "simple" | "whitespace" | "ngram"
        ) || options.weights.len() != index.paths.len()
            || options
                .weights
                .iter()
                .any(|weight| !weight.is_finite() || *weight <= 0.0)
        {
            return Err(FastDbError::format("FTS index options are incompatible"));
        }
        match options.surface.as_str() {
            "surreal" => {
                if options.tokenizer != "whitespace"
                    || options.analyzer.is_none()
                    || index.paths.len() != 1
                    || options.weights != [1.0]
                {
                    return Err(FastDbError::format(
                        "Surreal FTS index options are incompatible",
                    ));
                }
            }
            "fastdb" => {
                if options.analyzer.is_some() || options.highlights {
                    return Err(FastDbError::format(
                        "native FTS index contains Surreal-only options",
                    ));
                }
            }
            _ => return Err(FastDbError::format("FTS surface is unknown")),
        }
        Ok(())
    }

    fn create_statement(&self, index: &IndexDefinition, physical_table: &str) -> Result<Stmt> {
        self.validate_definition(index)?;
        let options = FtsIndexOptions::parse_canonical(&index.options_json)?;
        crate::lower::physical_fts_index_ddl(
            &index.physical_name,
            physical_table,
            &index.physical_columns,
            &options.tokenizer,
            &options.weights,
        )
    }

    fn drop_statement(&self, index: &IndexDefinition) -> Result<Stmt> {
        self.validate_definition(index)?;
        crate::lower::physical_drop_index_ddl(&index.physical_name)
    }

    fn rebuild_statement(&self, index: &IndexDefinition) -> Result<Stmt> {
        self.validate_definition(index)?;
        crate::lower::physical_optimize_index_stmt(&index.physical_name)
    }
}

static BUILTIN_BTREE: BuiltinBtreeProvider = BuiltinBtreeProvider;
static BUILTIN_GRAPH: BuiltinGraphProvider = BuiltinGraphProvider;
static BUILTIN_FTS: BuiltinFtsProvider = BuiltinFtsProvider;

pub(crate) fn index_provider(index: &IndexDefinition) -> Result<&'static dyn IndexProviderAdapter> {
    match index.provider {
        Provider::BuiltinBtree => {
            BUILTIN_BTREE.validate_definition(index)?;
            Ok(&BUILTIN_BTREE)
        }
        Provider::BuiltinGraph => {
            BUILTIN_GRAPH.validate_definition(index)?;
            Ok(&BUILTIN_GRAPH)
        }
        Provider::BuiltinFts => {
            BUILTIN_FTS.validate_definition(index)?;
            Ok(&BUILTIN_FTS)
        }
        Provider::BuiltinVector => Err(FastDbError::format(
            "exact vector provider does not own indexes",
        )),
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
