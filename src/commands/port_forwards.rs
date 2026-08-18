use owo_colors::OwoColorize;

use crate::api::{PortForward, UnifiClient};
use crate::output::{OutputConfig, use_color};

/// Renders an absent or blank value as the dash the other detail views use.
fn text(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or("-")
        .to_string()
}

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
        return Ok(());
    }

    let name = forward.name.as_deref().unwrap_or("-");
    println!(
        "{}",
        if use_color() {
            format!("{}", name.bold())
        } else {
            name.into()
        }
    );
    for (label, value) in [
        ("ID", forward.id.clone()),
        ("Enabled", forward.enabled.to_string()),
        ("Protocol", text(forward.proto.as_deref())),
        ("Interface", text(forward.pfwd_interface.as_deref())),
        ("Source", text(forward.src.as_deref())),
        ("Source port", text(forward.src_port.as_deref())),
        ("External port", text(forward.dst_port.as_deref())),
        ("Destination", text(forward.fwd.as_deref())),
        ("Destination port", text(forward.fwd_port.as_deref())),
        ("Logging", forward.log.to_string()),
    ] {
        println!("  {label:<18} {value}");
    }
    Ok(())
}
