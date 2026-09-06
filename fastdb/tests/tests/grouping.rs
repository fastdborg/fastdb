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
    assert_eq!(
        q(
            &c,
            "SELECT region AS place, count(*) AS n FROM sales GROUP BY place ORDER BY place"
        )
        .rows,
        q(
            &c,
            "SELECT region AS place, count(*) AS n FROM sales GROUP BY region ORDER BY place"
        )
        .rows
    );
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

#[test]
fn having_aliases_match_native_precedence_and_aggregate_expressions() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE sales");
    q(&c, "CREATE TABLE baseline(region,amount,total)");
    for (region, amount, total) in [("a", 2, 99), ("a", 3, 99), ("b", 8, 0)] {
        q(
            &c,
            &format!("INSERT INTO sales {{region:'{region}',amount:{amount},total:{total}}}"),
        );
        q(
            &c,
            &format!("INSERT INTO baseline VALUES ('{region}',{amount},{total})"),
        );
    }
    for having in [
        "total>6",
        "abs(total)>6 AND n=1",
        "place='a'",
        "total>6 AND s.total=0",
    ] {
        for distinct in ["", "DISTINCT "] {
            let tail = format!("GROUP BY s.region HAVING {having} ORDER BY total");
            let projection =
                format!("{distinct}s.region AS place,sum(s.amount) AS total,count(*) AS n");
            assert_eq!(
                q(&c, &format!("SELECT {projection} FROM sales s {tail}")).rows,
                q(&c, &format!("SELECT {projection} FROM baseline s {tail}")).rows,
                "{having}"
            );
        }
    }
    assert_eq!(
        q(&c, "SELECT count(*) AS n FROM sales HAVING N=3").rows,
        vec![vec![Value::Integer(3)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT s.region AS amount,count(*) AS n FROM sales s GROUP BY s.amount ORDER BY n"
        )
        .rows,
        q(
            &c,
            "SELECT s.region AS amount,count(*) AS n FROM baseline s GROUP BY s.amount ORDER BY n"
        )
        .rows
    );
    q(&c, "CREATE TABLE totals");
    q(&c,"INSERT INTO totals (region,total) SELECT region,sum(amount) AS total FROM sales GROUP BY region HAVING total>6");
    assert_eq!(
        q(&c, "SELECT total FROM totals").rows,
        vec![vec![Value::Integer(8)]]
    );
}

#[test]
fn having_aliases_preserve_typed_helper_inputs_and_reject_fetched_values() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {id:docs:2,active:true}");
    q(&c, "INSERT INTO docs {id:docs:10,active:false}");
    assert_eq!(
        q(
            &c,
            "SELECT active AS flag,count(*) AS n FROM docs GROUP BY active HAVING flag=true"
        )
        .rows,
        vec![vec![Value::Boolean(true), Value::Integer(1)]]
    );
    let result = q(
        &c,
        "SELECT id AS ref,count(*) AS n FROM docs GROUP BY id HAVING record::id(ref)=10",
    );
    assert!(matches!(
        &result.rows[0][0],
        Value::Record(fastdb::Record {
            key: fastdb::Key::Integer(10),
            ..
        })
    ));
    assert!(c.execute("SELECT record::fetch(id) AS target,count(*) AS n FROM docs GROUP BY id HAVING target IS NOT NULL",&Parameters::new()).is_err());
}

#[test]
fn grouping_ordinals_follow_native_wrappers_and_scalar_equality() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE TABLE baseline(v)");
    for value in ["1", "1.0", "true", "2", "null"] {
        q(&c, &format!("INSERT INTO docs {{v:{value}}}"));
        q(&c, &format!("INSERT INTO baseline VALUES ({value})"));
    }
    for group in [
        "2",
        "(2)",
        "+2",
        "((+2))",
        "2 COLLATE BINARY",
        "(2 COLLATE BINARY)",
    ] {
        let counts = |table: &str| {
            q(
                &c,
                &format!("SELECT count(*) AS n,v FROM {table} GROUP BY {group} ORDER BY n"),
            )
            .rows
            .into_iter()
            .map(|row| row[0].clone())
            .collect::<Vec<_>>()
        };
        assert_eq!(counts("docs"), counts("baseline"), "{group}");
        assert_eq!(
            counts("docs"),
            vec![Value::Integer(1), Value::Integer(1), Value::Integer(3)],
            "{group}"
        );
    }
    // The pinned engine only recognizes a single sign directly on a numeric
    // literal. More complex expressions remain constant grouping expressions.
    for group in ["+(2)", "-(-2)", "2.0", "2+0"] {
        assert_eq!(
            q(
                &c,
                &format!("SELECT count(*) AS n,v FROM docs GROUP BY {group}")
            )
            .rows[0][0],
            Value::Integer(5),
            "{group}"
        );
    }
    for group in ["0", "(0)", "+0", "-1", "(3)", "+3", "3 COLLATE BINARY"] {
        assert!(
            c.execute(
                &format!("SELECT v,count(*) FROM docs GROUP BY {group}"),
                &Parameters::new()
            )
            .is_err(),
            "{group}"
        );
    }
    assert_eq!(
        q(&c, "SELECT 7 AS key,count(*) AS n FROM docs GROUP BY (+1)").rows,
        vec![vec![Value::Integer(7), Value::Integer(5)]]
    );
}

