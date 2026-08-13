#![forbid(unsafe_code)]
#![deny(warnings)]

use clap::{Parser, ValueEnum};
use fastdb::{
    Builder, Connection, Error, ErrorCategory, Params, QueryResponse, StatementResult, Value,
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
    /// Open a private in-memory database.
    #[arg(long, conflicts_with = "path")]
    memory: bool,

    /// Database file to open or create.
    #[arg(value_name = "PATH", required_unless_present = "memory")]
    path: Option<PathBuf>,

    /// Execute one request and exit.
    #[arg(short = 'c', value_name = "SOURCE")]
    command: Option<String>,

    /// Request output format.
    #[arg(long, value_enum, default_value_t = Output::Human)]
    output: Output,

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
    let params = parse_params(&args.params).map_err(|error| (error, output))?;
    let builder = if args.memory {
        Builder::new_memory()
    } else {
        Builder::new_local(args.path.expect("clap requires PATH or --memory"))
    };
    let database = builder.build().await.map_err(|error| (error, output))?;
    let connection = database.connect().map_err(|error| (error, output))?;

    let result = if let Some(source) = args.command {
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
    match (result, close) {
        (Err(error), _) => Err((error, output)),
        (Ok(()), Err(error)) => Err((error, output)),
        (Ok(()), Ok(())) => Ok(()),
    }
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
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Str(value) => {
            serde_json::to_string(value).expect("serializing a string cannot fail")
        }
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
        Value::RecordId(value) => value.to_string(),
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
