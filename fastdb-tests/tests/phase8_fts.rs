#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::process::Command;
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint, StatementResult, Value};

const FORMAT_TWO_FTS: &[u8] = include_bytes!("../fixtures/phase8-format2-fts.fastdb");

fn rows(result: &StatementResult) -> &[Value] {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    rows
}

#[test]
fn p8_fts_003_backfill_churn_plan_rebuild_and_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("fts.fastdb");
    let path = path.to_str().unwrap();

    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE doc:one SET text = 'Rust web programming'; \
                 CREATE doc:two SET text = 'web Rust internals'; \
                 CREATE doc:three SET text = 'rust web lowercase'",
            )
            .unwrap();
        connection
            .execute("DEFINE ANALYZER blankish TOKENIZERS blank")
            .unwrap();
        connection
            .execute(
                "DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish HIGHLIGHTS",
            )
            .unwrap();

        let matched = connection
            .execute("SELECT id FROM doc WHERE text @@ 'Rust web'")
            .unwrap();
        assert_eq!(rows(&matched.statements[0]).len(), 2);

        let explained = connection
            .execute("EXPLAIN SELECT id FROM doc WHERE text @@ 'Rust web'")
            .unwrap();
        let details = rows(&explained.statements[0])
            .iter()
            .filter_map(|value| match value {
                Value::Object(row) => match row.get("detail") {
                    Some(Value::Str(value)) => Some(value.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            details.iter().any(|detail| detail.contains("__fastdb_i_")),
            "FTS plan did not name the opaque index: {details:?}"
        );

        connection
            .execute("UPDATE doc:three SET text = 'Rust web lowercase'")
            .unwrap();
        assert_eq!(
            rows(
                &connection
                    .execute("SELECT id FROM doc WHERE text @@ 'Rust web'")
                    .unwrap()
                    .statements[0]
            )
            .len(),
            3
        );
        connection.execute("DELETE doc:two").unwrap();
        connection.execute("REBUILD INDEX text_idx ON doc").unwrap();
    }

    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        let matched = connection
            .execute("SELECT id FROM doc WHERE text @@ 'Rust web'")
            .unwrap();
        assert_eq!(rows(&matched.statements[0]).len(), 2);
        let error = connection
            .execute("REMOVE INDEX text_idx ON doc")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Schema);
    }
}

#[test]
fn p8_fts_004_invalid_existing_or_new_values_roll_back_atomically() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE doc:one SET text = ['not', 'text']")
        .unwrap();
    connection
        .execute("DEFINE ANALYZER blankish TOKENIZERS blank")
        .unwrap();
    let error = connection
        .execute("DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Constraint, "{error}");

    connection
        .execute("UPDATE doc:one SET text = 'valid'; DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish")
        .unwrap();
    let error = connection
        .execute("UPDATE doc:one SET text = { nested: true }")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Constraint);
    let matched = connection
        .execute("SELECT * FROM doc WHERE text @@ 'valid'")
        .unwrap();
    assert_eq!(rows(&matched.statements[0]).len(), 1);
}

#[test]
fn p8_fts_005_provider_publication_failpoints_leave_no_partial_index() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection.arm_failpoint(Failpoint::AfterFtsAnalyzerCatalog);
    assert_eq!(
        connection
            .execute("DEFINE ANALYZER a TOKENIZERS blank")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    connection
        .execute("DEFINE ANALYZER a TOKENIZERS blank")
        .unwrap();

    for failpoint in [
        Failpoint::AfterFtsHiddenCatalog,
        Failpoint::AfterFtsPhysicalColumn,
        Failpoint::AfterFtsBackfill,
        Failpoint::AfterFtsProviderIndex,
    ] {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE doc:one SET text = 'rollback term'; DEFINE ANALYZER a TOKENIZERS blank",
            )
            .unwrap();
        connection.arm_failpoint(failpoint);
        let error = connection
            .execute("DEFINE INDEX i ON doc FIELDS text FULLTEXT ANALYZER a")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Transaction);
        connection.disarm_all_failpoints();

        connection
            .execute("DEFINE INDEX i ON doc FIELDS text FULLTEXT ANALYZER a")
            .unwrap();
        let matched = connection
            .execute("SELECT * FROM doc WHERE text @@ 'rollback'")
            .unwrap();
        assert_eq!(rows(&matched.statements[0]).len(), 1, "{failpoint:?}");
    }
}