#[test]
fn collated_grouping_ordinals_preserve_text_and_insert_select() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE TABLE baseline(v)");
    for value in ["a", "A", "b"] {
        q(&c, &format!("INSERT INTO docs {{v:'{value}'}}"));
        q(&c, &format!("INSERT INTO baseline VALUES ('{value}')"));
    }
    for group in ["1 COLLATE NOCASE", "((+1) COLLATE NOCASE)"] {
        let tail = format!("GROUP BY {group} ORDER BY n,v");
        assert_eq!(
            q(&c, &format!("SELECT v,count(*) AS n FROM docs {tail}")).rows,
            q(&c, &format!("SELECT v,count(*) AS n FROM baseline {tail}")).rows
        );
    }
    for group in ["1", "(1)", "+1", "1 COLLATE BINARY"] {
        let query = |table: &str| {
            format!("SELECT v COLLATE NOCASE AS value,count(*) AS n FROM {table} GROUP BY {group} ORDER BY n,value")
        };
        assert_eq!(
            q(&c, &query("docs")).rows,
            q(&c, &query("baseline")).rows,
            "projected collation: {group}"
        );
    }
    q(&c, "CREATE TABLE totals");
    q(
        &c,
        "INSERT INTO totals (v,n) SELECT v,count(*) FROM docs GROUP BY (1 COLLATE NOCASE)",
    );
    assert_eq!(
        q(&c, "SELECT n FROM totals ORDER BY n").rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    // Ordinal substitution must not subsequently reinterpret a source name as
    // another projection's alias.
    assert_eq!(
        q(
            &c,
            "SELECT v AS key,count(*) AS v FROM docs GROUP BY (1) ORDER BY key"
        )
        .rows,
        q(
            &c,
            "SELECT v AS key,count(*) AS v FROM baseline GROUP BY (1) ORDER BY key"
        )
        .rows
    );
}

#[test]
fn group_aliases_resolve_expressions_constants_and_explicit_field_collisions() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "CREATE TABLE baseline(city TEXT,amount INTEGER,bucket TEXT)",
    );
    for (city, amount) in [("A", 1), ("a", 2), ("B", 3)] {
        q(
            &c,
            &format!("INSERT INTO docs {{city:'{city}',amount:{amount},bucket:'same'}}"),
        );
        q(
            &c,
            &format!("INSERT INTO baseline VALUES ('{city}',{amount},'same')"),
        );
    }
    for tail in [
        "GROUP BY place ORDER BY place",
        "GROUP BY (place COLLATE NOCASE) ORDER BY place",
        "GROUP BY lower(place) ORDER BY place",
    ] {
        let sql = |table| format!("SELECT city AS place,count(*) AS total FROM {table} {tail}");
        assert_eq!(
            q(&c, &sql("docs")).rows,
            q(&c, &sql("baseline")).rows,
            "{tail}"
        );
    }
    assert_eq!(
        q(
            &c,
            "SELECT 7 AS key,count(*) AS total FROM docs GROUP BY key"
        )
        .rows,
        vec![vec![Value::Integer(7), Value::Integer(3)]]
    );
    assert_eq!(q(&c, "SELECT lower(city) AS bucket,count(*) AS total FROM docs GROUP BY bucket ORDER BY bucket").rows,
               q(&c, "SELECT lower(city) AS bucket,count(*) AS total FROM baseline GROUP BY lower(city) ORDER BY bucket").rows);
    assert_eq!(
        q(
            &c,
            "SELECT lower(city) AS bucket,count(*) AS total FROM docs d GROUP BY d.bucket"
        )
        .rows
        .len(),
        1
    );
    assert!(c
        .execute(
            "SELECT sum(amount) AS total FROM docs GROUP BY total",
            &Parameters::new()
        )
        .is_err());
    q(&c, "CREATE TABLE typed");
    q(
        &c,
        "INSERT INTO typed (value) VALUES (X'31'),(X'31'),(docs:1),(docs:1)",
    );
    assert_eq!(
        q(
            &c,
            "SELECT value AS key,count(*) AS total FROM typed GROUP BY key ORDER BY key"
        )
        .rows,
        q(
            &c,
            "SELECT value AS key,count(*) AS total FROM typed GROUP BY value ORDER BY key"
        )
        .rows
    );
}
