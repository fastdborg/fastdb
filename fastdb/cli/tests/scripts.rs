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
