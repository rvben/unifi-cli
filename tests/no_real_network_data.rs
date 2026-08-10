//! Guards the repository against real network identifiers.
//!
//! Fixtures are written by copying what a live controller answered, so a real
//! MAC, a real LAN address or a real WAN address reaches git as a side effect
//! of making a test realistic. Review does not catch it: a plausible-looking
//! address is exactly what a fixture is supposed to contain, and once it is
//! committed it stays in the history.
//!
//! So the rule is enforced here instead of by convention. Addresses come from
//! the documentation ranges (RFC 5737: 192.0.2.0/24, 198.51.100.0/24,
//! 203.0.113.0/24) and MACs from an allowlist of prefixes no vendor is
//! assigned. Anything else fails, including a prefix that merely looks
//! synthetic, because deciding case by case is how the last one got in.
//!
//! This file is the one place allowed to name the patterns, so it excludes
//! itself from the scan.

use std::path::{Path, PathBuf};

/// MAC prefixes fixtures may use. Every one is either a locally administered
/// address or an obvious placeholder; none is an assigned vendor OUI.
const ALLOWED_MAC_PREFIXES: &[&str] = &[
    "aabbcc", // the house style for fixture MACs
    "112233", "223344", "ddeeff", // placeholder sequences
    "000000", "ffffff", // the unspecified and broadcast addresses
];

/// Addresses fixtures may use: the RFC 5737 documentation ranges, plus the
/// three addresses that mean something specific rather than naming a host.
const ALLOWED_IP_PREFIXES: &[&str] = &["192.0.2.", "198.51.100.", "203.0.113."];
const ALLOWED_IPS: &[&str] = &["127.0.0.1", "0.0.0.0", "255.255.255.255"];

/// Every MAC-shaped token in `text`, normalized to lowercase hex digits.
fn macs(text: &str) -> Vec<String> {
    let bytes: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i + 16 < bytes.len() + 1 {
        // A MAC is six hex pairs joined by a single separator, so it spans 17
        // characters. Anchor on a boundary or a run of hex digits either side
        // would match the tail of a longer token.
        let window: String = bytes[i..(i + 17).min(bytes.len())].iter().collect();
        if window.len() == 17 && is_mac(&window) {
            let before_is_hex = i > 0 && (bytes[i - 1].is_ascii_hexdigit() || bytes[i - 1] == ':');
            let after = bytes.get(i + 17);
            let after_is_hex =
                after.is_some_and(|c| c.is_ascii_hexdigit() || *c == ':' || *c == '-');
            if !before_is_hex && !after_is_hex {
                found.push(window.to_ascii_lowercase().replace([':', '-'], ""));
                i += 17;
                continue;
            }
        }
        i += 1;
    }
    found
}

fn is_mac(window: &str) -> bool {
    let sep = window.as_bytes()[2] as char;
    if sep != ':' && sep != '-' {
        return false;
    }
    window.chars().enumerate().all(|(idx, c)| {
        if idx % 3 == 2 {
            c == sep
        } else {
            c.is_ascii_hexdigit()
        }
    })
}

/// Every dotted-quad token in `text`.
fn ipv4s(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for candidate in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let parts: Vec<&str> = candidate.split('.').collect();
        if parts.len() != 4 {
            continue;
        }
        if parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u8>().is_ok())
        {
            found.push(candidate.to_string());
        }
    }
    found
}

fn scanned_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("README.md"), root.join("CHANGELOG.md")];
    for dir in ["src", "tests"] {
        collect_rs(&root.join(dir), &mut files);
    }
    let self_path = root.join("tests").join("no_real_network_data.rs");
    files.retain(|f| f != &self_path && f.exists());
    files
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_vendor_assigned_mac_addresses_are_committed() {
    // Positive control: a real Ubiquiti MAC must be rejected, or a detector
    // that silently matches nothing would report a clean repository.
    let probe = format!("mac: \"9c:05:{}:bc:06:43\"", "d6");
    let probe_hits = macs(&probe);
    assert_eq!(
        probe_hits.len(),
        1,
        "the MAC scanner found nothing in {probe}"
    );
    assert!(!ALLOWED_MAC_PREFIXES.contains(&&probe_hits[0][..6]));

    let mut offenders = Vec::new();
    for file in scanned_files() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for mac in macs(&text) {
            if !ALLOWED_MAC_PREFIXES.contains(&&mac[..6]) {
                offenders.push(format!("{}: {mac}", file.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "MAC addresses outside the fixture allowlist {ALLOWED_MAC_PREFIXES:?} \
         reached the repository. If one is real, replace it; if a new synthetic \
         prefix is wanted, add it to the allowlist deliberately:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn no_addresses_outside_the_documentation_ranges_are_committed() {
    // Positive control, assembled so this file holds no banned literal itself.
    let probe = format!("ip: \"10.{}.0.5\", gateway: \"192.168.1.1\"", 0);
    let probe_hits = ipv4s(&probe);
    assert_eq!(
        probe_hits.len(),
        2,
        "the address scanner found {probe_hits:?} in {probe}"
    );
    for hit in &probe_hits {
        assert!(!is_allowed_ip(hit), "{hit} must not be allowed");
    }

    let mut offenders = Vec::new();
    for file in scanned_files() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for ip in ipv4s(&text) {
            if !is_allowed_ip(&ip) {
                offenders.push(format!("{}: {ip}", file.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "addresses outside the RFC 5737 documentation ranges reached the \
         repository. Use 192.0.2.x, 198.51.100.x or 203.0.113.x:\n  {}",
        offenders.join("\n  ")
    );
}

fn is_allowed_ip(ip: &str) -> bool {
    ALLOWED_IPS.contains(&ip) || ALLOWED_IP_PREFIXES.iter().any(|p| ip.starts_with(p))
}

#[test]
fn the_scanners_read_the_files_they_claim_to() {
    let files = scanned_files();
    assert!(
        files.len() > 5,
        "the scan covers only {files:?}, so a clean result would prove nothing"
    );
    assert!(
        files.iter().any(|f| f.ends_with("mock_server.rs")),
        "the largest fixture file must be scanned: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.ends_with("no_real_network_data.rs")),
        "this file names the patterns and must exclude itself"
    );
}
