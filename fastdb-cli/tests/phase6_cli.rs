#![forbid(unsafe_code)]
#![deny(warnings)]

use std::process::Command;

#[test]
fn p6_cli_001_projection_explain_rebuild_and_remove_use_the_json_envelope() {
    let output = Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args([
            "--memory",
            "--output",
            "json",
            "-c",
            "CREATE person:one SET score=2 RETURN NONE; \
             DEFINE INDEX by_score ON person FIELDS score; \
             SELECT score + 1 AS next_score FROM person:one; \
             EXPLAIN SELECT * FROM person WHERE score=2; \
             REBUILD INDEX by_score ON person; \
             REMOVE INDEX by_score ON TABLE person",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["$fastdb"]["t"], "response");
    let statements = response["$fastdb"]["statements"].as_array().unwrap();
    assert_eq!(statements.len(), 6);
    assert_eq!(statements[2]["kind"], "rows");
    assert_eq!(statements[2]["value"][0]["next_score"], 3);
    assert_eq!(statements[3]["kind"], "rows");
    let plan = statements[3]["value"].as_array().unwrap();
    assert!(plan.iter().any(|row| {
        row["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("__fastdb_i_"))
            && row["ordinal"].is_number()
    }));
    assert_eq!(statements[4]["kind"], "none");
    assert_eq!(statements[5]["kind"], "none");
}

#[test]
fn p6_cli_002_unavailable_provider_is_a_structured_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args([
            "--memory",
            "--output",
            "json",
            "-c",
            "DEFINE INDEX body ON article FIELDS body USING fts WITH (tokenizer='simple')",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["$fastdb"]["t"], "error");
    assert_eq!(error["$fastdb"]["category"], "UnsupportedSyntax");
    assert!(error["$fastdb"]["span"]["offset"].is_number());
}
