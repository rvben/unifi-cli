use owo_colors::OwoColorize;

use crate::api::UnifiClient;
use crate::output::{OutputConfig, use_color};

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let networks = client.list_networks().await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(
            &networks
                .iter()
                .map(|n| {
                    serde_json::json!({
                        "name": n.name,
                        "vlan_id": n.vlan_id,
                        "enabled": n.enabled,
                        "default": n.default,
                    })
                })
                .collect::<Vec<_>>(),
        )?);
        out.print_message(&format!("\n{} networks", networks.len()));
        return Ok(());
    }

    let color = use_color();
    let header = format!(
        "{:<30} {:<8} {:<10} {}",
        "Name", "VLAN", "Enabled", "Default"
    );
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(58).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(58));
    }

    for n in &networks {
        let name = n.name.as_deref().unwrap_or("-");
        let vlan = n
            .vlan_id
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into());
        let enabled = if n.enabled { "yes" } else { "no" };
        let is_default = if n.default { "yes" } else { "no" };

        if color {
            println!(
                " {:<29} {:<8} {:<10} {}",
                name.bold(),
                vlan,
                enabled,
                is_default,
            );
        } else {
            println!(" {:<29} {:<8} {:<10} {}", name, vlan, enabled, is_default);
        }
    }

    out.print_message(&format!("\n{} networks", networks.len()));
    Ok(())
}

pub async fn show(
    client: &UnifiClient,
    identifier: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let network = client.get_network_detail(identifier).await?;
    let dns_servers: Vec<&str> = [
        network.dhcpd_dns_1.as_deref(),
        network.dhcpd_dns_2.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|server| !server.is_empty())
    .collect();

    let record = serde_json::json!({
        "id": network.id,
        "name": network.name,
        "purpose": network.purpose,
        "vlan_id": network.vlan,
        "subnet": network.ip_subnet,
        "enabled": network.enabled,
        "dhcp_enabled": network.dhcpd_enabled,
        "dns_custom": network.dhcpd_dns_enabled,
        "dns_servers": dns_servers,
        "mdns_enabled": network.mdns_enabled,
        "cellular_backup_enabled": network.lte_lan_enabled,
    });

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&record)?);
        return Ok(());
    }

    let name = record["name"].as_str().unwrap_or("-");
    println!(
        "{}",
        if use_color() {
            format!("{}", name.bold())
        } else {
            name.into()
        }
    );
    for (label, value) in [
        ("ID", record["id"].as_str().unwrap_or("-").to_string()),
        (
            "Purpose",
            record["purpose"].as_str().unwrap_or("-").to_string(),
        ),
        (
            "VLAN",
            record["vlan_id"]
                .as_u64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
        (
            "Subnet",
            record["subnet"].as_str().unwrap_or("-").to_string(),
        ),
        (
            "DHCP",
            record["dhcp_enabled"]
                .as_bool()
                .unwrap_or(false)
                .to_string(),
        ),
        ("DNS", dns_servers.join(", ")),
        (
            "mDNS",
            record["mdns_enabled"]
                .as_bool()
                .unwrap_or(false)
                .to_string(),
        ),
        (
            "Cellular backup",
            record["cellular_backup_enabled"]
                .as_bool()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
    ] {
        println!("  {label:<18} {value}");
    }
    Ok(())
}
