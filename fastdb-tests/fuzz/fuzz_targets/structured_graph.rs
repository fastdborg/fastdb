#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;
use turso_fastdb::{Database, RecordIdValue, StatementResult, Value};

fuzz_target!(|bytes: &[u8]| {
    let database = Database::open_memory().expect("in-memory database opens");
    let connection = database.connect().expect("connection opens");
    connection
        .execute("DEFINE TABLE links TYPE RELATION FROM person TO person")
        .expect("relation definition succeeds");
    let mut nodes = BTreeSet::<u8>::new();
    let mut edges = Vec::<(u8, u8)>::new();

    for chunk in bytes.chunks(3).take(96) {
        let from = chunk.first().copied().unwrap_or(0) % 16;
        let to = chunk.get(1).copied().unwrap_or(0) % 16;
        match chunk.get(2).copied().unwrap_or(0) % 4 {
            0 if nodes.insert(from) => {
                connection
                    .execute(&format!("CREATE person:r{from} CONTENT {{}} RETURN NONE"))
                    .expect("model node creation succeeds");
            }
            1 => {
                connection
                    .execute(&format!("RELATE person:r{from}->links->person:r{to}"))
                    .expect("model edge creation succeeds");
                edges.push((from, to));
            }
            2 if nodes.remove(&from) => {
                connection
                    .execute(&format!("DELETE person:r{from}"))
                    .expect("model cascade succeeds");
                edges.retain(|(left, right)| *left != from && *right != from);
            }
            _ => {
                let response = connection
                    .execute(&format!(
                        "SELECT ->links->person AS ids FROM person:r{from}"
                    ))
                    .expect("model traversal succeeds");
                let StatementResult::Rows(rows) = &response.statements[0] else {
                    panic!("traversal returns rows")
                };
                if !nodes.contains(&from) {
                    assert!(rows.is_empty());
                    continue;
                }
                let Value::Object(row) = &rows[0] else {
                    panic!("traversal row is an object")
                };
                let Value::Array(ids) = &row["ids"] else {
                    panic!("traversal projection is an array")
                };
                let mut actual = ids
                    .iter()
                    .map(|value| {
                        let Value::RecordId(id) = value else {
                            panic!("traversal endpoint is typed")
                        };
                        let RecordIdValue::String(id) = &id.id else {
                            panic!("model uses string record IDs")
                        };
                        id.strip_prefix('r').unwrap().parse::<u8>().unwrap()
                    })
                    .collect::<Vec<_>>();
                let mut expected = edges
                    .iter()
                    .filter_map(|(left, right)| (*left == from).then_some(*right))
                    .collect::<Vec<_>>();
                actual.sort_unstable();
                expected.sort_unstable();
                assert_eq!(actual, expected);
            }
        }
    }

    let actual_edges = match &connection
        .execute("SELECT * FROM links")
        .expect("final edge read succeeds")
        .statements[0]
    {
        StatementResult::Rows(rows) => rows.len(),
        _ => panic!("edge read returns rows"),
    };
    assert_eq!(actual_edges, edges.len());
});
