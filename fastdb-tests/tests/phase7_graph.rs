#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Arc, Barrier};
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint, Params, RecordId, StatementResult, Value};

const FORMAT_TWO_GRAPH: &[u8] = include_bytes!("../fixtures/phase7-format2-graph.fastdb");

fn rows(result: &StatementResult) -> &[Value] {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    rows
}

fn projected_ids(connection: &turso_fastdb::Connection, source: &str, alias: &str) -> Vec<String> {
    let response = connection.execute(source).unwrap();
    let Value::Object(object) = &rows(&response.statements[0])[0] else {
        panic!("expected projected object")
    };
    let Some(Value::Array(values)) = object.get(alias) else {
        panic!("expected projected ID array")
    };
    let mut ids = values
        .iter()
        .map(|value| match value {
            Value::RecordId(value) => value.to_string(),
            _ => panic!("expected record ID"),
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[test]
fn p7_graph_002_dangling_edges_bound_endpoints_rollback_and_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("graph.fastdb");
    let path = path.to_str().unwrap();

    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();

        let dangling = connection
            .execute("RELATE ONLY person:missing->likes->post:missing SET weight = 1")
            .unwrap();
        let StatementResult::Value(Value::Object(edge)) = &dangling.statements[0] else {
            panic!("expected dangling edge")
        };
        assert_eq!(
            edge.get("in"),
            Some(&Value::RecordId(RecordId::new("person", "missing")))
        );
        assert_eq!(
            edge.get("out"),
            Some(&Value::RecordId(RecordId::new("post", "missing")))
        );

        connection
            .execute("CREATE person:one CONTENT {}; CREATE post:one CONTENT {}")
            .unwrap();
        let params = Params::from([
            (
                "from".to_string(),
                Value::RecordId(RecordId::new("person", "one")),
            ),
            (
                "to".to_string(),
                Value::RecordId(RecordId::new("post", "one")),
            ),
        ]);
        connection
            .execute_with_params("RELATE $from->likes->$to", &params)
            .unwrap();

        connection.execute("BEGIN").unwrap();
        connection
            .execute("RELATE person:one->likes->post:missing")
            .unwrap();
        connection.execute("CANCEL").unwrap();
        assert_eq!(
            rows(
                &connection
                    .execute("SELECT * FROM likes")
                    .unwrap()
                    .statements[0]
            )
            .len(),
            2
        );
    }

    {
        let database = Database::open(path).unwrap();
        let connection = database.connect().unwrap();
        let selected = connection.execute("SELECT * FROM likes").unwrap();
        assert_eq!(rows(&selected.statements[0]).len(), 2);

        let traversal = connection
            .execute("SELECT ->likes->post AS ids, ->likes->post.* AS docs FROM person:missing")
            .unwrap();
        assert!(rows(&traversal.statements[0]).is_empty());

        let deleted = connection.execute("DELETE post:one").unwrap();
        assert_eq!(deleted.mutation_count, 2);
        assert_eq!(
            rows(
                &connection
                    .execute("SELECT * FROM likes")
                    .unwrap()
                    .statements[0]
            )
            .len(),
            1
        );
    }
}

#[test]
fn p7_graph_003_enforced_schemafull_and_immutable_endpoints_fail_atomically() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "DEFINE TABLE owns SCHEMAFULL TYPE RELATION FROM person TO asset ENFORCED; \
             DEFINE FIELD since ON owns TYPE int",
        )
        .unwrap();

    let missing = connection
        .execute("RELATE person:one->owns->asset:one SET since = 2026")
        .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::Constraint);
    assert_eq!(
        rows(&connection.execute("SELECT * FROM owns").unwrap().statements[0]).len(),
        0
    );

    connection
        .execute("CREATE person:one CONTENT {}; CREATE asset:one CONTENT {}")
        .unwrap();
    connection
        .execute("RELATE person:one->owns->asset:one SET since = 2026")
        .unwrap();
    let immutable = connection
        .execute("UPDATE owns SET in = person:two")
        .unwrap_err();
    assert_eq!(immutable.category(), ErrorCategory::Schema);

    let wrong_schema = connection
        .execute("RELATE person:one->owns->asset:one SET unknown = true")
        .unwrap_err();
    assert_eq!(wrong_schema.category(), ErrorCategory::Schema);
    let selected = connection.execute("SELECT * FROM owns").unwrap();
    assert_eq!(rows(&selected.statements[0]).len(), 1);

    let StatementResult::Rows(edges) = &selected.statements[0] else {
        unreachable!()
    };
    let Value::Object(edge) = &edges[0] else {
        panic!("expected edge object")
    };
    assert_eq!(edge.get("since"), Some(&Value::Integer(2026)));
    assert_eq!(
        edge.keys().cloned().collect::<Vec<_>>(),
        BTreeMap::from([
            ("id".to_string(), ()),
            ("in".to_string(), ()),
            ("out".to_string(), ()),
            ("since".to_string(), ()),
        ])
        .into_keys()
        .collect::<Vec<_>>()
    );
}

