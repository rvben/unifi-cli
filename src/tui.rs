use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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

/// Outcome of handling a key press. The pure `handle_key` reports side effects
/// here rather than performing them, so the event loop owns quitting and task
/// spawning while the input logic stays testable without a terminal.
enum InputOutcome {
    Continue,
    Quit,
    Spawn(PendingAction),
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
    client_cursor: usize,
    client_offset: usize,
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
            client_cursor: 0,
            client_offset: 0,
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

    fn cursor_up(&mut self) {
        match self.focus {
            Panel::Clients => {
                self.client_cursor = self.client_cursor.saturating_sub(1);
            }
            Panel::Devices => {
                self.device_scroll = self.device_scroll.saturating_sub(1);
            }
        }
    }

    fn cursor_down(&mut self, max_clients: usize, max_devices: usize) {
        match self.focus {
            Panel::Clients => {
                if self.client_cursor + 1 < max_clients {
                    self.client_cursor += 1;
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
                self.client_cursor = self.client_cursor.saturating_sub(page_size);
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
                self.client_cursor = (self.client_cursor + page_size).min(max);
            }
            Panel::Devices => {
                let max = max_devices.saturating_sub(1);
                self.device_scroll = (self.device_scroll + page_size).min(max);
            }
        }
    }

    /// Adjust client_offset so that client_cursor is visible within visible_height rows
    fn ensure_client_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.client_cursor < self.client_offset {
            self.client_offset = self.client_cursor;
        } else if self.client_cursor >= self.client_offset + visible_height {
            self.client_offset = self.client_cursor - visible_height + 1;
        }
    }

