use fastdb::{Database, ExecutionReport, Parameters};
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
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--line" => line_mode = true,
            "--help" | "-h" => {
                println!("Usage: fastdb-cli [--line] [DATABASE]\nReads a semicolon-delimited script from stdin; stops on the first error.\n--line retains one-statement-per-line execution and continues after errors.");
                return Ok(std::process::ExitCode::SUCCESS);
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}").into()),
            _ if path.is_none() => path = Some(arg),
            _ => return Err("expected one database path".into()),
        }
    }
    let db = Database::open(path.as_deref().unwrap_or(":memory:"))?;
    let conn = db.connect()?;
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
