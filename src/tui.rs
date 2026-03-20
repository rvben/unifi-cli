use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table};

use crate::api::{
    ApiError, HealthSubsystem, HostSystem, LegacyClient, LegacyDevice, SysInfo, UnifiClient,
    format_bytes, format_uptime,
};

const HEADER_COLOR: Color = Color::Cyan;
const ONLINE_COLOR: Color = Color::Green;
const OFFLINE_COLOR: Color = Color::Red;
const WARN_COLOR: Color = Color::Yellow;
const DIM_COLOR: Color = Color::DarkGray;
const ACCENT_COLOR: Color = Color::Cyan;
const SELECTED_BG: Color = Color::Rgb(40, 40, 60);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    Clients,
    Devices,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SortMode {
    Bandwidth,
    Name,
    Ip,
}

impl SortMode {
    fn label(self) -> &'static str {
        match self {
            SortMode::Bandwidth => "total ↓",
            SortMode::Name => "name ↓",
            SortMode::Ip => "ip ↓",
        }
    }

    fn next(self) -> Self {
        match self {
            SortMode::Bandwidth => SortMode::Name,
            SortMode::Name => SortMode::Ip,
            SortMode::Ip => SortMode::Bandwidth,
        }
    }
}

enum Overlay {
    ClientDetail(usize),
    DeviceDetail(usize),
    ApPicker {
        client_idx: usize,
        ap_cursor: usize,
    },
    Confirm {
        message: String,
        action: PendingAction,
    },
}

enum PendingAction {
    Client(ClientAction),
    Device(DeviceAction),
}

enum ClientAction {
    Kick(String),                             // MAC
    Block(String),                            // MAC
    Unblock(String),                          // MAC
    LockToAp { mac: String, ap_mac: String }, // Lock client to AP
    UnlockFromAp(String),                     // Unlock client from AP
}

enum DeviceAction {
    Restart(String),      // MAC
    Upgrade(String),      // MAC
    Locate(String, bool), // MAC, enable
}

struct AppState {
    sysinfo: Option<SysInfo>,
    host_system: Option<HostSystem>,
    health: Vec<HealthSubsystem>,
    clients: Vec<LegacyClient>,
    devices: Vec<LegacyDevice>,
    device_names: HashMap<String, String>, // normalized MAC -> device name
    focus: Panel,
    sort: SortMode,
    client_scroll: usize,
    device_scroll: usize,
    filter: String,
    filtering: bool,
    overlay: Option<Overlay>,
    loading: bool,
    last_error: Option<String>,
    status_msg: Option<(String, Instant)>,
    locating: HashMap<String, bool>,
}

impl AppState {
    fn new() -> Self {
        Self {
            sysinfo: None,
            host_system: None,
            health: Vec::new(),
            clients: Vec::new(),
            devices: Vec::new(),
            device_names: HashMap::new(),
            focus: Panel::Clients,
            sort: SortMode::Bandwidth,
            client_scroll: 0,
            device_scroll: 0,
            filter: String::new(),
            filtering: false,
            overlay: None,
            loading: true,
            last_error: None,
            status_msg: None,
            locating: HashMap::new(),
        }
    }

    fn rebuild_device_names(&mut self) {
        self.device_names = self
            .devices
            .iter()
            .filter_map(|d| {
                let mac = crate::api::normalize_mac(d.mac.as_deref()?);
                let name = d.name.as_deref()?.to_string();
                Some((mac, name))
            })
            .collect();
    }

    fn resolve_device_name(&self, mac: &str) -> Option<&str> {
        self.device_names
            .get(&crate::api::normalize_mac(mac))
            .map(|s| s.as_str())
    }

    fn sorted_clients(&self) -> Vec<&LegacyClient> {
        let mut clients: Vec<&LegacyClient> = self
            .clients
            .iter()
            .filter(|c| {
                if self.filter.is_empty() {
                    return true;
                }
                let needle = self.filter.to_lowercase();
                let name = c.display_name().to_lowercase();
                let ip = c.ip.as_deref().unwrap_or("").to_lowercase();
                let mac = c.mac.as_deref().unwrap_or("").to_lowercase();
                name.contains(&needle) || ip.contains(&needle) || mac.contains(&needle)
            })
            .collect();

        match self.sort {
            SortMode::Bandwidth => {
                clients.sort_by(|a, b| {
                    let total_a = a.tx_bytes.unwrap_or(0) + a.rx_bytes.unwrap_or(0);
                    let total_b = b.tx_bytes.unwrap_or(0) + b.rx_bytes.unwrap_or(0);
                    total_b.cmp(&total_a)
                });
            }
            SortMode::Name => {
                clients.sort_by_key(|c| c.display_name().to_lowercase());
            }
            SortMode::Ip => {
                clients.sort_by(|a, b| {
                    let ip_a = a.ip.as_deref().unwrap_or("255.255.255.255");
                    let ip_b = b.ip.as_deref().unwrap_or("255.255.255.255");
                    ip_sort_key(ip_a).cmp(&ip_sort_key(ip_b))
                });
            }
        }

        clients
    }

    fn ap_devices(&self) -> Vec<&LegacyDevice> {
        self.devices
            .iter()
            .filter(|d| d.device_type.as_deref().is_some_and(|t| t == "uap"))
            .collect()
    }

    fn scroll_up(&mut self) {
        match self.focus {
            Panel::Clients => {
                self.client_scroll = self.client_scroll.saturating_sub(1);
            }
            Panel::Devices => {
                self.device_scroll = self.device_scroll.saturating_sub(1);
            }
        }
    }

    fn scroll_down(&mut self, max_clients: usize, max_devices: usize) {
        match self.focus {
            Panel::Clients => {
                if self.client_scroll + 1 < max_clients {
                    self.client_scroll += 1;
                }
            }
            Panel::Devices => {
                if self.device_scroll + 1 < max_devices {
                    self.device_scroll += 1;
                }
            }
        }
    }

    fn page_up(&mut self, page_size: usize) {
        match self.focus {
            Panel::Clients => {
                self.client_scroll = self.client_scroll.saturating_sub(page_size);
            }
            Panel::Devices => {
                self.device_scroll = self.device_scroll.saturating_sub(page_size);
            }
        }
    }

    fn page_down(&mut self, max_clients: usize, max_devices: usize, page_size: usize) {
        match self.focus {
            Panel::Clients => {
                let max = max_clients.saturating_sub(1);
                self.client_scroll = (self.client_scroll + page_size).min(max);
            }
            Panel::Devices => {
                let max = max_devices.saturating_sub(1);
                self.device_scroll = (self.device_scroll + page_size).min(max);
            }
        }
    }
}

fn ip_sort_key(ip: &str) -> Vec<u32> {
    ip.split('.')
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

fn format_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_073_741_824.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1024.0)
    } else if bytes_per_sec >= 1.0 {
        format!("{:.0} B/s", bytes_per_sec)
    } else {
        "0 B/s".into()
    }
}

