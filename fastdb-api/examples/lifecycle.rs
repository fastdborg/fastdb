use fastdb::{params, Builder, ErrorCategory, Value};

async fn lifecycle() -> Result<(), fastdb::Error> {
    let database = Builder::new_memory().build().await?;
    let mut connection = database.connect()?;
    let summary = connection
        .execute(
            "CREATE person:one SET age=$age RETURN NONE",
            params! { "age" => 42 },
        )
        .await?;
    assert_eq!(summary.mutation_count, 1);

    let response = connection
        .query("SELECT age FROM person:one", params! {})
        .await?;
    assert!(matches!(
        response.statements[0],
        fastdb::StatementResult::Rows(_)
    ));

    let mut transaction = connection.transaction().await?;
    transaction
        .execute("UPDATE person:one SET age=43", params! {})
        .await?;
    transaction.commit().await?;

    let error = connection
        .query("SELECT FROM person", params! {})
        .await
        .expect_err("invalid source must fail");
    assert_eq!(error.category(), ErrorCategory::Parse);
    assert!(error.span().is_some());
    let _typed_value = Value::from(43_i32);
    connection.close().await
}

fn main() {
    let _future = lifecycle();
}
