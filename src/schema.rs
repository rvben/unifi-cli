use std::collections::HashMap;

use unifi_cli::fields;

/// Domain-specific metadata that clap cannot express.
struct CommandMeta {
    output_fields: Option<&'static [(&'static str, &'static str)]>,
    mutating: bool,
    note: Option<&'static str>,
}

/// Arg IDs that are global - excluded from per-command arg lists.
/// Includes both Cli-level args and per-command OutputConfig args (output, quiet).
const GLOBAL_ARG_IDS: &[&str] = &[
    "host",
    "api_key",
    "username",
    "password",
    "accept_invalid_certs",
    "profile",
    "json",
    "output",
    "quiet",
    "help",
    "version",
    "yes",
];

fn command_metadata() -> HashMap<&'static str, CommandMeta> {
    let mut m = HashMap::new();
    let f = |fields: &'static [(&'static str, &'static str)],
             mutating: bool,
             note: Option<&'static str>| CommandMeta {
        output_fields: Some(fields),
        mutating,
        note,
    };
    let n = |note: &'static str| CommandMeta {
        output_fields: None,
        mutating: false,
        note: Some(note),
    };

    // clients
    m.insert("clients list", f(fields::CLIENTS_LIST, false, None));
    m.insert("clients show", f(fields::CLIENTS_SHOW, false, None));
    m.insert(
        "clients set-fixed-ip",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
                ("ip", "string"),
                ("name", "string"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "clients block",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "clients unblock",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "clients kick",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "clients top",
        f(
            &[
                ("name", "string"),
                ("mac", "string"),
                ("ip", "string"),
                ("tx_bytes", "integer"),
                ("rx_bytes", "integer"),
                ("total_bytes", "integer"),
            ],
            false,
            None,
        ),
    );

    // devices
    m.insert("devices list", f(fields::DEVICES_LIST, false, None));
    m.insert(
        "devices show",
        f(
            &[
                ("name", "string"),
                ("model", "string"),
                ("mac", "string"),
                ("ip", "string"),
                ("state", "string"),
                ("firmware", "string"),
                ("uptime", "integer"),
                ("num_sta", "integer"),
                ("version", "string"),
            ],
            false,
            None,
        ),
    );
    m.insert(
        "devices restart",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "devices locate",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "devices ports",
        f(
            fields::PORTS_LIST,
            false,
            Some("Alias for `ports list`; returns a bare JSON array for backward compatibility."),
        ),
    );
    m.insert(
        "devices upgrade",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("mac", "string"),
            ],
            true,
            None,
        ),
    );

    // ports
    m.insert("ports list", f(fields::PORTS_LIST, false, None));
    m.insert(
        "ports show",
        f(
            &[
                ("device_mac", "string"),
                ("device_name", "string"),
                ("port_idx", "integer"),
                ("name", "string"),
                ("media", "string"),
                ("up", "boolean"),
                ("speed", "integer"),
                ("full_duplex", "boolean"),
                ("autoneg", "boolean"),
                ("enable", "boolean"),
                ("is_uplink", "boolean"),
                ("stp_state", "string"),
                ("port_poe", "boolean"),
                ("poe_enable", "boolean"),
                ("poe_mode", "string"),
                ("poe_class", "string"),
                ("poe_power", "number"),
                ("poe_voltage", "number"),
                ("poe_current", "number"),
                ("poe_good", "boolean"),
                ("attached_mac", "string"),
                ("tx_bytes", "integer"),
                ("rx_bytes", "integer"),
                ("tx_errors", "integer"),
                ("rx_errors", "integer"),
            ],
            false,
            None,
        ),
    );

    // networks / events / system
    m.insert("networks list", f(fields::NETWORKS_LIST, false, None));
    m.insert("events list", f(fields::EVENTS_LIST, false, None));
    m.insert(
        "system health",
        f(
            &[
                ("subsystem", "string"),
                ("status", "string"),
                ("num_sta", "integer"),
                ("num_ap", "integer"),
                ("num_switches", "integer"),
                ("wan_ip", "string"),
                ("isp_name", "string"),
            ],
            false,
            None,
        ),
    );
    m.insert(
        "system info",
        f(
            &[
                ("hostname", "string"),
                ("version", "string"),
                ("timezone", "string"),
                ("uptime", "integer"),
            ],
            false,
            None,
        ),
    );

    // protect
    m.insert(
        "protect cameras list",
        f(
            &[
                ("id", "string"),
                ("name", "string"),
                ("mac", "string"),
                ("state", "string"),
                ("model_key", "string"),
                ("mic_enabled", "boolean"),
                ("video_mode", "string"),
            ],
            false,
            Some(
                "With --full: adds ip, type, firmware, recording, resolution, codec, uptime, wifi",
            ),
        ),
    );
    m.insert(
        "protect cameras show",
        f(
            &[
                ("id", "string"),
                ("name", "string"),
                ("mac", "string"),
                ("state", "string"),
                ("model_key", "string"),
                ("mic_enabled", "boolean"),
                ("video_mode", "string"),
                ("feature_flags", "object"),
            ],
            false,
            Some(
                "With --full: adds ip, type, firmware, uptime, channels, wifi, storage, recording settings",
            ),
        ),
    );
    m.insert(
        "protect rtsps list",
        f(
            &[
                ("high", "string"),
                ("medium", "string"),
                ("low", "string"),
                ("package", "string"),
            ],
            false,
            None,
        ),
    );
    m.insert(
        "protect rtsps create",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("camera_id", "string"),
                ("streams", "object"),
            ],
            true,
            None,
        ),
    );
    m.insert(
        "protect rtsps delete",
        f(
            &[
                ("status", "string"),
                ("action", "string"),
                ("camera_id", "string"),
                ("qualities", "string[]"),
            ],
            true,
            None,
        ),
    );

    // utility commands
    m.insert("completions", n("Does not require --host or --api-key"));
    m.insert(
        "config init",
        n("Does not require --host or --api-key. Supports named profiles."),
    );
    m.insert(
        "config check",
        n("Requires --host and --api-key (or config file)."),
    );
    m.insert(
        "tui",
        n("Interactive TUI. Keys: q quit, s sort, tab focus, / filter, up/down scroll"),
    );

    m
}

