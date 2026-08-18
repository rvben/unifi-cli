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

/// A port-forward record from the legacy Network API. This intentionally
/// allowlists only fields useful for policy audits.
#[derive(Debug, Deserialize)]
pub struct PortForward {
    #[serde(rename = "_id")]
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub proto: Option<String>,
    pub src: Option<String>,
    pub src_port: Option<String>,
    pub dst_port: Option<String>,
    pub fwd: Option<String>,
    pub fwd_port: Option<String>,
    pub pfwd_interface: Option<String>,
    #[serde(default)]
    pub log: bool,
}

/// A gauge UniFi reports as `-1` when it has no measurement to report.
///
/// Decoding that as a reading makes an unmeasured link indistinguishable from
/// a healthy one, so it decodes as absent instead. This is only applied to
/// quantities for which no negative value is a measurement: a latency, a
/// percentage, a counter or a rate. Radio metrics are negative by nature and
/// are decoded as they arrive.
fn unmeasured_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<f64>::deserialize(deserializer)?.filter(|value| *value >= 0.0))
}

/// The counter form of `unmeasured_f64`.
///
/// Decoding through `i64` keeps a negative marker local to the field that
/// carries it. Declaring these `u64` instead lets one such value abort the
/// whole WAN block, which turns a single unknown counter into a command that
/// reports nothing at all.
fn unmeasured_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<i64>::deserialize(deserializer)?
        .filter(|value| *value >= 0)
        .map(|value| value as u64))
}

#[derive(Debug, Deserialize)]
pub struct GatewayWanStatus {
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    pub wan1: Option<WanInterface>,
    pub wan2: Option<WanInterface>,
    pub wan3: Option<WanInterface>,
}

