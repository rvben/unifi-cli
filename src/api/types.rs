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

/// UniFi's documented default TTL for A, AAAA, and CNAME policies.
pub const DEFAULT_DNS_TTL_SECONDS: u32 = 14400;

/// A static DNS record type UniFi can store.
///
/// The Integration API names these `A_RECORD`, `AAAA_RECORD`, and so on. The
/// v2 `static-dns` API uses the usual DNS tokens (`A`, `AAAA`, ...). Commands
/// accept either spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsRecordType {
    A,
    Aaaa,
    Cname,
    Mx,
    Txt,
    Srv,
    Ns,
}

impl DnsRecordType {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        let token = upper.strip_suffix("_RECORD").unwrap_or(&upper);
        match token {
            "A" => Ok(Self::A),
            "AAAA" => Ok(Self::Aaaa),
            "CNAME" => Ok(Self::Cname),
            "MX" => Ok(Self::Mx),
            "TXT" => Ok(Self::Txt),
            "SRV" => Ok(Self::Srv),
            "NS" => Ok(Self::Ns),
            _ => Err(format!(
                "unknown DNS record type '{raw}'. Valid types: A, AAAA, CNAME, MX, TXT, SRV, NS"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::Aaaa => "AAAA",
            Self::Cname => "CNAME",
            Self::Mx => "MX",
            Self::Txt => "TXT",
            Self::Srv => "SRV",
            Self::Ns => "NS",
        }
    }

    pub fn integration_name(self) -> Option<&'static str> {
        match self {
            Self::A => Some("A_RECORD"),
            Self::Aaaa => Some("AAAA_RECORD"),
            Self::Cname => Some("CNAME_RECORD"),
            Self::Mx => Some("MX_RECORD"),
            Self::Txt => Some("TXT_RECORD"),
            Self::Srv => Some("SRV_RECORD"),
            Self::Ns => None,
        }
    }

    pub fn accepts_ttl(self) -> bool {
        matches!(self, Self::A | Self::Aaaa | Self::Cname | Self::Ns)
    }

    pub fn can_create(self) -> bool {
        !matches!(self, Self::Ns)
    }
}

/// A static DNS record after both controller APIs have been normalized.
///
/// Commands emit this shape. The Integration DNS-policies API and the v2
/// `static-dns` API disagree on field names, so neither raw object is ever
/// serialized to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticDnsRecord {
    pub id: String,
    pub name: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: Option<u32>,
    pub enabled: bool,
    pub priority: Option<u32>,
    pub weight: Option<u32>,
    pub port: Option<u32>,
}

impl StaticDnsRecord {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "type": self.record_type.as_str(),
            "value": self.value,
            "ttl": self.ttl,
            "enabled": self.enabled,
            "priority": self.priority,
            "weight": self.weight,
            "port": self.port,
        })
    }
}

/// Fields for creating a record, or the merged result of an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticDnsWrite {
    pub name: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: Option<u32>,
    pub enabled: bool,
    pub priority: Option<u32>,
    pub weight: Option<u32>,
    pub port: Option<u32>,
}

impl StaticDnsWrite {
    /// Start an update from what the controller holds.
    ///
    /// The v2 `static-dns` API stores a TTL on every record, including MX and
    /// TXT, while `validate` rejects a TTL on those types. Copying it across
    /// would make an update that never mentioned `--ttl` fail, so it is only
    /// carried for types that accept one.
    pub fn from_record(record: &StaticDnsRecord) -> Self {
        Self {
            name: record.name.clone(),
            record_type: record.record_type,
            value: record.value.clone(),
            ttl: record.ttl.filter(|_| record.record_type.accepts_ttl()),
            enabled: record.enabled,
            priority: record.priority,
            weight: record.weight,
            port: record.port,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("DNS record name must not be empty".into());
        }
        if !self.record_type.can_create() {
            return Err(
                "NS records cannot be created or updated; delete an existing NS record with `dns delete`"
                    .into(),
            );
        }
        match self.record_type {
            DnsRecordType::A => {
                if !is_ipv4(&self.value) {
                    return Err(format!(
                        "A record value must be an IPv4 address, got '{}'",
                        self.value
                    ));
                }
            }
            DnsRecordType::Aaaa => {
                if !is_ipv6(&self.value) {
                    return Err(format!(
                        "AAAA record value must be an IPv6 address, got '{}'",
                        self.value
                    ));
                }
            }
            DnsRecordType::Mx => {
                if self.priority.is_none() {
                    return Err("MX records require --priority".into());
                }
            }
            DnsRecordType::Srv => {
                split_srv_owner(&self.name)?;
                if self.priority.is_none() || self.weight.is_none() || self.port.is_none() {
                    return Err("SRV records require --priority, --weight, and --port".into());
                }
            }
            DnsRecordType::Cname | DnsRecordType::Txt | DnsRecordType::Ns => {}
        }
        if self.priority.is_some()
            && !matches!(self.record_type, DnsRecordType::Mx | DnsRecordType::Srv)
        {
            return Err("--priority is only valid for MX and SRV records".into());
        }
        if (self.weight.is_some() || self.port.is_some()) && self.record_type != DnsRecordType::Srv
        {
            return Err("--weight and --port are only valid for SRV records".into());
        }
        if self.ttl.is_some() && !self.record_type.accepts_ttl() {
            return Err(format!(
                "--ttl is not accepted for {} records on UniFi Network 10.1+",
                self.record_type.as_str()
            ));
        }
        Ok(())
    }

