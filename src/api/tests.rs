use super::*;

// --- normalize_mac ---

#[test]
fn normalize_mac_colon_separated() {
    assert_eq!(normalize_mac("AA:BB:CC:DD:EE:FF"), "aabbccddeeff");
}

#[test]
fn normalize_mac_dash_separated() {
    assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF"), "aabbccddeeff");
}

#[test]
fn normalize_mac_already_clean() {
    assert_eq!(normalize_mac("aabbccddeeff"), "aabbccddeeff");
}

#[test]
fn normalize_mac_mixed_case() {
    assert_eq!(normalize_mac("aA:bB:cC:dD:eE:fF"), "aabbccddeeff");
}

#[test]
fn normalize_mac_mixed_separators() {
    assert_eq!(normalize_mac("AA:BB-CC:DD-EE:FF"), "aabbccddeeff");
}

// --- format_mac ---

#[test]
fn format_mac_from_clean() {
    assert_eq!(format_mac("aabbccddeeff"), "aa:bb:cc:dd:ee:ff");
}

#[test]
fn format_mac_from_colon_separated() {
    assert_eq!(format_mac("AA:BB:CC:DD:EE:FF"), "aa:bb:cc:dd:ee:ff");
}

#[test]
fn format_mac_from_dash_separated() {
    assert_eq!(format_mac("AA-BB-CC-DD-EE-FF"), "aa:bb:cc:dd:ee:ff");
}

#[test]
fn format_mac_invalid_length_returns_original() {
    assert_eq!(format_mac("aabb"), "aabb");
    assert_eq!(format_mac(""), "");
    assert_eq!(format_mac("aabbccddeeff00"), "aabbccddeeff00");
}

// --- format_bytes ---

#[test]
fn format_bytes_bytes() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1023), "1023 B");
}

#[test]
fn format_bytes_kilobytes() {
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1536), "1.5 KB");
}

#[test]
fn format_bytes_megabytes() {
    assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(format_bytes(1024 * 1024 * 5 + 1024 * 512), "5.5 MB");
}

#[test]
fn format_bytes_gigabytes() {
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    assert_eq!(
        format_bytes(1024 * 1024 * 1024 * 2 + 1024 * 1024 * 512),
        "2.5 GB"
    );
}

// --- format_uptime ---

#[test]
fn format_uptime_minutes_only() {
    assert_eq!(format_uptime(0), "0m");
    assert_eq!(format_uptime(59), "0m");
    assert_eq!(format_uptime(60), "1m");
    assert_eq!(format_uptime(300), "5m");
}

#[test]
fn format_uptime_hours_and_minutes() {
    assert_eq!(format_uptime(3600), "1h 0m");
    assert_eq!(format_uptime(3660), "1h 1m");
    assert_eq!(format_uptime(7200 + 1800), "2h 30m");
}

#[test]
fn format_uptime_days_hours_minutes() {
    assert_eq!(format_uptime(86400), "1d 0h 0m");
    assert_eq!(format_uptime(86400 + 3600 + 120), "1d 1h 2m");
    assert_eq!(format_uptime(86400 * 20 + 3600 * 2 + 60 * 26), "20d 2h 26m");
}

// --- ApiError Display ---

#[test]
fn api_error_display_http() {
    // Build a reqwest error by trying to use an invalid header value
    let err = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
        .get("http://localhost")
        .header("bad\nheader", "value")
        .build()
        .unwrap_err();
    let api_err = ApiError::Http(err);
    assert!(api_err.to_string().starts_with("HTTP error:"));
}

#[test]
fn api_error_display_api() {
    let err = ApiError::Api {
        status: 500,
        message: "Internal Server Error".into(),
    };
    assert_eq!(err.to_string(), "API error (500): Internal Server Error");
}

#[test]
fn api_error_display_not_found() {
    let err = ApiError::NotFound("Client with MAC aa:bb".into());
    assert_eq!(err.to_string(), "Not found: Client with MAC aa:bb");
}

