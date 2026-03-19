use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Helper to create a UnifiClient pointing at the mock server
async fn mock_client(server: &MockServer) -> unifi_cli::api::UnifiClient {
    unifi_cli::api::UnifiClient::new(&server.uri(), "test-api-key").unwrap()
}

// Mount the site discovery endpoint that ensure_site_id() calls
async fn mount_site_discovery(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/proxy/network/integration/v1/sites"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "offset": 0,
            "limit": 25,
            "count": 1,
            "totalCount": 1,
            "data": [{"id": "test-site-uuid"}]
        })))
        .expect(1..)
        .mount(server)
        .await;
}

// --- UnifiClient API tests ---

mod client_api {
    use super::*;

    #[tokio::test]
    async fn list_clients_returns_paginated_results() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;

        Mock::given(method("GET"))
            .and(path_regex(r"/proxy/network/integration/v1/sites/.*/clients"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"macAddress": "aa:bb:cc:dd:ee:ff", "ipAddress": "10.0.0.1", "name": "Device1", "type": "WIRED"},
                    {"macAddress": "11:22:33:44:55:66", "ipAddress": "10.0.0.2", "hostname": "host2", "type": "WIRELESS"}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let clients = client.list_clients().await.unwrap();
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].name.as_deref(), Some("Device1"));
        assert_eq!(clients[1].hostname.as_deref(), Some("host2"));
    }

    #[tokio::test]
    async fn list_clients_handles_pagination() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;

        // First page
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 200, "totalCount": 201,
                "data": (0..200).map(|i| serde_json::json!({
                    "macAddress": format!("aa:bb:cc:dd:{:02x}:{:02x}", i / 256, i % 256),
                    "type": "WIRED"
                })).collect::<Vec<_>>()
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second page
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 200, "limit": 200, "count": 1, "totalCount": 201,
                "data": [{"macAddress": "ff:ff:ff:ff:ff:ff", "type": "WIRED"}]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let clients = client.list_clients().await.unwrap();
        assert_eq!(clients.len(), 201);
    }

    #[tokio::test]
    async fn get_client_detail_finds_by_mac() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "ip": "10.0.0.1", "name": "Target", "is_wired": true, "uptime": 7200},
                    {"_id": "def", "mac": "11:22:33:44:55:66", "ip": "10.0.0.2"}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let detail = client.get_client_detail("AA:BB:CC:DD:EE:FF").await.unwrap();
        assert_eq!(detail.display_name(), "Target");
        assert!(detail.is_wired);
        assert_eq!(detail.uptime, Some(7200));
    }

    #[tokio::test]
    async fn get_client_detail_not_found() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "abc", "mac": "aa:bb:cc:dd:ee:ff"}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client
            .get_client_detail("00:00:00:00:00:00")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"));
    }

    #[tokio::test]
    async fn get_client_detail_accepts_dash_format() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "name": "Found"}]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let detail = client.get_client_detail("AA-BB-CC-DD-EE-FF").await.unwrap();
        assert_eq!(detail.display_name(), "Found");
    }

    #[tokio::test]
    async fn set_fixed_ip_via_put() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"_id": "client123", "mac": "aa:bb:cc:dd:ee:ff"}]
            })))
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/proxy/network/api/s/default/rest/user/client123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .set_fixed_ip("aa:bb:cc:dd:ee:ff", "10.0.0.50", None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn set_fixed_ip_falls_back_to_post_on_404() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"_id": "newclient", "mac": "aa:bb:cc:dd:ee:ff"}]
            })))
            .mount(&server)
            .await;

        // PUT returns 404 (no existing user entry)
        Mock::given(method("PUT"))
            .and(path("/proxy/network/api/s/default/rest/user/newclient"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.ObjectNotFound"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        // POST creates the user entry
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/rest/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .set_fixed_ip("aa:bb:cc:dd:ee:ff", "10.0.0.99", Some("NewDevice"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn set_fixed_ip_client_not_found() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client
            .set_fixed_ip("00:00:00:00:00:00", "10.0.0.1", None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"));
    }

    #[tokio::test]
    async fn block_client_sends_correct_command() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client.block_client("AABBCCDDEEFF").await.unwrap();
    }

    #[tokio::test]
    async fn unblock_client_sends_correct_command() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client.unblock_client("aa:bb:cc:dd:ee:ff").await.unwrap();
    }

    #[tokio::test]
    async fn kick_client_sends_correct_command() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client.kick_client("aa:bb:cc:dd:ee:ff").await.unwrap();
    }

    #[tokio::test]
    async fn list_devices_returns_devices() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;

        Mock::given(method("GET"))
            .and(path_regex(r"/proxy/network/integration/v1/sites/.*/devices"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"macAddress": "9c:05:d6:bc:06:43", "ipAddress": "192.168.1.1", "name": "UCG Ultra", "model": "UCG Ultra", "state": "ONLINE", "firmwareVersion": "5.0.12"},
                    {"macAddress": "60:22:32:58:b8:00", "ipAddress": "192.168.1.190", "name": "U6-Lite", "model": "U6 Lite", "state": "ONLINE", "firmwareVersion": "6.7.41"}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let devices = client.list_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name.as_deref(), Some("UCG Ultra"));
        assert_eq!(devices[1].firmware_version.as_deref(), Some("6.7.41"));
    }

    #[tokio::test]
    async fn restart_device_sends_correct_command() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client.restart_device("aa:bb:cc:dd:ee:ff").await.unwrap();
    }

    #[tokio::test]
    async fn locate_device_enable() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .locate_device("aa:bb:cc:dd:ee:ff", true)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn locate_device_disable() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .locate_device("aa:bb:cc:dd:ee:ff", false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_networks_returns_networks() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;

        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/networks",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 3, "totalCount": 3,
                "data": [
                    {"name": "Default", "enabled": true, "vlanId": 1, "default": true},
                    {"name": "IoT", "enabled": true, "vlanId": 20, "default": false},
                    {"name": "Guest", "enabled": false, "vlanId": 10, "default": false}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let networks = client.list_networks().await.unwrap();
        assert_eq!(networks.len(), 3);
        assert_eq!(networks[0].name.as_deref(), Some("Default"));
        assert!(networks[0].default);
        assert_eq!(networks[1].vlan_id, Some(20));
        assert!(!networks[2].enabled);
    }

    #[tokio::test]
    async fn get_health_returns_subsystems() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"subsystem": "wan", "status": "ok", "wan_ip": "81.172.153.156", "isp_name": "Caiway"},
                    {"subsystem": "wlan", "status": "ok", "num_ap": 3, "num_sta": 15},
                    {"subsystem": "lan", "status": "ok", "num_sw": 4, "num_sta": 20}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let health = client.get_health().await.unwrap();
        assert_eq!(health.len(), 3);
        assert_eq!(health[0].wan_ip.as_deref(), Some("81.172.153.156"));
        assert_eq!(health[1].num_ap, Some(3));
        assert_eq!(health[2].num_switches, Some(4));
    }

    #[tokio::test]
    async fn get_sysinfo_returns_info() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sysinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "hostname": "UCG-Ultra",
                    "version": "10.1.85",
                    "timezone": "Europe/Amsterdam",
                    "uptime": 1737960
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let info = client.get_sysinfo().await.unwrap();
        assert_eq!(info.hostname.as_deref(), Some("UCG-Ultra"));
        assert_eq!(info.version.as_deref(), Some("10.1.85"));
        assert_eq!(info.uptime, Some(1737960));
    }

    #[tokio::test]
    async fn get_sysinfo_empty_data() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sysinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client.get_sysinfo().await.unwrap_err();
        assert!(err.to_string().contains("No sysinfo returned"));
    }
}

