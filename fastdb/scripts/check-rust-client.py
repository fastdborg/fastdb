#!/usr/bin/env python3
"""Build a private path-dependent consumer outside the Turso workspace (offline)."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def registry_packages(lock):
    return {
        (p["name"], p["version"], p["source"], p.get("checksum"))
        for p in tomllib.loads(lock)["package"]
        if "source" in p
    }


def main():
    baseline = (ROOT / "Cargo.lock").read_text()
    compiler = subprocess.check_output(["rustc", "-vV"], text=True)
    host = next(line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: "))
    environment = os.environ.copy()
    # Do not make a dependent application rely on the checkout's injected flags.
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS"):
        environment.pop(key, None)
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target" / "fastdb-rust-consumer")
    with tempfile.TemporaryDirectory(prefix="fastdb-rust-client-") as directory:
        consumer = Path(directory)
        (consumer / "src").mkdir()
        (consumer / "Cargo.toml").write_text(
            '[package]\nname = "fastdb-consumer-smoke"\nversion = "0.0.0"\n'
            'edition = "2021"\npublish = false\n\n[workspace]\n\n'
            '[dependencies]\nfastdb = { path = '
            + json.dumps(str(ROOT / "fastdb" / "frontend"))
            + ' }\n'
        )
        shutil.copyfile(ROOT / "Cargo.lock", consumer / "Cargo.lock")
        (consumer / "src" / "main.rs").write_text(r'''
use fastdb::{Database, Parameters, Record, Key, Value, IntegrityLimits, IntegrityReport, ProfiledQuery, ResultLimits, CancellationToken};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::env::args().nth(1).expect("database path");
    let id = Record { table: "docs".into(), key: Key::String("saved".into()) };
    let portable = Value::Array(vec![
        Value::Integer(i64::MAX), Value::Number(-0.0),
        Value::Binary(vec![0, 127, 255]), Value::Record(id.clone()),
        Value::String("quoted\" key\nไทย".into()),
    ]);
    let encoded = portable.clone().into_portable_json()?.parse()?;
    assert_eq!(portable, Value::from_portable_value(encoded)?);
    assert_eq!(Value::Number(f64::NAN).into_portable_json().unwrap_err().code(), "FDB_VALIDATION");
    {
        let db = Database::open(&file)?;
        let c = db.connect()?;
        c.execute("CREATE TABLE docs", &Parameters::new())?;
        c.execute("DEFINE FIELD value ON docs TYPE integer REQUIRED CHECK(value>0)", &Parameters::new())?;
        c.execute("CREATE UNIQUE INDEX docs_value ON docs(value)", &Parameters::new())?;
        c.execute("INSERT INTO docs (id,value) VALUES ($id,$value)", &Parameters::from([
            ("$id".into(),Value::Record(id.clone())), ("$value".into(),Value::Integer(i64::MAX)),
        ]))?;
        c.execute("BEGIN", &Parameters::new())?;
        c.execute("UPDATE docs SET value=7", &Parameters::new())?;
        let report=c.execute_report("UPDATE docs SET value=-1", &Parameters::new());
        assert_eq!(report.result.unwrap_err().code(), "FDB_VALIDATION");
        assert_eq!(report.transaction_after,fastdb::TransactionState::Active);
        let limit = c.check_collection_integrity("docs", IntegrityLimits { max_documents: 0, ..IntegrityLimits::default() }).unwrap_err();
        assert_eq!(limit.code(), "FDB_LIMIT");
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        c.execute("ROLLBACK", &Parameters::new())?;
        assert_eq!(c.lookup_index("docs","docs_value",&Value::Integer(i64::MAX))?.len(),1);
        assert!(c.lookup_index("docs","docs_value",&Value::Integer(7))?.is_empty());
        assert_eq!(c.execute("SELECT string::slugify('Hello Rust') AS slug",&Parameters::new())?.exactly_one()?,vec![Value::String("hello-rust".into())]);
        for vector in [Value::vector32_sparse_entries(3, &[(0,1.0),(2,-1.0)])?, Value::vector32(&[1.0,0.0,-1.0])?, Value::vector64(&[0.1,0.2,0.3])?, Value::vector32_sparse(&[1.0,0.0,-1.0])?, Value::vector8(&[1.0,0.0,-1.0])?, Value::vector1bit(&[1.0,0.0,-1.0])?] {
            assert_eq!(vector.vector_dimensions()?,3);
            assert_eq!(c.execute("SELECT $v AS embedding",&Parameters::from([("$v".into(),vector.clone())]))?.exactly_one()?,vec![vector]);
        }
    }
    let db=Database::open(&file)?;
    let c=db.connect()?;
    assert_eq!(c.get(&id)?.expect("persisted document")["value"],Value::Integer(i64::MAX));
    assert_eq!(c.lookup_index("docs","docs_value",&Value::Integer(i64::MAX))?.len(),1);
    let audit: IntegrityReport = c.check_collection_integrity("docs", IntegrityLimits::default())?;
    assert_eq!((audit.documents,audit.indexes,audit.index_entries),(1,1,1));
    assert!(audit.encoded_bytes>0);
    assert_eq!(c.check_collection_integrity("docs", IntegrityLimits { max_documents: 1, max_encoded_bytes: audit.encoded_bytes })?.encoded_bytes,audit.encoded_bytes);
    let params=Parameters::from([("$value".into(),Value::Integer(i64::MAX))]);
    let profile: ProfiledQuery=c.profile_select("SELECT id,value FROM docs WHERE value=$value", &params)?;
    assert_eq!(profile.result.exactly_one()?,vec![Value::Record(id),Value::Integer(i64::MAX)]);
    assert!(profile.metrics.vm_steps>0);
    // A unique lookup seeks directly and need not iterate an index.
    assert!(profile.metrics.btree_seeks>0);
    assert_eq!(profile.metrics.rows_written,0);
    assert_eq!(c.profile_select("SELECT id,value FROM docs WHERE value=$value", &params)?.metrics, profile.metrics);
    assert_eq!(c.profile_select("DELETE FROM docs", &Parameters::new()).unwrap_err().code(),"FDB_UNSUPPORTED");
    assert_eq!(c.check_collection_integrity("docs", IntegrityLimits::default())?.documents,1);
    let empty = Parameters::new();
    let one = ResultLimits { max_rows: 1, max_payload_bytes: 9 };
    assert_eq!(c.select_with_limits("SELECT value AS v FROM docs", &empty, one)?.rows, vec![vec![Value::Integer(i64::MAX)]]);
    assert_eq!(c.profile_select_with_limits("SELECT value AS v FROM docs", &empty, one)?.result.rows, vec![vec![Value::Integer(i64::MAX)]]);
    assert_eq!(c.select_with_limits("SELECT value AS v FROM docs", &empty, ResultLimits { max_payload_bytes:8, ..one }).unwrap_err().code(), "FDB_LIMIT");
    let fetch = ResultLimits { max_rows:1, max_payload_bytes:25 };
    assert_eq!(c.select_with_limits("SELECT record::fetch(docs:saved) AS v", &empty, fetch)?.rows.len(), 1);
    assert_eq!(c.select_with_limits("SELECT record::fetch(docs:saved) AS v", &empty, ResultLimits { max_payload_bytes:24, ..fetch }).unwrap_err().code(), "FDB_LIMIT");
    c.execute("BEGIN", &empty)?;
    c.execute("UPDATE docs SET value=7", &empty)?;
    let write = "UPDATE docs SET value=8 RETURNING value AS v";
    assert_eq!(c.write_with_result_limits(write, &empty, ResultLimits { max_rows:0, ..one }).unwrap_err().code(), "FDB_LIMIT");
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(c.select_with_limits("SELECT value AS v FROM docs", &empty, one)?.rows, vec![vec![Value::Integer(7)]]);
    assert_eq!(c.write_with_result_limits(write, &empty, one)?.rows, vec![vec![Value::Integer(8)]]);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(c.write_with_result_limits_cancellable("DELETE FROM docs", &empty, one, &cancelled).unwrap_err().code(), "FDB_CANCELLED");
    let expired = CancellationToken::with_deadline(std::time::Instant::now());
    assert_eq!(c.execute_cancellable("DELETE FROM docs", &empty, &expired).unwrap_err().code(), "FDB_CANCELLED");
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(c.execute("SELECT value FROM docs", &empty)?.rows, vec![vec![Value::Integer(8)]]);
    let fresh = CancellationToken::with_deadline(std::time::Instant::now() + std::time::Duration::from_secs(60));
    assert_eq!(c.write_with_result_limits_cancellable("UPDATE docs SET value=9 RETURNING value AS v", &empty, one, &fresh)?.rows, vec![vec![Value::Integer(9)]]);
    assert_eq!(c.select_with_limits_cancellable("SELECT value AS v FROM docs", &empty, one, &fresh)?.rows, vec![vec![Value::Integer(9)]]);
    assert_eq!(c.profile_select_with_limits_cancellable("SELECT value AS v FROM docs", &empty, one, &fresh)?.result.rows, vec![vec![Value::Integer(9)]]);
    c.execute("ROLLBACK", &empty)?;
    assert_eq!(c.lookup_index("docs", "docs_value", &Value::Integer(i64::MAX))?.len(), 1);
    assert_eq!(c.check_collection_integrity("docs", IntegrityLimits::default())?.documents, 1);
    c.execute("BEGIN", &empty)?;
    c.execute("INSERT INTO docs {id:docs:pending,value:2}", &empty)?;
    let c = c.with_write_buffer_limits(ResultLimits { max_rows:1, max_payload_bytes:1000 });
    assert_eq!(c.execute("DELETE FROM docs RETURNING value", &empty).unwrap_err().code(), "FDB_LIMIT");
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(c.execute("SELECT value FROM docs ORDER BY value", &empty)?.rows,
        vec![vec![Value::Integer(2)],vec![Value::Integer(i64::MAX)]]);
    let c = c.with_write_buffer_limits(ResultLimits { max_rows:2, max_payload_bytes:1000 });
    assert_eq!(c.execute("DELETE FROM docs RETURNING value", &empty)?.rows.len(),2);
    assert_eq!(c.check_collection_integrity("docs", IntegrityLimits::default())?.documents,0);
    c.execute("ROLLBACK", &empty)?;
    assert_eq!(c.lookup_index("docs", "docs_value", &Value::Integer(i64::MAX))?.len(),1);
    assert_eq!(c.check_collection_integrity("docs", IntegrityLimits::default())?.documents,1);
    println!("Standalone Rust client smoke passed: typed values, portable JSON, validation, indexes, rollback, QuickJS, vectors, profiles, audits, result/write buffer limits, cancellation/deadlines and reopen");
    Ok(())
}
''')
        # Let Cargo add the consumer and prune unused workspace packages while
        # retaining the seeded versions. Reject any registry/git dependency drift.
        subprocess.run(
            ["cargo", "metadata", "--offline", "--format-version", "1", "--filter-platform", host],
            cwd=consumer, env=environment, stdout=subprocess.DEVNULL, check=True,
        )
        resolved = registry_packages((consumer / "Cargo.lock").read_text())
        drift = resolved - registry_packages(baseline)
        if drift:
            raise RuntimeError(f"Consumer resolved packages outside the pinned lockfile: {sorted(drift)}")
        subprocess.run(
            ["cargo", "run", "--offline", "--locked", "--", str(consumer / "database.db")],
            cwd=consumer, env=environment, check=True,
        )
        print(f"Verified {len(resolved)} registry/git package identities against the workspace lockfile")


if __name__ == "__main__":
    main()
