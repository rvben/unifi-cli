use serde::Deserialize;
use std::fmt;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

// Integration API response wrapper (paginated)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T> {
    pub total_count: usize,
    pub data: Vec<T>,
}

// Legacy API response wrapper (pub for standalone fetch in TUI)
#[derive(Debug, Deserialize)]
pub struct LegacyResponse<T> {
    pub meta: LegacyMeta,
    pub data: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct LegacyMeta {
    pub rc: String,
    pub msg: Option<String>,
}

// Site
#[derive(Debug, Deserialize)]
pub struct Site {
    pub id: String,
}

// Client from Integration API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    #[serde(alias = "macAddress")]
    pub mac_address: Option<String>,
    #[serde(alias = "ipAddress")]
    pub ip_address: Option<String>,
    pub name: Option<String>,
    pub hostname: Option<String>,
    #[serde(alias = "type")]
    pub client_type: Option<String>,
}

impl Client {
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .or(self.hostname.as_deref())
            .unwrap_or("-")
    }

    pub fn clean_name(&self) -> String {
        let name = self.display_name();
        strip_mac_suffix(name, self.mac_address.as_deref())
    }
}

// Client from Legacy stat/sta endpoint (richer data)
#[derive(Debug, Deserialize)]
pub struct LegacyClient {
    #[serde(rename = "_id")]
    pub id: String,
    pub mac: Option<String>,
    pub ip: Option<String>,
    pub hostname: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub is_wired: bool,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub fixed_ap_enabled: bool,
    pub fixed_ap_mac: Option<String>,
    pub uptime: Option<u64>,
    pub tx_bytes: Option<u64>,
    pub rx_bytes: Option<u64>,
    pub signal: Option<i32>,
    pub ap_mac: Option<String>,
    #[serde(rename = "essid")]
    pub ssid: Option<String>,
}

impl LegacyClient {
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .or(self.hostname.as_deref())
            .unwrap_or("-")
    }

    pub fn clean_name(&self) -> String {
        let name = self.display_name();
        strip_mac_suffix(name, self.mac.as_deref())
    }
}

// Device from Integration API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub state: Option<String>,
    pub firmware_version: Option<String>,
}

// Device from Legacy stat/device endpoint (richer data)
#[derive(Debug, Deserialize)]
pub struct LegacyDevice {
    pub mac: Option<String>,
    pub ip: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    pub state: Option<u32>,
    pub version: Option<String>,
    pub uptime: Option<u64>,
    pub num_sta: Option<u32>,
    #[serde(default)]
    pub upgradable: bool,
    pub upgrade_to_firmware: Option<String>,
}

impl LegacyDevice {
    pub fn state_str(&self) -> &str {
        match self.state {
            Some(1) => "ONLINE",
            Some(0) => "OFFLINE",
            Some(2) => "ADOPTING",
            Some(4) => "UPGRADING",
            Some(5) => "PROVISIONING",
            _ => "UNKNOWN",
        }
    }
}

// Network from Integration API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub vlan_id: Option<u16>,
    #[serde(default)]
    pub default: bool,
}

// Health subsystem from Legacy stat/health
#[derive(Debug, Deserialize)]
pub struct HealthSubsystem {
    pub subsystem: String,
    pub status: Option<String>,
    pub num_sta: Option<u32>,
    pub num_ap: Option<u32>,
    #[serde(rename = "num_sw")]
    pub num_switches: Option<u32>,
    pub wan_ip: Option<String>,
    pub isp_name: Option<String>,
}

// Sysinfo from Legacy stat/sysinfo
#[derive(Debug, Deserialize)]
pub struct SysInfo {
    pub hostname: Option<String>,
    pub version: Option<String>,
    pub timezone: Option<String>,
    pub uptime: Option<u64>,
}

// Host system info from /api/system (UniFi OS level)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSystem {
    pub device_state: Option<String>,
    pub name: Option<String>,
}

impl HostSystem {
    pub fn update_available(&self) -> bool {
        self.device_state.as_deref() == Some("updateAvailable")
    }
}

