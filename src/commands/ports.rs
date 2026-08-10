use owo_colors::OwoColorize;

use crate::api::{
    ApiError, DeviceWithPorts, LegacyClient, PortEntry, UnifiClient, format_bytes, format_mac,
    normalize_mac,
};
use crate::output::{OutputConfig, use_color};

/// One port, flattened with the device that owns it. Every `ports` subcommand
/// renders these, so the filtered and unfiltered listings cannot drift apart.
pub struct PortRow<'a> {
    pub device_mac: String,
    pub device_name: String,
    pub port: &'a PortEntry,
}

pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    /// Field names already validated against `fields::PORTS_LIST`.
    pub fields: Option<Vec<String>>,
}

/// Derive a port row's formatted device MAC and display name: `name` ->
/// `model` -> `name_fallback`. Shared by `show` and `collect_rows_with_fallback`
/// so this three-tier fallback can never drift between the two call sites.
fn device_identity(device: &DeviceWithPorts, name_fallback: &str) -> (String, String) {
    let device_mac = device
        .mac
        .as_deref()
        .map(format_mac)
        .unwrap_or_else(|| "-".into());
    let device_name = device
        .name
        .as_deref()
        .or(device.model.as_deref())
        .unwrap_or(name_fallback)
        .to_string();
    (device_mac, device_name)
}

/// Flatten devices into port rows, skipping devices with no port table.
/// `ports list` / `ports find` fall back to `"-"` for a device with neither
/// `name` nor `model`.
pub fn collect_rows(devices: &[DeviceWithPorts]) -> Vec<PortRow<'_>> {
    collect_rows_with_fallback(devices, "-")
}

/// Same flattening as `collect_rows`, but with a caller-chosen fallback for a
/// device that has neither `name` nor `model`. `devices ports` keeps its
/// historical `"Device"` label here rather than duplicating the whole
/// flattening loop just to change one fallback string.
pub fn collect_rows_with_fallback<'a>(
    devices: &'a [DeviceWithPorts],
    name_fallback: &str,
) -> Vec<PortRow<'a>> {
    let mut rows = Vec::new();
    for d in devices {
        if d.port_table.is_empty() {
            continue;
        }
        let (device_mac, device_name) = device_identity(d, name_fallback);
        for port in &d.port_table {
            rows.push(PortRow {
                device_mac: device_mac.clone(),
                device_name: device_name.clone(),
                port,
            });
        }
    }
    rows
}

/// The `PORTS_LIST` field set for one row.
pub fn row_json(row: &PortRow) -> serde_json::Value {
    let p = row.port;
    serde_json::json!({
        "device_mac": row.device_mac,
        "device_name": row.device_name,
        "port_idx": p.port_idx,
        "name": p.name,
        "media": p.media,
        "up": p.up,
        "speed": p.speed,
        "full_duplex": p.full_duplex,
        "poe_enable": p.poe_enable,
        "poe_power": p.poe_power,
        "port_poe": p.port_poe,
        "tx_bytes": p.tx_bytes,
        "rx_bytes": p.rx_bytes,
    })
}

/// Apply a validated `--fields` projection in place.
pub fn project(value: &mut serde_json::Value, fields: &Option<Vec<String>>) {
    if let Some(keep) = fields
        && let Some(map) = value.as_object_mut()
    {
        map.retain(|k, _| keep.iter().any(|f| f == k));
    }
}

/// Human-readable PoE cell: draw in watts, or on/off/- .
fn poe_cell(p: &PortEntry) -> String {
    if p.poe_enable {
        match p.poe_power {
            Some(w) if w > 0.0 => format!("{w:.1}W"),
            _ => "on".into(),
        }
    } else if p.port_poe {
        "off".into()
    } else {
        "-".into()
    }
}

fn speed_cell(p: &PortEntry) -> String {
    if !p.up {
        return "down".into();
    }
    match p.speed {
        Some(s) => format!("{s}{}", if p.full_duplex { "FD" } else { "HD" }),
        None => "up".into(),
    }
}

/// Split a port's `last_connection` into what is attached *now* and what the
/// record names regardless of state, as `(attached_mac, last_seen_mac)`.
///
/// The controller keeps a port's `last_connection` after the device is
/// unplugged, marking it `connected: false`. A stale record therefore names a
/// device that may have been gone for months, and reporting it as the
/// attachment would tell an operator a port is in use moments before they cut
/// its power. `attached_mac` is `None` unless the record is live; the MAC
/// itself stays available as `last_seen_mac` so the history is not thrown
/// away. `cycle_summary` applies the same rule to the confirmation prompt.
fn attachment(p: &PortEntry) -> (Option<String>, Option<String>) {
    let lc = match p.last_connection.as_ref() {
        Some(lc) => lc,
        None => return (None, None),
    };
    let mac = lc.mac.as_deref().map(format_mac);
    let connected = lc.connected.unwrap_or(false);
    (connected.then(|| mac.clone()).flatten(), mac)
}

/// Right-pad a table cell to `width` visible columns, deriving the padding
/// from `plain`'s length rather than `rendered`'s.
///
/// This exists because `format!("{:<width$}", rendered)` counts *bytes*: a
/// coloured cell like `"up".green()` renders as `"\x1b[32mup\x1b[39m"`, 12
/// bytes for 2 visible characters, which `{:<6}` sees as already over width
/// and pads with nothing. Every coloured cell in this module's tables must
/// go through this instead of a `{:<N}` specifier.
///
/// `rendered` is `plain` itself on the uncoloured path (see call sites), so
/// that path pads identically to the `{:<width$}` it replaces, and this must
/// stay true, since existing tests assert on uncoloured output byte-for-byte.
fn pad_visible(rendered: &str, plain: &str, width: usize) -> String {
    let pad = width.saturating_sub(plain.len());
    format!("{rendered}{}", " ".repeat(pad))
}

