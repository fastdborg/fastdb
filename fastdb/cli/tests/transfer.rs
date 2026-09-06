use std::{
    io::Write,
    process::{Command, Stdio},
};
fn run(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn cli_exports_and_imports_persistent_collections() {
    let dir = std::env::temp_dir().join(format!(
        "fastdb-transfer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let a = dir.join("a.db");
    let b = dir.join("b.db");
    let a = a.to_str().unwrap();
    let b = b.to_str().unwrap();
    assert!(run(
        &[a],
        b"CREATE TABLE docs; INSERT INTO docs {id:docs:p1,value:9223372036854775807};"
    )
    .status
    .success());
    assert!(run(&[b], b"CREATE TABLE docs;").status.success());
    let export = run(&["--export", "docs", "--ndjson", a], b"");
    assert!(export.status.success(), "{export:?}");
    let import = run(&["--import", "docs", "--ndjson", b], &export.stdout);
    assert!(import.status.success(), "{import:?}");
    assert_eq!(
        run(&["--export", "docs", "--ndjson", b], b"").stdout,
        export.stdout
    );
    assert!(!run(&["--import", "docs", "--ndjson", b], &export.stdout)
        .status
        .success());
    assert_eq!(
        run(&["--export", "docs", "--ndjson", b], b"").stdout,
        export.stdout
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
