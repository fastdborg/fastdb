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
        match conn.execute(&line, &Parameters::new()) {
            Ok(result) => println!("{}", serde_json::to_string(&result)?),
            Err(error) => println!(
                "{}",
                serde_json::json!({"error": {"code": error.code(), "message": error.to_string()}})
            ),
        }
    }
    Ok(())
}
