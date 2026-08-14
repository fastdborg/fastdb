#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint, StatementResult};

#[test]
fn p15_catalog_001_parameter_corruption_fails_closed_without_mutation() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        (
            "value",
            "UPDATE __fastdb_parameters SET value_json='{'",
        ),
        (
            "encoding",
            "UPDATE __fastdb_parameters SET encoding_version=99",
        ),
        (
            "ownership",
            "UPDATE __fastdb_parameters SET definition='DEFINE PARAM $other VALUE 1 PERMISSIONS FULL'",
        ),
    ] {
        let file = directory.path().join(format!("{name}.fastdb"));
        {
            let database = Database::open(file.to_str().unwrap()).unwrap();
            let connection = database.connect().unwrap();
            connection
                .execute("DEFINE PARAM $stable VALUE { n: 1 }")
                .unwrap();
            common::native_exec(connection.native(), mutation);
            connection.close().unwrap();
        }
        let before = std::fs::read(&file).unwrap();
        assert_eq!(
            Database::open(file.to_str().unwrap())
                .unwrap_err()
                .category(),
            ErrorCategory::Format,
            "{name}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), before, "{name}");
    }
}

#[test]
fn p15_catalog_002_function_corruption_fails_closed_without_mutation() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        (
            "arguments",
            "UPDATE __fastdb_functions SET arguments_ast='['",
        ),
        ("version", "UPDATE __fastdb_functions SET ast_version=99"),
        (
            "ownership",
            "UPDATE __fastdb_functions SET logical_name='other'",
        ),
    ] {
        let file = directory.path().join(format!("function-{name}.fastdb"));
        {
            let database = Database::open(file.to_str().unwrap()).unwrap();
            let connection = database.connect().unwrap();
            connection
                .execute("DEFINE FUNCTION fn::stable($x: int) { RETURN $x; }")
                .unwrap();
            common::native_exec(connection.native(), mutation);
            connection.close().unwrap();
        }
        let before = std::fs::read(&file).unwrap();
        assert_eq!(
            Database::open(file.to_str().unwrap())
                .unwrap_err()
                .category(),
            ErrorCategory::Format,
            "{name}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), before, "{name}");
    }
}

#[test]
fn p15_catalog_003_table_and_field_metadata_corruption_fails_closed() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        (
            "table-owner",
            "UPDATE __fastdb_tables SET definition='DEFINE TABLE other TYPE NORMAL SCHEMAFULL PERMISSIONS NONE' WHERE logical_name='item'",
        ),
        (
            "table-kind",
            "UPDATE __fastdb_tables SET definition='DEFINE TABLE item TYPE RELATION SCHEMAFULL PERMISSIONS NONE' WHERE logical_name='item'",
        ),
        (
            "field-type",
            "UPDATE __fastdb_fields SET definition='DEFINE FIELD score ON item TYPE string PERMISSIONS FULL'",
        ),
        (
            "field-expression",
            "UPDATE __fastdb_fields SET definition='DEFINE FIELD score ON item TYPE int ASSERT ('",
        ),
    ] {
        let file = directory.path().join(format!("schema-{name}.fastdb"));
        {
            let database = Database::open(file.to_str().unwrap()).unwrap();
            let connection = database.connect().unwrap();
            connection
                .execute(
                    "DEFINE TABLE item SCHEMAFULL TYPE NORMAL; \
                     DEFINE FIELD score ON item TYPE int DEFAULT 1 ASSERT $value >= 0",
                )
                .unwrap();
            common::native_exec(connection.native(), mutation);
            connection.close().unwrap();
        }
        let before = std::fs::read(&file).unwrap();
        assert_eq!(
            Database::open(file.to_str().unwrap())
                .unwrap_err()
                .category(),
            ErrorCategory::Format,
            "{name}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), before, "{name}");
    }
}

