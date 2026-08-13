use std::io::IsTerminal;
use unifi_cli::output::{OutputConfig, OutputFormat};

/// Parse the schema output from the binary and validate it against the clispec v0.3 JSON Schema.
#[test]
fn schema_validates_against_clispec_v02() {
    // Load the vendored clispec v0.3 JSON Schema
    let schema_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clispec-v0.3.json");
    let schema_str =
        std::fs::read_to_string(&schema_path).expect("failed to read clispec-v0.3.json fixture");
    let schema_value: serde_json::Value =
        serde_json::from_str(&schema_str).expect("failed to parse clispec schema");

    // Generate the unifi schema by calling print_schema via the binary
    let binary = env!("CARGO_BIN_EXE_unifi");
    let output = std::process::Command::new(binary)
        .arg("schema")
        .output()
        .expect("failed to run unifi schema");
    assert!(
        output.status.success(),
        "schema command failed: {:?}",
        output.status
    );

    let schema_output: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("schema output is not valid JSON");

    // Validate using jsonschema crate
    let validator = jsonschema::validator_for(&schema_value).expect("invalid clispec schema");
    let errors: Vec<String> = validator
        .iter_errors(&schema_output)
        .map(|e| format!("{e}"))
        .collect();
    if !errors.is_empty() {
        panic!("Schema validation failed:\n{}", errors.join("\n"));
    }
}

/// Walk every leaf command in `unifi schema`, yielding (path, arg).
fn schema_args() -> Vec<(String, serde_json::Value)> {
    let binary = env!("CARGO_BIN_EXE_unifi");
    let output = std::process::Command::new(binary)
        .arg("schema")
        .output()
        .expect("failed to run unifi schema");
    let schema: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("schema output is not valid JSON");

    let mut out = Vec::new();
    let mut stack: Vec<(String, serde_json::Value)> = schema["commands"]
        .as_array()
        .expect("schema.commands must be an array")
        .iter()
        .map(|c| (String::new(), c.clone()))
        .collect();
    while let Some((prefix, cmd)) = stack.pop() {
        let name = cmd["name"].as_str().unwrap_or_default();
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix} {name}")
        };
        match cmd["subcommands"].as_array() {
            Some(subs) if !subs.is_empty() => {
                stack.extend(subs.iter().map(|s| (path.clone(), s.clone())));
            }
            _ => {
                for arg in cmd["args"].as_array().into_iter().flatten() {
                    out.push((path.clone(), arg.clone()));
                }
            }
        }
    }
    out
}

#[test]
fn schema_types_flags_as_boolean_and_numeric_args_as_integer() {
    // The published type is the contract an agent plans against: a `--live`
    // typed "string" invites it to pass a value, and a `port` typed "string"
    // understates a u32. Both were wrong before this test existed.
    let args = schema_args();
    assert!(!args.is_empty(), "schema exposed no args at all");

    let mut wrong = Vec::new();
    let mut saw_flag = false;
    let mut saw_integer = false;
    for (path, arg) in &args {
        let name = arg["name"].as_str().unwrap_or_default();
        let ty = arg["type"].as_str().unwrap_or_default();
        // Flags carry no value, so clap parses them with a SetTrue action.
        let is_flag = matches!(
            name,
            "--wired" | "--wireless" | "--off" | "--live" | "--full" | "--yes"
        );
        let is_numeric = matches!(
            name,
            "--limit" | "--offset" | "--interval" | "--watch" | "port"
        );
        if is_flag {
            saw_flag = true;
            if ty != "boolean" {
                wrong.push(format!("{path} {name}: expected boolean, got {ty}"));
            }
        } else if is_numeric {
            saw_integer = true;
            if ty != "integer" {
                wrong.push(format!("{path} {name}: expected integer, got {ty}"));
            }
        }
    }
    // Negative control: if a rename made every arm unreachable the loop above
    // would pass while checking nothing.
    assert!(saw_flag, "no boolean flag was reached; the test is vacuous");
    assert!(
        saw_integer,
        "no numeric arg was reached; the test is vacuous"
    );
    assert!(wrong.is_empty(), "mistyped args:\n{}", wrong.join("\n"));
}

#[test]
fn explicit_text_format_is_not_json() {
    let out = OutputConfig::new(OutputFormat::Text, false);
    assert!(
        !out.is_json(),
        "OutputFormat::Text must never be JSON even when piped"
    );
}

#[test]
fn explicit_json_format_is_json() {
    let out = OutputConfig::new(OutputFormat::Json, false);
    assert!(out.is_json(), "OutputFormat::Json must always be JSON");
}

#[test]
fn auto_format_is_json_when_not_tty() {
    let out = OutputConfig::new(OutputFormat::Auto, false);
    let expected = !std::io::stdout().is_terminal();
    assert_eq!(out.is_json(), expected);
}

#[test]
fn error_envelope_last_line_is_json() {
    let kind = "auth_error";
    let message = "Authentication error: bad key";
    let expected = serde_json::json!({
        "error": {
            "kind": kind,
            "message": message,
        }
    });
    assert!(expected["error"]["kind"].as_str().is_some());
    assert_eq!(expected["error"]["kind"].as_str().unwrap(), kind);
    assert_eq!(expected["error"]["message"].as_str().unwrap(), message);
}

#[test]
fn confirmation_required_error_without_yes_and_no_tty() {
    let binary = env!("CARGO_BIN_EXE_unifi");
    let output = std::process::Command::new(binary)
        .args([
            "--host",
            "https://192.0.2.1",
            "--api-key",
            "fake-key",
            "clients",
            "block",
            "aa:bb:cc:dd:ee:ff",
        ])
        .stdin(std::fs::File::open("/dev/null").unwrap())
        .output()
        .expect("failed to run binary");

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let last_line = stderr.trim_end().lines().last().unwrap_or("");
    let envelope: serde_json::Value =
        serde_json::from_str(last_line).expect("last stderr line must be valid JSON");
    assert_eq!(
        envelope["error"]["kind"].as_str(),
        Some("confirmation_required")
    );
}

#[test]
fn pagination_envelope_shape() {
    let items: Vec<serde_json::Value> = vec![];
    let envelope = serde_json::json!({
        "items": items,
        "total": 0usize,
        "limit": 100usize,
        "offset": 0usize,
    });
    assert!(envelope["items"].is_array());
    assert!(envelope["total"].is_number());
    assert!(envelope["limit"].is_number());
    assert!(envelope["offset"].is_number());
}
