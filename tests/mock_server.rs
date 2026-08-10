use wiremock::matchers::{body_json, method, path, path_regex};
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
                    {"macAddress": "aa:bb:cc:dd:ee:ff", "ipAddress": "192.0.2.1", "name": "Device1", "type": "WIRED"},
                    {"macAddress": "11:22:33:44:55:66", "ipAddress": "192.0.2.2", "hostname": "host2", "type": "WIRELESS"}
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
                    {"_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "ip": "192.0.2.1", "name": "Target", "is_wired": true, "uptime": 7200},
                    {"_id": "def", "mac": "11:22:33:44:55:66", "ip": "192.0.2.2"}
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
            .set_fixed_ip("aa:bb:cc:dd:ee:ff", "192.0.2.50", None)
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
            .set_fixed_ip("aa:bb:cc:dd:ee:ff", "192.0.2.99", Some("NewDevice"))
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
            .set_fixed_ip("00:00:00:00:00:00", "192.0.2.1", None)
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
                    {"macAddress": "aa:bb:cc:dd:06:43", "ipAddress": "198.51.100.1", "name": "UCG Ultra", "model": "UCG Ultra", "state": "ONLINE", "firmwareVersion": "5.0.12"},
                    {"macAddress": "aa:bb:cc:dd:b8:00", "ipAddress": "198.51.100.190", "name": "U6-Lite", "model": "U6 Lite", "state": "ONLINE", "firmwareVersion": "6.7.41"}
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
    async fn power_cycle_port_sends_correct_command() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .and(body_json(serde_json::json!({
                "cmd": "power-cycle",
                "mac": "aa:bb:cc:dd:ee:ff",
                "port_idx": 5
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        client
            .power_cycle_port("AA-BB-CC-DD-EE-FF", 5)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn upgrade_device_sends_correct_command() {
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
        client.upgrade_device("aa:bb:cc:dd:ee:ff").await.unwrap();
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
                    {"subsystem": "wan", "status": "ok", "wan_ip": "203.0.113.156", "isp_name": "ExampleISP"},
                    {"subsystem": "wlan", "status": "ok", "num_ap": 3, "num_sta": 15},
                    {"subsystem": "lan", "status": "ok", "num_sw": 4, "num_sta": 20}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let health = client.get_health().await.unwrap();
        assert_eq!(health.len(), 3);
        assert_eq!(health[0].wan_ip.as_deref(), Some("203.0.113.156"));
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

    #[tokio::test]
    async fn list_all_device_ports_returns_every_device() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"mac": "aa:bb:cc:dd:ee:ff", "name": "SwitchA",
                     "port_table": [{"port_idx": 1, "port_poe": true}]},
                    {"mac": "11:22:33:44:55:66", "name": "SwitchB",
                     "port_table": [{"port_idx": 1}, {"port_idx": 2}]}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let devices = client.list_all_device_ports().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].port_table.len(), 2);
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
        let msg = err.to_string();
        assert!(msg.contains("Authentication error:"));
        assert!(msg.contains("Hint:"));
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
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:ff", "ip": "192.0.2.1", "name": "Desktop", "is_wired": true, "tx_bytes": 1000000, "rx_bytes": 2000000},
                    {"_id": "c2", "mac": "11:22:33:44:55:66", "ip": "192.0.2.2", "hostname": "phone", "is_wired": false, "tx_bytes": 500, "rx_bytes": 300}
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
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
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
        let device = client.get_device_ports("aa:bb:cc:dd:06:43").await.unwrap();
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
        OutputConfig::new(unifi_cli::output::OutputFormat::Text, false)
    }

    fn out_json() -> OutputConfig {
        OutputConfig::new(unifi_cli::output::OutputFormat::Json, false)
    }

    fn default_pagination() -> unifi_cli::commands::clients::Pagination {
        unifi_cli::commands::clients::Pagination {
            limit: 100,
            offset: 0,
            fields: None,
        }
    }

    fn default_devices_pagination() -> unifi_cli::commands::devices::Pagination {
        unifi_cli::commands::devices::Pagination {
            limit: 100,
            offset: 0,
            fields: None,
        }
    }

    fn default_events_pagination(limit: usize) -> unifi_cli::commands::events::Pagination {
        unifi_cli::commands::events::Pagination {
            limit,
            offset: 0,
            fields: None,
        }
    }

    // Helper: mount sites + both client endpoints.
    //
    // `clients list` joins the integration-API record to the live legacy record
    // so it can report SSID, network and the address the client actually holds.
    // Device1's live address deliberately differs from the integration API's
    // last-known value, and host2 has no live address at all.
    async fn mount_clients_list(server: &MockServer) {
        mount_site_discovery(server).await;
        Mock::given(method("GET"))
            .and(path_regex(r"/proxy/network/integration/v1/sites/.*/clients"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "offset": 0, "limit": 200, "count": 2, "totalCount": 2,
                "data": [
                    {"macAddress": "aa:bb:cc:dd:ee:ff", "ipAddress": "192.0.2.1", "name": "Device1", "type": "WIRED"},
                    {"macAddress": "11:22:33:44:55:66", "ipAddress": "192.0.2.2", "hostname": "host2", "type": "WIRELESS"}
                ]
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "1", "mac": "aa:bb:cc:dd:ee:ff", "ip": "192.0.2.99",
                     "is_wired": true, "network": "Default", "vlan": 1},
                    {"_id": "2", "mac": "11:22:33:44:55:66", "essid": "GuestNet",
                     "signal": -55, "uptime": 100, "network": "IoT", "vlan": 20}
                ]
            })))
            .mount(server)
            .await;
    }

    /// `clients list` also reads the live legacy view. Tests that only care about
    /// filtering can serve an empty one.
    async fn mount_empty_legacy_clients(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"}, "data": []
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
        unifi_cli::commands::clients::list(
            &mut client,
            out_table(),
            no_filter(),
            None,
            default_pagination(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn clients_list_json() {
        let server = MockServer::start().await;
        mount_clients_list(&server).await;
        let mut client = mock_client(&server).await;
        unifi_cli::commands::clients::list(
            &mut client,
            out_json(),
            no_filter(),
            None,
            default_pagination(),
        )
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
                    "_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "ip": "192.0.2.1",
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
                    "_id": "def", "mac": "11:22:33:44:55:66", "ip": "192.0.2.2",
                    "name": "WirelessDevice", "is_wired": false, "uptime": 3600,
                    "tx_bytes": 512000, "rx_bytes": 1024000,
                    "signal": -55, "essid": "GuestNet", "ap_mac": "aa:bb:cc:dd:b8:00"
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
                    "_id": "abc", "mac": "aa:bb:cc:dd:ee:ff", "ip": "192.0.2.1",
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
            "192.0.2.50",
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
            "192.0.2.50",
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
                "data": [{"macAddress": "aa:bb:cc:dd:06:43", "ipAddress": "198.51.100.1", "name": "UCG Ultra", "model": "UCG Ultra", "state": "ONLINE", "firmwareVersion": "5.0.12"}]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        unifi_cli::commands::devices::list(
            &mut client,
            out_table(),
            None,
            default_devices_pagination(),
        )
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
                "data": [{"macAddress": "aa:bb:cc:dd:06:43", "name": "UCG Ultra", "model": "UCG Ultra", "state": "ONLINE", "firmwareVersion": "5.0.12"}]
            })))
            .mount(&server)
            .await;

        let mut client = mock_client(&server).await;
        unifi_cli::commands::devices::list(
            &mut client,
            out_json(),
            None,
            default_devices_pagination(),
        )
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
                    {"subsystem": "wan", "status": "ok", "wan_ip": "203.0.113.4", "isp_name": "ISP"},
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
                    {"subsystem": "wan", "status": "ok", "wan_ip": "203.0.113.4", "isp_name": "ISP"}
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
                    "mac": "aa:bb:cc:dd:06:43", "ip": "198.51.100.1",
                    "name": "UCG Ultra", "model": "UCG Ultra",
                    "state": 1, "version": "5.0.12", "uptime": 86400, "num_sta": 42
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::show(&client, "aa:bb:cc:dd:06:43", out_table())
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
                    "mac": "aa:bb:cc:dd:06:43", "ip": "198.51.100.1",
                    "name": "UCG Ultra", "model": "UCG Ultra",
                    "state": 1, "version": "5.0.12"
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::show(&client, "aa:bb:cc:dd:06:43", out_json())
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
        mount_empty_legacy_clients(&server).await;
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
        unifi_cli::commands::clients::list(
            &mut client,
            out_json(),
            filter,
            None,
            default_pagination(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn clients_list_wireless_filter() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        mount_empty_legacy_clients(&server).await;
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
        unifi_cli::commands::clients::list(
            &mut client,
            out_json(),
            filter,
            None,
            default_pagination(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn clients_list_name_filter() {
        let server = MockServer::start().await;
        mount_site_discovery(&server).await;
        mount_empty_legacy_clients(&server).await;
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
        unifi_cli::commands::clients::list(
            &mut client,
            out_json(),
            filter,
            None,
            default_pagination(),
        )
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
        unifi_cli::commands::events::list(&client, out_table(), default_events_pagination(10))
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
        unifi_cli::commands::events::list(&client, out_json(), default_events_pagination(5))
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
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:01", "ip": "192.0.2.1", "name": "Heavy User", "is_wired": true, "tx_bytes": 5000000000_u64, "rx_bytes": 10000000000_u64},
                    {"_id": "c2", "mac": "aa:bb:cc:dd:ee:02", "ip": "192.0.2.2", "name": "Light User", "is_wired": false, "tx_bytes": 1000, "rx_bytes": 2000},
                    {"_id": "c3", "mac": "aa:bb:cc:dd:ee:03", "ip": "192.0.2.3", "hostname": "medium-host", "is_wired": true, "tx_bytes": 500000, "rx_bytes": 600000}
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
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:01", "ip": "192.0.2.1", "name": "User1", "is_wired": true, "tx_bytes": 100, "rx_bytes": 200}
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
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE", "model": "USW-24-PoE",
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
        unifi_cli::commands::devices::ports(&client, "aa:bb:cc:dd:06:43", out_table())
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
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-Lite-8",
                    "port_table": [
                        {"port_idx": 1, "name": "Port 1", "media": "GE", "up": true, "speed": 1000, "full_duplex": true, "poe_enable": true, "poe_power": 3.8, "port_poe": true, "tx_bytes": 100, "rx_bytes": 200}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::devices::ports(&client, "aa:bb:cc:dd:06:43", out_json())
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
    async fn devices_upgrade_output() {
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
        unifi_cli::commands::devices::upgrade(&client, "aa:bb:cc:dd:ee:ff", out_table())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn devices_upgrade_json() {
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
        unifi_cli::commands::devices::upgrade(&client, "aa:bb:cc:dd:ee:ff", out_json())
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

    // `devices::ports` used to derive its own `name -> model -> "Device"`
    // device-label fallback; routing it through the shared `collect_rows`
    // silently changed the fallback to "-" for a device with neither `name`
    // nor `model`, and nothing caught it. Drives the real binary (JSON is
    // easiest to assert on) so the regression is locked in at the command
    // level, not just in the `collect_rows_with_fallback` unit test in
    // `src/commands/ports.rs`.
    #[tokio::test]
    async fn devices_ports_falls_back_to_device_label_when_name_and_model_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:ee:ff",
                    "port_table": [{"port_idx": 1}]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "devices",
                "ports",
                "aa:bb:cc:dd:ee:ff",
                "-o",
                "json",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "devices ports failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let items = body
            .as_array()
            .expect("devices ports must emit a bare JSON array");
        assert_eq!(
            items[0]["device_name"], "Device",
            "devices ports must keep its historical \"Device\" fallback, not \"-\": {items:?}"
        );
    }

    // --- Ports show (single-port detail) ---

    #[tokio::test]
    async fn ports_show_table() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [
                        {"port_idx": 1, "name": "Port 1", "media": "GE", "up": true},
                        {
                            "port_idx": 5, "name": "Port 5", "media": "GE", "up": true,
                            "speed": 1000, "full_duplex": true, "autoneg": true, "enable": true,
                            "is_uplink": false, "stp_state": "forwarding",
                            "port_poe": true, "poe_enable": true, "poe_mode": "auto",
                            "poe_class": "4", "poe_power": 5.2, "poe_voltage": 53.5,
                            "poe_current": 120.3, "poe_good": true,
                            "last_connection": {"mac": "aabbccddeeff", "connected": true},
                            "tx_bytes": 100, "rx_bytes": 200, "tx_errors": 0, "rx_errors": 2
                        }
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        unifi_cli::commands::ports::show(&client, "aa:bb:cc:dd:06:43", 5, out_table())
            .await
            .unwrap();
    }

    // `ports_show_table` above only smoke-tests that the text branch does not
    // panic. The text branch carries real formatting logic (speed_cell,
    // poe_cell, voltage/current, the attached MAC), so it also gets a test that
    // spawns the real binary and asserts on the rendered text.
    #[tokio::test]
    async fn ports_show_text_output_renders_expected_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{
                        "port_idx": 5, "name": "Port 5", "media": "GE", "up": true,
                        "speed": 1000, "full_duplex": true,
                        "port_poe": true, "poe_enable": true, "poe_mode": "auto",
                        "poe_class": "4", "poe_power": 5.2, "poe_voltage": 53.5,
                        "poe_current": 120.3,
                        "last_connection": {"mac": "aabbccddeeff", "connected": true}
                    }]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "show",
                "aa:bb:cc:dd:06:43",
                "5",
                "--output",
                "text",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("Port 5 on USW-24-PoE (aa:bb:cc:dd:06:43)"),
            "title line: {text}"
        );
        assert!(text.contains("Port 5"), "port name: {text}");
        assert!(text.contains("1000FD"), "speed+duplex formatting: {text}");
        assert!(text.contains("GE"), "media: {text}");
        assert!(text.contains("5.2W"), "PoE wattage: {text}");
        assert!(text.contains("auto"), "PoE mode: {text}");
        assert!(text.contains("53.50 V"), "PoE voltage: {text}");
        assert!(text.contains("120.30 mA"), "PoE current: {text}");
        assert!(text.contains("aa:bb:cc:dd:ee:ff"), "attached MAC: {text}");
    }

    // Drives the real `unifi` binary so the JSON this command actually prints
    // can be inspected, and cross-checks it against what `unifi schema`
    // publishes for "ports show": the two are supposed to be the same
    // contract, and nothing else in this suite would catch them drifting
    // apart.
    #[tokio::test]
    async fn ports_show_json_matches_schema_output_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{
                        "port_idx": 5, "name": "Port 5", "media": "GE", "up": true,
                        "speed": 1000, "full_duplex": true, "autoneg": true, "enable": true,
                        "is_uplink": false, "stp_state": "forwarding",
                        "port_poe": true, "poe_enable": true, "poe_mode": "auto",
                        "poe_class": "4", "poe_power": 5.2, "poe_voltage": 53.5,
                        "poe_current": 120.3, "poe_good": true,
                        "last_connection": {"mac": "aabbccddeeff", "connected": true},
                        "tx_bytes": 100, "rx_bytes": 200, "tx_errors": 0, "rx_errors": 2
                    }]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "show",
                "aa:bb:cc:dd:06:43",
                "5",
                "--output",
                "json",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout was not valid JSON ({e}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        let obj = body
            .as_object()
            .expect("ports show must emit a JSON object");

        // Values that were previously fetched and thrown away.
        assert_eq!(obj["device_mac"], "aa:bb:cc:dd:06:43");
        assert_eq!(obj["port_idx"], 5);
        assert_eq!(obj["poe_mode"], "auto");
        assert_eq!(obj["poe_class"], "4");
        assert_eq!(obj["poe_voltage"], 53.5);
        assert_eq!(obj["poe_current"], 120.3);
        assert_eq!(obj["poe_good"], true);
        assert_eq!(
            obj["attached_mac"], "aa:bb:cc:dd:ee:ff",
            "attached_mac must be read from last_connection.mac and formatted"
        );
        assert_eq!(obj["tx_errors"], 0);
        assert_eq!(obj["rx_errors"], 2);

        // The schema's published output_fields must exactly match the keys
        // this JSON branch actually emits: no undiscoverable field, and no
        // documented field that never appears.
        let schema_output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .arg("schema")
            .output()
            .expect("failed to run unifi schema");
        let schema: serde_json::Value = serde_json::from_slice(&schema_output.stdout)
            .expect("unifi schema must print valid JSON");
        let ports_show = schema["commands"]
            .as_array()
            .expect("schema must have a commands array")
            .iter()
            .find(|c| c["name"] == "ports show")
            .expect("schema must publish a \"ports show\" command");
        let mut declared: Vec<&str> = ports_show["output_fields"]
            .as_array()
            .expect("ports show must declare output_fields")
            .iter()
            .map(|f| f["name"].as_str().expect("output field must have a name"))
            .collect();
        declared.sort_unstable();
        let mut emitted: Vec<&str> = obj.keys().map(String::as_str).collect();
        emitted.sort_unstable();
        assert_eq!(
            emitted, declared,
            "ports show output_fields in the schema must exactly match the JSON branch's keys"
        );
    }

    // `autoneg`/`enable`/`is_uplink`/`poe_good` are tri-state: a firmware that
    // omits the key must serialize as JSON null, not fall back to `false`,
    // since a missing key must not read as a confident "disabled". Likewise
    // `attached_mac` must be null when no device has ever linked to the port.
    #[tokio::test]
    async fn ports_show_omitted_tri_state_fields_serialize_as_null() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:ee:ff", "name": "USW-Lite-8",
                    "port_table": [
                        {"port_idx": 3, "name": "Port 3", "media": "GE", "up": false}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "show",
                "aa:bb:cc:dd:ee:ff",
                "3",
                "--output",
                "json",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(output.status.success());

        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for field in ["autoneg", "enable", "is_uplink", "poe_good", "attached_mac"] {
            assert!(
                body[field].is_null(),
                "{field} must be null when firmware omits it, not false: {body}"
            );
        }
    }

    // A `last_connection` the controller has marked `connected: false` is
    // history, not an attachment: the device may have been unplugged months
    // ago. Reporting it as attached would tell an operator a port is in use
    // moments before they cut its power, so `attached_mac` must be null and the
    // MAC must survive only as `attached_last_seen_mac`.
    #[tokio::test]
    async fn ports_show_reports_a_stale_last_connection_as_unattached() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{
                        "port_idx": 5, "name": "Port 5", "media": "GE", "up": false,
                        "port_poe": true, "poe_enable": true, "poe_mode": "auto",
                        "last_connection": {"mac": "aabbccddeeff", "connected": false}
                    }]
                }]
            })))
            .mount(&server)
            .await;

        let run = |format: &str| {
            std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
                .args([
                    "--host",
                    &server.uri(),
                    "--api-key",
                    "test-key",
                    "ports",
                    "show",
                    "aa:bb:cc:dd:06:43",
                    "5",
                    "--output",
                    format,
                ])
                .output()
                .expect("failed to run the unifi binary")
        };

        let json_out = run("json");
        assert!(
            json_out.status.success(),
            "ports show failed: {}",
            String::from_utf8_lossy(&json_out.stderr)
        );
        let body: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
        assert!(
            body["attached_mac"].is_null(),
            "a stale last_connection must not be published as attached: {body}"
        );
        assert_eq!(
            body["attached_last_seen_mac"], "aa:bb:cc:dd:ee:ff",
            "the stale MAC must stay available as history: {body}"
        );
        assert_eq!(
            body["attached_connected"], false,
            "the controller's own flag must be reported as it stands: {body}"
        );

        let text_out = run("text");
        assert!(text_out.status.success());
        let text = String::from_utf8_lossy(&text_out.stdout);
        assert!(
            text.contains("- (last seen aa:bb:cc:dd:ee:ff)"),
            "the text branch must qualify a stale MAC rather than print it bare: {text}"
        );
    }

    // A firmware that reports `last_connection.mac` without a `connected` flag
    // has said nothing about the present, which is not the same fact as
    // "disconnected". It is still not grounds to claim an attachment, so
    // `attached_mac` stays null, but the tri-state `attached_connected` and the
    // text output both distinguish "not reported" from "gone".
    #[tokio::test]
    async fn ports_show_distinguishes_an_unreported_connection_from_a_stale_one() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{
                        "port_idx": 5, "name": "Port 5", "media": "GE", "up": true,
                        "last_connection": {"mac": "aabbccddeeff"}
                    }]
                }]
            })))
            .mount(&server)
            .await;

        let run = |format: &str| {
            std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
                .args([
                    "--host",
                    &server.uri(),
                    "--api-key",
                    "test-key",
                    "ports",
                    "show",
                    "aa:bb:cc:dd:06:43",
                    "5",
                    "--output",
                    format,
                ])
                .output()
                .expect("failed to run the unifi binary")
        };

        let json_out = run("json");
        assert!(json_out.status.success());
        let body: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
        assert!(
            body["attached_mac"].is_null(),
            "an unreported connection must not be claimed as attached: {body}"
        );
        assert!(
            body["attached_connected"].is_null(),
            "a missing connected flag must stay null, not become false: {body}"
        );
        assert_eq!(body["attached_last_seen_mac"], "aa:bb:cc:dd:ee:ff");

        let text = String::from_utf8_lossy(&run("text").stdout).to_string();
        assert!(
            text.contains("unknown (last seen aa:bb:cc:dd:ee:ff)"),
            "the text branch must say the state is unknown, not that the device is gone: {text}"
        );
    }

    // Locates `unifi ports cycle <MAC> 99` uses the same `find_port` lookup;
    // a bogus port index must be reported as not-found (exit 4) rather than
    // firing a command at the controller for a port that does not exist.
    #[tokio::test]
    async fn ports_show_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:ee:ff", "name": "USW-Lite-8",
                    "port_table": [{"port_idx": 1}]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "show",
                "aa:bb:cc:dd:ee:ff",
                "99",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert_eq!(
            output.status.code(),
            Some(4),
            "a nonexistent port must exit 4 (not found), got {:?}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Not found"),
            "stderr must explain the port was not found: {stderr}"
        );
    }

    // The row-count trailer must read "1 port" for a single row and "N ports"
    // otherwise. Spawns the real binary (rather than calling `render_text`
    // in-process) so this observes literal stderr text, the same surface an
    // operator actually reads.
    #[tokio::test]
    async fn ports_list_trailer_is_singular_for_exactly_one_row() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"mac": "aa:bb:cc:dd:ee:01", "name": "SwitchA",
                          "port_table": [{"port_idx": 1}]}]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "list",
                "--output",
                "text",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.trim_end().ends_with("1 port"),
            "a single row must be reported as \"1 port\", not \"1 ports\": {stderr:?}"
        );
    }

    #[tokio::test]
    async fn ports_list_trailer_is_plural_for_multiple_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{"mac": "aa:bb:cc:dd:ee:01", "name": "SwitchA",
                          "port_table": [{"port_idx": 1}, {"port_idx": 2}]}]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "list",
                "--output",
                "text",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.trim_end().ends_with("2 ports"),
            "two rows must be reported as \"2 ports\": {stderr:?}"
        );
    }

    // --- Ports list (top-level) ---
    //
    // Drives the real `unifi` binary against a wiremock server so the JSON
    // envelope it actually prints can be inspected. A regression that computed
    // `total` from the truncated page (instead of the full flattened result)
    // would let an agent mistake a partial page for a complete one, so this
    // must observe real stdout rather than call `commands::ports::list`
    // in-process and only check that it returns `Ok`.
    #[tokio::test]
    async fn ports_list_pagination_reports_full_total_and_truncated_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"mac": "aa:bb:cc:dd:ee:01", "name": "SwitchA",
                     "port_table": [{"port_idx": 1}, {"port_idx": 2}]},
                    {"mac": "aa:bb:cc:dd:ee:02", "name": "SwitchB",
                     "port_table": [{"port_idx": 1}, {"port_idx": 2}, {"port_idx": 3}]}
                ]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "list",
                "--output",
                "json",
                "--limit",
                "3",
                "--offset",
                "1",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert!(
            output.status.success(),
            "ports list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout was not valid JSON ({e}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });

        let items = body["items"]
            .as_array()
            .expect("envelope must have an items array");
        assert_eq!(
            items.len(),
            3,
            "the page must be truncated to the requested limit"
        );
        assert_eq!(
            body["total"], 5,
            "total must reflect every port across every device, not just this page"
        );
        assert_ne!(
            body["total"].as_u64().unwrap(),
            items.len() as u64,
            "an agent must be able to tell a truncated page from a complete result"
        );
        assert_eq!(body["limit"], 3);
        assert_eq!(body["offset"], 1);
    }

    // `render_text`'s Device column width used to be derived from whatever
    // page it was handed, which for `ports list` is the already-paginated
    // page. Two `--offset` pages of the same query could then render the
    // column at different widths. The device names below are chosen so the
    // longest one falls on the second page only; if the width regressed back
    // to being page-local, the two headers would render at different widths
    // and this comparison would fail.
    #[tokio::test]
    async fn ports_list_device_column_width_is_stable_across_pages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"mac": "aa:bb:cc:dd:ee:01", "name": "SwitchA",
                     "port_table": [{"port_idx": 1}, {"port_idx": 2}]},
                    {"mac": "aa:bb:cc:dd:ee:02", "name": "A-Very-Long-Switch-Name",
                     "port_table": [{"port_idx": 1}]}
                ]
            })))
            .mount(&server)
            .await;

        let run_text = |limit: &str, offset: &str| -> String {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
                .args([
                    "--host",
                    &server.uri(),
                    "--api-key",
                    "test-key",
                    "ports",
                    "list",
                    "--output",
                    "text",
                    "--limit",
                    limit,
                    "--offset",
                    offset,
                ])
                .output()
                .expect("failed to run the unifi binary");
            assert!(output.status.success());
            String::from_utf8_lossy(&output.stdout).into_owned()
        };

        // Page 1: only SwitchA's two ports (the long name lives on page 2).
        let page1 = run_text("2", "0");
        // Page 2: only the long-named device's one port.
        let page2 = run_text("1", "2");

        fn header(s: &str) -> &str {
            s.lines()
                .find(|l| l.contains("Device"))
                .expect("text output must have a header row containing \"Device\"")
        }
        assert_eq!(
            header(&page1),
            header(&page2),
            "the Device column width must come from the full result set, not the page, \
             so two --offset pages of the same query render an identical header:\n\
             page1: {page1}\npage2: {page2}"
        );
    }

    // `devices ports <MAC>` is documented as an alias for `ports list <MAC>`
    // that deliberately keeps the historical bare JSON array shape, while
    // `ports list` emits the paginated `{items,total,limit,offset}` envelope.
    // Wrapping `devices ports` in the envelope would break any consumer that
    // indexes the top level, which the design explicitly forbids.
    //
    // Nothing else in this suite would catch that regression: the in-process
    // `devices_ports_*` tests above only `.unwrap()`/`.unwrap_err()` and never
    // capture stdout, and `devices_ports_and_ports_list_are_the_same_command`
    // in `tests/cli_contract.rs` only asserts the exit code isn't a usage
    // error. So this spawns the real compiled binary against a wiremock
    // server (same pattern as `ports_list_pagination_reports_full_total_and_truncated_items`
    // above) and parses actual stdout as JSON to assert on shape.
    #[tokio::test]
    async fn devices_ports_bare_array_vs_ports_list_envelope() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [
                        {"port_idx": 1, "name": "Port 1", "media": "GE", "up": true, "speed": 1000, "full_duplex": true, "poe_enable": true, "poe_power": 5.2, "port_poe": true, "tx_bytes": 123456789, "rx_bytes": 987654321},
                        {"port_idx": 2, "name": "Port 2", "media": "GE", "up": true, "speed": 100, "full_duplex": false, "poe_enable": false, "port_poe": true, "tx_bytes": 1000, "rx_bytes": 2000}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let run_json = |args: &[&str]| -> serde_json::Value {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
                .args(["--host", &server.uri(), "--api-key", "test-key"])
                .args(args)
                .output()
                .expect("failed to run the unifi binary");
            assert!(
                output.status.success(),
                "{args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
                panic!(
                    "{args:?} stdout was not valid JSON ({e}): {}",
                    String::from_utf8_lossy(&output.stdout)
                )
            })
        };

        let alias = run_json(&["devices", "ports", "aa:bb:cc:dd:06:43", "-o", "json"]);
        let canonical = run_json(&["ports", "list", "aa:bb:cc:dd:06:43", "-o", "json"]);

        // 1. `devices ports` must be a bare array, and rows must carry the
        //    device_mac/device_name fields shared with `ports list`.
        let alias_items = alias
            .as_array()
            .unwrap_or_else(|| panic!("devices ports must emit a bare JSON array, got: {alias}"));
        assert!(
            !alias_items.is_empty(),
            "expected at least one port row from devices ports"
        );
        let alias_row = alias_items[0]
            .as_object()
            .expect("devices ports row must be a JSON object");
        assert!(
            alias_row.contains_key("device_mac"),
            "devices ports row must carry device_mac: {alias_row:?}"
        );
        assert!(
            alias_row.contains_key("device_name"),
            "devices ports row must carry device_name: {alias_row:?}"
        );

        // 2. `ports list` must be the {items,total,limit,offset} envelope.
        assert!(
            canonical.is_object(),
            "ports list must emit an {{items,total,limit,offset}} envelope object, got: {canonical}"
        );
        let items = canonical["items"]
            .as_array()
            .expect("ports list envelope must have an items array");
        assert!(
            canonical.get("total").is_some(),
            "ports list envelope must have a total field"
        );
        assert!(
            canonical.get("limit").is_some(),
            "ports list envelope must have a limit field"
        );
        assert!(
            canonical.get("offset").is_some(),
            "ports list envelope must have an offset field"
        );
        assert!(
            !items.is_empty(),
            "expected at least one port row from ports list"
        );

        // 3. The two spellings must carry the same key set per row, locking
        //    in the shared-field-set property alongside the envelope split.
        let mut alias_keys: Vec<&str> = alias_row.keys().map(String::as_str).collect();
        alias_keys.sort_unstable();
        let mut canonical_keys: Vec<&str> = items[0]
            .as_object()
            .expect("ports list row must be a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        canonical_keys.sort_unstable();

        assert_eq!(
            alias_keys, canonical_keys,
            "devices ports and ports list must share the same per-row field set"
        );
    }

    // --- Ports find (reverse lookup) ---

    // A MAC identifier must resolve locally so the common scripted path stays
    // a single round trip; mounting `/stat/sta` with `.expect(0)` turns an
    // accidental client-list fetch into a test failure instead of a silent,
    // unnoticed second request. This also locks in connected-first sorting
    // and the exact `PORTS_FIND` field set end to end, through the real
    // binary and JSON output, not just the in-process helpers.
    #[tokio::test]
    async fn ports_find_by_mac_sorts_connected_first_and_skips_client_lookup() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [
                        {"port_idx": 2, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": false}},
                        {"port_idx": 7, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": true}},
                        {"port_idx": 9, "last_connection": {"mac": "11:22:33:44:55:66", "connected": true}}
                    ]
                }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"}, "data": []
            })))
            .expect(0)
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "find",
                "aa:bb:cc:dd:ee:10",
                "-o",
                "json",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports find failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let body: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout was not valid JSON ({e}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        let items = body
            .as_array()
            .expect("ports find must emit a bare JSON array, like `networks list`");
        assert_eq!(items.len(), 2, "the device appears on two ports");
        assert_eq!(
            items[0]["port_idx"], 7,
            "the connected port must sort first"
        );
        assert_eq!(items[0]["connected"], true);
        assert_eq!(items[1]["port_idx"], 2, "the stale record sorts last");
        assert_eq!(items[1]["connected"], false);

        let mut emitted: Vec<&str> = items[0]
            .as_object()
            .expect("row must be a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        emitted.sort_unstable();
        let mut declared: Vec<&str> = unifi_cli::fields::names(unifi_cli::fields::PORTS_FIND);
        declared.sort_unstable();
        assert_eq!(
            emitted, declared,
            "ports find rows must carry exactly the PORTS_FIND field set"
        );
    }

    // Ambiguity is judged by port occupancy, not by how many client records a
    // name matches: `office` genuinely matches two devices here, and both
    // are actually attached to a switch port (unlike the "one interface
    // never shows up" fixtures below), so this must still exit 6 (conflict)
    // and name both candidates. Modeled on a live-controller case: two
    // physically distinct office devices sharing a name on the same switch.
    #[tokio::test]
    async fn ports_find_ambiguous_name_exits_with_conflict() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "1", "mac": "aa:bb:cc:dd:ee:20", "name": "office-ap", "ip": "192.0.2.6"},
                    {"_id": "2", "mac": "aa:bb:cc:dd:ee:21", "name": "Main-Office", "ip": "192.0.2.7"}
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW Pro XG 8 PoE",
                    "port_table": [
                        {"port_idx": 3, "last_connection": {"mac": "aa:bb:cc:dd:ee:20", "connected": true}},
                        {"port_idx": 4, "last_connection": {"mac": "aa:bb:cc:dd:ee:21", "connected": true}}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "find",
                "office",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert_eq!(
            output.status.code(),
            Some(6),
            "an ambiguous name must exit 6 (conflict), got {:?}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last_line = stderr.trim_end().lines().last().unwrap_or("");
        let envelope: serde_json::Value =
            serde_json::from_str(last_line).expect("last stderr line must be valid JSON");
        assert_eq!(envelope["error"]["kind"], "conflict");
        let message = envelope["error"]["message"]
            .as_str()
            .expect("error envelope must carry a message");
        assert!(message.contains("office-ap"), "got: {message}");
        assert!(message.contains("Main-Office"), "got: {message}");
    }

    // A device whose wired and wireless interfaces share a name: `garage-pi`
    // matches two client records (a Raspberry Pi's wired and wireless
    // interfaces, MACs one bit apart in the last octet), but only the wired
    // interface ever shows up in a port table. That must resolve cleanly to
    // the one candidate that is actually on a port, not conflict.
    #[tokio::test]
    async fn ports_find_name_matches_two_clients_only_one_on_a_port_resolves_without_conflict() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "1", "mac": "aa:bb:cc:dd:ee:10", "name": "garage-pi", "ip": "192.0.2.5"},
                    {"_id": "2", "mac": "aa:bb:cc:dd:ee:11", "name": "garage-pi", "ip": "192.0.2.9"}
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW Pro XG 8 PoE",
                    "port_table": [
                        {"port_idx": 5, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": true}}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "find",
                "garage-pi",
                "-o",
                "json",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports find must resolve the single ported candidate, not conflict: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let items: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout was not valid JSON ({e}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
        let items = items
            .as_array()
            .expect("ports find must emit a bare JSON array");
        assert_eq!(items.len(), 1, "only the wired interface is on a port");
        assert_eq!(items[0]["port_idx"], 5);
        assert_eq!(items[0]["connected"], true);
    }

    // The other client record sharing the name never appears in any port
    // table at all: not "only the wireless interface", but no candidate on a
    // port whatsoever, so this must be not_found, not a conflict.
    #[tokio::test]
    async fn ports_find_name_matches_clients_none_on_a_port_is_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "1", "mac": "aa:bb:cc:dd:ee:10", "name": "lobby-display", "ip": "192.0.2.15"},
                    {"_id": "2", "mac": "aa:bb:cc:dd:ee:11", "name": "lobby-display", "ip": "192.0.2.16"}
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW Pro XG 8 PoE",
                    "port_table": [
                        {"port_idx": 1, "last_connection": {"mac": "11:22:33:44:55:66", "connected": true}}
                    ]
                }]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "find",
                "lobby-display",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert_eq!(
            output.status.code(),
            Some(4),
            "neither candidate is on any port, so this must exit 4 (not_found), got {:?}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last_line = stderr.trim_end().lines().last().unwrap_or("");
        let envelope: serde_json::Value =
            serde_json::from_str(last_line).expect("last stderr line must be valid JSON");
        assert_eq!(envelope["error"]["kind"], "not_found");
        let message = envelope["error"]["message"]
            .as_str()
            .expect("error envelope must carry a message");
        assert!(message.contains("lobby-display"), "got: {message}");
    }

    // `find`'s JSON output has always carried `connected`; only the text
    // table lacked it, leaving the connected-first sort order as the sole
    // (easy-to-miss) signal for which row is the device's *current* port,
    // a distinction that matters because this lookup feeds the destructive
    // `ports cycle`. Two distinctly-named single-port devices (rather than
    // one device with two ports) so each rendered row can be identified by
    // its device name, independent of the connected-first sort this test
    // does not itself re-verify (that is `ports_find_by_mac_sorts_connected_first_and_skips_client_lookup`'s job).
    #[tokio::test]
    async fn ports_find_text_output_shows_connected_column() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"mac": "aa:bb:cc:dd:ee:01", "name": "SwitchConnected",
                     "port_table": [
                        {"port_idx": 7, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": true}}
                     ]},
                    {"mac": "aa:bb:cc:dd:ee:02", "name": "SwitchStale",
                     "port_table": [
                        {"port_idx": 2, "last_connection": {"mac": "aa:bb:cc:dd:ee:10", "connected": false}}
                     ]}
                ]
            })))
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "find",
                "aa:bb:cc:dd:ee:10",
                "-o",
                "text",
            ])
            .output()
            .expect("failed to run the unifi binary");
        assert!(
            output.status.success(),
            "ports find failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

        let header = stdout
            .lines()
            .find(|l| l.contains("Device"))
            .expect("text output must have a header row containing \"Device\"");
        assert!(
            header.contains("Connected"),
            "find's header must carry a Connected column: {header}"
        );

        let connected_row = stdout
            .lines()
            .find(|l| l.contains("SwitchConnected"))
            .expect("expected a row for the connected device");
        let stale_row = stdout
            .lines()
            .find(|l| l.contains("SwitchStale"))
            .expect("expected a row for the stale device");

        assert!(
            connected_row.trim_end().ends_with("yes"),
            "the connected row's Connected column must render \"yes\": {connected_row}"
        );
        assert!(
            stale_row.trim_end().ends_with('-'),
            "the stale row's Connected column must render \"-\": {stale_row}"
        );
    }

    // --- Ports cycle (mutation orchestration) ---
    //
    // `power_cycle_port_sends_correct_command` (in `client_api` above) only
    // covers the client method's endpoint and body. Nothing exercised the
    // orchestration in `commands::ports::cycle` that decides *whether* to call
    // it at all, and that orchestration is the only place in this CLI that
    // cuts power to physical hardware. These four cases pin down the guard-rail
    // ordering (find_port -> check_cyclable -> confirm -> POST) as a tested
    // property rather than a code-reading exercise: the `.expect(0)` mounts on
    // decline/conflict/not-found assert, via wiremock's mount-drop
    // verification, that no HTTP write happens on any of the three
    // non-cycling paths.

    #[tokio::test]
    async fn ports_cycle_confirmed_cycles_the_port() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{
                        "port_idx": 5, "port_poe": true, "poe_mode": "auto",
                        "poe_enable": true
                    }]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .and(body_json(serde_json::json!({
                "cmd": "power-cycle",
                "mac": "aa:bb:cc:dd:06:43",
                "port_idx": 5
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let outcome =
            unifi_cli::commands::ports::cycle(&client, "aa:bb:cc:dd:06:43", 5, out_table(), |_| {
                Ok(true)
            })
            .await
            .unwrap();
        assert_eq!(outcome, unifi_cli::commands::ports::CycleOutcome::Cycled);
    }

    #[tokio::test]
    async fn ports_cycle_declined_never_posts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{
                        "port_idx": 5, "port_poe": true, "poe_mode": "auto",
                        "poe_enable": true
                    }]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(0)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let outcome =
            unifi_cli::commands::ports::cycle(&client, "aa:bb:cc:dd:06:43", 5, out_table(), |_| {
                Ok(false)
            })
            .await
            .unwrap();
        assert_eq!(outcome, unifi_cli::commands::ports::CycleOutcome::Declined);
    }

    #[tokio::test]
    async fn ports_cycle_non_poe_port_is_conflict_and_never_posts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-Lite-8",
                    "port_table": [{"port_idx": 9, "port_poe": false}]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(0)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        // The confirm callback returns `Ok(true)` deliberately: `check_cyclable`
        // must reject before `confirm` is ever consulted, so a callback that
        // would approve proves nothing about ordering unless it's wired to run
        // second.
        let err =
            unifi_cli::commands::ports::cycle(&client, "aa:bb:cc:dd:06:43", 9, out_table(), |_| {
                Ok(true)
            })
            .await
            .unwrap_err();
        let api_err = err
            .downcast_ref::<unifi_cli::api::ApiError>()
            .unwrap_or_else(|| {
                panic!("cycle must reject a non-PoE port as an ApiError, got {err}")
            });
        assert!(
            matches!(api_err, unifi_cli::api::ApiError::Conflict(_)),
            "expected Conflict, got {api_err:?}"
        );
    }

    // Mirrors `ports_cycle_non_poe_port_is_conflict_and_never_posts` for the
    // third guard rail: a port that is PoE-capable and not administratively
    // off, but that the controller reports as not currently delivering power
    // (poe_enable: false). This is the fixture from the live UCG-Max finding
    // that motivated the guard; see `check_cyclable` in
    // `src/commands/ports.rs` for what was actually observed.
    #[tokio::test]
    async fn ports_cycle_poe_enable_false_is_conflict_and_never_posts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:ee:fe", "name": "USW Lite 8 PoE",
                    "port_table": [{
                        "port_idx": 4, "port_poe": true, "poe_mode": "auto",
                        "poe_enable": false, "poe_power": 0.0, "up": false
                    }]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(0)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        // `Ok(true)` deliberately, same reasoning as the non-PoE case above:
        // proves `check_cyclable` rejects before `confirm` is ever consulted.
        let err =
            unifi_cli::commands::ports::cycle(&client, "aa:bb:cc:dd:ee:fe", 4, out_table(), |_| {
                Ok(true)
            })
            .await
            .unwrap_err();
        let api_err = err
            .downcast_ref::<unifi_cli::api::ApiError>()
            .unwrap_or_else(|| {
                panic!("cycle must reject a poe_enable=false port as an ApiError, got {err}")
            });
        assert!(
            matches!(api_err, unifi_cli::api::ApiError::Conflict(_)),
            "expected Conflict, got {api_err:?}"
        );
    }

    #[tokio::test]
    async fn ports_cycle_missing_port_is_not_found_and_never_posts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [{
                    "mac": "aa:bb:cc:dd:06:43", "name": "USW-24-PoE",
                    "port_table": [{"port_idx": 1, "port_poe": true}]
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/proxy/network/api/s/default/cmd/devmgr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": []
            })))
            .expect(0)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let err = unifi_cli::commands::ports::cycle(
            &client,
            "aa:bb:cc:dd:06:43",
            99,
            out_table(),
            |_| Ok(true),
        )
        .await
        .unwrap_err();
        let api_err = err
            .downcast_ref::<unifi_cli::api::ApiError>()
            .unwrap_or_else(|| {
                panic!("cycle must report a missing port as an ApiError, got {err}")
            });
        assert!(
            matches!(api_err, unifi_cli::api::ApiError::NotFound(_)),
            "expected NotFound, got {api_err:?}"
        );
    }

    #[tokio::test]
    async fn list_events_returns_stat_event_records() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"key": "EVT_AP_Connected", "msg": "AP connected", "subsystem": "wlan", "time": 200, "datetime": "2026-07-07T16:00:00Z"},
                    {"key": "EVT_SW_LostContact", "msg": "Switch lost contact", "subsystem": "lan", "time": 100, "datetime": "2026-07-07T15:00:00Z"}
                ]
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let events = client.list_events(10).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key.as_deref(), Some("EVT_AP_Connected"));
    }

    // UniFi Network 9+ (UniFi OS) removed the legacy stat/event route, which now
    // returns api.err.NotFound (404). list_events must fall back to rest/alarm and
    // return the most recent `limit` records.
    #[tokio::test]
    async fn list_events_falls_back_to_alarms_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.NotFound"},
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/rest/alarm"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"key": "EVT_GW_Older", "msg": "older", "time": 100, "datetime": "a"},
                    {"key": "EVT_GW_Newest", "msg": "newest", "time": 300, "datetime": "c"},
                    {"key": "EVT_GW_Middle", "msg": "middle", "time": 200, "datetime": "b"}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        let events = client.list_events(2).await.unwrap();
        // Most-recent-first, truncated to the requested limit.
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].msg.as_deref(), Some("newest"));
        assert_eq!(events[1].msg.as_deref(), Some("middle"));
    }

    #[tokio::test]
    async fn list_events_propagates_non_404_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.ServerError"},
                "data": []
            })))
            .mount(&server)
            .await;

        let client = mock_client(&server).await;
        assert!(client.list_events(10).await.is_err());
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

