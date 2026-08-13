#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint};

const FORMAT_TWO: &[u8] = include_bytes!("../fixtures/phase6-format2.fastdb");
const FORMAT_THREE: &[u8] = include_bytes!("../fixtures/phase12-format3.fastdb");

#[test]
fn p12_format_001_format_two_migrates_once_to_complete_format_three() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("format2.fastdb");
    std::fs::write(&file, FORMAT_TWO).unwrap();
    {
        let database = Database::open(file.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            common::native_rows(
                connection.native(),
                "SELECT format_version,last_migration,document_encoding_version FROM __fastdb_meta",
            ),
            vec![vec!["3", "3", "2"]]
        );
        let catalogs = common::native_rows(
            connection.native(),
            "SELECT name FROM sqlite_schema WHERE type='table' AND name LIKE '__fastdb_%' ORDER BY name",
        );
        for expected in [
            "__fastdb_accesses",
            "__fastdb_events",
            "__fastdb_functions",
            "__fastdb_parameters",
            "__fastdb_permissions",
            "__fastdb_users",
            "__fastdb_views",
        ] {
            assert!(catalogs.iter().any(|row| row[0] == expected), "{expected}");
        }
        assert_eq!(common::integrity_check(connection.native()), "ok");
        connection.close().unwrap();
    }
    let before = std::fs::read(&file).unwrap();
    let database = Database::open(file.to_str().unwrap()).unwrap();
    database.connect().unwrap().close().unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn p12_format_002_each_format_three_boundary_rolls_back_to_format_two() {
    let directory = tempdir().unwrap();
    for failpoint in [
        Failpoint::AfterFormat3Metadata,
        Failpoint::AfterFormat3ProviderColumns,
        Failpoint::AfterFormat3Catalogs,
        Failpoint::AfterFormat3Validation,
        Failpoint::AfterMigration,
    ] {
        let file = directory.path().join(format!("{failpoint:?}.fastdb"));
        std::fs::write(&file, FORMAT_TWO).unwrap();
        let error =
            Database::open_with_catalog_failpoint(file.to_str().unwrap(), failpoint).unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::Transaction,
            "{failpoint:?}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), FORMAT_TWO, "{failpoint:?}");

        let database = Database::open(file.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            common::native_rows(
                connection.native(),
                "SELECT format_version,last_migration,document_encoding_version FROM __fastdb_meta",
            ),
            vec![vec!["3", "3", "2"]]
        );
        connection.close().unwrap();
    }
}

#[test]
fn p12_format_003_unknown_encoding_and_sealed_catalog_rows_fail_closed() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        (
            "document-encoding",
            "UPDATE __fastdb_meta SET document_encoding_version=99",
        ),
        (
            "provider-auxiliary",
            "UPDATE __fastdb_indexes SET auxiliary_version=99",
        ),
        (
            "future-catalog-row",
            "INSERT INTO __fastdb_functions VALUES ('00000000000000000000000000000001','future','[]','RETURN NONE',1,'{}','DEFINE FUNCTION fn::future() { RETURN NONE; }')",
        ),
    ] {
        let file = directory.path().join(format!("{name}.fastdb"));
        {
            let database = Database::open(file.to_str().unwrap()).unwrap();
            let connection = database.connect().unwrap();
            connection
                .execute(
                    "CREATE item:one SET n=1; DEFINE INDEX by_n ON item FIELDS n",
                )
                .unwrap();
            if name == "document-encoding" {
                common::native_exec(connection.native(), "PRAGMA ignore_check_constraints=ON");
            }
            common::native_exec(connection.native(), mutation);
            if name == "document-encoding" {
                common::native_exec(connection.native(), "PRAGMA ignore_check_constraints=OFF");
            }
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
fn p12_format_004_committed_format_three_fixture_reopens_and_uses_index() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("format3.fastdb");
    std::fs::write(&file, FORMAT_THREE).unwrap();
    let database = Database::open(file.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    assert_eq!(
        common::native_rows(
            connection.native(),
            "SELECT format_version,last_migration,document_encoding_version FROM __fastdb_meta",
        ),
        vec![vec!["3", "3", "2"]]
    );
    let rows = connection.execute("SELECT * FROM person:phase12").unwrap();
    assert_eq!(rows.mutation_count, 0);
    let physical_index = connection
        .catalog_state()
        .unwrap()
        .snapshot()
        .unwrap()
        .tables["person"]
        .indexes["by_score"]
        .physical_name
        .clone();
    let plan = connection
        .explain_query_with_params(
            "SELECT * FROM person WHERE score >= 12",
            &turso_fastdb::Params::new(),
        )
        .unwrap();
    assert!(plan.iter().any(|detail| detail.contains(&physical_index)));
    connection
        .execute("CREATE person:phase12b SET name='Phase 12b', score=13")
        .unwrap();
    assert_eq!(common::integrity_check(connection.native()), "ok");
}