fn build_global_args(cmd: &clap::Command) -> Vec<serde_json::Value> {
    let mut args = Vec::new();

    // Emit global flags derived from the clap command tree
    let included = &[
        "host",
        "api_key",
        "username",
        "password",
        "accept_invalid_certs",
        "profile",
    ];
    for arg in cmd.get_arguments() {
        let id = arg.get_id().as_str();
        if !included.contains(&id) {
            continue;
        }
        let name = match arg.get_long() {
            Some(long) => {
                if let Some(names) = arg.get_value_names() {
                    let vs: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
                    format!("--{long} <{}>", vs.join(", "))
                } else {
                    format!("--{long}")
                }
            }
            None => continue,
        };
        let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
        args.push(serde_json::json!({
            "name": name,
            "type": "string",
            "required": false,
            "description": help,
        }));
    }

    // Synthetic global args (added per-command via OutputConfig, not on the Cli struct directly)
    args.push(serde_json::json!({
        "name": "--output",
        "type": "string",
        "required": false,
        "enum": ["auto", "text", "json"],
        "default": "auto",
        "description": "Output format: auto (TTY detection), text, or json. Alias: --json for json.",
    }));
    args.push(serde_json::json!({
        "name": "--quiet",
        "type": "boolean",
        "required": false,
        "description": "Suppress non-data output",
    }));
    args.push(serde_json::json!({
        "name": "--yes",
        "type": "boolean",
        "required": false,
        "description": "Skip confirmation prompt for destructive commands (required without a TTY)",
    }));
    args
}

fn infer_arg_type(arg: &clap::Arg) -> &'static str {
    let id = arg.get_id().as_str();
    // Boolean flags (no value name = flag/switch)
    if arg.get_value_names().is_none_or(|v| v.is_empty()) {
        return "boolean";
    }
    // Known integer args by id
    match id {
        "limit" | "offset" | "interval" => "integer",
        _ => "string",
    }
}