/// "port" for exactly one row, "ports" otherwise: the row-count trailer's
/// singular/plural noun.
fn port_noun(count: usize) -> &'static str {
    if count == 1 { "port" } else { "ports" }
}

/// Device column width, in characters. Callers that paginate must compute
/// this from the full result set, not just the page handed to `render_text`.
/// Otherwise two `--offset` pages of the same query can render the column at
/// different widths.
pub fn device_col_width(rows: &[&PortRow]) -> usize {
    rows.iter()
        .map(|r| r.device_name.len())
        .max()
        .unwrap_or(6)
        .max(6)
        + 2
}

/// Render rows as a table. `show_device_col` is true only for the unfiltered
/// listing; the filtered table stays byte-identical to what `devices ports`
/// has always printed. `dev_w` is the Device column width; pass
/// `device_col_width` of the *full* result set, not just `rows`, so a
/// paginated caller renders a stable width across pages.
pub fn render_text(rows: &[&PortRow], show_device_col: bool, dev_w: usize, out: &OutputConfig) {
    render_rows(rows, show_device_col, dev_w, None, out);
}

/// Same table as `render_text`, with an extra `Connected` column (`yes`/`-`)
/// appended after RX, aligned by index with `rows`. Only `ports find` calls
/// this: `find`'s entire purpose is telling the operator which port a device
/// is on *now*, and previously only the connected-first sort order
/// distinguished that from stale history. `list` and `devices ports` keep
/// calling `render_text` above, unaffected by this column's existence.
pub fn render_text_with_connected(
    rows: &[&PortRow],
    dev_w: usize,
    connected: &[bool],
    out: &OutputConfig,
) {
    render_rows(rows, true, dev_w, Some(connected), out);
}

/// Shared implementation behind `render_text` and `render_text_with_connected`.
/// `connected` is `None` for `render_text`'s two callers, so their output is
/// untouched; `Some` only from `render_text_with_connected`.
fn render_rows(
    rows: &[&PortRow],
    show_device_col: bool,
    dev_w: usize,
    connected: Option<&[bool]>,
    out: &OutputConfig,
) {
    let color = use_color();
    // Must match the `{:<N}` widths the header below uses for these two
    // columns, so `pad_visible` reproduces them exactly.
    const LINK_W: usize = 6;
    const CONNECTED_W: usize = 9;

    let mut header = if show_device_col {
        format!(
            "{:<dev_w$} {:<6} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
            "Device", "Port", "Name", "Link", "Speed", "PoE", "TX", "RX"
        )
    } else {
        format!(
            "{:<6} {:<16} {:<6} {:<10} {:<8} {:>10} {:>10}",
            "Port", "Name", "Link", "Speed", "PoE", "TX", "RX"
        )
    };
    let mut rule_w = if show_device_col { 70 + dev_w } else { 70 };
    if connected.is_some() {
        header.push_str(&format!(" {:<9}", "Connected"));
        rule_w += 10;
    }
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(rule_w).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(rule_w));
    }

    for (i, r) in rows.iter().enumerate() {
        let p = r.port;
        let port = p
            .port_idx
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into());
        let name = p.name.as_deref().unwrap_or("-");
        let link = if p.up { "up" } else { "down" };
        let link_rendered = if color {
            if p.up {
                format!("{}", "up".green())
            } else {
                format!("{}", "down".dimmed())
            }
        } else {
            link.to_string()
        };
        let link_cell = pad_visible(&link_rendered, link, LINK_W);
        let speed = speed_cell(p);
        let poe = poe_cell(p);
        let tx = p.tx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());
        let rx = p.rx_bytes.map(format_bytes).unwrap_or_else(|| "-".into());

        let mut line = if show_device_col {
            format!(
                " {:<dev_w$} {:<5} {:<16} {link_cell} {:<10} {:<8} {:>10} {:>10}",
                r.device_name, port, name, speed, poe, tx, rx
            )
        } else {
            format!(
                " {:<5} {:<16} {link_cell} {:<10} {:<8} {:>10} {:>10}",
                port, name, speed, poe, tx, rx
            )
        };
        if let Some(flags) = connected {
            let is_connected = flags[i];
            let plain = if is_connected { "yes" } else { "-" };
            let rendered = if color {
                if is_connected {
                    format!("{}", "yes".green())
                } else {
                    format!("{}", "-".dimmed())
                }
            } else {
                plain.to_string()
            };
            let cell = pad_visible(&rendered, plain, CONNECTED_W);
            line.push_str(&format!(" {cell}"));
        }
        println!("{line}");
    }
    out.print_message(&format!("\n{} {}", rows.len(), port_noun(rows.len())));
}

pub async fn list(
    client: &UnifiClient,
    mac: Option<&str>,
    out: OutputConfig,
    pagination: Pagination,
) -> Result<(), Box<dyn std::error::Error>> {
    let devices = match mac {
        Some(m) => vec![client.get_device_ports(m).await?],
        None => client.list_all_device_ports().await?,
    };
    let rows = collect_rows(&devices);
    let total = rows.len();
    let page: Vec<&PortRow> = rows
        .iter()
        .skip(pagination.offset)
        .take(pagination.limit)
        .collect();

    if out.is_json() {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|r| {
                let mut v = row_json(r);
                project(&mut v, &pagination.fields);
                v
            })
            .collect();
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "items": items,
            "total": total,
            "limit": pagination.limit,
            "offset": pagination.offset,
        }))?);
    } else {
        // Computed from the full `rows`, not the paginated `page`, so two
        // `--offset` pages of the same query render the Device column at the
        // same width.
        let full_refs: Vec<&PortRow> = rows.iter().collect();
        let dev_w = device_col_width(&full_refs);
        render_text(&page, mac.is_none(), dev_w, &out);
    }
    Ok(())
}

