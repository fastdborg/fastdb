use fastdb::{Database, Parameters, ResultLimits, TransactionState, Value};

#[test]
fn collection_write_buffer_rows_reject_before_mutation_and_preserve_prior_work() {
    for sql in [
        "INSERT INTO docs(n) VALUES(4),(5)",
        "INSERT INTO docs(n) SELECT n+10 FROM docs",
        "UPDATE docs SET n=n+10",
        "UPDATE docs {n:n+10}",
        "DELETE FROM docs",
    ] {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        let p = Parameters::new();
        for sql in [
            "CREATE TABLE docs",
            "CREATE UNIQUE INDEX docs_n ON docs(n)",
            "INSERT INTO docs {id:docs:a,n:1}",
            "BEGIN",
            "INSERT INTO docs {id:docs:b,n:2}",
        ] {
            c.execute(sql, &p).unwrap();
        }
        let before = c.execute("SELECT * FROM docs ORDER BY n", &p).unwrap().rows;
        let c = c.with_write_buffer_limits(ResultLimits {
            max_rows: 1,
            max_payload_bytes: 10000,
        });
        assert_eq!(c.execute(sql, &p).unwrap_err().code(), "FDB_LIMIT", "{sql}");
        assert_eq!(c.transaction_state(), TransactionState::Active);
        assert_eq!(
            c.execute("SELECT * FROM docs ORDER BY n", &p).unwrap().rows,
            before
        );
        assert_eq!(
            c.check_collection_integrity("docs", Default::default())
                .unwrap()
                .documents,
            2
        );
        let c = c.with_write_buffer_limits(ResultLimits {
            max_rows: 4,
            max_payload_bytes: 10000,
        });
        c.execute(sql, &p).unwrap();
        c.execute("ROLLBACK", &p).unwrap();
        assert_eq!(
            c.execute("SELECT n FROM docs", &p).unwrap().rows,
            vec![vec![Value::Integer(1)]]
        );
    }
}

#[test]
fn write_snapshot_limit_checks_generated_document_before_retention() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    c.execute("CREATE TABLE docs", &p).unwrap();
    c.execute("BEGIN", &p).unwrap();
    // VALUES costs 5-byte record + 8-byte integer. The stored document adds
    // the keys id (2) and n (1), giving a 16-byte snapshot.
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 1,
        max_payload_bytes: 15,
    });
    for sql in [
        "INSERT INTO docs(id,n) VALUES(docs:z,4)",
        "INSERT INTO docs {id:docs:z,n:4}",
        "UPSERT docs:z {n:4}",
    ] {
        assert_eq!(c.execute(sql, &p).unwrap_err().code(), "FDB_LIMIT", "{sql}");
        assert!(c.execute("SELECT * FROM docs", &p).unwrap().rows.is_empty());
        assert_eq!(c.transaction_state(), TransactionState::Active);
    }
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 1,
        max_payload_bytes: 16,
    });
    c.execute("INSERT INTO docs(id,n) VALUES(docs:z,4)", &p)
        .unwrap();
    c.execute("ROLLBACK", &p).unwrap();
    assert!(c.execute("SELECT * FROM docs", &p).unwrap().rows.is_empty());
}

#[test]
fn bound_assignment_payload_is_counted_and_reads_keep_their_policy() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    c.execute("CREATE TABLE docs", &p).unwrap();
    c.execute("INSERT INTO docs {id:docs:a,n:1}", &p).unwrap();
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 1,
        max_payload_bytes: 64,
    });
    let params = Parameters::from([("$blob".into(), Value::Binary(vec![7; 128]))]);
    for sql in [
        "UPDATE docs SET payload=$blob",
        "UPDATE docs {payload:$blob}",
    ] {
        assert_eq!(c.execute(sql, &params).unwrap_err().code(), "FDB_LIMIT");
        assert_eq!(
            c.execute("SELECT payload FROM docs", &p).unwrap().rows,
            vec![vec![Value::Null]]
        );
        assert_eq!(c.transaction_state(), TransactionState::Autocommit);
    }
    // This setting does not cap ordinary reads or underlying native SQL buffers.
    assert_eq!(
        c.execute("SELECT $blob", &params).unwrap().rows,
        vec![vec![Value::Binary(vec![7; 128])]]
    );
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 1,
        max_payload_bytes: 48,
    });
    // Restored parameters must share the candidate query's metadata budget.
    // The updated document itself is small enough; dropping column-name bytes
    // during parameter restoration would incorrectly accept this candidate.
    let text = Parameters::from([("$text".into(), Value::String("abcdefghijkl".into()))]);
    assert_eq!(
        c.execute("UPDATE docs SET n=$text", &text)
            .unwrap_err()
            .code(),
        "FDB_LIMIT"
    );
    assert_eq!(
        c.execute("SELECT n FROM docs", &p).unwrap().rows,
        vec![vec![Value::Integer(1)]]
    );
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 1,
        max_payload_bytes: 1024,
    });
    c.execute("UPDATE docs SET payload=$blob", &params).unwrap();
    assert_eq!(
        c.execute("SELECT payload FROM docs", &p).unwrap().rows,
        vec![vec![Value::Binary(vec![7; 128])]]
    );
}

