//! Contract tests that drive the real binary.
//!
//! These guard behaviour an agent depends on: that a bad `--fields` request is
//! refused rather than silently answered with empty objects, and that the
//! subcommand surface is consistent across `clients`, `devices`, `events` and
//! `networks`.
//!
//! Every command here either short-circuits before any HTTP (argument
//! validation, `--help`) or is pointed at an unroutable host, so the suite
//! never touches a controller.

use std::process::Command;

fn unifi() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_unifi"));
    // Never read the developer's real ~/.config/unifi/config.toml.
    cmd.arg("--host").arg("127.0.0.1:1");
    cmd.arg("--api-key").arg("not-a-real-key");
    cmd
}

/// The structured error envelope is the last line of stderr.
fn error_envelope(stderr: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stderr);
    let last = text
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or_default();
    serde_json::from_str(last)
        .unwrap_or_else(|e| panic!("last stderr line is not a JSON envelope: {last:?} ({e})"))
}

// --- `--fields` must reject unknown fields, not silently drop them ---

#[test]
fn clients_list_rejects_unknown_field() {
    let out = unifi()
        .args(["clients", "list", "--fields", "bogus"])
        .output()
        .expect("failed to run binary");

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected usage exit code 2, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "stdout must stay clean on error, got: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    let envelope = error_envelope(&out.stderr);
    let message = envelope["error"]["message"].as_str().unwrap_or_default();
    assert_eq!(envelope["error"]["kind"], "config_error");
    assert!(
        message.contains("bogus"),
        "error should name the offending field, got: {message}"
    );
    assert!(
        message.contains("ssid"),
        "error should list the valid fields, got: {message}"
    );
}

#[test]
fn clients_list_rejects_unknown_field_among_valid_ones() {
    let out = unifi()
        .args(["clients", "list", "--fields", "mac,bogus,ip"])
        .output()
        .expect("failed to run binary");

    assert_eq!(out.status.code(), Some(2));
    let envelope = error_envelope(&out.stderr);
    let message = envelope["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("bogus"), "got: {message}");
    assert!(
        !message.contains("'mac'"),
        "valid fields must not be reported as invalid, got: {message}"
    );
}

#[test]
fn devices_list_rejects_unknown_field() {
    let out = unifi()
        .args(["devices", "list", "--fields", "nope"])
        .output()
        .expect("failed to run binary");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        error_envelope(&out.stderr)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("nope")
    );
}

#[test]
fn events_list_rejects_unknown_field() {
    let out = unifi()
        .args(["events", "list", "--fields", "nope"])
        .output()
        .expect("failed to run binary");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        error_envelope(&out.stderr)["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("nope")
    );
}

#[test]
fn fields_validation_happens_before_any_network_call() {
    // --host points at a closed port. If validation ran after connecting we
    // would see a transport error (exit 1), not a usage error (exit 2).
    let out = unifi()
        .args(["clients", "list", "--fields", "bogus"])
        .output()
        .expect("failed to run binary");
    assert_eq!(
        out.status.code(),
        Some(2),
        "validation must precede the HTTP request"
    );
}

