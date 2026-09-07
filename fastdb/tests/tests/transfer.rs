use fastdb::{Database, Parameters, TransferFormat, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn portable_formats_preserve_values_across_databases() {
    let a = Database::open(":memory:").unwrap();
    let a = a.connect().unwrap();
    q(&a, "CREATE TABLE docs");
    let params = Parameters::from([
        ("$blob".into(), Value::Binary(vec![0, 255])),
        ("$number".into(), Value::Number(-0.0)),
        ("$vector".into(), Value::vector32(&[1.0, 2.0]).unwrap()),
    ]);
    a.execute("INSERT INTO docs {id:type::record('docs',9223372036854775807),large:9223372036854775807,small:-9223372036854775808,text:'docs:p1',yes:true,missing:null,nested:{type:'Integer',value:'not a tag'},bytes:$blob,number:$number,vector:$vector}",&params).unwrap();
    for format in [TransferFormat::Json, TransferFormat::Ndjson] {
        let data = a.export_documents("docs", format).unwrap();
        assert!(data.contains("\"9223372036854775807\""));
        assert!(data.contains("8000000000000000"));
        let b = Database::open(":memory:").unwrap();
        let b = b.connect().unwrap();
        q(&b, "CREATE TABLE docs");
        assert_eq!(b.import_documents("docs", &data, format).unwrap(), 1);
        assert_eq!(
            q(&a, "SELECT * FROM docs").rows,
            q(&b, "SELECT * FROM docs").rows
        );
    }
}
#[test]
fn import_failures_roll_back_documents_and_indexes() {
    let a = Database::open(":memory:").unwrap();
    let a = a.connect().unwrap();
    q(&a, "CREATE TABLE docs");
    q(&a, "INSERT INTO docs {id:docs:a,name:'same'}");
    q(&a, "INSERT INTO docs {id:docs:b,name:'same'}");
    let data = a.export_documents("docs", TransferFormat::Ndjson).unwrap();
    let b = Database::open(":memory:").unwrap();
    let b = b.connect().unwrap();
    q(&b, "CREATE TABLE docs");
    q(&b, "CREATE UNIQUE INDEX names ON docs(name)");
    assert!(b
        .import_documents("docs", &data, TransferFormat::Ndjson)
        .is_err());
    assert!(q(&b, "SELECT * FROM docs").rows.is_empty());
    assert!(b
        .lookup_index("docs", "names", &Value::String("same".into()))
        .unwrap()
        .is_empty());
    let invalid = format!("{data}not json\n");
    assert!(b
        .import_documents("docs", &invalid, TransferFormat::Ndjson)
        .is_err());
    let unknown = data.replacen("\"version\":1", "\"version\":999", 1);
    assert!(b
        .import_documents("docs", &unknown, TransferFormat::Ndjson)
        .is_err());
    assert!(q(&b, "SELECT * FROM docs").rows.is_empty());
}

#[test]
fn ambiguous_fields_and_noncanonical_integers_are_rejected() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    for fields in [
        r#""x":{"type":"Integer","value":9007199254740993}"#,
        r#""x":{"type":"Integer","value":"01"}"#,
        r#""x":{"type":"Number","value":"7ff0000000000000"}"#,
        r#""x":{"type":"Null"},"x":{"type":"Null"}"#,
    ] {
        let input=format!("{{\"format\":\"fastdb.documents\",\"version\":1}}\n{{\"type\":\"Object\",\"value\":{{{fields}}}}}\n");
        assert!(c
            .import_documents("docs", &input, TransferFormat::Ndjson)
            .is_err());
    }
    assert!(q(&c, "SELECT * FROM docs").rows.is_empty());
}

#[test]
fn json_replay_preserves_constraint_errors_and_outer_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {id:docs:a,name:'same'}");
    q(&c, "INSERT INTO docs {id:docs:b,name:'same'}");
    let payload = c.export_documents("docs", TransferFormat::Json).unwrap();
    q(&c, "DELETE FROM docs");
    q(&c, "CREATE UNIQUE INDEX names ON docs(name)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs {id:docs:prior,name:'prior'}");
    assert_eq!(
        c.import_documents("docs", &payload, TransferFormat::Json)
            .unwrap_err()
            .code(),
        "FDB_CONSTRAINT"
    );
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        1
    );
    assert!(c
        .lookup_index("docs", "names", &Value::String("same".into()))
        .unwrap()
        .is_empty());
    let retry = payload.replacen("same", "different", 1);
    assert_eq!(
        c.import_documents("docs", &retry, TransferFormat::Json)
            .unwrap(),
        2
    );
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        3
    );
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT * FROM docs").rows.is_empty());
}
