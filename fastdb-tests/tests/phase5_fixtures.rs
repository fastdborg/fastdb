#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, Params, StatementResult, Value};

const FORMAT_ONE: &[u8] = include_bytes!("../fixtures/phase3-format1.fastdb");
const MIGRATION_ZERO: &[u8] = include_bytes!("../fixtures/migration-level0.fastdb");

fn assert_fixture(path: &str, expected_migration: i64) {
    let database = Database::open(path).unwrap();
    let connection = database.connect().unwrap();
    let catalog = connection.catalog_state().unwrap();
    let snapshot = catalog.snapshot().unwrap();
    assert_eq!(snapshot.metadata.last_migration, expected_migration);
    assert_eq!(snapshot.tables["person"].fields.len(), 2);
    assert_eq!(snapshot.tables["person"].indexes.len(), 1);

    let mut params = Params::new();
    params.insert("minimum".into(), Value::Integer(2));
    let plan = connection
        .explain_query_with_params("SELECT * FROM person WHERE score >= $minimum", &params)
        .unwrap();
    let physical_index = &snapshot.tables["person"].indexes["by_score"].physical_name;
    assert!(
        plan.iter().any(|line| line.contains(physical_index)),
        "{plan:?}"
    );
    let response = connection
        .execute_with_params(
            "SELECT * FROM person WHERE score >= $minimum ORDER BY id",
            &params,
        )
        .unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(common::integrity_check(connection.native()), "ok");
    connection
        .execute("CREATE person:three SET name='Three', score=3 RETURN NONE")
        .unwrap();
    connection.close().unwrap();
}

#[test]
fn p5_fixture_001_format_one_and_level_zero_migrate_reopen_mutate_and_use_index() {
    let directory = tempdir().unwrap();
    for (name, bytes) in [
        ("phase3-format1.fastdb", FORMAT_ONE),
        ("migration-level0.fastdb", MIGRATION_ZERO),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        let path = path.to_str().unwrap();
        assert_fixture(path, 1);

        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        let metadata = common::native_rows(
            connection.native(),
            "SELECT format_version, dialect_version, last_migration FROM __fastdb_meta",
        );
        assert_eq!(metadata, vec![vec!["1", "1", "1"]]);
        let response = connection.execute("SELECT * FROM person:three").unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(common::integrity_check(connection.native()), "ok");
        connection.close().unwrap();
    }
}
