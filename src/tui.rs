use std::collections::{HashMap, VecDeque};
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
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table};

use crate::api::{
    ApiError, HealthSubsystem, LegacyClient, LegacyDevice, SysInfo, UnifiClient, format_bytes,
    format_uptime, normalize_mac,
};

const SPARKLINE_HISTORY: usize = 60;
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
            SortMode::Bandwidth => "bandwidth ↓",
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

struct ClientRate {
    tx_rate: f64,
    rx_rate: f64,
    history: VecDeque<u64>,
}

struct AppState {
    sysinfo: Option<SysInfo>,
    health: Vec<HealthSubsystem>,
    clients: Vec<LegacyClient>,
    devices: Vec<LegacyDevice>,
    prev_bytes: HashMap<String, (u64, u64, Instant)>,
    rates: HashMap<String, ClientRate>,
    focus: Panel,
    sort: SortMode,
    client_scroll: usize,
    device_scroll: usize,
    filter: String,
    filtering: bool,
    interval_secs: u64,
    last_error: Option<String>,
}

impl AppState {
    fn new(interval_secs: u64) -> Self {
        Self {
            sysinfo: None,
            health: Vec::new(),
            clients: Vec::new(),
            devices: Vec::new(),
            prev_bytes: HashMap::new(),
            rates: HashMap::new(),
            focus: Panel::Clients,
            sort: SortMode::Bandwidth,
            client_scroll: 0,
            device_scroll: 0,
            filter: String::new(),
            filtering: false,
            interval_secs,
            last_error: None,
        }
    }

