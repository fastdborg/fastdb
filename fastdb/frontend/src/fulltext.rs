//! Managed full-text storage and deterministic ranking over pinned native FTS.
use crate::{
    canonical, quote, text, Connection, Document, EngineValue, Error, Index, IndexKind, Result,
    Value,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub additional_paths: Vec<Vec<String>>,
    pub tokenizer: String,
}
const TOKENIZER: &str = "tantivy-default-0.26";
// Stay below the pinned native FTS cursor's 1,000-document flush threshold.
// Sixteen text fields plus the ID use at most 2,176 bound parameters.
const BUILD_BATCH_ROWS: usize = 128;
const _: () = assert!(BUILD_BATCH_ROWS < turso_core::index_method::fts::BATCH_COMMIT_SIZE);
fn invalid(message: &str) -> Error {
    Error::Validation(format!("fulltext: {message}"))
}

impl Index {
    pub(crate) fn paths(&self) -> impl Iterator<Item = &[String]> {
        std::iter::once(self.path.as_slice()).chain(
            self.fulltext
                .iter()
                .flat_map(|c| c.additional_paths.iter().map(Vec::as_slice)),
        )
    }
    pub(crate) fn validate_text_config(&self) -> Result<()> {
        match (&self.kind, &self.fulltext) {
            (IndexKind::FullText, Some(config))
                if config.tokenizer == TOKENIZER && !self.unique =>
            {
                // The native method generates its directory DDL from the index
                // name without quoting. Reject names it cannot represent safely.
                if self.name.is_empty()
                    || !self
                        .name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    return Err(invalid(
                        "index names must contain ASCII letters, digits or underscores",
                    ));
                }
                let mut seen = std::collections::BTreeSet::new();
                for path in self.paths() {
                    crate::validate_path(path)?;
                    if !seen.insert(path) {
                        return Err(invalid("duplicate field path"));
                    }
                }
                if seen.len() > 16 {
                    return Err(invalid("at most 16 text fields per index"));
                }
                Ok(())
            }
            (IndexKind::Scalar | IndexKind::Spatial | IndexKind::Vector, None) => Ok(()),
            _ => Err(invalid("incompatible text index configuration")),
        }
    }
    pub(crate) fn document_keys(&self, doc: &Document) -> Result<Vec<EngineValue>> {
        if self.kind != IndexKind::FullText {
            return self.keys(crate::path_value(doc, &self.path)?.unwrap_or(&Value::Null));
        }
        self.paths()
            .map(
                |path| match crate::path_value(doc, path)?.unwrap_or(&Value::Null) {
                    Value::Null => Ok(EngineValue::Null),
                    Value::String(s) => Ok(text(s)),
                    _ => Err(invalid("indexed fields must be strings, missing or null")),
                },
            )
            .collect()
    }
    pub(crate) fn key_columns(&self) -> String {
        match self.kind {
            IndexKind::Scalar | IndexKind::Vector => "\"key\"".into(),
            IndexKind::Spatial => "\"key\",longitude".into(),
            IndexKind::FullText => std::iter::once("\"key\"".to_owned())
                .chain((1..self.paths().count()).map(|i| format!("f{i}")))
                .collect::<Vec<_>>()
                .join(","),
        }
    }
    pub(crate) fn text_table_ddl(&self) -> String {
        let extra = (1..self.paths().count())
            .map(|i| format!(",f{i} TEXT"))
            .collect::<String>();
        format!(
            "CREATE TABLE {} (\"key\" TEXT,id BLOB UNIQUE NOT NULL{extra})",
            quote(&self.storage)
        )
    }
    pub(crate) fn text_index_ddl(&self) -> String {
        format!(
            "CREATE INDEX {} ON {} USING fts ({})",
            quote(&self.name),
            quote(&self.storage),
            self.key_columns()
        )
    }
    pub(crate) fn text_stats(&self) -> String {
        format!("{}_stats", self.storage)
    }
    pub(crate) fn text_stats_ddl(&self) -> String {
        format!("CREATE TABLE {} (slot INTEGER PRIMARY KEY CHECK(slot=1), count INTEGER NOT NULL CHECK(count>=0))", quote(&self.text_stats()))
    }
    pub(crate) fn text_directory(&self) -> String {
        format!("__turso_internal_fts_dir_{}", self.name)
    }
    pub(crate) fn text_directory_ddl(&self) -> String {
        format!(
            "CREATE TABLE {} (path TEXT NOT NULL, chunk_no INTEGER NOT NULL, bytes BLOB NOT NULL)",
            self.text_directory()
        )
    }
    pub(crate) fn text_directory_index_ddl(&self) -> String {
        let dir = self.text_directory();
        format!("CREATE INDEX IF NOT EXISTS {dir}_key ON {dir} USING backing_btree (path, chunk_no, bytes)")
    }
}

