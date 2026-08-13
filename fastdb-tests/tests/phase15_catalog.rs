#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory};

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
