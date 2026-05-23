use std::collections::HashMap;

/// Domain-specific metadata that clap cannot express.
struct CommandMeta {
    output_fields: Option<&'static [&'static str]>,
    mutating: bool,
    note: Option<&'static str>,
}

/// Arg IDs that are global — excluded from per-command arg lists.
/// Includes both Cli-level args and per-command OutputConfig args (json, quiet).
const GLOBAL_ARG_IDS: &[&str] = &[
    "host",
    "api_key",
    "username",
    "password",
    "accept_invalid_certs",
    "profile",
    "json",
    "quiet",
    "help",
    "version",
];

fn command_metadata() -> HashMap<&'static str, CommandMeta> {
    let mut m = HashMap::new();
    let f =
        |fields: &'static [&'static str], mutating: bool, note: Option<&'static str>| CommandMeta {
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
    m.insert(
        "clients list",
        f(&["name", "mac", "ip", "type"], false, None),
    );
    m.insert(
        "clients show",
        f(
            &[
                "name", "mac", "ip", "wired", "uptime", "tx_bytes", "rx_bytes", "signal", "ssid",
                "ap_mac",
            ],
            false,
            None,
        ),
    );
    m.insert(
        "clients set-fixed-ip",
        f(&["status", "action", "mac", "ip", "name"], true, None),
    );
    m.insert("clients block", f(&["status", "action", "mac"], true, None));
    m.insert(
        "clients unblock",
        f(&["status", "action", "mac"], true, None),
    );
    m.insert("clients kick", f(&["status", "action", "mac"], true, None));
    m.insert(
        "clients top",
        f(
            &["name", "mac", "ip", "tx_bytes", "rx_bytes", "total_bytes"],
            false,
            None,
        ),
    );

    // devices
    m.insert(
        "devices list",
        f(
            &["name", "model", "mac", "ip", "state", "firmware"],
            false,
            None,
        ),
    );
    m.insert(
        "devices show",
        f(
            &[
                "name", "model", "mac", "ip", "state", "firmware", "uptime", "num_sta", "version",
            ],
            false,
            None,
        ),
    );
    m.insert(
        "devices restart",
        f(&["status", "action", "mac"], true, None),
    );
    m.insert(
        "devices locate",
        f(&["status", "action", "mac"], true, None),
    );
    m.insert(
        "devices ports",
        f(
            &[
                "port_idx",
                "name",
                "media",
                "up",
                "speed",
                "full_duplex",
                "poe_enable",
                "poe_power",
                "tx_bytes",
                "rx_bytes",
            ],
            false,
            None,
        ),
    );
    m.insert(
        "devices upgrade",
        f(&["status", "action", "mac"], true, None),
    );

    // networks / events / system
    m.insert(
        "networks",
        f(&["name", "vlan_id", "enabled", "default"], false, None),
    );
    m.insert(
        "events list",
        f(
            &["key", "msg", "subsystem", "time", "datetime"],
            false,
            None,
        ),
    );
    m.insert(
        "system health",
        f(
            &[
                "subsystem",
                "status",
                "num_sta",
                "num_ap",
                "num_switches",
                "wan_ip",
                "isp_name",
            ],
            false,
            None,
        ),
    );
    m.insert(
        "system info",
        f(&["hostname", "version", "timezone", "uptime"], false, None),
    );

    // protect
    m.insert(
        "protect cameras list",
        f(
            &[
                "id",
                "name",
                "mac",
                "state",
                "model_key",
                "mic_enabled",
                "video_mode",
            ],
            false,
            Some(
                "With --full: adds ip, type, firmware, recording, resolution, codec, uptime, wifi",
            ),
        ),
    );
    m.insert("protect cameras show", f(
        &["id", "name", "mac", "state", "model_key", "mic_enabled", "video_mode", "feature_flags"], false,
        Some("With --full: adds ip, type, firmware, uptime, channels, wifi, storage, recording settings"),
    ));
    m.insert(
        "protect rtsps list",
        f(&["high", "medium", "low", "package"], false, None),
    );
    m.insert(
        "protect rtsps create",
        f(&["status", "action", "camera_id", "streams"], true, None),
    );
    m.insert(
        "protect rtsps delete",
        f(&["status", "action", "camera_id", "qualities"], true, None),
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

fn extract_global_flags(cmd: &clap::Command) -> serde_json::Map<String, serde_json::Value> {
    let mut flags = serde_json::Map::new();
    for arg in cmd.get_arguments() {
        let id = arg.get_id().as_str();
        if !GLOBAL_ARG_IDS.contains(&id) || matches!(id, "help" | "version" | "json" | "quiet") {
            continue;
        }
        let key = match arg.get_long() {
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
        flags.insert(key, serde_json::Value::String(help));
    }
    // Synthetic flags added per-command by OutputConfig, not on the Cli struct
    flags.insert(
        "--json".into(),
        "Output as JSON (auto-enabled when piped)".into(),
    );
    flags.insert("--quiet".into(), "Suppress non-data output".into());
    flags
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
        args.push(serde_json::json!({
            "name": name,
            "required": arg.is_required_set(),
            "description": help,
        }));
    }
    args
}

fn walk_commands(
    cmd: &clap::Command,
    prefix: &str,
    metadata: &HashMap<&str, CommandMeta>,
    out: &mut serde_json::Map<String, serde_json::Value>,
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
            let mut entry = serde_json::Map::new();
            entry.insert("description".into(), desc.into());
            entry.insert("args".into(), extract_args(sub).into());
            if let Some(meta) = metadata.get(path.as_str()) {
                if let Some(fields) = meta.output_fields {
                    let arr: Vec<serde_json::Value> = fields.iter().map(|f| (*f).into()).collect();
                    entry.insert("output_fields".into(), arr.into());
                }
                if meta.mutating {
                    entry.insert("mutating".into(), true.into());
                }
                if let Some(note) = meta.note {
                    entry.insert("note".into(), note.into());
                }
            }
            out.insert(path, entry.into());
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
    let global_flags = extract_global_flags(&cmd);

    let mut commands = serde_json::Map::new();
    walk_commands(&cmd, "", &metadata, &mut commands);

    let schema = serde_json::json!({
        "name": cmd.get_name(),
        "version": env!("CARGO_PKG_VERSION"),
        "description": cmd.get_about().map(|h| h.to_string()).unwrap_or_default(),
        "global_flags": global_flags,
        "exit_codes": {
            "0": "success",
            "1": "general error",
            "2": "configuration error (missing host/api-key)",
            "3": "authentication error (401/403)",
            "4": "not found (404)",
            "5": "API error (server error)",
        },
        "commands": commands,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("failed to serialize schema")
    );
}
