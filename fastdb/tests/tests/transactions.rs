use fastdb::{Database, Parameters, TransactionState as State, Value};
fn q(c: &fastdb::Connection, sql: &str) {
    c.execute(sql, &Parameters::new()).expect(sql);
}
#[test]
fn validation_errors_preserve_outer_work_and_report_active_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE posts");
        q(&c, "DEFINE FIELD n ON posts TYPE integer REQUIRED");
        assert_eq!(c.transaction_state(), State::Autocommit);
        let begin = c.execute_report("BEGIN", &Parameters::new());
        begin.result.unwrap();
        assert_eq!(
            (begin.transaction_before, begin.transaction_after),
            (State::Autocommit, State::Active)
        );
        q(&c, "INSERT INTO posts {n:1}");
        let error = c.execute_report("INSERT INTO posts {n:'bad'}", &Parameters::new());
        assert_eq!(error.result.unwrap_err().code(), "FDB_VALIDATION");
        assert_eq!(
            (error.transaction_before, error.transaction_after),
            (State::Active, State::Active)
        );
        let commit = c.execute_report("COMMIT", &Parameters::new());
        commit.result.unwrap();
        assert_eq!(commit.transaction_after, State::Autocommit);
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    assert_eq!(
        c.execute("SELECT n FROM posts", &Parameters::new())
            .unwrap()
            .rows,
        vec![vec![Value::Integer(1)]]
    );
}
#[test]
fn engine_abort_reports_loss_of_outer_transaction_and_connection_recovers() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE posts");
    q(&c, "INSERT INTO posts {n:1}");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO posts {n:2}");
    let report = c.execute_report(
        "UPDATE posts SET n=n+10 RETURNING array::append(1,2) AS bad",
        &Parameters::new(),
    );
    assert!(report.result.is_err());
    assert_eq!(
        (report.transaction_before, report.transaction_after),
        (State::Active, State::Autocommit)
    );
    assert_eq!(
        c.execute("SELECT n FROM posts", &Parameters::new())
            .unwrap()
            .rows,
        vec![vec![Value::Integer(1)]]
    );
    q(&c, "BEGIN");
    q(&c, "INSERT INTO posts {n:3}");
    q(&c, "COMMIT");
    assert_eq!(c.transaction_state(), State::Autocommit);
}
#[test]
fn savepoints_and_independent_connections_have_independent_states() {
    let db = Database::open(":memory:").unwrap();
    let a = db.connect().unwrap();
    let b = db.connect().unwrap();
    q(&a, "SAVEPOINT outer_work");
    assert_eq!(a.transaction_state(), State::Active);
    assert_eq!(b.transaction_state(), State::Autocommit);
    let error = a.execute_report("SELECT (", &Parameters::new());
    assert!(error.result.is_err());
    assert_eq!(error.transaction_after, State::Active);
    q(&a, "SAVEPOINT inner_work");
    q(&a, "ROLLBACK TO inner_work");
    q(&a, "RELEASE inner_work");
    assert_eq!(a.transaction_state(), State::Active);
    q(&a, "RELEASE outer_work");
    assert_eq!(a.transaction_state(), State::Autocommit);
}

#[test]
fn native_constraint_categories_preserve_reports_and_statement_rollback() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(
        &c,
        "CREATE TABLE items(id INTEGER PRIMARY KEY,name TEXT UNIQUE NOT NULL,n INTEGER CHECK(n>0))",
    );
    q(&c, "INSERT INTO items VALUES (1,'one',1)");
    for sql in [
        "INSERT INTO items VALUES (1,'two',2)",
        "INSERT INTO items VALUES (2,'one',2)",
        "INSERT INTO items VALUES (2,NULL,2)",
        "INSERT INTO items VALUES (2,'two',-1)",
    ] {
        let report = c.execute_report(sql, &Parameters::new());
        assert_eq!(report.result.unwrap_err().code(), "FDB_CONSTRAINT", "{sql}");
        assert_eq!(report.transaction_after, State::Autocommit);
    }
    q(&c, "BEGIN");
    q(&c, "INSERT INTO items VALUES (2,'two',2)");
    let report = c.execute_report(
        "INSERT INTO items VALUES (3,'three',-1)",
        &Parameters::new(),
    );
    assert_eq!(report.result.unwrap_err().code(), "FDB_CONSTRAINT");
    assert_eq!(report.transaction_after, State::Active);
    assert_eq!(
        c.execute("SELECT count(*) FROM items", &Parameters::new())
            .unwrap()
            .rows,
        vec![vec![Value::Integer(2)]]
    );
    q(&c, "ROLLBACK");
    q(&c, "PRAGMA foreign_keys=ON");
    q(
        &c,
        "CREATE TABLE child(parent INTEGER REFERENCES items(id))",
    );
    assert_eq!(
        c.execute("INSERT INTO child VALUES (99)", &Parameters::new())
            .unwrap_err()
            .code(),
        "FDB_CONSTRAINT"
    );
    q(&c,"CREATE TRIGGER reject_item BEFORE INSERT ON items WHEN new.id=9 BEGIN SELECT RAISE(ABORT,'rejected item'); END");
    assert_eq!(
        c.execute("INSERT INTO items VALUES (9,'nine',9)", &Parameters::new())
            .unwrap_err()
            .code(),
        "FDB_CONSTRAINT"
    );
    assert_eq!(
        c.execute("SELECT missing_function()", &Parameters::new())
            .unwrap_err()
            .code(),
        "FDB_ENGINE"
    );
}

