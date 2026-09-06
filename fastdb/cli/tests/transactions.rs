use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn cli_attaches_transaction_state_to_success_and_error_lines() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .arg(":memory:")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"CREATE TABLE posts\nBEGIN\nINSERT INTO posts {n:1}\nUPDATE posts SET n=2 RETURNING array::append(1,2) AS bad\nSELECT * FROM posts\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[1]["transaction"]["before"], "autocommit");
    assert_eq!(rows[1]["transaction"]["after"], "active");
    assert!(rows[3]["error"]["code"].is_string());
    assert_eq!(rows[3]["transaction"]["before"], "active");
    assert_eq!(rows[3]["transaction"]["after"], "autocommit");
    assert_eq!(rows[4]["rows"], serde_json::json!([]));
}