// --- Error handling tests ---

mod error_handling {
    use super::*;

    #[tokio::test]
    async fn api_returns_401_unauthorized() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;

        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let err = client.list_clients().await.unwrap_err();
        assert!(err.to_string().contains("Authentication error:"));
    }

    #[tokio::test]
    async fn api_returns_500_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client.get_health().await.unwrap_err();
        assert!(err.to_string().contains("API error (500)"));
    }

    #[tokio::test]
    async fn legacy_api_returns_error_rc() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.LoginRequired"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client.get_health().await.unwrap_err();
        assert!(err.to_string().contains("api.err.LoginRequired"));
    }

    #[tokio::test]
    async fn legacy_api_error_without_message() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "error"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client.get_health().await.unwrap_err();
        assert!(err.to_string().contains("unknown error"));
    }

    #[tokio::test]
    async fn no_sites_found() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/integration/v1/sites"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 25, "count": 0, "totalCount": 0,
                "data": []
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 0, "totalCount": 0, "data": []
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let err = client.list_clients().await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No sites found") && msg.contains("API key"));
    }

    #[tokio::test]
    async fn post_command_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client.block_client("aa:bb:cc:dd:ee:ff").await.unwrap_err();
        assert!(err.to_string().contains("Authentication error:"));
    }

    #[tokio::test]
    async fn list_events_returns_events() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"key": "EVT_WU_Connected", "msg": "User connected", "subsystem": "wlan", "time": 1700000000, "datetime": "2024-01-01T00:00:00Z"},
                    {"key": "EVT_LU_Disconnected", "msg": "User disconnected", "subsystem": "lan", "time": 1700000001}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let events = client.list_events(10).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key.as_deref(), Some("EVT_WU_Connected"));
        assert_eq!(events[1].subsystem.as_deref(), Some("lan"));
    }

    #[tokio::test]
    async fn list_clients_legacy_returns_clients() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:ff", "ip": "10.0.0.1", "name": "Desktop", "is_wired": true, "tx_bytes": 1000000, "rx_bytes": 2000000},
                    {"_id": "c2", "mac": "11:22:33:44:55:66", "ip": "10.0.0.2", "hostname": "phone", "is_wired": false, "tx_bytes": 500, "rx_bytes": 300}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let clients = client.list_clients_legacy().await.unwrap();
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].tx_bytes, Some(1000000));
        assert_eq!(clients[1].display_name(), "phone");
    }

    #[tokio::test]
    async fn get_device_ports_finds_by_mac() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "9c:05:d6:bc:06:43", "name": "USW-24-PoE",
                    "model": "USW-24-PoE",
                    "port_table": [
                        {"port_idx": 1, "name": "Port 1", "media": "GE", "up": true, "speed": 1000, "full_duplex": true, "poe_enable": true, "poe_power": 5.2, "port_poe": true, "tx_bytes": 123456, "rx_bytes": 654321},
                        {"port_idx": 2, "name": "Port 2", "media": "GE", "up": false, "speed": 0, "full_duplex": false, "poe_enable": false, "port_poe": true, "tx_bytes": 0, "rx_bytes": 0}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let device = client.get_device_ports("9c:05:d6:bc:06:43").await.unwrap();
        assert_eq!(device.port_table.len(), 2);
        assert!(device.port_table[0].up);
        assert!(!device.port_table[1].up);
        assert_eq!(device.port_table[0].poe_power, Some(5.2));
    }

    #[tokio::test]
    async fn get_device_ports_not_found() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = client
            .get_device_ports("00:00:00:00:00:00")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"));
    }

    #[tokio::test]
    async fn list_clients_legacy_sorted_by_bandwidth_descending() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:01", "name": "Light", "is_wired": true, "tx_bytes": 100, "rx_bytes": 200},
                    {"_id": "c2", "mac": "aa:bb:cc:dd:ee:02", "name": "Heavy", "is_wired": true, "tx_bytes": 5000000, "rx_bytes": 10000000},
                    {"_id": "c3", "mac": "aa:bb:cc:dd:ee:03", "name": "Medium", "is_wired": false, "tx_bytes": 50000, "rx_bytes": 60000}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let mut clients = client.list_clients_legacy().await.unwrap();

        // Verify sorting matches what `clients top` does
        clients
            .sort_by_key(|c| std::cmp::Reverse(c.tx_bytes.unwrap_or(0) + c.rx_bytes.unwrap_or(0)));

        assert_eq!(clients[0].display_name(), "Heavy");
        assert_eq!(clients[1].display_name(), "Medium");
        assert_eq!(clients[2].display_name(), "Light");
    }

    #[tokio::test]
    async fn get_device_ports_field_values() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:ee:ff", "name": "TestSwitch",
                    "port_table": [
                        {"port_idx": 1, "name": "Uplink", "media": "GE", "up": true, "speed": 1000, "full_duplex": true, "poe_enable": true, "poe_power": 12.5, "port_poe": true, "tx_bytes": 999999, "rx_bytes": 888888},
                        {"port_idx": 2, "up": false, "port_poe": false},
                        {"port_idx": 3, "up": true, "speed": 100, "full_duplex": false, "poe_enable": false, "port_poe": true}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let device = client.get_device_ports("aa:bb:cc:dd:ee:ff").await.unwrap();
        assert_eq!(device.name.as_deref(), Some("TestSwitch"));

        // Port 1: full data
        let p1 = &device.port_table[0];
        assert_eq!(p1.port_idx, Some(1));
        assert_eq!(p1.name.as_deref(), Some("Uplink"));
        assert!(p1.up);
        assert_eq!(p1.speed, Some(1000));
        assert!(p1.full_duplex);
        assert!(p1.poe_enable);
        assert_eq!(p1.poe_power, Some(12.5));
        assert_eq!(p1.tx_bytes, Some(999999));
        assert_eq!(p1.rx_bytes, Some(888888));

        // Port 2: minimal data, down
        let p2 = &device.port_table[1];
        assert!(!p2.up);
        assert!(p2.name.is_none());
        assert!(!p2.port_poe);

        // Port 3: up, half duplex, PoE-capable but disabled
        let p3 = &device.port_table[2];
        assert!(p3.up);
        assert_eq!(p3.speed, Some(100));
        assert!(!p3.full_duplex);
        assert!(!p3.poe_enable);
        assert!(p3.port_poe);
    }
}