#[test]
fn p15_catalog_004_event_corruption_fails_closed_without_mutation() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        ("owner", "UPDATE __fastdb_events SET logical_name='other'"),
        (
            "version",
            "UPDATE __fastdb_events SET expression_version=99",
        ),
        (
            "action",
            "UPDATE __fastdb_events SET then_source='{ RETURN $before; }'",
        ),
    ] {
        let file = directory.path().join(format!("event-{name}.fastdb"));
        {
            let database = Database::open(file.to_str().unwrap()).unwrap();
            let connection = database.connect().unwrap();
            connection
                .execute(
                    "DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
                     DEFINE EVENT stable ON item THEN { RETURN $after; }",
                )
                .unwrap();
            common::native_exec(connection.native(), mutation);
            connection.close().unwrap();
        }
        let before = std::fs::read(&file).unwrap();
        assert_eq!(
            Database::open(file.to_str().unwrap())
                .unwrap_err()
                .category(),
            ErrorCategory::Format,
            "{name}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), before, "{name}");
    }
}

#[test]
fn p15_catalog_005_event_boundary_failure_rolls_back_source_and_action() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
             DEFINE EVENT audit ON item THEN { CREATE log; }",
        )
        .unwrap();
    connection.arm_failpoint(Failpoint::BeforeEventActions);
    assert_eq!(
        connection
            .execute("CREATE item:rolled_back")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    for table in ["item", "log"] {
        assert!(matches!(
            connection
                .execute(&format!("SELECT * FROM {table}"))
                .unwrap()
                .statements[0],
            StatementResult::Rows(ref rows) if rows.is_empty()
        ));
    }
    connection.close().unwrap();
}

#[test]
fn p15_catalog_006_view_catalog_corruption_fails_closed() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        ("version", "UPDATE __fastdb_views SET ast_version=99"),
        (
            "dependencies",
            "UPDATE __fastdb_views SET dependencies_json='[]'",
        ),
        (
            "ownership",
            "UPDATE __fastdb_views SET logical_name='other'",
        ),
    ] {
        let file = directory.path().join(format!("view-{name}.fastdb"));
        {
            let database = Database::open(file.to_str().unwrap()).unwrap();
            let connection = database.connect().unwrap();
            connection
                .execute(
                    "DEFINE TABLE source SCHEMALESS; \
                     CREATE source:a SET n=1; \
                     DEFINE TABLE derived AS SELECT n FROM source",
                )
                .unwrap();
            common::native_exec(connection.native(), mutation);
            connection.close().unwrap();
        }
        let before = std::fs::read(&file).unwrap();
        assert_eq!(
            Database::open(file.to_str().unwrap())
                .unwrap_err()
                .category(),
            ErrorCategory::Format,
            "{name}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), before, "{name}");
    }

    let file = directory.path().join("view-derived.fastdb");
    {
        let database = Database::open(file.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE TABLE source SCHEMALESS; \
                 CREATE source:a SET n=1; \
                 DEFINE TABLE derived AS SELECT n FROM source",
            )
            .unwrap();
        let physical = common::physical_name_for(connection.native(), "derived").unwrap();
        common::native_exec(
            connection.native(),
            &format!("UPDATE {physical} SET doc=jsonb('{{\"n\":9}}')"),
        );
        connection.close().unwrap();
    }
    let before = std::fs::read(&file).unwrap();
    assert_eq!(
        Database::open(file.to_str().unwrap())
            .unwrap_err()
            .category(),
        ErrorCategory::Format
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn p15_catalog_007_view_publication_boundaries_roll_back_source_and_derived_rows() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("DEFINE TABLE source SCHEMALESS; CREATE source:a SET n=1")
        .unwrap();

    connection.arm_failpoint(Failpoint::AfterViewCatalog);
    assert_eq!(
        connection
            .execute("DEFINE TABLE derived AS SELECT n FROM source")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    let StatementResult::Rows(rows) = &connection
        .execute("SELECT * FROM derived")
        .unwrap()
        .statements[0]
    else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());

    connection
        .execute("DEFINE TABLE derived AS SELECT n FROM source")
        .unwrap();
    connection.arm_failpoint(Failpoint::DuringViewRefresh);
    assert_eq!(
        connection
            .execute("UPDATE source:a SET n=2")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    for table in ["source", "derived"] {
        let StatementResult::Rows(rows) = &connection
            .execute(&format!("SELECT VALUE n FROM {table}:a"))
            .unwrap()
            .statements[0]
        else {
            panic!("expected rows")
        };
        assert_eq!(rows, &vec![turso_fastdb::Value::Integer(1)], "{table}");
    }
}