// --- An application the controller does not have ---
//
// UniFi OS does not 404 a request for an application that is not installed:
// it proxies the request to its own web UI, which answers 200 with an HTML
// page. Parsing that as JSON yields "error decoding response body", which
// names neither the endpoint nor the reason, so an agent cannot tell a
// missing application from a transport fault it should retry. These drive
// the real binary so the published envelope and exit code are observed.

mod unsupported_application {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const UNIFI_OS_SHELL: &str =
        "<!DOCTYPE html><html><head><title>UniFi OS</title></head><body></body></html>";

    fn envelope(stderr: &str) -> serde_json::Value {
        let last_line = stderr.trim_end().lines().last().unwrap_or("");
        serde_json::from_str(last_line)
            .unwrap_or_else(|e| panic!("last stderr line must be valid JSON ({e}): {last_line:?}"))
    }

    #[tokio::test]
    async fn protect_cameras_list_reports_unsupported_when_the_controller_serves_html() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/protect/integration/v1/cameras"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(UNIFI_OS_SHELL, "text/html; charset=utf-8"),
            )
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "protect",
                "cameras",
                "list",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert_eq!(
            output.status.code(),
            Some(4),
            "an absent application must exit 4, got {:?}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let envelope = envelope(&stderr);
        assert_eq!(
            envelope["error"]["kind"], "unsupported",
            "an absent application is not a transport fault: {stderr}"
        );
        let message = envelope["error"]["message"]
            .as_str()
            .expect("error envelope must carry a message");
        assert!(
            message.contains("/proxy/protect/integration/v1/cameras"),
            "the message must name the endpoint that answered: {message}"
        );
        assert!(
            message.contains("text/html"),
            "the message must name what it answered with: {message}"
        );
        assert!(
            message.contains("Protect"),
            "a Protect endpoint must say which application is missing: {message}"
        );
    }

    // The same proxy behaviour on a Network endpoint. Nothing about the check
    // is Protect-specific, but only the Protect message carries the hint, so
    // this pins that a Network endpoint reports the kind without inventing an
    // application that is in fact installed.
    #[tokio::test]
    async fn a_legacy_endpoint_answering_html_reports_unsupported_without_a_protect_hint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(UNIFI_OS_SHELL, "text/html; charset=utf-8"),
            )
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "list",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert_eq!(
            output.status.code(),
            Some(4),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let envelope = envelope(&stderr);
        assert_eq!(envelope["error"]["kind"], "unsupported");
        let message = envelope["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("/proxy/network/api/s/default/stat/device"),
            "the message must name the endpoint that answered: {message}"
        );
        assert!(
            !message.contains("Protect"),
            "a Network endpoint must not be blamed on Protect: {message}"
        );
    }

    // A body that decodes is an answer, whatever the header says it is. A
    // controller behind a proxy that rewrites or drops the content type is
    // still serving the endpoint, so reporting it as an application the
    // controller does not have would be worse than the error this replaced:
    // it would name a cause that is not merely vague but wrong.
    #[tokio::test]
    async fn json_under_a_non_json_content_type_still_decodes() {
        for content_type in ["text/plain", "application/octet-stream"] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/proxy/network/api/s/default/stat/device"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    r#"{"meta":{"rc":"ok"},"data":[{"mac":"aa:bb:cc:dd:ee:01","name":"SwitchA","port_table":[{"port_idx":1}]}]}"#,
                    content_type,
                ))
                .mount(&server)
                .await;

            let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
                .args([
                    "--host",
                    &server.uri(),
                    "--api-key",
                    "test-key",
                    "ports",
                    "list",
                ])
                .output()
                .expect("failed to run the unifi binary");

            assert!(
                output.status.success(),
                "a JSON body served as {content_type} must still decode: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let body: serde_json::Value = serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|e| panic!("stdout was not JSON ({e}) for {content_type}"));
            assert_eq!(body["items"][0]["device_name"], "SwitchA", "{content_type}");
        }
    }

    // A malformed body from an endpoint the controller does serve is a fault
    // in that controller, not a missing application, and must keep saying so.
    #[tokio::test]
    async fn a_broken_json_body_is_a_general_error_not_unsupported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/device"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(r#"{"meta":{"rc":"ok"},"data":["#, "application/json"),
            )
            .mount(&server)
            .await;

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "ports",
                "list",
            ])
            .output()
            .expect("failed to run the unifi binary");

        assert_eq!(
            output.status.code(),
            Some(1),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let envelope = envelope(&stderr);
        assert_eq!(
            envelope["error"]["kind"], "general_error",
            "the endpoint is served, the body is broken: {stderr}"
        );
        let message = envelope["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("/proxy/network/api/s/default/stat/device"),
            "the message must still name the endpoint: {message}"
        );
    }

    // The content-type check must not swallow a real JSON answer, including
    // one whose type carries a suffix or a charset.
    #[tokio::test]
    async fn a_json_content_type_still_decodes() {
        for content_type in [
            "application/json",
            "application/json; charset=utf-8",
            "application/vnd.api+json",
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/proxy/network/api/s/default/stat/device"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    r#"{"meta":{"rc":"ok"},"data":[{"mac":"aa:bb:cc:dd:ee:01","name":"SwitchA","port_table":[{"port_idx":1}]}]}"#,
                    content_type,
                ))
                .mount(&server)
                .await;

            let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
                .args([
                    "--host",
                    &server.uri(),
                    "--api-key",
                    "test-key",
                    "ports",
                    "list",
                ])
                .output()
                .expect("failed to run the unifi binary");

            assert!(
                output.status.success(),
                "{content_type} must decode as JSON: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let body: serde_json::Value = serde_json::from_slice(&output.stdout)
                .unwrap_or_else(|e| panic!("stdout was not JSON ({e}) for {content_type}"));
            assert_eq!(body["items"][0]["device_name"], "SwitchA", "{content_type}");
        }
    }
}

