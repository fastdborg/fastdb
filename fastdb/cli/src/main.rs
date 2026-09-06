mod input;
#[cfg(unix)]
mod signals;
use fastdb::{Database, ExecutionReport, Parameters, TransferFormat};
use std::io::{self, BufRead, IsTerminal, Read, Write};
fn output(
    writer: &mut impl Write,
    report: ExecutionReport,
    offset: Option<usize>,
) -> Result<bool, Box<dyn std::error::Error>> {
    output_with_metrics(writer, report, offset, None)
}
fn output_with_metrics(
    writer: &mut impl Write,
    report: ExecutionReport,
    offset: Option<usize>,
    metrics: Option<fastdb::QueryMetrics>,
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
    if let Some(metrics) = metrics {
        output["profile"] = serde_json::to_value(metrics)?;
    }
    if let Some(offset) = offset {
        output["offset"] = offset.into();
    }
    writeln!(writer, "{}", serde_json::to_string(&output)?)?;
    writer.flush()?;
    Ok(failed)
}
fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let mut path = None;
    let mut input_limit = None;
    let mut line_mode = false;
    let mut interactive = false;
    let mut script_mode = false;
    let mut history = None;
    let mut transfer = None;
    let mut migrations = None;
    let mut audit = None;
    let mut audit_documents = None;
    let mut audit_bytes = None;
    let mut format = TransferFormat::Json;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-input-bytes" => {
                let limit = args
                    .next()
                    .ok_or("expected input byte limit")?
                    .parse::<usize>()?;
                if limit == 0 || limit.checked_add(1).is_none() {
                    return Err("input byte limit must be positive and below usize::MAX".into());
                }
                input_limit = Some(limit);
            }
            "--check-collection" => {
                if audit.is_some() {
                    return Err("choose one collection audit".into());
                }
                audit = Some(args.next().ok_or("expected collection name")?);
            }
            "--max-documents" => {
                audit_documents = Some(
                    args.next()
                        .ok_or("expected document limit")?
                        .parse::<u64>()?,
                )
            }
            "--max-encoded-bytes" => {
                audit_bytes = Some(
                    args.next()
                        .ok_or("expected encoded-byte limit")?
                        .parse::<u64>()?,
                )
            }
            "--migrate" => {
                if migrations.is_some() {
                    return Err("choose one migration directory".into());
                }
                migrations = Some(args.next().ok_or("expected migration directory")?);
            }
            "--history" => {
                if history.is_some() {
                    return Err("choose one history path".into());
                }
                history = Some(std::path::PathBuf::from(
                    args.next().ok_or("expected history path")?,
                ));
            }
            "--line" => line_mode = true,
            "--interactive" => interactive = true,
            "--script" => script_mode = true,
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
                println!("Usage: fastdb-cli [--interactive | --script | --line] [--max-input-bytes N] [--history PATH] [DATABASE]\n       fastdb-cli --migrate DIRECTORY [DATABASE]\n       fastdb-cli (--import COLLECTION | --export COLLECTION) [--ndjson] [DATABASE]\n       fastdb-cli --check-collection COLLECTION [--max-documents N] [--max-encoded-bytes N] DATABASE\nTerminal input opens an interactive prompt; piped input runs a script.\n--script reads through EOF and stops on the first error.\n--interactive accepts multiline statements and .help, .clear, .quit.\nUnix terminals support line editing and in-memory history; --history PATH saves history.\nCtrl-C clears pending input at the prompt or requests cancellation of running engine work.\n--line retains one-statement-per-line execution and continues after errors.\nInput buffers default to 16 MiB; --max-input-bytes changes this byte limit.");
                return Ok(std::process::ExitCode::SUCCESS);
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}").into()),
            _ if path.is_none() => path = Some(arg),
            _ => return Err("expected one database path".into()),
        }
    }
    if usize::from(line_mode) + usize::from(interactive) + usize::from(script_mode) > 1 {
        return Err("choose one input mode".into());
    }
    if (interactive || script_mode) && (migrations.is_some() || transfer.is_some()) {
        return Err("input modes cannot be combined with migration/import/export".into());
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
    if input_limit.is_some() && (migrations.is_some() || transfer.is_some()) {
        return Err("--max-input-bytes applies only to SQL input modes".into());
    }
    if audit.is_none() && (audit_documents.is_some() || audit_bytes.is_some()) {
        return Err("audit limits require --check-collection".into());
    }
    if let Some(table) = audit {
        if line_mode
            || interactive
            || script_mode
            || migrations.is_some()
            || transfer.is_some()
            || history.is_some()
            || input_limit.is_some()
        {
            return Err("collection audit cannot be combined with input, history, migration or transfer options".into());
        }
        let path = path
            .as_deref()
            .ok_or("collection audit requires a database path")?;
        let mut limits = fastdb::IntegrityLimits::default();
        if let Some(value) = audit_documents {
            limits.max_documents = value;
        }
        if let Some(value) = audit_bytes {
            limits.max_encoded_bytes = value;
        }
        return run_audit(path, &table, limits);
    }
    let interactive_mode = interactive || (!line_mode && !script_mode && io::stdin().is_terminal());
    let terminal = interactive_mode
        && migrations.is_none()
        && transfer.is_none()
        && io::stdin().is_terminal()
        && io::stderr().is_terminal()
        && input::terminal_available();
    if history.is_some() && !terminal {
        return Err("--history requires interactive terminal input and terminal stderr".into());
    }
    let mut editor = if terminal {
        Some(input::Terminal::new(
            history.as_deref(),
            std::path::Path::new(path.as_deref().unwrap_or(":memory:")),
        )?)
    } else {
        None
    };
    let input_limit = input_limit.unwrap_or(16 * 1024 * 1024);
    let db = Database::open(path.as_deref().unwrap_or(":memory:"))?;
    let conn = db.connect()?;
    #[cfg(unix)]
    let _interrupts = if terminal {
        Some(signals::Interrupts::new(conn.interrupt_handle())?)
    } else {
        None
    };
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
    if interactive_mode {
        if let Some(editor) = &mut editor {
            failed = run_interactive(
                &conn,
                editor,
                &mut writer,
                &mut io::stderr().lock(),
                input_limit,
            )?;
            editor.save()?;
        } else {
            failed = run_interactive(
                &conn,
                &mut input::Plain(&mut io::stdin().lock()),
                &mut writer,
                &mut io::stderr().lock(),
                input_limit,
            )?;
        }
    } else if line_mode {
        let mut reader = io::stdin().lock();
        loop {
            let bytes = read_input(&mut reader, input_limit, true)?;
            if bytes.len() > input_limit {
                report_input_limit(&conn, &mut writer, input_limit)?;
                return Ok(std::process::ExitCode::FAILURE);
            }
            if bytes.is_empty() {
                break;
            }
            let line = String::from_utf8(bytes)?;
            if !line.trim().is_empty() {
                failed |= if let Some(sql) = line.trim_start().strip_prefix(".profile ") {
                    run_profile(&conn, sql, &mut writer)?
                } else {
                    output(
                        &mut writer,
                        conn.execute_report(&line, &Parameters::new()),
                        None,
                    )?
                };
            }
        }
    } else {
        let bytes = read_input(&mut io::stdin().lock(), input_limit, false)?;
        if bytes.len() > input_limit {
            report_input_limit(&conn, &mut writer, input_limit)?;
            return Ok(std::process::ExitCode::FAILURE);
        }
        failed = run_script(&conn, &String::from_utf8(bytes)?, &mut writer)?;
    }

    Ok(if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    })
}