    /// Apply a key press to the state and report what the event loop should do.
    /// This is the pure half of the TUI: it mutates state and describes side
    /// effects (quit, spawn an async action) without performing them, so the
    /// full input behavior can be exercised in tests without a terminal.
    fn handle_key(&mut self, key: KeyEvent) -> InputOutcome {
        if self.filtering {
            match key.code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                }
                KeyCode::Enter => {
                    self.filtering = false;
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.client_cursor = 0;
                }
                _ => {}
            }
            return InputOutcome::Continue;
        }

        if self.overlay.is_some() {
            // ApPicker has its own navigation and selection handling.
            if let Some(Overlay::ApPicker {
                client_idx,
                ap_cursor,
            }) = &self.overlay
            {
                let client_idx = *client_idx;
                let ap_cursor = *ap_cursor;
                match key.code {
                    KeyCode::Esc => {
                        self.overlay = Some(Overlay::ClientDetail(client_idx));
                    }
                    KeyCode::Char('q') => return InputOutcome::Quit,
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.overlay = Some(Overlay::ApPicker {
                            client_idx,
                            ap_cursor: ap_cursor.saturating_sub(1),
                        });
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let max = self.ap_devices().len().saturating_sub(1);
                        self.overlay = Some(Overlay::ApPicker {
                            client_idx,
                            ap_cursor: (ap_cursor + 1).min(max),
                        });
                    }
                    KeyCode::Enter => {
                        let macs = {
                            let clients = self.sorted_clients();
                            let aps = self.ap_devices();
                            clients
                                .get(client_idx)
                                .and_then(|c| c.mac.clone())
                                .zip(aps.get(ap_cursor).and_then(|ap| ap.mac.clone()))
                        };
                        if let Some((mac, ap_mac)) = macs {
                            self.overlay = None;
                            return InputOutcome::Spawn(PendingAction::Client(
                                ClientAction::LockToAp { mac, ap_mac },
                            ));
                        }
                    }
                    _ => {}
                }
                return InputOutcome::Continue;
            }

            // Confirm dialog: y/Y runs the pending action, n/N/Esc cancels.
            if matches!(&self.overlay, Some(Overlay::Confirm { .. })) {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if let Some(Overlay::Confirm { action, .. }) = self.overlay.take() {
                            return InputOutcome::Spawn(action);
                        }
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.overlay = None;
                    }
                    KeyCode::Char('q') => return InputOutcome::Quit,
                    _ => {}
                }
                return InputOutcome::Continue;
            }

            match key.code {
                KeyCode::Esc => {
                    self.overlay = None;
                }
                KeyCode::Char('q') => return InputOutcome::Quit,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return InputOutcome::Quit;
                }
                KeyCode::Char('k') | KeyCode::Char('b') | KeyCode::Char('a') => {
                    if let Some(Overlay::ClientDetail(idx)) = &self.overlay {
                        let idx = *idx;
                        let info = self.sorted_clients().get(idx).and_then(|c| {
                            c.mac.clone().map(|mac| {
                                (
                                    mac,
                                    c.display_name().to_string(),
                                    c.is_wired,
                                    c.fixed_ap_enabled,
                                    c.blocked,
                                )
                            })
                        });
                        if let Some((mac, name, is_wired, fixed_ap_enabled, blocked)) = info {
                            match key.code {
                                KeyCode::Char('a') if !is_wired => {
                                    if fixed_ap_enabled {
                                        self.overlay = Some(Overlay::Confirm {
                                            message: format!("Unlock {name} from AP?"),
                                            action: PendingAction::Client(
                                                ClientAction::UnlockFromAp(mac),
                                            ),
                                        });
                                    } else {
                                        self.overlay = Some(Overlay::ApPicker {
                                            client_idx: idx,
                                            ap_cursor: 0,
                                        });
                                    }
                                }
                                KeyCode::Char('k') => {
                                    self.overlay = Some(Overlay::Confirm {
                                        message: format!("Kick {name}?"),
                                        action: PendingAction::Client(ClientAction::Kick(mac)),
                                    });
                                }
                                KeyCode::Char('b') => {
                                    let (action, verb) = if blocked {
                                        (ClientAction::Unblock(mac), "Unblock")
                                    } else {
                                        (ClientAction::Block(mac), "Block")
                                    };
                                    self.overlay = Some(Overlay::Confirm {
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
                    if let Some(Overlay::DeviceDetail(idx)) = &self.overlay {
                        let idx = *idx;
                        let info = self.devices.get(idx).and_then(|d| {
                            d.mac.clone().map(|mac| {
                                (
                                    mac,
                                    d.name.as_deref().unwrap_or("device").to_string(),
                                    d.upgradable,
                                )
                            })
                        });
                        if let Some((mac, name, upgradable)) = info {
                            match key.code {
                                KeyCode::Char('r') => {
                                    self.overlay = Some(Overlay::Confirm {
                                        message: format!("Restart {name}?"),
                                        action: PendingAction::Device(DeviceAction::Restart(mac)),
                                    });
                                }
                                KeyCode::Char('u') if upgradable => {
                                    self.overlay = Some(Overlay::Confirm {
                                        message: format!("Upgrade firmware on {name}?"),
                                        action: PendingAction::Device(DeviceAction::Upgrade(mac)),
                                    });
                                }
                                KeyCode::Char('l') => {
                                    // Locate is safe/reversible, so no confirmation is needed.
                                    let normalized = crate::api::normalize_mac(&mac);
                                    let currently_locating =
                                        self.locating.get(&normalized).copied().unwrap_or(false);
                                    self.locating.insert(normalized, !currently_locating);
                                    return InputOutcome::Spawn(PendingAction::Device(
                                        DeviceAction::Locate(mac, !currently_locating),
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
            return InputOutcome::Continue;
        }

        match key.code {
            KeyCode::Char('q') => return InputOutcome::Quit,
            KeyCode::Esc => return InputOutcome::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return InputOutcome::Quit;
            }
            KeyCode::Enter => {
                self.overlay = match self.focus {
                    Panel::Clients => {
                        if self.sorted_clients().is_empty() {
                            None
                        } else {
                            Some(Overlay::ClientDetail(self.client_cursor))
                        }
                    }
                    Panel::Devices => {
                        if self.devices.is_empty() {
                            None
                        } else {
                            Some(Overlay::DeviceDetail(self.device_scroll))
                        }
                    }
                };
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Panel::Clients => Panel::Devices,
                    Panel::Devices => Panel::Clients,
                };
            }
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                self.filter.clear();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max_c = self.sorted_clients().len();
                let max_d = self.devices.len();
                self.cursor_down(max_c, max_d);
            }
            KeyCode::PageUp => {
                self.page_up(10);
            }
            KeyCode::PageDown => {
                let max_c = self.sorted_clients().len();
                let max_d = self.devices.len();
                self.page_down(max_c, max_d, 10);
            }
            KeyCode::Home => match self.focus {
                Panel::Clients => self.client_cursor = 0,
                Panel::Devices => self.device_scroll = 0,
            },
            KeyCode::End => match self.focus {
                Panel::Clients => {
                    self.client_cursor = self.sorted_clients().len().saturating_sub(1);
                }
                Panel::Devices => {
                    self.device_scroll = self.devices.len().saturating_sub(1);
                }
            },
            _ => {}
        }
        InputOutcome::Continue
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
        return Err(crate::api::error_for_status(status, body));
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
        format!(" [{}/{}]", state.client_cursor + 1, clients.len())
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
        .skip(state.client_offset)
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

            let is_selected = is_focused && i == state.client_cursor;
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

    let rows: Vec<Row> = state
        .devices
        .iter()
        .enumerate()
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

    let line = if state.overlay.is_some() {
        // Overlay hints are shown on the overlay itself
        Line::from(vec![error_span, status_span])
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
            if d.upgradable {
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

    let hint_key = Style::default()
        .fg(ACCENT_COLOR)
        .add_modifier(Modifier::BOLD);
    let hint_dim = Style::default().fg(DIM_COLOR);

    match overlay {
        Overlay::ClientDetail(idx) => {
            let clients = state.sorted_clients();
            let Some(c) = clients.get(*idx) else { return };

            let title = format!(" {} ", c.display_name());

            let block_label = if c.blocked { "unblock" } else { "block" };
            let mut hints = vec![
                Span::styled(" esc", hint_key),
                Span::styled(" back ", hint_dim),
                Span::styled("k", hint_key),
                Span::styled(" kick ", hint_dim),
                Span::styled("b", hint_key),
                Span::styled(format!(" {block_label} "), hint_dim),
            ];
            if !c.is_wired {
                let ap_label = if c.fixed_ap_enabled {
                    "unlock AP"
                } else {
                    "lock to AP"
                };
                hints.push(Span::styled("a", hint_key));
                hints.push(Span::styled(format!(" {ap_label} "), hint_dim));
            }

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
                ))
                .title_bottom(Line::from(hints));

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

            let locate_label = d
                .mac
                .as_ref()
                .map(|mac| {
                    let normalized = crate::api::normalize_mac(mac);
                    if state.locating.get(&normalized).copied().unwrap_or(false) {
                        "stop locate"
                    } else {
                        "locate"
                    }
                })
                .unwrap_or("locate");

            let mut hints = vec![
                Span::styled(" esc", hint_key),
                Span::styled(" back ", hint_dim),
                Span::styled("r", hint_key),
                Span::styled(" restart ", hint_dim),
            ];
            if d.upgradable {
                hints.push(Span::styled("u", hint_key));
                hints.push(Span::styled(" upgrade ", hint_dim));
            }
            hints.push(Span::styled("l", hint_key));
            hints.push(Span::styled(format!(" {locate_label} "), hint_dim));

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
                ))
                .title_bottom(Line::from(hints));

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
                if d.upgradable {
                    if let Some(ref new_v) = d.upgrade_to_firmware {
                        rows.push(detail_row("Firmware", &format!("{v} → {new_v}")));
                    } else {
                        rows.push(detail_row("Firmware", &format!("{v} (update available)")));
                    }
                } else {
                    rows.push(detail_row("Firmware", v));
                }
            }
            if d.upgradable && d.version.is_none() {
                rows.push(detail_row("Firmware", "Update available"));
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
    let height = 3_u16;
    let area = centered_rect_fixed(width, height, f.area());
    f.render_widget(Clear, area);

    let hint_key = Style::default().fg(WARN_COLOR).add_modifier(Modifier::BOLD);
    let hint_dim = Style::default().fg(DIM_COLOR);
    let hints = vec![
        Span::styled(" y", hint_key),
        Span::styled(" confirm ", hint_dim),
        Span::styled(
            "n/esc",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel ", hint_dim),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(WARN_COLOR))
        .style(Style::default().bg(Color::Black))
        .title(Span::styled(
            " Confirm ",
            Style::default().fg(WARN_COLOR).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(hints));

    let text = Line::from(Span::styled(
        message.to_string(),
        Style::default().fg(Color::White),
    ));
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

    let hint_key = Style::default()
        .fg(ACCENT_COLOR)
        .add_modifier(Modifier::BOLD);
    let hint_dim = Style::default().fg(DIM_COLOR);
    let hints = vec![
        Span::styled(" ↑↓", hint_key),
        Span::styled(" select ", hint_dim),
        Span::styled("enter", hint_key),
        Span::styled(" lock ", hint_dim),
        Span::styled("esc", hint_key),
        Span::styled(" back ", hint_dim),
    ];

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
        ))
        .title_bottom(Line::from(hints));

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

        // Adjust viewport so cursor stays visible
        if !state.loading {
            let term_height = terminal.size()?.height;
            let device_rows = (state.devices.len() + 4).max(5) as u16;
            // client area = total - header(3) - devices - footer(1), minus borders+header(4)
            let client_visible = term_height
                .saturating_sub(3 + device_rows + 1)
                .saturating_sub(4) as usize;
            state.ensure_client_visible(client_visible);
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

            match state.handle_key(key) {
                InputOutcome::Continue => {}
                InputOutcome::Quit => break Ok(()),
                InputOutcome::Spawn(action) => {
                    let http = api.clone_http();
                    let base_url = api.base_url().to_string();
                    let action_tx = action_tx.clone();
                    match action {
                        PendingAction::Client(ca) => {
                            tokio::spawn(async move {
                                let result = execute_client_action(&http, &base_url, ca).await;
                                let _ = action_tx.send(result).await;
                            });
                        }
                        PendingAction::Device(da) => {
                            tokio::spawn(async move {
                                let result = execute_device_action(&http, &base_url, da).await;
                                let _ = action_tx.send(result).await;
                            });
                        }
                    }
                }
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
        state.cursor_up();
        assert_eq!(state.client_cursor, 0);

        state.cursor_down(3, 2);
        assert_eq!(state.client_cursor, 1);
        state.cursor_down(3, 2);
        assert_eq!(state.client_cursor, 2);
        state.cursor_down(3, 2);
        assert_eq!(state.client_cursor, 2); // capped

        state.cursor_up();
        assert_eq!(state.client_cursor, 1);
    }

    #[test]
    fn device_state_str_values() {
        assert_eq!(device_state_str(Some(1)).0, "ONLINE");
        assert_eq!(device_state_str(Some(0)).0, "OFFLINE");
        assert_eq!(device_state_str(Some(2)).0, "ADOPTING");
        assert_eq!(device_state_str(None).0, "UNKNOWN");
    }

    // --- Panel focus ---

    #[test]
    fn default_focus_is_clients() {
        let state = AppState::new();
        assert_eq!(state.focus, Panel::Clients);
    }

    #[test]
    fn tab_toggles_focus() {
        let mut state = AppState::new();
        assert_eq!(state.focus, Panel::Clients);
        state.focus = Panel::Devices;
        assert_eq!(state.focus, Panel::Devices);
        state.focus = Panel::Clients;
        assert_eq!(state.focus, Panel::Clients);
    }

    // --- Device cursor ---

    #[test]
    fn device_scroll_bounds() {
        let mut state = AppState::new();
        state.focus = Panel::Devices;
        state.cursor_up();
        assert_eq!(state.device_scroll, 0);

        state.cursor_down(0, 3);
        assert_eq!(state.device_scroll, 1);
        state.cursor_down(0, 3);
        assert_eq!(state.device_scroll, 2);
        state.cursor_down(0, 3);
        assert_eq!(state.device_scroll, 2); // capped

        state.cursor_up();
        assert_eq!(state.device_scroll, 1);
    }

    // --- Page up/down ---

    #[test]
    fn page_down_clients() {
        let mut state = AppState::new();
        state.page_down(100, 10, 20);
        assert_eq!(state.client_cursor, 20);
        state.page_down(100, 10, 20);
        assert_eq!(state.client_cursor, 40);
    }

    #[test]
    fn page_up_clients() {
        let mut state = AppState::new();
        state.client_cursor = 30;
        state.page_up(20);
        assert_eq!(state.client_cursor, 10);
        state.page_up(20);
        assert_eq!(state.client_cursor, 0);
    }

    #[test]
    fn page_down_capped_at_max() {
        let mut state = AppState::new();
        state.page_down(5, 10, 20);
        assert_eq!(state.client_cursor, 4); // 5 items, max index is 4
    }

    #[test]
    fn page_down_devices() {
        let mut state = AppState::new();
        state.focus = Panel::Devices;
        state.page_down(100, 10, 5);
        assert_eq!(state.device_scroll, 5);
    }

    #[test]
    fn page_up_devices() {
        let mut state = AppState::new();
        state.focus = Panel::Devices;
        state.device_scroll = 8;
        state.page_up(5);
        assert_eq!(state.device_scroll, 3);
    }

    // --- Viewport scrolling ---

    #[test]
    fn ensure_client_visible_scrolls_down() {
        let mut state = AppState::new();
        state.client_cursor = 25;
        state.client_offset = 0;
        state.ensure_client_visible(10);
        assert_eq!(state.client_offset, 16); // cursor 25 - height 10 + 1
    }

    #[test]
    fn ensure_client_visible_scrolls_up() {
        let mut state = AppState::new();
        state.client_cursor = 3;
        state.client_offset = 10;
        state.ensure_client_visible(10);
        assert_eq!(state.client_offset, 3);
    }

    #[test]
    fn ensure_client_visible_no_scroll_needed() {
        let mut state = AppState::new();
        state.client_cursor = 5;
        state.client_offset = 0;
        state.ensure_client_visible(10);
        assert_eq!(state.client_offset, 0);
    }

    #[test]
    fn ensure_client_visible_zero_height() {
        let mut state = AppState::new();
        state.client_cursor = 5;
        state.client_offset = 0;
        state.ensure_client_visible(0);
        assert_eq!(state.client_offset, 0); // no change
    }

    // --- Filter ---

    #[test]
    fn filter_starts_empty() {
        let state = AppState::new();
        assert!(state.filter.is_empty());
        assert!(!state.filtering);
    }

    #[test]
    fn filter_mode_toggle() {
        let mut state = AppState::new();
        state.filtering = true;
        state.filter = "test".to_string();
        assert!(state.filtering);
        assert_eq!(state.filter, "test");
        state.filtering = false;
        assert!(!state.filtering);
    }

    // --- Overlay ---

    #[test]
    fn overlay_starts_none() {
        let state = AppState::new();
        assert!(state.overlay.is_none());
    }

    #[test]
    fn overlay_client_detail() {
        let mut state = AppState::new();
        state.overlay = Some(Overlay::ClientDetail(5));
        assert!(matches!(state.overlay, Some(Overlay::ClientDetail(5))));
    }

    #[test]
    fn overlay_device_detail() {
        let mut state = AppState::new();
        state.overlay = Some(Overlay::DeviceDetail(2));
        assert!(matches!(state.overlay, Some(Overlay::DeviceDetail(2))));
    }

    #[test]
    fn overlay_confirm() {
        let mut state = AppState::new();
        state.overlay = Some(Overlay::Confirm {
            message: "Kick client?".to_string(),
            action: PendingAction::Client(ClientAction::Kick("aa:bb:cc:dd:ee:ff".to_string())),
        });
        assert!(matches!(state.overlay, Some(Overlay::Confirm { .. })));
    }

    #[test]
    fn overlay_ap_picker() {
        let mut state = AppState::new();
        state.overlay = Some(Overlay::ApPicker {
            client_idx: 3,
            ap_cursor: 0,
        });
        assert!(matches!(state.overlay, Some(Overlay::ApPicker { .. })));
    }

    // --- Device name resolution ---

    #[test]
    fn rebuild_device_names_maps_mac_to_name() {
        let mut state = AppState::new();
        state.devices = vec![
            serde_json::from_str(r#"{"mac": "aa:bb:cc:dd:ee:ff", "name": "Switch"}"#).unwrap(),
            serde_json::from_str(r#"{"mac": "11:22:33:44:55:66", "name": "AP-LR"}"#).unwrap(),
        ];
        state.rebuild_device_names();
        assert_eq!(
            state.resolve_device_name("AA:BB:CC:DD:EE:FF"),
            Some("Switch")
        );
        assert_eq!(
            state.resolve_device_name("11:22:33:44:55:66"),
            Some("AP-LR")
        );
        assert_eq!(state.resolve_device_name("00:00:00:00:00:00"), None);
    }

    #[test]
    fn rebuild_device_names_skips_nameless() {
        let mut state = AppState::new();
        state.devices = vec![serde_json::from_str(r#"{"mac": "aa:bb:cc:dd:ee:ff"}"#).unwrap()];
        state.rebuild_device_names();
        assert_eq!(state.resolve_device_name("aa:bb:cc:dd:ee:ff"), None);
    }

    // --- Sorted clients ---

    fn make_client(id: &str, name: &str, ip: &str, tx: u64, rx: u64) -> LegacyClient {
        serde_json::from_str(&format!(
            r#"{{"_id": "{id}", "name": "{name}", "ip": "{ip}", "tx_bytes": {tx}, "rx_bytes": {rx}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn sorted_clients_bandwidth_default() {
        let mut state = AppState::new();
        state.clients = vec![
            make_client("1", "Low", "10.0.0.1", 100, 100),
            make_client("2", "High", "10.0.0.2", 10000, 10000),
            make_client("3", "Mid", "10.0.0.3", 1000, 1000),
        ];
        let sorted = state.sorted_clients();
        assert_eq!(sorted[0].display_name(), "High");
        assert_eq!(sorted[1].display_name(), "Mid");
        assert_eq!(sorted[2].display_name(), "Low");
    }

    #[test]
    fn sorted_clients_by_name() {
        let mut state = AppState::new();
        state.sort = SortMode::Name;
        state.clients = vec![
            make_client("1", "Charlie", "10.0.0.1", 0, 0),
            make_client("2", "Alice", "10.0.0.2", 0, 0),
            make_client("3", "Bob", "10.0.0.3", 0, 0),
        ];
        let sorted = state.sorted_clients();
        assert_eq!(sorted[0].display_name(), "Alice");
        assert_eq!(sorted[1].display_name(), "Bob");
        assert_eq!(sorted[2].display_name(), "Charlie");
    }

    #[test]
    fn sorted_clients_by_ip() {
        let mut state = AppState::new();
        state.sort = SortMode::Ip;
        state.clients = vec![
            make_client("1", "A", "10.0.0.10", 0, 0),
            make_client("2", "B", "10.0.0.2", 0, 0),
            make_client("3", "C", "10.0.0.1", 0, 0),
        ];
        let sorted = state.sorted_clients();
        assert_eq!(sorted[0].display_name(), "C"); // 10.0.0.1
        assert_eq!(sorted[1].display_name(), "B"); // 10.0.0.2
        assert_eq!(sorted[2].display_name(), "A"); // 10.0.0.10
    }

    #[test]
    fn sorted_clients_filter_by_name() {
        let mut state = AppState::new();
        state.filter = "ali".to_string();
        state.clients = vec![
            make_client("1", "Alice", "10.0.0.1", 0, 0),
            make_client("2", "Bob", "10.0.0.2", 0, 0),
        ];
        let sorted = state.sorted_clients();
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].display_name(), "Alice");
    }

    #[test]
    fn sorted_clients_filter_by_ip() {
        let mut state = AppState::new();
        state.filter = "10.0.0.2".to_string();
        state.clients = vec![
            make_client("1", "Alice", "10.0.0.1", 0, 0),
            make_client("2", "Bob", "10.0.0.2", 0, 0),
        ];
        let sorted = state.sorted_clients();
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].display_name(), "Bob");
    }

    #[test]
    fn sorted_clients_filter_by_mac() {
        let mut state = AppState::new();
        state.filter = "aa:bb".to_string();
        state.clients = vec![
            serde_json::from_str(r#"{"_id": "1", "name": "Match", "mac": "aa:bb:cc:dd:ee:ff"}"#)
                .unwrap(),
            serde_json::from_str(r#"{"_id": "2", "name": "NoMatch", "mac": "11:22:33:44:55:66"}"#)
                .unwrap(),
        ];
        let sorted = state.sorted_clients();
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].display_name(), "Match");
    }

    #[test]
    fn sorted_clients_empty_filter_returns_all() {
        let mut state = AppState::new();
        state.filter = String::new();
        state.clients = vec![
            make_client("1", "A", "10.0.0.1", 0, 0),
            make_client("2", "B", "10.0.0.2", 0, 0),
        ];
        assert_eq!(state.sorted_clients().len(), 2);
    }

    // --- AP devices filter ---

    #[test]
    fn ap_devices_filters_by_type() {
        let mut state = AppState::new();
        state.devices = vec![
            serde_json::from_str(r#"{"mac": "aa:bb:cc:dd:ee:ff", "type": "uap", "name": "AP"}"#)
                .unwrap(),
            serde_json::from_str(
                r#"{"mac": "11:22:33:44:55:66", "type": "usw", "name": "Switch"}"#,
            )
            .unwrap(),
        ];
        let aps = state.ap_devices();
        assert_eq!(aps.len(), 1);
        assert_eq!(aps[0].name.as_deref(), Some("AP"));
    }

    // --- Loading state ---

    #[test]
    fn initial_state_is_loading() {
        let state = AppState::new();
        assert!(state.loading);
        assert!(state.last_error.is_none());
        assert!(state.status_msg.is_none());
    }

    // --- SortMode label ---

    #[test]
    fn sort_mode_labels() {
        assert_eq!(SortMode::Bandwidth.label(), "total ↓");
        assert_eq!(SortMode::Name.label(), "name ↓");
        assert_eq!(SortMode::Ip.label(), "ip ↓");
    }

    // --- ip_sort_key edge cases ---

    #[test]
    fn ip_sort_key_empty() {
        assert_eq!(ip_sort_key(""), Vec::<u32>::new());
    }

    #[test]
    fn ip_sort_key_non_ip() {
        assert_eq!(ip_sort_key("not-an-ip"), Vec::<u32>::new());
    }

    #[test]
    fn ip_sort_key_partial() {
        assert_eq!(ip_sort_key("10.0"), vec![10, 0]);
    }

    // --- Input handling and rendering ---

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn client_with_mac(id: &str, name: &str, mac: &str, total: u64) -> LegacyClient {
        serde_json::from_str(&format!(
            r#"{{"_id":"{id}","name":"{name}","mac":"{mac}","tx_bytes":{total},"rx_bytes":0}}"#
        ))
        .unwrap()
    }

    fn make_device(mac: &str, name: &str, dtype: &str) -> LegacyDevice {
        serde_json::from_str(&format!(
            r#"{{"mac":"{mac}","name":"{name}","type":"{dtype}","state":1}}"#
        ))
        .unwrap()
    }

    /// A populated, non-loading dashboard. Clients carry MACs so action keys
    /// (kick/block/lock) produce real pending actions. Bandwidth order is
    /// Laptop, Phone, Tablet.
    fn dashboard() -> AppState {
        let mut state = AppState::new();
        state.loading = false;
        state.clients = vec![
            client_with_mac("1", "Laptop", "aa:bb:cc:00:00:01", 10_000),
            client_with_mac("2", "Phone", "aa:bb:cc:00:00:02", 200),
            client_with_mac("3", "Tablet", "aa:bb:cc:00:00:03", 100),
        ];
        state.devices = vec![
            make_device("dd:ee:ff:00:00:01", "AP-Office", "uap"),
            make_device("dd:ee:ff:00:00:02", "Switch-01", "usw"),
        ];
        state
    }

    /// Render the dashboard into an in-memory buffer and return its text.
    fn render(state: &AppState, width: u16, height: u16) -> String {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, state)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn render_shows_loading_screen() {
        let state = AppState::new();
        let text = render(&state, 80, 24);
        assert!(text.contains("Connecting to controller"), "{text}");
    }

    #[test]
    fn render_dashboard_shows_clients_and_devices() {
        let text = render(&dashboard(), 120, 30);
        assert!(text.contains("Clients (3)"), "{text}");
        assert!(text.contains("Laptop"), "{text}");
        assert!(text.contains("Phone"), "{text}");
        assert!(text.contains("AP-Office"), "{text}");
    }

    #[test]
    fn render_confirm_overlay_shows_message() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::Confirm {
            message: "Kick Laptop?".into(),
            action: PendingAction::Client(ClientAction::Kick("aa:bb:cc:00:00:01".into())),
        });
        let text = render(&state, 120, 30);
        assert!(text.contains("Confirm"), "{text}");
        assert!(text.contains("Kick Laptop?"), "{text}");
    }

    #[test]
    fn render_every_overlay_without_panicking() {
        let mut state = dashboard();
        state
            .devices
            .push(make_device("dd:ee:ff:00:00:03", "AP-Garage", "uap"));
        for overlay in [
            Overlay::ClientDetail(0),
            Overlay::DeviceDetail(0),
            Overlay::ApPicker {
                client_idx: 0,
                ap_cursor: 1,
            },
            Overlay::Confirm {
                message: "Restart AP-Office?".into(),
                action: PendingAction::Device(DeviceAction::Restart("dd:ee:ff:00:00:01".into())),
            },
        ] {
            state.overlay = Some(overlay);
            // The assertion is simply that rendering does not panic.
            let _ = render(&state, 120, 30);
        }
    }

    #[test]
    fn handle_key_quit_keys() {
        let mut state = dashboard();
        assert!(matches!(
            state.handle_key(press(KeyCode::Char('q'))),
            InputOutcome::Quit
        ));
        assert!(matches!(
            state.handle_key(press(KeyCode::Esc)),
            InputOutcome::Quit
        ));
        assert!(matches!(
            state.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            InputOutcome::Quit
        ));
    }

    #[test]
    fn handle_key_navigation_moves_and_clamps_cursor() {
        let mut state = dashboard();
        assert_eq!(state.client_cursor, 0);
        state.handle_key(press(KeyCode::Down));
        assert_eq!(state.client_cursor, 1);
        state.handle_key(press(KeyCode::Char('j')));
        assert_eq!(state.client_cursor, 2);
        state.handle_key(press(KeyCode::Down)); // already at last of three
        assert_eq!(state.client_cursor, 2);
        state.handle_key(press(KeyCode::Up));
        assert_eq!(state.client_cursor, 1);
    }

    #[test]
    fn handle_key_tab_toggles_focus() {
        let mut state = dashboard();
        assert_eq!(state.focus, Panel::Clients);
        state.handle_key(press(KeyCode::Tab));
        assert_eq!(state.focus, Panel::Devices);
        state.handle_key(press(KeyCode::Tab));
        assert_eq!(state.focus, Panel::Clients);
    }

    #[test]
    fn handle_key_s_cycles_sort() {
        let mut state = dashboard();
        assert_eq!(state.sort, SortMode::Bandwidth);
        state.handle_key(press(KeyCode::Char('s')));
        assert_eq!(state.sort, SortMode::Name);
    }

    #[test]
    fn handle_key_filter_typing_appends_and_resets_cursor() {
        let mut state = dashboard();
        state.client_cursor = 2;
        state.handle_key(press(KeyCode::Char('/')));
        assert!(state.filtering);
        state.handle_key(press(KeyCode::Char('a')));
        assert_eq!(state.filter, "a");
        assert_eq!(state.client_cursor, 0);
        state.handle_key(press(KeyCode::Char('b')));
        state.handle_key(press(KeyCode::Backspace));
        assert_eq!(state.filter, "a");
        state.handle_key(press(KeyCode::Enter));
        assert!(!state.filtering);
        assert_eq!(state.filter, "a", "Enter keeps the filter");
    }

    #[test]
    fn handle_key_filter_esc_clears() {
        let mut state = dashboard();
        state.handle_key(press(KeyCode::Char('/')));
        state.handle_key(press(KeyCode::Char('x')));
        assert_eq!(state.filter, "x");
        state.handle_key(press(KeyCode::Esc));
        assert!(!state.filtering);
        assert_eq!(state.filter, "");
    }

    #[test]
    fn handle_key_enter_opens_and_esc_closes_client_detail() {
        let mut state = dashboard();
        state.client_cursor = 1;
        state.handle_key(press(KeyCode::Enter));
        assert!(matches!(state.overlay, Some(Overlay::ClientDetail(1))));
        state.handle_key(press(KeyCode::Esc));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn handle_key_enter_opens_device_detail_when_focused() {
        let mut state = dashboard();
        state.focus = Panel::Devices;
        state.device_scroll = 1;
        state.handle_key(press(KeyCode::Enter));
        assert!(matches!(state.overlay, Some(Overlay::DeviceDetail(1))));
    }

    #[test]
    fn handle_key_enter_noop_when_empty() {
        let mut state = AppState::new();
        state.loading = false;
        state.handle_key(press(KeyCode::Enter));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn client_detail_kick_opens_confirm() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::ClientDetail(0)); // Laptop
        let outcome = state.handle_key(press(KeyCode::Char('k')));
        assert!(matches!(outcome, InputOutcome::Continue));
        match &state.overlay {
            Some(Overlay::Confirm { message, action }) => {
                assert_eq!(message, "Kick Laptop?");
                assert!(matches!(
                    action,
                    PendingAction::Client(ClientAction::Kick(_))
                ));
            }
            _ => panic!("expected a confirm overlay"),
        }
    }

    #[test]
    fn client_detail_block_opens_confirm() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::ClientDetail(1)); // Phone, not blocked
        state.handle_key(press(KeyCode::Char('b')));
        match &state.overlay {
            Some(Overlay::Confirm { message, action }) => {
                assert_eq!(message, "Block Phone?");
                assert!(matches!(
                    action,
                    PendingAction::Client(ClientAction::Block(_))
                ));
            }
            _ => panic!("expected a confirm overlay"),
        }
    }

    #[test]
    fn confirm_yes_spawns_action_and_clears_overlay() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::Confirm {
            message: "Kick Laptop?".into(),
            action: PendingAction::Client(ClientAction::Kick("aa:bb:cc:00:00:01".into())),
        });
        let outcome = state.handle_key(press(KeyCode::Char('y')));
        assert!(matches!(
            outcome,
            InputOutcome::Spawn(PendingAction::Client(ClientAction::Kick(_)))
        ));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn confirm_no_cancels_without_spawning() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::Confirm {
            message: "Kick Laptop?".into(),
            action: PendingAction::Client(ClientAction::Kick("m".into())),
        });
        let outcome = state.handle_key(press(KeyCode::Char('n')));
        assert!(matches!(outcome, InputOutcome::Continue));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn device_detail_restart_opens_confirm() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::DeviceDetail(0)); // AP-Office
        state.handle_key(press(KeyCode::Char('r')));
        match &state.overlay {
            Some(Overlay::Confirm { message, action }) => {
                assert_eq!(message, "Restart AP-Office?");
                assert!(matches!(
                    action,
                    PendingAction::Device(DeviceAction::Restart(_))
                ));
            }
            _ => panic!("expected a confirm overlay"),
        }
    }

    #[test]
    fn device_detail_locate_spawns_and_toggles_locating() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::DeviceDetail(0));
        let outcome = state.handle_key(press(KeyCode::Char('l')));
        assert!(matches!(
            outcome,
            InputOutcome::Spawn(PendingAction::Device(DeviceAction::Locate(_, true)))
        ));
        // Locate leaves the detail overlay open and records the new state.
        assert!(matches!(state.overlay, Some(Overlay::DeviceDetail(0))));
        let norm = crate::api::normalize_mac("dd:ee:ff:00:00:01");
        assert_eq!(state.locating.get(&norm), Some(&true));
    }

    #[test]
    fn ap_picker_navigates_and_selects() {
        let mut state = dashboard();
        state
            .devices
            .push(make_device("dd:ee:ff:00:00:03", "AP-Garage", "uap"));
        state.overlay = Some(Overlay::ApPicker {
            client_idx: 0,
            ap_cursor: 0,
        });
        state.handle_key(press(KeyCode::Down));
        assert!(matches!(
            state.overlay,
            Some(Overlay::ApPicker { ap_cursor: 1, .. })
        ));
        let outcome = state.handle_key(press(KeyCode::Enter));
        assert!(matches!(
            outcome,
            InputOutcome::Spawn(PendingAction::Client(ClientAction::LockToAp { .. }))
        ));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn ap_picker_esc_returns_to_client_detail() {
        let mut state = dashboard();
        state.overlay = Some(Overlay::ApPicker {
            client_idx: 2,
            ap_cursor: 0,
        });
        state.handle_key(press(KeyCode::Esc));
        assert!(matches!(state.overlay, Some(Overlay::ClientDetail(2))));
    }
}