// --- An event log the firmware no longer serves ---
//
// UniFi Network 9 answers stat/event with 404 api.err.NotFound, and some
// builds do not serve the rest/alarm fallback either: they reject the
// resource with 400 api.err.InvalidObject, the same answer a nonsense
// resource name gets. Reporting that verbatim tells a caller its request was
// malformed and invites it to retry with other parameters, when in truth no
// request would work. These drive the real binary so the published envelope
// and exit code are observed.

mod events_surface_removed {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn envelope(stderr: &str) -> serde_json::Value {
        let last_line = stderr.trim_end().lines().last().unwrap_or("");
        serde_json::from_str(last_line)
            .unwrap_or_else(|e| panic!("last stderr line must be valid JSON ({e}): {last_line:?}"))
    }

    fn run_events_list(server: &MockServer) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                &server.uri(),
                "--api-key",
                "test-key",
                "events",
                "list",
            ])
            .output()
            .expect("failed to run the unifi binary")
    }

    async fn mount_stat_event_404(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/event"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.NotFound"},
                "data": []
            })))
            .mount(server)
            .await;
    }

    async fn mount_alarm(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/rest/alarm"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn both_endpoints_gone_reports_unsupported_not_a_rejected_request() {
        let server = MockServer::start().await;
        mount_stat_event_404(&server).await;
        mount_alarm(
            &server,
            ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.InvalidObject"},
                "data": []
            })),
        )
        .await;

        let output = run_events_list(&server);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(4),
            "an absent event surface must exit 4, not 5: {stderr}"
        );
        let envelope = envelope(&stderr);
        assert_eq!(
            envelope["error"]["kind"], "unsupported",
            "the request was fine, the endpoint is gone: {stderr}"
        );
        let message = envelope["error"]["message"]
            .as_str()
            .expect("error envelope must carry a message");
        assert!(
            message.contains("/proxy/network/api/s/default/stat/event"),
            "the message must name the endpoint the caller asked for: {message}"
        );
        assert!(
            message.contains("WebSocket"),
            "the message must say what event stream remains: {message}"
        );
        assert!(
            !message.contains("instead of JSON"),
            "this controller answered JSON, it just refused the resource: {message}"
        );
        assert!(
            !message.contains("Protect"),
            "a Network endpoint must not be blamed on Protect: {message}"
        );
    }

    // The fallback answering 404 means the same thing as its 400: the resource
    // is not there. Both arms must reach the same kind.
    #[tokio::test]
    async fn a_fallback_that_404s_reports_unsupported_too() {
        let server = MockServer::start().await;
        mount_stat_event_404(&server).await;
        mount_alarm(
            &server,
            ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.NotFound"},
                "data": []
            })),
        )
        .await;

        let output = run_events_list(&server);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(4), "stderr: {stderr}");
        assert_eq!(envelope(&stderr)["error"]["kind"], "unsupported");
    }

    // The negative control for the 400 arm. A 400 that is not the controller
    // disowning the resource is a genuinely rejected request, and must keep
    // saying so: turning every 400 into `unsupported` would hide real faults
    // behind "this controller cannot do that".
    #[tokio::test]
    async fn a_fallback_rejecting_the_request_stays_a_client_error() {
        let server = MockServer::start().await;
        mount_stat_event_404(&server).await;
        mount_alarm(
            &server,
            ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "meta": {"rc": "error", "msg": "api.err.InvalidPayload"},
                "data": []
            })),
        )
        .await;

        let output = run_events_list(&server);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(5),
            "a rejected request is not an absent endpoint: {stderr}"
        );
        let envelope = envelope(&stderr);
        assert_eq!(envelope["error"]["kind"], "client_error", "{stderr}");
        let message = envelope["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("api.err.InvalidPayload"),
            "the controller's own reason must survive: {message}"
        );
    }

    // The positive control. A controller that does serve the fallback must
    // still get its events, so the check above cannot be passing by refusing
    // everything.
    #[tokio::test]
    async fn a_working_fallback_still_returns_events() {
        let server = MockServer::start().await;
        mount_stat_event_404(&server).await;
        mount_alarm(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"key": "EVT_GW_Restarted", "msg": "Gateway restarted", "subsystem": "wan", "time": 300, "datetime": "2026-07-07T17:00:00Z"}
                ]
            })),
        )
        .await;

        let output = run_events_list(&server);
        assert!(
            output.status.success(),
            "a served fallback must succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");
        assert_eq!(body["items"][0]["key"], "EVT_GW_Restarted");
    }
}

