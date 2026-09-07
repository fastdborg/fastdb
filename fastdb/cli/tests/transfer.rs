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

#[test]
#[cfg(target_os = "linux")]
fn transfer_output_failures_preserve_committed_data_in_both_formats() {
    fn fail_output(args: &[&str], input: &[u8]) -> std::process::Output {
        let full = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(full))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
    let dir = std::env::temp_dir().join(format!(
        "fastdb-transfer-output-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("source.db");
    let source = source.to_str().unwrap();
    assert!(run(
        &[source],
        b"CREATE TABLE docs; INSERT INTO docs {id:docs:first,n:1};"
    )
    .status
    .success());
    for ndjson in [false, true] {
        let target = dir.join(if ndjson { "ndjson.db" } else { "json.db" });
        let target = target.to_str().unwrap();
        assert!(run(&[target], b"CREATE TABLE docs;").status.success());
        let mut export_args = vec!["--export", "docs", source];
        let mut import_args = vec!["--import", "docs", target];
        let mut target_args = vec!["--export", "docs", target];
        if ndjson {
            export_args.push("--ndjson");
            import_args.push("--ndjson");
            target_args.push("--ndjson");
        }
        let exported = run(&export_args, b"");
        assert!(exported.status.success(), "{exported:?}");
        for failed in [
            fail_output(&export_args, b""),
            fail_output(&import_args, &exported.stdout),
        ] {
            assert!(!failed.status.success());
            let message = String::from_utf8_lossy(&failed.stderr);
            assert!(!message.is_empty());
            assert!(!message.contains("panicked"), "{message}");
        }
        let actual = run(&target_args, b"");
        assert!(actual.status.success(), "{actual:?}");
        assert_eq!(actual.stdout, exported.stdout);
        assert!(!run(&import_args, &exported.stdout).status.success());
        assert_eq!(run(&target_args, b"").stdout, exported.stdout);
        assert_eq!(run(&export_args, b"").stdout, exported.stdout);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
