#![forbid(unsafe_code)]
#![deny(warnings)]

use std::process::Command;

#[test]
fn p5_cli_001_parameters_and_json_envelopes_cannot_inject_source() {
    let payload = "x'; DELETE item; --";
    let output = Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args([
            "--memory",
            "--output",
            "json",
            "--param",
            &format!("payload={}", serde_json::to_string(payload).unwrap()),
            "--param",
            "nested={\"$fastdb\":\"user data\"}",
            "-c",
            "CREATE item:one CONTENT { text:$payload, nested:$nested }; SELECT * FROM item:one",
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
    assert_eq!(statements.len(), 2);
    assert_eq!(statements[1]["value"][0]["text"], payload);
    assert_eq!(
        statements[1]["value"][0]["nested"]["$fastdb"]["t"],
        "object"
    );
}
