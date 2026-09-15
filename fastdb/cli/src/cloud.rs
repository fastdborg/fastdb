use reqwest::{blocking::Client, Method, Url};
use serde_json::{json, Value};
use std::io::{self, IsTerminal, Read, Write};
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const LIMIT: usize = 64 * 1024;
const HELP: &str = "fastdb cloud whoami\nfastdb cloud db create NAME\nfastdb cloud db list\nfastdb cloud db show UUID\nfastdb cloud db delete UUID\nfastdb cloud db access UUID\n\nSet FASTDB_API_KEY. FASTDB_CLOUD_URL defaults to https://cloud.fastdb.org.\nAccess accepts SQL/FastQL ending in semicolons; batches commit atomically.\n.quit exits, .clear discards input, .retry retries an uncertain request.\nQueries need query and databases:read scopes. JSON output goes to stdout.";

struct Cloud {
    http: Client,
    origin: Url,
    key: String,
}
struct Failure {
    message: String,
    uncertain: bool,
}
impl Cloud {
    fn new(endpoint: &str, key: String) -> Result<Self> {
        let origin = Url::parse(endpoint).map_err(|_| "Invalid FASTDB_CLOUD_URL")?;
        let local = origin.host_str() == Some("localhost")
            || origin.host_str().is_some_and(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            });
        if !(origin.scheme() == "https" || origin.scheme() == "http" && local)
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
        {
            return Err(
                "FASTDB_CLOUD_URL must be an HTTPS origin (HTTP is allowed only on loopback)"
                    .into(),
            );
        }
        if !key.starts_with("fdbk_")
            || key.len() != 69
            || !key[5..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Set FASTDB_API_KEY to a valid FastDB API key".into());
        }
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| "Cannot initialize HTTPS client")?;
        Ok(Self { http, origin, key })
    }
    fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> std::result::Result<Value, Failure> {
        let failure = |message: &str, uncertain| Failure {
            message: message.into(),
            uncertain,
        };
        let url = self
            .origin
            .join(path)
            .map_err(|_| failure("Invalid API path", false))?;
        let mut request = self.http.request(method, url).bearer_auth(&self.key);
        if let Some(body) = body {
            let encoded =
                serde_json::to_vec(body).map_err(|_| failure("Invalid query body", false))?;
            if encoded.len() > LIMIT {
                return Err(failure("Request exceeds 64 KiB", false));
            }
            request = request
                .header("content-type", "application/json")
                .body(encoded);
        }
        let response = request
            .send()
            .map_err(|_| failure("Cloud request could not be acknowledged", true))?;
        let status = response.status();
        if status.as_u16() == 204 {
            return Ok(Value::Null);
        }
        let mut bytes = Vec::new();
        response
            .take(512 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| failure("Cloud response was interrupted", true))?;
        if bytes.len() > 512 * 1024 {
            return Err(failure("Cloud response exceeds 512 KiB", true));
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| failure("Invalid cloud response", true))?;
        if !status.is_success() {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Cloud request failed");
            return Err(Failure {
                message: format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    message.replace(&self.key, "[redacted]")
                ),
                uncertain: status.is_server_error() || status.is_redirection(),
            });
        }
        Ok(value)
    }
    fn output(&self, value: &Value) -> Result<()> {
        println!(
            "{}",
            serde_json::to_string(value)?.replace(&self.key, "[redacted]")
        );
        Ok(())
    }
    fn access(&self, id: &str) -> Result<std::process::ExitCode> {
        let path = database_path(id)?;
        let interactive = io::stdin().is_terminal();
        let mut stdin = io::stdin().lock();
        let mut reader: Box<dyn crate::input::Input + '_> =
            if interactive && crate::input::terminal_available() {
                Box::new(crate::input::Terminal::new(
                    None,
                    std::path::Path::new(":memory:"),
                )?)
            } else {
                Box::new(crate::input::Plain(&mut stdin))
            };
        let mut prompt: Box<dyn Write> = if interactive {
            Box::new(io::stderr())
        } else {
            Box::new(io::sink())
        };
        let mut buffer = String::new();
        let mut pending: Option<Value> = None;
        let mut pending_uncertain = false;
        let mut failed = false;
        loop {
            let bytes = match reader.read(
                if buffer.is_empty() {
                    "fastdb(cloud)> "
                } else {
                    "...> "
                },
                &mut prompt,
                LIMIT,
            )? {
                crate::input::Read::Line(bytes) => bytes,
                crate::input::Read::Interrupted => {
                    buffer.clear();
                    continue;
                }
            };
            let eof = bytes.is_empty();
            let line = String::from_utf8(bytes)?;
            if line.trim() == ".quit" || line.trim() == ".exit" || eof && buffer.trim().is_empty() {
                if let Some(request) = pending {
                    eprintln!(
                        "Unresolved request {} at sequence {}; no new write was submitted",
                        request["requestId"], request["expectedSequence"]
                    );
                    failed = true;
                }
                return Ok(if failed {
                    std::process::ExitCode::FAILURE
                } else {
                    std::process::ExitCode::SUCCESS
                });
            }
            if line.trim() == ".help" {
                writeln!(prompt, "{HELP}")?;
                continue;
            }
            if line.trim() == ".clear" {
                buffer.clear();
                continue;
            }
            if line.trim() != ".retry" {
                if pending.is_some() {
                    writeln!(
                        prompt,
                        "Use .retry to resolve the previous request before submitting more SQL"
                    )?;
                    failed = true;
                    continue;
                }
                if buffer.len() + line.len() > LIMIT {
                    return Err("Cloud input exceeds 64 KiB".into());
                }
                buffer.push_str(&line);
                if !eof && !fastql_parser::script_complete(&buffer).unwrap_or(true) {
                    continue;
                }
                let statements = fastql_parser::split_script(&buffer)
                    .map_err(|_| "Invalid SQL/FastQL script")?;
                if statements.is_empty() {
                    buffer.clear();
                    continue;
                }
                if statements.len() > 32 {
                    return Err("Cloud batches allow at most 32 statements".into());
                }
                let info = self
                    .request(Method::GET, &path, None)
                    .map_err(|error| error.message)?;
                let sequence = info["sequence"]
                    .as_u64()
                    .ok_or("Missing database sequence")?;
                pending_uncertain = false;
                pending = Some(
                    json!({ "requestId": uuid::Uuid::new_v4().to_string(), "expectedSequence": sequence,
                    "statements": statements.iter().map(|statement| json!({"sql": statement.sql})).collect::<Vec<_>>() }),
                );
                buffer.clear();
            }
            let Some(request) = pending.as_ref() else {
                writeln!(prompt, "No request to retry")?;
                continue;
            };
            match self.request(Method::POST, &(path.clone() + "/query"), Some(request)) {
                Ok(result) => {
                    self.output(&result)?;
                    pending = None;
                }
                Err(error) => {
                    self.output(&json!({ "error": error.message, "uncertain": pending_uncertain || error.uncertain, "requestId": request["requestId"], "expectedSequence": request["expectedSequence"] }))?;
                    pending_uncertain |= error.uncertain;
                    if !pending_uncertain {
                        pending = None;
                    }
                    failed = true;
                }
            }
        }
    }
}
fn database_path(id: &str) -> Result<String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "Invalid database UUID")?;
    if id.get_version_num() != 4 {
        return Err("Expected a version 4 database UUID".into());
    }
    Ok(format!("/v1/databases/{id}"))
}
pub fn run(args: Vec<String>) -> Result<std::process::ExitCode> {
    if args.is_empty() || args == ["--help"] || args == ["help"] {
        println!("{HELP}");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    let cloud = Cloud::new(
        &std::env::var("FASTDB_CLOUD_URL").unwrap_or_else(|_| "https://cloud.fastdb.org".into()),
        std::env::var("FASTDB_API_KEY")
            .map_err(|_| "Set FASTDB_API_KEY before using cloud commands")?,
    )?;
    let parts: Vec<_> = args.iter().map(String::as_str).collect();
    let (method, path, body) = match parts.as_slice() {
        ["whoami"] => (Method::GET, "/v1/whoami".into(), None),
        ["db", "list"] => (Method::GET, "/v1/databases".into(), None),
        ["db", "create", name] => (
            Method::POST,
            "/v1/databases".into(),
            Some(json!({ "name": name })),
        ),
        ["db", "show", id] => (Method::GET, database_path(id)?, None),
        ["db", "delete", id] => (Method::DELETE, database_path(id)?, None),
        ["db", "access", id] => return cloud.access(id),
        _ => return Err(HELP.into()),
    };
    let result = cloud
        .request(method, &path, body.as_ref())
        .map_err(|error| error.message)?;
    cloud.output(&result)?;
    Ok(std::process::ExitCode::SUCCESS)
}
