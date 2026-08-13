#![forbid(unsafe_code)]
#![deny(warnings)]

use std::process::Command;

#[test]
fn p8_cli_001_surreal_fts_uses_the_json_envelope() {
    let output = Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args([
            "--memory",
            "--output",
            "json",
            "-c",
            "DEFINE ANALYZER blankish TOKENIZERS blank; \
             CREATE article:one SET body = 'Rust web'; \
             DEFINE INDEX body_idx ON article FIELDS body FULLTEXT ANALYZER blankish HIGHLIGHTS; \
             SELECT id, search::highlight('<b>', '</b>', 1) AS marked \
             FROM article WHERE body @1@ 'Rust web'",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let statements = response["$fastdb"]["statements"].as_array().unwrap();
    assert_eq!(
        statements[3]["value"][0]["marked"],
        "<b>Rust</b> <b>web</b>"
    );
    assert_eq!(
        statements[3]["value"][0]["id"]["$fastdb"]["table"],
        "article"
    );
}
