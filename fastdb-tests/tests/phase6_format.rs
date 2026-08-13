#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::process::Command;
use tempfile::tempdir;
use turso_fastdb::catalog::{IndexKind, Provider, ProviderState, TableKind};
use turso_fastdb::{Database, ErrorCategory, Failpoint, Params, StatementResult, Value};

const FORMAT_ONE: &[u8] = include_bytes!("../fixtures/phase3-format1.fastdb");
const FORMAT_TWO: &[u8] = include_bytes!("../fixtures/phase6-format2.fastdb");

#[test]
fn p6_fmt_001_format_one_migrates_transactionally_and_reopens_as_format_two() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("migrate.fastdb");
    std::fs::write(&file, FORMAT_ONE).unwrap();
    let path = file.to_str().unwrap();

    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        let snapshot = connection
            .catalog_state()
            .unwrap()
            .snapshot()
            .unwrap()
            .clone();
        assert_eq!(snapshot.metadata.last_migration, 2);
        let table = &snapshot.tables["person"];
        assert_eq!(table.kind, TableKind::Normal);
        assert_eq!(table.relation_in_table_id, None);
        assert_eq!(table.relation_out_table_id, None);
        assert!(!table.relation_enforced);
        let index = &table.indexes["by_score"];
        assert_eq!(index.kind, IndexKind::Btree);
        assert_eq!(index.provider, Provider::BuiltinBtree);
        assert_eq!(index.provider_version, 1);
        assert_eq!(index.options_json, "{}");
        assert_eq!(index.state, ProviderState::Ready);
        assert_eq!(index.encoding_version, 1);
        assert!(snapshot.analyzers.is_empty());
        assert!(snapshot.hidden_columns.is_empty());
        assert!(snapshot.capabilities.is_empty());
        assert_eq!(
            common::native_rows(
                connection.native(),
                "SELECT format_version,dialect_version,last_migration FROM __fastdb_meta",
            ),
            vec![vec!["2", "1", "2"]]
        );
        assert_eq!(common::integrity_check(connection.native()), "ok");
        connection.close().unwrap();
    }

    let database = Database::open(path).unwrap();
    let connection = database.connect().unwrap();
    let response = connection
        .execute("SELECT * FROM person WHERE score >= 2")
        .unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), 1);
    let plan = connection
        .explain_query_with_params("SELECT * FROM person WHERE score >= 2", &Params::new())
        .unwrap();
    let physical_index = connection
        .catalog_state()
        .unwrap()
        .snapshot()
        .unwrap()
        .tables["person"]
        .indexes["by_score"]
        .physical_name
        .clone();
    assert!(plan.iter().any(|line| line.contains(&physical_index)));
}

#[test]
fn p6_fmt_002_every_format_two_migration_boundary_rolls_back() {
    let directory = tempdir().unwrap();
    for failpoint in [
        Failpoint::AfterFormat2TableColumns,
        Failpoint::AfterFormat2IndexColumns,
        Failpoint::AfterFormat2Catalogs,
        Failpoint::AfterFormat2Validation,
        Failpoint::AfterMigration,
    ] {
        let file = directory.path().join(format!("{failpoint:?}.fastdb"));
        std::fs::write(&file, FORMAT_ONE).unwrap();
        let path = file.to_str().unwrap();
        assert_eq!(
            Database::open_with_catalog_failpoint(path, failpoint)
                .unwrap_err()
                .category(),
            ErrorCategory::Transaction,
            "{failpoint:?}"
        );
        assert_eq!(
            std::fs::read(&file).unwrap(),
            FORMAT_ONE,
            "main database bytes changed after {failpoint:?} rollback"
        );

        // Reaching the first migration boundary again proves the failed
        // transaction did not leave any format-2 DDL or header publication.
        assert_eq!(
            Database::open_with_catalog_failpoint(path, Failpoint::AfterFormat2TableColumns,)
                .unwrap_err()
                .category(),
            ErrorCategory::Transaction,
            "retry after {failpoint:?}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), FORMAT_ONE);

        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            common::native_rows(
                connection.native(),
                "SELECT format_version,last_migration FROM __fastdb_meta",
            ),
            vec![vec!["2", "2"]]
        );
        assert_eq!(common::integrity_check(connection.native()), "ok");
        connection.close().unwrap();
    }
}

