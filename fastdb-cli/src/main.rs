#![forbid(unsafe_code)]
#![deny(warnings)]

use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use fastdb::{
    Builder, CheckReport, Connection, Error, ErrorCategory, Params, QueryResponse, StatementResult,
    Value,
};
use futures::executor::block_on;
use std::collections::BTreeSet;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use turso_fastdb_parser::{classify_input, InputCompleteness};

#[derive(Debug, Parser)]
#[command(name = "fastdb", version, about = "FastDB local database shell")]
struct Args {
    #[command(subcommand)]
    operation: Option<Operation>,

    /// Open a private in-memory database.
    #[arg(long, conflicts_with = "path")]
    memory: bool,

    /// Database file to open or create.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    /// Execute one request and exit.
    #[arg(short = 'c', value_name = "SOURCE")]
    source_command: Option<String>,

    /// Request output format.
    #[arg(long, value_enum, default_value_t = Output::Human, global = true)]
    output: Output,

    /// Bind a named JSON value. May be repeated with distinct names.
    #[arg(long = "param", value_name = "NAME=JSON")]
    params: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum Operation {
    /// Open the interactive or command-driven database shell.
    Shell(ShellArgs),
    /// Validate catalogs, provider state, and engine integrity.
    Check { path: PathBuf },
    /// Create a checkpointed, validated backup artifact.
    Backup { path: PathBuf, destination: PathBuf },
    /// Validate and restore a backup to a new database path.
    Restore {
        backup: PathBuf,
        destination: PathBuf,
    },
    /// Rebuild one catalog-resolved index from document state.
    RebuildIndex {
        path: PathBuf,
        table: String,
        index: String,
    },
}

#[derive(Debug, ClapArgs)]
struct ShellArgs {
    /// Open a private in-memory database.
    #[arg(long, conflicts_with = "path")]
    memory: bool,

    /// Database file to open or create.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    /// Execute one request and exit.
    #[arg(short = 'c', value_name = "SOURCE")]
    source_command: Option<String>,