#[test]
fn p8_fts_006_reopen_validation_rejects_provider_corruption() {
    for corruption in 0..5 {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE doc:one SET text = 'integrity term'; \
                 DEFINE ANALYZER a TOKENIZERS blank; \
                 DEFINE INDEX i ON doc FIELDS text FULLTEXT ANALYZER a",
            )
            .unwrap();
        let before = connection.catalog_state().unwrap();
        match corruption {
            0 => common::native_exec(
                connection.native(),
                "UPDATE __fastdb_analyzers SET provider_version=99",
            ),
            1 => common::native_exec(
                connection.native(),
                "DELETE FROM __fastdb_hidden_columns WHERE provider='BUILTIN_FTS'",
            ),
            2 => common::native_exec(
                connection.native(),
                "UPDATE __fastdb_hidden_columns SET options_json='{}' WHERE provider='BUILTIN_FTS'",
            ),
            3 => common::native_exec(
                connection.native(),
                "UPDATE __fastdb_indexes SET options_json='{}' WHERE index_kind='FTS'",
            ),
            4 => common::native_exec(
                connection.native(),
                "DELETE FROM __fastdb_capabilities WHERE provider='BUILTIN_FTS'",
            ),
            _ => unreachable!(),
        }
        assert_eq!(
            connection.reload_catalog().unwrap_err().category(),
            ErrorCategory::Format,
            "corruption case {corruption}"
        );
        assert_eq!(connection.catalog_state().unwrap(), before);
    }
}

#[test]
fn p8_fts_007_relation_fields_are_indexed_and_cascade_atomically() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE person:one CONTENT {}; CREATE person:two CONTENT {}; \
             RELATE person:one->knows->person:two SET note = 'Rust graph'; \
             DEFINE ANALYZER a TOKENIZERS blank; \
             DEFINE INDEX note_idx ON knows FIELDS note FULLTEXT ANALYZER a",
        )
        .unwrap();
    assert_eq!(
        rows(
            &connection
                .execute("SELECT in, out FROM knows WHERE note @@ 'Rust graph'")
                .unwrap()
                .statements[0]
        )
        .len(),
        1
    );
    connection.execute("DELETE person:one").unwrap();
    assert!(rows(
        &connection
            .execute("SELECT * FROM knows WHERE note @@ 'Rust'")
            .unwrap()
            .statements[0]
    )
    .is_empty());
}

#[test]
fn p8_fts_008_surreal_blank_scores_match_the_v315_reference() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE doc:one SET text = 'x x'; CREATE doc:two SET text = 'x'; \
             CREATE doc:three SET text = 'a'; CREATE doc:four SET text = 'b'; \
             CREATE doc:five SET text = 'c'; \
             DEFINE ANALYZER a TOKENIZERS blank; \
             DEFINE INDEX i ON doc FIELDS text FULLTEXT ANALYZER a;",
        )
        .unwrap();
    let result = connection
        .execute("SELECT id, search::score(1) AS score FROM doc WHERE text @1@ 'x'")
        .unwrap();
    let mut scores = rows(&result.statements[0])
        .iter()
        .map(|value| {
            let Value::Object(row) = value else {
                panic!("expected object")
            };
            let Value::RecordId(id) = &row["id"] else {
                panic!("expected typed id")
            };
            let Value::Float(score) = row["score"] else {
                panic!("expected score")
            };
            (format!("{:?}", id.id), score)
        })
        .collect::<Vec<_>>();
    scores.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(scores.len(), 2);
    assert!((scores[0].1 - 0.3587977886199951).abs() < 0.000_001);
    assert!((scores[1].1 - 0.36109215021133423).abs() < 0.000_001);
}