#[test]
fn p6_fmt_006_committed_migration_survives_abrupt_process_exit() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("migration-crash.fastdb");
    std::fs::write(&file, FORMAT_ONE).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_phase5_crash_helper"))
        .arg("migrate-format2")
        .arg(&file)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));

    let database = Database::open(file.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    assert_eq!(
        common::native_rows(
            connection.native(),
            "SELECT format_version,last_migration FROM __fastdb_meta",
        ),
        vec![vec!["2", "2"]]
    );
    assert_eq!(common::integrity_check(connection.native()), "ok");
    let plan = connection
        .explain_query_with_params("SELECT * FROM person WHERE score >= 2", &Params::new())
        .unwrap();
    assert!(plan.iter().any(|detail| detail.contains("__fastdb_i_")));
}

#[test]
fn p6_fmt_003_unknown_capabilities_and_providers_fail_without_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE person:one SET score=1; DEFINE INDEX by_score ON person FIELDS score;")
        .unwrap();
    let before = connection.catalog_state().unwrap();

    common::native_exec(
        connection.native(),
        "INSERT INTO __fastdb_capabilities VALUES ('unknown_provider',1,1)",
    );
    assert_eq!(
        connection.reload_catalog().unwrap_err().category(),
        ErrorCategory::Format
    );
    assert_eq!(connection.catalog_state().unwrap(), before);
    assert_eq!(
        common::native_rows(
            connection.native(),
            "SELECT provider FROM __fastdb_capabilities",
        ),
        vec![vec!["unknown_provider"]]
    );
    common::native_exec(connection.native(), "DELETE FROM __fastdb_capabilities");

    common::native_exec(
        connection.native(),
        "UPDATE __fastdb_indexes SET provider='unknown_provider'",
    );
    assert_eq!(
        connection.reload_catalog().unwrap_err().category(),
        ErrorCategory::Format
    );
    assert_eq!(connection.catalog_state().unwrap(), before);
    assert_eq!(
        common::native_rows(connection.native(), "SELECT provider FROM __fastdb_indexes",),
        vec![vec!["unknown_provider"]]
    );
}

#[test]
fn p6_fmt_004_committed_format_two_fixture_reopens_mutates_and_uses_index() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("format2.fastdb");
    std::fs::write(&file, FORMAT_TWO).unwrap();
    let path = file.to_str().unwrap();
    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            common::native_rows(
                connection.native(),
                "SELECT format_version,last_migration FROM __fastdb_meta",
            ),
            vec![vec!["2", "2"]]
        );
        let snapshot = connection.catalog_state().unwrap();
        let snapshot = snapshot.snapshot().unwrap();
        let physical_index = &snapshot.tables["person"].indexes["by_score"].physical_name;
        let plan = connection
            .explain_query_with_params("SELECT * FROM person WHERE score >= 2", &Params::new())
            .unwrap();
        assert!(plan.iter().any(|detail| detail.contains(physical_index)));
        connection
            .execute("CREATE person:three SET name='Three', score=3 RETURN NONE")
            .unwrap();
        assert_eq!(common::integrity_check(connection.native()), "ok");
        connection.close().unwrap();
    }
    let database = Database::open(path).unwrap();
    let connection = database.connect().unwrap();
    let response = connection.execute("SELECT * FROM person:three").unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(common::integrity_check(connection.native()), "ok");
}

