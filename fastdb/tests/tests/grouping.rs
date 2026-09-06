use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn grouped_aggregates_match_relational_engine() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE sales");
    q(&c, "CREATE TABLE baseline (region TEXT, amount INTEGER)");
    for (id, region, amount) in [(1, "east", 3), (2, "east", 7), (3, "west", 4)] {
        q(
            &c,
            &format!("INSERT INTO sales {{id:sales:{id},region:'{region}',amount:{amount}}}"),
        );
        q(
            &c,
            &format!("INSERT INTO baseline VALUES ('{region}',{amount})"),
        );
    }
    for tail in [
        "GROUP BY region ORDER BY region",
        "WHERE amount > 3 GROUP BY region HAVING sum(amount) >= 7 ORDER BY region",
        "GROUP BY 1 ORDER BY 1 LIMIT 1",
    ] {
        let projection = "region, count(*) AS n, sum(amount) AS total, avg(amount) AS mean";
        assert_eq!(
            q(&c, &format!("SELECT {projection} FROM sales {tail}")).rows,
            q(&c, &format!("SELECT {projection} FROM baseline {tail}")).rows
        );
    }
    assert_eq!(
        q(&c, "SELECT 7 AS key, count(*) AS n FROM sales GROUP BY 1").rows,
        vec![vec![Value::Integer(7), Value::Integer(3)]]
    );
    q(&c, "INSERT INTO sales {id:sales:4,amount:2}");
    q(&c, "INSERT INTO sales {id:sales:5,region:null,amount:1}");
    assert_eq!(
        q(
            &c,
            "SELECT region, count(*) AS n FROM sales GROUP BY region HAVING region IS NULL"
        )
        .rows,
        vec![vec![Value::Null, Value::Integer(2)]]
    );
    assert!(c
        .execute(
            "SELECT region AS place, count(*) AS n FROM sales GROUP BY place",
            &Parameters::new()
        )
        .is_err());
}
#[test]
fn grouped_insert_select_validates_atomically() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE sales");
    q(&c, "INSERT INTO sales {region:'a',amount:2}");
    q(&c, "INSERT INTO sales {region:'b',amount:8}");
    q(&c, "CREATE TABLE totals");
    q(
        &c,
        "DEFINE FIELD total ON totals TYPE integer CHECK (total < 5)",
    );
    assert!(c.execute("INSERT INTO totals (region,total) SELECT region,sum(amount) FROM sales GROUP BY region ORDER BY region",&Parameters::new()).is_err());
    assert!(q(&c, "SELECT * FROM totals").rows.is_empty());
    q(&c,"INSERT INTO totals (region,total) SELECT region,sum(amount) FROM sales GROUP BY region HAVING sum(amount)<5");
    assert_eq!(
        q(&c, "SELECT total FROM totals").rows,
        vec![vec![Value::Integer(2)]]
    );
}
