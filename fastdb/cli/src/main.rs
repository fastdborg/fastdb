use fastdb::{Database, ExecutionReport, Parameters, TransferFormat};
use std::io::{self, BufRead, Read, Write};
fn output(
    writer: &mut impl Write,
    report: ExecutionReport,
    offset: Option<usize>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let failed = report.result.is_err();
    let mut output = match report.result {
        Ok(result) => serde_json::to_value(result)?,
        Err(error) => {
            serde_json::json!({"error": {"code": error.code(), "message": error.to_string()}})
        }
    };
    output["transaction"] =
        serde_json::json!({"before": report.transaction_before, "after": report.transaction_after});
    if let Some(offset) = offset {
        output["offset"] = offset.into();
    }
    writeln!(writer, "{}", serde_json::to_string(&output)?)?;
    writer.flush()?;
    Ok(failed)
}
fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let mut path = None;
    let mut line_mode = false;
    let mut transfer = None;
    let mut migrations = None;
    let mut format = TransferFormat::Json;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--migrate" => {
                if migrations.is_some() {
                    return Err("choose one migration directory".into());
                }
                migrations = Some(args.next().ok_or("expected migration directory")?);
            }
            "--line" => line_mode = true,
            "--ndjson" => format = TransferFormat::Ndjson,
            "--import" | "--export" => {
                if transfer.is_some() {
                    return Err("choose one import/export operation".into());
                }
                transfer = Some((
                    arg == "--import",
                    args.next().ok_or("expected collection name")?,
                ));
            }
            "--help" | "-h" => {
                println!("Usage: fastdb-cli [--line] [DATABASE]\n       fastdb-cli --migrate DIRECTORY [DATABASE]\n       fastdb-cli (--import COLLECTION | --export COLLECTION) [--ndjson] [DATABASE]\nReads a semicolon-delimited script from stdin; stops on the first error.\n--line retains one-statement-per-line execution and continues after errors.");
                return Ok(std::process::ExitCode::SUCCESS);
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}").into()),
            _ if path.is_none() => path = Some(arg),
            _ => return Err("expected one database path".into()),
        }
    }
    if migrations.is_some()
        && (line_mode || transfer.is_some() || matches!(format, TransferFormat::Ndjson))
    {
        return Err("--migrate cannot be combined with script or transfer options".into());
    }
    if line_mode && transfer.is_some() {
        return Err("--line cannot be combined with import/export".into());
    }
    if transfer.is_none() && matches!(format, TransferFormat::Ndjson) {
        return Err("--ndjson requires import/export".into());
    }
    let db = Database::open(path.as_deref().unwrap_or(":memory:"))?;
    let conn = db.connect()?;
    if let Some(directory) = migrations {
        let plan = migration_plan(&directory)?;
        println!("{}", serde_json::to_string(&conn.migrate(&plan)?)?);
        return Ok(std::process::ExitCode::SUCCESS);
    }
    if let Some((import, table)) = transfer {
        if import {
            let mut input = String::new();
            io::stdin()
                .take(64 * 1024 * 1024 + 1)
                .read_to_string(&mut input)?;
            let count = conn.import_documents(&table, &input, format)?;
            println!("{}", serde_json::json!({"imported":count}));
        } else {
            print!("{}", conn.export_documents(&table, format)?);
        }
        return Ok(std::process::ExitCode::SUCCESS);
    }
    let mut writer = io::stdout().lock();
    let mut failed = false;
    if line_mode {
        for line in io::stdin().lock().lines() {
            let line = line?;
            if !line.trim().is_empty() {
                failed |= output(
                    &mut writer,
                    conn.execute_report(&line, &Parameters::new()),
                    None,
                )?;
            }
        }
    } else {
        let mut script = String::new();
        io::stdin().read_to_string(&mut script)?;
        failed = run_script(&conn, &script, &mut writer)?;
    }
    Ok(if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    })
}

fn run_script(
    conn: &fastdb::Connection,
    script: &str,
    writer: &mut impl Write,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut failed = false;
    let mut output_error = None;
    let execution = conn.visit_batch(script, |report| {
        match output(writer, report.execution, Some(report.offset)) {
            Ok(statement_failed) => failed |= statement_failed,
            Err(error) => output_error = Some(error),
        }
        Ok(output_error.is_none())
    });
    if let Some(error) = output_error {
        return Err(error);
    }
    if let Err(error) = execution {
        let state = conn.transaction_state();
        output(
            writer,
            ExecutionReport {
                result: Err(error),
                transaction_before: state,
                transaction_after: state,
            },
            None,
        )?;
        failed = true;
    }
    Ok(failed)
}

fn migration_plan(directory: &str) -> Result<Vec<fastdb::Migration>, Box<dyn std::error::Error>> {
    let mut plan = Vec::new();
    let mut bytes = 0usize;
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "sql") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("migration filename must be UTF-8")?
            .to_owned();
        let (version, _) = name
            .split_once('_')
            .ok_or("expected VERSION_name.sql migration filename")?;
        let version = version.parse::<i64>()?;
        let mut sql = String::new();
        std::fs::File::open(&path)?
            .take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut sql)?;
        bytes += sql.len();
        if sql.len() > 4 * 1024 * 1024 || bytes > 16 * 1024 * 1024 || plan.len() >= 1000 {
            return Err("migration files exceed runner limits".into());
        }
        plan.push(fastdb::Migration { version, name, sql });
    }
    plan.sort_by_key(|m| m.version);
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingOutput {
        flushes: usize,
        fail_flush: bool,
    }
    impl Write for FailingOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.flushes == 1 && !self.fail_flush {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            Ok(())
        }
    }

    #[test]
    fn output_failure_stops_scripts_before_later_writes() {
        for fail_flush in [false, true] {
            let db = Database::open(":memory:").unwrap();
            let conn = db.connect().unwrap();
            conn.execute("CREATE TABLE samples(value INTEGER)", &Parameters::new())
                .unwrap();
            let mut writer = FailingOutput {
                flushes: 0,
                fail_flush,
            };
            let script = if fail_flush {
                "INSERT INTO samples VALUES (1); INSERT INTO samples VALUES (2);"
            } else {
                "INSERT INTO samples VALUES (1); SELECT * FROM samples; INSERT INTO samples VALUES (2);"
            };
            let error = run_script(&conn, script, &mut writer).unwrap_err();
            assert_eq!(
                error.downcast_ref::<io::Error>().unwrap().kind(),
                io::ErrorKind::BrokenPipe
            );
            assert_eq!(writer.flushes, 1);
            assert_eq!(
                conn.execute("SELECT * FROM samples", &Parameters::new())
                    .unwrap()
                    .rows,
                vec![vec![fastdb::Value::Integer(1)]]
            );
            assert_eq!(
                conn.transaction_state(),
                fastdb::TransactionState::Autocommit
            );
        }
    }

    #[test]
    fn output_failure_preserves_explicit_transaction_for_caller() {
        let db = Database::open(":memory:").unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE TABLE samples(value INTEGER)", &Parameters::new())
            .unwrap();
        conn.execute("BEGIN", &Parameters::new()).unwrap();
        let mut writer = FailingOutput {
            flushes: 0,
            fail_flush: true,
        };
        assert!(run_script(
            &conn,
            "INSERT INTO samples VALUES (1); COMMIT;",
            &mut writer
        )
        .is_err());
        assert_eq!(conn.transaction_state(), fastdb::TransactionState::Active);
        conn.execute("ROLLBACK", &Parameters::new()).unwrap();
        assert!(conn
            .execute("SELECT * FROM samples", &Parameters::new())
            .unwrap()
            .rows
            .is_empty());
    }
}