#[test]
fn p6_fmt_005_refuses_every_unavailable_btree_contract_value() {
    for (name, mutation) in [
        ("kind", "UPDATE __fastdb_indexes SET index_kind='FULLTEXT'"),
        ("provider", "UPDATE __fastdb_indexes SET provider='UNKNOWN'"),
        (
            "provider_version",
            "UPDATE __fastdb_indexes SET provider_version=2",
        ),
        (
            "options",
            "UPDATE __fastdb_indexes SET options_json='{\"future\":true}'",
        ),
        (
            "known_state",
            "UPDATE __fastdb_indexes SET state='REBUILD_REQUIRED'",
        ),
        (
            "unknown_state",
            "UPDATE __fastdb_indexes SET state='FUTURE'",
        ),
        ("encoding", "UPDATE __fastdb_indexes SET encoding_version=2"),
    ] {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:one SET score=1 RETURN NONE; \
                 DEFINE INDEX by_score ON person FIELDS score",
            )
            .unwrap();
        let before = connection.catalog_state().unwrap();
        common::native_exec(connection.native(), mutation);
        assert_eq!(
            connection.reload_catalog().unwrap_err().category(),
            ErrorCategory::Format,
            "{name}"
        );
        assert_eq!(connection.catalog_state().unwrap(), before, "{name}");
    }

    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE person:one SET score=1 RETURN NONE")
        .unwrap();
    let before = connection.catalog_state().unwrap();
    common::native_exec(
        connection.native(),
        "UPDATE __fastdb_tables SET kind='RELATION'",
    );
    assert_eq!(
        connection.reload_catalog().unwrap_err().category(),
        ErrorCategory::Format
    );
    assert_eq!(connection.catalog_state().unwrap(), before);
}

#[test]
fn p6_provider_001_document_and_hidden_state_commit_or_roll_back_together() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    let original = r#"{"name":"before"}"#;
    let changed = r#"{"name":"after","score":7}"#;
    let handle = connection.install_test_provider(original).unwrap();
    assert_eq!(
        connection.read_test_provider(&handle).unwrap(),
        (original.to_string(), i64::try_from(original.len()).unwrap())
    );

    connection.arm_failpoint(Failpoint::AfterTestProviderDocument);
    assert_eq!(
        connection
            .write_test_provider(&handle, changed)
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    assert_eq!(
        connection.read_test_provider(&handle).unwrap(),
        (original.to_string(), i64::try_from(original.len()).unwrap())
    );

    connection.write_test_provider(&handle, changed).unwrap();
    assert_eq!(
        connection.read_test_provider(&handle).unwrap(),
        (changed.to_string(), i64::try_from(changed.len()).unwrap())
    );
    assert_eq!(common::integrity_check(connection.native()), "ok");
}

#[test]
fn p6_lang_001_expression_projections_alias_without_mutating_documents() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE person:one CONTENT {score: 2, nested: {amount: 4}} RETURN NONE")
        .unwrap();
    let response = connection
        .execute("SELECT score + nested.amount AS total, nested.amount FROM person:one")
        .unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), 1);
    let Value::Object(projected) = &rows[0] else {
        panic!("expected object")
    };
    assert_eq!(projected["total"], Value::Integer(6));
    assert_eq!(
        projected["nested"],
        Value::Object(std::collections::BTreeMap::from([(
            "amount".to_string(),
            Value::Integer(4),
        )]))
    );

    let stored = connection.execute("SELECT * FROM person:one").unwrap();
    let StatementResult::Rows(stored_rows) = &stored.statements[0] else {
        panic!("expected rows")
    };
    let Value::Object(stored_document) = &stored_rows[0] else {
        panic!("expected object")
    };
    assert!(!stored_document.contains_key("total"));
}

