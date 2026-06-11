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
