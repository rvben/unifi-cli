use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;

use super::types::*;

pub fn error_for_status(status: u16, message: String) -> ApiError {
    match status {
        401 | 403 => ApiError::Auth(message),
        404 => ApiError::NotFound(message),
        _ => ApiError::Api { status, message },
    }
}

/// Valid RTSPS quality levels accepted by the Protect API.
const VALID_QUALITIES: &[&str] = &["high", "medium", "low", "package"];

/// Validate quality values against the Protect API allowlist.
pub fn validate_qualities(qualities: &[String]) -> Result<(), ApiError> {
    for q in qualities {
        if !VALID_QUALITIES.contains(&q.as_str()) {
            return Err(ApiError::Other(format!(
                "Invalid quality '{q}'. Valid values: {}",
                VALID_QUALITIES.join(", ")
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClientOptions {
    pub accept_invalid_certs: bool,
}

fn normalize_base_url(host: &str) -> Result<String, ApiError> {
    // A bare host (no scheme) defaults to https; an explicit scheme is kept and
    // validated after parsing so only http/https are accepted. Parsing also
    // rejects an empty host, since the url crate requires one for http/https.
    let candidate = if host.contains("://") {
        host.trim_end_matches('/').to_string()
    } else {
        format!("https://{}", host.trim_end_matches('/'))
    };

    let url = reqwest::Url::parse(&candidate)
        .map_err(|e| ApiError::Other(format!("Invalid controller host: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::Other(
            "Controller host must use http:// or https://".into(),
        ));
    }
    Ok(candidate)
}

pub struct UnifiClient {
    http: reqwest::Client,
    base_url: String,
    site_id: Option<String>,
}

impl UnifiClient {
    pub fn new(host: &str, api_key: &str) -> Result<Self, ApiError> {
        Self::new_with_options(host, api_key, ClientOptions::default())
    }

    pub fn new_with_options(
        host: &str,
        api_key: &str,
        options: ClientOptions,
    ) -> Result<Self, ApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-API-KEY",
            HeaderValue::from_str(api_key).map_err(|e| ApiError::Other(e.to_string()))?,
        );

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(options.accept_invalid_certs)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(ApiError::Http)?;

        let base_url = normalize_base_url(host)?;

        Ok(Self {
            http,
            base_url,
            site_id: None,
        })
    }

    pub fn clone_http(&self) -> reqwest::Client {
        self.http.clone()
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // Auto-discover site UUID from Integration API
    async fn ensure_site_id(&mut self) -> Result<&str, ApiError> {
        if self.site_id.is_none() {
            let resp: PaginatedResponse<Site> = self
                .get_integration("/proxy/network/integration/v1/sites")
                .await?;
            let site = resp.data.into_iter().next().ok_or_else(|| {
                ApiError::Other("No sites found. Check that the API key has site access".into())
            })?;
            self.site_id = Some(site.id);
        }
        Ok(self.site_id.as_deref().unwrap())
    }

    async fn get_integration<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }

    async fn get_legacy<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, ApiError> {
        let url = format!("{}/proxy/network/api/s/default{path}", self.base_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
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

    async fn post_legacy_cmd(
        &self,
        manager: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        let url = format!(
            "{}/proxy/network/api/s/default/cmd/{manager}",
            self.base_url
        );
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }

    async fn put_legacy<T: serde::Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}/proxy/network/api/s/default{path}", self.base_url);
        let resp = self.http.put(&url).json(body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }

    async fn post_legacy<T: serde::Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}/proxy/network/api/s/default{path}", self.base_url);
        let resp = self.http.post(&url).json(body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }

    // Paginate through all results from Integration API
    async fn paginate_all<T: DeserializeOwned>(&self, base_path: &str) -> Result<Vec<T>, ApiError> {
        let mut all = Vec::new();
        let mut offset = 0;
        let limit = 200;

        loop {
            let separator = if base_path.contains('?') { '&' } else { '?' };
            let path = format!("{base_path}{separator}offset={offset}&limit={limit}");
            let resp: PaginatedResponse<T> = self.get_integration(&path).await?;
            let count = resp.data.len();
            all.extend(resp.data);

            if all.len() >= resp.total_count || count < limit {
                break;
            }
            offset += count;
        }

        Ok(all)
    }

    // --- Public API ---

    // Clients
    pub async fn list_clients(&mut self) -> Result<Vec<Client>, ApiError> {
        let site_id = self.ensure_site_id().await?.to_string();
        self.paginate_all(&format!(
            "/proxy/network/integration/v1/sites/{site_id}/clients"
        ))
        .await
    }

    pub async fn get_client_detail(&self, mac: &str) -> Result<LegacyClient, ApiError> {
        let normalized = normalize_mac(mac);
        let clients: Vec<LegacyClient> = self.get_legacy("/stat/sta").await?;
        clients
            .into_iter()
            .find(|c| {
                c.mac
                    .as_deref()
                    .is_some_and(|m| normalize_mac(m) == normalized)
            })
            .ok_or_else(|| ApiError::NotFound(format!("Client with MAC {mac}")))
    }

    pub async fn set_fixed_ip(
        &self,
        mac: &str,
        ip: &str,
        name: Option<&str>,
    ) -> Result<(), ApiError> {
        let normalized = normalize_mac(mac);

        // Find client _id from legacy stat/sta
        let clients: Vec<LegacyClient> = self.get_legacy("/stat/sta").await?;
        let client = clients
            .into_iter()
            .find(|c| {
                c.mac
                    .as_deref()
                    .is_some_and(|m| normalize_mac(m) == normalized)
            })
            .ok_or_else(|| ApiError::NotFound(format!("Client with MAC {mac}")))?;

        let mut payload = serde_json::json!({
            "mac": format_mac(&normalized),
            "use_fixedip": true,
            "fixed_ip": ip,
        });

        if let Some(n) = name {
            payload["name"] = serde_json::Value::String(n.to_string());
            payload["noted"] = serde_json::Value::Bool(true);
        }

        let path = format!("/rest/user/{}", client.id);
        match self.put_legacy(&path, &payload).await {
            Ok(_) => Ok(()),
            Err(ApiError::NotFound(_)) => {
                // Client doesn't have a user entry yet, create one
                self.post_legacy("/rest/user", &payload).await?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn block_client(&self, mac: &str) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        self.post_legacy_cmd(
            "stamgr",
            serde_json::json!({"cmd": "block-sta", "mac": formatted}),
        )
        .await?;
        Ok(())
    }

    pub async fn unblock_client(&self, mac: &str) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        self.post_legacy_cmd(
            "stamgr",
            serde_json::json!({"cmd": "unblock-sta", "mac": formatted}),
        )
        .await?;
        Ok(())
    }

    pub async fn kick_client(&self, mac: &str) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        self.post_legacy_cmd(
            "stamgr",
            serde_json::json!({"cmd": "kick-sta", "mac": formatted}),
        )
        .await?;
        Ok(())
    }

    // Devices
    pub async fn list_devices(&mut self) -> Result<Vec<Device>, ApiError> {
        let site_id = self.ensure_site_id().await?.to_string();
        self.paginate_all(&format!(
            "/proxy/network/integration/v1/sites/{site_id}/devices"
        ))
        .await
    }

    pub async fn get_device_detail(&self, mac: &str) -> Result<LegacyDevice, ApiError> {
        let normalized = normalize_mac(mac);
        let devices: Vec<LegacyDevice> = self.get_legacy("/stat/device").await?;
        devices
            .into_iter()
            .find(|d| {
                d.mac
                    .as_deref()
                    .is_some_and(|m| normalize_mac(m) == normalized)
            })
            .ok_or_else(|| ApiError::NotFound(format!("Device with MAC {mac}")))
    }

    pub async fn restart_device(&self, mac: &str) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        self.post_legacy_cmd(
            "devmgr",
            serde_json::json!({"cmd": "restart", "mac": formatted}),
        )
        .await?;
        Ok(())
    }

    /// Power-cycle a single PoE port. `mac` is the **switch's** MAC, not the
    /// attached device's.
    pub async fn power_cycle_port(&self, mac: &str, port_idx: u32) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        self.post_legacy_cmd(
            "devmgr",
            serde_json::json!({"cmd": "power-cycle", "mac": formatted, "port_idx": port_idx}),
        )
        .await?;
        Ok(())
    }

    pub async fn upgrade_device(&self, mac: &str) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        self.post_legacy_cmd(
            "devmgr",
            serde_json::json!({"cmd": "upgrade", "mac": formatted}),
        )
        .await?;
        Ok(())
    }

    pub async fn locate_device(&self, mac: &str, enable: bool) -> Result<(), ApiError> {
        let formatted = format_mac(&normalize_mac(mac));
        let cmd = if enable { "set-locate" } else { "unset-locate" };
        self.post_legacy_cmd("devmgr", serde_json::json!({"cmd": cmd, "mac": formatted}))
            .await?;
        Ok(())
    }

    // Networks
    pub async fn list_networks(&mut self) -> Result<Vec<Network>, ApiError> {
        let site_id = self.ensure_site_id().await?.to_string();
        self.paginate_all(&format!(
            "/proxy/network/integration/v1/sites/{site_id}/networks"
        ))
        .await
    }

    // Events
    //
    // Legacy `stat/event` was removed in UniFi Network 9+ (UniFi OS) and now
    // returns api.err.NotFound (404). On those controllers the surviving REST
    // surface for notable events is `rest/alarm`, whose records share this
    // `Event` shape, so fall back to it. (The full live event stream on newer
    // controllers is only exposed over the events WebSocket, which this REST
    // client does not consume.)
    pub async fn list_events(&self, limit: usize) -> Result<Vec<Event>, ApiError> {
        match self
            .get_legacy::<Event>(&format!("/stat/event?_limit={limit}"))
            .await
        {
            Ok(events) => Ok(events),
            Err(ApiError::NotFound(_)) => {
                let mut alarms: Vec<Event> = self.get_legacy("/rest/alarm").await?;
                // `rest/alarm` is neither time-ordered nor limited server-side;
                // present the most recent `limit` records to match the
                // semantics `stat/event?_limit=` provided on older controllers.
                alarms.sort_by_key(|e| std::cmp::Reverse(e.time));
                alarms.truncate(limit);
                Ok(alarms)
            }
            Err(e) => Err(e),
        }
    }

    // Port table for a specific device
    pub async fn get_device_ports(&self, mac: &str) -> Result<DeviceWithPorts, ApiError> {
        let normalized = normalize_mac(mac);
        let devices: Vec<DeviceWithPorts> = self.get_legacy("/stat/device").await?;
        devices
            .into_iter()
            .find(|d| {
                d.mac
                    .as_deref()
                    .is_some_and(|m| normalize_mac(m) == normalized)
            })
            .ok_or_else(|| ApiError::NotFound(format!("Device with MAC {mac}")))
    }

    /// Every device that reports a port table, in one request. `/stat/device`
    /// already returns all devices with their port tables, so the unfiltered
    /// listing costs no more than the filtered one.
    pub async fn list_all_device_ports(&self) -> Result<Vec<DeviceWithPorts>, ApiError> {
        self.get_legacy("/stat/device").await
    }

    // All clients with bandwidth data (legacy endpoint for richer stats)
    pub async fn list_clients_legacy(&self) -> Result<Vec<LegacyClient>, ApiError> {
        self.get_legacy("/stat/sta").await
    }

    // All devices with full detail (legacy endpoint)
    pub async fn get_legacy_devices(&self) -> Result<Vec<LegacyDevice>, ApiError> {
        self.get_legacy("/stat/device").await
    }

    // --- Protect API ---

    /// List all cameras from the Protect Integration API.
    pub async fn list_protect_cameras(&self) -> Result<Vec<ProtectCamera>, ApiError> {
        let resp: Vec<ProtectCamera> = self
            .get_integration("/proxy/protect/integration/v1/cameras")
            .await?;
        Ok(resp)
    }

    /// Get a single camera by ID from the Protect Integration API.
    pub async fn get_protect_camera(&self, id: &str) -> Result<ProtectCamera, ApiError> {
        self.get_integration(&format!("/proxy/protect/integration/v1/cameras/{id}"))
            .await
    }

    /// Get existing RTSPS stream URLs for a camera.
    pub async fn get_rtsps_streams(&self, camera_id: &str) -> Result<RtspsStreams, ApiError> {
        self.get_integration(&format!(
            "/proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream"
        ))
        .await
    }

    /// Create new RTSPS streams for a camera at the specified quality levels.
    pub async fn create_rtsps_streams(
        &self,
        camera_id: &str,
        qualities: &[String],
    ) -> Result<RtspsStreams, ApiError> {
        validate_qualities(qualities)?;
        let url = format!(
            "{}/proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream",
            self.base_url
        );
        let body = serde_json::json!({ "qualities": qualities });
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }

    /// Delete RTSPS streams for a camera at the specified quality levels.
    pub async fn delete_rtsps_streams(
        &self,
        camera_id: &str,
        qualities: &[String],
    ) -> Result<(), ApiError> {
        validate_qualities(qualities)?;
        let query: String = qualities
            .iter()
            .map(|q| format!("qualities={q}"))
            .collect::<Vec<_>>()
            .join("&");
        let url = format!(
            "{}/proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream?{query}",
            self.base_url
        );
        let resp = self.http.delete(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(())
    }

    /// Resolve a camera identifier (ID or name) to a camera ID.
    /// If the input is a 24-char hex string, treats it as an ID.
    /// Otherwise, searches by name (case-insensitive).
    pub async fn resolve_camera_id(&self, id_or_name: &str) -> Result<String, ApiError> {
        // If it looks like a Protect camera ID (24 hex chars), use it directly
        if id_or_name.len() == 24 && id_or_name.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(id_or_name.to_string());
        }
        // Otherwise, search by name
        let cameras = self.list_protect_cameras().await?;
        let needle = id_or_name.to_lowercase();
        cameras
            .into_iter()
            .find(|c| {
                c.name
                    .as_deref()
                    .is_some_and(|n| n.trim().to_lowercase() == needle)
            })
            .map(|c| c.id)
            .ok_or_else(|| ApiError::NotFound(format!("Camera '{id_or_name}'")))
    }

    // System
    pub async fn get_health(&self) -> Result<Vec<HealthSubsystem>, ApiError> {
        self.get_legacy("/stat/health").await
    }

    pub async fn get_sysinfo(&self) -> Result<SysInfo, ApiError> {
        let mut data: Vec<SysInfo> = self.get_legacy("/stat/sysinfo").await?;
        data.pop()
            .ok_or_else(|| ApiError::Other("No sysinfo returned".into()))
    }

    pub async fn get_host_system(&self) -> Result<HostSystem, ApiError> {
        let url = format!("{}/api/system", self.base_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }
}

/// Session-based client for the direct Protect API.
///
/// Uses username/password login to get a session cookie, then hits
/// `/proxy/protect/api/` endpoints which return full camera objects
/// (IP, firmware, channels, stats, WiFi, ISP settings, etc).
pub struct ProtectSession {
    http: reqwest::Client,
    base_url: String,
    token: String,
    csrf_token: Option<String>,
}

impl ProtectSession {
    /// Login to UniFi OS and return a session with cookie auth.
    pub async fn login(host: &str, username: &str, password: &str) -> Result<Self, ApiError> {
        Self::login_with_options(host, username, password, ClientOptions::default()).await
    }

    pub async fn login_with_options(
        host: &str,
        username: &str,
        password: &str,
        options: ClientOptions,
    ) -> Result<Self, ApiError> {
        let base_url = normalize_base_url(host)?;

        // Don't use cookie_provider: the `partitioned` cookie attribute
        // isn't handled by reqwest's jar. We extract the token manually.
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(options.accept_invalid_certs)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(ApiError::Http)?;

        let url = format!("{base_url}/api/auth/login");
        let body = serde_json::json!({
            "username": username,
            "password": password,
        });

        let resp = http.post(&url).json(&body).send().await?;
        let status = resp.status().as_u16();

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }

        // Extract TOKEN from Set-Cookie header
        let token = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .find_map(|v| {
                let s = v.to_str().ok()?;
                if s.starts_with("TOKEN=") {
                    s.split(';')
                        .next()?
                        .strip_prefix("TOKEN=")
                        .map(String::from)
                } else {
                    None
                }
            })
            .ok_or_else(|| ApiError::Auth("Login succeeded but no TOKEN cookie returned".into()))?;

        // Extract CSRF token from response headers
        let csrf_token = resp
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        // Consume body to finalize the response
        let _ = resp.text().await;

        Ok(Self {
            http,
            base_url,
            token,
            csrf_token,
        })
    }

    /// GET from the direct Protect API (cookie-authenticated).
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let url = format!("{}/proxy/protect/api{path}", self.base_url);
        let mut req = self
            .http
            .get(&url)
            .header("cookie", format!("TOKEN={}", self.token));
        if let Some(ref token) = self.csrf_token {
            req = req.header("x-csrf-token", token);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        let bytes = resp.bytes().await.map_err(ApiError::Http)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| ApiError::Other(format!("JSON parse error: {e}")))
    }

    /// List all cameras from the direct Protect API (full objects).
    pub async fn list_cameras_full(&self) -> Result<Vec<ProtectCameraFull>, ApiError> {
        self.get("/cameras").await
    }

    /// Get a single camera by ID (full object).
    pub async fn get_camera_full(&self, id: &str) -> Result<ProtectCameraFull, ApiError> {
        self.get(&format!("/cameras/{id}")).await
    }
}
