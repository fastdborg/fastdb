use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn dense_vector_validation_and_exact_queries_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE snippets");
        q(
            &c,
            "DEFINE FIELD embedding ON snippets TYPE vector<3> REQUIRED",
        );
        q(
            &c,
            "INSERT INTO snippets {id:snippets:s1,embedding:vector32('[1,0,0]')}",
        );
        q(
            &c,
            "INSERT INTO snippets (id,embedding) VALUES (snippets:s2,vector32('[0,1,0]'))",
        );
        let rows=q(&c,"SELECT id, vector_distance_cos(embedding, vector32('[1,0,0]')) AS distance FROM snippets ORDER BY distance,id LIMIT 2").rows;
        let baseline = q(
            &c,
            "SELECT vector_distance_cos(vector32('[1,0,0]'),vector32('[1,0,0]'))",
        );
        assert_eq!(rows[0][1], baseline.rows[0][0]);
        assert!(matches!(rows[0][1],Value::Number(n) if n.abs()<1e-5));
        assert_eq!(rows[1][1], Value::Number(1.0));
        assert!(matches!(
            &q(&c, "SELECT embedding FROM snippets LIMIT 1").rows[0][0],
            Value::Vector(_)
        ));
        assert!(c
            .execute(
                "UPDATE snippets SET embedding=vector32('[1,2]')",
                &Parameters::new()
            )
            .is_err());
        assert!(c
            .execute(
                "CREATE INDEX vector_index ON snippets (embedding)",
                &Parameters::new()
            )
            .is_err());
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    let rows = q(&c, "SELECT embedding FROM snippets").rows;
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row[0].vector_dimensions().unwrap(), 3);
    }
}
#[test]
fn typed_vector_parameters_validate_every_supported_write() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE points");
    q(&c, "DEFINE FIELD v ON points TYPE vector<2>");
    let params = Parameters::from([("$v".into(), Value::vector64(&[1.0, 2.0]).unwrap())]);
    c.execute("INSERT INTO points {id:points:p1,v:$v}", &params)
        .unwrap();
    assert_eq!(q(&c, "SELECT v FROM points").rows[0][0], params["$v"]);
    q(&c, "UPDATE points SET v=vector32('[3,4]') RETURNING v");
    assert_eq!(
        q(&c, "SELECT vector_extract(v) AS text FROM points").rows[0][0],
        Value::String("[3,4]".into())
    );
    for value in [
        Value::Vector(vec![0, 1]),
        Value::Vector(f32::NAN.to_le_bytes().to_vec()),
        Value::Binary(vec![0; 8]),
        Value::vector32(&[1.0]).unwrap(),
    ] {
        let params = Parameters::from([("$v".into(), value)]);
        for sql in [
            "UPDATE points SET v=$v",
            "UPDATE points:p1 {v:$v}",
            "UPSERT points:p1 {v:$v}",
            "INSERT INTO points (v) VALUES ($v)",
        ] {
            assert!(c.execute(sql, &params).is_err(), "{sql}");
        }
    }
    assert!(Value::vector32(&[]).is_err());
    assert!(Value::vector64(&[f64::INFINITY]).is_err());
    assert!(c
        .execute(
            "DEFINE FIELD other ON points TYPE vector<0>",
            &Parameters::new()
        )
        .is_err());
}
#[test]
fn vector_definition_checks_existing_values_and_reports_dimensions() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE points");
    q(&c, "INSERT INTO points {v:vector32('[1,2,3]')}");
    assert!(c
        .execute(
            "DEFINE FIELD v ON points TYPE vector<2>",
            &Parameters::new()
        )
        .is_err());
    q(&c, "DEFINE FIELD v ON points TYPE vector<3>");
    let info = q(&c, "INFO FOR TABLE points");
    assert!(format!("{info:?}").contains("vector<3>"));
}
