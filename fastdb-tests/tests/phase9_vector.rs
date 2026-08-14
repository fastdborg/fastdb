#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::collections::BTreeMap;
use std::process::Command;
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint, Params, RecordId, StatementResult, Value};

const FORMAT_TWO_VECTOR: &[u8] = include_bytes!("../fixtures/phase9-format2-vector.fastdb");

fn rows(result: &StatementResult) -> &[Value] {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    rows
}

#[test]
fn p9_vector_002_bound_exact_search_update_and_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vector.fastdb");
    let path = path.to_str().unwrap();
    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE item:a SET embedding = [1,0], cohort = 'kept'; \
                 CREATE item:b SET embedding = [0,1], cohort = 'kept'; \
                 CREATE item:c SET embedding = [0.99,0.01], cohort = 'dropped'; \
                 DEFINE FIELD embedding ON item TYPE array<float, 2>",
            )
            .unwrap();
        let params = Params::from([(
            "query".to_string(),
            Value::Array(vec![Value::Float(1.0), Value::Float(0.0)]),
        )]);
        let selected = connection
            .execute_with_params(
                "SELECT id, vector::distance::knn() AS distance FROM item \
                 WHERE cohort = 'kept' AND embedding <|2,EUCLIDEAN|> $query",
                &params,
            )
            .unwrap();
        assert_eq!(rows(&selected.statements[0]).len(), 2);
        let Value::Object(first) = &rows(&selected.statements[0])[0] else {
            panic!("expected projected row")
        };
        assert_eq!(
            first.get("id"),
            Some(&Value::RecordId(RecordId::new("item", "a")))
        );
        connection
            .execute("UPDATE item:b SET embedding = [0.8,0.2]")
            .unwrap();
        let explained = connection
            .execute(
                "EXPLAIN SELECT id FROM item \
                 WHERE cohort = 'kept' AND embedding <|2,EUCLIDEAN|> [1,0]",
            )
            .unwrap();
        assert!(rows(&explained.statements[0]).iter().any(|value| {
            matches!(value, Value::Object(row)
                if matches!(row.get("detail"), Some(Value::Str(detail))
                    if detail == "VECTOR EXACT SCAN EUCLIDEAN K=2"))
        }));
    }
    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        let selected = connection
            .execute("SELECT id FROM item WHERE embedding <|1,EUCLIDEAN|> [0.75,0.25]")
            .unwrap();
        let Value::Object(row) = &rows(&selected.statements[0])[0] else {
            panic!("expected projected row")
        };
        assert_eq!(
            row.get("id"),
            Some(&Value::RecordId(RecordId::new("item", "b")))
        );
    }
}

#[test]
fn p9_vector_003_invalid_values_and_queries_fail_without_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE item:a SET embedding = [1,0]; \
             DEFINE FIELD embedding ON item TYPE array<float, 2>",
        )
        .unwrap();
    for source in [
        "UPDATE item:a SET embedding = [1]",
        "UPDATE item:a SET embedding = [1, 'bad']",
        "SELECT * FROM item WHERE embedding <|1,COSINE|> [0,0]",
        "SELECT * FROM item WHERE embedding <|1,COSINE|> [1]",
        "SELECT * FROM item WHERE embedding <|1,COSINE|> [1,0] OR true",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT embedding FROM item:a").unwrap();
    let Value::Object(row) = &rows(&selected.statements[0])[0] else {
        panic!("expected row")
    };
    assert_eq!(
        row.get("embedding"),
        Some(&Value::Array(vec![Value::Float(1.0), Value::Float(0.0)]))
    );

    let mut params = Params::new();
    params.insert(
        "q".to_string(),
        Value::Array(vec![Value::Float(f64::NAN), Value::Float(0.0)]),
    );
    let error = connection
        .execute_with_params(
            "SELECT * FROM item WHERE embedding <|1,EUCLIDEAN|> $q",
            &params,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Schema);
}

