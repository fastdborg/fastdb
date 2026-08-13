#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;
use turso_fastdb::{Database, RecordIdValue, StatementResult, Value};

fuzz_target!(|bytes: &[u8]| {
    let database = Database::open_memory().expect("in-memory database opens");
    let connection = database.connect().expect("connection opens");
    let mut model = BTreeMap::<u8, i64>::new();

    for chunk in bytes.chunks(3).take(96) {
        let id = chunk.first().copied().unwrap_or(0) % 16;
        let operation = chunk.get(1).copied().unwrap_or(0) % 4;
        let value = i64::from(chunk.get(2).copied().unwrap_or(0));
        match operation {
            0 if !model.contains_key(&id) => {
                connection
                    .execute(&format!("CREATE item:r{id} SET n={value} RETURN NONE"))
                    .expect("model create succeeds");
                model.insert(id, value);
            }
            1 if model.contains_key(&id) => {
                connection
                    .execute(&format!("UPDATE item:r{id} SET n={value} RETURN NONE"))
                    .expect("model update succeeds");
                model.insert(id, value);
            }
            2 if model.remove(&id).is_some() => {
                connection
                    .execute(&format!("DELETE item:r{id} RETURN BEFORE"))
                    .expect("model delete succeeds");
            }
            _ => {
                let response = connection
                    .execute(&format!("SELECT n FROM item:r{id}"))
                    .expect("model read succeeds");
                let StatementResult::Rows(rows) = &response.statements[0] else {
                    panic!("SELECT returns rows")
                };
                assert_eq!(rows.len(), usize::from(model.contains_key(&id)));
            }
        }
    }

    let response = connection
        .execute("SELECT id, n FROM item ORDER BY id")
        .expect("final model read succeeds");
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("SELECT returns rows")
    };
    let actual = rows
        .iter()
        .map(|row| {
            let Value::Object(row) = row else {
                panic!("row is an object")
            };
            let Value::RecordId(id) = &row["id"] else {
                panic!("id is typed")
            };
            let RecordIdValue::String(id) = &id.id else {
                panic!("generated record ID is a string")
            };
            let id = id.strip_prefix('r').unwrap().parse::<u8>().unwrap();
            let Value::Integer(value) = row["n"] else {
                panic!("n is an integer")
            };
            (id, value)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual, model);
});
