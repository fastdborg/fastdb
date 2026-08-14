#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;
use turso_fastdb::{Database, StatementResult, Value};

fn token(byte: u8) -> &'static str {
    const TOKENS: [&str; 8] = [
        "alpha", "beta", "gamma", "delta", "rust", "web", "db", "index",
    ];
    TOKENS[usize::from(byte) % TOKENS.len()]
}

fn matches_blank(text: &str, query: &str) -> bool {
    query
        .split_whitespace()
        .all(|needle| text.split_whitespace().any(|word| word == needle))
}

fuzz_target!(|bytes: &[u8]| {
    let database = Database::open_memory().expect("in-memory database opens");
    let connection = database.connect().expect("connection opens");
    connection
        .execute(
            "DEFINE ANALYZER blankish TOKENIZERS blank; \
             CREATE doc:r0 SET text = 'seed'; \
             DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish HIGHLIGHTS",
        )
        .expect("FTS bootstrap succeeds");
    let mut model = BTreeMap::<u8, String>::from([(0, "seed".to_owned())]);

    for chunk in bytes.chunks(4).take(64) {
        let id = chunk.first().copied().unwrap_or(0) % 16;
        let first = token(chunk.get(1).copied().unwrap_or(0));
        let second = token(chunk.get(2).copied().unwrap_or(0));
        match chunk.get(3).copied().unwrap_or(0) % 4 {
            0 => {
                let value = format!("{first} {second}");
                connection
                    .execute(&format!("UPDATE doc:r{id} SET text = '{value}'"))
                    .expect("model update succeeds");
                if model.contains_key(&id) {
                    model.insert(id, value);
                }
            }
            1 => {
                let value = format!("{first} {second}");
                let result = connection.execute(&format!("CREATE doc:r{id} SET text = '{value}'"));
                if model.contains_key(&id) {
                    result.expect_err("duplicate model record is rejected");
                } else {
                    result.expect("model creation succeeds");
                    model.insert(id, value);
                }
            }
            2 => {
                connection
                    .execute(&format!("DELETE doc:r{id}"))
                    .expect("model delete succeeds");
                model.remove(&id);
            }
            _ => {
                let query = format!("{first} {second}");
                let params = BTreeMap::from([("query".to_owned(), Value::Str(query.clone()))]);
                let response = connection
                    .execute_with_params("SELECT id FROM doc WHERE text @@ $query", &params)
                    .expect("bound FTS query succeeds");
                let StatementResult::Rows(rows) = &response.statements[0] else {
                    panic!("FTS query returns rows")
                };
                let expected = model
                    .values()
                    .filter(|text| matches_blank(text, &query))
                    .count();
                assert_eq!(rows.len(), expected);
            }
        }
    }
});