fn signal_bar(dbm: i32) -> &'static str {
    match dbm {
        -50..=0 => "▂▄▆█",
        -60..=-51 => "▂▄▆░",
        -70..=-61 => "▂▄░░",
        -80..=-71 => "▂░░░",
        _ => "░░░░",
    }
}

fn signal_color(dbm: i32) -> Color {
    match dbm {
        -50..=0 => ONLINE_COLOR,
        -60..=-51 => ONLINE_COLOR,
        -70..=-61 => WARN_COLOR,
        _ => OFFLINE_COLOR,
    }
}

fn status_color(status: &str) -> Color {
    match status {
        "ok" => ONLINE_COLOR,
        "unknown" => DIM_COLOR,
        _ => WARN_COLOR,
    }
}

fn device_state_str(state: Option<u32>) -> (&'static str, Color) {
    match state {
        Some(1) => ("ONLINE", ONLINE_COLOR),
        Some(0) => ("OFFLINE", OFFLINE_COLOR),
        Some(2) => ("ADOPTING", WARN_COLOR),
        Some(4) => ("UPGRADING", WARN_COLOR),
        Some(5) => ("PROVISIONING", WARN_COLOR),
        _ => ("UNKNOWN", DIM_COLOR),
    }
}

async fn fetch_data_standalone(
    http: &reqwest::Client,
    base_url: &str,
) -> Result<
    (
        Option<SysInfo>,
        Option<HostSystem>,
        Vec<HealthSubsystem>,
        Vec<LegacyClient>,
        Vec<LegacyDevice>,
    ),
    ApiError,
> {
    let sysinfo: Option<SysInfo> = legacy_get(http, base_url, "/stat/sysinfo")
        .await
        .ok()
        .and_then(|mut v: Vec<SysInfo>| v.pop());

    let host_system: Option<HostSystem> = async {
        let url = format!("{base_url}/api/system");
        let resp = http.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<HostSystem>().await.ok()
    }
    .await;

    let health: Vec<HealthSubsystem> = legacy_get(http, base_url, "/stat/health")
        .await
        .unwrap_or_default();
    let clients: Vec<LegacyClient> = legacy_get(http, base_url, "/stat/sta").await?;
    let devices: Vec<LegacyDevice> = legacy_get(http, base_url, "/stat/device")
        .await
        .unwrap_or_default();

    Ok((sysinfo, host_system, health, clients, devices))
}

async fn legacy_get<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    base_url: &str,
    path: &str,
) -> Result<Vec<T>, ApiError> {
    use crate::api::types::LegacyResponse;
    let url = format!("{base_url}/proxy/network/api/s/default{path}");
    let resp = http.get(&url).send().await?;
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UnifiClient::error_for_status_pub(status, body));
    }
    let legacy: LegacyResponse<T> = resp.json().await?;
    if legacy.meta.rc != "ok" {
        return Err(ApiError::Api {
            status: 200,
            message: legacy.meta.msg.unwrap_or_else(|| "unknown error".into()),
        });
    }
    Ok(legacy.data)
}

async fn legacy_put(
    http: &reqwest::Client,
    base_url: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    let url = format!("{base_url}/proxy/network/api/s/default{path}");
    let resp = http
        .put(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API error ({status}): {body}"));
    }
    Ok(())
}

async fn find_client_id(
    http: &reqwest::Client,
    base_url: &str,
    mac: &str,
) -> Result<String, String> {
    let normalized = crate::api::normalize_mac(mac);
    let clients: Vec<LegacyClient> = legacy_get(http, base_url, "/stat/sta")
        .await
        .map_err(|e| e.to_string())?;
    clients
        .into_iter()
        .find(|c| {
            c.mac
                .as_deref()
                .is_some_and(|m| crate::api::normalize_mac(m) == normalized)
        })
        .map(|c| c.id)
        .ok_or_else(|| format!("Client {mac} not found"))
}

async fn legacy_post_cmd(
    http: &reqwest::Client,
    base_url: &str,
    manager: &str,
    body: serde_json::Value,
) -> Result<(), String> {
    let url = format!("{base_url}/proxy/network/api/s/default/cmd/{manager}");
    let resp = http
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API error: {body}"));
    }
    Ok(())
}

async fn execute_client_action(
    http: &reqwest::Client,
    base_url: &str,
    action: ClientAction,
) -> Result<String, String> {
    match action {
        ClientAction::Kick(mac) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            legacy_post_cmd(
                http,
                base_url,
                "stamgr",
                serde_json::json!({"cmd": "kick-sta", "mac": formatted}),
            )
            .await?;
            Ok(format!("Kicked {formatted}"))
        }
        ClientAction::Block(mac) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            legacy_post_cmd(
                http,
                base_url,
                "stamgr",
                serde_json::json!({"cmd": "block-sta", "mac": formatted}),
            )
            .await?;
            Ok(format!("Blocked {formatted}"))
        }
        ClientAction::Unblock(mac) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            legacy_post_cmd(
                http,
                base_url,
                "stamgr",
                serde_json::json!({"cmd": "unblock-sta", "mac": formatted}),
            )
            .await?;
            Ok(format!("Unblocked {formatted}"))
        }
        ClientAction::LockToAp { mac, ap_mac } => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            let ap_formatted = crate::api::format_mac(&crate::api::normalize_mac(&ap_mac));
            let client_id = find_client_id(http, base_url, &mac).await?;
            let payload = serde_json::json!({
                "mac": formatted,
                "fixed_ap_enabled": true,
                "fixed_ap_mac": ap_formatted,
            });
            legacy_put(http, base_url, &format!("/rest/user/{client_id}"), &payload).await?;
            Ok(format!("Locked to AP {ap_formatted}"))
        }
        ClientAction::UnlockFromAp(mac) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            let client_id = find_client_id(http, base_url, &mac).await?;
            let payload = serde_json::json!({
                "mac": formatted,
                "fixed_ap_enabled": false,
            });
            legacy_put(http, base_url, &format!("/rest/user/{client_id}"), &payload).await?;
            Ok("Unlocked from AP".to_string())
        }
    }
}

async fn execute_device_action(
    http: &reqwest::Client,
    base_url: &str,
    action: DeviceAction,
) -> Result<String, String> {
    match action {
        DeviceAction::Restart(mac) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            legacy_post_cmd(
                http,
                base_url,
                "devmgr",
                serde_json::json!({"cmd": "restart", "mac": formatted}),
            )
            .await?;
            Ok(format!("Restarting {formatted}"))
        }
        DeviceAction::Upgrade(mac) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            legacy_post_cmd(
                http,
                base_url,
                "devmgr",
                serde_json::json!({"cmd": "upgrade", "mac": formatted}),
            )
            .await?;
            Ok(format!("Upgrading {formatted}"))
        }
        DeviceAction::Locate(mac, enable) => {
            let formatted = crate::api::format_mac(&crate::api::normalize_mac(&mac));
            let cmd = if enable { "set-locate" } else { "unset-locate" };
            legacy_post_cmd(
                http,
                base_url,
                "devmgr",
                serde_json::json!({"cmd": cmd, "mac": formatted}),
            )
            .await?;
            let action_str = if enable {
                "Locating"
            } else {
                "Stopped locating"
            };
            Ok(format!("{action_str} {formatted}"))
        }
    }
}