    pub fn integration_body(&self) -> Result<serde_json::Value, String> {
        self.validate()?;
        let type_name = self
            .record_type
            .integration_name()
            .ok_or_else(|| "NS records are not supported by the Integration API".to_string())?;
        let mut body = serde_json::json!({
            "type": type_name,
            "enabled": self.enabled,
        });
        let value = strip_trailing_dot(&self.value);
        match self.record_type {
            DnsRecordType::A => {
                body["domain"] = self.name.clone().into();
                body["ipv4Address"] = value.into();
                body["ttlSeconds"] = self.ttl.unwrap_or(DEFAULT_DNS_TTL_SECONDS).into();
            }
            DnsRecordType::Aaaa => {
                body["domain"] = self.name.clone().into();
                body["ipv6Address"] = value.into();
                body["ttlSeconds"] = self.ttl.unwrap_or(DEFAULT_DNS_TTL_SECONDS).into();
            }
            DnsRecordType::Cname => {
                body["domain"] = self.name.clone().into();
                body["targetDomain"] = value.into();
                body["ttlSeconds"] = self.ttl.unwrap_or(DEFAULT_DNS_TTL_SECONDS).into();
            }
            DnsRecordType::Mx => {
                body["domain"] = self.name.clone().into();
                body["mailServerDomain"] = value.into();
                body["priority"] = self.priority.expect("validated").into();
            }
            DnsRecordType::Txt => {
                body["domain"] = self.name.clone().into();
                body["text"] = self.value.clone().into();
            }
            DnsRecordType::Srv => {
                let (service, protocol, domain) = split_srv_owner(&self.name)?;
                body["service"] = service.into();
                body["protocol"] = protocol.into();
                body["domain"] = domain.into();
                body["serverDomain"] = value.into();
                body["priority"] = self.priority.expect("validated").into();
                body["weight"] = self.weight.expect("validated").into();
                body["port"] = self.port.expect("validated").into();
            }
            DnsRecordType::Ns => unreachable!("validate rejects NS"),
        }
        Ok(body)
    }

    pub fn legacy_body(&self) -> Result<serde_json::Value, String> {
        self.validate()?;
        // A trailing dot on a TXT value is data, not a root label.
        let value = if self.record_type == DnsRecordType::Txt {
            self.value.clone()
        } else {
            strip_trailing_dot(&self.value)
        };
        let mut body = serde_json::json!({
            "enabled": self.enabled,
            "key": self.name,
            "record_type": self.record_type.as_str(),
            "value": value,
        });
        if self.record_type.accepts_ttl() {
            body["ttl"] = self.ttl.unwrap_or(DEFAULT_DNS_TTL_SECONDS).into();
        }
        if let Some(priority) = self.priority {
            body["priority"] = priority.into();
        }
        if let Some(weight) = self.weight {
            body["weight"] = weight.into();
        }
        if let Some(port) = self.port {
            body["port"] = port.into();
        }
        Ok(body)
    }
}

/// A DNS policy as the Integration API returns it.
///
/// The schema is polymorphic on `type`. Optional fields cover every static
/// record variant; domain-forward policies are dropped before they become a
/// `StaticDnsRecord`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationDnsPolicy {
    #[serde(rename = "type")]
    pub policy_type: String,
    pub id: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub domain: Option<String>,
    pub ipv4_address: Option<String>,
    pub ipv6_address: Option<String>,
    pub target_domain: Option<String>,
    pub mail_server_domain: Option<String>,
    pub text: Option<String>,
    pub server_domain: Option<String>,
    pub ttl_seconds: Option<u32>,
    pub priority: Option<u32>,
    pub weight: Option<u32>,
    pub port: Option<u32>,
    pub service: Option<String>,
    pub protocol: Option<String>,
}

