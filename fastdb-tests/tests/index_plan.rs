//! P0.9 — Canonical JSON expression index and execution-plan proof.
//!
//! Installs the test-only non-unique expression index on `name`, proves the
//! equality filter returns the correct record, asserts `EXPLAIN QUERY PLAN`
//! selects the opaque index (not a full scan) before and after reopen, and
//! shows the filter stops matching a deleted record.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::{tempdir, TempDir};
use turso_fastdb::{Database, Value};

fn fresh_db() -> (TempDir, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("i.fastdb");
    let s = path.to_str().unwrap().to_string();
    (dir, s)
}

fn seed_three(path: &str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE person:tobie SET name = 'Tobie';").unwrap();
    conn.execute("CREATE person:jaime SET name = 'Jaime';").unwrap();
    conn.execute("CREATE person:nikola SET name = 'Nikola';").unwrap();
}

/// Assert the plan selects the index (mentions its name) and is not a plain
/// full table scan of the hidden physical table.
fn assert_uses_index(plans: &[String], idx_name: &str) {
    assert!(
        plans.iter().any(|p| p.contains(idx_name)),
        "expected plan to mention index {idx_name}; got {plans:?}"
    );
    assert!(
        !plans
            .iter()
            .any(|p| p.contains("SCAN") && !p.contains(idx_name)),
        "plan shows an unindexed full scan: {plans:?}"
    );
}

#[test]
fn expression_index_selected_before_and_after_reopen() {
    let (_dir, path) = fresh_db();
    let idx_name = {
        seed_three(&path);
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        let idx = conn.create_field_index("person", "name").unwrap();

        // Correct result via the FastDB filter path.
        let r = conn
            .execute("SELECT * FROM person WHERE name = 'Jaime';")
            .unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].id.id, "jaime");
        assert_eq!(
            r.records[0].fields,
            vec![("name".to_string(), Value::Str("Jaime".to_string()))]
        );

        // Plan selects the opaque index before reopen.
        let plans = conn.explain_field_filter("person", "name").unwrap();
        assert_uses_index(&plans, &idx);
        idx
    };

    // After reopen: same result and same index selection.
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        let r = conn
            .execute("SELECT * FROM person WHERE name = 'Nikola';")
            .unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].id.id, "nikola");

        let plans = conn.explain_field_filter("person", "name").unwrap();
        assert_uses_index(&plans, &idx_name);
    }

    // Delete one indexed record; the filter no longer returns it.
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("DELETE person:jaime;").unwrap();
        let r = conn
            .execute("SELECT * FROM person WHERE name = 'Jaime';")
            .unwrap();
        assert!(r.records.is_empty(), "deleted record must not match");
        // Other records still match.
        let r = conn
            .execute("SELECT * FROM person WHERE name = 'Tobie';")
            .unwrap();
        assert_eq!(r.records.len(), 1);
    }
}
