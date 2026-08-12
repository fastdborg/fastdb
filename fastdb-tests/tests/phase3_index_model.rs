#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::{BTreeMap, BTreeSet};
use tempfile::tempdir;
use turso_fastdb::{Database, Params, StatementResult, Value};

fn selected_ids(connection: &turso_fastdb::Connection, source: &str) -> BTreeSet<String> {
    let response = connection.execute(source).unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    rows.iter()
        .map(|row| {
            let Value::Object(row) = row else {
                panic!("expected object")
            };
            let Value::RecordId(id) = &row["id"] else {
                panic!("expected record ID")
            };
            id.id.to_source()
        })
        .collect()
}

fn assert_indexed_plans(connection: &turso_fastdb::Connection) {
    let catalog = connection.catalog_state().unwrap();
    let table = catalog.snapshot().unwrap().tables.get("person").unwrap();
    let composite = &table.indexes["by_name_age"].physical_name;
    let range = &table.indexes["by_score"].physical_name;

    let mut params = Params::new();
    params.insert("name".into(), Value::Str("Tracy".into()));
    params.insert("age".into(), Value::Integer(42));
    let plan = connection
        .explain_query_with_params(
            "SELECT * FROM person WHERE name=$name AND age=$age",
            &params,
        )
        .unwrap();
    assert!(plan.iter().any(|line| line.contains(composite)), "{plan:?}");

    let mut range_params = Params::new();
    range_params.insert("minimum".into(), Value::Integer(40));
    let plan = connection
        .explain_query_with_params(
            "SELECT * FROM person WHERE score >= $minimum",
            &range_params,
        )
        .unwrap();
    assert!(plan.iter().any(|line| line.contains(range)), "{plan:?}");

    let unsafe_plan = connection
        .explain_query_with_params(
            "SELECT * FROM person WHERE score >= $minimum OR name='Nobody'",
            &range_params,
        )
        .unwrap();
    assert!(
        unsafe_plan
            .iter()
            .all(|line| !line.contains(composite) && !line.contains(range)),
        "{unsafe_plan:?}"
    );
}

#[test]
fn p3_idx_001_parameter_equality_composite_and_required_range_survive_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("indexes.fastdb");
    let path = path.to_str().unwrap();
    for pass in 0..2 {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        if pass == 0 {
            conn.execute(
                "DEFINE TABLE person SCHEMAFULL; \
                 DEFINE FIELD name ON person TYPE string; \
                 DEFINE FIELD age ON person TYPE int; \
                 DEFINE FIELD score ON person TYPE int; \
                 DEFINE INDEX by_name_age ON person FIELDS name, age; \
                 DEFINE INDEX by_score ON person FIELDS score; \
                 CREATE person:tracy CONTENT { name:'Tracy', age:42, score:42 }; \
                 CREATE person:jaime CONTENT { name:'Jaime', age:39, score:39 }",
            )
            .unwrap();
        }
        assert_indexed_plans(&conn);
        let mut params = Params::new();
        params.insert("minimum".into(), Value::Integer(40));
        let response = conn
            .execute_with_params(
                "SELECT * FROM person WHERE score >= $minimum OR name='Nobody'",
                &params,
            )
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 1);
    }
}

#[test]
fn p3_model_001_deterministic_crud_sequences_match_independent_map() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let mut model = BTreeMap::<u64, i64>::new();
    let mut state = 0x4d595df4d0f33173u64;
    for _ in 0..160 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let id = (state >> 32) % 12;
        let operation = state % 4;
        match operation {
            0 if !model.contains_key(&id) => {
                let value = (state as i64).rem_euclid(100);
                conn.execute(&format!("CREATE item:r{id} SET n={value}"))
                    .unwrap();
                model.insert(id, value);
            }
            1 if model.contains_key(&id) => {
                conn.execute(&format!("UPDATE item:r{id} SET n=n+1 RETURN NONE"))
                    .unwrap();
                *model.get_mut(&id).unwrap() += 1;
            }
            2 if model.contains_key(&id) => {
                conn.execute(&format!("DELETE item:r{id}")).unwrap();
                model.remove(&id);
            }
            _ => {
                let actual = selected_ids(&conn, &format!("SELECT * FROM item:r{id}"));
                assert_eq!(actual.is_empty(), !model.contains_key(&id));
            }
        }

        let actual = conn.execute("SELECT id, n FROM item ORDER BY id").unwrap();
        let StatementResult::Rows(rows) = &actual.statements[0] else {
            panic!("expected rows")
        };
        let actual = rows
            .iter()
            .map(|row| {
                let Value::Object(row) = row else {
                    panic!("expected object")
                };
                let Value::RecordId(id) = &row["id"] else {
                    panic!("expected ID")
                };
                let turso_fastdb::RecordIdValue::String(id) = &id.id else {
                    panic!("expected string ID")
                };
                let numeric_id = id.strip_prefix('r').unwrap().parse::<u64>().unwrap();
                let Value::Integer(value) = row["n"] else {
                    panic!("expected integer")
                };
                (numeric_id, value)
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, model);
    }
}
