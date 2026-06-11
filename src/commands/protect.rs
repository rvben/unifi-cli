use owo_colors::OwoColorize;

use crate::api::{ProtectSession, UnifiClient, format_bytes, format_mac, format_uptime};
use crate::output::{OutputConfig, use_color};

pub async fn cameras_list(
    client: &UnifiClient,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let cameras = client.list_protect_cameras().await?;

    if out.is_json() {
        out.print_data(
            &serde_json::to_string_pretty(
                &cameras
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "name": c.name,
                            "mac": c.mac,
                            "state": c.state,
                            "model_key": c.model_key,
                            "mic_enabled": c.is_mic_enabled,
                            "video_mode": c.video_mode,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
        let color = use_color();

        let col = |min: usize, label_len: usize, vals: Vec<usize>| -> usize {
            vals.into_iter().max().unwrap_or(0).max(label_len).max(min) + 2
        };
        let names: Vec<&str> = cameras
            .iter()
            .map(|c| c.name.as_deref().unwrap_or("-"))
            .collect();
        let name_w = col(4, 4, names.iter().map(|n| n.len()).collect());

        let header = format!(
            "{:<name_w$} {:<19} {:<12} {:<26}",
            "Name", "MAC", "State", "ID"
        );
        let total_w = name_w + 19 + 12 + 26;
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(total_w).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(total_w));
        }

        for c in &cameras {
            let name = c.name.as_deref().unwrap_or("-");
            let mac = c
                .mac
                .as_deref()
                .map(format_mac)
                .unwrap_or_else(|| "-".into());
            let state = c.state.as_deref().unwrap_or("-");
            let id = &c.id;

            if color {
                let state_display = match state {
                    "CONNECTED" => format!("{}", state.green()),
                    "DISCONNECTED" => format!("{}", state.red()),
                    _ => state.to_string(),
                };
                println!(
                    " {:<nw$} {:<19} {:<12} {}",
                    name.bold(),
                    mac.dimmed(),
                    state_display,
                    id.dimmed(),
                    nw = name_w - 1,
                );
            } else {
                println!(
                    " {:<nw$} {:<19} {:<12} {}",
                    name,
                    mac,
                    state,
                    id,
                    nw = name_w - 1,
                );
            }
        }
    }
    out.print_message(&format!("\n{} cameras", cameras.len()));
    Ok(())
}

pub async fn cameras_show(
    client: &UnifiClient,
    id_or_name: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let camera_id = client.resolve_camera_id(id_or_name).await?;
    let c = client.get_protect_camera(&camera_id).await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "id": c.id,
            "name": c.name,
            "mac": c.mac,
            "state": c.state,
            "model_key": c.model_key,
            "mic_enabled": c.is_mic_enabled,
            "video_mode": c.video_mode,
            "feature_flags": c.feature_flags.as_ref().map(|f| serde_json::json!({
                "has_hdr": f.has_hdr,
                "has_mic": f.has_mic,
                "has_speaker": f.has_speaker,
                "smart_detect_types": f.smart_detect_types,
                "video_modes": f.video_modes,
            })),
        }))?);
        return Ok(());
    }

    let color = use_color();
    let label = |l: &str| -> String {
        if color {
            format!("{}", l.dimmed())
        } else {
            l.to_string()
        }
    };

    let name = c.name.as_deref().unwrap_or("Camera");
    if color {
        println!("{}", name.bold());
    } else {
        println!("{name}");
    }

    println!("  {}  {}", label("ID:        "), c.id);
    println!(
        "  {}  {}",
        label("MAC:       "),
        c.mac
            .as_deref()
            .map(format_mac)
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  {}  {}",
        label("State:     "),
        c.state.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Model:     "),
        c.model_key.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Video mode:"),
        c.video_mode.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Mic:       "),
        if c.is_mic_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    if let Some(ref flags) = c.feature_flags {
        let features: Vec<&str> = [
            flags.has_hdr.then_some("HDR"),
            flags.has_mic.then_some("Mic"),
            flags.has_speaker.then_some("Speaker"),
            flags.has_led_status.then_some("LED"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !features.is_empty() {
            println!("  {}  {}", label("Features:  "), features.join(", "));
        }
        if !flags.smart_detect_types.is_empty() {
            println!(
                "  {}  {}",
                label("Detects:   "),
                flags.smart_detect_types.join(", ")
            );
        }
    }

    Ok(())
}

pub async fn rtsps_list(
    client: &UnifiClient,
    id_or_name: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let camera_id = client.resolve_camera_id(id_or_name).await?;
    let streams = client.get_rtsps_streams(&camera_id).await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&streams)?);
        return Ok(());
    }

    let color = use_color();
    out.print_message(&format!("RTSPS streams for camera {id_or_name}:\n"));

    let header = format!("{:<10} {}", "Quality", "URL");
    if color {
        println!("{}", header.bold());
        println!("{}", "-".repeat(70).dimmed());
    } else {
        println!("{header}");
        println!("{}", "-".repeat(70));
    }

    let mut keys: Vec<&String> = streams.keys().collect();
    keys.sort_by_key(|k| match k.as_str() {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        "package" => 3,
        _ => 4,
    });
    for quality in keys {
        let url = streams
            .get(quality)
            .and_then(|v| v.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("-");
        if color {
            println!(" {:<9} {}", quality.bold(), url);
        } else {
            println!(" {:<9} {}", quality, url);
        }
    }

    Ok(())
}

