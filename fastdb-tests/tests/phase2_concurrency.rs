#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory};

#[test]
fn p2_conc_001_duplicate_define_race_has_one_owner_and_no_orphan() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("define-race.fastdb");
    let path = path.to_str().unwrap().to_string();
    let db = Database::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let database = db.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let connection = database.connect().unwrap();
                barrier.wait();
                connection.execute("DEFINE TABLE person SCHEMALESS")
            })
        })
        .collect::<Vec<_>>();
    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter_map(|outcome| outcome.as_ref().err())
            .filter(|error| error.category() == ErrorCategory::Constraint)
            .count(),
        1
    );
    let conn = db.connect().unwrap();
    assert_eq!(
        common::native_rows(conn.native(), "SELECT logical_name FROM __fastdb_tables").len(),
        1
    );
    assert_eq!(
        common::native_rows(
            conn.native(),
            "SELECT name FROM sqlite_schema WHERE type='table' AND name LIKE '__fastdb_t_%' AND length(name)=43",
        )
        .len(),
        1
    );
}

#[test]
fn p2_conc_002_simultaneous_implicit_registration_commits_one_table_two_rows() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("implicit-race.fastdb");
    let path = path.to_str().unwrap().to_string();
    let db = Database::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles = ["tracy", "jaime"]
        .into_iter()
        .map(|id| {
            let database = db.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let connection = database.connect().unwrap();
                barrier.wait();
                connection
                    .execute(&format!("CREATE person:{id} SET name='{id}'"))
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    let conn = db.connect().unwrap();
    assert_eq!(
        conn.execute("SELECT * FROM person")
            .unwrap()
            .legacy_records()
            .len(),
        2
    );
    assert_eq!(
        common::native_rows(conn.native(), "SELECT logical_name FROM __fastdb_tables").len(),
        1
    );
}

#[test]
fn p2_conc_003_concurrent_opens_of_empty_database_remain_read_only() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("open-race.fastdb");
    let path = path.to_str().unwrap().to_string();
    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                let database = Database::open(&path).unwrap();
                let connection = database.connect().unwrap();
                assert!(
                    common::native_rows(connection.native(), "SELECT name FROM sqlite_schema")
                        .is_empty()
                );
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
}