#[test]
fn zero_candidate_rows_allow_empty_writes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    c.execute("CREATE TABLE docs", &p).unwrap();
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 0,
        max_payload_bytes: 1000,
    });
    for sql in [
        "UPDATE docs SET n=1 WHERE 0",
        "DELETE FROM docs WHERE 0",
        "UPDATE docs {n:1} WHERE 0",
    ] {
        assert_eq!(c.execute(sql, &p).unwrap().affected, 0);
    }
    assert_eq!(
        c.execute("INSERT INTO docs {n:1}", &p).unwrap_err().code(),
        "FDB_LIMIT"
    );
    assert!(c.execute("SELECT * FROM docs", &p).unwrap().rows.is_empty());
}

#[test]
fn batch_buffer_rejection_stops_before_commit_and_preserves_prior_statement() {
    let db = Database::open(":memory:").unwrap();
    let c = db
        .connect()
        .unwrap()
        .with_write_buffer_limits(ResultLimits {
            max_rows: 1,
            max_payload_bytes: 1000,
        });
    let p = Parameters::new();
    c.execute("CREATE TABLE docs", &p).unwrap();
    c.execute("INSERT INTO docs {id:docs:a,n:1}", &p).unwrap();
    let script = "BEGIN; INSERT INTO docs {id:docs:b,n:2}; UPDATE docs SET n=n+10; COMMIT;";
    let reports = c.execute_batch(script).unwrap();
    assert_eq!(reports.len(), 3);
    let rejected = &reports[2];
    assert_eq!(rejected.offset, script.find("UPDATE").unwrap());
    assert_eq!(
        rejected.execution.result.as_ref().unwrap_err().code(),
        "FDB_LIMIT"
    );
    assert_eq!(
        rejected.execution.transaction_after,
        TransactionState::Active
    );
    assert_eq!(
        c.execute("SELECT n FROM docs ORDER BY n", &p).unwrap().rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    c.execute("ROLLBACK", &p).unwrap();
    assert_eq!(
        c.execute("SELECT n FROM docs", &p).unwrap().rows,
        vec![vec![Value::Integer(1)]]
    );
}

#[test]
fn migration_buffer_rejection_restores_pending_schema_data_and_history() {
    use fastdb::{Error, Migration};
    let db = Database::open(":memory:").unwrap();
    let c = db
        .connect()
        .unwrap()
        .with_write_buffer_limits(ResultLimits {
            max_rows: 1,
            max_payload_bytes: 1000,
        });
    let p = Parameters::new();
    let plan = vec![
        Migration {version:1, name:"initial".into(), sql:"CREATE TABLE docs; CREATE UNIQUE INDEX docs_n ON docs(n); INSERT INTO docs {id:docs:a,n:1};".into()},
        Migration {version:2, name:"pending".into(), sql:"CREATE TABLE audit(n); INSERT INTO audit VALUES(9); INSERT INTO docs {id:docs:b,n:2};".into()},
        Migration {version:3, name:"rewrite".into(), sql:"UPDATE docs SET n=n+10;".into()},
    ];
    c.migrate(&plan[..1]).unwrap();
    let error = c.migrate(&plan).unwrap_err();
    assert_eq!(error.code(), "FDB_MIGRATION");
    let Error::Migration { source, .. } = error else {
        panic!("expected migration context")
    };
    assert_eq!(source.code(), "FDB_LIMIT");
    assert_eq!(c.transaction_state(), TransactionState::Autocommit);
    assert!(c.execute("SELECT * FROM audit", &p).is_err());
    assert_eq!(
        c.execute("SELECT n FROM docs", &p).unwrap().rows,
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(c.migrate(&plan[..1]).unwrap().already_applied, 1);
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        1
    );
    let c = c.with_write_buffer_limits(ResultLimits {
        max_rows: 2,
        max_payload_bytes: 1000,
    });
    assert_eq!(c.migrate(&plan).unwrap().applied, vec![2, 3]);
    assert_eq!(c.migrate(&plan).unwrap().already_applied, 3);
    assert_eq!(
        c.execute("SELECT n FROM docs ORDER BY n", &p).unwrap().rows,
        vec![vec![Value::Integer(11)], vec![Value::Integer(12)]]
    );
    assert_eq!(
        c.execute("SELECT n FROM audit", &p).unwrap().rows,
        vec![vec![Value::Integer(9)]]
    );
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        2
    );
}