#[test]
fn api_error_display_auth() {
    let err = ApiError::Auth("Invalid API key".into());
    let display = err.to_string();
    assert!(display.starts_with("Authentication error: Invalid API key"));
    assert!(display.contains("Hint:"));
}

#[test]
fn api_error_display_other() {
    let err = ApiError::Other("something went wrong".into());
    assert_eq!(err.to_string(), "something went wrong");
}

// --- Deserialization: PaginatedResponse ---

#[test]
fn deserialize_paginated_response() {
    let json = r#"{
        "offset": 0,
        "limit": 25,
        "count": 2,
        "totalCount": 2,
        "data": [
            {"macAddress": "aa:bb:cc:dd:ee:ff", "ipAddress": "10.0.0.1", "name": "Test", "type": "WIRED"},
            {"macAddress": "11:22:33:44:55:66", "ipAddress": "10.0.0.2", "hostname": "host2", "type": "WIRELESS"}
        ]
    }"#;
    let resp: PaginatedResponse<Client> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.total_count, 2);
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].name.as_deref(), Some("Test"));
    assert_eq!(
        resp.data[0].mac_address.as_deref(),
        Some("aa:bb:cc:dd:ee:ff")
    );
    assert_eq!(resp.data[1].hostname.as_deref(), Some("host2"));
    assert_eq!(resp.data[1].client_type.as_deref(), Some("WIRELESS"));
}

#[test]
fn deserialize_paginated_response_ignores_extra_fields() {
    let json = r#"{
        "offset": 0,
        "limit": 25,
        "count": 1,
        "totalCount": 1,
        "data": [
            {"macAddress": "aa:bb:cc:dd:ee:ff", "unknownField": true, "type": "WIRED"}
        ]
    }"#;
    let resp: PaginatedResponse<Client> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.data.len(), 1);
}

// --- Deserialization: LegacyResponse ---

#[test]
fn deserialize_legacy_response_ok() {
    let json = r#"{
        "meta": {"rc": "ok"},
        "data": [
            {"_id": "abc123", "mac": "aa:bb:cc:dd:ee:ff", "ip": "10.0.0.1", "is_wired": true}
        ]
    }"#;
    let resp: LegacyResponse<LegacyClient> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.meta.rc, "ok");
    assert!(resp.meta.msg.is_none());
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].id, "abc123");
    assert!(resp.data[0].is_wired);
}

#[test]
fn deserialize_legacy_response_error() {
    let json = r#"{
        "meta": {"rc": "error", "msg": "api.err.LoginRequired"},
        "data": []
    }"#;
    let resp: LegacyResponse<LegacyClient> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.meta.rc, "error");
    assert_eq!(resp.meta.msg.as_deref(), Some("api.err.LoginRequired"));
}

// --- Deserialization: Client ---

#[test]
fn deserialize_client_all_fields() {
    let json = r#"{
        "macAddress": "d0:11:e5:ce:d5:54",
        "ipAddress": "192.168.1.180",
        "name": "Mac Mini",
        "hostname": "mac-mini",
        "type": "WIRED"
    }"#;
    let client: Client = serde_json::from_str(json).unwrap();
    assert_eq!(client.mac_address.as_deref(), Some("d0:11:e5:ce:d5:54"));
    assert_eq!(client.ip_address.as_deref(), Some("192.168.1.180"));
    assert_eq!(client.name.as_deref(), Some("Mac Mini"));
    assert_eq!(client.hostname.as_deref(), Some("mac-mini"));
    assert_eq!(client.client_type.as_deref(), Some("WIRED"));
}

#[test]
fn deserialize_client_minimal() {
    let json = r#"{}"#;
    let client: Client = serde_json::from_str(json).unwrap();
    assert!(client.mac_address.is_none());
    assert!(client.ip_address.is_none());
    assert!(client.name.is_none());
    assert!(client.hostname.is_none());
    assert!(client.client_type.is_none());
}