#[test]
fn p9_vector_004_public_vectors_remain_arrays() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE edge:one SET embedding = [-1,2]; \
             DEFINE FIELD embedding ON edge TYPE array<float, 2>",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM edge:one").unwrap();
    let Value::Object(row) = &rows(&selected.statements[0])[0] else {
        panic!("expected row")
    };
    assert_eq!(
        row.get("embedding"),
        Some(&Value::Array(vec![Value::Float(-1.0), Value::Float(2.0)]))
    );
    let _: BTreeMap<String, Value> = row.clone();
}

#[test]
fn p9_vector_005_backfill_boundaries_roll_back_completely() {
    for failpoint in [
        Failpoint::AfterVectorHiddenCatalog,
        Failpoint::AfterVectorPhysicalColumn,
        Failpoint::AfterVectorBackfill,
    ] {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("CREATE item:a SET embedding = [1,0]")
            .unwrap();
        connection.arm_failpoint(failpoint);
        let error = connection
            .execute("DEFINE FIELD embedding ON item TYPE array<float, 2>")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Transaction);
        connection.disarm_all_failpoints();

        connection
            .execute("DEFINE FIELD embedding ON item TYPE array<float, 2>")
            .unwrap();
        let result = connection
            .execute("SELECT id FROM item WHERE embedding <|1,EUCLIDEAN|> [1,0]")
            .unwrap();
        assert_eq!(rows(&result.statements[0]).len(), 1);
    }
}

#[test]
fn p9_vector_006_reopen_rejects_document_blob_disagreement() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vector-corrupt.fastdb");
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE point:a SET embedding = [1,0]; \
                 DEFINE FIELD embedding ON point TYPE array<float, 2>",
            )
            .unwrap();
        let native = connection.native();
        let table = common::physical_name_for(native, "point").unwrap();
        let column = common::native_rows(
            native,
            "SELECT physical_name FROM __fastdb_hidden_columns \
             WHERE provider = 'BUILTIN_VECTOR_EXACT'",
        )[0][0]
            .clone();
        common::native_exec(native, &format!("UPDATE {table} SET {column} = x'00'"));
        connection.close().unwrap();
    }
    let error = Database::open(path.to_str().unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Format);
    assert!(error.to_string().contains("native vector state disagree"));
}

#[test]
fn p9_vector_007_abrupt_exit_recovers_document_and_native_vectors() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vector-crash.fastdb");
    let status = Command::new(env!("CARGO_BIN_EXE_phase5_crash_helper"))
        .arg("vector-write")
        .arg(&path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    let result = connection
        .execute("SELECT id FROM point WHERE embedding <|2,EUCLIDEAN|> [1,0]")
        .unwrap();
    assert_eq!(rows(&result.statements[0]).len(), 2);
    assert_eq!(common::integrity_check(connection.native()), "ok");
}

#[test]
fn p9_vector_008_committed_fixture_reopens_mutates_and_searches() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vector-fixture.fastdb");
    std::fs::write(&path, FORMAT_TWO_VECTOR).unwrap();
    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    let result = connection
        .execute(
            "SELECT id FROM point WHERE active = true \
             AND embedding <|2,COSINE|> [1,0]",
        )
        .unwrap();
    assert_eq!(rows(&result.statements[0]).len(), 2);
    connection
        .execute("UPDATE point:b SET embedding = [0.8,0.2]")
        .unwrap();
    let result = connection
        .execute("SELECT id FROM point WHERE embedding <|1,EUCLIDEAN|> [0.75,0.25]")
        .unwrap();
    let Value::Object(row) = &rows(&result.statements[0])[0] else {
        panic!("expected projected row")
    };
    assert_eq!(
        row.get("id"),
        Some(&Value::RecordId(RecordId::new("point", "b")))
    );
    assert_eq!(common::integrity_check(connection.native()), "ok");
}

#[test]
fn p9_vector_009_bound_parameter_reaches_dimension_ceiling() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "DEFINE TABLE huge SCHEMAFULL; \
             DEFINE FIELD embedding ON huge TYPE array<float, 65536>",
        )
        .unwrap();
    let vector = Value::Array(vec![Value::Float(0.25); 65_536]);
    let params = Params::from([("vector".to_string(), vector)]);
    connection
        .execute_with_params("CREATE huge:a SET embedding = $vector", &params)
        .unwrap();
    let result = connection
        .execute_with_params(
            "SELECT id FROM huge WHERE embedding <|1,EUCLIDEAN|> $vector",
            &params,
        )
        .unwrap();
    assert_eq!(rows(&result.statements[0]).len(), 1);

    let too_large = Params::from([(
        "vector".to_string(),
        Value::Array(vec![Value::Float(0.25); 65_537]),
    )]);
    assert_eq!(
        connection
            .execute_with_params("SELECT * FROM huge", &too_large)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
}