/// Strip trailing MAC suffix from display names.
/// UniFi appends " XX:XX" (last 2 bytes of MAC) to hostnames when no user name is set.
pub fn strip_mac_suffix(name: &str, mac: Option<&str>) -> String {
    if let Some(mac) = mac {
        let clean_mac = normalize_mac(mac);
        // Check for " XX:XX" suffix (last 4 hex chars of MAC with colon)
        if clean_mac.len() >= 4 {
            let last4 = &clean_mac[clean_mac.len() - 4..];
            let suffix = format!(" {}:{}", &last4[..2], &last4[2..]);
            if let Some(stripped) = name.strip_suffix(&suffix) {
                return stripped.to_string();
            }
            // Also try without colon in suffix
            let suffix_no_colon = format!(" {last4}");
            if let Some(stripped) = name.strip_suffix(&suffix_no_colon) {
                return stripped.to_string();
            }
        }
    }
    name.to_string()
}

pub fn normalize_mac(mac: &str) -> String {
    mac.to_lowercase().replace([':', '-'], "")
}

pub fn format_mac(mac: &str) -> String {
    let clean = normalize_mac(mac);
    if clean.len() != 12 {
        return mac.to_string();
    }
    format!(
        "{}:{}:{}:{}:{}:{}",
        &clean[0..2],
        &clean[2..4],
        &clean[4..6],
        &clean[6..8],
        &clean[8..10],
        &clean[10..12]
    )
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

// Event from Legacy stat/event endpoint
#[derive(Debug, Deserialize)]
pub struct Event {
    pub key: Option<String>,
    pub msg: Option<String>,
    pub subsystem: Option<String>,
    pub time: Option<u64>,
    pub datetime: Option<String>,
}

// Port entry from Legacy stat/device port_table
#[derive(Debug, Deserialize)]
pub struct PortEntry {
    pub port_idx: Option<u32>,
    pub name: Option<String>,
    pub media: Option<String>,
    #[serde(default)]
    pub up: bool,
    pub speed: Option<u32>,
    #[serde(default)]
    pub full_duplex: bool,
    #[serde(default)]
    pub poe_enable: bool,
    pub poe_power: Option<f64>,
    #[serde(default)]
    pub port_poe: bool,
    pub tx_bytes: Option<u64>,
    pub rx_bytes: Option<u64>,
}

// Device with port_table from Legacy stat/device endpoint
#[derive(Debug, Deserialize)]
pub struct DeviceWithPorts {
    pub mac: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub port_table: Vec<PortEntry>,
}

// Error types
#[derive(Debug)]
pub enum ApiError {
    Http(reqwest::Error),
    Api { status: u16, message: String },
    NotFound(String),
    Auth(String),
    Other(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Http(e) => {
                let msg = e.to_string();
                write!(f, "HTTP error: {e}")?;
                if msg.contains("connect") || msg.contains("Connection refused") {
                    write!(
                        f,
                        "\n  Hint: Check that the host is reachable and the URL is correct"
                    )?;
                } else if msg.contains("dns") || msg.contains("resolve") {
                    write!(
                        f,
                        "\n  Hint: Could not resolve hostname. Check the host value"
                    )?;
                } else if msg.contains("timed out") || msg.contains("timeout") {
                    write!(f, "\n  Hint: Request timed out. Is the controller running?")?;
                } else if msg.contains("certificate") || msg.contains("SSL") {
                    write!(
                        f,
                        "\n  Hint: TLS/certificate error. The CLI accepts self-signed certs by default"
                    )?;
                }
                Ok(())
            }
            ApiError::Api { status, message } => write!(f, "API error ({status}): {message}"),
            ApiError::NotFound(msg) => write!(f, "Not found: {msg}"),
            ApiError::Auth(msg) => {
                write!(f, "Authentication error: {msg}")?;
                write!(
                    f,
                    "\n  Hint: Check your API key. Generate one in UniFi Settings > API"
                )
            }
            ApiError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        if e.status()
            .is_some_and(|s| s.as_u16() == 401 || s.as_u16() == 403)
        {
            ApiError::Auth(e.to_string())
        } else if e.status().is_some_and(|s| s.as_u16() == 404) {
            ApiError::NotFound(e.to_string())
        } else {
            ApiError::Http(e)
        }
    }
}
