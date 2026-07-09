//! The set of fields each list command can emit, and validation of `--fields`.
//!
//! These tables are the single source of truth. `schema` publishes them as
//! `output_fields`, and `--fields` validates against them, so the contract an
//! agent reads is exactly the contract the CLI enforces.
//!
//! A `--fields` request naming something outside the table is a usage error.
//! Silently dropping it, as this once did, hands back `{}` per row with exit 0,
//! which is indistinguishable from a successful query that found nothing.

/// A field name paired with its JSON type, as published in `unifi schema`.
pub type Field = (&'static str, &'static str);

pub const CLIENTS_LIST: &[Field] = &[
    ("name", "string"),
    ("mac", "string"),
    ("ip", "string"),
    ("type", "string"),
    ("ssid", "string"),
    ("signal", "integer"),
    ("uptime", "integer"),
    ("network", "string"),
    ("vlan", "integer"),
    ("tx_bytes", "integer"),
    ("rx_bytes", "integer"),
    ("blocked", "boolean"),
    ("connected_at", "string"),
];

pub const CLIENTS_SHOW: &[Field] = &[
    ("name", "string"),
    ("mac", "string"),
    ("ip", "string"),
    ("wired", "boolean"),
    ("uptime", "integer"),
    ("tx_bytes", "integer"),
    ("rx_bytes", "integer"),
    ("signal", "integer"),
    ("ssid", "string"),
    ("ap_mac", "string"),
    ("network", "string"),
    ("vlan", "integer"),
    ("blocked", "boolean"),
];

pub const DEVICES_LIST: &[Field] = &[
    ("name", "string"),
    ("model", "string"),
    ("mac", "string"),
    ("ip", "string"),
    ("state", "string"),
    ("firmware", "string"),
];

pub const EVENTS_LIST: &[Field] = &[
    ("key", "string"),
    ("msg", "string"),
    ("subsystem", "string"),
    ("time", "integer"),
    ("datetime", "string"),
];

pub const NETWORKS_LIST: &[Field] = &[
    ("name", "string"),
    ("vlan_id", "integer"),
    ("enabled", "boolean"),
    ("default", "boolean"),
];

/// A `--fields` request naming one or more unknown fields.
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidFields {
    pub unknown: Vec<String>,
    pub valid: Vec<&'static str>,
}

impl std::fmt::Display for InvalidFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let plural = if self.unknown.len() == 1 {
            "field"
        } else {
            "fields"
        };
        write!(
            f,
            "unknown {plural} in --fields: {}. Valid fields: {}",
            self.unknown.join(", "),
            self.valid.join(", ")
        )
    }
}

impl std::error::Error for InvalidFields {}

/// Field names of a table, in declaration order.
pub fn names(table: &[Field]) -> Vec<&'static str> {
    table.iter().map(|(n, _)| *n).collect()
}

/// Parse and validate a comma-separated `--fields` spec against a table.
///
/// Returns the requested names in the order given. Empty segments (`a,,b`) and
/// surrounding whitespace are tolerated; anything not in the table is an error.
pub fn validate(spec: &str, table: &[Field]) -> Result<Vec<String>, InvalidFields> {
    let valid = names(table);
    let requested: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut unknown: Vec<String> = requested
        .iter()
        .filter(|r| !valid.contains(*r))
        .map(|r| (*r).to_string())
        .collect();
    unknown.dedup();

    if unknown.is_empty() {
        Ok(requested.into_iter().map(str::to_string).collect())
    } else {
        Err(InvalidFields { unknown, valid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_single_known_field() {
        assert_eq!(validate("mac", CLIENTS_LIST).unwrap(), vec!["mac"]);
    }

    #[test]
    fn accepts_several_known_fields_preserving_order() {
        assert_eq!(
            validate("ip,mac,ssid", CLIENTS_LIST).unwrap(),
            vec!["ip", "mac", "ssid"]
        );
    }

    #[test]
    fn tolerates_whitespace_and_empty_segments() {
        assert_eq!(
            validate(" mac , , ip ", CLIENTS_LIST).unwrap(),
            vec!["mac", "ip"]
        );
    }

    #[test]
    fn rejects_an_unknown_field() {
        let err = validate("bogus", CLIENTS_LIST).unwrap_err();
        assert_eq!(err.unknown, vec!["bogus"]);
        assert!(err.valid.contains(&"ssid"));
    }

    #[test]
    fn rejects_an_unknown_field_mixed_with_known_ones() {
        let err = validate("mac,bogus,ip", CLIENTS_LIST).unwrap_err();
        assert_eq!(err.unknown, vec!["bogus"]);
    }

    #[test]
    fn reports_every_unknown_field() {
        let err = validate("bogus,mac,nope", CLIENTS_LIST).unwrap_err();
        assert_eq!(err.unknown, vec!["bogus", "nope"]);
    }

    #[test]
    fn an_all_empty_spec_selects_nothing_rather_than_erroring() {
        // `--fields ""` is a request for no fields, not an invalid field.
        assert!(validate("", CLIENTS_LIST).unwrap().is_empty());
        assert!(validate(" , ", CLIENTS_LIST).unwrap().is_empty());
    }

    #[test]
    fn error_message_names_the_offender_and_the_valid_set() {
        let msg = validate("bogus", CLIENTS_LIST).unwrap_err().to_string();
        assert!(msg.contains("bogus"), "{msg}");
        assert!(msg.contains("Valid fields:"), "{msg}");
        assert!(msg.contains("ssid"), "{msg}");
        assert!(msg.contains("field in --fields"), "{msg}");
    }

    #[test]
    fn error_message_pluralises() {
        let msg = validate("a,b", CLIENTS_LIST).unwrap_err().to_string();
        assert!(msg.contains("fields in --fields"), "{msg}");
    }

    #[test]
    fn every_table_has_unique_field_names() {
        for table in [
            CLIENTS_LIST,
            CLIENTS_SHOW,
            DEVICES_LIST,
            EVENTS_LIST,
            NETWORKS_LIST,
        ] {
            let mut seen = names(table);
            let before = seen.len();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(before, seen.len(), "duplicate field name in table");
        }
    }

    #[test]
    fn every_field_declares_a_json_type() {
        for table in [
            CLIENTS_LIST,
            CLIENTS_SHOW,
            DEVICES_LIST,
            EVENTS_LIST,
            NETWORKS_LIST,
        ] {
            for (name, ty) in table {
                assert!(
                    ["string", "integer", "boolean"].contains(ty),
                    "field {name} has unexpected type {ty}"
                );
            }
        }
    }

    #[test]
    fn clients_list_can_project_everything_clients_show_reports() {
        // A field visible for one client must be reachable in bulk, otherwise
        // answering "which SSID is each client on" costs one call per client.
        for (name, _) in CLIENTS_SHOW {
            if *name == "wired" || *name == "ap_mac" {
                continue; // `type` covers wired; ap_mac is detail-only
            }
            assert!(
                names(CLIENTS_LIST).contains(name),
                "clients list cannot project {name}, which clients show reports"
            );
        }
    }
}