// --- Ranking clients the controller published no counters for ---
//
// The live controller omits tx_bytes/rx_bytes for a substantial share of the
// clients it lists, so this is the common case rather than a corner of it.

mod clients_top_unknown_counters {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// One client that transferred a lot, one that reported a real zero, and one
    /// the controller published no counters for at all.
    async fn serving_a_mixed_population() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/proxy/network/api/s/default/stat/sta"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": {"rc": "ok"},
                "data": [
                    {"_id": "c1", "mac": "aa:bb:cc:dd:ee:01", "ip": "192.0.2.1",
                     "name": "Talker", "is_wired": true,
                     "tx_bytes": 500000, "rx_bytes": 600000},
                    {"_id": "c2", "mac": "aa:bb:cc:dd:ee:02", "ip": "192.0.2.2",
                     "name": "Silent", "is_wired": true},
                    {"_id": "c3", "mac": "aa:bb:cc:dd:ee:03", "ip": "192.0.2.3",
                     "name": "Measured Idle", "is_wired": true,
                     "tx_bytes": 0, "rx_bytes": 0}
                ]
            })))
            .mount(&server)
            .await;
        server
    }

    fn run(server_uri: &str, args: &[&str]) -> std::process::Output {
        let mut argv = vec!["--host", server_uri, "--api-key", "test-key"];
        argv.extend_from_slice(args);
        std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args(argv)
            .output()
            .expect("failed to run the unifi binary")
    }

    #[tokio::test]
    async fn a_client_with_no_reported_counters_is_not_drawn_as_having_moved_nothing() {
        let server = serving_a_mixed_population().await;
        let stdout = String::from_utf8_lossy(
            &run(
                &server.uri(),
                &["clients", "top", "--limit", "10", "-o", "text"],
            )
            .stdout,
        )
        .into_owned();

        let silent = stdout
            .lines()
            .find(|l| l.contains("Silent"))
            .unwrap_or_else(|| panic!("no row for the client without counters:\n{stdout}"));
        assert!(
            !silent.contains("0 B"),
            "counters the controller never sent are unknown, and `0 B` claims a \
             measurement nobody made: {silent}"
        );

        // The negative control: a client that really did report zero must keep
        // saying so, or the fix has simply hidden every zero.
        let idle = stdout
            .lines()
            .find(|l| l.contains("Measured Idle"))
            .unwrap_or_else(|| panic!("no row for the idle client:\n{stdout}"));
        assert!(
            idle.contains("0 B"),
            "a client that did report zero has been measured: {idle}"
        );
    }

    #[tokio::test]
    async fn an_unrankable_client_does_not_displace_one_that_can_be_ranked() {
        let server = serving_a_mixed_population().await;
        let stdout = String::from_utf8_lossy(
            &run(
                &server.uri(),
                &["clients", "top", "--limit", "10", "-o", "text"],
            )
            .stdout,
        )
        .into_owned();

        let row_of = |name: &str| {
            stdout
                .lines()
                .position(|l| l.contains(name))
                .unwrap_or_else(|| panic!("no row for {name}:\n{stdout}"))
        };
        assert!(
            row_of("Talker") < row_of("Measured Idle"),
            "a ranking by traffic still ranks what it can:\n{stdout}"
        );
        assert!(
            row_of("Measured Idle") < row_of("Silent"),
            "a client that cannot be ranked belongs after every client that \
             can, not interleaved with them:\n{stdout}"
        );
    }

    #[tokio::test]
    async fn the_total_is_not_a_number_when_neither_half_is() {
        let server = serving_a_mixed_population().await;
        let output = run(
            &server.uri(),
            &["clients", "top", "--limit", "10", "-o", "json"],
        );
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");
        let items = body.as_array().expect("clients top emits an array");
        let silent = items
            .iter()
            .find(|c| c["name"] == "Silent")
            .expect("the client without counters must still be listed");

        assert!(
            silent["tx_bytes"].is_null() && silent["rx_bytes"].is_null(),
            "{silent}"
        );
        assert!(
            silent["total_bytes"].is_null(),
            "a total of two unknowns is unknown, and `0` next to two nulls is a \
             contradiction in one object: {silent}"
        );
    }
}

