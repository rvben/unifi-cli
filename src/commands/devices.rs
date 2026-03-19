use tabled::{Table, Tabled};

use crate::api::{Device, UnifiClient, format_mac};
use crate::output::OutputConfig;

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

fn render_devices(devices: &[Device], out: &OutputConfig) {
    if out.json {
        out.print_data(
            &serde_json::to_string_pretty(
                &devices
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "name": d.name,
                            "model": d.model,
                            "mac": d.mac_address,
                            "ip": d.ip_address,
                            "state": d.state,
                            "firmware": d.firmware_version,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
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

        out.print_data(&Table::new(rows).to_string());
    }
    out.print_message(&format!("\n{} devices", devices.len()));
}

pub async fn list(
    client: &mut UnifiClient,
    out: OutputConfig,
    watch: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(interval) = watch {
        loop {
            eprint!("\x1B[2J\x1B[H");
            let devices = client.list_devices().await?;
            render_devices(&devices, &out);
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        }
    } else {
        let devices = client.list_devices().await?;
        render_devices(&devices, &out);
        Ok(())
    }
}

pub async fn restart(
    client: &UnifiClient,
    mac: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.restart_device(mac).await?;
    out.print_result(
        &serde_json::json!({"status": "ok", "action": "restart", "mac": format_mac(mac)}),
        &format!("Restarting {}", format_mac(mac)),
    );
    Ok(())
}

pub async fn locate(
    client: &UnifiClient,
    mac: &str,
    off: bool,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    client.locate_device(mac, !off).await?;
    let action = if off { "locate_off" } else { "locate_on" };
    let msg = if off {
        format!("Stopped locating {}", format_mac(mac))
    } else {
        format!("Locating {} (LED blinking)", format_mac(mac))
    };
    out.print_result(
        &serde_json::json!({"status": "ok", "action": action, "mac": format_mac(mac)}),
        &msg,
    );
    Ok(())
}