fn draw_header(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let info = state.sysinfo.as_ref();
    let hostname = info
        .and_then(|s| s.hostname.as_deref())
        .unwrap_or("UniFi Controller");
    let version = info.and_then(|s| s.version.as_deref()).unwrap_or("-");
    let uptime_str = info
        .and_then(|s| s.uptime)
        .map(format_uptime)
        .unwrap_or_else(|| "-".into());

    let title = format!(" {} v{} │ Up {} ", hostname, version, uptime_str);

    // Build health spans
    let mut health_spans: Vec<Span> = vec![Span::raw("  ")];
    for h in &state.health {
        let color = status_color(h.status.as_deref().unwrap_or("unknown"));
        let bullet = Span::styled("● ", Style::default().fg(color));
        let sub = h.subsystem.to_uppercase();
        let detail = match h.subsystem.as_str() {
            "wan" => h
                .wan_ip
                .as_deref()
                .map(|ip| format!(" ({ip})"))
                .unwrap_or_default(),
            "wlan" => {
                let ap = h.num_ap.unwrap_or(0);
                let sta = h.num_sta.unwrap_or(0);
                format!(" ({ap} AP, {sta} sta)")
            }
            "lan" => {
                let sw = h.num_switches.unwrap_or(0);
                let sta = h.num_sta.unwrap_or(0);
                format!(" ({sw} sw, {sta} sta)")
            }
            _ => String::new(),
        };
        health_spans.push(bullet);
        health_spans.push(Span::styled(
            format!("{sub}{detail}"),
            Style::default().fg(Color::White),
        ));
        health_spans.push(Span::raw("  "));
    }

    if state
        .host_system
        .as_ref()
        .is_some_and(|h| h.update_available())
    {
        health_spans.push(Span::styled(
            "⬆ Update available",
            Style::default().fg(WARN_COLOR).add_modifier(Modifier::BOLD),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(HEADER_COLOR))
        .title(Span::styled(
            title,
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ));

    let health_line = Line::from(health_spans);
    let paragraph = Paragraph::new(health_line).block(block);
    f.render_widget(paragraph, area);
}

fn draw_clients(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let clients = state.sorted_clients();
    let is_focused = state.focus == Panel::Clients;

    let border_color = if is_focused { ACCENT_COLOR } else { DIM_COLOR };

    let filter_info = if !state.filter.is_empty() {
        format!(" │ filter: {}", state.filter)
    } else {
        String::new()
    };

    let pos_info = if !clients.is_empty() {
        format!(" [{}/{}]", state.client_scroll + 1, clients.len())
    } else {
        String::new()
    };

    let title = format!(
        " Clients ({}){} │ sort: {}{} ",
        clients.len(),
        pos_info,
        state.sort.label(),
        filter_info,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let header_style = Style::default()
        .fg(HEADER_COLOR)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from("Name").style(header_style),
        Cell::from("Connection").style(header_style),
        Cell::from("Signal").style(header_style),
        Cell::from("IP").style(header_style),
        Cell::from("Total").style(header_style),
    ])
    .height(1);

    // Calculate visible area (subtract borders + header)
    let inner_height = area.height.saturating_sub(4) as usize;

    let rows: Vec<Row> = clients
        .iter()
        .enumerate()
        .skip(state.client_scroll)
        .take(inner_height)
        .map(|(i, c)| {
            let total_bytes = c.tx_bytes.unwrap_or(0) + c.rx_bytes.unwrap_or(0);
            let is_idle = total_bytes == 0;

            let type_icon = if c.is_wired { "⌐ " } else { "◦ " };

            // Show full MAC for unnamed clients
            let display = if c.display_name() == "-" {
                c.mac
                    .as_deref()
                    .map(crate::api::format_mac)
                    .unwrap_or_else(|| "-".into())
            } else {
                c.display_name().to_string()
            };
            let name = format!("{type_icon}{display}");

            let name_style = if is_idle {
                Style::default().fg(DIM_COLOR)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };

            let is_selected = is_focused && i == state.client_scroll;
            let row_style = if is_selected {
                Style::default().bg(SELECTED_BG)
            } else {
                Style::default()
            };

            let total_style = if is_idle {
                Style::default().fg(DIM_COLOR)
            } else {
                Style::default().fg(Color::White)
            };

            // Connection info: AP name for wireless, "Wired" for wired
            // Signal bars in separate column for alignment
            let (conn_str, conn_color, sig_str, sig_color) = if c.is_wired {
                ("Wired".to_string(), DIM_COLOR, String::new(), DIM_COLOR)
            } else {
                let ap_name = c
                    .ap_mac
                    .as_deref()
                    .and_then(|m| state.resolve_device_name(m));
                let label = ap_name.unwrap_or(c.ssid.as_deref().unwrap_or("?"));
                let sig = c
                    .signal
                    .map(|s| signal_bar(s).to_string())
                    .unwrap_or_default();
                let color = c.signal.map(signal_color).unwrap_or(DIM_COLOR);
                (label.to_string(), color, sig, color)
            };

            Row::new(vec![
                Cell::from(name).style(name_style),
                Cell::from(conn_str).style(Style::default().fg(conn_color)),
                Cell::from(sig_str).style(Style::default().fg(sig_color)),
                Cell::from(c.ip.as_deref().unwrap_or("-").to_string())
                    .style(Style::default().fg(DIM_COLOR)),
                Cell::from(format_bytes(total_bytes)).style(total_style),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Min(20),
        Constraint::Length(16),
        Constraint::Length(6),
        Constraint::Length(16),
        Constraint::Length(10),
    ];

    if clients.is_empty() {
        let msg = if state.filter.is_empty() {
            "No clients connected"
        } else {
            "No clients match filter"
        };
        let empty = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default().fg(DIM_COLOR),
        )))
        .block(block)
        .alignment(Alignment::Center);
        f.render_widget(empty, area);
    } else {
        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().bg(SELECTED_BG));
        f.render_widget(table, area);
    }
}

