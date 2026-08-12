#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{catalog::CatalogState, Database, ErrorCategory, Failpoint};

#[test]
fn p2_cat_001_empty_open_is_read_only_and_bootstrap_shape_is_exact() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("catalog.fastdb");
    let path = path.to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        assert!(matches!(conn.catalog_state().unwrap(), CatalogState::Empty));
        assert!(common::native_rows(conn.native(), "SELECT name FROM sqlite_schema").is_empty());
    }
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy SET name='Tracy'")
            .unwrap();
        let metadata = common::native_rows(
            conn.native(),
            "SELECT singleton,format_version,dialect_version,database_id,creation_version,last_migration FROM __fastdb_meta",
        );
        assert_eq!(metadata.len(), 1);
        assert_eq!(&metadata[0][..3], ["1", "1", "1"]);
        assert_eq!(metadata[0][3].len(), 32);
        assert_eq!(metadata[0][5], "1");
        let catalogs = common::native_rows(
            conn.native(),
            "SELECT name,sql FROM sqlite_schema WHERE type='table' AND name LIKE '__fastdb_%' ORDER BY name",
        );
        assert_eq!(catalogs.len(), 5, "four catalogs plus one physical table");
        assert!(catalogs.iter().all(|row| row[1].ends_with(" STRICT")));
    }
    Database::open(path).unwrap();
}

#[test]
fn p2_cat_002_refuses_nonfastdb_phase0_future_and_unknown_migration_on_open() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        ("phase0", "UPDATE __fastdb_meta SET format_version=0"),
        (
            "future_format",
            "UPDATE __fastdb_meta SET format_version=99",
        ),
        (
            "future_dialect",
            "UPDATE __fastdb_meta SET dialect_version=99",
        ),
        (
            "future_migration",
            "UPDATE __fastdb_meta SET last_migration=99",
        ),
    ] {
        let file = directory.path().join(format!("{name}.fastdb"));
        let path = file.to_str().unwrap();
        {
            let db = Database::open(path).unwrap();
            let conn = db.connect().unwrap();
            conn.execute("CREATE person:tracy SET name='Tracy'")
                .unwrap();
            common::native_exec(conn.native(), mutation);
        }
        assert_eq!(
            Database::open(path).unwrap_err().category(),
            ErrorCategory::Format,
            "{name}"
        );
    }

    let file = directory.path().join("foreign.fastdb");
    let path = file.to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        common::native_exec(conn.native(), "CREATE TABLE ordinary(x INTEGER)");
    }
    assert_eq!(
        Database::open(path).unwrap_err().category(),
        ErrorCategory::Format
    );
}

#[test]
fn p2_cat_003_migration_is_atomic_idempotent_and_cache_publishes_after_commit() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("migration.fastdb");
    let path = file.to_str().unwrap();
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE person:tracy SET name='Tracy'")
        .unwrap();
    common::native_exec(conn.native(), "UPDATE __fastdb_meta SET last_migration=0");
    let before = conn.catalog_state().unwrap();
    conn.arm_failpoint(Failpoint::AfterMigration);
    assert_eq!(
        conn.reload_catalog().unwrap_err().category(),
        ErrorCategory::Transaction
    );
    assert_eq!(
        common::native_rows(conn.native(), "SELECT last_migration FROM __fastdb_meta")[0][0],
        "0"
    );
    assert_eq!(conn.catalog_state().unwrap(), before);
    conn.disarm_all_failpoints();
    conn.reload_catalog().unwrap();
    assert_eq!(
        common::native_rows(conn.native(), "SELECT last_migration FROM __fastdb_meta")[0][0],
        "1"
    );
    conn.reload_catalog().unwrap();
    assert_eq!(
        conn.catalog_state()
            .unwrap()
            .snapshot()
            .unwrap()
            .metadata
            .last_migration,
        1
    );
}

#[test]
fn p2_cat_004_refuses_malformed_missing_mismatched_and_orphan_objects() {
    let directory = tempdir().unwrap();
    for (name, mutation) in [
        (
            "malformed",
            "UPDATE __fastdb_tables SET table_id='not-a-catalog-id'",
        ),
        (
            "orphan",
            "CREATE TABLE __fastdb_t_00000000000000000000000000000000(rid TEXT)",
        ),
    ] {
        let file = directory.path().join(format!("{name}.fastdb"));
        let path = file.to_str().unwrap();
        {
            let db = Database::open(path).unwrap();
            let conn = db.connect().unwrap();
            conn.execute("CREATE person:tracy SET name='Tracy'")
                .unwrap();
            common::native_exec(conn.native(), mutation);
        }
        assert_eq!(
            Database::open(path).unwrap_err().category(),
            ErrorCategory::Format,
            "{name}"
        );
    }

    for (name, suffix) in [("missing", "DROP TABLE"), ("mismatch", "ALTER TABLE")] {
        let file = directory.path().join(format!("{name}.fastdb"));
        let path = file.to_str().unwrap();
        {
            let db = Database::open(path).unwrap();
            let conn = db.connect().unwrap();
            conn.execute("CREATE person:tracy SET name='Tracy'")
                .unwrap();
            let physical = common::physical_name_for(conn.native(), "person").unwrap();
            let sql = if suffix == "DROP TABLE" {
                format!("DROP TABLE {physical}")
            } else {
                format!("ALTER TABLE {physical} ADD COLUMN extra TEXT")
            };
            common::native_exec(conn.native(), &sql);
        }
        assert_eq!(
            Database::open(path).unwrap_err().category(),
            ErrorCategory::Format,
            "{name}"
        );
    }
}

#[test]
fn p2_cat_005_database_clones_share_committed_snapshot() {
    let db = Database::open_memory().unwrap();
    let other = db.clone();
    let first = db.connect().unwrap();
    let second = other.connect().unwrap();
    first.execute("DEFINE TABLE person SCHEMALESS").unwrap();
    assert!(second
        .catalog_state()
        .unwrap()
        .snapshot()
        .unwrap()
        .tables
        .contains_key("person"));
}

#[test]
fn p2_cat_006_malformed_stored_rid_and_value_tags_are_format_corruption() {
    for (name, mutation) in [
        ("rid", "UPDATE {table} SET rid='v1:i:01'"),
        (
            "tag",
            r#"UPDATE {table} SET doc=jsonb('{"$fastdb":{"v":1,"t":"future"}}')"#,
        ),
    ] {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy SET name='Tracy'")
            .unwrap();
        let physical = common::physical_name_for(conn.native(), "person").unwrap();
        common::native_exec(conn.native(), &mutation.replace("{table}", &physical));
        assert_eq!(
            conn.execute("SELECT * FROM person").unwrap_err().category(),
            ErrorCategory::Format,
            "{name}"
        );
    }
}