#[derive(Debug, Deserialize)]
pub struct WanInterface {
    pub name: Option<String>,
    pub ifname: Option<String>,
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub up: bool,
    pub ip: Option<String>,
    #[serde(default, deserialize_with = "unmeasured_f64")]
    pub availability: Option<f64>,
    #[serde(default, deserialize_with = "unmeasured_f64")]
    pub latency: Option<f64>,
    #[serde(default, deserialize_with = "unmeasured_u64")]
    pub speed: Option<u64>,
    #[serde(default, deserialize_with = "unmeasured_u64")]
    pub rx_bytes: Option<u64>,
    #[serde(default, deserialize_with = "unmeasured_u64")]
    pub tx_bytes: Option<u64>,
    #[serde(default, deserialize_with = "unmeasured_u64")]
    pub rx_rate: Option<u64>,
    #[serde(default, deserialize_with = "unmeasured_u64")]
    pub tx_rate: Option<u64>,
    pub mbb: Option<CellularStatus>,
    pub mbb_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CellularStatus {
    #[serde(default, deserialize_with = "unmeasured_f64")]
    pub signal_pct: Option<f64>,
    pub rat: Option<String>,
    /// Reference signal power in dBm, negative across its whole range.
    pub lte_rsrp: Option<f64>,
    /// Reference signal quality in dB, negative across its whole range.
    pub lte_rsrq: Option<f64>,
    /// Signal to noise ratio in dB, negative on a link that is worse than its
    /// own noise floor.
    pub lte_sinr: Option<f64>,
}

#[derive(Debug)]
pub struct NamedWanInterface {
    pub slot: &'static str,
    pub interface: WanInterface,
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
    /// The last address the controller recorded for this client. It outlives the
    /// lease, so it can name an address the client no longer holds. `clients list`
    /// reports the live address from `stat/sta` instead, so that it agrees with
    /// `clients show`.
    #[serde(alias = "ipAddress")]
    pub ip_address: Option<String>,
    pub name: Option<String>,
    pub hostname: Option<String>,
    #[serde(alias = "type")]
    pub client_type: Option<String>,
    /// ISO8601 timestamp of the current association.
    #[serde(alias = "connectedAt")]
    pub connected_at: Option<String>,
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
    /// Name of the network the client landed on ("Default", "IoT", ...). Absent
    /// while the client is associated but has not obtained an address.
    pub network: Option<String>,
    pub vlan: Option<u32>,
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

/// Network configuration from the legacy `rest/networkconf` endpoint.
///
/// Keep this deliberately typed: network configuration records can grow new,
/// sensitive fields over time and commands must never serialize the raw object.
#[derive(Debug, Deserialize)]
pub struct LegacyNetwork {
    #[serde(rename = "_id")]
    pub id: String,
    pub name: Option<String>,
    pub purpose: Option<String>,
    pub vlan: Option<u16>,
    pub ip_subnet: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub dhcpd_enabled: bool,
    #[serde(default)]
    pub dhcpd_dns_enabled: bool,
    pub dhcpd_dns_1: Option<String>,
    pub dhcpd_dns_2: Option<String>,
    #[serde(default)]
    pub mdns_enabled: bool,
    pub lte_lan_enabled: Option<bool>,
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
    /// Whether a firmware update is waiting, or `None` when the host did not say.
    ///
    /// A host that reported no device state has not reported an up-to-date one,
    /// so the answer is unknown rather than negative. Only a state the host did
    /// report settles the question, either way.
    pub fn update_available(&self) -> Option<bool> {
        self.device_state
            .as_deref()
            .map(|state| state == "updateAvailable")
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
    // The legacy /stat/device endpoint may return this as a JSON string
    // (e.g. "0.00") or as a JSON number depending on firmware. Accept either form.
    #[serde(default, deserialize_with = "deserialize_string_or_number_f64")]
    pub poe_power: Option<f64>,
    #[serde(default)]
    pub port_poe: bool,
    /// "auto", "off", "passthrough", "passive24v". Absent on some firmware.
    pub poe_mode: Option<String>,
    pub poe_class: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_number_f64")]
    pub poe_voltage: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_string_or_number_f64")]
    pub poe_current: Option<f64>,
    pub poe_good: Option<bool>,
    /// Auto-negotiation state. `Option`, not a defaulted bool, like `enable`
    /// and `is_uplink` below: a firmware that omits this key must not be
    /// reported as "auto-negotiation off". Matches `poe_good` above; contrast
    /// `up`/`poe_enable`, where an absent key genuinely does mean false.
    pub autoneg: Option<bool>,
    /// Administrative enable state. Same tri-state rationale as `autoneg`: an
    /// absent key must not be reported as "port administratively disabled".
    pub enable: Option<bool>,
    /// Whether this port is the switch's uplink. Same tri-state rationale as
    /// `autoneg`: an absent key must not be reported as "not an uplink".
    pub is_uplink: Option<bool>,
    pub stp_state: Option<String>,
    pub tx_errors: Option<u64>,
    pub rx_errors: Option<u64>,
    /// Absent entirely on a port nothing has linked to within retention.
    pub last_connection: Option<LastConnection>,
    pub tx_bytes: Option<u64>,
    pub rx_bytes: Option<u64>,
}

/// The device most recently seen on a port. `connected` distinguishes a live
/// attachment from a stale record of a device that has since moved.
#[derive(Debug, Deserialize)]
pub struct LastConnection {
    pub mac: Option<String>,
    pub connected: Option<bool>,
    pub last_seen: Option<u64>,
}

fn deserialize_string_or_number_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        Number(f64),
        String(String),
    }
    match Option::<StringOrNumber>::deserialize(deserializer)? {
        None => Ok(None),
        Some(StringOrNumber::Number(n)) => Ok(Some(n)),
        Some(StringOrNumber::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<f64>().map(Some).map_err(D::Error::custom)
            }
        }
    }
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

// --- Protect API types ---

/// Camera from Protect Integration API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectCamera {
    pub id: String,
    pub name: Option<String>,
    pub mac: Option<String>,
    pub state: Option<String>,
    pub model_key: Option<String>,
    pub is_mic_enabled: Option<bool>,
    pub video_mode: Option<String>,
    pub feature_flags: Option<ProtectFeatureFlags>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectFeatureFlags {
    #[serde(default)]
    pub has_hdr: bool,
    #[serde(default)]
    pub has_mic: bool,
    #[serde(default)]
    pub has_speaker: bool,
    #[serde(default)]
    pub has_led_status: bool,
    #[serde(default)]
    pub smart_detect_types: Vec<String>,
    #[serde(default)]
    pub video_modes: Vec<String>,
}

/// Full camera from direct Protect API (cookie auth)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectCameraFull {
    pub id: String,
    pub name: Option<String>,
    pub mac: Option<String>,
    pub host: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "type")]
    pub camera_type: Option<String>,
    pub market_name: Option<String>,
    pub platform: Option<String>,
    pub firmware_version: Option<String>,
    pub hardware_revision: Option<String>,
    pub uptime: Option<u64>,
    pub up_since: Option<u64>,
    pub last_seen: Option<u64>,
    pub is_recording: Option<bool>,
    /// Tri-state for the same reason as `is_recording`: a camera that did not
    /// report motion has not reported stillness.
    pub is_motion_detected: Option<bool>,
    pub is_dark: Option<bool>,
    pub video_codec: Option<String>,
    pub current_resolution: Option<String>,
    pub video_mode: Option<String>,
    pub hdr_type: Option<String>,
    pub phy_rate: Option<f64>,
    pub is_mic_enabled: Option<bool>,
    #[serde(default)]
    pub is_poor_network: bool,
    pub last_motion: Option<u64>,
    pub hq_bytes_per_day: Option<u64>,
    pub lq_bytes_per_day: Option<u64>,
    pub model_key: Option<String>,
    #[serde(default)]
    pub channels: Vec<CameraChannel>,
    pub stats: Option<CameraStats>,
    pub wifi_connection_state: Option<WifiConnectionState>,
    pub feature_flags: Option<ProtectFeatureFlags>,
    pub recording_settings: Option<RecordingSettings>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraChannel {
    pub id: u32,
    pub name: Option<String>,
    /// Tri-state like `is_rtsp_enabled` below: a channel whose state the camera
    /// did not report is not a channel reported as switched off.
    pub enabled: Option<bool>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
    pub bitrate: Option<u64>,
    pub is_rtsp_enabled: Option<bool>,
    pub rtsp_alias: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraStats {
    pub wifi: Option<WifiStats>,
    pub storage: Option<StorageStats>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiStats {
    pub channel: Option<u32>,
    pub frequency: Option<u32>,
    pub signal_quality: Option<i32>,
    pub signal_strength: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub used: Option<u64>,
    pub rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiConnectionState {
    pub channel: Option<u32>,
    pub frequency: Option<u32>,
    pub signal_quality: Option<i32>,
    pub signal_strength: Option<i32>,
    pub ssid: Option<String>,
    pub ap_name: Option<String>,
    pub connectivity: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSettings {
    pub mode: Option<String>,
    /// Tri-state: settings that did not mention motion detection have not said
    /// it is switched off.
    pub enable_motion_detection: Option<bool>,
}

/// RTSPS stream URLs keyed by quality level
pub type RtspsStreams = std::collections::HashMap<String, Option<String>>;

// Error types
#[derive(Debug)]
pub enum ApiError {
    Http(reqwest::Error),
    Api {
        status: u16,
        message: String,
    },
    NotFound(String),
    Auth(String),
    /// A request that cannot succeed against the resource's current state,
    /// rejected locally before any HTTP call. Published by `unifi schema`
    /// as kind `conflict`, exit code 6.
    Conflict(String),
    /// The controller does not serve this endpoint at all. Distinct from
    /// `NotFound` because the whole API is absent, not one record, so there is
    /// no other identifier worth trying. Published as kind `unsupported`.
    Unsupported {
        endpoint: String,
        reason: UnsupportedReason,
    },
    Other(String),
}

/// How the controller revealed that an endpoint is absent.
///
/// Both forms mean the same thing to a caller, so they share one error kind.
/// They are kept apart because the message has to say what actually happened:
/// guessing at the wrong one sends the reader looking for a fault that is not
/// there.
#[derive(Debug)]
pub enum UnsupportedReason {
    /// The endpoint answered with something other than JSON. UniFi OS proxies
    /// a request for an application it does not have to its own web UI, so the
    /// call returns 200 with an HTML page.
    NotJson { content_type: String },
    /// The controller rejected the endpoint itself rather than the request.
    /// UniFi Network answers an unknown legacy resource this way, so a
    /// firmware that has dropped an endpoint is indistinguishable from one
    /// that never had it, and neither is worth retrying.
    Removed,
}

/// Scan a single error string for TLS certificate failure markers. rustls
/// reports these as "invalid peer certificate: <reason>", so "certificate" is
/// the reliable marker; "self-signed" is matched defensively.
fn text_indicates_cert_failure(s: &str) -> bool {
    let s = s.to_lowercase();
    s.contains("certificate") || s.contains("self-signed")
}

/// Walk a reqwest error's source chain looking for a TLS certificate failure.
/// reqwest's own Display is only "error sending request for url (...)", so the
/// cert cause must be read from the nested chain. The top-level Display and the
/// Debug form are deliberately not scanned: both embed the request URL, so a
/// controller hostname containing a word like "certificate" would otherwise be
/// misread as a certificate failure on any unrelated network error.
fn reqwest_is_cert_failure(e: &reqwest::Error) -> bool {
    use std::error::Error;
    let mut source: Option<&dyn std::error::Error> = e.source();
    while let Some(err) = source {
        if text_indicates_cert_failure(&err.to_string()) {
            return true;
        }
        source = err.source();
    }
    false
}

impl ApiError {
    /// True when the error indicates a TLS certificate verification failure, so
    /// callers can offer the `--accept-invalid-certs` opt-out.
    pub fn is_tls_cert_error(&self) -> bool {
        match self {
            ApiError::Http(e) => reqwest_is_cert_failure(e),
            other => text_indicates_cert_failure(&other.to_string()),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Http(e) => {
                write!(f, "HTTP error: {e}")?;
                // Certificate failures are checked first because reqwest also
                // classifies a failed TLS handshake as a connect error.
                if reqwest_is_cert_failure(e) {
                    write!(
                        f,
                        "\n  Hint: TLS certificate verification failed. For a trusted controller \
                         with a self-signed cert, run 'unifi config init' to trust it \
                         interactively, or pass --accept-invalid-certs (or set \
                         UNIFI_ACCEPT_INVALID_CERTS=true or accept_invalid_certs = true in config)"
                    )?;
                } else if e.is_connect() {
                    write!(
                        f,
                        "\n  Hint: Check that the host is reachable and the URL is correct"
                    )?;
                } else if e.is_timeout() {
                    write!(f, "\n  Hint: Request timed out. Is the controller running?")?;
                } else {
                    let msg = e.to_string().to_lowercase();
                    if msg.contains("dns") || msg.contains("resolve") {
                        write!(
                            f,
                            "\n  Hint: Could not resolve hostname. Check the host value"
                        )?;
                    }
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
            ApiError::Conflict(msg) => write!(f, "{msg}"),
            ApiError::Unsupported { endpoint, reason } => {
                match reason {
                    UnsupportedReason::NotJson { content_type } => write!(
                        f,
                        "This controller does not serve {endpoint}: it answered with \
                         {content_type} instead of JSON"
                    )?,
                    UnsupportedReason::Removed => write!(
                        f,
                        "This controller does not serve {endpoint}: it rejected the endpoint \
                         itself, so no parameter or identifier would change the result"
                    )?,
                }
                if endpoint.contains("/protect/") {
                    write!(
                        f,
                        "\n  Hint: UniFi OS proxies the request to its web UI when the Protect \
                         application is not installed on the controller"
                    )?;
                } else if endpoint.contains("/stat/event") {
                    write!(
                        f,
                        "\n  Hint: UniFi Network 9 removed the REST event log, and this \
                         controller does not serve /rest/alarm either. The remaining event \
                         stream is the events WebSocket, which this CLI does not consume"
                    )?;
                }
                Ok(())
            }
            ApiError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::Http(e) => Some(e),
            _ => None,
        }
    }
}

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