#[test]
fn constraint_code_does_not_imply_full_rollback_for_native_fail_policy() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE items(n INTEGER UNIQUE)");
    let report = c.execute_report(
        "INSERT OR FAIL INTO items VALUES (1),(2),(1),(3)",
        &Parameters::new(),
    );
    assert_eq!(report.result.unwrap_err().code(), "FDB_CONSTRAINT");
    assert_eq!(report.transaction_after, State::Autocommit);
    assert_eq!(
        c.execute("SELECT n FROM items ORDER BY n", &Parameters::new())
            .unwrap()
            .rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
}

#[test]
fn excessive_sql_delimiters_fail_before_writes_and_preserve_outer_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE TABLE native(value INTEGER)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs {value:1}");
    let deep = format!("{}2{}", "(".repeat(128), ")".repeat(128));
    for statement in [
        format!("INSERT INTO native VALUES ({deep})"),
        format!("INSERT INTO docs (value) VALUES (2) RETURNING {deep}"),
        format!("UPDATE docs SET value={deep}"),
        format!("SELECT {deep} FROM docs"),
    ] {
        let report = c.execute_report(&statement, &Parameters::new());
        assert_eq!(report.result.unwrap_err().code(), "FDB_SYNTAX");
        assert_eq!(report.transaction_after, State::Active);
        assert_eq!(
            c.execute("SELECT value FROM docs", &Parameters::new())
                .unwrap()
                .rows,
            vec![vec![Value::Integer(1)]]
        );
        assert!(c
            .execute("SELECT * FROM native", &Parameters::new())
            .unwrap()
            .rows
            .is_empty());
    }
    q(&c, "INSERT INTO native VALUES (3)");
    q(&c, "ROLLBACK");
    assert!(c
        .execute("SELECT * FROM docs", &Parameters::new())
        .unwrap()
        .rows
        .is_empty());
    assert!(c
        .execute("SELECT * FROM native", &Parameters::new())
        .unwrap()
        .rows
        .is_empty());
}

#[test]
fn oversized_lexer_inputs_fail_before_mutation_and_preserve_active_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE INDEX docs_value ON docs(value)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs {value:1}");
    for sql in [
        format!(
            "DELETE FROM docs; /*{}*/",
            "x".repeat(fastql_parser::MAX_INPUT_BYTES)
        ),
        format!(
            "DELETE FROM docs RETURNING {}",
            "1,".repeat(fastql_parser::MAX_TOKENS / 2)
        ),
    ] {
        let report = c.execute_report(&sql, &Parameters::new());
        assert_eq!(report.result.unwrap_err().code(), "FDB_SYNTAX");
        assert_eq!(report.transaction_after, State::Active);
        assert_eq!(c.execute_batch(&sql).unwrap_err().code(), "FDB_SYNTAX");
        assert_eq!(c.transaction_state(), State::Active);
        assert_eq!(
            c.lookup_index("docs", "docs_value", &Value::Integer(1))
                .unwrap()
                .len(),
            1
        );
    }
    q(&c, "ROLLBACK");
    assert!(c
        .execute("SELECT * FROM docs", &Parameters::new())
        .unwrap()
        .rows
        .is_empty());
}

#[test]
fn recursive_sql_returns_depth_errors_on_a_small_caller_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let db = Database::open(":memory:").unwrap();
            let c = db.connect().unwrap();
            q(&c, "CREATE TABLE docs");
            q(&c, "BEGIN");
            q(&c, "INSERT INTO docs {v:1}");
            for expr in [
                format!("{}1", "NOT ".repeat(2000)),
                format!(
                    "{}1{}",
                    "CASE WHEN 1 THEN ".repeat(200),
                    " ELSE 0 END".repeat(200)
                ),
            ] {
                for suffix in ["", " FROM docs"] {
                    let report =
                        c.execute_report(&format!("SELECT {expr}{suffix}"), &Parameters::new());
                    let error = report.result.unwrap_err();
                    assert_eq!(error.code(), "FDB_ENGINE");
                    assert!(error.to_string().contains("maximum depth 100"));
                    assert!(c
                        .profile_select(&format!("SELECT {expr}{suffix}"), &Parameters::new())
                        .unwrap_err()
                        .to_string()
                        .contains("maximum depth 100"));
                    assert_eq!(report.transaction_after, State::Active);
                }
            }
            assert_eq!(
                c.execute("SELECT v FROM docs", &Parameters::new())
                    .unwrap()
                    .rows,
                vec![vec![Value::Integer(1)]]
            );
            q(&c, "ROLLBACK");
        })
        .unwrap()
        .join()
        .unwrap();
}
