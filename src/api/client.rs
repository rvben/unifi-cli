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

/// True when a legacy API error means the endpoint itself is not there.
///
/// UniFi Network answers an unknown `stat/` path with 404 `api.err.NotFound`
/// and an unknown `rest/` resource with 400 `api.err.InvalidObject`, verified
/// against a UniFi OS 9 controller by control: a nonsense resource and
/// `rest/alarm` produce byte-identical 400s while `rest/networkconf` returns
/// records.
///
/// A 400 alone does not carry that meaning, since the same status reports a
/// genuinely malformed request, so the marker string is required too and this
/// is only consulted for a request that carries no caller-supplied body or
/// parameters that could be the invalid object.
fn is_absent_legacy_endpoint(err: &ApiError) -> bool {
    match err {
        ApiError::NotFound(_) => true,
        ApiError::Api {
            status: 400,
            message,
        } => message.contains("api.err.InvalidObject"),
        _ => false,
    }
}

/// Decode a successful response as JSON, naming what answered when it is not.
///
/// UniFi OS answers a request for an application the controller does not have
/// by proxying it to the web UI, which returns 200 with an HTML page. Decoding
/// that as JSON produces "error decoding response body", which names neither
/// the endpoint nor the reason, so the caller cannot tell a missing application
/// from a transport fault worth retrying.
///
/// The body is decoded first and the content type only chooses the error for a
/// body that did not decode: a controller or proxy that serves JSON under
/// `text/plain` or under no content type at all is still answering the request,
/// so it must not be reported as an application that is not there.
async fn json_or_unsupported<T: DeserializeOwned>(
    resp: reqwest::Response,
    endpoint: &str,
) -> Result<T, ApiError> {
    let raw = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.bytes().await?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(value),
        // A body that claims to be JSON and is not is a fault in a controller
        // that does serve this endpoint, which is a different thing entirely.
        Err(e) if raw.to_ascii_lowercase().contains("json") => Err(ApiError::Other(format!(
            "Failed to decode the response from {endpoint}: {e}"
        ))),
        Err(_) => {
            let content_type = match raw.split(';').next().map(str::trim) {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => "no content type".to_string(),
            };
            Err(ApiError::Unsupported {
                endpoint: endpoint.to_string(),
                reason: UnsupportedReason::NotJson { content_type },
            })
        }
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
        json_or_unsupported(resp, path).await
    }

    async fn get_legacy<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, ApiError> {
        let endpoint = format!("/proxy/network/api/s/default{path}");
        let url = format!("{}{endpoint}", self.base_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        let legacy: LegacyResponse<T> = json_or_unsupported(resp, &endpoint).await?;
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
        let endpoint = format!("/proxy/network/api/s/default/cmd/{manager}");
        let url = format!("{}{endpoint}", self.base_url);
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        json_or_unsupported(resp, &endpoint).await
    }

    async fn put_legacy<T: serde::Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<serde_json::Value, ApiError> {
        let endpoint = format!("/proxy/network/api/s/default{path}");
        let url = format!("{}{endpoint}", self.base_url);
        let resp = self.http.put(&url).json(body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        json_or_unsupported(resp, &endpoint).await
    }

    async fn post_legacy<T: serde::Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<serde_json::Value, ApiError> {
        let endpoint = format!("/proxy/network/api/s/default{path}");
        let url = format!("{}{endpoint}", self.base_url);
        let resp = self.http.post(&url).json(body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        json_or_unsupported(resp, &endpoint).await
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

    pub async fn list_firewall_rules(&self) -> Result<Vec<FirewallRule>, ApiError> {
        self.get_legacy("/rest/firewallrule").await
    }

    pub async fn list_firewall_groups(&self) -> Result<Vec<FirewallGroup>, ApiError> {
        self.get_legacy("/rest/firewallgroup").await
    }

    // Events
    //
    // Legacy `stat/event` was removed in UniFi Network 9+ (UniFi OS) and now
    // returns api.err.NotFound (404). On those controllers the surviving REST
    // surface for notable events is `rest/alarm`, whose records share this
    // `Event` shape, so fall back to it. (The full live event stream on newer
    // controllers is only exposed over the events WebSocket, which this REST
    // client does not consume.)
    //
    // Some UniFi Network 9 builds serve neither. Reporting the fallback's own
    // rejection verbatim would tell a caller that its request was malformed and
    // invite it to retry with different parameters, when in truth the whole
    // event surface is gone and nothing it can send would work.
    pub async fn list_events(&self, limit: usize) -> Result<Vec<Event>, ApiError> {
        let events_path = format!("/stat/event?_limit={limit}");
        match self.get_legacy::<Event>(&events_path).await {
            Ok(events) => Ok(events),
            Err(ApiError::NotFound(_)) => match self.get_legacy::<Event>("/rest/alarm").await {
                // `rest/alarm` is neither time-ordered nor limited server-side;
                // present the most recent `limit` records to match the
                // semantics `stat/event?_limit=` provided on older controllers.
                Ok(mut alarms) => {
                    alarms.sort_by_key(|e| std::cmp::Reverse(e.time));
                    alarms.truncate(limit);
                    Ok(alarms)
                }
                Err(e) if is_absent_legacy_endpoint(&e) => Err(ApiError::Unsupported {
                    endpoint: format!("/proxy/network/api/s/default{events_path}"),
                    reason: UnsupportedReason::Removed,
                }),
                Err(e) => Err(e),
            },
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
        let endpoint = format!("/proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream");
        let url = format!("{}{endpoint}", self.base_url);
        let body = serde_json::json!({ "qualities": qualities });
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(error_for_status(status, body));
        }
        json_or_unsupported(resp, &endpoint).await
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
    ///
    /// A name that fits more than one camera is an error rather than a choice
    /// made silently: the caller may be about to delete that camera's streams,
    /// and picking whichever the controller happened to list first would act on
    /// a different camera than the one the user was asked to confirm.
    pub async fn resolve_camera_id(&self, id_or_name: &str) -> Result<String, ApiError> {
        // If it looks like a Protect camera ID (24 hex chars), use it directly
        if id_or_name.len() == 24 && id_or_name.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(id_or_name.to_string());
        }
        // Otherwise, search by name
        let cameras = self.list_protect_cameras().await?;
        let needle = id_or_name.to_lowercase();
        let mut matches: Vec<ProtectCamera> = cameras
            .into_iter()
            .filter(|c| {
                c.name
                    .as_deref()
                    .is_some_and(|n| n.trim().to_lowercase() == needle)
            })
            .collect();

        match matches.len() {
            0 => Err(ApiError::NotFound(format!("Camera '{id_or_name}'"))),
            1 => Ok(matches.pop().expect("checked len == 1 above").id),
            _ => {
                let list = matches
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(ApiError::Conflict(format!(
                    "'{id_or_name}' matches {} cameras: {list}. Use the ID.",
                    matches.len()
                )))
            }
        }
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
        json_or_unsupported(resp, "/api/system").await
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
        let endpoint = format!("/proxy/protect/api{path}");
        let url = format!("{}{endpoint}", self.base_url);
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
        json_or_unsupported(resp, &endpoint).await
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
