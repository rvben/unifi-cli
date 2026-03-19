use tabled::{Table, Tabled};

use crate::api::UnifiClient;
use crate::output::OutputConfig;

#[derive(Tabled)]
struct NetworkRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "VLAN")]
    vlan: String,
    #[tabled(rename = "Enabled")]
    enabled: String,
    #[tabled(rename = "Default")]
    is_default: String,
}

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let networks = client.list_networks().await?;

    if out.json {
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
        return Ok(());
    }

    let rows: Vec<NetworkRow> = networks
        .iter()
        .map(|n| NetworkRow {
            name: n.name.as_deref().unwrap_or("-").to_string(),
            vlan: n
                .vlan_id
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            enabled: if n.enabled { "yes" } else { "no" }.into(),
            is_default: if n.default { "yes" } else { "no" }.into(),
        })
        .collect();

    out.print_data(&Table::new(rows).to_string());
    Ok(())
}
