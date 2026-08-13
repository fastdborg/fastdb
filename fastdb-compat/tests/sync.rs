//! Repository-level compatibility inventory checks.

#![forbid(unsafe_code)]
#![deny(warnings)]

use std::path::Path;
use turso_fastdb_compat::{Inventory, Status};

#[test]
fn p12_compat_001_locked_inventory_and_matrix_are_synchronized() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let inventory = Inventory::from_path(&root.join("compat/surrealdb-v3.1.5.toml")).unwrap();
    let ids = std::fs::read_to_string(root.join("compat/surrealdb-v3.1.5.ids")).unwrap();
    inventory.validate_locked_ids(&ids).unwrap();
    let matrix = std::fs::read_to_string(root.join("COMPAT.md")).unwrap();
    assert_eq!(matrix, inventory.render_markdown());
}

#[test]
fn p12_compat_003_locked_ids_reject_deletion_and_replacement() {
    let inventory = load_inventory();
    let locked = inventory.locked_ids();

    let mut deleted = inventory.clone();
    deleted.capability.remove(0);
    assert!(deleted.validate_locked_ids(&locked).is_err());

    let mut replaced = inventory;
    replaced.capability[0].id.push_str("-REPLACED");
    assert!(replaced.validate_locked_ids(&locked).is_err());
}

#[test]
fn p12_compat_004_status_and_evidence_rules_fail_closed() {
    let inventory = load_inventory();

    let mut duplicate = inventory.clone();
    duplicate.capability[1].id = duplicate.capability[0].id.clone();
    assert!(duplicate.validate().unwrap_err().contains("duplicate"));

    let mut future_partial = inventory.clone();
    let active_phase = future_partial.inventory.active_phase;
    let capability = future_partial
        .capability
        .iter_mut()
        .find(|capability| capability.phase > active_phase)
        .unwrap();
    capability.status = Status::Partial;
    assert!(future_partial
        .validate()
        .unwrap_err()
        .contains("outside the active phase"));

    let mut unsupported_with_evidence = inventory.clone();
    let capability = unsupported_with_evidence
        .capability
        .iter_mut()
        .find(|capability| capability.status == Status::Unsupported)
        .unwrap();
    capability.execution_evidence.push("P12-INVALID-001".into());
    assert!(unsupported_with_evidence
        .validate()
        .unwrap_err()
        .contains("Unsupported but has execution evidence"));

    let mut supported_without_evidence = inventory;
    let capability = supported_without_evidence
        .capability
        .iter_mut()
        .find(|capability| capability.status == Status::Supported)
        .unwrap();
    capability.execution_evidence.clear();
    assert!(supported_without_evidence
        .validate()
        .unwrap_err()
        .contains("Supported without execution evidence"));
}

fn load_inventory() -> Inventory {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    Inventory::from_path(&root.join("compat/surrealdb-v3.1.5.toml")).unwrap()
}
