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

// Legacy API response wrapper
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
            ApiError::Http(e) => write!(f, "HTTP error: {e}"),
            ApiError::Api { status, message } => write!(f, "API error ({status}): {message}"),
            ApiError::NotFound(msg) => write!(f, "Not found: {msg}"),
            ApiError::Auth(msg) => write!(f, "Authentication error: {msg}"),
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
