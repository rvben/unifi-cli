pub mod api;
pub mod commands;
pub mod fields;
pub mod output;
pub mod tui;

/// Commands that refuse to act without confirmation: they ask on a terminal,
/// and exit `confirmation_required` when stdin is not one unless `--yes` is
/// passed.
///
/// One list, three consumers: `unifi schema` publishes it so an agent can tell
/// which invocations need `--yes` before it runs them, `main` gates these
/// commands on it, and the contract tests drive every entry to prove the gate
/// is really there. A mutating command is not automatically on this list;
/// `devices locate` only blinks an LED, and `clients set-fixed-ip` and
/// `protect rtsps create` add configuration rather than taking something away.
pub const CONFIRMATION_GATED_COMMANDS: &[&str] = &[
    "clients block",
    "clients unblock",
    "clients kick",
    "devices restart",
    "devices upgrade",
    "ports cycle",
    "protect rtsps delete",
];
