#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint, StatementResult, Value};

fn logical_hash(connection: &turso_fastdb::Connection) -> u64 {
    let response = connection
        .execute(
            "SELECT * FROM person; SELECT * FROM knows; \
             SELECT id FROM knows WHERE note @@ 'rust'; \
             SELECT id FROM knows WHERE embedding <|8,EUCLIDEAN|> [1,0]; \
             SELECT ->knows->person AS peers FROM person:a",
        )
        .unwrap();
    let mut hasher = DefaultHasher::new();
    format!("{:?}", response.statements).hash(&mut hasher);
    hasher.finish()
}

fn seed_mixed(path: &str) -> (Database, turso_fastdb::Connection) {
    let database = Database::open(path).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE person:a SET ordinal = 1; CREATE person:b SET ordinal = 2; \
             DEFINE TABLE knows TYPE RELATION FROM person TO person ENFORCED; \
             RELATE person:a->knows->person:b SET note = 'rust backup', embedding = [1,0]; \
             DEFINE FIELD embedding ON knows TYPE array<float, 2>; \
             DEFINE ANALYZER blankish TOKENIZERS blank; \
             DEFINE INDEX note_idx ON knows FIELDS note FULLTEXT ANALYZER blankish",
        )
        .unwrap();
    let mut state = 0x5eed_u64;
    let mut randomized = String::new();
    for ordinal in 0..64 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        randomized.push_str(&format!(
            "CREATE person:p{ordinal} SET ordinal = {};",
            state % 10_000
        ));
    }
    connection.execute(&randomized).unwrap();
    (database, connection)
}

#[test]
fn p10_operations_001_mixed_provider_backup_has_identical_logical_hash() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.fastdb");
    let backup = directory.path().join("backup.fastdb");
    let (database, connection) = seed_mixed(source.to_str().unwrap());
    let before = logical_hash(&connection);
    connection.close().unwrap();

    let report = database.backup_to(&backup).unwrap();
    assert_eq!(report.tables, 2);
    assert_eq!(report.fts_indexes, 1);
    assert_eq!(report.vector_fields, 1);
    assert!(report.pinned_fts_exception);

    let restored = Database::open(backup.to_str().unwrap()).unwrap();
    let restored_connection = restored.connect().unwrap();
    assert_eq!(logical_hash(&restored_connection), before);
    restored.rebuild_index("knows", "note_idx").unwrap();
    let explained = restored_connection
        .execute("EXPLAIN SELECT id FROM knows WHERE note @@ 'rust'")
        .unwrap();
    assert!(matches!(
        &explained.statements[0],
        StatementResult::Rows(rows)
            if rows.iter().any(|value| matches!(value, Value::Object(row)
                if matches!(row.get("detail"), Some(Value::Str(detail))
                    if detail.contains("FTS"))))
    ));
}

#[test]
fn p10_operations_002_interrupted_backup_never_publishes_partial_output() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.fastdb");
    let destination = directory.path().join("backup.fastdb");
    let (database, connection) = seed_mixed(source.to_str().unwrap());
    let before = logical_hash(&connection);
    connection.close().unwrap();

    let error = database
        .backup_to_with_failpoint(&destination, Failpoint::AfterBackupCopy)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Transaction);
    assert!(!destination.exists());
    assert!(std::fs::read_dir(directory.path())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("fastdb-backup")));

    let reopened_connection = database.connect().unwrap();
    assert_eq!(logical_hash(&reopened_connection), before);
}

#[test]
fn p10_operations_003_check_rejects_catalog_and_adjacency_corruption() {
    let directory = tempdir().unwrap();
    let catalog_path = directory.path().join("catalog.fastdb");
    let (catalog_database, catalog_connection) = seed_mixed(catalog_path.to_str().unwrap());
    common::native_exec(
        catalog_connection.native(),
        "UPDATE __fastdb_hidden_columns SET encoding_version = 999 \
         WHERE provider = 'BUILTIN_VECTOR_EXACT'",
    );
    let error = catalog_database.check().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Format);
    assert!(
        error.to_string().contains("unsupported version or state"),
        "unexpected check error: {error}"
    );

    let graph_path = directory.path().join("graph.fastdb");
    let (graph_database, graph_connection) = seed_mixed(graph_path.to_str().unwrap());
    let indexes = common::native_rows(
        graph_connection.native(),
        "SELECT physical_name FROM __fastdb_indexes \
         WHERE index_kind = 'GRAPH_ADJACENCY' LIMIT 1",
    );
    let index = &indexes[0][0];
    common::native_exec(graph_connection.native(), &format!("DROP INDEX {index}"));
    let error = graph_database.check().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Format);
    assert!(
        error.to_string().contains("physical object") && error.to_string().contains("is missing"),
        "unexpected check error: {error}"
    );
}
