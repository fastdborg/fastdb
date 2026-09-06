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
    assert_eq!(rows[3]["error"]["code"], "FDB_ENGINE");
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