fn draw_devices(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let is_focused = state.focus == Panel::Devices;
    let border_color = if is_focused { ACCENT_COLOR } else { DIM_COLOR };

    let dev_pos = if !state.devices.is_empty() {
        format!(" [{}/{}]", state.device_scroll + 1, state.devices.len())
    } else {
        String::new()
    };
    let title = format!(" Devices ({}){} ", state.devices.len(), dev_pos);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let header = Row::new(vec![
        Cell::from("Name").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Model").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("IP").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("State").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Clients").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Uptime").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Firmware").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1);

    let inner_height = area.height.saturating_sub(4) as usize;

    let rows: Vec<Row> = state
        .devices
        .iter()
        .enumerate()
        .skip(state.device_scroll)
        .take(inner_height)
        .map(|(i, d)| {
            let (state_str, state_color) = device_state_str(d.state);

            let is_selected = is_focused && i == state.device_scroll;
            let row_style = if is_selected {
                Style::default().bg(SELECTED_BG)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(d.name.as_deref().unwrap_or("-").to_string()).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(d.model.as_deref().unwrap_or("-").to_string())
                    .style(Style::default().fg(DIM_COLOR)),
                Cell::from(d.ip.as_deref().unwrap_or("-").to_string())
                    .style(Style::default().fg(DIM_COLOR)),
                Cell::from(format!("● {state_str}")).style(Style::default().fg(state_color)),
                Cell::from(
                    d.num_sta
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "-".into()),
                )
                .style(Style::default().fg(Color::White)),
                Cell::from(d.uptime.map(format_uptime).unwrap_or_else(|| "-".into()))
                    .style(Style::default().fg(DIM_COLOR)),
                Cell::from(d.version.as_deref().unwrap_or("-").to_string())
                    .style(Style::default().fg(DIM_COLOR)),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Min(18),
        Constraint::Length(10),
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Length(16),
        Constraint::Length(14),
    ];

    if state.devices.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "No devices found",
            Style::default().fg(DIM_COLOR),
        )))
        .block(block)
        .alignment(Alignment::Center);
        f.render_widget(empty, area);
    } else {
        let table = Table::new(rows, widths).header(header).block(block);
        f.render_widget(table, area);
    }
}

fn draw_footer(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let error_span = if let Some(ref err) = state.last_error {
        Span::styled(format!(" ⚠ {err} "), Style::default().fg(OFFLINE_COLOR))
    } else {
        Span::raw("")
    };

    let key_style = Style::default()
        .fg(ACCENT_COLOR)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(DIM_COLOR);

    let status_span = if let Some((ref msg, _)) = state.status_msg {
        Span::styled(format!(" ✓ {msg} "), Style::default().fg(ONLINE_COLOR))
    } else {
        Span::raw("")
    };

    let line = if let Some(Overlay::Confirm { .. }) = &state.overlay {
        Line::from(vec![
            Span::styled(
                " y",
                Style::default().fg(WARN_COLOR).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm ", dim),
            Span::styled("n/esc", key_style),
            Span::styled(" cancel", dim),
            error_span,
            status_span,
        ])
    } else if let Some(Overlay::ApPicker { .. }) = &state.overlay {
        Line::from(vec![
            Span::styled(" ↑↓", key_style),
            Span::styled(" select ", dim),
            Span::styled("enter", key_style),
            Span::styled(" lock ", dim),
            Span::styled("esc", key_style),
            Span::styled(" back ", dim),
            Span::styled("q", key_style),
            Span::styled(" quit", dim),
            error_span,
            status_span,
        ])
    } else if let Some(Overlay::ClientDetail(idx)) = &state.overlay {
        let clients = state.sorted_clients();
        let block_label = clients
            .get(*idx)
            .map(|c| if c.blocked { " unblock " } else { " block " })
            .unwrap_or(" block ");
        let is_wireless = clients.get(*idx).is_some_and(|c| !c.is_wired);
        let ap_label = clients
            .get(*idx)
            .filter(|c| !c.is_wired)
            .map(|c| {
                if c.fixed_ap_enabled {
                    " unlock AP "
                } else {
                    " lock to AP "
                }
            })
            .unwrap_or("");
        let mut spans = vec![
            Span::styled(" esc", key_style),
            Span::styled(" back ", dim),
            Span::styled("k", key_style),
            Span::styled(" kick ", dim),
            Span::styled("b", key_style),
            Span::styled(block_label, dim),
        ];
        if is_wireless {
            spans.push(Span::styled("a", key_style));
            spans.push(Span::styled(ap_label, dim));
        }
        spans.push(Span::styled("q", key_style));
        spans.push(Span::styled(" quit", dim));
        spans.push(error_span);
        spans.push(status_span);
        Line::from(spans)
    } else if let Some(Overlay::DeviceDetail(idx)) = &state.overlay {
        let locate_label = state
            .devices
            .get(*idx)
            .and_then(|d| d.mac.as_ref())
            .map(|mac| {
                let normalized = crate::api::normalize_mac(mac);
                if state.locating.get(&normalized).copied().unwrap_or(false) {
                    " stop locate "
                } else {
                    " locate "
                }
            })
            .unwrap_or(" locate ");
        Line::from(vec![
            Span::styled(" esc", key_style),
            Span::styled(" back ", dim),
            Span::styled("r", key_style),
            Span::styled(" restart ", dim),
            Span::styled("u", key_style),
            Span::styled(" upgrade ", dim),
            Span::styled("l", key_style),
            Span::styled(locate_label, dim),
            Span::styled("q", key_style),
            Span::styled(" quit", dim),
            error_span,
            status_span,
        ])
    } else if state.filtering {
        Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(
                format!("filter: {}▌", state.filter),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  esc", key_style),
            Span::styled(" clear ", dim),
            Span::styled("enter", key_style),
            Span::styled(" apply", dim),
            error_span,
        ])
    } else {
        Line::from(vec![
            Span::styled(" q", key_style),
            Span::styled(" quit ", dim),
            Span::styled("s", key_style),
            Span::styled(" sort ", dim),
            Span::styled("/", key_style),
            Span::styled(" filter ", dim),
            Span::styled("enter", key_style),
            Span::styled(" details ", dim),
            Span::styled("tab", key_style),
            Span::styled(" switch panel", dim),
            error_span,
            status_span,
        ])
    };

    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}

fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}

