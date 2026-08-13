//! Mechanical checks for the locked SurrealDB v3.1.5 capability inventory.

#![forbid(unsafe_code)]
#![deny(warnings)]

use std::path::{Path, PathBuf};
use turso_fastdb_compat::{Inventory, Status};

#[test]
fn p12_compat_002_supported_inventory_evidence_names_executable_tests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let inventory = Inventory::from_path(&root.join("compat/surrealdb-v3.1.5.toml")).unwrap();
    let source = collect_sources(root);

    assert!(
        inventory.capability.len() >= 700,
        "atomic inventory unexpectedly lost coverage"
    );
    for capability in &inventory.capability {
        if capability.status != Status::Supported {
            continue;
        }
        assert!(
            !capability.parser_evidence.is_empty(),
            "supported query capability {} lacks parser evidence",
            capability.id
        );
        for evidence in capability
            .parser_evidence
            .iter()
            .chain(&capability.execution_evidence)
        {
            let function = evidence.to_ascii_lowercase().replace('-', "_");
            assert!(
                source.contains(&format!("fn {function}_")),
                "unknown executable evidence {evidence} in {}",
                capability.id
            );
        }
    }
}

fn collect_sources(root: &Path) -> String {
    [
        "fastdb-parser/tests",
        "fastdb-frontend/src",
        "fastdb-api/tests",
        "fastdb-cli/tests",
        "fastdb-tests/tests",
    ]
    .into_iter()
    .flat_map(|directory| rust_files(&root.join(directory)))
    .map(|path| std::fs::read_to_string(path).unwrap())
    .collect::<Vec<_>>()
    .join("\n")
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
