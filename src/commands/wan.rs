use crate::api::{NamedWanInterface, UnifiClient};
use crate::output::OutputConfig;

fn record(wan: &NamedWanInterface) -> serde_json::Value {
    let interface = &wan.interface;
    serde_json::json!({
        "slot": wan.slot, "name": interface.name, "interface": interface.ifname,
        "enabled": interface.enable, "up": interface.up, "ip": interface.ip,
        "availability": interface.availability, "latency_ms": interface.latency,
        "speed_mbps": interface.speed, "rx_bytes": interface.rx_bytes,
        "tx_bytes": interface.tx_bytes, "rx_rate": interface.rx_rate,
        "tx_rate": interface.tx_rate, "cellular": interface.mbb.is_some(),
        "cellular_state": interface.mbb_state,
        "signal_percent": interface.mbb.as_ref().and_then(|m| m.signal_pct),
        "radio_access": interface.mbb.as_ref().and_then(|m| m.rat.as_deref()),
        "lte_rsrp": interface.mbb.as_ref().and_then(|m| m.lte_rsrp),
        "lte_rsrq": interface.mbb.as_ref().and_then(|m| m.lte_rsrq),
        "lte_sinr": interface.mbb.as_ref().and_then(|m| m.lte_sinr),
    })
}

pub async fn list(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let interfaces = client.list_wan_interfaces().await?;
    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(
            &interfaces.iter().map(record).collect::<Vec<_>>(),
        )?);
    } else {
        println!(
            "{:<6} {:<24} {:<8} {:<8} {:<12} IP",
            "Slot", "Name", "Enabled", "Up", "Cellular"
        );
        println!("{}", "-".repeat(86));
        for wan in &interfaces {
            println!(
                "{:<6} {:<24} {:<8} {:<8} {:<12} {}",
                wan.slot,
                wan.interface.name.as_deref().unwrap_or("-"),
                if wan.interface.enable { "yes" } else { "no" },
                if wan.interface.up { "yes" } else { "no" },
                if wan.interface.mbb.is_some() {
                    "yes"
                } else {
                    "no"
                },
                wan.interface.ip.as_deref().unwrap_or("-")
            );
        }
    }
    out.print_message(&format!("\n{} WAN interfaces", interfaces.len()));
    Ok(())
}