impl IntegrationDnsPolicy {
    pub fn to_static_record(&self) -> Option<StaticDnsRecord> {
        if self.policy_type.eq_ignore_ascii_case("FORWARD_DOMAIN") {
            return None;
        }
        let record_type = DnsRecordType::parse(&self.policy_type).ok()?;
        let id = self.id.clone().unwrap_or_default();
        let name = match record_type {
            DnsRecordType::Srv => srv_owner_from_parts(
                self.service.as_deref(),
                self.protocol.as_deref(),
                self.domain.as_deref(),
            ),
            _ => self.domain.clone().unwrap_or_default(),
        };
        let value = match record_type {
            DnsRecordType::A => self.ipv4_address.clone(),
            DnsRecordType::Aaaa => self.ipv6_address.clone(),
            DnsRecordType::Cname => self.target_domain.clone(),
            DnsRecordType::Mx => self.mail_server_domain.clone(),
            DnsRecordType::Txt => self.text.clone(),
            DnsRecordType::Srv => self.server_domain.clone(),
            DnsRecordType::Ns => None,
        }
        .unwrap_or_default();
        Some(StaticDnsRecord {
            id,
            name,
            record_type,
            value,
            ttl: self.ttl_seconds.filter(|ttl| *ttl > 0),
            enabled: self.enabled,
            priority: self.priority,
            weight: self.weight,
            port: self.port,
        })
    }
}

/// A record from `/proxy/network/v2/api/site/{site}/static-dns`.
#[derive(Debug, Deserialize)]
pub struct LegacyStaticDns {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(default)]
    pub enabled: bool,
    pub key: Option<String>,
    pub record_type: Option<String>,
    pub value: Option<String>,
    pub ttl: Option<u32>,
    pub port: Option<u32>,
    pub priority: Option<u32>,
    pub weight: Option<u32>,
}

impl LegacyStaticDns {
    pub fn to_static_record(&self) -> Option<StaticDnsRecord> {
        let record_type = DnsRecordType::parse(self.record_type.as_deref().unwrap_or("")).ok()?;
        Some(StaticDnsRecord {
            id: self.id.clone(),
            name: self.key.clone().unwrap_or_default(),
            record_type,
            value: self.value.clone().unwrap_or_default(),
            ttl: self.ttl.filter(|ttl| *ttl > 0),
            enabled: self.enabled,
            priority: self.priority,
            weight: self.weight,
            port: self.port,
        })
    }
}

/// Find a record by exact id, or by case-insensitive exact name.
///
/// A name that matches more than one record is a conflict rather than a guess:
/// the caller may be about to delete it.
pub fn resolve_static_dns<'a>(
    records: &'a [StaticDnsRecord],
    identifier: &str,
) -> Result<&'a StaticDnsRecord, ApiError> {
    if let Some(by_id) = records.iter().find(|record| record.id == identifier) {
        return Ok(by_id);
    }
    let needle = identifier.to_ascii_lowercase();
    let mut matches: Vec<&StaticDnsRecord> = records
        .iter()
        .filter(|record| record.name.to_ascii_lowercase() == needle)
        .collect();
    match matches.len() {
        0 => Err(ApiError::NotFound(format!("DNS record '{identifier}'"))),
        1 => Ok(matches.pop().expect("checked len == 1")),
        _ => {
            let ids = matches
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(ApiError::Conflict(format!(
                "'{identifier}' matches {} DNS records: {ids}. Use the id.",
                matches.len()
            )))
        }
    }
}

fn is_ipv4(value: &str) -> bool {
    matches!(
        value.parse::<std::net::IpAddr>(),
        Ok(std::net::IpAddr::V4(_))
    )
}

fn is_ipv6(value: &str) -> bool {
    matches!(
        value.parse::<std::net::IpAddr>(),
        Ok(std::net::IpAddr::V6(_))
    )
}

fn strip_trailing_dot(value: &str) -> String {
    value.trim_end_matches('.').to_string()
}

/// Split `_service._protocol.domain` into the parts the Integration API wants.
pub fn split_srv_owner(name: &str) -> Result<(String, String, String), String> {
    let parts: Vec<&str> = name.splitn(3, '.').collect();
    if parts.len() < 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(format!(
            "SRV name '{name}' must be _service._protocol.domain, for example _ldap._tcp.example.com"
        ));
    }
    let with_underscore = |label: &str| {
        if label.starts_with('_') {
            label.to_string()
        } else {
            format!("_{label}")
        }
    };
    Ok((
        with_underscore(parts[0]),
        with_underscore(parts[1]),
        parts[2].to_string(),
    ))
}

fn srv_owner_from_parts(
    service: Option<&str>,
    protocol: Option<&str>,
    domain: Option<&str>,
) -> String {
    match (service, protocol, domain) {
        (Some(service), Some(protocol), Some(domain))
            if !service.is_empty() && !protocol.is_empty() && !domain.is_empty() =>
        {
            format!("{service}.{protocol}.{domain}")
        }
        _ => domain.unwrap_or_default().to_string(),
    }
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
                    "\n  Hint: Create or replace the key in UniFi Network > Integrations\n  Guide: https://help.ui.com/hc/en-us/articles/30076656117655-Getting-Started-with-the-Official-UniFi-API"
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
