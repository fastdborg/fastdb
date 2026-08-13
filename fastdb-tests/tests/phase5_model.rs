#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::BTreeMap;
use tempfile::tempdir;
use turso_fastdb::{Database, RecordIdValue, StatementResult, Value};

fn actual_model(connection: &turso_fastdb::Connection, context: &str) -> BTreeMap<u8, i64> {
    let response = connection
        .execute("SELECT id, n FROM item ORDER BY id")
        .unwrap_or_else(|error| panic!("{context}: {error:?}"));
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    rows.iter()
        .map(|row| {
            let Value::Object(row) = row else {
                panic!("expected object")
            };
            let Value::RecordId(id) = &row["id"] else {
                panic!("expected typed ID")
            };
            let RecordIdValue::String(id) = &id.id else {
                panic!("expected string ID")
            };
            let numeric_id = id.strip_prefix('r').unwrap().parse::<u8>().unwrap();
            let Value::Integer(value) = row["n"] else {
                panic!("expected integer")
            };
            (numeric_id, value)
        })
        .collect()
}

fn exercise(connection: &turso_fastdb::Connection, seed: u64) -> BTreeMap<u8, i64> {
    let mut state = seed;
    let mut model = BTreeMap::new();
    for step in 0..128 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let id = ((state >> 32) % 24) as u8;
        let value = (state as i64).rem_euclid(10_000);
        match state % 5 {
            0 if !model.contains_key(&id) => {
                connection
                    .execute(&format!("CREATE item:r{id} SET n={value} RETURN NONE"))
                    .unwrap();
                model.insert(id, value);
            }
            1 if model.contains_key(&id) => {
                connection
                    .execute(&format!("UPDATE item:r{id} SET n={value} RETURN NONE"))
                    .unwrap();
                model.insert(id, value);
            }
            2 if model.contains_key(&id) => {
                connection
                    .execute(&format!("DELETE item:r{id} RETURN BEFORE"))
                    .unwrap();
                model.remove(&id);
            }
            3 => {
                connection.execute("BEGIN").unwrap();
                let temporary = 200_u8 + (step % 40) as u8;
                connection
                    .execute(&format!(
                        "CREATE item:r{temporary} SET n={value} RETURN NONE"
                    ))
                    .unwrap();
                connection.execute("CANCEL").unwrap();
            }
            _ => {}
        }
        assert_eq!(
            actual_model(connection, &format!("seed={seed:#x} step={step}")),
            model,
            "seed={seed:#x} step={step}"
        );
    }
    model
}

#[test]
fn p5_model_001_multi_seed_memory_and_disk_match_independent_map() {
    let seeds = [
        0x0000_0000_0000_0001,
        0x4d59_5df4_d0f3_3173,
        0x9e37_79b9_7f4a_7c15,
        0xd1b5_4a32_d192_ed03,
    ];
    for seed in seeds {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        exercise(&connection, seed);

        let directory = tempdir().unwrap();
        let path = directory.path().join(format!("model-{seed:016x}.fastdb"));
        let path = path.to_str().unwrap();
        let expected = {
            let database = Database::open(path).unwrap();
            let connection = database.connect().unwrap();
            let expected = exercise(&connection, seed);
            connection.close().unwrap();
            expected
        };
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            actual_model(&connection, &format!("seed={seed:#x} reopen")),
            expected,
            "seed={seed:#x} reopen"
        );
        assert_eq!(common::integrity_check(connection.native()), "ok");
    }
}

mod common;