/// Locate a port by index within a device's port table.
pub fn find_port(device: &DeviceWithPorts, port_idx: u32) -> Result<&PortEntry, ApiError> {
    device
        .port_table
        .iter()
        .find(|p| p.port_idx == Some(port_idx))
        .ok_or_else(|| {
            let mac = device
                .mac
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "device".into());
            ApiError::NotFound(format!("Port {port_idx} on {mac}"))
        })
}

/// Normalize `identifier` and return it only if it already has MAC shape (12
/// hex digits once separators are stripped). Shared by `resolve_identifier`
/// and `find` so a MAC identifier is recognized identically in both places
/// without duplicating the predicate.
fn identifier_as_mac(identifier: &str) -> Option<String> {
    let normalized = normalize_mac(identifier);
    (normalized.len() == 12 && normalized.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(normalized)
}

/// Resolve a MAC, IP, or client name to every candidate MAC it could refer
/// to. Deliberately does *not* decide ambiguity here: two client records can
/// share a name because they are two interfaces (wired and wireless, say) of
/// one physical device, and only one of them may ever appear in a switch's
/// port table. `find` decides ambiguity from port occupancy instead, after
/// looking up every candidate this returns.
///
/// Ordered, stopping at the first tier that matches: normalized MAC equality,
/// then exact IP, then case-insensitive name/hostname substring (all matches
/// in that last tier are returned together). Follows the
/// `protect cameras show <id-or-name>` precedent rather than the MAC-only
/// convention of `clients show`, because the whole point of `find` is not
/// having to look the MAC up first.
pub fn resolve_candidates(
    identifier: &str,
    clients: &[LegacyClient],
) -> Result<Vec<String>, ApiError> {
    if let Some(mac) = identifier_as_mac(identifier) {
        return Ok(vec![mac]);
    }

    if let Some(c) = clients.iter().find(|c| c.ip.as_deref() == Some(identifier))
        && let Some(mac) = c.mac.as_deref()
    {
        return Ok(vec![normalize_mac(mac)]);
    }

    let wanted = identifier.to_lowercase();
    let by_name: Vec<&LegacyClient> = clients
        .iter()
        .filter(|c| {
            c.name
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains(&wanted))
                || c.hostname
                    .as_deref()
                    .is_some_and(|h| h.to_lowercase().contains(&wanted))
        })
        .collect();

    if by_name.is_empty() {
        return Err(ApiError::NotFound(format!(
            "No client matching '{identifier}'"
        )));
    }

    let macs: Vec<String> = by_name
        .iter()
        .filter_map(|c| c.mac.as_deref().map(normalize_mac))
        .collect();
    if macs.is_empty() {
        return Err(ApiError::NotFound(format!(
            "Client '{identifier}' has no MAC"
        )));
    }
    Ok(macs)
}

/// Describe one `find` candidate for a conflict message: its name/hostname,
/// formatted MAC, and the switch port it was found on (the connected-first
/// row, i.e. `hits[0]`). Only called once a candidate is already known to
/// have at least one port match.
fn candidate_descriptor(mac: &str, clients: &[LegacyClient], row: &PortRow) -> String {
    let label = clients
        .iter()
        .find(|c| c.mac.as_deref().map(normalize_mac).as_deref() == Some(mac))
        .and_then(|c| c.name.as_deref().or(c.hostname.as_deref()))
        .unwrap_or("-");
    let port = row
        .port
        .port_idx
        .map(|i| i.to_string())
        .unwrap_or_else(|| "-".into());
    format!(
        "{label} ({}) on {} port {port}",
        format_mac(mac),
        row.device_name
    )
}

/// Rows whose `last_connection.mac` matches, connected first so a stale record
/// reads as history rather than as the device's current location.
pub fn matching_rows<'a>(
    rows: &'a [PortRow<'a>],
    normalized_mac: &str,
) -> Vec<(&'a PortRow<'a>, bool)> {
    let mut hits: Vec<(&PortRow, bool)> = rows
        .iter()
        .filter_map(|r| {
            let lc = r.port.last_connection.as_ref()?;
            let m = lc.mac.as_deref()?;
            (normalize_mac(m) == normalized_mac).then(|| (r, lc.connected.unwrap_or(false)))
        })
        .collect();
    hits.sort_by_key(|(_, connected)| !*connected);
    hits
}

