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
#[test]
fn sparse_quantized_and_bit_vectors_round_trip_in_native_encodings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let expected;
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE points");
        q(&c, "DEFINE FIELD v ON points TYPE vector<3> REQUIRED");
        for (i, constructor) in ["vector32_sparse", "vector8", "vector1bit"]
            .iter()
            .enumerate()
        {
            q(&c,&format!("INSERT INTO points {{id:type::record('points',{i}),v:{constructor}('[1,0,-1]')}}"));
            let native = q(&c, &format!("SELECT {constructor}('[1,0,-1]')"));
            let stored = q(
                &c,
                &format!("SELECT v FROM points WHERE id=type::record('points',{i})"),
            );
            let (Value::Binary(native), Value::Vector(stored)) =
                (&native.rows[0][0], &stored.rows[0][0])
            else {
                panic!("native and typed vector outputs");
            };
            assert_eq!(native, stored);
        }
        q(&c, "UPDATE points SET converted=vector32(v)");
        expected = q(&c, "SELECT v FROM points ORDER BY id").rows;
        for row in &expected {
            assert_eq!(row[0].vector_dimensions().unwrap(), 3);
        }
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    assert_eq!(q(&c, "SELECT v FROM points ORDER BY id").rows, expected);
}
#[test]
fn vector_slices_concatenation_and_bit_distance_use_typed_operands() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE points");
    q(
        &c,
        "INSERT INTO points {v:vector32_sparse('[1,0,3]'),b:vector1bit('[1,-1,1]')}",
    );
    q(
        &c,
        "UPDATE points SET part=vector_slice(v,0,2),joined=vector_concat(v,v)",
    );
    let rows=q(&c,"SELECT part,joined,vector_distance_jaccard(b,vector1bit('[1,-1,1]')) AS distance FROM points").rows;
    assert_eq!(rows[0][0].vector_dimensions().unwrap(), 2);
    assert_eq!(rows[0][1].vector_dimensions().unwrap(), 6);
    assert_eq!(
        q(
            &c,
            "SELECT vector_extract(vector32(joined)) AS v FROM points"
        )
        .rows[0][0],
        Value::String("[1,0,3,1,0,3]".into())
    );
    assert!(matches!(rows[0][2],Value::Number(n) if n.abs()<1e-6));
    assert!(c
        .execute(
            "UPDATE points SET v=vector_slice(v,0,99)",
            &Parameters::new()
        )
        .is_err());
}
#[test]
fn malformed_vector_metadata_is_rejected_without_native_parsing() {
    for bytes in [
        vec![3],
        vec![0, 255, 3],
        vec![4],
        vec![0, 0, 4],
        vec![9],
        vec![0, 0, 0, 0, 9],
        vec![0, 0, 0, 0, 5],
    ] {
        assert!(Value::Vector(bytes).validate().is_err());
    }
    let mut sparse = 1.0f32.to_le_bytes().to_vec();
    sparse.extend(9u32.to_le_bytes());
    sparse.extend(3u32.to_le_bytes());
    sparse.push(9);
    assert!(Value::Vector(sparse).validate().is_err());
    let mut zero = 3u32.to_le_bytes().to_vec();
    zero.push(9);
    assert_eq!(Value::Vector(zero).vector_dimensions().unwrap(), 3);
    let mut quantized = vec![0; 4];
    quantized.extend(f32::NAN.to_le_bytes());
    quantized.extend(0f32.to_le_bytes());
    quantized.extend([0, 1, 4]);
    assert!(Value::Vector(quantized).validate().is_err());
}

#[test]
fn rust_vector_constructors_match_native_encodings_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("constructors.db");
    let expected;
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE points");
        for values in [
            vec![1.0f32, 0.0, -1.0],
            vec![0.0; 9],
            vec![2.5; 16],
            vec![-2.0, 0.5, 3.0, 0.0, 4.0, 1.0, -1.0, 2.0, 0.0],
        ] {
            let text = format!(
                "[{}]",
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            for (name, value) in [
                ("vector32_sparse", Value::vector32_sparse(&values).unwrap()),
                ("vector8", Value::vector8(&values).unwrap()),
                ("vector1bit", Value::vector1bit(&values).unwrap()),
            ] {
                assert_eq!(value.vector_dimensions().unwrap(), values.len());
                let native = q(&c, &format!("SELECT {name}('{text}')"));
                let Value::Vector(bytes) = &value else {
                    panic!("typed vector")
                };
                assert_eq!(
                    native.rows,
                    vec![vec![Value::Binary(bytes.clone())]],
                    "{name}: {text}"
                );
                c.execute(
                    "INSERT INTO points(v) VALUES ($v)",
                    &Parameters::from([("$v".into(), value)]),
                )
                .unwrap();
            }
        }
        expected = q(&c, "SELECT v FROM points ORDER BY id").rows;
        assert_eq!(
            c.check_collection_integrity("points", Default::default())
                .unwrap()
                .documents,
            12
        );
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    assert_eq!(q(&c, "SELECT v FROM points ORDER BY id").rows, expected);
}

#[test]
fn rust_vector_constructors_enforce_dimensions_and_finite_values() {
    type Constructor = fn(&[f32]) -> fastdb::Result<Value>;
    let constructors: [Constructor; 4] = [
        Value::vector32,
        Value::vector32_sparse,
        Value::vector8,
        Value::vector1bit,
    ];
    for constructor in constructors {
        assert!(constructor(&[]).is_err());
        assert_eq!(
            constructor(&vec![0.0; 65_537]).unwrap_err().code(),
            "FDB_LIMIT"
        );
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(constructor(&[value]).is_err());
        }
        assert_eq!(
            constructor(&vec![0.0; 65_536])
                .unwrap()
                .vector_dimensions()
                .unwrap(),
            65_536
        );
    }
    assert_eq!(
        Value::vector64(&vec![0.0; 65_537]).unwrap_err().code(),
        "FDB_LIMIT"
    );
    assert!(Value::vector64(&[]).is_err());
}
