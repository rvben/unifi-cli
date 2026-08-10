mod client;
pub mod types;

pub use client::ClientOptions;
pub use client::ProtectSession;
pub use client::UnifiClient;
pub use client::error_for_status;
pub use types::{
    ApiError, Client, Device, DeviceWithPorts, Event, HealthSubsystem, HostSystem, LastConnection,
    LegacyClient, LegacyDevice, LegacyResponse, Network, PortEntry, SysInfo, UnsupportedReason,
    format_bytes, format_mac, format_uptime, normalize_mac,
};
