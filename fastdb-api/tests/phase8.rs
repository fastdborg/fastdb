#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{params, Builder, StatementResult, Value};
use futures::executor::block_on;

#[test]
fn p8_api_001_bound_fts_query_and_ranking_cross_async_worker() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE ANALYZER blankish TOKENIZERS blank; \
                 CREATE article:one SET body = 'Rust web'; \
                 CREATE article:two SET body = 'Rust database'; \
                 DEFINE INDEX body_idx ON article FIELDS body FULLTEXT ANALYZER blankish HIGHLIGHTS",
                params! {},
            )
            .await
            .unwrap();
        let response = connection
            .query(
                "SELECT id, search::score(1) AS score, \
                 search::highlight('<b>', '</b>', 1) AS marked \
                 FROM article WHERE body @1@ $query ORDER BY score DESC",
                params! { "query" => "Rust web" },
            )
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 1);
        let Value::Object(row) = &rows[0] else {
            panic!("expected object")
        };
        assert!(matches!(row.get("score"), Some(Value::Float(value)) if value.is_finite()));
        assert_eq!(
            row.get("marked"),
            Some(&Value::Str("<b>Rust</b> <b>web</b>".into()))
        );
        connection.close().await.unwrap();
    });
}