/// Find which switch port a device is attached to, by MAC, IP, or client
/// name.
///
/// Resolution and port lookup interleave rather than picking a single client
/// up front: a name can match more than one client record while only one of
/// them is ever attached to a switch port (a device's wired and wireless
/// interfaces commonly share a name and report separately). Ambiguity is
/// judged by port occupancy, computed after fetching the port tables, not by
/// how many client records the name matched.
pub async fn find(
    client: &UnifiClient,
    identifier: &str,
    out: OutputConfig,
    fields: Option<Vec<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // A MAC identifier resolves locally, so the common scripted path stays a
    // single round trip: no client lookup is needed to know which MAC to
    // look for on the port tables.
    let (candidates, clients) = if let Some(mac) = identifier_as_mac(identifier) {
        (vec![mac], Vec::new())
    } else {
        let clients = client.list_clients_legacy().await?;
        let candidates = resolve_candidates(identifier, &clients)?;
        (candidates, clients)
    };

    let devices = client.list_all_device_ports().await?;
    let rows = collect_rows(&devices);

    // Port matches for every candidate, keeping only the ones actually on a
    // port. A candidate that matched the name/IP but never appears in any
    // port table (e.g. a client's WiFi interface, when only its wired
    // interface is on a switch) is not noise worth surfacing here.
    let mut ported: Vec<(String, Vec<(&PortRow, bool)>)> = candidates
        .into_iter()
        .filter_map(|mac| {
            let hits = matching_rows(&rows, &mac);
            (!hits.is_empty()).then_some((mac, hits))
        })
        .collect();

    let hits = match ported.len() {
        0 => {
            return Err(Box::new(ApiError::NotFound(format!(
                "No switch port with '{identifier}' attached"
            ))));
        }
        1 => ported.pop().expect("checked len == 1 above").1,
        _ => {
            let list = ported
                .iter()
                .map(|(mac, hits)| candidate_descriptor(mac, &clients, hits[0].0))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Box::new(ApiError::Conflict(format!(
                "'{identifier}' matches {} devices on switch ports: {list}",
                ported.len()
            ))));
        }
    };

    if out.is_json() {
        let items: Vec<serde_json::Value> = hits
            .iter()
            .map(|(r, connected)| {
                let mut v = row_json(r);
                v["connected"] = (*connected).into();
                project(&mut v, &fields);
                v
            })
            .collect();
        out.print_data(&serde_json::to_string_pretty(&items)?);
    } else {
        let refs: Vec<&PortRow> = hits.iter().map(|(r, _)| *r).collect();
        let connected: Vec<bool> = hits.iter().map(|(_, c)| *c).collect();
        // `find` never paginates, so `refs` is already the full result set.
        let dev_w = device_col_width(&refs);
        render_text_with_connected(&refs, dev_w, &connected, &out);
    }
    Ok(())
}

pub async fn show(
    client: &UnifiClient,
    mac: &str,
    port_idx: u32,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = client.get_device_ports(mac).await?;
    let p = find_port(&device, port_idx)?;
    let (device_mac, device_name) = device_identity(&device, "-");
    let (attached_mac, last_seen_mac) = attachment(p);

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "device_mac": device_mac,
            "device_name": device_name,
            "port_idx": p.port_idx,
            "name": p.name,
            "media": p.media,
            "up": p.up,
            "speed": p.speed,
            "full_duplex": p.full_duplex,
            "autoneg": p.autoneg,
            "enable": p.enable,
            "is_uplink": p.is_uplink,
            "stp_state": p.stp_state,
            "port_poe": p.port_poe,
            "poe_enable": p.poe_enable,
            "poe_mode": p.poe_mode,
            "poe_class": p.poe_class,
            "poe_power": p.poe_power,
            "poe_voltage": p.poe_voltage,
            "poe_current": p.poe_current,
            "poe_good": p.poe_good,
            "attached_mac": attached_mac,
            "attached_last_seen_mac": last_seen_mac,
            "tx_bytes": p.tx_bytes,
            "rx_bytes": p.rx_bytes,
            "tx_errors": p.tx_errors,
            "rx_errors": p.rx_errors,
        }))?);
        return Ok(());
    }

    let color = use_color();
    let label = |l: &str| -> String {
        if color {
            format!("{}", l.dimmed())
        } else {
            l.to_string()
        }
    };
    let title = format!("Port {port_idx} on {device_name} ({device_mac})");
    if color {
        println!("{}", title.bold());
    } else {
        println!("{title}");
    }
    println!(
        "  {}  {}",
        label("Name:     "),
        p.name.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Link:     "),
        if p.up { "up" } else { "down" }
    );
    println!("  {}  {}", label("Speed:    "), speed_cell(p));
    println!(
        "  {}  {}",
        label("Media:    "),
        p.media.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("PoE:      "),
        if p.port_poe {
            poe_cell(p)
        } else {
            "not supported".into()
        }
    );
    if p.port_poe {
        println!(
            "  {}  {}",
            label("PoE mode: "),
            p.poe_mode.as_deref().unwrap_or("-")
        );
        println!(
            "  {}  {}",
            label("PoE class:"),
            p.poe_class.as_deref().unwrap_or("-")
        );
        if let Some(v) = p.poe_voltage {
            println!("  {}  {v:.2} V", label("Voltage:  "));
        }
        if let Some(c) = p.poe_current {
            println!("  {}  {c:.2} mA", label("Current:  "));
        }
    }
    // A stale record prints as unattached with its MAC qualified, so the line
    // can never be read as "this device is plugged in right now".
    let attached_cell = match (&attached_mac, &last_seen_mac) {
        (Some(mac), _) => mac.clone(),
        (None, Some(stale)) => format!("- (last seen {stale})"),
        (None, None) => "-".to_string(),
    };
    println!("  {}  {}", label("Attached: "), attached_cell);
    Ok(())
}

/// Reject a power-cycle that cannot succeed, before any HTTP call.
pub fn check_cyclable(port: &PortEntry, device_mac: &str) -> Result<(), ApiError> {
    let idx = port
        .port_idx
        .map(|i| i.to_string())
        .unwrap_or_else(|| "?".into());
    let mac = format_mac(device_mac);

    // `port_poe` is `#[serde(default)] bool` (see src/api/types.rs), so
    // firmware that simply omits the key also lands here, indistinguishable
    // from a genuinely non-PoE port. That is deliberate: for a command that
    // cuts power, failing closed is the right direction. It does mean the
    // message/hint below can fire for PoE-capable hardware whose firmware
    // didn't report the field, not only for true non-PoE ports.
    if !port.port_poe {
        return Err(ApiError::Conflict(format!(
            "Port {idx} on {mac} does not support PoE. \
             Run `unifi ports list {mac}` to see PoE-capable ports."
        )));
    }
    // Only an explicit "off" blocks. An absent or unrecognised poe_mode
    // proceeds: the field is not guaranteed across firmware revisions.
    if port.poe_mode.as_deref() == Some("off") {
        return Err(ApiError::Conflict(format!(
            "PoE is administratively disabled on port {idx} of {mac} (poe_mode=off)"
        )));
    }
    // `poe_enable` is the controller's own precondition, confirmed in both
    // directions against a live UCG-Max: a port with port_poe: true,
    // poe_mode: "auto" and poe_enable: false rejects `power-cycle` with HTTP
    // 400 api.err.InvalidTargetPort, while the same command against a port
    // with poe_enable: true succeeds and reboots the attached device.
    // Checking it here turns that 400 into a local `conflict` naming the
    // reason, instead of an opaque status from the controller.
    if !port.poe_enable {
        return Err(ApiError::Conflict(format!(
            "Port {idx} on {mac} is not currently delivering PoE (poe_enable=false), \
             so there is no power to cycle."
        )));
    }
    Ok(())
}