// --- Command output tests ---
// These exercise the commands::* functions which format and print results

mod command_output {
    use super::*;
    use unifi_cli::output::OutputConfig;

    fn out_table() -> OutputConfig {
        OutputConfig {
            json: false,
            quiet: false,
        }
    }

    fn out_json() -> OutputConfig {
        OutputConfig {
            json: true,
            quiet: false,
        }
    }

    // Helper: mount sites + clients list endpoint
    async fn mount_clients_list(server: &MockServer) {
        mount_site_discovery(server).await;
        Mock::given(method("GET"))
            .and(path_regex(r"/proxy/network/integration/v1/sites/.*/clients"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"macAddress": "aa:bb:cc:dd:ee:ff", "ipAddress": "10.0.0.1", "name": "Device1", "type": "WIRED"},
                    {"macAddress": "11:22:33:44:55:66", "ipAddress": "10.0.0.2", "hostname": "host2", "type": "WIRELESS"}
                ]
            })))
            .mount(server)
            .await;
    }

    fn no_filter() -> unifi_cli::commands::clients::ListFilter {
        unifi_cli::commands::clients::ListFilter {
            wired: false,
            wireless: false,
            name: None,
        }
    }

    #[tokio::test]
    async fn clients_list_table() {
        let server = MockServer::start().await;
        mount_clients_list(&server).await;
        let mut client = mock_client(&server).await;
        unifi_cli::commands::clients::list(&mut client, out_table(), no_filter(), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_list_json() {
        let server = MockServer::start().await;
        mount_clients_list(&server).await;
        let mut client = mock_client(&server).await;
        unifi_cli::commands::clients::list(&mut client, out_json(), no_filter(), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_show_wired_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "ip": "10.0.0.1",
                    "name": "WiredDevice", "is_wired": true, "uptime": 86400,
                    "tx_bytes": 1048576, "rx_bytes": 2097152
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::show(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_show_wireless_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "_id": "def", "mac": "11:22:33:44:55:66", "ip": "10.0.0.2",
                    "name": "WirelessDevice", "is_wired": false, "uptime": 3600,
                    "tx_bytes": 512000, "rx_bytes": 1024000,
                    "signal": -55, "essid": "Notwork", "ap_mac": "60:22:32:58:b8:00"
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::show(&client, "11:22:33:44:55:66", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_show_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "ip": "10.0.0.1",
                    "name": "Device", "is_wired": true
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::show(&client, "aa:bb:cc:dd:ee:ff", out_json())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_set_fixed_ip_output() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"_id": "c1", "mac": "aa:bb:cc:dd:ee:ff"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/proxy/network/api/s/default/rest/user/c1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": [{}]})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::set_fixed_ip(
            &client,
            "aa:bb:cc:dd:ee:ff",
            "10.0.0.50",
            None,
            out_table(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn clients_set_fixed_ip_with_name_output() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"_id": "c1", "mac": "aa:bb:cc:dd:ee:ff"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/proxy/network/api/s/default/rest/user/c1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": [{}]})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::set_fixed_ip(
            &client,
            "aa:bb:cc:dd:ee:ff",
            "10.0.0.50",
            Some("MyDevice"),
            out_table(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn clients_block_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": []})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::block(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_unblock_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": []})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::unblock(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_kick_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/stamgr"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": []})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::kick(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_list_table() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(r"/proxy/network/integration/v1/sites/.*/devices"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 1, "totalCount": 1,
                "data": [{"macAddress": "9c:05:d6:bc:06:43", "ipAddress": "192.168.1.1", "name": "UCG Ultra", "model": "UCG Ultra", "state": "ONLINE", "firmwareVersion": "5.0.12"}]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        unifi_cli::commands::devices::list(&mut client, out_table(), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_list_json() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(r"/proxy/network/integration/v1/sites/.*/devices"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 1, "totalCount": 1,
                "data": [{"macAddress": "9c:05:d6:bc:06:43", "name": "UCG Ultra", "model": "UCG Ultra", "state": "ONLINE", "firmwareVersion": "5.0.12"}]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        unifi_cli::commands::devices::list(&mut client, out_json(), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_restart_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": []})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::restart(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_locate_on_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": []})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::locate(&client, "aa:bb:cc:dd:ee:ff", false, out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_locate_off_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"meta": {"rc": "ok"}, "data": []})),
            )
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::locate(&client, "aa:bb:cc:dd:ee:ff", true, out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn networks_list_table() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/networks",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"name": "Default", "enabled": true, "vlanId": 1, "default": true},
                    {"name": "IoT", "enabled": true, "vlanId": 20, "default": false}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        unifi_cli::commands::networks::list(&mut client, out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn networks_list_json() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/networks",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"name": "Default", "enabled": true, "vlanId": 1, "default": true},
                    {"name": "IoT", "enabled": true, "vlanId": 20, "default": false}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        unifi_cli::commands::networks::list(&mut client, out_json())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn system_health_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"subsystem": "wan", "status": "ok", "wan_ip": "1.2.3.4", "isp_name": "ISP"},
                    {"subsystem": "wlan", "status": "ok", "num_ap": 2, "num_sta": 10},
                    {"subsystem": "lan", "status": "ok", "num_sw": 3, "num_sta": 5},
                    {"subsystem": "vpn", "status": "unknown"}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::system::health(&client, out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn system_health_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"subsystem": "wan", "status": "ok", "wan_ip": "1.2.3.4", "isp_name": "ISP"}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::system::health(&client, out_json())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn system_info_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sysinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"hostname": "UCG-Ultra", "version": "10.1.85", "timezone": "Europe/Amsterdam", "uptime": 1737960}]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::system::info(&client, out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn system_info_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sysinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"hostname": "UCG-Ultra", "version": "10.1.85", "timezone": "Europe/Amsterdam", "uptime": 1737960}]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::system::info(&client, out_json())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn system_info_partial_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sysinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"hostname": "UCG-Ultra"}]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::system::info(&client, out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_show_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "9c:05:d6:bc:06:43", "ip": "192.168.1.1",
                    "name": "UCG Ultra", "model": "UCG Ultra",
                    "state": 1, "version": "5.0.12", "uptime": 86400, "num_sta": 42
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::show(&client, "9c:05:d6:bc:06:43", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_show_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "9c:05:d6:bc:06:43", "ip": "192.168.1.1",
                    "name": "UCG Ultra", "model": "UCG Ultra",
                    "state": 1, "version": "5.0.12"
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::show(&client, "9c:05:d6:bc:06:43", out_json())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_show_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = unifi_cli::commands::devices::show(&client, "00:00:00:00:00:00", out_table())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"));
    }

    // --- Filter integration tests ---
    // apply_filter is private, so we test filtering through the list command

    #[tokio::test]
    async fn clients_list_wired_filter() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 3, "totalCount": 3,
                "data": [
                    {"macAddress": "aa:bb:cc:dd:ee:01", "name": "WiredDevice", "type": "WIRED"},
                    {"macAddress": "aa:bb:cc:dd:ee:02", "name": "WirelessDevice", "type": "WIRELESS"},
                    {"macAddress": "aa:bb:cc:dd:ee:03", "name": "AnotherWired", "type": "WIRED"}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let filter = unifi_cli::commands::clients::ListFilter {
            wired: true,
            wireless: false,
            name: None,
        };
        // Should succeed (filter happens internally, we verify no error)
        unifi_cli::commands::clients::list(&mut client, out_json(), filter, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_list_wireless_filter() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"macAddress": "aa:bb:cc:dd:ee:01", "name": "WiredDevice", "type": "WIRED"},
                    {"macAddress": "aa:bb:cc:dd:ee:02", "name": "WirelessDevice", "type": "WIRELESS"}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let filter = unifi_cli::commands::clients::ListFilter {
            wired: false,
            wireless: true,
            name: None,
        };
        unifi_cli::commands::clients::list(&mut client, out_json(), filter, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_list_name_filter() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"/proxy/network/integration/v1/sites/.*/clients",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 3, "totalCount": 3,
                "data": [
                    {"macAddress": "aa:bb:cc:dd:ee:01", "name": "iPhone", "type": "WIRELESS"},
                    {"macAddress": "aa:bb:cc:dd:ee:02", "name": "Desktop", "type": "WIRED"},
                    {"macAddress": "aa:bb:cc:dd:ee:03", "name": "iPad", "type": "WIRELESS"}
                ]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        let filter = unifi_cli::commands::clients::ListFilter {
            wired: false,
            wireless: false,
            name: Some("phone".into()),
        };
        unifi_cli::commands::clients::list(&mut client, out_json(), filter, None)
            .await
            .unwrap();
    }

    // --- Events ---

    #[tokio::test]
    async fn events_list_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"key": "EVT_WU_Connected", "msg": "User[aa:bb:cc:dd:ee:ff] has connected", "subsystem": "wlan", "datetime": "2024-01-15T10:30:00Z"},
                    {"key": "EVT_SW_PoeOverload", "msg": "PoE overload on port 5", "subsystem": "lan", "datetime": "2024-01-15T10:29:00Z"}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::events::list(&client, out_table(), 10)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn events_list_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"key": "EVT_WU_Connected", "msg": "User connected", "subsystem": "wlan", "time": 1700000000}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::events::list(&client, out_json(), 5)
            .await
            .unwrap();
    }

    // --- Clients top ---

    #[tokio::test]
    async fn clients_top_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:01", "ip": "10.0.0.1", "name": "Heavy User", "is_wired": true, "tx_bytes": 5000000000_u64, "rx_bytes": 10000000000_u64},
                    {"_id": "c2", "mac": "aa:bb:cc:dd:ee:02", "ip": "10.0.0.2", "name": "Light User", "is_wired": false, "tx_bytes": 1000, "rx_bytes": 2000},
                    {"_id": "c3", "mac": "aa:bb:cc:dd:ee:03", "ip": "10.0.0.3", "hostname": "medium-host", "is_wired": true, "tx_bytes": 500000, "rx_bytes": 600000}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::top(&client, out_table(), 2)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn clients_top_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:01", "ip": "10.0.0.1", "name": "User1", "is_wired": true, "tx_bytes": 100, "rx_bytes": 200}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::clients::top(&client, out_json(), 10)
            .await
            .unwrap();
    }

    // --- Devices ports ---

    #[tokio::test]
    async fn devices_ports_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "9c:05:d6:bc:06:43", "name": "USW-24-PoE", "model": "USW-24-PoE",
                    "port_table": [
                        {"port_idx": 1, "name": "Port 1", "media": "GE", "up": true, "speed": 1000, "full_duplex": true, "poe_enable": true, "poe_power": 5.2, "port_poe": true, "tx_bytes": 123456789, "rx_bytes": 987654321},
                        {"port_idx": 2, "name": "Port 2", "media": "GE", "up": true, "speed": 100, "full_duplex": false, "poe_enable": false, "port_poe": true, "tx_bytes": 1000, "rx_bytes": 2000},
                        {"port_idx": 3, "name": "Port 3", "media": "GE", "up": false, "poe_enable": false, "port_poe": false}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::ports(&client, "9c:05:d6:bc:06:43", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_ports_json() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "9c:05:d6:bc:06:43", "name": "USW-Lite-8",
                    "port_table": [
                        {"port_idx": 1, "name": "Port 1", "media": "GE", "up": true, "speed": 1000, "full_duplex": true, "poe_enable": true, "poe_power": 3.8, "port_poe": true, "tx_bytes": 100, "rx_bytes": 200}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::ports(&client, "9c:05:d6:bc:06:43", out_json())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_ports_empty_port_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:ee:ff", "name": "UAP-AC-Pro",
                    "port_table": []
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::ports(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_ports_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = unifi_cli::commands::devices::ports(&client, "00:00:00:00:00:00", out_table())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"));
    }
}

// --- Client construction tests ---

mod client_construction {
    #[test]
    fn new_with_https_host() {
        let client = unifi_cli::api::UnifiClient::new("https://unifi.example.com", "key123");
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_http_host() {
        let client = unifi_cli::api::UnifiClient::new("http://localhost:8443", "key123");
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_bare_host() {
        let client = unifi_cli::api::UnifiClient::new("unifi.local", "key123");
        assert!(client.is_ok());
    }

    #[test]
    fn new_strips_trailing_slash() {
        let client = unifi_cli::api::UnifiClient::new("https://unifi.local/", "key123");
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_invalid_api_key() {
        let client = unifi_cli::api::UnifiClient::new("host", "bad\nkey");
        assert!(client.is_err());
    }
}
