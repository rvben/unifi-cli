use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;

use super::types::*;

pub struct UnifiClient {
    http: reqwest::Client,
    base_url: String,
    site_id: Option<String>,
}

impl UnifiClient {
    pub fn new(host: &str, api_key: &str) -> Result<Self, ApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-API-KEY",
            HeaderValue::from_str(api_key).map_err(|e| ApiError::Other(e.to_string()))?,
        );

        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(ApiError::Http)?;

        let base_url = if host.starts_with("http") {
            host.trim_end_matches('/').to_string()
        } else {
            format!("https://{host}")
        };

        Ok(Self {
            http,
            base_url,
            site_id: None,
        })
    }

    fn error_for_status(status: u16, message: String) -> ApiError {
        match status {
            401 | 403 => ApiError::Auth(message),
            404 => ApiError::NotFound(message),
            _ => ApiError::Api { status, message },
        }
    }

    // Auto-discover site UUID from Integration API
    async fn ensure_site_id(&mut self) -> Result<&str, ApiError> {
        if self.site_id.is_none() {
            let resp: PaginatedResponse<Site> = self
                .get_integration("/proxy/network/integration/v1/sites")
                .await?;
            let site = resp.data.into_iter().next().ok_or_else(|| {
                ApiError::Other("No sites found — check that the API key has site access".into())
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
            return Err(Self::error_for_status(status, body));
        }
        Ok(resp.json().await?)
    }

    async fn get_legacy<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, ApiError> {
        let url = format!("{}/proxy/network/api/s/default{path}", self.base_url);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::error_for_status(status, body));
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
            return Err(Self::error_for_status(status, body));
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
            return Err(Self::error_for_status(status, body));
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
            return Err(Self::error_for_status(status, body));
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

    // System
    pub async fn get_health(&self) -> Result<Vec<HealthSubsystem>, ApiError> {
        self.get_legacy("/stat/health").await
    }

    pub async fn get_sysinfo(&self) -> Result<SysInfo, ApiError> {
        let mut data: Vec<SysInfo> = self.get_legacy("/stat/sysinfo").await?;
        data.pop()
            .ok_or_else(|| ApiError::Other("No sysinfo returned".into()))
    }
}