#[test]
fn client_display_name_prefers_name() {
    let json = r#"{"name": "My Device", "hostname": "host1"}"#;
    let client: Client = serde_json::from_str(json).unwrap();
    assert_eq!(client.display_name(), "My Device");
}

#[test]
fn client_display_name_falls_back_to_hostname() {
    let json = r#"{"hostname": "host1"}"#;
    let client: Client = serde_json::from_str(json).unwrap();
    assert_eq!(client.display_name(), "host1");
}

#[test]
fn client_display_name_falls_back_to_dash() {
    let json = r#"{}"#;
    let client: Client = serde_json::from_str(json).unwrap();
    assert_eq!(client.display_name(), "-");
}

// --- Deserialization: LegacyClient ---

#[test]
fn deserialize_legacy_client_full() {
    let json = r#"{
        "_id": "67890",
        "mac": "aa:bb:cc:dd:ee:ff",
        "ip": "10.0.0.5",
        "hostname": "myhost",
        "name": "My Client",
        "is_wired": false,
        "uptime": 3600,
        "tx_bytes": 1048576,
        "rx_bytes": 2097152,
        "signal": -55,
        "ap_mac": "60:22:32:58:b8:00",
        "essid": "Notwork"
    }"#;
    let client: LegacyClient = serde_json::from_str(json).unwrap();
    assert_eq!(client.id, "67890");
    assert_eq!(client.mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    assert!(!client.is_wired);
    assert_eq!(client.uptime, Some(3600));
    assert_eq!(client.tx_bytes, Some(1048576));
    assert_eq!(client.rx_bytes, Some(2097152));
    assert_eq!(client.signal, Some(-55));
    assert_eq!(client.ap_mac.as_deref(), Some("60:22:32:58:b8:00"));
    assert_eq!(client.ssid.as_deref(), Some("Notwork"));
}

#[test]
fn deserialize_legacy_client_minimal() {
    let json = r#"{"_id": "abc"}"#;
    let client: LegacyClient = serde_json::from_str(json).unwrap();
    assert_eq!(client.id, "abc");
    assert!(client.mac.is_none());
    assert!(!client.is_wired);
    assert!(client.uptime.is_none());
}

#[test]
fn legacy_client_display_name_prefers_name() {
    let json = r#"{"_id": "x", "name": "Named", "hostname": "host"}"#;
    let client: LegacyClient = serde_json::from_str(json).unwrap();
    assert_eq!(client.display_name(), "Named");
}

#[test]
fn legacy_client_display_name_falls_back_to_hostname() {
    let json = r#"{"_id": "x", "hostname": "host"}"#;
    let client: LegacyClient = serde_json::from_str(json).unwrap();
    assert_eq!(client.display_name(), "host");
}

// --- Deserialization: Site ---

#[test]
fn deserialize_site() {
    let json = r#"{"id": "88f7af54-98f8-306a-a1c7-c9349722b1f6", "name": "Default", "internalReference": "default"}"#;
    let site: Site = serde_json::from_str(json).unwrap();
    assert_eq!(site.id, "88f7af54-98f8-306a-a1c7-c9349722b1f6");
}

// --- Deserialization: Device ---

#[test]
fn deserialize_device() {
    let json = r#"{
        "macAddress": "9c:05:d6:bc:06:43",
        "ipAddress": "192.168.1.1",
        "name": "UCG Ultra",
        "model": "UCG Ultra",
        "state": "ONLINE",
        "firmwareVersion": "5.0.12"
    }"#;
    let device: Device = serde_json::from_str(json).unwrap();
    assert_eq!(device.mac_address.as_deref(), Some("9c:05:d6:bc:06:43"));
    assert_eq!(device.name.as_deref(), Some("UCG Ultra"));
    assert_eq!(device.state.as_deref(), Some("ONLINE"));
    assert_eq!(device.firmware_version.as_deref(), Some("5.0.12"));
}