#[test]
fn p7_graph_004_catalog_edge_and_cascade_failpoints_roll_back_every_component() {
    for failpoint in [
        Failpoint::AfterCatalogRow,
        Failpoint::AfterGraphHiddenCatalog,
        Failpoint::AfterPhysicalDdl,
        Failpoint::AfterGraphForwardIndex,
        Failpoint::AfterGraphReverseIndex,
    ] {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        connection.arm_failpoint(failpoint);
        let error = connection
            .execute("DEFINE TABLE links TYPE RELATION FROM person TO post")
            .unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::Transaction,
            "{failpoint:?}"
        );
        connection.disarm_all_failpoints();
        connection
            .execute("DEFINE TABLE links TYPE RELATION FROM person TO post")
            .unwrap();
        assert!(rows(
            &connection
                .execute("SELECT * FROM links")
                .unwrap()
                .statements[0]
        )
        .is_empty());
    }

    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE person:one CONTENT {}; CREATE post:one CONTENT {}")
        .unwrap();
    connection.arm_failpoint(Failpoint::AfterGraphEdgeInsert);
    assert_eq!(
        connection
            .execute("RELATE person:one->links->post:one")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    assert!(rows(
        &connection
            .execute("SELECT * FROM links")
            .unwrap()
            .statements[0]
    )
    .is_empty());

    connection
        .execute("RELATE person:one->links->post:one")
        .unwrap();
    connection.arm_failpoint(Failpoint::AfterDeleteMutation);
    assert_eq!(
        connection
            .execute("DELETE person:one")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.disarm_all_failpoints();
    assert_eq!(
        rows(
            &connection
                .execute("SELECT * FROM person:one")
                .unwrap()
                .statements[0]
        )
        .len(),
        1
    );
    assert_eq!(
        rows(
            &connection
                .execute("SELECT * FROM links")
                .unwrap()
                .statements[0]
        )
        .len(),
        1
    );
}

#[test]
fn p7_graph_005_reopen_validation_rejects_graph_catalog_and_physical_corruption() {
    for corruption in 0..5 {
        let database = Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("DEFINE TABLE links TYPE RELATION FROM person TO post")
            .unwrap();
        let before = connection.catalog_state().unwrap();
        match corruption {
            0 => common::native_exec(
                connection.native(),
                "DELETE FROM __fastdb_hidden_columns WHERE options_json='{\"role\":\"in_rid\"}'",
            ),
            1 => common::native_exec(
                connection.native(),
                "UPDATE __fastdb_hidden_columns SET options_json='{\"role\":\"in_table\"}' \
                 WHERE options_json='{\"role\":\"out_table\"}'",
            ),
            2 => common::native_exec(
                connection.native(),
                "DELETE FROM __fastdb_indexes WHERE options_json='{\"direction\":\"reverse\"}'",
            ),
            3 => common::native_exec(
                connection.native(),
                "UPDATE __fastdb_indexes SET provider_version=99 \
                 WHERE index_kind='GRAPH_ADJACENCY'",
            ),
            4 => {
                let snapshot = before.snapshot().unwrap();
                let index = snapshot.tables["links"].indexes.values().next().unwrap();
                common::native_exec(
                    connection.native(),
                    &format!("DROP INDEX {}", index.physical_name),
                );
            }
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
fn p7_graph_006_direction_chain_duplicate_and_self_loop_model() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE person:a CONTENT {}; CREATE person:b CONTENT {}; \
             CREATE person:c CONTENT {}; \
             RELATE person:a->follows->person:b; \
             RELATE person:c->follows->person:b; \
             RELATE person:b->follows->person:b; \
             RELATE person:a->follows->person:b",
        )
        .unwrap();

    assert_eq!(
        projected_ids(
            &connection,
            "SELECT ->follows->person AS ids FROM person:a",
            "ids"
        ),
        ["person:`b`", "person:`b`"]
    );
    assert_eq!(
        projected_ids(
            &connection,
            "SELECT <-follows<-person AS ids FROM person:b",
            "ids"
        ),
        ["person:`a`", "person:`a`", "person:`b`", "person:`c`"]
    );
    assert_eq!(
        projected_ids(
            &connection,
            "SELECT <->follows<->person AS ids FROM person:b",
            "ids"
        ),
        [
            "person:`a`",
            "person:`a`",
            "person:`b`",
            "person:`b`",
            "person:`c`"
        ]
    );
    assert_eq!(
        projected_ids(
            &connection,
            "SELECT ->follows->person->follows->person AS ids FROM person:a",
            "ids"
        ),
        ["person:`b`", "person:`b`"]
    );

    let deleted = connection.execute("DELETE person:b").unwrap();
    assert_eq!(deleted.mutation_count, 5, "one node plus four edge records");
    assert!(rows(
        &connection
            .execute("SELECT * FROM follows")
            .unwrap()
            .statements[0]
    )
    .is_empty());
}

#[test]
fn p7_graph_007_abrupt_exit_recovers_committed_edge_and_cascade() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("graph-crash.fastdb");
    let helper = env!("CARGO_BIN_EXE_phase5_crash_helper");
    let status = Command::new(helper)
        .arg("graph-write")
        .arg(&path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));

    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            rows(
                &connection
                    .execute("SELECT * FROM links")
                    .unwrap()
                    .statements[0]
            )
            .len(),
            1
        );
        connection.close().unwrap();
    }

    let status = Command::new(helper)
        .arg("graph-cascade")
        .arg(&path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    assert!(rows(
        &connection
            .execute("SELECT * FROM links")
            .unwrap()
            .statements[0]
    )
    .is_empty());
    assert!(rows(
        &connection
            .execute("SELECT * FROM person:one")
            .unwrap()
            .statements[0]
    )
    .is_empty());
    assert_eq!(common::integrity_check(connection.native()), "ok");
}