fn run_audit(
    path: &str,
    table: &str,
    limits: fastdb::IntegrityLimits,
) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let opened = if std::path::Path::new(path).is_file() {
        Database::open(path).and_then(|db| db.connect().map(|conn| (db, conn)))
    } else {
        Err(fastdb::Error::NotFound(format!(
            "audit requires an existing database file: {path}"
        )))
    };
    let error_value = |error: fastdb::Error| serde_json::json!({"error":{"code":error.code(),"message":error.to_string()}});
    let (value, failed) = match opened {
        Err(error) => (error_value(error), true),
        Ok((_db, conn)) => {
            let before = conn.transaction_state();
            let (mut value, failed) = match conn.check_collection_integrity(table, limits) {
                Ok(report) => (serde_json::to_value(report)?, false),
                Err(error) => (error_value(error), true),
            };
            value["transaction"] =
                serde_json::json!({"before":before,"after":conn.transaction_state()});
            (value, failed)
        }
    };
    let mut writer = io::stdout().lock();
    writeln!(writer, "{}", serde_json::to_string(&value)?)?;
    writer.flush()?;
    Ok(if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    })
}

// Read at most one sentinel byte beyond the limit; never execute this prefix
// when input is oversized, and check size before decoding a split UTF-8 scalar.
fn read_input(reader: &mut impl BufRead, limit: usize, line: bool) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut bounded = reader.take((limit + 1) as u64);
    if line {
        bounded.read_until(b'\n', &mut bytes)?;
    } else {
        bounded.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}
