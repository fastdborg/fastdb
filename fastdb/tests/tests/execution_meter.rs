use std::sync::Arc;
use turso_core::{execution_meter::ExecutionMeter, Connection, Database, LimboError, StepResult};

// Reuse the existing deterministic engine I/O harness without changing it.
#[allow(dead_code)]
#[path = "../../../tests/integration/queued_io.rs"]
mod queued_io;

// The upstream run_ignore_rows helper maps interruption to Busy. Use the
// callback helper, which preserves the StepResult::Interrupt distinction.
fn run(c: &Arc<Connection>, sql: &str) -> turso_core::Result<()> {
    c.prepare(sql)?.run_with_row_callback(|_| Ok(()))
}

fn connection(path: &str) -> Arc<Connection> {
    let db = Database::open_file(Database::io_for_path(path).unwrap(), path).unwrap();
    let c = db.connect().unwrap();
    c.execute("CREATE TABLE input(n INTEGER)").unwrap();
    c.execute("INSERT INTO input VALUES (1),(2),(3),(4),(5)")
        .unwrap();
    c
}

fn exercise(path: &str) {
    let c = connection(path);
    let meter = Arc::new(ExecutionMeter::default());
    let mut query = c.prepare("SELECT n FROM input").unwrap();
    c.set_execution_meter(Some(meter.clone())).unwrap();
    query.run_ignore_rows().unwrap();
    assert_eq!(meter.snapshot().rows_read, 5);
    assert_eq!(meter.snapshot().vm_steps, query.metrics().vm_steps);
    let successful = meter.snapshot();
    // Partial reads remain available when expression evaluation fails.
    let mut failing = c
        .prepare("SELECT abs(CASE WHEN n=3 THEN -9223372036854775808 ELSE n END) FROM input")
        .unwrap();
    assert!(failing.run_ignore_rows().is_err());
    let failed = meter.snapshot();
    assert_eq!(failed.rows_read - successful.rows_read, 3);
    assert!(failed.vm_steps > successful.vm_steps);
    drop(failing);
    assert_eq!(meter.snapshot(), failed);
    c.set_execution_meter(None).unwrap();
    query.reset().unwrap();
    query.run_ignore_rows().unwrap();
    assert_eq!(meter.snapshot(), failed);

    // A reusable statement binds the new scope after reset, not at prepare time.
    let second = Arc::new(ExecutionMeter::default());
    c.set_execution_meter(Some(second.clone())).unwrap();
    query.reset().unwrap();
    query.run_ignore_rows().unwrap();
    assert_eq!(second.snapshot().rows_read, 5);
    assert_eq!(meter.snapshot(), failed);
    c.set_execution_meter(None).unwrap();
    drop(query);
    drop(c);
    assert_eq!(meter.snapshot(), failed);
}

#[test]
fn retained_metrics_cover_success_failure_reset_and_drop() {
    exercise(":memory:");
    let dir = tempfile::tempdir().unwrap();
    exercise(dir.path().join("meter.db").to_str().unwrap());
}