#[test]
fn p7_graph_008_concurrent_auto_registration_has_one_graph_catalog_and_two_edges() {
    let database = Database::open_memory().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();
    for suffix in ["one", "two"] {
        let database = database.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let connection = database.connect().unwrap();
            barrier.wait();
            connection
                .execute(&format!("RELATE person:{suffix}->links->post:{suffix}"))
                .unwrap();
        }));
    }
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    let connection = database.connect().unwrap();
    assert_eq!(
        rows(
            &connection
                .execute("SELECT * FROM links")
                .unwrap()
                .statements[0]
        )
        .len(),
        2
    );
    let snapshot = connection.catalog_state().unwrap();
    let snapshot = snapshot.snapshot().unwrap();
    assert_eq!(
        snapshot
            .tables
            .values()
            .filter(|table| table.kind == turso_fastdb::catalog::TableKind::Relation)
            .count(),
        1
    );
    assert_eq!(snapshot.hidden_columns.len(), 4);
    assert_eq!(snapshot.tables["links"].indexes.len(), 2);
}

#[test]
fn p7_graph_009_committed_graph_fixture_reopens_mutates_and_uses_both_indexes() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("graph-fixture.fastdb");
    std::fs::write(&path, FORMAT_TWO_GRAPH).unwrap();
    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE post:two CONTENT {}; RELATE person:one->wrote->post:two SET role='editor'")
        .unwrap();
    let response = connection
        .execute("SELECT ->wrote->post AS ids FROM person:one")
        .unwrap();
    let Value::Object(projected) = &rows(&response.statements[0])[0] else {
        panic!("expected projection")
    };
    assert!(matches!(projected.get("ids"), Some(Value::Array(ids)) if ids.len() == 2));
    let explain = connection
        .execute(
            "EXPLAIN SELECT ->wrote->post AS forward, <-wrote<-person AS reverse FROM person:one",
        )
        .unwrap();
    let details = rows(&explain.statements[0])
        .iter()
        .filter_map(|value| match value {
            Value::Object(value) => match value.get("detail") {
                Some(Value::Str(value)) => Some(value.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let state = connection.catalog_state().unwrap();
    for index in state.snapshot().unwrap().tables["wrote"].indexes.values() {
        assert!(details
            .iter()
            .any(|detail| detail.contains(&index.physical_name)));
    }
    assert_eq!(common::integrity_check(connection.native()), "ok");
}