fn report_input_limit(
    conn: &fastdb::Connection,
    writer: &mut impl Write,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = conn.transaction_state();
    output(
        writer,
        ExecutionReport {
            result: Err(fastdb::Error::Limit(format!(
                "CLI input exceeds {limit} bytes"
            ))),
            transaction_before: state,
            transaction_after: state,
        },
        None,
    )?;
    Ok(())
}

fn run_interactive(
    conn: &fastdb::Connection,
    reader: &mut impl input::Input,
    writer: &mut impl Write,
    prompt: &mut impl Write,
    input_limit: usize,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut buffer = String::new();
    let mut failed = false;
    loop {
        let label = if !buffer.is_empty() {
            "...> "
        } else if conn.transaction_state() == fastdb::TransactionState::Active {
            "fastdb(tx)> "
        } else {
            "fastdb> "
        };
        let bytes = match reader.read(label, prompt, input_limit)? {
            input::Read::Line(bytes) => bytes,
            input::Read::Interrupted => {
                buffer.clear();
                writeln!(prompt)?;
                continue;
            }
        };
        if bytes.len() > input_limit {
            report_input_limit(conn, writer, input_limit)?;
            return Ok(true);
        }
        if bytes.is_empty() {
            if !buffer.trim().is_empty() {
                reader.remember(&buffer)?;
                failed |= run_script(conn, &buffer, writer)?;
            }
            return Ok(failed);
        }
        let line = String::from_utf8(bytes)?;
        match line.trim() {
            ".quit" | ".exit" => return Ok(failed),
            ".clear" => {
                buffer.clear();
                continue;
            }
            ".help" => {
                writeln!(
                    prompt,
                    "End statements with a semicolon. .clear discards pending input; .quit exits.\n.profile SELECT ... executes one SELECT with primary engine counters.
Transactions use BEGIN, COMMIT and ROLLBACK. JSON results go to stdout.\nTerminal editing supports arrows and history. Ctrl-C clears pending input or cancels running engine work.\nHistory stays in memory unless --history PATH is supplied; leading spaces omit entries."
                )?;
                continue;
            }
            _ => {}
        }
        if line.len() > input_limit - buffer.len() {
            report_input_limit(conn, writer, input_limit)?;
            return Ok(true);
        }
        buffer.push_str(&line);
        match fastql_parser::script_complete(&buffer) {
            Ok(false) => continue,
            Ok(true) | Err(_) => {
                reader.remember(&buffer)?;
                failed |= run_script(conn, &buffer, writer)?;
                buffer.clear();
            }
        }
    }
}

fn run_profile(
    conn: &fastdb::Connection,
    sql: &str,
    writer: &mut impl Write,
) -> Result<bool, Box<dyn std::error::Error>> {
    let before = conn.transaction_state();
    let (result, metrics) = match conn.profile_select(sql, &Parameters::new()) {
        Ok(profile) => (Ok(profile.result), Some(profile.metrics)),
        Err(error) => (Err(error), None),
    };
    output_with_metrics(
        writer,
        ExecutionReport {
            result,
            transaction_before: before,
            transaction_after: conn.transaction_state(),
        },
        None,
        metrics,
    )
}
fn run_script(
    conn: &fastdb::Connection,
    script: &str,
    writer: &mut impl Write,
) -> Result<bool, Box<dyn std::error::Error>> {
    if let Some(sql) = script.trim_start().strip_prefix(".profile ") {
        return run_profile(conn, sql, writer);
    }
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