#[test]
fn active_statement_prevents_rebinding_and_drop_keeps_partial_reads() {
    let c = connection(":memory:");
    let meter = Arc::new(ExecutionMeter::default());
    c.set_execution_meter(Some(meter.clone())).unwrap();
    let mut query = c.prepare("SELECT n FROM input").unwrap();
    loop {
        match query.step().unwrap() {
            StepResult::IO => query._io().step().unwrap(),
            StepResult::Yield => continue,
            StepResult::Row => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(matches!(
        c.set_execution_meter(None),
        Err(LimboError::StatementsInProgress(_))
    ));
    assert_eq!(meter.snapshot().rows_read, 1);
    drop(query);
    c.set_execution_meter(None).unwrap();
    assert_eq!(meter.snapshot().rows_read, 1);
    c.execute("SELECT n FROM input").unwrap();
    assert_eq!(meter.snapshot().rows_read, 1);
}

#[test]
fn vm_budget_is_exact_shared_across_statements_and_interrupts_writes() {
    let dir = tempfile::tempdir().unwrap();
    for path in [
        ":memory:".to_string(),
        dir.path().join("budget.db").to_str().unwrap().to_string(),
    ] {
        let c = connection(&path);
        c.execute("CREATE TABLE output(n INTEGER)").unwrap();
        let meter = Arc::new(ExecutionMeter::new(Some(100)));
        c.set_execution_meter(Some(meter.clone())).unwrap();
        c.execute("SELECT n FROM input").unwrap();
        let before = meter.snapshot();
        assert!(before.vm_steps > 0 && before.vm_steps < 100);
        let mut write = c
            .prepare("INSERT INTO output SELECT a.n FROM input a,input b,input d")
            .unwrap();
        assert!(matches!(
            write.run_with_row_callback(|_| Ok(())),
            Err(LimboError::Interrupt)
        ));
        assert_eq!(meter.snapshot().vm_steps, 100);
        assert!(meter.snapshot().vm_budget_exhausted);
        assert!(meter.snapshot().rows_read > before.rows_read);
        assert!(
            meter.snapshot().rows_written > 0,
            "exercise rollback after actual writes"
        );
        let final_counts = meter.snapshot();
        assert!(matches!(run(&c, "SELECT 1"), Err(LimboError::Interrupt)));
        assert_eq!(meter.snapshot(), final_counts);
        drop(write);
        c.set_execution_meter(None).unwrap();
        let mut rows = 0;
        c.prepare("SELECT n FROM output")
            .unwrap()
            .run_with_row_callback(|_| {
                rows += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(rows, 0, "interrupted writes must roll back");
        c.execute("INSERT INTO output VALUES(9)").unwrap();
        assert_eq!(meter.snapshot(), final_counts);
        let zero = Arc::new(ExecutionMeter::new(Some(0)));
        c.set_execution_meter(Some(zero.clone())).unwrap();
        assert!(matches!(
            run(&c, "SELECT n FROM input"),
            Err(LimboError::Interrupt)
        ));
        assert_eq!(zero.snapshot().vm_steps, 0);
        assert_eq!(zero.snapshot().rows_read, 0);
        assert!(zero.snapshot().vm_budget_exhausted);
    }
}

#[test]
fn physical_write_counts_survive_explicit_rollback() {
    let c = connection(":memory:");
    let meter = Arc::new(ExecutionMeter::default());
    c.set_execution_meter(Some(meter.clone())).unwrap();
    c.execute("BEGIN").unwrap();
    c.execute("INSERT INTO input VALUES(6),(7)").unwrap();
    let written = meter.snapshot();
    assert!(written.rows_written >= 2);
    c.execute("ROLLBACK").unwrap();
    assert_eq!(meter.snapshot().rows_written, written.rows_written);
    c.set_execution_meter(None).unwrap();
    let mut rows = 0;
    c.prepare("SELECT n FROM input")
        .unwrap()
        .run_with_row_callback(|_| {
            rows += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(rows, 5);
}

#[test]
fn explain_modes_obey_zero_budget() {
    let c = connection(":memory:");
    for sql in [
        "EXPLAIN SELECT n FROM input",
        "EXPLAIN QUERY PLAN SELECT n FROM input",
    ] {
        let meter = Arc::new(ExecutionMeter::new(Some(0)));
        c.set_execution_meter(Some(meter.clone())).unwrap();
        assert!(matches!(run(&c, sql), Err(LimboError::Interrupt)));
        assert_eq!(meter.snapshot().vm_steps, 0);
        assert!(meter.snapshot().vm_budget_exhausted);
        c.set_execution_meter(None).unwrap();
    }
}

#[test]
fn exact_completed_statement_budget_succeeds_and_one_less_interrupts() {
    let c = connection(":memory:");
    let mut statement = c.prepare("SELECT n FROM input").unwrap();
    let baseline = Arc::new(ExecutionMeter::default());
    c.set_execution_meter(Some(baseline.clone())).unwrap();
    statement.run_with_row_callback(|_| Ok(())).unwrap();
    let required = baseline.snapshot().vm_steps;
    assert!(required > 0);
    for (limit, succeeds) in [(required, true), (required - 1, false)] {
        let meter = Arc::new(ExecutionMeter::new(Some(limit)));
        c.set_execution_meter(Some(meter.clone())).unwrap();
        statement.reset().unwrap();
        let result = statement.run_with_row_callback(|_| Ok(()));
        assert_eq!(result.is_ok(), succeeds);
        if !succeeds {
            assert!(matches!(result, Err(LimboError::Interrupt)));
        }
        assert_eq!(meter.snapshot().vm_steps, limit);
        assert_eq!(meter.snapshot().vm_budget_exhausted, !succeeds);
    }
}

#[test]
fn pending_io_polls_do_not_charge_additional_work() {
    let io = Arc::new(queued_io::QueuedIo::new());
    {
        let db = Database::open_file(io.clone(), "meter-queued.db").unwrap();
        let c = db.connect().unwrap();
        c.execute("CREATE TABLE input(n INTEGER PRIMARY KEY, payload BLOB)")
            .unwrap();
        c.execute("BEGIN").unwrap();
        for n in 0..100 {
            c.execute(format!("INSERT INTO input VALUES({n},zeroblob(1000))"))
                .unwrap();
        }
        c.execute("COMMIT").unwrap();
        c.execute("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    }
    let db = Database::open_file(io.clone(), "meter-queued.db").unwrap();
    let c = db.connect().unwrap();
    let mut statement = c.prepare("SELECT n FROM input").unwrap();
    let meter = Arc::new(ExecutionMeter::default());
    c.set_execution_meter(Some(meter.clone())).unwrap();
    let mut rows = 0;
    let mut suspended = 0;
    loop {
        match statement.step().unwrap() {
            StepResult::Row => rows += 1,
            StepResult::Done => break,
            StepResult::Yield => continue,
            StepResult::IO => {
                suspended += 1;
                let before = meter.snapshot();
                for _ in 0..3 {
                    assert!(matches!(statement.step().unwrap(), StepResult::IO));
                    assert_eq!(meter.snapshot(), before);
                }
                assert!(io.step_one().unwrap().is_some());
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(suspended > 0);
    assert_eq!(rows, 100);
    assert_eq!(meter.snapshot().rows_read, 100);
    assert_eq!(meter.snapshot().vm_steps, statement.metrics().vm_steps);
}
