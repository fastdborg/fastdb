#![no_main]

use libfuzzer_sys::fuzz_target;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use turso_fastdb::{Database, Params, RecordId, StatementResult, Value};

fuzz_target!(|bytes: &[u8]| {
    let database = Database::open_memory().expect("in-memory database opens");
    let connection = database.connect().expect("connection opens");
    connection
        .execute(
            "CREATE point:r0 SET embedding = [0,0], enabled = true; \
             DEFINE FIELD embedding ON point TYPE array<float, 2>",
        )
        .expect("vector bootstrap succeeds");
    let mut model = BTreeMap::<u8, ([f64; 2], bool)>::from([(0, ([0.0, 0.0], true))]);

    for chunk in bytes.chunks(5).take(64) {
        let id = chunk.first().copied().unwrap_or(0) % 16;
        let x = f64::from(chunk.get(1).copied().unwrap_or(0) as i8) / 8.0;
        let y = f64::from(chunk.get(2).copied().unwrap_or(0) as i8) / 8.0;
        let enabled = chunk.get(3).copied().unwrap_or(0) % 2 == 0;
        match chunk.get(4).copied().unwrap_or(0) % 4 {
            0 => {
                let source =
                    format!("CREATE point:r{id} SET embedding = [{x},{y}], enabled = {enabled}");
                let result = connection.execute(&source);
                if model.contains_key(&id) {
                    result.expect_err("duplicate model record is rejected");
                } else {
                    result.expect("model create succeeds");
                    model.insert(id, ([x, y], enabled));
                }
            }
            1 => {
                connection
                    .execute(&format!(
                        "UPDATE point:r{id} SET embedding = [{x},{y}], enabled = {enabled}"
                    ))
                    .expect("model update succeeds");
                if model.contains_key(&id) {
                    model.insert(id, ([x, y], enabled));
                }
            }
            2 => {
                connection
                    .execute(&format!("DELETE point:r{id}"))
                    .expect("model delete succeeds");
                model.remove(&id);
            }
            _ => {
                let mut expected = model
                    .iter()
                    .filter(|(_, (_, enabled))| *enabled)
                    .map(|(id, (point, _))| {
                        let distance = ((point[0] - x).powi(2) + (point[1] - y).powi(2)).sqrt();
                        (*id, distance)
                    })
                    .collect::<Vec<_>>();
                expected.sort_by(|(left_id, left), (right_id, right)| {
                    left.partial_cmp(right)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| left_id.cmp(right_id))
                });
                expected.truncate(3);
                let params = Params::from([(
                    "query".to_string(),
                    Value::Array(vec![Value::Float(x), Value::Float(y)]),
                )]);
                let response = connection
                    .execute_with_params(
                        "SELECT id FROM point WHERE enabled = true \
                         AND embedding <|3,EUCLIDEAN|> $query",
                        &params,
                    )
                    .expect("bound exact vector query succeeds");
                let StatementResult::Rows(rows) = &response.statements[0] else {
                    panic!("vector query returns rows")
                };
                let actual = rows
                    .iter()
                    .map(|value| match value {
                        Value::Object(row) => match row.get("id") {
                            Some(Value::RecordId(id)) => id.clone(),
                            _ => panic!("projected ID is a record ID"),
                        },
                        _ => panic!("vector projection returns objects"),
                    })
                    .collect::<Vec<_>>();
                let expected = expected
                    .into_iter()
                    .map(|(id, _)| RecordId::new("point", format!("r{id}")))
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected);
            }
        }
    }
});