pub async fn rtsps_create(
    client: &UnifiClient,
    id_or_name: &str,
    qualities: &[String],
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let camera_id = client.resolve_camera_id(id_or_name).await?;
    let streams = client.create_rtsps_streams(&camera_id, qualities).await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "status": "ok",
            "action": "create_rtsps",
            "camera_id": camera_id,
            "streams": streams,
        }))?);
        return Ok(());
    }

    out.print_message(&format!("Created RTSPS streams for camera {id_or_name}:\n"));
    for (quality, url) in &streams {
        if let Some(u) = url {
            println!("  {quality}: {u}");
        }
    }

    Ok(())
}

pub async fn rtsps_delete(
    client: &UnifiClient,
    id_or_name: &str,
    qualities: &[String],
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let camera_id = client.resolve_camera_id(id_or_name).await?;
    client.delete_rtsps_streams(&camera_id, qualities).await?;

    out.print_result(
        &serde_json::json!({
            "status": "ok",
            "action": "delete_rtsps",
            "camera_id": camera_id,
            "qualities": qualities,
        }),
        &format!(
            "Deleted RTSPS streams ({}) for camera {id_or_name}",
            qualities.join(", ")
        ),
    );
    Ok(())
}

// --- Full (direct API) commands ---

pub async fn cameras_list_full(
    session: &ProtectSession,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let cameras = session.list_cameras_full().await?;

    if out.is_json() {
        out.print_data(
            &serde_json::to_string_pretty(
                &cameras
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "name": c.name,
                            "mac": c.mac,
                            "ip": c.host,
                            "state": c.state,
                            "type": c.camera_type,
                            "firmware": c.firmware_version,
                            "recording": c.is_recording,
                            "resolution": c.current_resolution,
                            "codec": c.video_codec,
                            "uptime": c.uptime,
                            "wifi": c.wifi_connection_state.as_ref().map(|w| serde_json::json!({
                                "ssid": w.ssid,
                                "ap_name": w.ap_name,
                                "signal_quality": w.signal_quality,
                                "signal_strength": w.signal_strength,
                                "connectivity": w.connectivity,
                            })),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("failed to serialize JSON"),
        );
    } else {
        let color = use_color();

        let col = |min: usize, label_len: usize, vals: Vec<usize>| -> usize {
            vals.into_iter().max().unwrap_or(0).max(label_len).max(min) + 2
        };
        let names: Vec<&str> = cameras
            .iter()
            .map(|c| c.name.as_deref().unwrap_or("-"))
            .collect();
        let types: Vec<&str> = cameras
            .iter()
            .map(|c| {
                c.market_name
                    .as_deref()
                    .or(c.camera_type.as_deref())
                    .unwrap_or("-")
            })
            .collect();
        let name_w = col(4, 4, names.iter().map(|n| n.len()).collect());
        let type_w = col(4, 4, types.iter().map(|t| t.len()).collect());

        let header = format!(
            "{:<name_w$} {:<type_w$} {:<16} {:<12} {:<10} {:<12} {}",
            "Name", "Type", "IP", "State", "FW", "Recording", "WiFi"
        );
        let total_w = name_w + type_w + 16 + 12 + 10 + 12 + 15;
        if color {
            println!("{}", header.bold());
            println!("{}", "-".repeat(total_w).dimmed());
        } else {
            println!("{header}");
            println!("{}", "-".repeat(total_w));
        }

        for c in &cameras {
            let name = c.name.as_deref().unwrap_or("-");
            let ctype = c
                .market_name
                .as_deref()
                .or(c.camera_type.as_deref())
                .unwrap_or("-");
            let ip = c.host.as_deref().unwrap_or("-");
            let state = c.state.as_deref().unwrap_or("-");
            let fw = c.firmware_version.as_deref().unwrap_or("-");
            let rec = if c.is_recording { "yes" } else { "no" };
            let wifi = c
                .wifi_connection_state
                .as_ref()
                .and_then(|w| w.signal_quality.map(|q| format!("{}%", q)))
                .unwrap_or_else(|| "-".into());

            if color {
                let state_display = match state {
                    "CONNECTED" => format!("{}", state.green()),
                    "DISCONNECTED" => format!("{}", state.red()),
                    _ => state.to_string(),
                };
                println!(
                    " {:<nw$} {:<tw$} {:<16} {:<12} {:<10} {:<12} {}",
                    name.bold(),
                    ctype,
                    ip,
                    state_display,
                    fw,
                    rec,
                    wifi,
                    nw = name_w - 1,
                    tw = type_w,
                );
            } else {
                println!(
                    " {:<nw$} {:<tw$} {:<16} {:<12} {:<10} {:<12} {}",
                    name,
                    ctype,
                    ip,
                    state,
                    fw,
                    rec,
                    wifi,
                    nw = name_w - 1,
                    tw = type_w,
                );
            }
        }
    }
    out.print_message(&format!("\n{} cameras", cameras.len()));
    Ok(())
}

pub async fn cameras_show_full(
    session: &ProtectSession,
    client: &UnifiClient,
    id_or_name: &str,
    out: OutputConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve name to ID using the integration API (always available)
    let camera_id = client.resolve_camera_id(id_or_name).await?;
    let c = session.get_camera_full(&camera_id).await?;

    if out.is_json() {
        out.print_data(&serde_json::to_string_pretty(&serde_json::json!({
            "id": c.id,
            "name": c.name,
            "mac": c.mac,
            "ip": c.host,
            "state": c.state,
            "type": c.camera_type,
            "market_name": c.market_name,
            "firmware": c.firmware_version,
            "uptime": c.uptime,
            "recording": c.is_recording,
            "motion_detected": c.is_motion_detected,
            "dark": c.is_dark,
            "codec": c.video_codec,
            "resolution": c.current_resolution,
            "hdr": c.hdr_type,
            "mic_enabled": c.is_mic_enabled,
            "hq_bytes_per_day": c.hq_bytes_per_day,
            "lq_bytes_per_day": c.lq_bytes_per_day,
            "channels": c.channels.iter().map(|ch| serde_json::json!({
                "name": ch.name,
                "enabled": ch.enabled,
                "width": ch.width,
                "height": ch.height,
                "fps": ch.fps,
                "bitrate": ch.bitrate,
                "rtsp_enabled": ch.is_rtsp_enabled,
                "rtsp_alias": ch.rtsp_alias,
            })).collect::<Vec<_>>(),
            "wifi": c.wifi_connection_state.as_ref().map(|w| serde_json::json!({
                "ssid": w.ssid,
                "ap_name": w.ap_name,
                "channel": w.channel,
                "frequency": w.frequency,
                "signal_quality": w.signal_quality,
                "signal_strength": w.signal_strength,
                "connectivity": w.connectivity,
            })),
            "recording_settings": c.recording_settings.as_ref().map(|r| serde_json::json!({
                "mode": r.mode,
                "motion_detection": r.enable_motion_detection,
            })),
            "feature_flags": c.feature_flags.as_ref().map(|f| serde_json::json!({
                "has_hdr": f.has_hdr,
                "has_mic": f.has_mic,
                "has_speaker": f.has_speaker,
                "smart_detect_types": f.smart_detect_types,
                "video_modes": f.video_modes,
            })),
        }))?);
        return Ok(());
    }

    let color = use_color();
    let label = |l: &str| -> String {
        if color {
            format!("{}", l.dimmed())
        } else {
            l.to_string()
        }
    };

    let name = c.name.as_deref().unwrap_or("Camera");
    if color {
        println!("{}", name.bold());
    } else {
        println!("{name}");
    }

    println!("  {}  {}", label("ID:        "), c.id);
    println!(
        "  {}  {}",
        label("MAC:       "),
        c.mac
            .as_deref()
            .map(format_mac)
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  {}  {}",
        label("IP:        "),
        c.host.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("State:     "),
        c.state.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Type:      "),
        c.market_name
            .as_deref()
            .or(c.camera_type.as_deref())
            .unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Firmware:  "),
        c.firmware_version.as_deref().unwrap_or("-")
    );
    if let Some(uptime) = c.uptime {
        println!("  {}  {}", label("Uptime:    "), format_uptime(uptime));
    }
    println!(
        "  {}  {}",
        label("Codec:     "),
        c.video_codec.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Resolution:"),
        c.current_resolution.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("HDR:       "),
        c.hdr_type.as_deref().unwrap_or("-")
    );
    println!(
        "  {}  {}",
        label("Recording: "),
        if c.is_recording { "yes" } else { "no" }
    );
    println!(
        "  {}  {}",
        label("Dark:      "),
        if c.is_dark { "yes" } else { "no" }
    );

    if let Some(hq) = c.hq_bytes_per_day {
        let lq = c.lq_bytes_per_day.unwrap_or(0);
        println!(
            "  {}  {} HQ / {} LQ per day",
            label("Storage:   "),
            format_bytes(hq),
            format_bytes(lq)
        );
    }

    // WiFi
    if let Some(ref w) = c.wifi_connection_state {
        println!();
        if color {
            println!("  {}", "WiFi".bold());
        } else {
            println!("  WiFi");
        }
        println!(
            "    {}  {}",
            label("SSID:      "),
            w.ssid.as_deref().unwrap_or("-")
        );
        println!(
            "    {}  {}",
            label("AP:        "),
            w.ap_name.as_deref().unwrap_or("-")
        );
        if let Some(q) = w.signal_quality {
            println!("    {}  {}%", label("Quality:   "), q);
        }
        if let Some(s) = w.signal_strength {
            println!("    {}  {} dBm", label("Signal:    "), s);
        }
        println!(
            "    {}  {}",
            label("Status:    "),
            w.connectivity.as_deref().unwrap_or("-")
        );
    }

    // Channels
    if !c.channels.is_empty() {
        println!();
        if color {
            println!("  {}", "Channels".bold());
        } else {
            println!("  Channels");
        }
        for ch in &c.channels {
            let name = ch.name.as_deref().unwrap_or("-");
            let res = match (ch.width, ch.height) {
                (Some(w), Some(h)) => format!("{w}x{h}"),
                _ => "-".into(),
            };
            let fps = ch
                .fps
                .map(|f| format!("{f}fps"))
                .unwrap_or_else(|| "-".into());
            let bitrate = ch
                .bitrate
                .map(|b| format!("{}kbps", b / 1000))
                .unwrap_or_else(|| "-".into());
            let rtsp = if ch.is_rtsp_enabled {
                "RTSP on"
            } else {
                "RTSP off"
            };
            println!("    {:<8} {} @ {} {} ({})", name, res, fps, bitrate, rtsp);
        }
    }

    Ok(())
}