fn extract_args(cmd: &clap::Command) -> Vec<serde_json::Value> {
    let mut args = Vec::new();
    for arg in cmd.get_arguments() {
        let id = arg.get_id().as_str();
        if GLOBAL_ARG_IDS.contains(&id) || id == "help" || id == "version" {
            continue;
        }
        let name = if arg.is_positional() {
            id.to_string()
        } else if let Some(long) = arg.get_long() {
            format!("--{long}")
        } else if let Some(short) = arg.get_short() {
            format!("-{short}")
        } else {
            continue;
        };
        let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
        let arg_type = infer_arg_type(arg);
        let mut obj = serde_json::json!({
            "name": name,
            "type": arg_type,
            "required": arg.is_required_set(),
            "description": help,
        });
        if let Some(default) = arg.get_default_values().first() {
            obj["default"] = serde_json::Value::String(default.to_string_lossy().into_owned());
        }
        args.push(obj);
    }
    args
}

fn walk_commands(
    cmd: &clap::Command,
    prefix: &str,
    metadata: &HashMap<&str, CommandMeta>,
    out: &mut Vec<serde_json::Value>,
) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix} {name}")
        };

        // If this command has its own subcommands, recurse; otherwise emit a leaf entry
        let children: Vec<_> = sub
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .collect();
        if !children.is_empty() {
            walk_commands(sub, &path, metadata, out);
        } else {
            let desc = sub.get_about().map(|h| h.to_string()).unwrap_or_default();
            let mut entry = serde_json::json!({
                "name": path,
                "description": desc,
                "mutating": false,
                "args": extract_args(sub),
            });
            if let Some(meta) = metadata.get(path.as_str()) {
                if let Some(fields) = meta.output_fields {
                    let arr: Vec<serde_json::Value> = fields
                        .iter()
                        .map(|(fname, ftype)| serde_json::json!({"name": fname, "type": ftype}))
                        .collect();
                    entry["output_fields"] = arr.into();
                }
                entry["mutating"] = meta.mutating.into();
                if let Some(note) = meta.note {
                    entry["note"] = note.into();
                }
            }
            out.push(entry);
        }
    }
}

/// Generate and print the full CLI schema as JSON.
///
/// Command structure and argument definitions are derived from the clap
/// `Command` tree (single source of truth). Domain-specific metadata
/// (`output_fields`, `mutating`, `note`) comes from `command_metadata()`.
pub fn print_schema(cmd: clap::Command) {
    let metadata = command_metadata();
    let global_args = build_global_args(&cmd);

    let mut commands: Vec<serde_json::Value> = Vec::new();
    walk_commands(&cmd, "", &metadata, &mut commands);

    let schema = serde_json::json!({
        "clispec": "0.2",
        "name": cmd.get_name(),
        "version": env!("CARGO_PKG_VERSION"),
        "description": cmd.get_about().map(|h| h.to_string()).unwrap_or_default(),
        "global_args": global_args,
        "commands": commands,
        "errors": [
            {
                "kind": "general_error",
                "exit_code": 1,
                "retryable": false,
                "description": "Unspecified error",
            },
            {
                "kind": "config_error",
                "exit_code": 2,
                "retryable": false,
                "description": "Missing or invalid configuration (host/api-key not set)",
            },
            {
                "kind": "confirmation_required",
                "exit_code": 2,
                "retryable": false,
                "description": "Destructive command requires --yes flag when stdin is not a terminal",
            },
            {
                "kind": "auth_error",
                "exit_code": 3,
                "retryable": false,
                "description": "Authentication or authorization failure (401/403)",
            },
            {
                "kind": "not_found",
                "exit_code": 4,
                "retryable": false,
                "description": "Requested resource not found (404)",
            },
            {
                "kind": "api_error",
                "exit_code": 5,
                "retryable": true,
                "description": "API returned a server-side error (5xx)",
            },
            {
                "kind": "conflict",
                "exit_code": 6,
                "retryable": false,
                "description": "Resource already exists with incompatible configuration",
            },
        ],
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("failed to serialize schema")
    );
}