fn draw_overlay(f: &mut ratatui::Frame, state: &AppState) {
    let overlay = match &state.overlay {
        Some(o) => o,
        None => return,
    };

    // Handle ApPicker and Confirm separately (different layout)
    if let Overlay::ApPicker {
        client_idx,
        ap_cursor,
    } = overlay
    {
        draw_ap_picker(f, state, *client_idx, *ap_cursor);
        return;
    }
    if let Overlay::Confirm { message, .. } = overlay {
        draw_confirm(f, message);
        return;
    }

    // Count rows to size the overlay
    let row_count = match overlay {
        Overlay::ClientDetail(idx) => {
            let clients = state.sorted_clients();
            let c = match clients.get(*idx) {
                Some(c) => c,
                None => return,
            };
            let mut n = 3; // MAC, IP, Type
            if c.uptime.is_some() {
                n += 1;
            }
            if c.tx_bytes.is_some() {
                n += 1;
            }
            if c.rx_bytes.is_some() {
                n += 1;
            }
            if !c.is_wired {
                if c.signal.is_some() {
                    n += 1;
                }
                if c.ssid.is_some() {
                    n += 1;
                }
                if c.ap_mac.is_some() {
                    n += 1;
                }
                n += 1; // AP Lock row
            }
            n
        }
        Overlay::DeviceDetail(idx) => {
            let d = match state.devices.get(*idx) {
                Some(d) => d,
                None => return,
            };
            let mut n = 4; // Model, MAC, IP, State
            if d.version.is_some() {
                n += 1;
            }
            if d.uptime.is_some() {
                n += 1;
            }
            if d.num_sta.is_some() {
                n += 1;
            }
            n
        }
        Overlay::ApPicker { .. } | Overlay::Confirm { .. } => 0,
    };

    // 2 borders + 1 header row gap + data rows
    let height = (row_count as u16 + 3).min(f.area().height.saturating_sub(4));
    let width = 44_u16.min(f.area().width.saturating_sub(4));
    let area = centered_rect_fixed(width, height, f.area());
    f.render_widget(Clear, area);

    match overlay {
        Overlay::ClientDetail(idx) => {
            let clients = state.sorted_clients();
            let Some(c) = clients.get(*idx) else { return };

            let title = format!(" {} ", c.display_name());
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT_COLOR))
                .style(Style::default().bg(Color::Black))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(ACCENT_COLOR)
                        .add_modifier(Modifier::BOLD),
                ));

            let mut rows = vec![
                detail_row(
                    "MAC",
                    &c.mac
                        .as_deref()
                        .map(crate::api::format_mac)
                        .unwrap_or_else(|| "-".into()),
                ),
                detail_row("IP", c.ip.as_deref().unwrap_or("-")),
                detail_row("Type", if c.is_wired { "Wired" } else { "Wireless" }),
            ];

            if let Some(uptime) = c.uptime {
                rows.push(detail_row("Uptime", &format_uptime(uptime)));
            }
            if let Some(tx) = c.tx_bytes {
                rows.push(detail_row("TX", &format_bytes(tx)));
            }
            if let Some(rx) = c.rx_bytes {
                rows.push(detail_row("RX", &format_bytes(rx)));
            }
            if !c.is_wired {
                if let Some(signal) = c.signal {
                    rows.push(detail_row("Signal", &format!("{signal} dBm")));
                }
                if let Some(ref ssid) = c.ssid {
                    rows.push(detail_row("SSID", ssid));
                }
                if let Some(ref ap) = c.ap_mac {
                    let ap_label = state.resolve_device_name(ap).unwrap_or(ap.as_str());
                    rows.push(detail_row("AP", ap_label));
                }
                // AP Lock status
                let lock_value = if c.fixed_ap_enabled {
                    let ap_name = c.fixed_ap_mac.as_deref().map(|m| {
                        state
                            .resolve_device_name(m)
                            .map(String::from)
                            .unwrap_or_else(|| crate::api::format_mac(m))
                    });
                    format!("🔒 {}", ap_name.unwrap_or_else(|| "Yes".into()))
                } else {
                    "Off  (a to lock)".into()
                };
                rows.push(detail_row("AP Lock", &lock_value));
            }

            let widths = [Constraint::Length(10), Constraint::Min(20)];
            let table = Table::new(rows, widths).block(block);
            f.render_widget(table, area);
        }
        Overlay::DeviceDetail(idx) => {
            let Some(d) = state.devices.get(*idx) else {
                return;
            };

            let name = d.name.as_deref().unwrap_or("Device");
            let title = format!(" {name} ");
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT_COLOR))
                .style(Style::default().bg(Color::Black))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(ACCENT_COLOR)
                        .add_modifier(Modifier::BOLD),
                ));

            let (state_str, _) = device_state_str(d.state);
            let mut rows = vec![
                detail_row("Model", d.model.as_deref().unwrap_or("-")),
                detail_row(
                    "MAC",
                    &d.mac
                        .as_deref()
                        .map(crate::api::format_mac)
                        .unwrap_or_else(|| "-".into()),
                ),
                detail_row("IP", d.ip.as_deref().unwrap_or("-")),
                detail_row("State", state_str),
            ];

            if let Some(ref v) = d.version {
                rows.push(detail_row("Firmware", v));
            }
            if let Some(uptime) = d.uptime {
                rows.push(detail_row("Uptime", &format_uptime(uptime)));
            }
            if let Some(num_sta) = d.num_sta {
                rows.push(detail_row("Clients", &num_sta.to_string()));
            }

            let widths = [Constraint::Length(10), Constraint::Min(20)];
            let table = Table::new(rows, widths).block(block);
            f.render_widget(table, area);
        }
        Overlay::ApPicker { .. } | Overlay::Confirm { .. } => {}
    }
}

fn detail_row(field: &str, value: &str) -> Row<'static> {
    Row::new(vec![
        Cell::from(field.to_string()).style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from(value.to_string()).style(Style::default().fg(Color::White)),
    ])
}

fn draw_confirm(f: &mut ratatui::Frame, message: &str) {
    let width = (message.len() as u16 + 6).min(f.area().width.saturating_sub(4));
    let height = 5_u16;
    let area = centered_rect_fixed(width, height, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(WARN_COLOR))
        .style(Style::default().bg(Color::Black))
        .title(Span::styled(
            " Confirm ",
            Style::default().fg(WARN_COLOR).add_modifier(Modifier::BOLD),
        ));

    let text = vec![
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y",
                Style::default().fg(WARN_COLOR).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm  ", Style::default().fg(DIM_COLOR)),
            Span::styled(
                "n/esc",
                Style::default()
                    .fg(ACCENT_COLOR)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(DIM_COLOR)),
        ]),
    ];
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

fn draw_ap_picker(f: &mut ratatui::Frame, state: &AppState, client_idx: usize, ap_cursor: usize) {
    let clients = state.sorted_clients();
    let client = match clients.get(client_idx) {
        Some(c) => c,
        None => return,
    };

    let aps = state.ap_devices();
    if aps.is_empty() {
        return;
    }

    // Determine which AP the client is currently connected to
    let current_ap_mac = client.ap_mac.as_deref().map(crate::api::normalize_mac);

    let client_name = client.display_name();
    let title = format!(" Lock {client_name} to AP ");
    let row_count = aps.len();
    let height = (row_count as u16 + 3).min(f.area().height.saturating_sub(4));
    let width = 50_u16.min(f.area().width.saturating_sub(4));
    let area = centered_rect_fixed(width, height, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_COLOR))
        .style(Style::default().bg(Color::Black))
        .title(Span::styled(
            title,
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ));

    let rows: Vec<Row> =
        aps.iter()
            .enumerate()
            .map(|(i, ap)| {
                let name = ap.name.as_deref().unwrap_or("-");
                let mac = ap
                    .mac
                    .as_deref()
                    .map(crate::api::format_mac)
                    .unwrap_or_else(|| "-".into());
                let is_selected = i == ap_cursor;
                let is_current = ap.mac.as_deref().is_some_and(|m| {
                    current_ap_mac.as_deref() == Some(&crate::api::normalize_mac(m))
                });
                let style = if is_selected {
                    Style::default().bg(SELECTED_BG).fg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                let prefix = if is_selected { "▸ " } else { "  " };
                let suffix = if is_current { " ◂ connected" } else { "" };
                Row::new(vec![
                    Cell::from(format!("{prefix}{name}{suffix}")).style(style),
                    Cell::from(mac).style(Style::default().fg(DIM_COLOR)),
                ])
            })
            .collect();

    let widths = [Constraint::Min(24), Constraint::Length(18)];
    let table = Table::new(rows, widths).block(block);
    f.render_widget(table, area);
}

