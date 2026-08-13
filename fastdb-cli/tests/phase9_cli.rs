#![forbid(unsafe_code)]
#![deny(warnings)]

use std::process::Command;

#[test]
fn p9_cli_001_vector_arrays_and_distance_use_json_envelope() {
    let output = Command::new(env!("CARGO_BIN_EXE_fastdb"))
        .args([
            "--memory",
            "--output",
            "json",
            "-c",
            "CREATE point:a SET embedding = [1,0]; \
             DEFINE FIELD embedding ON point TYPE array<float, 2>; \
             SELECT id, embedding, vector::distance::knn() AS distance FROM point \
             WHERE embedding <|1,EUCLIDEAN|> [1,0]",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let row = &response["$fastdb"]["statements"][2]["value"][0];
    assert_eq!(row["embedding"], serde_json::json!([1.0, 0.0]));
    assert_eq!(row["distance"], 0.0);
    assert_eq!(row["id"]["$fastdb"]["table"], "point");
}
