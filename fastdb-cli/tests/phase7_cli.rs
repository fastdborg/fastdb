#![forbid(unsafe_code)]
#![deny(warnings)]

use std::process::Command;

#[test]
fn p7_cli_001_relation_and_traversal_use_typed_json_envelopes() {
    let output = Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args([
            "--memory",
            "--output",
            "json",
            "-c",
            "CREATE person:one CONTENT {}; CREATE post:one CONTENT {}; \
             RELATE ONLY person:one->likes->post:one SET weight=2; \
             SELECT ->likes->post AS ids FROM person:one",
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
    let edge = &statements[2]["value"];
    assert_eq!(edge["in"]["$fastdb"]["t"], "rid");
    assert_eq!(edge["out"]["$fastdb"]["table"], "post");
    let endpoint = &statements[3]["value"][0]["ids"][0];
    assert_eq!(endpoint["$fastdb"]["t"], "rid");
    assert_eq!(endpoint["$fastdb"]["table"], "post");
}