#[test]
fn p6_lang_002_functions_and_unaliased_expressions_fail_before_catalog_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    assert_eq!(
        connection
            .execute("CREATE person CONTENT search::score(1)")
            .unwrap_err()
            .category(),
        ErrorCategory::UnsupportedSyntax
    );
    assert!(matches!(
        connection.catalog_state().unwrap(),
        turso_fastdb::catalog::CatalogState::Empty
    ));
    assert_eq!(
        connection
            .execute("SELECT vector::distance::cosine([1], [1]) AS distance FROM missing")
            .unwrap_err()
            .category(),
        ErrorCategory::UnsupportedSyntax
    );
    assert_eq!(
        connection
            .execute("SELECT 1 + 2 FROM missing")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    assert_eq!(
        connection
            .execute("DEFINE INDEX body ON article FIELDS body FULLTEXT ANALYZER blank")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    assert!(matches!(
        connection.catalog_state().unwrap(),
        turso_fastdb::catalog::CatalogState::Empty
    ));
    assert_eq!(
        connection
            .execute("DEFINE INDEX body ON article FIELDS body USING fts WITH (tokenizer='simple')")
            .unwrap_err()
            .category(),
        ErrorCategory::UnsupportedSyntax
    );
}

#[test]
fn p6_index_001_explain_rebuild_and_remove_are_structured_and_atomic() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("index-maintenance.fastdb");
    let path = file.to_str().unwrap();
    let physical_index;
    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:one SET score=1 RETURN NONE; \
                 CREATE person:two SET score=2 RETURN NONE; \
                 DEFINE INDEX by_score ON TABLE person FIELDS score",
            )
            .unwrap();
        physical_index = connection
            .catalog_state()
            .unwrap()
            .snapshot()
            .unwrap()
            .tables["person"]
            .indexes["by_score"]
            .physical_name
            .clone();

        let explained = connection
            .execute("EXPLAIN SELECT * FROM person WHERE score = 1")
            .unwrap();
        let StatementResult::Rows(plan_rows) = &explained.statements[0] else {
            panic!("expected structured explain rows")
        };
        assert!(!plan_rows.is_empty());
        assert!(plan_rows.iter().any(|row| {
            matches!(row, Value::Object(values) if matches!(values.get("detail"), Some(Value::Str(detail)) if detail.contains(&physical_index)) && values.contains_key("ordinal"))
        }));

        connection.arm_failpoint(Failpoint::AfterIndexRebuild);
        assert_eq!(
            connection
                .execute("REBUILD INDEX by_score ON person")
                .unwrap_err()
                .category(),
            ErrorCategory::Transaction
        );
        connection.disarm_all_failpoints();
        connection
            .execute("REBUILD INDEX by_score ON person")
            .unwrap();

        for failpoint in [
            Failpoint::AfterIndexRemovePhysical,
            Failpoint::AfterIndexRemoveCatalog,
        ] {
            connection.arm_failpoint(failpoint);
            assert_eq!(
                connection
                    .execute("REMOVE INDEX by_score ON TABLE person")
                    .unwrap_err()
                    .category(),
                ErrorCategory::Transaction,
                "{failpoint:?}"
            );
            connection.disarm_all_failpoints();
            assert!(connection
                .catalog_state()
                .unwrap()
                .snapshot()
                .unwrap()
                .tables["person"]
                .indexes
                .contains_key("by_score"));
            assert_eq!(
                common::native_rows(
                    connection.native(),
                    &format!(
                        "SELECT name FROM sqlite_schema WHERE type='index' AND name='{physical_index}'"
                    ),
                ),
                vec![vec![physical_index.clone()]]
            );
        }

        connection
            .execute("REMOVE INDEX by_score ON person")
            .unwrap();
        assert!(!connection
            .catalog_state()
            .unwrap()
            .snapshot()
            .unwrap()
            .tables["person"]
            .indexes
            .contains_key("by_score"));
        assert!(common::native_rows(
            connection.native(),
            &format!(
                "SELECT name FROM sqlite_schema WHERE type='index' AND name='{physical_index}'"
            ),
        )
        .is_empty());
        connection.close().unwrap();
    }

    let database = Database::open(path).unwrap();
    let connection = database.connect().unwrap();
    assert!(!connection
        .catalog_state()
        .unwrap()
        .snapshot()
        .unwrap()
        .tables["person"]
        .indexes
        .contains_key("by_score"));
    let response = connection
        .execute("SELECT * FROM person WHERE score = 1")
        .unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(common::integrity_check(connection.native()), "ok");
}
