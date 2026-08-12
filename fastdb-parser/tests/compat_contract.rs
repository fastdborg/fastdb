//! Mechanical checks for the normative Phase 1 compatibility matrix.

#![forbid(unsafe_code)]
#![deny(warnings)]

use std::{collections::BTreeSet, fs, path::Path};

#[test]
fn p1_compat_001_every_feature_row_has_evidence_and_honest_status() {
    let matrix_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../COMPAT.md");
    let matrix = fs::read_to_string(matrix_path).unwrap();
    let parser_tests =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/phase1.rs")).unwrap();
    let executable_partial: BTreeSet<&str> = [
        "VAL-STR",
        "RID-BARE",
        "OP-EQ",
        "STMT-CREATE",
        "STMT-SELECT",
        "STMT-DELETE",
        "CLAUSE-CONTENT-SET",
        "CLAUSE-WHERE",
    ]
    .into_iter()
    .collect();

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
            columns[4].contains("P1-"),
            "missing Phase 1 parser test in {feature}"
        );
        for test_id in columns[4].split('`').filter(|part| part.starts_with("P1-")) {
            let function = test_id.to_ascii_lowercase().replace('-', "_");
            assert!(
                parser_tests.contains(&format!("fn {function}_")),
                "unknown parser test {test_id} in {feature}"
            );
        }
        assert!(
            columns[5].contains("docs/compat-research/phase1.md#"),
            "missing Phase 1 provenance in {feature}"
        );
        assert!(
            !columns[6].is_empty(),
            "missing conformance disposition in {feature}"
        );

        if status == "Partial" {
            assert!(
                executable_partial.contains(feature),
                "parser-only feature mislabeled Partial: {feature}"
            );
            assert!(
                columns[6].contains("P1-BRIDGE-001") || columns[6].contains("quoted_semicolon"),
                "Partial row lacks executable evidence: {feature}"
            );
        }
        assert_ne!(
            status, "Supported",
            "Phase 1 has no fully supported language row"
        );
    }

    assert!(row_count >= 40, "feature matrix unexpectedly lost coverage");
}
