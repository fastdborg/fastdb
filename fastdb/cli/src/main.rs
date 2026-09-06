use fastdb::{Database, Parameters};
use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| ":memory:".into());
    let db = Database::open(&path)?;
    let conn = db.connect()?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let report = conn.execute_report(&line, &Parameters::new());
        let mut output = match report.result {
            Ok(result) => serde_json::to_value(result)?,
            Err(error) => {
                serde_json::json!({"error": {"code": error.code(), "message": error.to_string()}})
            }
        };
        output["transaction"] = serde_json::json!({
            "before": report.transaction_before,
            "after": report.transaction_after,
        });
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
