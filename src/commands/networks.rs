use std::net::Ipv4Addr;

use owo_colors::OwoColorize;

use crate::api::types::LegacyNetwork;
use crate::api::{ApiError, DhcpDns, UnifiClient};
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
    let dns_servers = network.dns_servers();

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

/// Build the requested DHCP DNS setting from `networks set-dns` arguments.
///
/// clap has already parsed each server as IPv4 and made `--auto` exclusive
/// with a server list; this enforces what clap cannot express.
pub fn dhcp_dns_from_args(auto: bool, servers: &[Ipv4Addr]) -> Result<DhcpDns, String> {
    if auto {
        return Ok(DhcpDns::Auto);
    }
    if servers.is_empty() {
        return Err("Give at least one DNS server, or --auto for the gateway's own DNS.".into());
    }
    if servers.len() > DhcpDns::MAX_SERVERS {
        return Err(format!(
            "A network hands out at most {} DNS servers, got {}.",
            DhcpDns::MAX_SERVERS,
            servers.len()
        ));
    }
    for (index, server) in servers.iter().enumerate() {
        if servers[..index].contains(server) {
            return Err(format!("DNS server {server} is listed twice."));
        }
    }
    Ok(DhcpDns::Servers(
        servers.iter().map(ToString::to_string).collect(),
    ))
}

pub enum SetDnsOutcome {
    /// The network now hands out the requested servers, whether it was
    /// changed or already did.
    Done,
    Declined,
}

/// Point the DNS servers a network's DHCP server hands out at `dns`.
///
/// `confirm` receives a one-line summary of the change and returns whether to
/// proceed. It is not called when the network already hands out exactly
/// `dns`: nothing is written then, and the result reports `changed: false`.
pub async fn set_dns<F>(
    client: &UnifiClient,
    identifier: &str,
    dns: &DhcpDns,
    out: OutputConfig,
    confirm: F,
) -> Result<SetDnsOutcome, Box<dyn std::error::Error>>
where
    F: FnOnce(&str) -> std::io::Result<bool>,
{
    let config = client.get_network_config(identifier).await?;
    config.check_dhcp_dns_settable()?;
    let before = config.network().dns_setting();

    if before == *dns {
        print_set_dns(config.network(), &before, &before, &out);
        return Ok(SetDnsOutcome::Done);
    }

    let name = network_label(config.network());
    if !confirm(&format!(
        "{name}: DHCP DNS {} -> {}",
        before.describe(),
        dns.describe()
    ))? {
        return Ok(SetDnsOutcome::Declined);
    }

    let updated = client.set_network_dhcp_dns(&config, dns).await?;
    let after = updated.dns_setting();
    if after != *dns {
        return Err(ApiError::Other(format!(
            "The controller accepted the change, but network '{name}' now hands out {} instead of {}.",
            after.describe(),
            dns.describe()
        ))
        .into());
    }
    print_set_dns(&updated, &before, &after, &out);
    Ok(SetDnsOutcome::Done)
}

fn network_label(network: &LegacyNetwork) -> &str {
    network.name.as_deref().unwrap_or(&network.id)
}

fn print_set_dns(network: &LegacyNetwork, before: &DhcpDns, after: &DhcpDns, out: &OutputConfig) {
    let changed = before != after;
    let name = network_label(network);
    let message = if changed {
        format!(
            "DHCP DNS on {name}: {} -> {}. Clients pick this up when they renew their lease.",
            before.describe(),
            after.describe()
        )
    } else {
        format!(
            "DHCP DNS on {name} is already {}; nothing changed.",
            after.describe()
        )
    };
    out.print_result(
        &serde_json::json!({
            "status": "ok",
            "action": "set-dns",
            "id": network.id,
            "name": network.name,
            "changed": changed,
            "dns_custom": after.is_custom(),
            "dns_servers": after.servers(),
            "previous_dns_custom": before.is_custom(),
            "previous_dns_servers": before.servers(),
        }),
        &message,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(raw: &str) -> Ipv4Addr {
        raw.parse().unwrap()
    }

    #[test]
    fn auto_ignores_the_server_list() {
        assert_eq!(dhcp_dns_from_args(true, &[]), Ok(DhcpDns::Auto));
    }

    #[test]
    fn servers_keep_their_order() {
        assert_eq!(
            dhcp_dns_from_args(false, &[ip("198.51.100.53"), ip("192.0.2.53")]),
            Ok(DhcpDns::Servers(vec![
                "198.51.100.53".into(),
                "192.0.2.53".into()
            ]))
        );
    }

    #[test]
    fn four_servers_fill_every_slot() {
        let servers = [
            ip("192.0.2.1"),
            ip("192.0.2.2"),
            ip("192.0.2.3"),
            ip("192.0.2.4"),
        ];
        assert_eq!(
            dhcp_dns_from_args(false, &servers).unwrap().servers().len(),
            4
        );
    }

    #[test]
    fn a_fifth_server_is_refused() {
        let servers = [
            ip("192.0.2.1"),
            ip("192.0.2.2"),
            ip("192.0.2.3"),
            ip("192.0.2.4"),
            ip("192.0.2.5"),
        ];
        let err = dhcp_dns_from_args(false, &servers).unwrap_err();
        assert!(err.contains("at most 4"), "{err}");
    }

    #[test]
    fn a_duplicate_server_is_refused() {
        let err = dhcp_dns_from_args(false, &[ip("192.0.2.1"), ip("192.0.2.1")]).unwrap_err();
        assert!(err.contains("192.0.2.1"), "{err}");
    }

    #[test]
    fn no_servers_and_no_auto_is_refused() {
        let err = dhcp_dns_from_args(false, &[]).unwrap_err();
        assert!(err.contains("--auto"), "{err}");
    }
}
