use std::{
    io::Write,
    process::{Command, Stdio},
};
fn run(script: &str) -> (bool, Vec<serde_json::Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .arg(":memory:")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (output.status.success(), rows)
}
#[test]
fn cli_runs_multiline_scripts_and_returns_nonzero_on_first_failure() {
    let (ok,rows)=run("CREATE TABLE posts;\nINSERT INTO posts {\n id:posts:p1, text:'a;b'\n}; SELECT text FROM posts");
    assert!(ok);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2]["rows"][0][0]["value"], "a;b");
    assert!(rows[2]["offset"].is_number());
    let (ok,rows)=run("CREATE TABLE posts; INSERT INTO posts {id:posts:p1}; INSERT INTO posts {id:posts:p1}; SELECT 1;");
    assert!(!ok);
    assert_eq!(rows.len(), 3);
    assert!(rows[2]["error"].is_object());
    let (ok, rows) = run("CREATE TABLE posts; SELECT 'unterminated");
    assert!(!ok);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["error"]["code"], "FDB_SYNTAX");
}

#[test]
fn interactive_input_accumulates_multiline_statements_and_recovers_after_errors() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .args(["--interactive", ":memory:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"CREATE TABLE posts;\nBEGIN;\nINSERT INTO posts {\nid:posts:p1, text:'a;b'\n};\nINSERT INTO posts {id:posts:p1};\nROLLBACK;\nSELECT * FROM posts;\nSELECT 'unfinished\n.clear\nSELECT 7 AS n;\n.quit\nINSERT INTO posts {id:posts:never};\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[3]["error"]["code"], "FDB_CONSTRAINT");
    assert_eq!(rows[3]["transaction"]["after"], "active");
    assert_eq!(rows[5]["rows"], serde_json::json!([]));
    assert_eq!(rows[6]["rows"][0][0]["value"], 7);
    let prompts = String::from_utf8(output.stderr).unwrap();
    assert!(prompts.contains("fastdb(tx)> "));
    assert!(prompts.contains("...> "));
}

#[test]
fn interactive_trigger_bodies_wait_for_end_and_eof_runs_trailing_sql() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .args(["--interactive", ":memory:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"CREATE TABLE source(n);\nCREATE TABLE audit(n);\nCREATE TRIGGER record_insert AFTER INSERT ON source BEGIN\nINSERT INTO audit VALUES (new.n);\nEND;\nINSERT INTO source VALUES (3);\nSELECT n FROM audit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[4]["rows"][0][0]["value"], 3);
}

fn limited(mode: &str, limit: usize, input: &str) -> (bool, Vec<serde_json::Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .arg(mode)
        .arg("--max-input-bytes")
        .arg(limit.to_string())
        .arg(":memory:")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (output.status.success(), rows)
}
#[test]
fn cli_input_limits_count_utf8_bytes_and_never_submit_truncated_scripts() {
    let script = "SELECT 'é' AS value;";
    let (ok, rows) = limited("--script", script.len(), script);
    assert!(ok);
    assert_eq!(rows[0]["rows"][0][0]["value"], "é");
    for limit in [script.len() - 1, "SELECT '".len()] {
        let (ok, rows) = limited("--script", limit, script);
        assert!(!ok);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["error"]["code"], "FDB_LIMIT");
    }
    let (ok, rows) = limited("--script", 32, &format!("SELECT 1; {}", "x".repeat(64)));
    assert!(!ok);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["error"]["code"], "FDB_LIMIT");
}
#[test]
fn line_and_interactive_limits_stop_without_consuming_tail_as_sql() {
    let (ok, rows) = limited(
        "--line",
        32,
        &format!("SELECT 1;\n{}\nSELECT 2;\n", "x".repeat(64)),
    );
    assert!(!ok);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["error"]["code"], "FDB_LIMIT");
    let (ok, rows) = limited(
        "--interactive",
        32,
        &format!(
            "BEGIN;\nSELECT '{}\n{}\nCOMMIT;\n",
            "x".repeat(16),
            "y".repeat(16)
        ),
    );
    assert!(!ok);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["error"]["code"], "FDB_LIMIT");
    assert_eq!(rows[1]["transaction"]["after"], "active");
}

#[test]
fn cli_profiles_selects_without_allowing_profiled_writes() {
    let (ok, rows) = run(".profile SELECT 42;");
    assert!(ok);
    assert_eq!(rows[0]["rows"][0][0]["value"], 42);
    assert!(rows[0]["profile"]["vm_steps"].as_u64().unwrap() > 0);
    for mode in ["--line", "--interactive"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
            .args([mode, ":memory:"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"CREATE TABLE docs;\nINSERT INTO docs {n:1};\n.profile SELECT n FROM docs;\n.profile DELETE FROM docs;\nSELECT count(*) FROM docs;\n").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        let rows = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 5, "{rows:?}");
        assert_eq!(rows[2]["rows"][0][0]["value"], 1);
        assert!(rows[2]["profile"]["rows_read"].as_u64().unwrap() > 0);
        assert_eq!(rows[3]["error"]["code"], "FDB_UNSUPPORTED");
        assert_eq!(rows[4]["rows"][0][0]["value"], 1);
    }
}
