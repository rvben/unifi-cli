use tabled::{Table, Tabled};

use crate::api::{format_mac, UnifiClient};

#[derive(Tabled)]
struct DeviceRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Model")]
    model: String,
    #[tabled(rename = "MAC")]
    mac: String,
    #[tabled(rename = "IP")]
    ip: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "Firmware")]
    firmware: String,
}

pub async fn list(
    client: &mut UnifiClient,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let devices = client.list_devices().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &devices
                    .iter()
                    .map(|d| serde_json::json!({
                        "name": d.name,
                        "model": d.model,
                        "mac": d.mac_address,
                        "ip": d.ip_address,
                        "state": d.state,
                        "firmware": d.firmware_version,
                    }))
                    .collect::<Vec<_>>()
            )?
        );
        return Ok(());
    }

    let rows: Vec<DeviceRow> = devices
        .iter()
        .map(|d| DeviceRow {
            name: d.name.as_deref().unwrap_or("-").to_string(),
            model: d.model.as_deref().unwrap_or("-").to_string(),
            mac: d
                .mac_address
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into()),
            ip: d.ip_address.as_deref().unwrap_or("-").to_string(),
            state: d.state.as_deref().unwrap_or("-").to_string(),
            firmware: d.firmware_version.as_deref().unwrap_or("-").to_string(),
        })
        .collect();

    println!("{}", Table::new(rows));
    println!("\n{} devices", devices.len());
    Ok(())
}

pub async fn restart(
    client: &UnifiClient,
    mac: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    client.restart_device(mac).await?;
    println!("Restarting {}", format_mac(mac));
    Ok(())
}

pub async fn locate(
    client: &UnifiClient,
    mac: &str,
    off: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    client.locate_device(mac, !off).await?;
    if off {
        println!("Stopped locating {}", format_mac(mac));
    } else {
        println!("Locating {} (LED blinking)", format_mac(mac));
    }
    Ok(())
}
