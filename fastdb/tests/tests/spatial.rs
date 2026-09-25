use fastdb::{Database, Parameters, Value};

fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}

#[test]
fn spatial_distance_boundaries_and_types() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for (a, b, expected) in [
        ("0,0", "0,0", 0.0),
        ("180,0", "-180,0", 0.0),
        ("12,90", "-76,90", 0.0),
        ("12,-90", "-76,-90", 0.0),
        ("0,0", "1,0", 111_195.080_233_532_9),
        ("179,0", "-179,0", 222_390.160_467_065_8),
        ("0,0", "180,0", 20_015_114.442_035_925),
        ("12,90", "76,-90", 20_015_114.442_035_925),
    ] {
        for (a, b) in [(a, b), (b, a)] {
            let result = q(
                &c,
                &format!("SELECT geo::distance(geo::point({a}),geo::point({b})) AS d"),
            );
            let Value::Number(actual) = result.rows[0][0] else {
                panic!("distance type")
            };
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
    }
    assert_eq!(
        q(
            &c,
            "SELECT geo::within(geo::point(180,0),geo::point(-180,0),0)"
        )
        .rows,
        vec![vec![Value::Boolean(true)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT geo::within(geo::point(0,0),geo::point(1,0),100000)"
        )
        .rows,
        vec![vec![Value::Boolean(false)]]
    );
    assert_eq!(q(&c, "SELECT geo::within(geo::point(0,0),geo::point(1,0),geo::distance(geo::point(0,0),geo::point(1,0)))").rows,
        vec![vec![Value::Boolean(true)]]);
    let result = q(&c, "SELECT GEO::POINT(10,20)");
    assert!(result.columns[0].contains("geo::point"));
    assert!(matches!(result.rows[0][0], Value::Object(_)));
}

#[test]
fn spatial_invalid_inputs_and_failed_writes_are_atomic() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for expr in [
        "geo::point(181,0)",
        "geo::point(0,-91)",
        "geo::point('1',2)",
        "geo::point(NULL,2)",
        "geo::point(1)",
        "geo::point(1,2,3)",
        "geo::distance(1,2)",
        "geo::within(geo::point(0,0),geo::point(0,0),-1)",
        "geo::within(geo::point(0,0),geo::point(0,0),NULL)",
    ] {
        assert!(
            c.execute(&format!("SELECT {expr}"), &Parameters::new())
                .is_err(),
            "{expr}"
        );
        assert!(
            c.execute(
                &format!("INSERT INTO invalid {{location:{expr}}}"),
                &Parameters::new()
            )
            .is_err(),
            "{expr}"
        );
    }
    q(&c, "CREATE TABLE places");
    q(&c, "CREATE UNIQUE INDEX names ON places(name)");
    q(
        &c,
        "INSERT INTO places {id:places:a,name:'a',lon:1,location:geo::point(1,2)}",
    );
    q(
        &c,
        "INSERT INTO places {id:places:b,name:'b',lon:181,location:geo::point(2,2)}",
    );
    let before = q(&c, "SELECT * FROM places ORDER BY id").rows;
    q(&c, "BEGIN");
    assert!(c
        .execute(
            "UPDATE places SET location=geo::point(lon,2),name='changed'",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(q(&c, "SELECT * FROM places ORDER BY id").rows, before);
    assert_eq!(
        c.lookup_index("places", "names", &Value::String("a".into()))
            .unwrap()
            .len(),
        1
    );
    q(&c, "ROLLBACK");
}

#[test]
fn spatial_points_persist_and_query_through_bound_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spatial.db");
    let expected;
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(
            &c,
            "INSERT INTO places {id:places:a,location:geo::point(106.7,10.8)}",
        );
        expected = q(&c, "SELECT location FROM places").rows;
        q(&c, "BEGIN");
        q(
            &c,
            "UPDATE places SET location=geo::point(0,0) RETURNING location",
        );
        q(&c, "ROLLBACK");
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    assert_eq!(q(&c, "SELECT location FROM places").rows, expected);
    let params = Parameters::from([("$point".into(), expected[0][0].clone())]);
    assert_eq!(c.execute("SELECT geo::distance(location,$point) FROM places WHERE geo::within(location,$point,0)", &params).unwrap().rows,
        vec![vec![Value::Number(0.0)]]);
    for value in [
        Value::Null,
        Value::Number(f64::INFINITY),
        Value::Object(Default::default()),
        Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
    ] {
        let params = Parameters::from([("$point".into(), value)]);
        assert!(c
            .execute("SELECT geo::distance(location,$point) FROM places", &params)
            .is_err());
    }
}

fn near(c: &fastdb::Connection, center: &Value, radius: f64) -> Vec<Vec<Value>> {
    c.execute("SELECT id,distance_m FROM search::near('places_location',$point,$radius) ORDER BY distance_m,id",
        &Parameters::from([("$point".into(),center.clone()),("$radius".into(),Value::Number(radius))])).unwrap().rows
}

#[test]
fn spatial_index_lifecycle_reopen_and_statement_atomicity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("indexed.db");
    let center;
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(
            &c,
            "INSERT INTO places {id:places:a,location:geo::point(180,0),name:'a'}",
        );
        q(
            &c,
            "INSERT INTO places {id:places:b,location:null,name:'b'}",
        );
        q(&c, "INSERT INTO places {id:places:c,name:'c'}");
        q(&c, "DEFINE FIELD location ON places TYPE object NULLABLE");
        q(
            &c,
            "CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL",
        );
        q(
            &c,
            "CREATE SEARCH INDEX IF NOT EXISTS places_location ON places(location) USING SPATIAL",
        );
        q(&c, "CREATE UNIQUE INDEX names ON places(name)");
        center = q(&c, "SELECT geo::point(-180,0)").rows[0][0].clone();
        assert_eq!(near(&c, &center, 0.0).len(), 1);
        q(&c, "BEGIN");
        q(
            &c,
            "UPDATE places SET location=geo::point(-180,0) WHERE id=places:b",
        );
        assert_eq!(near(&c, &center, 0.0).len(), 2);
        assert!(c
            .execute(
                "UPDATE places SET location=geo::point(1,2),name='duplicate'",
                &Parameters::new()
            )
            .is_err());
        assert_eq!(near(&c, &center, 0.0).len(), 2);
        q(&c, "DELETE FROM places:a");
        assert_eq!(near(&c, &center, 0.0).len(), 1);
        q(&c, "ROLLBACK");
        assert_eq!(near(&c, &center, 0.0).len(), 1);
        for sql in [
            "INSERT INTO places {location:{type:'Point',coordinates:[181,0]}}",
            "UPDATE places SET location=1",
            "DEFINE FIELD location ON places TYPE string OVERWRITE",
            "CREATE SEARCH INDEX IF NOT EXISTS names ON places(location) USING SPATIAL",
        ] {
            assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
        }
        let audit = c
            .check_collection_integrity("places", Default::default())
            .unwrap();
        assert_eq!(
            (audit.documents, audit.indexes, audit.index_entries),
            (3, 2, 6)
        );
        q(&c, "BEGIN");
        q(&c, "DROP INDEX places_location");
        assert!(c
            .execute(
                "SELECT * FROM search::near('places_location',geo::point(0,0),100)",
                &Parameters::new()
            )
            .is_err());
        q(&c, "ROLLBACK");
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    assert_eq!(near(&c, &center, 0.0).len(), 1);
    c.check_collection_integrity("places", Default::default())
        .unwrap();
    q(&c, "DROP INDEX places_location");
    q(
        &c,
        "CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL",
    );
    assert_eq!(near(&c, &center, 0.0).len(), 1);
    q(&c, "DROP TABLE places");
    assert!(c
        .execute(
            "SELECT * FROM search::near('places_location',geo::point(0,0),100)",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn spatial_search_matches_scan_and_uses_index() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE places");
    q(&c, "BEGIN");
    // Deterministic coverage across longitude, latitude, poles and antimeridian.
    for i in 0..360 {
        let longitude = (i as f64) - 180.0;
        let latitude = ((i * 37) % 181) as f64 - 90.0;
        q(&c,&format!("INSERT INTO places {{id:type::record('places',{i}),location:geo::point({longitude},{latitude})}}"));
    }
    for (i, point) in [
        "180,0",
        "-180,0",
        "180,90",
        "-75,90",
        "42,-90",
        "-90,-90",
        "0,0",
        "0.00000000000001,0",
        "0,89.999999",
        "0,-89.999999",
    ]
    .iter()
    .enumerate()
    {
        q(
            &c,
            &format!(
                "INSERT INTO places {{id:type::record('places',{}),location:geo::point({point})}}",
                360 + i
            ),
        );
    }
    q(
        &c,
        "CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL",
    );
    q(&c, "COMMIT");
    for point in [
        "0,0",
        "180,0",
        "-180,0",
        "45,90",
        "70,-90",
        "17,10",
        "179.99999,89.99999",
    ] {
        let center = q(&c, &format!("SELECT geo::point({point})")).rows[0][0].clone();
        for radius in [
            0.0,
            1.0,
            100_000.0,
            2_000_000.0,
            20_015_114.0,
            21_000_000.0,
            f64::MAX,
        ] {
            let params = Parameters::from([
                ("$point".into(), center.clone()),
                ("$radius".into(), Value::Number(radius)),
            ]);
            let scan=c.execute("SELECT id,geo::distance(location,$point) AS distance_m FROM places NOT INDEXED WHERE geo::within(location,$point,$radius) ORDER BY distance_m,id",&params).unwrap();
            assert_eq!(
                near(&c, &center, radius),
                scan.rows,
                "center {point}, radius {radius}"
            );
        }
        // Exact floating-point boundaries must use identical refinement semantics.
        for row in q(
            &c,
            &format!("SELECT geo::distance(location,geo::point({point})) FROM places LIMIT 6"),
        )
        .rows
        {
            let Value::Number(radius) = row[0] else {
                panic!()
            };
            let params = Parameters::from([
                ("$point".into(), center.clone()),
                ("$radius".into(), Value::Number(radius)),
            ]);
            let scan=c.execute("SELECT id,geo::distance(location,$point) AS distance_m FROM places NOT INDEXED WHERE geo::within(location,$point,$radius) ORDER BY distance_m,id",&params).unwrap();
            assert_eq!(
                near(&c, &center, radius),
                scan.rows,
                "boundary center {point}"
            );
        }
    }
    let sql =
        "SELECT * FROM search::near('places_location',geo::point(0,0),100) ORDER BY distance_m,id";
    let plan = q(&c, &format!("EXPLAIN QUERY PLAN {sql}"));
    assert!(format!("{:?}", plan.rows).contains("places_location"));
    let profile = c.profile_select(sql, &Parameters::new()).unwrap();
    assert!(profile.metrics.rows_read < 40, "{:?}", profile.metrics);
    assert_eq!(profile.metrics.fullscan_steps, 0, "{:?}", profile.metrics);
    println!("spatial range profile: {:?}", profile.metrics);
    // Filtering, explicit ties/pagination and joins use the ordinary SELECT pipeline.
    assert_eq!(q(&c,"SELECT p.id,n.distance_m FROM search::near('places_location',geo::point(0,0),100) n JOIN places p ON n.id=p.id ORDER BY n.distance_m,p.id LIMIT 1").rows.len(),1);
    assert_eq!(q(&c,"WITH hits AS (SELECT * FROM search::near('places_location',geo::point(0,0),100)) SELECT * FROM hits ORDER BY distance_m,id").rows,profile.result.rows);
}

#[test]
fn failed_spatial_build_and_invalid_search_leave_prior_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(
        &c,
        "INSERT INTO places {id:places:a,location:geo::point(0,0)}",
    );
    q(&c, "INSERT INTO places {id:places:b,location:'invalid'}");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO notes {id:notes:n,n:1}");
    assert!(c
        .execute(
            "CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL",
            &Parameters::new()
        )
        .is_err());
    q(&c, "UPDATE places:b {location:geo::point(1,1)}");
    q(
        &c,
        "CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL",
    );
    for sql in [
        "SELECT * FROM search::near('missing',geo::point(0,0),100)",
        "SELECT * FROM search::near('places_location',geo::point(0,0),-1)",
        "SELECT * FROM search::near('places_location',geo::point(0,0),random())",
        "SELECT * FROM search::near('places_location',geo::point(0,0))",
        "SELECT * FROM search::near('places_location',NULL,100)",
        "SELECT * FROM search::near('places_location',$missing,100)",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(q(&c, "SELECT * FROM notes").rows.len(), 1);
    c.check_collection_integrity("places", Default::default())
        .unwrap();
    q(&c, "ROLLBACK");
    assert!(c
        .execute(
            "SELECT * FROM search::near('places_location',geo::point(0,0),100)",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn spatial_search_retains_exact_boundary_near_opposite_poles() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(
        &c,
        "INSERT INTO places {id:places:a,location:geo::point(0,-89.9999964108325)}",
    );
    q(
        &c,
        "CREATE SEARCH INDEX places_location ON places(location) USING SPATIAL",
    );
    let center = q(&c, "SELECT geo::point(0,89.99999479428419)").rows[0][0].clone();
    let radius = c
        .execute(
            "SELECT geo::distance(location,$point) FROM places",
            &Parameters::from([("$point".into(), center.clone())]),
        )
        .unwrap()
        .rows[0][0]
        .clone();
    let Value::Number(radius) = radius else {
        panic!()
    };
    assert_eq!(near(&c, &center, radius).len(), 1);
}

#[test]
fn h3_cells_match_reference_and_canonical_geographic_positions() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    // Reference example from H3's official latLngToCell documentation.
    assert_eq!(
        q(&c, "SELECT geo::cell(geo::point(40,45),2)").rows,
        vec![vec![Value::String("822d57fffffffff".into())]]
    );
    for resolution in 0..=15 {
        for (a, b) in [
            ("180,0", "-180,0"),
            ("180,90", "-73,90"),
            ("-180,-90", "97,-90"),
        ] {
            assert_eq!(
                q(
                    &c,
                    &format!("SELECT geo::cell(geo::point({a}),{resolution})")
                )
                .rows,
                q(
                    &c,
                    &format!("SELECT geo::cell(geo::point({b}),{resolution})")
                )
                .rows
            );
        }
        // Center-to-cell identity covers different resolutions, dateline and poles.
        for point in [
            "40,45",
            "0,0",
            "180,0",
            "17,90",
            "-120,-90",
            "-122.41795063018799,37.775938728915946",
        ] {
            let value = q(
                &c,
                &format!("SELECT geo::cell(geo::point({point}),{resolution})"),
            )
            .rows[0][0]
                .clone();
            let params = Parameters::from([("$cell".into(), value.clone())]);
            assert_eq!(
                c.execute(
                    &format!("SELECT geo::cell(geo::cell_center($cell),{resolution})"),
                    &params
                )
                .unwrap()
                .rows,
                vec![vec![value]]
            );
        }
    }
    for sql in [
        "SELECT geo::cell(geo::point(0,0),-1)",
        "SELECT geo::cell(geo::point(0,0),16)",
        "SELECT geo::cell(geo::point(0,0),2.5)",
        "SELECT geo::cell(geo::point(0,0),'2')",
        "SELECT geo::cell(geo::point(0,0),NULL)",
        "SELECT geo::cell(NULL,2)",
        "SELECT geo::cell_center('0')",
        "SELECT geo::cell_center('822D57FFFFFFFFF')",
        "SELECT geo::cell_center('0822d57fffffffff')",
        "SELECT geo::cell_center('122d57fffffffff')",
        "SELECT geo::cell_center(NULL)",
        "SELECT geo::cell_center(2)",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
}

#[test]
fn h3_cell_groups_persist_and_preserve_failed_write_atomicity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cells.db");
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(&c,"INSERT INTO places {id:places:a,location:geo::point(40,45),cell:geo::cell(geo::point(40,45),2)}");
        q(&c,"INSERT INTO places {id:places:b,location:geo::point(40,45),cell:geo::cell(geo::point(40,45),2)}");
        q(&c, "CREATE INDEX place_cells ON places(cell)");
        q(&c, "BEGIN");
        assert!(c
            .execute(
                "UPDATE places SET cell=geo::cell(location,20)",
                &Parameters::new()
            )
            .is_err());
        q(&c, "UPDATE places SET cell=geo::cell(location,3)");
        q(&c, "ROLLBACK");
        c.check_collection_integrity("places", Default::default())
            .unwrap();
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    let expected = vec![vec![
        Value::String("822d57fffffffff".into()),
        Value::Integer(2),
    ]];
    assert_eq!(q(&c,"SELECT geo::cell(location,2) AS cell,count(*) AS n FROM places GROUP BY geo::cell(location,2) HAVING count(*)>1 ORDER BY cell").rows,expected);
    assert_eq!(
        q(
            &c,
            "SELECT cell,count(*) FROM places GROUP BY cell ORDER BY cell"
        )
        .rows,
        expected
    );
    assert_eq!(
        c.lookup_index(
            "places",
            "place_cells",
            &Value::String("822d57fffffffff".into())
        )
        .unwrap()
        .len(),
        2
    );
    q(&c, "CREATE TABLE cells");
    q(&c,"INSERT INTO cells(cell,n) SELECT geo::cell(location,2),count(*) FROM places GROUP BY geo::cell(location,2)");
    assert_eq!(q(&c, "SELECT cell,n FROM cells").rows, expected);
}
