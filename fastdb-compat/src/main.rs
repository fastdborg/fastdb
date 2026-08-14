//! Repository command for checking or rendering the compatibility matrix.

#![forbid(unsafe_code)]
#![deny(warnings)]

use std::path::Path;
use turso_fastdb_compat::Inventory;

fn main() {
    if let Err(error) = run() {
        eprintln!("compat-sync: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let operation = std::env::args().nth(1).unwrap_or_else(|| "--check".into());
    if std::env::args().nth(2).is_some() {
        return Err("usage: compat-sync [--check|--write|--lock]".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "compat crate has no workspace parent".to_string())?;
    let inventory_path = root.join("compat/surrealdb-v3.1.5.toml");
    let ids_path = root.join("compat/surrealdb-v3.1.5.ids");
    let matrix_path = root.join("COMPAT.md");
    let inventory = Inventory::from_path(&inventory_path)?;
    let rendered = inventory.render_markdown();

    match operation.as_str() {
        "--check" => {
            let ids = read(&ids_path)?;
            inventory.validate_locked_ids(&ids)?;
            require_equal(&matrix_path, &rendered)?;
        }
        "--write" => {
            let ids = read(&ids_path)?;
            inventory.validate_locked_ids(&ids)?;
            write(&matrix_path, &rendered)?;
        }
        "--lock" => {
            write(&ids_path, &inventory.locked_ids())?;
            write(&matrix_path, &rendered)?;
        }
        _ => return Err("usage: compat-sync [--check|--write|--lock]".into()),
    }
    Ok(())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    std::fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn require_equal(path: &Path, expected: &str) -> Result<(), String> {
    let actual = read(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{} is stale; run `cargo run -p turso_fastdb_compat -- --write`",
            path.display()
        ))
    }
}
