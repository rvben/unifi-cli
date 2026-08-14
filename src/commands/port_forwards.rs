use owo_colors::OwoColorize;

use crate::api::{PortForward, UnifiClient};
use crate::output::{OutputConfig, use_color};

fn record(forward: &PortForward) -> serde_json::Value {
    serde_json::json!({
        "id": forward.id,
        "name": forward.name,
        "enabled": forward.enabled,
        "protocol": forward.proto,
        "source": forward.src,
        "source_port": forward.src_port,
        "external_port": forward.dst_port,
        "destination": forward.fwd,
        "destination_port": forward.fwd_port,
        "interface": forward.pfwd_interface,
        "logging": forward.log,
    })
}

pub async fn list(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let forwards = client.list_port_forwards().await?;
    if out.is_json() {
        let rows: Vec<_> = forwards.iter().map(record).collect();
        out.print_data(&serde_json::to_string_pretty(&rows)?);
    } else {
        let header = format!(
            "{:<28} {:<8} {:<10} {:<14} Destination",
            "Name", "Enabled", "Protocol", "Interface"
        );
        if use_color() {
            println!("{}", header.bold());
        } else {
            println!("{header}");
        }
        println!("{}", "-".repeat(86));
        for forward in &forwards {
            let destination = match (&forward.fwd, &forward.fwd_port) {
                (Some(host), Some(port)) => format!("{host}:{port}"),
                (Some(host), None) => host.clone(),
                _ => "-".into(),
            };
            println!(
                "{:<28} {:<8} {:<10} {:<14} {}",
                forward.name.as_deref().unwrap_or("-"),
                if forward.enabled { "yes" } else { "no" },
                forward.proto.as_deref().unwrap_or("-"),
                forward.pfwd_interface.as_deref().unwrap_or("-"),
                destination
            );
        }
    }
    out.print_message(&format!("\n{} port forwards", forwards.len()));
    Ok(())
}

pub async fn show(
    client: &UnifiClient,
    identifier: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward = client.get_port_forward(identifier).await?;
    let row = record(&forward);
    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&row)?);
    } else if let Some(fields) = row.as_object() {
        for (key, value) in fields {
            let rendered = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            println!("{:<20} {}", key.replace('_', " "), rendered);
        }
    }
    Ok(())
}
