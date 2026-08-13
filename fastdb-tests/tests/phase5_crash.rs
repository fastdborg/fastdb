#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::process::Command;
use tempfile::tempdir;
use turso_fastdb::{Database, StatementResult};

#[test]
fn p5_crash_001_frontend_publication_boundaries_recover_and_remain_integral() {
    let cases = [
        ("bootstrap", None),
        ("implicit-registration", Some("item:implicit")),
        ("schema-ddl", None),
        ("index-ddl", Some("item:indexed")),
        ("write", Some("item:written")),
        ("commit-publication", Some("item:committed")),
        ("rollback", Some("item:rolled_back")),
        ("clean-close", Some("item:closed")),
    ];
    for (point, target) in cases {
        let directory = tempdir().unwrap();
        let path = directory.path().join(format!("{point}.fastdb"));
        let status = Command::new(env!("CARGO_BIN_EXE_phase5_crash_helper"))
            .arg(point)
            .arg(&path)
            .status()
            .unwrap();
        if point == "clean-close" {
            assert!(status.success());
        } else {
            assert_eq!(status.code(), Some(86));
        }

        let database = Database::open(path.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            common::integrity_check(connection.native()),
            "ok",
            "{point}"
        );
        if let Some(target) = target {
            let response = connection
                .execute(&format!("SELECT * FROM {target}"))
                .unwrap();
            let StatementResult::Rows(rows) = &response.statements[0] else {
                panic!("expected rows")
            };
            let expected = usize::from(point != "rollback");
            assert_eq!(rows.len(), expected, "{point}");
        }
        if point == "index-ddl" {
            let plan = connection
                .explain_query_with_params(
                    "SELECT * FROM item WHERE n=1",
                    &turso_fastdb::Params::new(),
                )
                .unwrap();
            assert!(
                plan.iter().any(|line| line.contains("__fastdb_i_")),
                "{plan:?}"
            );
        }
        connection.close().unwrap();
    }
}
