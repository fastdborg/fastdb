//! Mechanical checks for the normative compatibility matrix through Phase 9.

#![forbid(unsafe_code)]
#![deny(warnings)]

use std::{fs, path::Path};

#[test]
fn p3_compat_001_every_feature_row_has_evidence_and_honest_status() {
    let matrix_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../COMPAT.md");
    let matrix = fs::read_to_string(matrix_path).unwrap();
    let parser_tests = [
        "tests/phase1.rs",
        "tests/phase3.rs",
        "tests/phase6.rs",
        "tests/phase7.rs",
        "tests/phase8.rs",
        "tests/phase9.rs",
    ]
    .into_iter()
    .map(|path| fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
    let integration_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fastdb-tests/tests");
    let integration_tests = fs::read_dir(integration_dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            (path.extension().and_then(|value| value.to_str()) == Some("rs")).then_some(path)
        })
        .map(|path| fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fastdb-frontend/src/lib.rs"),
        )
        .unwrap()
        + &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fastdb-api/tests/phase7.rs"),
        )
        .unwrap()
        + &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fastdb-api/tests/phase8.rs"),
        )
        .unwrap()
        + &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fastdb-api/tests/phase9.rs"),
        )
        .unwrap()
        + &fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../fastdb-cli/tests"))
            .unwrap()
            .filter_map(|entry| {
                let path = entry.unwrap().path();
                (path.extension().and_then(|value| value.to_str()) == Some("rs")).then_some(path)
            })
            .map(|path| fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>()
            .join("\n");

    let mut row_count = 0;
    for line in matrix.lines().filter(|line| line.starts_with("| `")) {
        row_count += 1;
        let normalized = line.replace("\\|", "or");
        let columns: Vec<_> = normalized.split('|').map(str::trim).collect();
        assert_eq!(columns.len(), 8, "malformed feature row: {line}");

        let feature = columns[1].trim_matches('`');
        let status = columns[2];
        assert!(
            matches!(status, "Supported" | "Partial" | "Planned" | "Unsupported"),
            "invalid status in {feature}: {status}"
        );
        assert!(
            columns[4].contains("P1-")
                || columns[4].contains("P2-UUID-")
                || columns[4].contains("P6-AST-")
                || columns[4].contains("P7-AST-")
                || columns[4].contains("P8-PARSE-")
                || columns[4].contains("P9-PARSE-"),
            "missing parser test in {feature}"
        );
        for test_id in columns[4].split('`').filter(|part| {
            part.starts_with("P1-")
                || part.starts_with("P2-UUID-")
                || part.starts_with("P6-AST-")
                || part.starts_with("P7-AST-")
                || part.starts_with("P8-PARSE-")
                || part.starts_with("P9-PARSE-")
        }) {
            let function = test_id.to_ascii_lowercase().replace('-', "_");
            assert!(
                parser_tests.contains(&format!("fn {function}_")),
                "unknown parser test {test_id} in {feature}"
            );
        }
        assert!(
            columns[5].contains("docs/compat-research/phase1.md#")
                || columns[5].contains("docs/compat-research/phase2.md#")
                || columns[5].contains("docs/compat-research/phase3.md#")
                || columns[5].contains("docs/compat-research/phase6.md#")
                || columns[5].contains("docs/compat-research/phase7.md")
                || columns[5].contains("docs/compat-research/phase8.md")
                || columns[5].contains("docs/compat-research/phase9.md"),
            "missing clean-room provenance in {feature}"
        );
        assert!(
            !columns[6].is_empty(),
            "missing conformance disposition in {feature}"
        );

        if matches!(status, "Partial" | "Supported") {
            assert!(
                columns[6].contains("P2-")
                    || columns[6].contains("P3-")
                    || columns[6].contains("P6-")
                    || columns[6].contains("P7-")
                    || columns[6].contains("P8-")
                    || columns[6].contains("P9-"),
                "executable row lacks execution evidence: {feature}"
            );
            for test_id in columns[6].split('`').filter(|part| {
                part.starts_with("P2-")
                    || part.starts_with("P3-")
                    || part.starts_with("P6-")
                    || part.starts_with("P7-")
                    || part.starts_with("P8-")
                    || part.starts_with("P9-")
            }) {
                let function = test_id.to_ascii_lowercase().replace('-', "_");
                assert!(
                    integration_tests.contains(&format!("fn {function}_"))
                        || parser_tests.contains(&format!("fn {function}_")),
                    "unknown execution test {test_id} in {feature}"
                );
            }
        }
    }

    assert!(row_count >= 40, "feature matrix unexpectedly lost coverage");
}
