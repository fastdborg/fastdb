#![forbid(unsafe_code)]
#![deny(warnings)]

use std::io::Write;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn run(args: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fastdb"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn p4_cli_001_human_golden_has_deterministic_statement_boundaries() {
    let output = run(
        &[
            "--memory",
            "-c",
            "CREATE person:one SET name='One'; SELECT name FROM person:one",
        ],
        None,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "-- statement 1 --\n[\n  {\"id\": person:`one`, \"name\": \"One\"},\n]\n\
         -- statement 2 --\n[\n  {\"name\": \"One\"},\n]\n"
    );
}

#[test]
fn p4_cli_002_piped_multiline_uses_parser_completeness() {
    let output = run(
        &["--memory"],
        Some("CREATE note:one CONTENT {\n text: 'a; b',\n ok: true\n};\nSELECT * FROM note:one"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"text\": \"a; b\""), "{stdout}");
    assert!(stdout.contains("-- statement 2 --"), "{stdout}");
}

#[test]
fn p4_cli_003_json_success_and_error_are_one_enveloped_object() {
    let success = run(
        &[
            "--memory",
            "--output",
            "json",
            "--param",
            "name=\"One\"",
            "-c",
            "CREATE person:one SET name=$name",
        ],
        None,
    );
    assert!(success.status.success());
    let json: serde_json::Value = serde_json::from_slice(&success.stdout).unwrap();
    assert_eq!(json["$fastdb"]["v"], 1);
    assert_eq!(json["$fastdb"]["t"], "response");

    let failure = run(
        &["--memory", "--output", "json", "-c", "SELECT FROM person"],
        None,
    );
    assert_eq!(failure.status.code(), Some(1));
    assert!(failure.stdout.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&failure.stderr).unwrap();
    assert_eq!(json["$fastdb"]["t"], "error");
    assert_eq!(json["$fastdb"]["category"], "Parse");
    assert!(json["$fastdb"]["span"]["offset"].is_number());
}

#[test]
fn p4_cli_004_repeated_parameters_fail_without_echoing_values() {
    let output = run(
        &[
            "--memory",
            "--param",
            "secret=\"first\"",
            "--param",
            "secret=\"second\"",
            "-c",
            "SELECT * FROM person",
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("provided more than once"), "{stderr}");
    assert!(!stderr.contains("first") && !stderr.contains("second"));
}

#[test]
fn p4_cli_005_phase3_fixture_runs_in_memory_and_on_disk() {
    let fixture = include_str!("fixtures/phase3.fastdbql");
    let memory = run(&["--memory", "--output", "json"], Some(fixture));
    assert!(
        memory.status.success(),
        "{}",
        String::from_utf8_lossy(&memory.stderr)
    );

    let directory = tempdir().unwrap();
    let path = directory.path().join("phase 3 unicode 数据.fastdb");
    let disk = run(&[path.to_str().unwrap(), "--output", "json"], Some(fixture));
    assert!(
        disk.status.success(),
        "{}",
        String::from_utf8_lossy(&disk.stderr)
    );
    let reopen = run(
        &[
            path.to_str().unwrap(),
            "--output",
            "json",
            "-c",
            "SELECT * FROM person:one",
        ],
        None,
    );
    assert!(
        reopen.status.success(),
        "{}",
        String::from_utf8_lossy(&reopen.stderr)
    );
}