/// Whether the cycle actually happened. `Declined` is not an error at this
/// layer; the caller decides how to report a refused confirmation.
#[derive(Debug, PartialEq, Eq)]
pub enum CycleOutcome {
    Cycled,
    Declined,
}

/// Power-cycle one PoE port.
///
/// `confirm` receives a human-readable summary of what is about to lose power
/// and returns whether to proceed. Taking it as a callback keeps the device
/// fetch and the guard rails to exactly one pass: the prompt needs the same
/// port data the checks do, so resolving it twice would mean two round trips
/// to the controller and two chances for the answers to disagree.
pub async fn cycle<F>(
    client: &UnifiClient,
    mac: &str,
    port_idx: u32,
    out: OutputConfig,
    confirm: F,
) -> Result<CycleOutcome, Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> std::io::Result<bool>,
{
    let device = client.get_device_ports(mac).await?;
    let port = find_port(&device, port_idx)?;
    let device_mac = device.mac.as_deref().unwrap_or(mac).to_string();
    check_cyclable(port, &device_mac)?;

    if !confirm(&cycle_summary(&device, port))? {
        return Ok(CycleOutcome::Declined);
    }

    client.power_cycle_port(&device_mac, port_idx).await?;
    out.print_result(
        &serde_json::json!({
            "status": "ok",
            "action": "power-cycle",
            "mac": format_mac(&device_mac),
            "port_idx": port_idx,
        }),
        &format!(
            "Power-cycling port {port_idx} on {}",
            format_mac(&device_mac)
        ),
    );
    Ok(CycleOutcome::Cycled)
}