#[test]
fn deserialize_device_minimal() {
    let json = r#"{}"#;
    let device: Device = serde_json::from_str(json).unwrap();
    assert!(device.name.is_none());
    assert!(device.mac_address.is_none());
}

// --- Deserialization: Network ---

#[test]
fn deserialize_network() {
    let json = r#"{
        "name": "Default",
        "enabled": true,
        "vlanId": 1,
        "default": true
    }"#;
    let network: Network = serde_json::from_str(json).unwrap();
    assert_eq!(network.name.as_deref(), Some("Default"));
    assert!(network.enabled);
    assert_eq!(network.vlan_id, Some(1));
    assert!(network.default);
}

#[test]
fn deserialize_network_defaults() {
    let json = r#"{"name": "Test"}"#;
    let network: Network = serde_json::from_str(json).unwrap();
    assert!(!network.enabled);
    assert!(network.vlan_id.is_none());
    assert!(!network.default);
}

// --- Deserialization: HealthSubsystem ---

#[test]
fn deserialize_health_wan() {
    let json = r#"{
        "subsystem": "wan",
        "status": "ok",
        "wan_ip": "81.172.153.156",
        "isp_name": "Caiway NL"
    }"#;
    let health: HealthSubsystem = serde_json::from_str(json).unwrap();
    assert_eq!(health.subsystem, "wan");
    assert_eq!(health.status.as_deref(), Some("ok"));
    assert_eq!(health.wan_ip.as_deref(), Some("81.172.153.156"));
    assert_eq!(health.isp_name.as_deref(), Some("Caiway NL"));
}

#[test]
fn deserialize_health_wlan() {
    let json = r#"{
        "subsystem": "wlan",
        "status": "ok",
        "num_ap": 3,
        "num_sta": 15
    }"#;
    let health: HealthSubsystem = serde_json::from_str(json).unwrap();
    assert_eq!(health.num_ap, Some(3));
    assert_eq!(health.num_sta, Some(15));
}

#[test]
fn deserialize_health_lan() {
    let json = r#"{
        "subsystem": "lan",
        "status": "ok",
        "num_sw": 4,
        "num_sta": 20
    }"#;
    let health: HealthSubsystem = serde_json::from_str(json).unwrap();
    assert_eq!(health.num_switches, Some(4));
}

// --- Deserialization: SysInfo ---

#[test]
fn deserialize_sysinfo() {
    let json = r#"{
        "hostname": "UCG-Ultra",
        "version": "10.1.85",
        "timezone": "Europe/Amsterdam",
        "uptime": 1737960
    }"#;
    let info: SysInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.hostname.as_deref(), Some("UCG-Ultra"));
    assert_eq!(info.version.as_deref(), Some("10.1.85"));
    assert_eq!(info.timezone.as_deref(), Some("Europe/Amsterdam"));
    assert_eq!(info.uptime, Some(1737960));
}

#[test]
fn deserialize_sysinfo_minimal() {
    let json = r#"{}"#;
    let info: SysInfo = serde_json::from_str(json).unwrap();
    assert!(info.hostname.is_none());
    assert!(info.uptime.is_none());
}

// --- Event ---

#[test]
fn deserialize_event() {
    let json = r#"{
        "key": "EVT_AP_Connected",
        "msg": "AP[80:2a:a8:cd:47:ab] was connected",
        "subsystem": "wlan",
        "time": 1710886800,
        "datetime": "2026-03-19T12:00:00Z"
    }"#;
    let event: Event = serde_json::from_str(json).unwrap();
    assert_eq!(event.key.as_deref(), Some("EVT_AP_Connected"));
    assert_eq!(event.subsystem.as_deref(), Some("wlan"));
    assert_eq!(event.time, Some(1710886800));
    assert!(event.msg.as_ref().unwrap().contains("was connected"));
}

#[test]
fn deserialize_event_minimal() {
    let json = r#"{}"#;
    let event: Event = serde_json::from_str(json).unwrap();
    assert!(event.key.is_none());
    assert!(event.msg.is_none());
    assert!(event.time.is_none());
}

