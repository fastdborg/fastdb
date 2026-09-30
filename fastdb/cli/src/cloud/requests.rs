//! Tracked one-shot reads and queries with immutable crash-recovery identity.
use super::{canonical_uuid, journal, Cloud, Method, Result, Value, LIMIT};
use serde_json::json;
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;
const MAX_SEQUENCE: u64 = 9_007_199_254_740_990;

fn validate_body(body: &Value) -> Result<()> {
    canonical_uuid(body["requestId"].as_str().ok_or("Missing request ID")?)?;
    if body["afterSequence"]
        .as_u64()
        .is_none_or(|s| s > MAX_SEQUENCE)
        || body.as_object().map(|o| o.len()) != Some(3)
    {
        return Err("Invalid request identity".into());
    }
    let statements = body["statements"].as_array().ok_or("Missing statements")?;
    if statements.is_empty()
        || statements.len() > 32
        || statements.iter().any(|s| {
            s.as_object().map(|o| o.len()) != Some(1)
                || s["sql"].as_str().is_none_or(|sql| sql.trim().is_empty())
        })
    {
        return Err("Expected 1 to 32 SQL statements".into());
    }
    if serde_json::to_vec(body)?.len() > LIMIT {
        return Err("Request exceeds 64 KiB".into());
    }
    Ok(())
}
pub(super) fn confirmed(body: &Value, reply: &Value) -> bool {
    reply["requestId"] == body["requestId"]
        && body["afterSequence"].as_u64().is_some_and(|after| {
            reply["sequence"].as_u64().is_some_and(|sequence| {
                sequence <= MAX_SEQUENCE + 1 && sequence > after && sequence - after <= 64
            })
        })
        && reply["results"].is_array()
}
pub(super) fn sequence(cloud: &Cloud, id: &str) -> Result<u64> {
    let info = cloud
        .request(Method::GET, &cloud.database_path(id)?, None)
        .map_err(|e| e.message)?;
    if info["id"] != id {
        return Err("Database response identity differs".into());
    }
    info["sequence"]
        .as_u64()
        .filter(|n| *n <= MAX_SEQUENCE)
        .ok_or_else(|| "Invalid database sequence".into())
}
fn dispatch(cloud: &Cloud, value: &Value) -> Result<ExitCode> {
    let id = value["databaseId"]
        .as_str()
        .ok_or("Missing database UUID")?;
    let op = value["operation"].as_str().ok_or("Missing operation")?;
    if value["version"] != 2
        || value["kind"] != "cloud-request"
        || value["origin"] != cloud.origin.as_str()
        || value["organizationId"] != cloud.organization()?
        || !["read", "query"].contains(&op)
        || value.as_object().map(|o| o.len()) != Some(7)
    {
        return Err(
            "Journal does not match the selected origin/organization or request format".into(),
        );
    }
    let path = format!("{}/{op}", cloud.database_path(id)?);
    let body = &value["request"];
    validate_body(body)?;
    eprintln!(
        "Request {}. Keep the journal and use db retry after an uncertain reply.",
        body["requestId"]
    );
    match cloud.request(Method::POST, &path, Some(body)) {
        Ok(reply) => {
            if !confirmed(body, &reply) {
                return Err("Unconfirmed response identity; retry the same journal".into());
            }
            cloud.output(&reply)?;
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            // The immutable journal can have earlier unknown dispatches. A new
            // rejection must never be presented as proof that they rolled back.
            cloud.output(&json!({"error":error.message,"outcome":"unresolved","requestId":body["requestId"],"afterSequence":body["afterSequence"]}))?;
            Ok(ExitCode::FAILURE)
        }
    }
}
pub(super) fn start(cloud: &Cloud, operation: &str, id: &str, path: &str) -> Result<ExitCode> {
    cloud.database_path(id)?;
    if Path::new(path).try_exists()? {
        return Err("Journal exists; use db retry rather than submitting a new request".into());
    }
    let mut sql = String::new();
    io::stdin()
        .take((LIMIT + 1) as u64)
        .read_to_string(&mut sql)?;
    if sql.len() > LIMIT {
        return Err("Cloud input exceeds 64 KiB".into());
    }
    let statements = fastql_parser::split_script(&sql).map_err(|_| "Invalid SQL/FastQL script")?;
    let mut body = json!({"requestId":uuid::Uuid::new_v4().to_string(),"afterSequence":0,
        "statements":statements.iter().map(|s|json!({"sql":s.sql})).collect::<Vec<_>>()});
    validate_body(&body)?;
    body["afterSequence"] = json!(sequence(cloud, id)?);
    validate_body(&body)?;
    let value = json!({"version":2,"kind":"cloud-request","origin":cloud.origin.as_str(),"organizationId":cloud.organization()?,
        "databaseId":id,"operation":operation,"request":body});
    journal::save(path, &value)?;
    dispatch(cloud, &value)
}
pub(super) fn retry(cloud: &Cloud, path: &str) -> Result<ExitCode> {
    dispatch(cloud, &journal::load(path, LIMIT + 4096)?)
}