    /// Bind a named JSON value. May be repeated with distinct names.
    #[arg(long = "param", value_name = "NAME=JSON")]
    params: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Output {
    Human,
    Json,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err((error, output)) => {
            write_error(&error, output);
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), (Error, Output)> {
    let output = args.output;
    match args.operation {
        Some(Operation::Shell(shell)) => {
            run_shell(
                shell.memory,
                shell.path,
                shell.source_command,
                shell.params,
                output,
            )
            .await
        }
        Some(Operation::Check { path }) => {
            let database = Builder::new_local(path)
                .build()
                .await
                .map_err(|error| (error, output))?;
            let report = database.check().await.map_err(|error| (error, output))?;
            database.close().await.map_err(|error| (error, output))?;
            print_operation("check", &report, output);
            Ok(())
        }
        Some(Operation::Backup { path, destination }) => {
            let database = Builder::new_local(path)
                .build()
                .await
                .map_err(|error| (error, output))?;
            let report = database
                .backup_to(destination)
                .await
                .map_err(|error| (error, output))?;
            database.close().await.map_err(|error| (error, output))?;
            print_operation("backup", &report, output);
            Ok(())
        }
        Some(Operation::Restore {
            backup,
            destination,
        }) => {
            let report = restore(&backup, &destination)
                .await
                .map_err(|error| (error, output))?;
            print_operation("restore", &report, output);
            Ok(())
        }
        Some(Operation::RebuildIndex { path, table, index }) => {
            let database = Builder::new_local(path)
                .build()
                .await
                .map_err(|error| (error, output))?;
            database
                .rebuild_index(&table, &index)
                .await
                .map_err(|error| (error, output))?;
            let report = database.check().await.map_err(|error| (error, output))?;
            database.close().await.map_err(|error| (error, output))?;
            print_operation("rebuild-index", &report, output);
            Ok(())
        }
        None => {
            run_shell(
                args.memory,
                args.path,
                args.source_command,
                args.params,
                output,
            )
            .await
        }
    }
}

async fn run_shell(
    memory: bool,
    path: Option<PathBuf>,
    source_command: Option<String>,
    raw_params: Vec<String>,
    output: Output,
) -> Result<(), (Error, Output)> {
    let params = parse_params(&raw_params).map_err(|error| (error, output))?;
    let builder = if memory {
        Builder::new_memory()
    } else {
        Builder::new_local(path.ok_or_else(|| {
            (
                Error::new(ErrorCategory::Schema, "PATH or --memory is required"),
                output,
            )
        })?)
    };
    let database = builder.build().await.map_err(|error| (error, output))?;
    let connection = database.connect().map_err(|error| (error, output))?;

    let result = if let Some(source) = source_command {
        execute_and_print(&connection, &source, params, output).await
    } else if io::stdin().is_terminal() {
        interactive(&connection, params, output).await
    } else {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| (Error::new(ErrorCategory::Io, error.to_string()), output))?;
        execute_and_print(&connection, &source, params, output).await
    };
    let close = connection.close().await;
    let database_close = if close.is_ok() {
        database.close().await
    } else {
        Ok(())
    };
    match (result, close, database_close) {
        (Err(error), _, _) => Err((error, output)),
        (Ok(()), Err(error), _) | (Ok(()), Ok(()), Err(error)) => Err((error, output)),
        (Ok(()), Ok(()), Ok(())) => Ok(()),
    }
}

async fn restore(backup: &PathBuf, destination: &PathBuf) -> Result<CheckReport, Error> {
    if !backup.is_file() {
        return Err(Error::new(
            ErrorCategory::Io,
            "restore source is not a database file",
        ));
    }
    if destination.exists() {
        return Err(Error::new(
            ErrorCategory::Io,
            "restore destination already exists",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| Error::new(ErrorCategory::Io, "restore destination has no parent"))?;
    let file_name = destination
        .file_name()
        .ok_or_else(|| Error::new(ErrorCategory::Io, "restore destination has no file name"))?;
    let source = std::fs::canonicalize(backup)
        .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
    let destination_absolute = if destination.is_absolute() {
        destination.clone()
    } else {
        std::env::current_dir()
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?
            .join(destination)
    };
    if source == destination_absolute {
        return Err(Error::new(
            ErrorCategory::Io,
            "restore destination must differ from the backup",
        ));
    }
    let source_database = Builder::new_local(&source).build().await?;
    source_database.check().await?;
    source_database.close().await?;

    let temporary = parent.join(format!(
        ".{}.fastdb-restore-{}.tmp",
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let result = async {
        std::fs::copy(&source, &temporary)
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        let restored = Builder::new_local(&temporary).build().await?;
        let report = restored.check().await?;
        restored.close().await?;
        std::fs::rename(&temporary, destination)
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        sync_parent(parent)?;
        Ok(report)
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn print_operation(operation: &str, report: &CheckReport, output: Output) {
    match output {
        Output::Human => println!(
            "{operation}: ok (format {}, tables {}, indexes {}, FTS indexes {}, vector fields {}, pinned FTS exception {})",
            report.format_version,
            report.tables,
            report.indexes,
            report.fts_indexes,
            report.vector_fields,
            report.pinned_fts_exception
        ),
        Output::Json => println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "operation": operation,
                "report": {
                    "format_version": report.format_version,
                    "tables": report.tables,
                    "indexes": report.indexes,
                    "fts_indexes": report.fts_indexes,
                    "vector_fields": report.vector_fields,
                    "pinned_fts_exception": report.pinned_fts_exception,
                }
            })
        ),
    }
}

#[cfg(unix)]
fn sync_parent(parent: &std::path::Path) -> Result<(), Error> {
    std::fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &std::path::Path) -> Result<(), Error> {
    Ok(())
}

async fn interactive(connection: &Connection, params: Params, output: Output) -> Result<(), Error> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let mut source = String::new();
    loop {
        let prompt = if source.is_empty() {
            "fastdb> "
        } else {
            "...> "
        };
        print!("{prompt}");
        io::stdout()
            .flush()
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        let Some(line) = lines.next() else {
            if source.trim().is_empty() {
                return Ok(());
            }
            return execute_and_print(connection, &source, params, output).await;
        };
        let line = line.map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        if !source.is_empty() {
            source.push('\n');
        }
        source.push_str(&line);
        match classify_input(&source) {
            InputCompleteness::Incomplete => continue,
            InputCompleteness::Complete | InputCompleteness::Invalid => {
                match execute_and_print(connection, &source, params.clone(), output).await {
                    Ok(()) => {}
                    Err(error) => write_error(&error, output),
                }
                source.clear();
            }
        }
    }
}

async fn execute_and_print(
    connection: &Connection,
    source: &str,
    params: Params,
    output: Output,
) -> Result<(), Error> {
    let response = connection.query(source, params).await?;
    match output {
        Output::Human => print_human(&response),
        Output::Json => {
            let value = fastdb::json::response_to_json(&response)?;
            println!("{value}");
        }
    }
    Ok(())
}

fn parse_params(values: &[String]) -> Result<Params, Error> {
    let mut names = BTreeSet::new();
    let mut params = Params::new();
    for binding in values {
        let (name, json) = binding
            .split_once('=')
            .ok_or_else(|| Error::new(ErrorCategory::Schema, "--param requires NAME=JSON"))?;
        if !names.insert(name.to_owned()) {
            return Err(Error::new(
                ErrorCategory::Schema,
                format!("parameter name {name:?} was provided more than once"),
            ));
        }
        let json = serde_json::from_str(json).map_err(|error| {
            Error::new(
                ErrorCategory::Schema,
                format!("parameter {name:?} is not valid JSON: {error}"),
            )
        })?;
        params.insert(name, fastdb::json::value_from_json(json)?);
    }
    Ok(params)
}

fn print_human(response: &QueryResponse) {
    for (index, statement) in response.statements.iter().enumerate() {
        println!("-- statement {} --", index + 1);
        match statement {
            StatementResult::None => println!("NONE"),
            StatementResult::Rows(rows) => {
                println!("[");
                for value in rows {
                    println!("  {},", human_value(value));
                }
                println!("]");
            }
            StatementResult::Value(value) => println!("{}", human_value(value)),
        }
    }
}

fn human_value(value: &Value) -> String {
    match value {
        Value::None => "NONE".into(),
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Decimal(value) => format!("{}dec", value.to_canonical()),
        Value::Str(value) => {
            serde_json::to_string(value).expect("serializing a string cannot fail")
        }
        Value::Bytes(value) => format!(
            "b\"{}\"",
            value
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        ),
        Value::Duration(value) => value.to_canonical(),
        Value::Datetime(value) => format!("d'{}'", value.to_canonical()),
        Value::Uuid(value) => format!("u'{}'", value.hyphenated()),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(human_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    serde_json::to_string(key).expect("serializing a key cannot fail"),
                    human_value(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Set(values) => format!(
            "set[{}]",
            values
                .as_slice()
                .iter()
                .map(human_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Range(value) => format!(
            "range({}, {})",
            human_range_bound(value.start()),
            human_range_bound(value.end())
        ),
        Value::Regex(value) => format!("/{}/", value.as_str()),
        Value::RecordId(value) => value.to_string(),
        Value::Table(value) => format!("table({})", value.as_str()),
        Value::File(value) => format!("f'{}'", value.as_str()),
    }
}

fn human_range_bound(bound: &fastdb::RangeBound) -> String {
    match bound {
        fastdb::RangeBound::Unbounded => "unbounded".into(),
        fastdb::RangeBound::Included(value) => format!("included {}", human_value(value)),
        fastdb::RangeBound::Excluded(value) => format!("excluded {}", human_value(value)),
    }
}

fn write_error(error: &Error, output: Output) {
    match output {
        Output::Human => {
            if let Some(span) = error.span() {
                eprintln!(
                    "{} error at bytes {}..{}: {}",
                    error.category().as_str(),
                    span.offset,
                    span.offset + span.len,
                    error
                );
            } else {
                eprintln!("{} error: {}", error.category().as_str(), error);
            }
        }
        Output::Json => eprintln!("{}", fastdb::json::error_to_json(error)),
    }
}
