mod client;
pub mod types;

pub use client::ClientOptions;
pub use client::ProtectSession;
pub use client::UnifiClient;
pub use client::error_for_status;
pub use types::{
    ApiError, Client, DEFAULT_DNS_TTL_SECONDS, Device, DeviceWithPorts, DnsRecordType, Event,
    HealthSubsystem, HostSystem, LastConnection, LegacyClient, LegacyDevice, LegacyResponse,
    NamedWanInterface, Network, PortEntry, PortForward, StaticDnsRecord, StaticDnsWrite, SysInfo,
    UnsupportedReason, WanInterface, format_bytes, format_mac, format_uptime, normalize_mac,
    resolve_static_dns,
};