    fn update_rates(&mut self) {
        let now = Instant::now();

        for client in &self.clients {
            let mac = match &client.mac {
                Some(m) => normalize_mac(m),
                None => continue,
            };
            let tx = client.tx_bytes.unwrap_or(0);
            let rx = client.rx_bytes.unwrap_or(0);

            if let Some((prev_tx, prev_rx, prev_time)) = self.prev_bytes.get(&mac) {
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

                    let rate = self.rates.entry(mac.clone()).or_insert(ClientRate {
                        tx_rate: 0.0,
                        rx_rate: 0.0,
                        history: VecDeque::with_capacity(SPARKLINE_HISTORY),
                    });
                    rate.tx_rate = tx_rate;
                    rate.rx_rate = rx_rate;

                    let combined = (tx_rate + rx_rate) as u64;
                    rate.history.push_back(combined);
                    if rate.history.len() > SPARKLINE_HISTORY {
                        rate.history.pop_front();
                    }
                }
            }

            self.prev_bytes.insert(mac, (tx, rx, now));
        }
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
                    let rate_a = a
                        .mac
                        .as_deref()
                        .map(|m| {
                            let mac = normalize_mac(m);
                            self.rates.get(&mac).map_or(0.0, |r| r.tx_rate + r.rx_rate)
                        })
                        .unwrap_or(0.0);
                    let rate_b = b
                        .mac
                        .as_deref()
                        .map(|m| {
                            let mac = normalize_mac(m);
                            self.rates.get(&mac).map_or(0.0, |r| r.tx_rate + r.rx_rate)
                        })
                        .unwrap_or(0.0);
                    rate_b
                        .partial_cmp(&rate_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
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

    fn total_rate(&self) -> (f64, f64) {
        self.rates
            .values()
            .fold((0.0, 0.0), |(tx, rx), r| (tx + r.tx_rate, rx + r.rx_rate))
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

async fn fetch_data(
    api: &UnifiClient,
) -> Result<
    (
        Option<SysInfo>,
        Vec<HealthSubsystem>,
        Vec<LegacyClient>,
        Vec<LegacyDevice>,
    ),
    ApiError,
> {
    let sysinfo = api.get_sysinfo().await.ok();
    let health = api.get_health().await.unwrap_or_default();
    let clients = api.list_clients_legacy().await?;
    let devices: Vec<LegacyDevice> = api.get_legacy_devices().await.unwrap_or_default();
    Ok((sysinfo, health, clients, devices))
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

    let title = format!(
        " Clients ({}) │ sort: {}{} ",
        clients.len(),
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

    let header = Row::new(vec![
        Cell::from("Name").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("IP").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Type").style(
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
        Cell::from("Total").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Trend").style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
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
            let mac_key = c.mac.as_deref().map(normalize_mac).unwrap_or_default();
            let rate = state.rates.get(&mac_key);
            let tx_rate = rate.map_or(0.0, |r| r.tx_rate);
            let rx_rate = rate.map_or(0.0, |r| r.rx_rate);
            let total_rate = tx_rate + rx_rate;

            let rate_color = if total_rate >= 1_048_576.0 {
                Color::Green
            } else if total_rate >= 1024.0 {
                Color::Yellow
            } else {
                Color::White
            };

            let type_str = if c.is_wired { "⌐ wired" } else { "◦ wifi" };
            let type_color = if c.is_wired {
                Color::Blue
            } else {
                Color::Magenta
            };

            // Build sparkline string from history
            let trend = rate.map_or(String::new(), |r| sparkline_str(&r.history));

            let is_selected = is_focused && i == state.client_scroll;
            let row_style = if is_selected {
                Style::default().bg(SELECTED_BG)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(c.display_name().to_string()).style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(c.ip.as_deref().unwrap_or("-").to_string())
                    .style(Style::default().fg(DIM_COLOR)),
                Cell::from(type_str).style(Style::default().fg(type_color)),
                Cell::from(format_rate(tx_rate)).style(Style::default().fg(rate_color)),
                Cell::from(format_rate(rx_rate)).style(Style::default().fg(rate_color)),
                Cell::from(format_bytes(
                    c.tx_bytes.unwrap_or(0) + c.rx_bytes.unwrap_or(0),
                ))
                .style(Style::default().fg(DIM_COLOR)),
                Cell::from(trend).style(Style::default().fg(ACCENT_COLOR)),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Min(18),
        Constraint::Length(16),
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Min(20),
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

    let title = format!(" Devices ({}) ", state.devices.len());
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
        Constraint::Length(14),
        Constraint::Length(16),
        Constraint::Length(16),
        Constraint::Length(8),
        Constraint::Length(14),
        Constraint::Length(10),
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
    let (total_tx, total_rx) = state.total_rate();
    let total = total_tx + total_rx;

    let error_span = if let Some(ref err) = state.last_error {
        Span::styled(format!(" ⚠ {err} "), Style::default().fg(OFFLINE_COLOR))
    } else {
        Span::raw("")
    };

    let filter_hint = if state.filtering {
        Span::styled(
            format!(" filter: {}▌ ", state.filter),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        Span::styled(
            " q",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit  ", Style::default().fg(DIM_COLOR)),
        Span::styled(
            "s",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" sort  ", Style::default().fg(DIM_COLOR)),
        Span::styled(
            "tab",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" focus  ", Style::default().fg(DIM_COLOR)),
        Span::styled(
            "/",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" filter  ", Style::default().fg(DIM_COLOR)),
        Span::styled(
            "↑↓",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" scroll", Style::default().fg(DIM_COLOR)),
        filter_hint,
        error_span,
        Span::raw("  "),
        Span::styled(
            format!("↻ {}s", state.interval_secs),
            Style::default().fg(DIM_COLOR),
        ),
        Span::raw("  "),
        Span::styled(
            format!("▲ {} ▼ {} ", format_rate(total_tx), format_rate(total_rx)),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("Σ {}", format_rate(total)),
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}

fn sparkline_str(history: &VecDeque<u64>) -> String {
    if history.is_empty() {
        return String::new();
    }
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = history.iter().copied().max().unwrap_or(1).max(1);
    history
        .iter()
        .map(|&v| {
            let idx = ((v as f64 / max as f64) * 7.0) as usize;
            bars[idx.min(7)]
        })
        .collect()
}

fn draw(f: &mut ratatui::Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),    // clients
            Constraint::Length(12), // devices
            Constraint::Length(1),  // footer
        ])
        .split(f.area());

    draw_header(f, chunks[0], state);
    draw_clients(f, chunks[1], state);
    draw_devices(f, chunks[2], state);
    draw_footer(f, chunks[3], state);
}

pub async fn run(api: &UnifiClient, interval_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(interval_secs);
    let tick_rate = Duration::from_secs(interval_secs);
    let mut last_tick = Instant::now() - tick_rate; // Force immediate first fetch

    let result = loop {
        // Fetch data if tick elapsed
        if last_tick.elapsed() >= tick_rate {
            match fetch_data(api).await {
                Ok((sysinfo, health, clients, devices)) => {
                    state.sysinfo = sysinfo;
                    state.health = health;
                    state.clients = clients;
                    state.devices = devices;
                    state.update_rates();
                    state.last_error = None;
                }
                Err(e) => {
                    state.last_error = Some(e.to_string());
                }
            }
            last_tick = Instant::now();
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

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break Ok(()),
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
    fn sparkline_str_empty() {
        assert_eq!(sparkline_str(&VecDeque::new()), "");
    }

    #[test]
    fn sparkline_str_single() {
        assert_eq!(sparkline_str(&VecDeque::from([100])), "█");
    }

    #[test]
    fn sparkline_str_ascending() {
        let result = sparkline_str(&VecDeque::from([0, 25, 50, 75, 100]));
        assert_eq!(result.chars().count(), 5);
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars[0], '▁');
        assert_eq!(chars[4], '█');
    }

    #[test]
    fn sparkline_str_all_zeros() {
        assert_eq!(sparkline_str(&VecDeque::from([0, 0, 0])), "▁▁▁");
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
        let mut state = AppState::new(2);
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