/// One-line description of what is about to lose power, shown at the prompt.
pub fn cycle_summary(device: &DeviceWithPorts, port: &PortEntry) -> String {
    let device_mac = device
        .mac
        .as_deref()
        .map(format_mac)
        .unwrap_or_else(|| "-".into());
    let device_name = device
        .name
        .as_deref()
        .or(device.model.as_deref())
        .unwrap_or("-");
    let idx = port
        .port_idx
        .map(|i| i.to_string())
        .unwrap_or_else(|| "?".into());
    let attached = port
        .last_connection
        .as_ref()
        .filter(|lc| lc.connected.unwrap_or(false))
        .and_then(|lc| lc.mac.as_deref())
        .map(format_mac)
        .unwrap_or_else(|| "nothing attached".into());
    let draw = match port.poe_power {
        Some(w) if w > 0.0 => format!("{w:.2} W"),
        _ => "0 W".into(),
    };
    let class = port.poe_class.as_deref().unwrap_or("-");
    format!(
        "Port {idx} on {device_name} ({device_mac})\n  attached: {attached}  •  {draw}  •  {class}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `DeviceWithPorts` fixture from a JSON literal, exercising the
    /// same `Deserialize` impl the API layer uses.
    fn device(json: serde_json::Value) -> DeviceWithPorts {
        serde_json::from_value(json).expect("test fixture must deserialize as DeviceWithPorts")
    }

    #[test]
    fn collect_rows_flattens_multiple_devices_and_skips_empty_port_tables() {
        let devices = vec![
            device(serde_json::json!({
                "mac": "aa:bb:cc:dd:ee:01", "name": "SwitchA",
                "port_table": [{"port_idx": 1}, {"port_idx": 2}]
            })),
            device(serde_json::json!({
                "mac": "aa:bb:cc:dd:ee:02", "name": "APWithNoPorts",
                "port_table": []
            })),
            device(serde_json::json!({
                "mac": "aa:bb:cc:dd:ee:03", "name": "SwitchC",
                "port_table": [{"port_idx": 1}]
            })),
        ];

        let rows = collect_rows(&devices);

        assert_eq!(
            rows.len(),
            3,
            "device with an empty port_table must contribute no rows"
        );
        assert_eq!(rows[0].device_name, "SwitchA");
        assert_eq!(rows[0].port.port_idx, Some(1));
        assert_eq!(rows[1].device_name, "SwitchA");
        assert_eq!(rows[1].port.port_idx, Some(2));
        assert_eq!(rows[2].device_name, "SwitchC");
        assert_eq!(rows[2].port.port_idx, Some(1));
        assert!(
            rows.iter().all(|r| r.device_name != "APWithNoPorts"),
            "a device with no ports must never appear in the flattened rows"
        );
    }

    #[test]
    fn collect_rows_formats_device_mac_and_falls_back_to_model_when_name_is_absent() {
        let devices = vec![
            device(serde_json::json!({
                "mac": "9c05d6bc0643", "name": "USW-24-PoE",
                "port_table": [{"port_idx": 1}]
            })),
            device(serde_json::json!({
                "mac": "aabbccddeeff", "model": "USW-Lite-8",
                "port_table": [{"port_idx": 1}]
            })),
            device(serde_json::json!({
                "mac": "112233445566",
                "port_table": [{"port_idx": 1}]
            })),
        ];

        let rows = collect_rows(&devices);

        assert_eq!(
            rows[0].device_mac, "9c:05:d6:bc:06:43",
            "device_mac must be formatted via format_mac, not passed through raw"
        );
        assert_eq!(rows[0].device_name, "USW-24-PoE");

        assert_eq!(rows[1].device_mac, "aa:bb:cc:dd:ee:ff");
        assert_eq!(
            rows[1].device_name, "USW-Lite-8",
            "device_name must fall back to model when name is absent"
        );

        assert_eq!(rows[2].device_mac, "11:22:33:44:55:66");
        assert_eq!(
            rows[2].device_name, "-",
            "device_name must fall back to '-' when both name and model are absent"
        );
    }

    #[test]
    fn collect_rows_with_fallback_uses_the_caller_supplied_fallback() {
        // `devices ports` restores the historical "Device" label for a
        // device with neither `name` nor `model`; `collect_rows` (used by
        // `ports list` / `ports find`) must keep falling back to "-".
        let devices = vec![device(serde_json::json!({
            "mac": "aa:bb:cc:dd:ee:ff",
            "port_table": [{"port_idx": 1}]
        }))];

        let fallback_rows = collect_rows_with_fallback(&devices, "Device");
        assert_eq!(fallback_rows[0].device_name, "Device");

        let default_rows = collect_rows(&devices);
        assert_eq!(
            default_rows[0].device_name, "-",
            "collect_rows must still fall back to '-', unaffected by the new parameter"
        );
    }

    #[test]
    fn row_json_emits_exactly_the_fields_declared_in_ports_list() {
        let devices = vec![device(serde_json::json!({
            "mac": "aa:bb:cc:dd:ee:ff", "name": "SwitchA",
            "port_table": [{
                "port_idx": 1, "name": "Port 1", "media": "GE", "up": true,
                "speed": 1000, "full_duplex": true, "poe_enable": true,
                "poe_power": 4.5, "port_poe": true, "tx_bytes": 100, "rx_bytes": 200
            }]
        }))];
        let rows = collect_rows(&devices);
        let value = row_json(&rows[0]);
        let obj = value.as_object().expect("row_json must emit a JSON object");

        let mut emitted: Vec<&str> = obj.keys().map(String::as_str).collect();
        emitted.sort_unstable();
        let mut declared: Vec<&str> = crate::fields::names(crate::fields::PORTS_LIST);
        declared.sort_unstable();

        assert_eq!(
            emitted, declared,
            "row_json keys must exactly match fields::PORTS_LIST, so the two cannot drift"
        );
    }

    #[test]
    fn project_retains_only_the_requested_fields() {
        let mut value = serde_json::json!({"a": 1, "b": 2, "c": 3});
        project(&mut value, &Some(vec!["a".to_string(), "c".to_string()]));

        let obj = value.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("a"));
        assert!(obj.contains_key("c"));
        assert!(!obj.contains_key("b"), "unrequested fields must be dropped");
    }

    #[test]
    fn project_is_a_noop_when_fields_is_none() {
        let mut value = serde_json::json!({"a": 1, "b": 2, "c": 3});
        let before = value.clone();

        project(&mut value, &None);

        assert_eq!(
            value, before,
            "a None projection must leave the value untouched"
        );
    }

    // `pad_visible` is what makes coloured Link/Connected cells line up with
    // the header at a TTY. `use_color()` reads `stdout().is_terminal()`
    // directly with no override, so a unit test cannot force colour on for
    // `render_rows` itself. Testing the padding helper directly, with a
    // hand-built ANSI-escaped string standing in for what `owo_colors` would
    // emit, exercises the defect it guards against (padding computed from
    // byte length instead of visible width) without needing a real TTY.
    #[test]
    fn pad_visible_pads_by_plain_width_not_escaped_byte_length() {
        // The real thing `render_rows` hands `pad_visible` on the coloured
        // path: `"up".green()` rendered to a `String`, several bytes of ANSI
        // escapes wrapped around 2 visible characters. A `{:<6}` specifier
        // sees this as already over width 6 (that was the bug) and pads with
        // nothing at all.
        let escaped = format!("{}", "up".green());
        assert!(
            escaped.len() > "up".len(),
            "fixture must actually carry escape bytes, or this test proves nothing: {escaped:?}"
        );

        let padded = pad_visible(&escaped, "up", 6);

        assert!(
            padded.starts_with(&escaped),
            "the coloured text itself must be emitted untouched: {padded:?}"
        );
        let visible_padding = &padded[escaped.len()..];
        assert_eq!(
            visible_padding, "    ",
            "padding must be derived from \"up\".len() (2), not the escaped \
             string's byte length: {padded:?}"
        );
    }

    #[test]
    fn pad_visible_matches_the_uncoloured_output_it_replaces() {
        // On the uncoloured path every call site passes `rendered == plain`,
        // so this must reproduce exactly what the old `{:<width$}` specifier
        // produced: the uncoloured path must not change at all.
        assert_eq!(pad_visible("up", "up", 6), format!("{:<6}", "up"));
        assert_eq!(pad_visible("down", "down", 6), format!("{:<6}", "down"));
        assert_eq!(pad_visible("yes", "yes", 9), format!("{:<9}", "yes"));
        assert_eq!(pad_visible("-", "-", 9), format!("{:<9}", "-"));
    }

    #[test]
    fn pad_visible_pads_nothing_when_plain_already_fills_the_width() {
        assert_eq!(pad_visible("down", "down", 4), "down");
    }

    #[test]
    fn port_noun_is_singular_for_exactly_one_row() {
        assert_eq!(port_noun(1), "port");
    }

    #[test]
    fn port_noun_is_plural_for_zero_or_many_rows() {
        assert_eq!(port_noun(0), "ports");
        assert_eq!(port_noun(2), "ports");
        assert_eq!(port_noun(100), "ports");
    }

    fn device_with(ports: serde_json::Value) -> DeviceWithPorts {
        serde_json::from_value(serde_json::json!({
            "mac": "aa:bb:cc:dd:ee:ff",
            "name": "SwitchA",
            "port_table": ports
        }))
        .expect("fixture must parse")
    }

    #[test]
    fn find_port_returns_the_matching_entry() {
        let d = device_with(serde_json::json!([
            {"port_idx": 1, "port_poe": true},
            {"port_idx": 5, "port_poe": true, "poe_mode": "auto"}
        ]));
        let p = find_port(&d, 5).expect("port 5 exists");
        assert_eq!(p.port_idx, Some(5));
        assert_eq!(p.poe_mode.as_deref(), Some("auto"));
    }

    #[test]
    fn find_port_missing_is_not_found() {
        let d = device_with(serde_json::json!([{"port_idx": 1}]));
        let err = find_port(&d, 99).expect_err("port 99 does not exist");
        assert!(matches!(err, crate::api::ApiError::NotFound(_)));
    }

    #[test]
    fn check_cyclable_rejects_non_poe_port() {
        let d = device_with(serde_json::json!([{"port_idx": 9, "port_poe": false}]));
        let p = find_port(&d, 9).unwrap();
        let err = check_cyclable(p, "aa:bb:cc:dd:ee:ff").expect_err("SFP+ has no PoE");
        match err {
            crate::api::ApiError::Conflict(msg) => {
                assert!(msg.contains("does not support PoE"), "got: {msg}")
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn check_cyclable_rejects_poe_mode_off() {
        let d = device_with(serde_json::json!([
            {"port_idx": 4, "port_poe": true, "poe_mode": "off"}
        ]));
        let p = find_port(&d, 4).unwrap();
        let err = check_cyclable(p, "aa:bb:cc:dd:ee:ff").expect_err("PoE is off");
        match err {
            crate::api::ApiError::Conflict(msg) => {
                assert!(msg.contains("poe_mode=off"), "got: {msg}")
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn check_cyclable_allows_absent_poe_mode() {
        // poe_mode is not guaranteed across firmware. A missing value must not
        // block a port that already passed the port_poe check. poe_enable is
        // set explicitly here so this test stays about poe_mode alone, not
        // about the separate poe_enable guard below.
        let d = device_with(serde_json::json!([
            {"port_idx": 4, "port_poe": true, "poe_enable": true}
        ]));
        let p = find_port(&d, 4).unwrap();
        assert!(check_cyclable(p, "aa:bb:cc:dd:ee:ff").is_ok());
    }

    #[test]
    fn check_cyclable_allows_a_port_actually_delivering_power() {
        // The genuinely cyclable case: PoE-capable, auto, and delivering
        // power right now.
        let d = device_with(serde_json::json!([
            {"port_idx": 4, "port_poe": true, "poe_mode": "auto", "poe_enable": true}
        ]));
        let p = find_port(&d, 4).unwrap();
        assert!(check_cyclable(p, "aa:bb:cc:dd:ee:ff").is_ok());
    }

    #[test]
    fn check_cyclable_rejects_poe_enable_false() {
        // This fixture is the live UCG-Max finding that prompted this guard:
        // PoE-capable, mode "auto", passing both prior checks, but not
        // currently delivering power (poe_enable defaults to false here,
        // matching what the controller reported). Firing power-cycle at it
        // was rejected with HTTP 400 api.err.InvalidTargetPort instead of
        // succeeding, which is why this must be rejected locally too rather
        // than treated as the earlier "happy path" this test used to assert.
        let d = device_with(serde_json::json!([
            {"port_idx": 4, "port_poe": true, "poe_mode": "auto", "up": false}
        ]));
        let p = find_port(&d, 4).unwrap();
        let err = check_cyclable(p, "aa:bb:cc:dd:ee:fe").expect_err("poe_enable is false");
        match err {
            crate::api::ApiError::Conflict(msg) => {
                assert!(msg.contains("not currently delivering PoE"), "got: {msg}")
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn check_cyclable_poe_mode_off_message_wins_over_poe_enable_false() {
        // poe_mode: "off" implies poe_enable: false (confirmed explicitly
        // here rather than relying on the default), so both guards would
        // fire. The administratively-disabled message must win: it is the
        // more specific and more useful of the two, and the poe_mode check
        // runs first in `check_cyclable`.
        let d = device_with(serde_json::json!([
            {"port_idx": 4, "port_poe": true, "poe_mode": "off", "poe_enable": false}
        ]));
        let p = find_port(&d, 4).unwrap();
        let err = check_cyclable(p, "aa:bb:cc:dd:ee:ff").expect_err("PoE is off");
        match err {
            crate::api::ApiError::Conflict(msg) => {
                assert!(msg.contains("poe_mode=off"), "got: {msg}");
                assert!(
                    !msg.contains("not currently delivering PoE"),
                    "the administratively-disabled message must win over the \
                     poe_enable=false message: {msg}"
                );
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    // `cycle_summary` is the text a human reads before authorising a power
    // cut. Untested, it carries real logic that would be easy to invert or
    // drop silently: the `connected` filter on `last_connection`, and the
    // watt formatting.

    #[test]
    fn cycle_summary_shows_the_attached_mac_when_connected() {
        let d = device_with(serde_json::json!([{
            "port_idx": 4, "port_poe": true,
            "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": true}
        }]));
        let p = find_port(&d, 4).unwrap();
        let summary = cycle_summary(&d, p);
        assert!(
            summary.contains("aa:bb:cc:dd:ee:10"),
            "a connected last_connection must show the formatted attached MAC: {summary}"
        );
    }

    #[test]
    fn cycle_summary_reads_nothing_attached_for_a_stale_record() {
        // connected: false is history, not the device's current location; the
        // summary must not read as if a live device would lose power.
        let d = device_with(serde_json::json!([{
            "port_idx": 4, "port_poe": true,
            "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": false}
        }]));
        let p = find_port(&d, 4).unwrap();
        let summary = cycle_summary(&d, p);
        assert!(
            summary.contains("nothing attached"),
            "a stale (disconnected) last_connection must read as unattached: {summary}"
        );
        assert!(
            !summary.contains("aa:bb:cc:dd:ee:10"),
            "a stale MAC must not appear as if it were live: {summary}"
        );
    }

    #[test]
    fn cycle_summary_reads_nothing_attached_when_no_last_connection() {
        let d = device_with(serde_json::json!([{"port_idx": 4, "port_poe": true}]));
        let p = find_port(&d, 4).unwrap();
        let summary = cycle_summary(&d, p);
        assert!(
            summary.contains("nothing attached"),
            "an absent last_connection must read as unattached: {summary}"
        );
    }

    #[test]
    fn cycle_summary_shows_the_wattage_for_a_powered_port() {
        let d = device_with(serde_json::json!([{
            "port_idx": 4, "port_poe": true, "poe_enable": true,
            "poe_power": 5.25, "poe_class": "4"
        }]));
        let p = find_port(&d, 4).unwrap();
        let summary = cycle_summary(&d, p);
        assert!(
            summary.contains("5.25 W"),
            "draw must be formatted to two decimal places: {summary}"
        );
    }

    // `_id` is required by `LegacyClient`, so every record here supplies one
    // or the fixture will not deserialize.
    fn clients_fixture() -> Vec<crate::api::LegacyClient> {
        serde_json::from_value(serde_json::json!([
            {"_id": "1", "mac": "aa:bb:cc:dd:ee:10", "name": "garage-pi",   "ip": "10.0.0.5"},
            {"_id": "2", "mac": "aa:bb:cc:dd:ee:20", "name": "office-ap",   "ip": "10.0.0.6"},
            {"_id": "3", "mac": "aa:bb:cc:dd:ee:21", "name": "Main-Office", "ip": "10.0.0.7"}
        ]))
        .expect("fixture must parse")
    }

    #[test]
    fn resolve_candidates_accepts_any_mac_format() {
        let c = clients_fixture();
        // A MAC resolves without consulting the client list at all.
        assert_eq!(
            resolve_candidates("AA-BB-CC-DD-EE-10", &c).unwrap(),
            vec!["aabbccddee10"]
        );
    }

    #[test]
    fn resolve_candidates_matches_ip_then_name() {
        let c = clients_fixture();
        assert_eq!(
            resolve_candidates("10.0.0.5", &c).unwrap(),
            vec!["aabbccddee10"]
        );
        assert_eq!(
            resolve_candidates("GARAGE-PI", &c).unwrap(),
            vec!["aabbccddee10"]
        );
    }

    // A name matching multiple client records is not an error here.
    // `resolve_candidates` returns every candidate; `find` calls it a conflict
    // only once it also knows more than one of them sits on a switch port.
    #[test]
    fn resolve_candidates_returns_every_name_match_without_erroring() {
        let c = clients_fixture();
        let macs = resolve_candidates("office", &c).expect("both are valid candidates");
        assert_eq!(
            macs,
            vec!["aabbccddee20".to_string(), "aabbccddee21".to_string()],
            "both office-ap and Main-Office must come back as candidates"
        );
    }

    #[test]
    fn resolve_candidates_unknown_is_not_found() {
        let c = clients_fixture();
        let err = resolve_candidates("nothing-here", &c).expect_err("unknown");
        assert!(matches!(err, crate::api::ApiError::NotFound(_)));
    }

    #[test]
    fn matching_rows_sort_connected_first() {
        let devices: Vec<DeviceWithPorts> = serde_json::from_value(serde_json::json!([{
            "mac": "aa:bb:cc:dd:ee:ff",
            "name": "SwitchA",
            "port_table": [
                {"port_idx": 2, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": false}},
                {"port_idx": 7, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": true}},
                {"port_idx": 9, "last_connection": {"mac": "11:22:33:44:55:66", "connected": true}}
            ]
        }]))
        .expect("fixture must parse");
        let rows = collect_rows(&devices);
        let hits = matching_rows(&rows, "aabbccddee10");
        assert_eq!(hits.len(), 2, "device appears on two ports");
        assert_eq!(
            hits[0].0.port.port_idx,
            Some(7),
            "connected port sorts first"
        );
        assert!(hits[0].1, "first hit is connected");
        assert!(!hits[1].1, "second hit is the stale record");
    }

    #[test]
    fn find_json_row_matches_exactly_the_fields_declared_in_ports_find() {
        // Exercises the same construction `find` uses (`row_json` plus the
        // manually-inserted `connected` key) without needing an HTTP mock, so
        // a drift between the two can never sneak past this test.
        let devices = vec![device(serde_json::json!({
            "mac": "aa:bb:cc:dd:ee:ff", "name": "SwitchA",
            "port_table": [{
                "port_idx": 7,
                "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": true}
            }]
        }))];
        let rows = collect_rows(&devices);
        let hits = matching_rows(&rows, "aabbccddee10");
        let (row, connected) = hits[0];
        let mut value = row_json(row);
        value["connected"] = connected.into();

        let obj = value.as_object().expect("must emit a JSON object");
        let mut emitted: Vec<&str> = obj.keys().map(String::as_str).collect();
        emitted.sort_unstable();
        let mut declared: Vec<&str> = crate::fields::names(crate::fields::PORTS_FIND);
        declared.sort_unstable();

        assert_eq!(
            emitted, declared,
            "find's emitted keys must exactly match fields::PORTS_FIND"
        );
    }
}
