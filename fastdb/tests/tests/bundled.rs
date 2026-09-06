use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn bundled_string_functions_work_in_sql_and_validated_object_writes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE posts");
    q(&c, "DEFINE FIELD slug ON posts TYPE string REQUIRED");
    q(&c, "CREATE UNIQUE INDEX slugs ON posts(slug)");
    q(
        &c,
        "INSERT INTO posts {id:posts:p1,slug:string::slugify(' Crème brûlée! ')}",
    );
    assert_eq!(
        q(&c, "SELECT slug FROM posts").rows,
        vec![vec![Value::String("creme-brulee".into())]]
    );
    q(
        &c,
        "UPDATE posts SET slug=string::slugify('Other Post') RETURNING slug",
    );
    let normalized = q(&c, "SELECT string::normalize('é','NFC') AS normalized");
    assert_eq!(normalized.rows, vec![vec![Value::String("é".into())]]);
    assert!(c
        .execute(
            "INSERT INTO posts {slug:string::slugify('Other Post')}",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(
        c.lookup_index("posts", "slugs", &Value::String("other-post".into()))
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn bundled_arguments_remain_data_and_invalid_arguments_are_rejected() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let text = "'); throw new Error('executed'); //";
    let params = Parameters::from([("$input".into(), Value::String(text.into()))]);
    assert_eq!(
        c.execute("SELECT string::normalize($input,'NFC') AS value", &params)
            .unwrap()
            .rows,
        vec![vec![Value::String(text.into())]]
    );
    for sql in [
        "SELECT string::normalize('x','invalid')",
        "SELECT string::slugify(1)",
        "SELECT string::eval('1+1')",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err());
    }
    q(&c, "CREATE TABLE posts");
    let params = Parameters::from([("$input".into(), Value::String("x".repeat(65_537)))]);
    assert_eq!(
        c.execute("INSERT INTO posts {slug:string::slugify($input)}", &params)
            .unwrap_err()
            .code(),
        "FDB_LIMIT"
    );
    assert!(q(&c, "SELECT * FROM posts").rows.is_empty());
}
