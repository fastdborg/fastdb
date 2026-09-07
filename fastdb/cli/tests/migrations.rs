use std::process::Command;
#[test]
fn cli_migrations_persist_and_detect_edited_files() {
    let root = std::env::temp_dir().join(format!(
        "fastdb-migrations-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir = root.join("migrations");
    std::fs::create_dir_all(&dir).unwrap();
    let db = root.join("test.db");
    let first = dir.join("001_create.sql");
    std::fs::write(&first, "CREATE TABLE docs;").unwrap();
    std::fs::write(
        dir.join("002_insert.sql"),
        "INSERT INTO docs {id:docs:p1,value:1};",
    )
    .unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
            .arg("--migrate")
            .arg(&dir)
            .arg(&db)
            .output()
            .unwrap()
    };
    let output = run();
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["applied"], serde_json::json!([1, 2]));
    let output = run();
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["already_applied"], 2);
    std::fs::write(first, "CREATE TABLE docs; -- edited").unwrap();
    assert!(!run().status.success());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_rejects_non_file_migration_sources_and_allows_retry() {
    let root = std::env::temp_dir().join(format!(
        "fastdb-migration-source-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir = root.join("migrations");
    std::fs::create_dir_all(&dir).unwrap();
    let invalid = dir.join("002_directory.sql");
    std::fs::create_dir(&invalid).unwrap();
    std::fs::write(dir.join("001_create.sql"), "CREATE TABLE docs;").unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_fastdb-cli"))
            .arg("--migrate")
            .arg(&dir)
            .arg(root.join("test.db"))
            .output()
            .unwrap()
    };
    let output = run();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("migration source must be a regular file"),
        "{error}"
    );
    assert!(error.contains("002_directory.sql"), "{error}");
    std::fs::remove_dir(invalid).unwrap();
    for (name, contents, diagnostic) in [
        ("002_invalid_utf8.sql", vec![0xff], "cannot read migration"),
        (
            "bad_version.sql",
            b"SELECT 1;".to_vec(),
            "invalid migration version",
        ),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        let output = run();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(diagnostic), "{error}");
        assert!(error.contains(name), "{error}");
        std::fs::remove_file(path).unwrap();
    }
    let output = run();
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["already_applied"], 0);
    assert_eq!(report["applied"], serde_json::json!([1]));
    std::fs::remove_dir_all(root).unwrap();
}