#[test]
fn p9_vector_010_cosine_rejects_zero_stored_vectors() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE point:zero SET embedding = [0,0]; \
             CREATE point:one SET embedding = [1,0]; \
             DEFINE FIELD embedding ON point TYPE array<float, 2>",
        )
        .unwrap();
    let error = connection
        .execute("SELECT * FROM point WHERE embedding <|1,COSINE|> [1,0]")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Schema);
    assert!(error.to_string().contains("zero-magnitude stored vector"));
    let euclidean = connection
        .execute("SELECT id FROM point WHERE embedding <|1,EUCLIDEAN|> [0,0]")
        .unwrap();
    assert_eq!(rows(&euclidean.statements[0]).len(), 1);
}

#[test]
fn p9_vector_011_relation_fts_transactions_and_cascade_share_derived_state() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "DEFINE TABLE links TYPE RELATION FROM person TO person ENFORCED; \
             CREATE person:a CONTENT {}; CREATE person:b CONTENT {}; \
             RELATE person:a->links->person:b \
               SET embedding = [1,0], note = 'Rust vector edge'; \
             DEFINE FIELD embedding ON links TYPE array<float, 2>; \
             DEFINE ANALYZER blankish TOKENIZERS blank; \
             DEFINE INDEX note_idx ON links FIELDS note FULLTEXT ANALYZER blankish",
        )
        .unwrap();

    let vector = connection
        .execute(
            "SELECT in, out, vector::distance::knn() AS distance FROM links \
             WHERE embedding <|1,EUCLIDEAN|> [1,0]",
        )
        .unwrap();
    let Value::Object(edge) = &rows(&vector.statements[0])[0] else {
        panic!("expected relation projection")
    };
    assert_eq!(edge.get("distance"), Some(&Value::Float(0.0)));
    assert_eq!(
        edge.get("in"),
        Some(&Value::RecordId(RecordId::new("person", "a")))
    );
    assert_eq!(
        edge.get("out"),
        Some(&Value::RecordId(RecordId::new("person", "b")))
    );
    assert_eq!(
        rows(
            &connection
                .execute("SELECT id FROM links WHERE note @@ 'Rust vector'")
                .unwrap()
                .statements[0]
        )
        .len(),
        1
    );

    connection.execute("BEGIN").unwrap();
    connection
        .execute("UPDATE links SET embedding = [0,1]")
        .unwrap();
    let changed = connection
        .execute(
            "SELECT vector::distance::knn() AS distance FROM links \
             WHERE embedding <|1,EUCLIDEAN|> [0,1]",
        )
        .unwrap();
    let Value::Object(row) = &rows(&changed.statements[0])[0] else {
        panic!("expected distance projection")
    };
    assert_eq!(row.get("distance"), Some(&Value::Float(0.0)));
    connection.execute("CANCEL").unwrap();

    let restored = connection
        .execute(
            "SELECT vector::distance::knn() AS distance FROM links \
             WHERE embedding <|1,EUCLIDEAN|> [1,0]",
        )
        .unwrap();
    let Value::Object(row) = &rows(&restored.statements[0])[0] else {
        panic!("expected distance projection")
    };
    assert_eq!(row.get("distance"), Some(&Value::Float(0.0)));

    let deleted = connection.execute("DELETE person:b").unwrap();
    assert_eq!(deleted.mutation_count, 2);
    assert!(rows(
        &connection
            .execute("SELECT id FROM links WHERE embedding <|1,EUCLIDEAN|> [1,0]")
            .unwrap()
            .statements[0]
    )
    .is_empty());
    assert!(rows(
        &connection
            .execute("SELECT id FROM links WHERE note @@ 'Rust vector'")
            .unwrap()
            .statements[0]
    )
    .is_empty());
}