// --- PortEntry ---

#[test]
fn deserialize_port_entry() {
    let json = r#"{
        "port_idx": 1,
        "name": "Port 1",
        "media": "GE",
        "up": true,
        "speed": 1000,
        "full_duplex": true,
        "poe_enable": true,
        "poe_power": 4.5,
        "port_poe": true,
        "tx_bytes": 1048576,
        "rx_bytes": 2097152
    }"#;
    let port: PortEntry = serde_json::from_str(json).unwrap();
    assert_eq!(port.port_idx, Some(1));
    assert_eq!(port.name.as_deref(), Some("Port 1"));
    assert!(port.up);
    assert_eq!(port.speed, Some(1000));
    assert!(port.full_duplex);
    assert!(port.poe_enable);
    assert_eq!(port.poe_power, Some(4.5));
    assert_eq!(port.tx_bytes, Some(1048576));
}

#[test]
fn deserialize_port_entry_minimal() {
    let json = r#"{}"#;
    let port: PortEntry = serde_json::from_str(json).unwrap();
    assert!(port.port_idx.is_none());
    assert!(!port.up);
    assert!(!port.poe_enable);
    assert!(!port.port_poe);
}

// --- DeviceWithPorts ---

#[test]
fn deserialize_device_with_ports() {
    let json = r#"{
        "mac": "9c:05:d6:bc:06:43",
        "name": "USW-24-PoE",
        "model": "USW-24-PoE",
        "port_table": [
            {"port_idx": 1, "name": "Port 1", "up": true, "speed": 1000},
            {"port_idx": 2, "name": "Port 2", "up": false}
        ]
    }"#;
    let device: DeviceWithPorts = serde_json::from_str(json).unwrap();
    assert_eq!(device.name.as_deref(), Some("USW-24-PoE"));
    assert_eq!(device.port_table.len(), 2);
    assert!(device.port_table[0].up);
    assert!(!device.port_table[1].up);
}

#[test]
fn deserialize_device_with_empty_port_table() {
    let json = r#"{"mac": "aa:bb:cc:dd:ee:ff"}"#;
    let device: DeviceWithPorts = serde_json::from_str(json).unwrap();
    assert!(device.port_table.is_empty());
}

// --- LegacyDevice ---

#[test]
fn deserialize_legacy_device() {
    let json = r#"{
        "mac": "9c:05:d6:bc:06:43",
        "ip": "192.168.1.1",
        "name": "UCG Ultra",
        "model": "UCG Ultra",
        "type": "ugw",
        "state": 1,
        "version": "5.0.12",
        "uptime": 1737960,
        "num_sta": 42
    }"#;
    let device: LegacyDevice = serde_json::from_str(json).unwrap();
    assert_eq!(device.mac.as_deref(), Some("9c:05:d6:bc:06:43"));
    assert_eq!(device.name.as_deref(), Some("UCG Ultra"));
    assert_eq!(device.state, Some(1));
    assert_eq!(device.state_str(), "ONLINE");
    assert_eq!(device.uptime, Some(1737960));
    assert_eq!(device.num_sta, Some(42));
}

#[test]
fn legacy_device_state_str() {
    let make_device = |state| -> LegacyDevice {
        serde_json::from_str(&format!(r#"{{"state": {state}}}"#)).unwrap()
    };
    assert_eq!(make_device(0).state_str(), "OFFLINE");
    assert_eq!(make_device(1).state_str(), "ONLINE");
    assert_eq!(make_device(2).state_str(), "ADOPTING");
    assert_eq!(make_device(4).state_str(), "UPGRADING");
    assert_eq!(make_device(5).state_str(), "PROVISIONING");
    assert_eq!(make_device(99).state_str(), "UNKNOWN");
}

#[test]
fn legacy_device_no_state() {
    let device: LegacyDevice = serde_json::from_str(r#"{}"#).unwrap();
    assert_eq!(device.state_str(), "UNKNOWN");
    assert!(device.mac.is_none());
}