impl Connection {
    /// Index up to 16 string paths with the pinned default text analyzer.
    pub fn create_fulltext_index(
        &self,
        table: &str,
        name: &str,
        paths: Vec<Vec<String>>,
        if_not_exists: bool,
    ) -> Result<()> {
        let name = canonical(name)?;
        let Some(path) = paths.first() else {
            return Err(invalid("at least one text field is required"));
        };
        let index = Index {
            kind: IndexKind::FullText,
            name: name.clone(),
            path: path.clone(),
            unique: false,
            storage: format!(
                "__fastdb_i_{}",
                name.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
            vector: None,
            fulltext: Some(Config {
                additional_paths: paths[1..].to_vec(),
                tokenizer: TOKENIZER.into(),
            }),
        };
        index.validate_text_config()?;
        self.atomic(|| {
            let mut c = self.catalog(table)?;
            if let Some(existing) = c.indexes.iter().find(|i| i.name == name) {
                if if_not_exists && existing.kind == index.kind && existing.path == index.path && existing.fulltext == index.fulltext { return Ok(()); }
                return Err(Error::AlreadyExists(name.clone()));
            }
            if !self.run("SELECT name FROM sqlite_schema WHERE name=?1 COLLATE NOCASE UNION ALL SELECT name FROM __fastdb_catalog WHERE name=?1", &[text(&name)])?.is_empty() {
                return Err(Error::AlreadyExists(name.clone()));
            }
            for path in index.paths() { crate::catalog::compatible_index(&c, path, index.kind)?; }
            self.run(&index.table_ddl(), &[])?;
            self.run(&index.index_ddl(), &[])?;
            self.run(&index.text_stats_ddl(), &[])?;
            self.run(&format!("INSERT INTO {} VALUES (1,0)", quote(&index.text_stats())), &[])?;
            for documents in self.documents(&c)?.chunks(BUILD_BATCH_ROWS) {
                self.insert_text_build_batch(&index, documents)?;
            }
            c.indexes.push(index.clone());
            self.save_catalog(&c)
        })
    }
    fn insert_text_build_batch(&self, index: &Index, documents: &[Document]) -> Result<()> {
        let mut values = Vec::new();
        let mut rows = Vec::with_capacity(documents.len());
        for document in documents {
            let mut row = index.document_keys(document)?;
            let id = document
                .get("id")
                .ok_or_else(|| Error::Storage("document has no id".into()))?;
            row.insert(1, EngineValue::Blob(id.encode()?));
            let first = values.len() + 1;
            let slots = (first..first + row.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",");
            rows.push(format!("({slots})"));
            values.extend(row);
        }
        // One statement shares an FTS cursor, amortizing its Tantivy commit.
        // Keep batches below its internal mid-insert flush/reentry path.
        self.run(
            &format!(
                "INSERT INTO {} VALUES {}",
                quote(&index.storage),
                rows.join(",")
            ),
            &values,
        )?;
        self.change_text_count(index, documents.len() as i64)
    }
    pub(crate) fn change_text_count(&self, index: &Index, delta: i64) -> Result<()> {
        let rows = self.run(
            &format!(
                "UPDATE {} SET count=count+?1 WHERE slot=1 RETURNING count",
                quote(&index.text_stats())
            ),
            &[EngineValue::from_i64(delta)],
        )?;
        if rows.len() != 1 {
            return Err(Error::Storage("missing full-text count".into()));
        }
        Ok(())
    }
    pub(crate) fn text_count(&self, index: &Index) -> Result<i64> {
        let rows = self.run(
            &format!(
                "SELECT count FROM {} WHERE slot=1",
                quote(&index.text_stats())
            ),
            &[],
        )?;
        match rows.as_slice() {
            [row] => match row.as_slice() {
                [EngineValue::Numeric(turso_core::Numeric::Integer(n))] if *n >= 0 => Ok(*n),
                _ => Err(Error::Storage("invalid full-text count".into())),
            },
            _ => Err(Error::Storage("missing full-text count".into())),
        }
    }
    pub(crate) fn delete_index_entry(&self, index: &Index, id: &EngineValue) -> Result<()> {
        if index.kind == IndexKind::Vector {
            return self.delete_vector_entry(index, id);
        }
        let returning = if index.kind == IndexKind::FullText {
            " RETURNING id"
        } else {
            ""
        };
        let rows = self.run(
            &format!(
                "DELETE FROM {} WHERE id=?1{returning}",
                quote(&index.storage)
            ),
            std::slice::from_ref(id),
        )?;
        if index.kind == IndexKind::FullText && !rows.is_empty() {
            self.change_text_count(index, -(rows.len() as i64))?;
        }
        Ok(())
    }
    pub(crate) fn drop_index_storage(&self, index: &Index) -> Result<()> {
        // Explicit DROP INDEX invokes the native method's directory/cache cleanup.
        if index.kind == IndexKind::FullText {
            self.run(&format!("DROP INDEX {}", quote(&index.name)), &[])?;
            self.run(&format!("DROP TABLE {}", quote(&index.text_stats())), &[])?;
        }
        if index.kind == IndexKind::Vector {
            for name in [index.ann_state(), index.ann_log()] {
                self.run(&format!("DROP TABLE {}", quote(&name)), &[])?;
            }
            self.discard_ann_cache(index)?;
        }
        self.run(&format!("DROP TABLE {}", quote(&index.storage)), &[])?;
        Ok(())
    }
    pub(crate) fn text_search_sql(
        &self,
        name: &Value,
        query: &Value,
        limit: &Value,
    ) -> Result<String> {
        let (Value::String(name), Value::String(query)) = (name, query) else {
            return Err(invalid("index and query must be strings"));
        };
        validate_query(query)?;
        let limit = match limit {
            Value::Integer(n) => *n as f64,
            Value::Number(n) => *n,
            _ => return Err(invalid("limit must be an integer from 0 through 10000")),
        };
        if !(0.0..=10_000.0).contains(&limit) || limit.fract() != 0.0 {
            return Err(invalid("limit must be an integer from 0 through 10000"));
        }
        let name = canonical(name)?;
        let index = self
            .collections()?
            .into_iter()
            .flat_map(|c| c.indexes)
            .find(|i| i.name == name)
            .ok_or_else(|| Error::NotFound(format!("text index {name}")))?;
        if index.kind != IndexKind::FullText {
            return Err(invalid("search::text requires a full-text index"));
        }
        // Supplying a positive document-count limit avoids the native method's
        // implicit million-hit cutoff. Materialization prevents outer LIMIT and
        // filters from cutting score ties before stable ID ordering.
        let count = self.text_count(&index)?.max(1);
        let query = format!("'{}'", query.replace('\'', "''"));
        let columns = index.key_columns();
        let inner = format!("SELECT id,fts_score({columns},{query}) AS score FROM {} WHERE fts_match({columns},{query}) LIMIT {count}", quote(&index.storage));
        let plan = self.run(&format!("EXPLAIN QUERY PLAN {inner}"), &[])?;
        if !plan.iter().any(|row| matches!(row.last(), Some(EngineValue::Text(t)) if t.as_str() == "QUERY INDEX METHOD fts")) {
            return Err(Error::Storage("full-text query did not select its native index".into()));
        }
        Ok(format!("WITH __fastdb_text_hits AS MATERIALIZED ({inner}) SELECT id,score FROM __fastdb_text_hits ORDER BY score DESC,id LIMIT {limit}"))
    }
}

fn validate_query(query: &str) -> Result<()> {
    use tantivy_query_grammar::{UserInputAst, UserInputLeaf};
    if query.len() > 4096 {
        return Err(Error::Limit("full-text query exceeds 4096 bytes".into()));
    }
    let ast = tantivy_query_grammar::parse_query(query)
        .map_err(|_| invalid("invalid text query syntax"))?;
    let mut pending = vec![(&ast, 0)];
    while let Some((node, depth)) = pending.pop() {
        if depth > 32 {
            return Err(Error::Limit("full-text query nesting exceeds 32".into()));
        }
        match node {
            UserInputAst::Clause(children) => {
                // The pinned grammar represents `a AND NOT b` as
                // (+a +(-b)). Tantivy treats the negative-only inner clause as
                // empty, producing incorrect Boolean exclusion. Reject that
                // shape instead of returning misleading matches. A positive
                // clause with sibling exclusions (`+a -b`) remains supported.
                if !children.is_empty()
                    && children
                        .iter()
                        .all(|(occur, _)| *occur == Some(tantivy_query_grammar::Occur::MustNot))
                {
                    return Err(invalid(
                        "negative-only query clauses are unsupported; use a positive term with -term in the same clause",
                    ));
                }
                pending.extend(children.iter().map(|(_, node)| (node, depth + 1)))
            }
            UserInputAst::Leaf(leaf)
                if matches!(leaf.as_ref(), UserInputLeaf::Literal(l) if l.field_name.is_none())
                    || matches!(leaf.as_ref(), UserInputLeaf::All) => {}
            _ => {
                return Err(invalid(
                    "queries support unqualified terms, phrases, prefixes, boolean clauses and *",
                ))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (crate::Database, Connection, Index) {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        c.execute(
            "INSERT INTO docs {title:'hello',body:'world'}",
            &crate::Parameters::new(),
        )
        .unwrap();
        c.create_fulltext_index(
            "docs",
            "text_idx",
            vec![vec!["title".into()], vec!["body".into()]],
            false,
        )
        .unwrap();
        let index = c.catalog("docs").unwrap().indexes.remove(0);
        (db, c, index)
    }
    #[test]
    fn text_metadata_and_owned_schema_cannot_be_weakened() {
        let (db, c, index) = setup();
        let catalog = c.catalog("docs").unwrap();
        assert_eq!(catalog.version, 3);
        let original = serde_json::to_value(catalog).unwrap();
        for (path, value) in [
            ("/version", serde_json::json!(2)),
            ("/indexes/0/fulltext/tokenizer", serde_json::json!("other")),
            ("/indexes/0/fulltext", serde_json::Value::Null),
            ("/indexes/0/unique", serde_json::json!(true)),
            (
                "/indexes/0/fulltext/additional_paths",
                serde_json::json!([["title"]]),
            ),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(path).unwrap() = value;
            assert!(crate::catalog::decode(&changed.to_string(), "docs").is_err());
        }
        c.run(&format!("DROP INDEX {}", quote(&index.name)), &[])
            .unwrap();
        assert!(db.connect().is_err());
        assert!(c
            .text_search_sql(
                &Value::String("text_idx".into()),
                &Value::String("hello".into()),
                &Value::Integer(10)
            )
            .is_err());
    }
    #[test]
    fn text_audit_detects_stale_fields_counts_and_missing_entries() {
        for mutation in [
            "UPDATE {storage} SET f1='changed'",
            "UPDATE {stats} SET count=0",
            "DELETE FROM {storage}",
        ] {
            let (_db, c, index) = setup();
            c.run(
                &mutation
                    .replace("{storage}", &quote(&index.storage))
                    .replace("{stats}", &quote(&index.text_stats())),
                &[],
            )
            .unwrap();
            assert!(
                c.check_collection_integrity("docs", Default::default())
                    .is_err(),
                "{mutation}"
            );
        }
    }
}
