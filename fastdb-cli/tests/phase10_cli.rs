#![forbid(unsafe_code)]
#![deny(warnings)]

use std::process::{Command, Output};
use tempfile::tempdir;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn p10_cli_001_check_backup_restore_and_rebuild_use_json_envelopes() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.fastdb");
    let backup = directory.path().join("backup.fastdb");
    let restored = directory.path().join("restored.fastdb");
    let seed = run(&[
        source.to_str().unwrap(),
        "-c",
        "CREATE doc:a SET text = 'Rust operations', embedding = [1,0]; \
         DEFINE FIELD embedding ON doc TYPE array<float, 2>; \
         DEFINE ANALYZER blankish TOKENIZERS blank; \
         DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish",
    ]);
    assert!(
        seed.status.success(),
        "{}",
        String::from_utf8_lossy(&seed.stderr)
    );

    let checked = run(&["check", source.to_str().unwrap(), "--output", "json"]);
    assert!(checked.status.success());
    let json: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["operation"], "check");
    assert_eq!(json["report"]["fts_indexes"], 1);
    assert_eq!(json["report"]["vector_fields"], 1);

    let rebuilt = run(&[
        "rebuild-index",
        source.to_str().unwrap(),
        "doc",
        "text_idx",
        "--output",
        "json",
    ]);
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );

    let backed_up = run(&[
        "backup",
        source.to_str().unwrap(),
        backup.to_str().unwrap(),
        "--output",
        "json",
    ]);
    assert!(
        backed_up.status.success(),
        "{}",
        String::from_utf8_lossy(&backed_up.stderr)
    );
    assert!(backup.is_file());

    let restore = run(&[
        "restore",
        backup.to_str().unwrap(),
        restored.to_str().unwrap(),
        "--output",
        "json",
    ]);
    assert!(
        restore.status.success(),
        "{}",
        String::from_utf8_lossy(&restore.stderr)
    );
    let queried = run(&[
        restored.to_str().unwrap(),
        "--output",
        "json",
        "-c",
        "SELECT id FROM doc WHERE text @@ 'Rust operations'",
    ]);
    assert!(
        queried.status.success(),
        "{}",
        String::from_utf8_lossy(&queried.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&queried.stdout).unwrap();
    assert_eq!(
        json["$fastdb"]["statements"][0]["value"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let overwrite = run(&[
        "restore",
        backup.to_str().unwrap(),
        restored.to_str().unwrap(),
        "--output",
        "json",
    ]);
    assert_eq!(overwrite.status.code(), Some(1));
    assert!(overwrite.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&overwrite.stderr).unwrap();
    assert_eq!(error["$fastdb"]["category"], "Io");
}

#[test]
fn p10_cli_002_invalid_restore_is_not_published() {
    let directory = tempdir().unwrap();
    let invalid = directory.path().join("invalid.fastdb");
    let destination = directory.path().join("destination.fastdb");
    std::fs::write(&invalid, b"not a database").unwrap();

    let restored = run(&[
        "restore",
        invalid.to_str().unwrap(),
        destination.to_str().unwrap(),
        "--output",
        "json",
    ]);
    assert_eq!(restored.status.code(), Some(1));
    assert!(!destination.exists());
    assert!(std::fs::read_dir(directory.path())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("fastdb-restore")));
}
