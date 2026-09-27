use fastdb::{Database, DdlWorkLimits, Parameters, ResultLimits, TransactionState, Value};
fn limits() -> ResultLimits {
    ResultLimits {
        max_rows: 1000,
        max_payload_bytes: 1_000_000,
    }
}
fn q(c: &fastdb::Connection, sql: &str) {
    c.execute(sql, &Parameters::new())
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}
fn seed(c: &fastdb::Connection) {
    q(c, "CREATE TABLE source(id INTEGER PRIMARY KEY, n INTEGER)");
    q(c, "INSERT INTO source VALUES(1,11),(2,22),(3,33)");
    q(c, "CREATE INDEX source_n ON source(n)");
    q(c, "CREATE TABLE docs");
    q(c, "INSERT INTO docs {id:docs:a,n:11}");
    q(c, "CREATE INDEX docs_n ON docs(n)");
}
#[test]
fn ordinary_views_count_schema_work_and_never_execute_their_select() {
    let dir = tempfile::tempdir().unwrap();
    for path in [
        ":memory:".to_owned(),
        dir.path().join("views.db").to_str().unwrap().to_owned(),
    ] {
        let c = Database::open(&path).unwrap().connect().unwrap();
        seed(&c);
        for sql in [
            "CREATE VIEW main.v AS SELECT n FROM source",
            "CREATE VIEW IF NOT EXISTS v AS SELECT n FROM source",
            "CREATE VIEW bad_eval AS SELECT abs(-9223372036854775808) FROM source",
        ] {
            let m = c
                .ddl_metered(
                    sql,
                    &Parameters::new(),
                    limits(),
                    DdlWorkLimits {
                        max_rows_read: Some(0),
                        max_row_mutations: Some(0),
                        ..Default::default()
                    },
                )
                .unwrap();
            m.outcome.unwrap_or_else(|e| panic!("{sql}: {e}"));
            assert_eq!(m.work.rows_read, 0, "{sql}: {:?}", m.work);
            assert_eq!(m.work.row_mutations, 0);
            assert!(m.work.vm_steps > 0);
        }
        let selected = c.select_metered(
            "SELECT n FROM v ORDER BY n",
            &Parameters::new(),
            limits(),
            Default::default(),
        );
        assert_eq!(selected.outcome.unwrap().rows.len(), 3);
        assert_eq!(selected.work.rows_read, 3);
        assert!(c
            .execute("SELECT * FROM bad_eval", &Parameters::new())
            .is_err());
        q(&c, "BEGIN");
        q(&c, "INSERT INTO source VALUES(4,44)");
        for sql in ["CREATE VIEW pending AS SELECT * FROM source", "DROP VIEW v"] {
            q(&c, "SAVEPOINT view_probe");
            let baseline = c
                .ddl_metered(sql, &Parameters::new(), limits(), Default::default())
                .unwrap();
            baseline.outcome.unwrap();
            q(&c, "ROLLBACK TO view_probe");
            q(&c, "RELEASE view_probe");
            assert!(baseline.work.vm_steps > 1);
            for work in [
                DdlWorkLimits {
                    max_vm_steps: Some(baseline.work.vm_steps - 1),
                    ..Default::default()
                },
                DdlWorkLimits {
                    max_schema_rows_read: Some(0),
                    ..Default::default()
                },
                DdlWorkLimits {
                    max_vm_steps: Some(0),
                    ..Default::default()
                },
            ] {
                let m = c
                    .ddl_metered(sql, &Parameters::new(), limits(), work)
                    .unwrap();
                assert!(m.outcome.is_err(), "{sql}");
                assert!(m.work.schema_budget_exhausted || m.work.vm_budget_exhausted);
                assert_eq!(m.work.row_mutations, 0);
                assert_eq!(c.transaction_state(), TransactionState::Active);
                assert_eq!(
                    c.execute("SELECT * FROM v", &Parameters::new())
                        .unwrap()
                        .rows
                        .len(),
                    4
                );
                assert!(c
                    .execute("SELECT * FROM pending", &Parameters::new())
                    .is_err());
            }
        }
        q(&c, "ROLLBACK");
        for sql in ["DROP VIEW bad_eval", "DROP VIEW IF EXISTS bad_eval"] {
            let m = c
                .ddl_metered(sql, &Parameters::new(), limits(), Default::default())
                .unwrap();
            m.outcome.unwrap();
            assert_eq!(m.work.rows_read, 0);
            assert_eq!(m.work.row_mutations, 0);
        }
        if path != ":memory:" {
            drop(c);
            let c = Database::open(&path).unwrap().connect().unwrap();
            assert_eq!(
                c.execute("SELECT * FROM v", &Parameters::new())
                    .unwrap()
                    .rows
                    .len(),
                3
            );
        }
    }
}
#[test]
fn explain_does_not_execute_reads_writes_or_schema_effects() {
    let c = Database::open(":memory:").unwrap().connect().unwrap();
    seed(&c);
    for sql in [
        "EXPLAIN SELECT * FROM source",
        "EXPLAIN QUERY PLAN SELECT * FROM source WHERE n=22",
        "EXPLAIN SELECT n FROM docs",
        "EXPLAIN QUERY PLAN SELECT n FROM docs WHERE n=11",
        "EXPLAIN INSERT INTO source VALUES(4,44)",
        "EXPLAIN UPDATE source SET n=0",
        "EXPLAIN DELETE FROM source",
        "EXPLAIN CREATE TABLE absent(n INTEGER)",
        "EXPLAIN DROP TABLE source",
        "EXPLAIN SELECT abs(-9223372036854775808) FROM source",
    ] {
        let m = c
            .explain_metered(
                sql,
                &Parameters::new(),
                limits(),
                DdlWorkLimits {
                    max_rows_read: Some(0),
                    max_row_mutations: Some(0),
                    max_vm_steps: None,
                    ..Default::default()
                },
            )
            .unwrap();
        let result = m.outcome.unwrap_or_else(|e| panic!("{sql}: {e}"));
        assert!(!result.rows.is_empty());
        assert_eq!(m.work.rows_read, 0, "{sql}: {:?}", m.work);
        assert_eq!(m.work.row_mutations, 0);
        assert!(m.work.vm_steps > 0);
        let stopped = c
            .explain_metered(
                sql,
                &Parameters::new(),
                limits(),
                DdlWorkLimits {
                    max_vm_steps: Some(m.work.vm_steps - 1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(stopped.outcome.is_err(), "{sql}");
        assert!(stopped.work.vm_budget_exhausted);
        assert_eq!(stopped.work.vm_steps, m.work.vm_steps - 1);
        assert_eq!(stopped.work.rows_read, 0);
        assert_eq!(stopped.work.row_mutations, 0);
        assert_eq!(
            result.rows,
            c.execute(sql, &Parameters::new()).unwrap().rows
        );
    }
    assert_eq!(
        c.execute("SELECT n FROM source ORDER BY id", &Parameters::new())
            .unwrap()
            .rows,
        vec![
            vec![Value::Integer(11)],
            vec![Value::Integer(22)],
            vec![Value::Integer(33)]
        ]
    );
    assert!(c
        .execute("SELECT * FROM absent", &Parameters::new())
        .is_err());
}
#[test]
fn explain_result_limits_bindings_errors_and_outer_transaction_are_preserved() {
    let c = Database::open(":memory:").unwrap().connect().unwrap();
    seed(&c);
    q(&c, "BEGIN");
    q(&c, "INSERT INTO source VALUES(4,44)");
    let p = Parameters::from([("$n".into(), Value::Integer(22))]);
    for sql in [
        "EXPLAIN SELECT * FROM source WHERE n=$n",
        "EXPLAIN QUERY PLAN SELECT * FROM source WHERE n=$n",
    ] {
        let m = c
            .explain_metered(sql, &p, limits(), Default::default())
            .unwrap();
        assert!(!m.outcome.unwrap().rows.is_empty());
        assert_eq!(m.work.rows_read, 0);
        for limit in [
            ResultLimits {
                max_rows: 0,
                ..limits()
            },
            ResultLimits {
                max_payload_bytes: 1,
                ..limits()
            },
        ] {
            let m = c
                .explain_metered(sql, &p, limit, Default::default())
                .unwrap();
            assert!(m.outcome.is_err());
            assert_eq!(m.work.row_mutations, 0);
            assert_eq!(c.transaction_state(), TransactionState::Active);
        }
    }
    for sql in [
        "EXPLAIN SELECT * FROM missing",
        "EXPLAIN SELECT",
        "EXPLAIN SELECT 1; DELETE FROM source",
        "EXPLAIN SELECT * FROM __fastdb_catalog",
    ] {
        let m = c
            .explain_metered(sql, &Parameters::new(), limits(), Default::default())
            .unwrap();
        assert!(m.outcome.is_err(), "{sql}");
        assert_eq!(m.work.row_mutations, 0);
    }
    assert!(c
        .explain_metered("EXPLAIN SELECT 1", &p, limits(), Default::default())
        .unwrap()
        .outcome
        .is_err());
    assert!(c
        .explain_metered(
            "DELETE FROM source",
            &Parameters::new(),
            limits(),
            Default::default()
        )
        .is_none());
    assert_eq!(
        c.execute("SELECT * FROM source", &Parameters::new())
            .unwrap()
            .rows
            .len(),
        4
    );
    q(&c, "ROLLBACK");
    assert_eq!(
        c.execute("SELECT * FROM source", &Parameters::new())
            .unwrap()
            .rows
            .len(),
        3
    );
}
#[test]
fn view_coverage_preserves_temp_resolution_and_managed_guards() {
    let c = Database::open(":memory:").unwrap().connect().unwrap();
    seed(&c);
    q(&c, "CREATE VIEW v AS SELECT n FROM source");
    q(&c, "CREATE TEMP TABLE v(n INTEGER)");
    q(&c, "INSERT INTO temp.v VALUES(99)");
    for sql in [
        "CREATE TEMP VIEW other AS SELECT 1",
        "CREATE MATERIALIZED VIEW other AS SELECT n FROM source",
        "DROP VIEW temp.v",
        "DROP VIEW v",
    ] {
        assert!(
            c.ddl_metered(sql, &Parameters::new(), limits(), Default::default())
                .is_none(),
            "{sql}"
        );
    }
    c.ddl_metered(
        "DROP VIEW main.v",
        &Parameters::new(),
        limits(),
        Default::default(),
    )
    .unwrap()
    .outcome
    .unwrap();
    assert_eq!(
        c.execute("SELECT n FROM v", &Parameters::new())
            .unwrap()
            .rows,
        vec![vec![Value::Integer(99)]]
    );
    for sql in [
        "CREATE VIEW bad AS SELECT * FROM docs",
        "CREATE VIEW bad AS SELECT * FROM __fastdb_catalog",
    ] {
        assert!(c
            .ddl_metered(sql, &Parameters::new(), limits(), Default::default())
            .unwrap()
            .outcome
            .is_err());
    }
}
#[test]
fn explain_search_distinguishes_nonexecuting_plans_from_materialized_ann() {
    let c = Database::open(":memory:").unwrap().connect().unwrap();
    q(&c, "CREATE TABLE docs");
    for n in 1..=5 {
        q(&c,&format!("INSERT INTO docs {{id:docs:d{n},title:'hello world',location:geo::point(0,0),v:vector32('[1,0]')}}"));
    }
    for (create,query) in [
        ("CREATE SEARCH INDEX docs_text ON docs(title) USING FULLTEXT","SELECT id FROM search::text('docs_text','hello',10)"),
        ("CREATE SEARCH INDEX docs_spatial ON docs(location) USING SPATIAL","SELECT id FROM search::near('docs_spatial',geo::point(0,0),1000000)"),
        ("CREATE SEARCH INDEX docs_vector ON docs(v) USING VECTOR WITH (metric='l2',dimensions=2)","SELECT id FROM search::vector('docs_vector',vector32('[1,0]'),5)"),
    ] {
        q(&c,create);
        for prefix in ["EXPLAIN", "EXPLAIN QUERY PLAN"] {
            let sql=format!("{prefix} {query}");
            if query.contains("search::vector") {
                assert!(c.explain_metered(&sql, &Parameters::new(), limits(), Default::default()).is_none(), "ANN lowering work must not be labeled zero");
                assert!(!c.execute(&sql, &Parameters::new()).unwrap().rows.is_empty(), "ordinary compatibility path is preserved");
                continue;
            }
            let m=c.explain_metered(&sql,&Parameters::new(),limits(),DdlWorkLimits{max_rows_read:Some(0),max_row_mutations:Some(0),..Default::default()}).unwrap();
            m.outcome.unwrap_or_else(|e|panic!("{sql}: {e}; {:?}",m.work));
            assert_eq!(m.work.rows_read,0);assert_eq!(m.work.row_mutations,0);
        }
    }
}