#[test]
fn p8_fts_009_rejects_ambiguous_queries_references_and_provider_options() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE doc:one SET text = 'Rust web'; DEFINE ANALYZER a TOKENIZERS blank; \
             DEFINE INDEX i ON doc FIELDS text FULLTEXT ANALYZER a",
        )
        .unwrap();
    for source in [
        "SELECT * FROM doc WHERE text @@ 'Rust' OR text = 'Rust web'",
        "SELECT search::score(2) AS score FROM doc WHERE text @1@ 'Rust'",
        "SELECT * FROM doc WHERE text @@ 'Rust' AND text @@ 'web'",
        "CREATE INDEX bad ON doc USING fts (text) WITH (unknown = 'x')",
        "CREATE INDEX bad ON doc USING fts (text) WITH (weights = [1, 2])",
        "CREATE INDEX bad ON doc USING fts (text) WITH (tokenizer = 'missing')",
    ] {
        assert_eq!(
            connection.execute(source).unwrap_err().category(),
            ErrorCategory::Schema,
            "{source}"
        );
    }
}

#[test]
fn p8_fts_010_abrupt_exit_recovers_document_and_provider_state() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("fts-crash.fastdb");
    let status = Command::new(env!("CARGO_BIN_EXE_phase5_crash_helper"))
        .arg("fts-write")
        .arg(&path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));

    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    assert_eq!(
        rows(
            &connection
                .execute("SELECT * FROM doc WHERE text @@ 'recovered'")
                .unwrap()
                .statements[0]
        )
        .len(),
        2
    );
    let integrity = common::integrity_check(connection.native());
    assert!(
        integrity.starts_with("wrong # of entries in index __turso_internal_fts_dir_")
            && integrity.ends_with("_key"),
        "unexpected pinned-provider integrity result: {integrity}"
    );
}

#[test]
fn p8_fts_011_broad_matches_stop_at_the_candidate_limit() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection.execute("BEGIN").unwrap();
    for batch in 0..=100 {
        let start = batch * 100;
        let end = usize::min(start + 100, 10_001);
        if start == end {
            break;
        }
        let mut source = String::new();
        for id in start..end {
            source.push_str(&format!("CREATE doc:r{id} SET text = 'common';"));
        }
        connection.execute(&source).unwrap();
    }
    connection
        .execute(
            "DEFINE ANALYZER a TOKENIZERS blank; \
             DEFINE INDEX i ON doc FIELDS text FULLTEXT ANALYZER a; COMMIT",
        )
        .unwrap();

    let error = connection
        .execute("SELECT * FROM doc WHERE text @@ 'common'")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Constraint, "{error}");
    assert!(error.to_string().contains("10,000"), "{error}");
}

#[test]
fn p8_fts_012_committed_fixture_reopens_mutates_and_selects_provider() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("fts-fixture.fastdb");
    std::fs::write(&path, FORMAT_TWO_FTS).unwrap();

    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    let explained = connection
        .execute("EXPLAIN SELECT id FROM article WHERE body @@ 'fixture'")
        .unwrap();
    assert!(rows(&explained.statements[0]).iter().any(|value| {
        matches!(
            value,
            Value::Object(row)
                if matches!(row.get("detail"), Some(Value::Str(detail)) if detail.contains("__fastdb_i_"))
        )
    }));
    assert_eq!(
        rows(
            &connection
                .execute("SELECT id FROM article WHERE body @@ 'fixture'")
                .unwrap()
                .statements[0]
        )
        .len(),
        2
    );
    connection
        .execute("CREATE article:three SET body = 'third fixture document'")
        .unwrap();
    assert_eq!(
        rows(
            &connection
                .execute("SELECT id FROM article WHERE body @@ 'fixture'")
                .unwrap()
                .statements[0]
        )
        .len(),
        3
    );
}