#[test]
fn clients_list_accepts_every_documented_field() {
    // A field that `clients show` reports must also be selectable in bulk.
    for field in [
        "name",
        "mac",
        "ip",
        "type",
        "ssid",
        "signal",
        "uptime",
        "network",
        "vlan",
        "tx_bytes",
        "rx_bytes",
        "blocked",
        "connected_at",
    ] {
        let out = unifi()
            .args(["clients", "list", "--fields", field])
            .output()
            .expect("failed to run binary");
        // The host is unroutable, so a valid field reaches the network layer and
        // fails there. What must never happen is a usage error.
        assert_ne!(
            out.status.code(),
            Some(2),
            "field {field:?} was rejected as invalid: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// --- `ports` surface ---

#[test]
fn ports_list_rejects_unknown_field() {
    let out = unifi()
        .args(["ports", "list", "--fields", "bogus"])
        .output()
        .expect("failed to run binary");

    assert_eq!(
        out.status.code(),
        Some(2),
        "expected usage exit code 2, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        error_envelope(&out.stderr)["error"]["kind"].as_str(),
        Some("config_error")
    );
}

#[test]
fn devices_ports_and_ports_list_are_the_same_command() {
    // Both spellings must accept a MAC and reach the network layer, not fail
    // at argument parsing. Pointed at an unroutable host, so no controller.
    for args in [
        vec!["devices", "ports", "aa:bb:cc:dd:ee:ff"],
        vec!["ports", "list", "aa:bb:cc:dd:ee:ff"],
    ] {
        let out = unifi().args(&args).output().expect("failed to run binary");
        assert_ne!(
            out.status.code(),
            Some(2),
            "{args:?} must not be a usage error, stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn ports_live_requires_a_mac() {
    let out = unifi()
        .args(["ports", "list", "--live"])
        .output()
        .expect("failed to run binary");

    assert_eq!(
        out.status.code(),
        Some(2),
        "--live without a MAC must be a usage error"
    );
}

// --- subcommand surface consistency ---

#[test]
fn networks_has_a_list_subcommand() {
    let out = unifi()
        .args(["networks", "list", "--help"])
        .output()
        .expect("failed to run binary");
    assert!(
        out.status.success(),
        "`unifi networks list` should exist like clients/devices/events; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn networks_without_subcommand_still_lists() {
    let out = unifi()
        .args(["networks", "--help"])
        .output()
        .expect("failed to run binary");
    assert!(out.status.success());
}

fn schema_json() -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_unifi"))
        .arg("schema")
        .output()
        .expect("failed to run binary");
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).expect("schema is JSON")
}

fn schema_command<'a>(schema: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    schema["commands"]
        .as_array()
        .expect("schema.commands is an array")
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("schema has no command named {name:?}"))
}

#[test]
fn schema_advertises_networks_list() {
    let schema = schema_json();
    let names: Vec<&str> = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(
        names.contains(&"networks list"),
        "schema must advertise `networks list`, found: {names:?}"
    );
}

/// The schema is the contract. `--fields` must accept exactly what it publishes,
/// otherwise an agent reading `output_fields` gets a usage error for a field the
/// CLI itself advertised.
#[test]
fn every_published_output_field_is_accepted_by_fields() {
    let schema = schema_json();
    for command in ["clients list", "devices list", "events list"] {
        let fields: Vec<String> = schema_command(&schema, command)["output_fields"]
            .as_array()
            .unwrap_or_else(|| panic!("{command} publishes output_fields"))
            .iter()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect();
        assert!(!fields.is_empty());

        let spec = fields.join(",");
        let mut args: Vec<&str> = command.split(' ').collect();
        args.push("--fields");
        args.push(&spec);

        let out = unifi().args(&args).output().expect("failed to run binary");
        assert_ne!(
            out.status.code(),
            Some(2),
            "`{command}` rejected its own published output_fields ({spec}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// One invocation per gated command, with the argument set it needs.
const GATED_INVOCATIONS: &[(&str, &[&str])] = &[
    ("clients block", &["aa:bb:cc:dd:ee:ff"]),
    ("clients unblock", &["aa:bb:cc:dd:ee:ff"]),
    ("clients kick", &["aa:bb:cc:dd:ee:ff"]),
    ("devices restart", &["aa:bb:cc:dd:ee:ff"]),
    ("devices upgrade", &["aa:bb:cc:dd:ee:ff"]),
    ("ports cycle", &["aa:bb:cc:dd:ee:ff", "5"]),
    ("protect rtsps delete", &["front-door"]),
];

/// Every command the schema publishes as `confirmation_required` must refuse to
/// act when there is no `--yes` and no TTY to ask on, and must refuse locally.
/// The host here is unroutable, so a `confirmation_required` envelope is also
/// proof that nothing was sent to the controller: a command that fell through
/// to HTTP would report a connection failure instead.
#[test]
fn every_confirmation_gated_command_refuses_without_yes_and_no_tty() {
    let covered: Vec<&str> = GATED_INVOCATIONS.iter().map(|(c, _)| *c).collect();
    for command in unifi_cli::CONFIRMATION_GATED_COMMANDS {
        assert!(
            covered.contains(command),
            "`{command}` is confirmation-gated but has no invocation in GATED_INVOCATIONS"
        );
    }
    assert_eq!(
        covered.len(),
        unifi_cli::CONFIRMATION_GATED_COMMANDS.len(),
        "GATED_INVOCATIONS lists a command that is not in CONFIRMATION_GATED_COMMANDS"
    );

    for (command, extra) in GATED_INVOCATIONS {
        let mut args: Vec<&str> = command.split(' ').collect();
        args.extend_from_slice(extra);

        let out = unifi()
            .args(&args)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("failed to run binary");

        assert_eq!(
            out.status.code(),
            Some(2),
            "`{command}` should exit 2 without --yes, stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            error_envelope(&out.stderr)["error"]["kind"].as_str(),
            Some("confirmation_required"),
            "`{command}` should report confirmation_required, stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// A mutating command that is not gated must not start asking for confirmation:
/// it reaches the controller (and fails on the unroutable host) instead. This is
/// the negative control for the test above, which would otherwise still pass if
/// every command refused everything.
#[test]
fn ungated_mutating_commands_do_not_ask_for_confirmation() {
    for args in [
        vec!["devices", "locate", "aa:bb:cc:dd:ee:ff"],
        vec!["clients", "set-fixed-ip", "aa:bb:cc:dd:ee:ff", "192.0.2.10"],
    ] {
        let out = unifi()
            .args(&args)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("failed to run binary");

        let kind = error_envelope(&out.stderr)["error"]["kind"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert_ne!(
            kind,
            "confirmation_required",
            "`{}` is not in CONFIRMATION_GATED_COMMANDS but asked for confirmation",
            args.join(" ")
        );
    }
}
