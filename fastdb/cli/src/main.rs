use fastdb::{Database, ExecutionReport, Parameters, TransferFormat};
use std::io::{self, BufRead, Read};
fn output(
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
    println!("{}", serde_json::to_string(&output)?);
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
    let mut failed = false;
    if line_mode {
        for line in io::stdin().lock().lines() {
            let line = line?;
            if !line.trim().is_empty() {
                failed |= output(conn.execute_report(&line, &Parameters::new()), None)?;
            }
        }
    } else {
        let mut script = String::new();
        io::stdin().read_to_string(&mut script)?;
        match conn.execute_batch(&script) {
            Ok(reports) => {
                for report in reports {
                    failed |= output(report.execution, Some(report.offset))?;
                }
            }
            Err(error) => {
                let state = conn.transaction_state();
                output(
                    ExecutionReport {
                        result: Err(error),
                        transaction_before: state,
                        transaction_after: state,
                    },
                    None,
                )?;
                failed = true;
            }
        }
    }
    Ok(if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    })
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