fn draw(f: &mut ratatui::Frame, state: &AppState) {
    if state.loading {
        let area = f.area();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT_COLOR));
        let text = Paragraph::new(Line::from(Span::styled(
            "Connecting to controller…",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center)
        .block(block);
        let centered = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area)[1];
        f.render_widget(text, centered);
        return;
    }

    // Devices: 2 borders + 1 header + 1 header gap + data rows, minimum 5
    let device_rows = (state.devices.len() + 4).max(5) as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),           // header
            Constraint::Min(10),             // clients (takes remaining)
            Constraint::Length(device_rows), // devices (sized to content)
            Constraint::Length(1),           // footer
        ])
        .split(f.area());

    draw_header(f, chunks[0], state);
    draw_clients(f, chunks[1], state);
    draw_devices(f, chunks[2], state);
    draw_footer(f, chunks[3], state);
    draw_overlay(f, state);
}

type FetchResult = Result<
    (
        Option<SysInfo>,
        Option<HostSystem>,
        Vec<HealthSubsystem>,
        Vec<LegacyClient>,
        Vec<LegacyDevice>,
    ),
    String,
>;

pub async fn run(api: &UnifiClient, interval_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new();
    let tick_rate = Duration::from_secs(interval_secs);
    let mut last_tick = Instant::now() - tick_rate; // Force immediate first fetch

    let (tx, mut rx) = tokio::sync::mpsc::channel::<FetchResult>(1);
    let (action_tx, mut action_rx) = tokio::sync::mpsc::channel::<Result<String, String>>(4);
    let mut fetch_in_progress = false;

    let result = loop {
        // Kick off background fetch if tick elapsed and no fetch is running
        if !fetch_in_progress && last_tick.elapsed() >= tick_rate {
            let tx = tx.clone();
            let http = api.clone_http();
            let base_url = api.base_url().to_string();
            fetch_in_progress = true;
            state.loading = state.clients.is_empty();
            tokio::spawn(async move {
                let result = fetch_data_standalone(&http, &base_url).await;
                let _ = tx.send(result.map_err(|e| e.to_string())).await;
            });
        }

        // Check for completed fetch (non-blocking)
        if let Ok(result) = rx.try_recv() {
            fetch_in_progress = false;
            state.loading = false;
            last_tick = Instant::now();
            match result {
                Ok((sysinfo, host_system, health, clients, devices)) => {
                    state.sysinfo = sysinfo;
                    state.host_system = host_system;
                    state.health = health;
                    state.clients = clients;
                    state.devices = devices;
                    state.rebuild_device_names();
                    state.last_error = None;
                }
                Err(e) => {
                    state.last_error = Some(e);
                }
            }
        }

        // Check for completed actions (non-blocking)
        if let Ok(result) = action_rx.try_recv() {
            match result {
                Ok(msg) => {
                    state.status_msg = Some((msg, Instant::now()));
                    // Force refresh after action
                    last_tick = Instant::now() - tick_rate;
                }
                Err(msg) => {
                    state.last_error = Some(msg);
                }
            }
        }

        // Clear status message after 3 seconds
        if let Some((_, t)) = &state.status_msg
            && t.elapsed() >= Duration::from_secs(3)
        {
            state.status_msg = None;
        }

        // Draw
        terminal.draw(|f| draw(f, &state))?;

        // Handle events (poll with short timeout for responsiveness)
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if state.filtering {
                match key.code {
                    KeyCode::Esc => {
                        state.filtering = false;
                        state.filter.clear();
                    }
                    KeyCode::Enter => {
                        state.filtering = false;
                    }
                    KeyCode::Backspace => {
                        state.filter.pop();
                    }
                    KeyCode::Char(c) => {
                        state.filter.push(c);
                        state.client_scroll = 0;
                    }
                    _ => {}
                }
                continue;
            }

            // Overlay is open: Esc closes it, q quits, action keys
            if state.overlay.is_some() {
                // ApPicker has its own key handling
                if let Some(Overlay::ApPicker {
                    client_idx,
                    ap_cursor,
                }) = &state.overlay
                {
                    let client_idx = *client_idx;
                    let ap_cursor = *ap_cursor;
                    match key.code {
                        KeyCode::Esc => {
                            state.overlay = Some(Overlay::ClientDetail(client_idx));
                        }
                        KeyCode::Char('q') => break Ok(()),
                        KeyCode::Up | KeyCode::Char('k') => {
                            let new_cursor = ap_cursor.saturating_sub(1);
                            state.overlay = Some(Overlay::ApPicker {
                                client_idx,
                                ap_cursor: new_cursor,
                            });
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let max = state.ap_devices().len().saturating_sub(1);
                            let new_cursor = (ap_cursor + 1).min(max);
                            state.overlay = Some(Overlay::ApPicker {
                                client_idx,
                                ap_cursor: new_cursor,
                            });
                        }
                        KeyCode::Enter => {
                            let clients = state.sorted_clients();
                            let aps = state.ap_devices();
                            if let Some(c) = clients.get(client_idx)
                                && let Some(ref mac) = c.mac
                                && let Some(ap) = aps.get(ap_cursor)
                                && let Some(ref ap_mac) = ap.mac
                            {
                                let action = ClientAction::LockToAp {
                                    mac: mac.clone(),
                                    ap_mac: ap_mac.clone(),
                                };
                                let http = api.clone_http();
                                let base_url = api.base_url().to_string();
                                let action_tx = action_tx.clone();
                                tokio::spawn(async move {
                                    let result =
                                        execute_client_action(&http, &base_url, action).await;
                                    let _ = action_tx.send(result).await;
                                });
                                state.overlay = None;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Confirm dialog handling
                if matches!(&state.overlay, Some(Overlay::Confirm { .. })) {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            // Take ownership by replacing overlay
                            let overlay = state.overlay.take();
                            if let Some(Overlay::Confirm { action, .. }) = overlay {
                                let http = api.clone_http();
                                let base_url = api.base_url().to_string();
                                let action_tx = action_tx.clone();
                                match action {
                                    PendingAction::Client(ca) => {
                                        tokio::spawn(async move {
                                            let result =
                                                execute_client_action(&http, &base_url, ca).await;
                                            let _ = action_tx.send(result).await;
                                        });
                                    }
                                    PendingAction::Device(da) => {
                                        tokio::spawn(async move {
                                            let result =
                                                execute_device_action(&http, &base_url, da).await;
                                            let _ = action_tx.send(result).await;
                                        });
                                    }
                                }
                            }
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            state.overlay = None;
                        }
                        KeyCode::Char('q') => break Ok(()),
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Esc => {
                        state.overlay = None;
                    }
                    KeyCode::Char('q') => break Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break Ok(());
                    }
                    KeyCode::Char('k') | KeyCode::Char('b') | KeyCode::Char('a') => {
                        if let Some(Overlay::ClientDetail(idx)) = &state.overlay {
                            let clients = state.sorted_clients();
                            if let Some(c) = clients.get(*idx)
                                && let Some(ref mac) = c.mac
                            {
                                let name = c.display_name().to_string();
                                match key.code {
                                    KeyCode::Char('a') => {
                                        if !c.is_wired {
                                            if c.fixed_ap_enabled {
                                                let action =
                                                    ClientAction::UnlockFromAp(mac.clone());
                                                state.overlay = Some(Overlay::Confirm {
                                                    message: format!("Unlock {name} from AP?"),
                                                    action: PendingAction::Client(action),
                                                });
                                            } else {
                                                let idx = *idx;
                                                state.overlay = Some(Overlay::ApPicker {
                                                    client_idx: idx,
                                                    ap_cursor: 0,
                                                });
                                            }
                                        }
                                    }
                                    KeyCode::Char('k') => {
                                        let action = ClientAction::Kick(mac.clone());
                                        state.overlay = Some(Overlay::Confirm {
                                            message: format!("Kick {name}?"),
                                            action: PendingAction::Client(action),
                                        });
                                    }
                                    KeyCode::Char('b') => {
                                        let (action, verb) = if c.blocked {
                                            (ClientAction::Unblock(mac.clone()), "Unblock")
                                        } else {
                                            (ClientAction::Block(mac.clone()), "Block")
                                        };
                                        state.overlay = Some(Overlay::Confirm {
                                            message: format!("{verb} {name}?"),
                                            action: PendingAction::Client(action),
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('u') | KeyCode::Char('l') => {
                        if let Some(Overlay::DeviceDetail(idx)) = &state.overlay
                            && let Some(d) = state.devices.get(*idx)
                            && let Some(ref mac) = d.mac
                        {
                            let name = d.name.as_deref().unwrap_or("device").to_string();
                            match key.code {
                                KeyCode::Char('r') => {
                                    let action = DeviceAction::Restart(mac.clone());
                                    state.overlay = Some(Overlay::Confirm {
                                        message: format!("Restart {name}?"),
                                        action: PendingAction::Device(action),
                                    });
                                }
                                KeyCode::Char('u') => {
                                    let action = DeviceAction::Upgrade(mac.clone());
                                    state.overlay = Some(Overlay::Confirm {
                                        message: format!("Upgrade firmware on {name}?"),
                                        action: PendingAction::Device(action),
                                    });
                                }
                                KeyCode::Char('l') => {
                                    // Locate is safe/reversible, no confirmation needed
                                    let normalized = crate::api::normalize_mac(mac);
                                    let currently_locating =
                                        state.locating.get(&normalized).copied().unwrap_or(false);
                                    state.locating.insert(normalized, !currently_locating);
                                    let action =
                                        DeviceAction::Locate(mac.clone(), !currently_locating);
                                    let http = api.clone_http();
                                    let base_url = api.base_url().to_string();
                                    let action_tx = action_tx.clone();
                                    tokio::spawn(async move {
                                        let result =
                                            execute_device_action(&http, &base_url, action).await;
                                        let _ = action_tx.send(result).await;
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Char('q') => break Ok(()),
                KeyCode::Esc => break Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break Ok(()),
                KeyCode::Enter => {
                    let overlay = match state.focus {
                        Panel::Clients => {
                            let clients = state.sorted_clients();
                            if !clients.is_empty() {
                                Some(Overlay::ClientDetail(state.client_scroll))
                            } else {
                                None
                            }
                        }
                        Panel::Devices => {
                            if !state.devices.is_empty() {
                                Some(Overlay::DeviceDetail(state.device_scroll))
                            } else {
                                None
                            }
                        }
                    };
                    state.overlay = overlay;
                }
                KeyCode::Tab => {
                    state.focus = match state.focus {
                        Panel::Clients => Panel::Devices,
                        Panel::Devices => Panel::Clients,
                    };
                }
                KeyCode::Char('s') => {
                    state.sort = state.sort.next();
                }
                KeyCode::Char('/') => {
                    state.filtering = true;
                    state.filter.clear();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.scroll_up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max_c = state.sorted_clients().len();
                    let max_d = state.devices.len();
                    state.scroll_down(max_c, max_d);
                }
                KeyCode::PageUp => {
                    state.page_up(10);
                }
                KeyCode::PageDown => {
                    let max_c = state.sorted_clients().len();
                    let max_d = state.devices.len();
                    state.page_down(max_c, max_d, 10);
                }
                KeyCode::Home => match state.focus {
                    Panel::Clients => state.client_scroll = 0,
                    Panel::Devices => state.device_scroll = 0,
                },
                KeyCode::End => match state.focus {
                    Panel::Clients => {
                        let max = state.sorted_clients().len().saturating_sub(1);
                        state.client_scroll = max;
                    }
                    Panel::Devices => {
                        let max = state.devices.len().saturating_sub(1);
                        state.device_scroll = max;
                    }
                },
                _ => {}
            }
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

// --- Ports Live TUI ---

use crate::api::{DeviceWithPorts, PortEntry};

struct PortsState {
    device: Option<DeviceWithPorts>,
    prev_bytes: HashMap<u32, (u64, u64, Instant)>,
    port_rates: HashMap<u32, (f64, f64)>,
    scroll: usize,
    interval_secs: u64,
    last_error: Option<String>,
}

impl PortsState {
    fn new(interval_secs: u64) -> Self {
        Self {
            device: None,
            prev_bytes: HashMap::new(),
            port_rates: HashMap::new(),
            scroll: 0,
            interval_secs,
            last_error: None,
        }
    }

    fn update_port_rates(&mut self) {
        let now = Instant::now();
        let ports = match &self.device {
            Some(d) => &d.port_table,
            None => return,
        };

        for port in ports {
            let idx = match port.port_idx {
                Some(i) => i,
                None => continue,
            };
            let tx = port.tx_bytes.unwrap_or(0);
            let rx = port.rx_bytes.unwrap_or(0);

            if let Some((prev_tx, prev_rx, prev_time)) = self.prev_bytes.get(&idx) {
                let elapsed = now.duration_since(*prev_time).as_secs_f64();
                if elapsed > 0.1 {
                    let tx_rate = if tx >= *prev_tx {
                        (tx - prev_tx) as f64 / elapsed
                    } else {
                        0.0
                    };
                    let rx_rate = if rx >= *prev_rx {
                        (rx - prev_rx) as f64 / elapsed
                    } else {
                        0.0
                    };
                    self.port_rates.insert(idx, (tx_rate, rx_rate));
                }
            }

            self.prev_bytes.insert(idx, (tx, rx, now));
        }
    }
}

fn port_link_color(port: &PortEntry) -> Color {
    if port.up {
        match port.speed {
            Some(s) if s >= 2500 => Color::Green,
            Some(s) if s >= 1000 => Color::Cyan,
            Some(_) => Color::Yellow,
            None => Color::White,
        }
    } else {
        DIM_COLOR
    }
}

fn draw_ports(f: &mut ratatui::Frame, state: &PortsState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // port table
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    let device_name = state
        .device
        .as_ref()
        .and_then(|d| d.name.as_deref())
        .unwrap_or("Device");
    let port_count = state
        .device
        .as_ref()
        .map(|d| d.port_table.len())
        .unwrap_or(0);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_COLOR))
        .title(Span::styled(
            format!(" {device_name} \u{2502} {port_count} ports "),
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ));

    let header = Row::new(vec![
        Cell::from("Port").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Name").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Link").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Speed").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("PoE").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("TX/s").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("RX/s").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("TX Total").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("RX Total").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1);

    let inner_height = chunks[0].height.saturating_sub(4) as usize;
    let ports = state
        .device
        .as_ref()
        .map(|d| &d.port_table[..])
        .unwrap_or(&[]);

    let rows: Vec<Row> = ports
        .iter()
        .skip(state.scroll)
        .take(inner_height)
        .map(|p| {
            let idx = p.port_idx.unwrap_or(0);
            let link_color = port_link_color(p);
            let (tx_rate, rx_rate) = state.port_rates.get(&idx).copied().unwrap_or((0.0, 0.0));

            let link_str = if p.up { "\u{25cf} up" } else { "\u{25cb} down" };

            let speed_str = if p.up {
                match p.speed {
                    Some(s) => {
                        let duplex = if p.full_duplex { "FD" } else { "HD" };
                        format!("{s} {duplex}")
                    }
                    None => "up".into(),
                }
            } else {
                "-".into()
            };

            let poe_str = if p.poe_enable {
                match p.poe_power {
                    Some(w) if w > 0.0 => format!("{w:.1}W"),
                    _ => "on".into(),
                }
            } else if p.port_poe {
                "off".into()
            } else {
                "-".into()
            };

            let poe_color = if p.poe_enable && p.poe_power.is_some_and(|w| w > 0.0) {
                Color::Yellow
            } else {
                DIM_COLOR
            };

            Row::new(vec![
                Cell::from(idx.to_string()).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(p.name.as_deref().unwrap_or("-").to_string())
                    .style(Style::default().fg(Color::White)),
                Cell::from(link_str).style(Style::default().fg(link_color)),
                Cell::from(speed_str).style(Style::default().fg(link_color)),
                Cell::from(poe_str).style(Style::default().fg(poe_color)),
                Cell::from(format_rate(tx_rate)).style(Style::default().fg(if tx_rate >= 1024.0 {
                    Color::Green
                } else {
                    DIM_COLOR
                })),
                Cell::from(format_rate(rx_rate)).style(Style::default().fg(if rx_rate >= 1024.0 {
                    Color::Green
                } else {
                    DIM_COLOR
                })),
                Cell::from(p.tx_bytes.map(format_bytes).unwrap_or_else(|| "-".into()))
                    .style(Style::default().fg(DIM_COLOR)),
                Cell::from(p.rx_bytes.map(format_bytes).unwrap_or_else(|| "-".into()))
                    .style(Style::default().fg(DIM_COLOR)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Min(14),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    if ports.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "No ports found (not a switch or router)",
            Style::default().fg(DIM_COLOR),
        )))
        .block(block)
        .alignment(Alignment::Center);
        f.render_widget(empty, chunks[0]);
    } else {
        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().bg(SELECTED_BG));
        f.render_widget(table, chunks[0]);
    }

    // Footer
    let error_span = if let Some(ref err) = state.last_error {
        Span::styled(
            format!(" \u{26a0} {err} "),
            Style::default().fg(OFFLINE_COLOR),
        )
    } else {
        Span::raw("")
    };

    let footer = Line::from(vec![
        Span::styled(
            " q",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit  ", Style::default().fg(DIM_COLOR)),
        Span::styled(
            "\u{2191}\u{2193}",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" scroll", Style::default().fg(DIM_COLOR)),
        error_span,
        Span::raw("  "),
        Span::styled(
            format!("\u{21bb} {}s", state.interval_secs),
            Style::default().fg(DIM_COLOR),
        ),
    ]);
    f.render_widget(Paragraph::new(footer), chunks[1]);
}

pub async fn run_ports(
    api: &UnifiClient,
    mac: &str,
    interval_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = PortsState::new(interval_secs);
    let tick_rate = Duration::from_secs(interval_secs);
    let mut last_tick = Instant::now() - tick_rate;

    let result = loop {
        if last_tick.elapsed() >= tick_rate {
            match api.get_device_ports(mac).await {
                Ok(device) => {
                    state.device = Some(device);
                    state.update_port_rates();
                    state.last_error = None;
                }
                Err(e) => {
                    state.last_error = Some(e.to_string());
                }
            }
            last_tick = Instant::now();
        }

        terminal.draw(|f| draw_ports(f, &state))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break Ok(()),
                KeyCode::Up | KeyCode::Char('k') => {
                    state.scroll = state.scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max = state
                        .device
                        .as_ref()
                        .map(|d| d.port_table.len())
                        .unwrap_or(0);
                    if state.scroll + 1 < max {
                        state.scroll += 1;
                    }
                }
                _ => {}
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_rate_zero() {
        assert_eq!(format_rate(0.0), "0 B/s");
    }

    #[test]
    fn format_rate_bytes() {
        assert_eq!(format_rate(512.0), "512 B/s");
    }

    #[test]
    fn format_rate_kilobytes() {
        assert_eq!(format_rate(10240.0), "10.0 KB/s");
    }

    #[test]
    fn format_rate_megabytes() {
        assert_eq!(format_rate(5_242_880.0), "5.0 MB/s");
    }

    #[test]
    fn format_rate_gigabytes() {
        assert_eq!(format_rate(1_073_741_824.0), "1.0 GB/s");
    }

    #[test]
    fn ip_sort_key_ordering() {
        let mut ips = vec!["10.0.0.2", "10.0.0.10", "10.0.0.1", "192.168.1.1"];
        ips.sort_by_key(|ip| ip_sort_key(ip));
        assert_eq!(
            ips,
            vec!["10.0.0.1", "10.0.0.2", "10.0.0.10", "192.168.1.1"]
        );
    }

    #[test]
    fn sort_mode_cycles() {
        assert_eq!(SortMode::Bandwidth.next(), SortMode::Name);
        assert_eq!(SortMode::Name.next(), SortMode::Ip);
        assert_eq!(SortMode::Ip.next(), SortMode::Bandwidth);
    }

    #[test]
    fn app_state_scroll_bounds() {
        let mut state = AppState::new();
        state.scroll_up();
        assert_eq!(state.client_scroll, 0);

        state.scroll_down(3, 2);
        assert_eq!(state.client_scroll, 1);
        state.scroll_down(3, 2);
        assert_eq!(state.client_scroll, 2);
        state.scroll_down(3, 2);
        assert_eq!(state.client_scroll, 2); // capped

        state.scroll_up();
        assert_eq!(state.client_scroll, 1);
    }

    #[test]
    fn device_state_str_values() {
        assert_eq!(device_state_str(Some(1)).0, "ONLINE");
        assert_eq!(device_state_str(Some(0)).0, "OFFLINE");
        assert_eq!(device_state_str(Some(2)).0, "ADOPTING");
        assert_eq!(device_state_str(None).0, "UNKNOWN");
    }
}