// --- The Protect camera surface ---
//
// There is no Protect application to test against, so these drive the real
// binary against a stand-in that serves the payloads Protect's own API is
// documented to return. That proves what the tool does with a given payload,
// which is where every finding below lived; it does not prove which payloads
// Protect actually sends.

mod protect_cameras {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const CAMERAS_PATH: &str = "/proxy/protect/integration/v1/cameras";

    fn run(server_uri: &str, args: &[&str]) -> std::process::Output {
        let mut argv = vec!["--host", server_uri, "--api-key", "test-key"];
        argv.extend_from_slice(args);
        std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args(argv)
            .output()
            .expect("failed to run the unifi binary")
    }

    async fn serving(body: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(CAMERAS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn the_camera_list_uses_the_same_envelope_as_every_other_list() {
        let server = serving(serde_json::json!([
            {"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door", "state": "CONNECTED"}
        ]))
        .await;

        let output = run(&server.uri(), &["protect", "cameras", "list", "-o", "json"]);
        assert!(output.status.success());
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");

        assert_eq!(
            body["items"][0]["name"], "Front Door",
            "a consumer reading `items` must not have to special-case cameras: {body}"
        );
        assert_eq!(body["total"], 1, "{body}");
    }

    #[tokio::test]
    async fn a_camera_that_did_not_report_its_mic_is_not_reported_as_muted() {
        let server = serving(serde_json::json!([
            {"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door", "state": "CONNECTED"}
        ]))
        .await;

        let output = run(&server.uri(), &["protect", "cameras", "list", "-o", "json"]);
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");

        assert!(
            body["items"][0]["mic_enabled"].is_null(),
            "an unreported flag is unknown, and `false` cannot be told apart \
             from a camera that really has its mic off: {body}"
        );
    }

    #[tokio::test]
    async fn a_camera_that_reported_its_mic_still_says_so() {
        let server = serving(serde_json::json!([
            {"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door", "isMicEnabled": false}
        ]))
        .await;

        let output = run(&server.uri(), &["protect", "cameras", "list", "-o", "json"]);
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");

        assert_eq!(
            body["items"][0]["mic_enabled"], false,
            "a flag the camera did report must survive: {body}"
        );
    }

    #[tokio::test]
    async fn one_camera_matching_a_name_resolves_to_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(CAMERAS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door"},
                {"id": "bbbbbbbbbbbbbbbbbbbbbbbb", "name": "Back Door"}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{CAMERAS_PATH}/aaaaaaaaaaaaaaaaaaaaaaaa")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door"}),
            ))
            .mount(&server)
            .await;

        let output = run(
            &server.uri(),
            &["protect", "cameras", "show", "Front Door", "-o", "json"],
        );
        assert!(
            output.status.success(),
            "an unambiguous name must resolve: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");
        assert_eq!(body["id"], "aaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[tokio::test]
    async fn a_name_two_cameras_share_is_refused_rather_than_guessed() {
        let server = serving(serde_json::json!([
            {"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door"},
            {"id": "bbbbbbbbbbbbbbbbbbbbbbbb", "name": "Front Door"}
        ]))
        .await;

        let output = run(
            &server.uri(),
            &["protect", "cameras", "show", "Front Door", "-o", "json"],
        );

        assert!(
            !output.status.success(),
            "acting on whichever camera was listed first is a silent choice \
             the caller never made"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("aaaaaaaaaaaaaaaaaaaaaaaa")
                && stderr.contains("bbbbbbbbbbbbbbbbbbbbbbbb"),
            "both candidates must be named so the caller can pick one: {stderr}"
        );
    }

    #[tokio::test]
    async fn a_stream_the_controller_did_not_return_is_not_reported_as_created() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!(
                "{CAMERAS_PATH}/aaaaaaaaaaaaaaaaaaaaaaaa/rtsps-stream"
            )))
            // Asked for high and medium; only high comes back.
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "high": "rtsps://192.0.2.10:7441/abc"
            })))
            .mount(&server)
            .await;

        let output = run(
            &server.uri(),
            &[
                "protect",
                "rtsps",
                "create",
                "aaaaaaaaaaaaaaaaaaaaaaaa",
                "--quality",
                "high,medium",
                "-o",
                "json",
            ],
        );

        assert!(
            !output.status.success(),
            "a request carried out in part must not exit 0: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("medium"),
            "the quality that was not created must be named: {stderr}"
        );
    }

    #[tokio::test]
    async fn every_stream_asked_for_coming_back_is_a_plain_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!(
                "{CAMERAS_PATH}/aaaaaaaaaaaaaaaaaaaaaaaa/rtsps-stream"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "high": "rtsps://192.0.2.10:7441/abc",
                "medium": "rtsps://192.0.2.10:7441/def"
            })))
            .mount(&server)
            .await;

        let output = run(
            &server.uri(),
            &[
                "protect",
                "rtsps",
                "create",
                "aaaaaaaaaaaaaaaaaaaaaaaa",
                "--quality",
                "high,medium",
                "-o",
                "json",
            ],
        );

        assert!(
            output.status.success(),
            "nothing was missing: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("stdout was not JSON");
        assert_eq!(body["status"], "ok", "{body}");
        assert_eq!(body["not_created"].as_array().map(|a| a.len()), Some(0));

        // The schema's published output_fields must exactly match the keys this
        // command emits, the same property `ports show` holds itself to. The
        // fields that say a request was only half carried out are worth nothing
        // if an agent reading the contract cannot learn they exist.
        let schema_output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .arg("schema")
            .output()
            .expect("failed to run unifi schema");
        let schema: serde_json::Value = serde_json::from_slice(&schema_output.stdout)
            .expect("unifi schema must print valid JSON");
        let create = schema["commands"]
            .as_array()
            .expect("schema must have a commands array")
            .iter()
            .find(|c| c["name"] == "protect rtsps create")
            .expect("schema must publish a \"protect rtsps create\" command");
        let mut declared: Vec<&str> = create["output_fields"]
            .as_array()
            .expect("protect rtsps create must declare output_fields")
            .iter()
            .map(|f| f["name"].as_str().expect("output field must have a name"))
            .collect();
        declared.sort_unstable();
        let mut emitted: Vec<&str> = body
            .as_object()
            .expect("output must be a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        emitted.sort_unstable();
        assert_eq!(
            emitted, declared,
            "protect rtsps create output_fields in the schema must exactly match \
             the keys it emits"
        );
    }

    /// Stand in for the cookie-authenticated direct Protect API that `--full`
    /// uses: a login that hands back a TOKEN cookie, plus one camera.
    async fn serving_full(camera: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "TOKEN=stand-in; Path=/")
                    .set_body_json(serde_json::json!({})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{CAMERAS_PATH}/aaaaaaaaaaaaaaaaaaaaaaaa")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door"}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/proxy/protect/api/cameras/aaaaaaaaaaaaaaaaaaaaaaaa"))
            .respond_with(ResponseTemplate::new(200).set_body_json(camera))
            .mount(&server)
            .await;
        server
    }

    fn show_full(server_uri: &str) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                server_uri,
                "--api-key",
                "test-key",
                "--username",
                "stand-in",
                "--password",
                "stand-in",
                "-o",
                "text",
                "protect",
                "cameras",
                "show",
                "aaaaaaaaaaaaaaaaaaaaaaaa",
                "--full",
            ])
            .output()
            .expect("failed to run the unifi binary")
    }

    #[tokio::test]
    async fn a_storage_figure_the_camera_did_not_report_is_not_shown_as_zero() {
        let server = serving_full(serde_json::json!({
            "id": "aaaaaaaaaaaaaaaaaaaaaaaa",
            "name": "Front Door",
            "hqBytesPerDay": 12_000_000_000u64
        }))
        .await;

        let output = show_full(&server.uri());
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let storage = stdout
            .lines()
            .find(|l| l.contains("Storage:"))
            .unwrap_or_else(|| panic!("no storage line:\n{stdout}"));
        assert!(
            storage.contains("- LQ"),
            "a figure the camera never sent is unknown, not a claim that the \
             low-quality stream costs nothing: {storage}"
        );
    }

    #[tokio::test]
    async fn both_storage_figures_are_shown_when_the_camera_reports_them() {
        let server = serving_full(serde_json::json!({
            "id": "aaaaaaaaaaaaaaaaaaaaaaaa",
            "name": "Front Door",
            "hqBytesPerDay": 12_000_000_000u64,
            "lqBytesPerDay": 1_000_000_000u64
        }))
        .await;

        let stdout = String::from_utf8_lossy(&show_full(&server.uri()).stdout).into_owned();
        let storage = stdout
            .lines()
            .find(|l| l.contains("Storage:"))
            .unwrap_or_else(|| panic!("no storage line:\n{stdout}"));
        assert!(
            storage.contains("GB HQ") && storage.contains("MB LQ"),
            "reported figures must both render: {storage}"
        );
    }

    #[tokio::test]
    async fn a_camera_silent_about_recording_does_not_report_that_it_is_not() {
        let server = serving_full(serde_json::json!({
            "id": "aaaaaaaaaaaaaaaaaaaaaaaa",
            "name": "Front Door"
        }))
        .await;

        let stdout = String::from_utf8_lossy(&show_full(&server.uri()).stdout).into_owned();
        let recording = stdout
            .lines()
            .find(|l| l.contains("Recording:"))
            .unwrap_or_else(|| panic!("no recording line:\n{stdout}"));
        assert!(
            recording.contains('-') && !recording.contains("no"),
            "a camera that said nothing about recording has not said it is \
             idle: {recording}"
        );
    }

    fn show_full_json(server_uri: &str) -> serde_json::Value {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_unifi"))
            .args([
                "--host",
                server_uri,
                "--api-key",
                "test-key",
                "--username",
                "stand-in",
                "--password",
                "stand-in",
                "-o",
                "json",
                "protect",
                "cameras",
                "show",
                "aaaaaaaaaaaaaaaaaaaaaaaa",
                "--full",
            ])
            .output()
            .expect("failed to run the unifi binary");
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout was not JSON ({e}): {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }

    /// The JSON-only siblings of the flags above. They reach an agent rather
    /// than a person, where a bare `false` is taken at face value.
    #[tokio::test]
    async fn the_flags_only_json_carries_are_null_when_the_camera_omits_them() {
        let server = serving_full(serde_json::json!({
            "id": "aaaaaaaaaaaaaaaaaaaaaaaa",
            "name": "Front Door",
            "recordingSettings": {"mode": "always"},
            "channels": [{"id": 0, "name": "High"}]
        }))
        .await;

        let body = show_full_json(&server.uri());
        assert!(
            body["motion_detected"].is_null(),
            "a camera that did not report motion has not reported stillness: {body}"
        );
        assert!(
            body["recording_settings"]["motion_detection"].is_null(),
            "settings that never mentioned motion detection have not said it is \
             off: {body}"
        );
        assert!(
            body["channels"][0]["enabled"].is_null(),
            "a channel whose state was not reported is not a disabled channel: {body}"
        );
    }

    #[tokio::test]
    async fn the_flags_only_json_carries_survive_when_the_camera_reports_them() {
        let server = serving_full(serde_json::json!({
            "id": "aaaaaaaaaaaaaaaaaaaaaaaa",
            "name": "Front Door",
            "isMotionDetected": false,
            "recordingSettings": {"mode": "always", "enableMotionDetection": true},
            "channels": [{"id": 0, "name": "High", "enabled": false}]
        }))
        .await;

        let body = show_full_json(&server.uri());
        assert_eq!(body["motion_detected"], false, "{body}");
        assert_eq!(
            body["recording_settings"]["motion_detection"], true,
            "{body}"
        );
        assert_eq!(body["channels"][0]["enabled"], false, "{body}");
    }

    #[tokio::test]
    async fn a_camera_page_is_not_requested_when_the_id_is_already_an_id() {
        // Resolution short-circuits on a 24-char hex ID, so no listing is
        // served here at all: if the binary asked for one it would fail.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("{CAMERAS_PATH}/aaaaaaaaaaaaaaaaaaaaaaaa")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id": "aaaaaaaaaaaaaaaaaaaaaaaa", "name": "Front Door"}),
            ))
            .mount(&server)
            .await;

        let output = run(
            &server.uri(),
            &[
                "protect",
                "cameras",
                "show",
                "aaaaaaaaaaaaaaaaaaaaaaaa",
                "-o",
                "json",
            ],
        );
        assert!(
            output.status.success(),
            "an ID must resolve without a listing: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
