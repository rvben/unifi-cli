mod client;
mod types;

pub use client::UnifiClient;
pub use types::{
    ApiError, Client, Device, HealthSubsystem, LegacyClient, LegacyDevice, Network, SysInfo,
    format_bytes, format_mac, format_uptime, normalize_mac,
};
