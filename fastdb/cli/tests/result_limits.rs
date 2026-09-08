use std::{
    io::Write,
    process::{Command, Stdio},
};

fn run(mode: &str, script: &str) -> (bool, Vec<serde_json::Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .args([mode, ":memory:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let reports = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (output.status.success(), reports)
}

#[test]
fn limited_commands_report_boundaries_and_invalid_arguments() {
    for command in [".select-limit", ".profile-limit"] {
        let (ok, reports) = run("--script", &format!("{command} 1 9 SELECT 7 AS n;"));
        assert!(ok);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["rows"][0][0]["value"], 7);
        assert_eq!(
            reports[0].get("profile").is_some(),
            command == ".profile-limit"
        );
        for limits in ["0 9", "1 8"] {
            let (ok, reports) = run("--script", &format!("{command} {limits} SELECT 7 AS n;"));
            assert!(!ok);
            assert_eq!(reports[0]["error"]["code"], "FDB_LIMIT");
            assert!(reports[0].get("rows").is_none());
            assert!(reports[0].get("profile").is_none());
        }
    }
    for input in [
        ".select-limit",
        ".write-limit 1",
        ".profile-limit -1 9 SELECT 1",
        ".select-limit 1 99999999999999999999999999 SELECT 1",
        ".write-limit 1 9",
    ] {
        let (ok, reports) = run("--script", input);
        assert!(!ok, "{input}");
        assert_eq!(reports[0]["error"]["code"], "FDB_VALIDATION");
    }
    let (ok, reports) = run(
        "--script",
        ".select-limit\t1   2\nSELECT record::fetch(missing:key) AS n;",
    );
    assert!(ok);
    assert_eq!(reports[0]["rows"][0][0]["type"], "Null");
    let (ok, reports) = run(
        "--script",
        ".select-limit 1 9 SELECT 1; DELETE FROM missing;",
    );
    assert!(!ok);
    assert_eq!(reports.len(), 1);
    assert!(reports[0].get("error").is_some());
}

#[test]
fn limited_write_commands_preserve_pending_work_and_allow_retry() {
    let script = "CREATE TABLE docs;\nBEGIN;\nINSERT INTO docs {n:9};\n.write-limit 1 17 INSERT INTO docs(n) VALUES(1),(2) RETURNING n;\n.select-limit 1 9 SELECT n FROM docs;\n.write-limit 2 17 INSERT INTO docs(n) VALUES(1),(2) RETURNING n;\n.profile-limit 3 25 SELECT n FROM docs ORDER BY n;\n.write-limit 0 0 COMMIT;\nROLLBACK;\n.select-limit 0 1 SELECT n FROM docs;\n";
    for mode in ["--line", "--interactive"] {
        let (ok, reports) = run(mode, script);
        assert!(!ok);
        assert_eq!(reports.len(), 10, "{mode}: {reports:?}");
        assert_eq!(reports[3]["error"]["code"], "FDB_LIMIT");
        assert_eq!(
            reports[3]["transaction"],
            serde_json::json!({"before":"active","after":"active"})
        );
        assert_eq!(reports[4]["rows"][0][0]["value"], 9);
        assert_eq!(reports[5]["affected"], 2);
        assert_eq!(reports[6]["rows"].as_array().unwrap().len(), 3);
        assert!(reports[6]["profile"].is_object());
        assert_eq!(reports[7]["error"]["code"], "FDB_UNSUPPORTED");
        assert_eq!(reports[7]["transaction"]["after"], "active");
        assert_eq!(reports[9]["rows"], serde_json::json!([]));
    }
}
