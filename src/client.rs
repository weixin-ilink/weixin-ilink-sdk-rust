use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngExt;
use url::Url;

use crate::error::{Error, HttpError, Result, SESSION_EXPIRED_ERRCODE};
use crate::http::HttpClient;
use crate::types::*;

pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const DEFAULT_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
const DEFAULT_LONG_POLL_TIMEOUT: Duration = Duration::from_secs(35);
const DEFAULT_API_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_CONFIG_TIMEOUT: Duration = Duration::from_secs(10);

/// Core client for the iLink Bot API.
pub struct ILinkClient<H: HttpClient = reqwest::Client> {
    http: H,
    base_url: Url,
    cdn_base_url: Url,
    token: Option<String>,
    route_tag: Option<String>,
    channel_version: String,
}

/// Builder for `ILinkClient`.
pub struct ILinkClientBuilder<H: HttpClient = reqwest::Client> {
    http: Option<H>,
    base_url: Option<String>,
    cdn_base_url: Option<String>,
    token: Option<String>,
    route_tag: Option<String>,
    channel_version: Option<String>,
}

impl<H: HttpClient> Default for ILinkClientBuilder<H> {
    fn default() -> Self {
        Self {
            http: None,
            base_url: None,
            cdn_base_url: None,
            token: None,
            route_tag: None,
            channel_version: None,
        }
    }
}

impl<H: HttpClient> ILinkClientBuilder<H> {
    pub fn http_client(mut self, http: H) -> Self {
        self.http = Some(http);
        self
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn cdn_base_url(mut self, url: impl Into<String>) -> Self {
        self.cdn_base_url = Some(url.into());
        self
    }

    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn route_tag(mut self, tag: impl Into<String>) -> Self {
        self.route_tag = Some(tag.into());
        self
    }

    pub fn channel_version(mut self, version: impl Into<String>) -> Self {
        self.channel_version = Some(version.into());
        self
    }
}

impl ILinkClientBuilder<reqwest::Client> {
    pub fn build(self) -> ILinkClient<reqwest::Client> {
        let http = self.http.unwrap_or_else(crate::http::default_http_client);
        ILinkClient::from_builder(self.base_url, self.cdn_base_url, self.token, self.route_tag, self.channel_version, http)
    }
}

impl<H: HttpClient> ILinkClientBuilder<H> {
    pub fn build_with(self) -> ILinkClient<H>
    where
        H: Default,
    {
        let http = self.http.unwrap_or_default();
        ILinkClient::from_builder(self.base_url, self.cdn_base_url, self.token, self.route_tag, self.channel_version, http)
    }

    pub fn build_with_http(self) -> std::result::Result<ILinkClient<H>, &'static str> {
        let http = self.http.ok_or("http_client is required for custom HttpClient")?;
        Ok(ILinkClient::from_builder(self.base_url, self.cdn_base_url, self.token, self.route_tag, self.channel_version, http))
    }
}

impl<H: HttpClient> ILinkClient<H> {
    fn from_builder(
        base_url: Option<String>,
        cdn_base_url: Option<String>,
        token: Option<String>,
        route_tag: Option<String>,
        channel_version: Option<String>,
        http: H,
    ) -> Self {
        let base_url = base_url
            .and_then(|u| Url::parse(&u).ok())
            .unwrap_or_else(|| Url::parse(DEFAULT_BASE_URL).unwrap());
        let cdn_base_url = cdn_base_url
            .and_then(|u| Url::parse(&u).ok())
            .unwrap_or_else(|| Url::parse(DEFAULT_CDN_BASE_URL).unwrap());
        Self {
            http,
            base_url,
            cdn_base_url,
            token,
            route_tag,
            channel_version: channel_version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
        }
    }

    pub fn builder() -> ILinkClientBuilder<H> {
        ILinkClientBuilder::default()
    }

    pub fn http(&self) -> &H {
        &self.http
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn cdn_base_url(&self) -> &Url {
        &self.cdn_base_url
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn set_token(&mut self, token: impl Into<String>) {
        self.token = Some(token.into());
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn base_info(&self) -> BaseInfo {
        BaseInfo {
            channel_version: Some(self.channel_version.clone()),
        }
    }

    fn random_wechat_uin() -> String {
        let uint32: u32 = rand::rng().random();
        BASE64.encode(uint32.to_string().as_bytes())
    }

    fn build_request(
        &self,
        endpoint: &str,
        body: Vec<u8>,
        timeout: Duration,
    ) -> http::Request<Vec<u8>> {
        let url = format!("{}{}", self.base_url.as_str().trim_end_matches('/'), if endpoint.starts_with('/') { endpoint.to_string() } else { format!("/{endpoint}") });

        let mut builder = http::Request::builder()
            .method(http::Method::POST)
            .uri(&url)
            .header("Content-Type", "application/json")
            .header("Content-Length", body.len().to_string())
            .header("AuthorizationType", "ilink_bot_token")
            .header("X-WECHAT-UIN", Self::random_wechat_uin())
            .header("X-Timeout-Ms", timeout.as_millis().to_string());

        if let Some(token) = &self.token {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(tag) = &self.route_tag {
            builder = builder.header("SKRouteTag", tag.as_str());
        }

        builder.body(body).expect("failed to build request")
    }

    async fn api_post<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        req: &Req,
        timeout: Duration,
    ) -> Result<Resp> {
        let body = serde_json::to_vec(req)?;
        let request = self.build_request(endpoint, body, timeout);

        tracing::debug!(endpoint, "API POST");

        let response = self.http.execute(request).await.map_err(Error::Http)?;
        let status = response.status();
        let body = response.into_body();

        if !status.is_success() {
            let text = String::from_utf8_lossy(&body).to_string();
            return Err(Error::Http(HttpError::Status {
                status: status.as_u16(),
                body: text,
            }));
        }

        let resp: Resp = serde_json::from_slice(&body)?;
        Ok(resp)
    }

    // ── Public API ──────────────────────────────────────────────────────

    /// Long-poll for new messages.
    ///
    /// Returns an empty response (ret=0, no msgs) on client timeout, which is
    /// normal for long-poll — the caller should simply retry.
    pub async fn get_updates(
        &self,
        get_updates_buf: &str,
        timeout: Option<Duration>,
    ) -> Result<GetUpdatesResponse> {
        let timeout = timeout.unwrap_or(DEFAULT_LONG_POLL_TIMEOUT);
        let req = GetUpdatesRequest {
            get_updates_buf: get_updates_buf.to_string(),
            base_info: self.base_info(),
        };

        match self.api_post("ilink/bot/getupdates", &req, timeout).await {
            Ok(resp) => Ok(resp),
            Err(Error::Http(HttpError::Timeout(_))) => {
                tracing::debug!("getUpdates client timeout, returning empty response");
                Ok(GetUpdatesResponse {
                    ret: Some(0),
                    errcode: None,
                    errmsg: None,
                    msgs: Some(Vec::new()),
                    get_updates_buf: Some(get_updates_buf.to_string()),
                    longpolling_timeout_ms: None,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Send a message downstream.
    pub async fn send_message(&self, req: &SendMessageRequest) -> Result<()> {
        let _: serde_json::Value = self
            .api_post("ilink/bot/sendmessage", req, DEFAULT_API_TIMEOUT)
            .await?;
        Ok(())
    }

    /// Get a pre-signed CDN upload URL.
    pub async fn get_upload_url(&self, req: &GetUploadUrlRequest) -> Result<GetUploadUrlResponse> {
        self.api_post("ilink/bot/getuploadurl", req, DEFAULT_API_TIMEOUT)
            .await
    }

    /// Fetch bot config (typing_ticket, etc.) for a user.
    pub async fn get_config(
        &self,
        user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResponse> {
        let req = GetConfigRequest {
            ilink_user_id: user_id.to_string(),
            context_token: context_token.map(String::from),
            base_info: Some(self.base_info()),
        };
        self.api_post("ilink/bot/getconfig", &req, DEFAULT_CONFIG_TIMEOUT)
            .await
    }

    /// Send a typing indicator.
    pub async fn send_typing(
        &self,
        user_id: &str,
        typing_ticket: &str,
        status: TypingStatus,
    ) -> Result<()> {
        let req = SendTypingRequest {
            ilink_user_id: user_id.to_string(),
            typing_ticket: typing_ticket.to_string(),
            status: status.into(),
            base_info: Some(self.base_info()),
        };
        let _: serde_json::Value = self
            .api_post("ilink/bot/sendtyping", &req, DEFAULT_CONFIG_TIMEOUT)
            .await?;
        Ok(())
    }

    // ── QR Login endpoints ──────────────────────────────────────────────

    /// Fetch a QR code for bot login.
    pub async fn get_bot_qrcode(&self, bot_type: &str) -> Result<QrCodeResponse> {
        let url = format!(
            "{}/ilink/bot/get_bot_qrcode?bot_type={}",
            self.base_url.as_str().trim_end_matches('/'),
            urlencoding::encode(bot_type)
        );

        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url);
        if let Some(tag) = &self.route_tag {
            builder = builder.header("SKRouteTag", tag.as_str());
        }
        let request = builder.body(Vec::new()).expect("failed to build request");

        let response = self.http.execute(request).await.map_err(Error::Http)?;
        let status = response.status();
        let body = response.into_body();

        if !status.is_success() {
            let text = String::from_utf8_lossy(&body).to_string();
            return Err(Error::Http(HttpError::Status {
                status: status.as_u16(),
                body: text,
            }));
        }

        Ok(serde_json::from_slice(&body)?)
    }

    /// Poll QR code scan status (long-poll).
    pub async fn get_qrcode_status(&self, qrcode: &str) -> Result<QrStatusResponse> {
        let url = format!(
            "{}/ilink/bot/get_qrcode_status?qrcode={}",
            self.base_url.as_str().trim_end_matches('/'),
            urlencoding::encode(qrcode)
        );

        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .header("iLink-App-ClientVersion", "1");
        if let Some(tag) = &self.route_tag {
            builder = builder.header("SKRouteTag", tag.as_str());
        }
        let request = builder.body(Vec::new()).expect("failed to build request");

        let response = self.http.execute(request).await.map_err(Error::Http)?;
        let status = response.status();
        let body = response.into_body();

        if !status.is_success() {
            let text = String::from_utf8_lossy(&body).to_string();
            return Err(Error::Http(HttpError::Status {
                status: status.as_u16(),
                body: text,
            }));
        }

        Ok(serde_json::from_slice(&body)?)
    }

    // ── CDN raw operations ──────────────────────────────────────────────

    /// Upload raw bytes to the CDN. Returns the `x-encrypted-param` download param.
    pub async fn cdn_upload(
        &self,
        upload_param: &str,
        filekey: &str,
        body: Vec<u8>,
    ) -> Result<String> {
        let url = format!(
            "{}/upload?encrypted_query_param={}&filekey={}",
            self.cdn_base_url.as_str().trim_end_matches('/'),
            urlencoding::encode(upload_param),
            urlencoding::encode(filekey),
        );

        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(&url)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .expect("failed to build CDN upload request");

        let response = self.http.execute(request).await.map_err(Error::Http)?;
        let status = response.status();

        if status.as_u16() >= 400 {
            let err_msg = response
                .headers()
                .get("x-error-message")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown error")
                .to_string();
            return Err(Error::Cdn(format!("CDN upload failed {status}: {err_msg}")));
        }

        let download_param = response
            .headers()
            .get("x-encrypted-param")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or_else(|| Error::Cdn("missing x-encrypted-param in CDN response".into()))?;

        Ok(download_param)
    }

    /// Download raw bytes from the CDN.
    pub async fn cdn_download(&self, encrypt_query_param: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/download?encrypted_query_param={}",
            self.cdn_base_url.as_str().trim_end_matches('/'),
            urlencoding::encode(encrypt_query_param),
        );

        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .body(Vec::new())
            .expect("failed to build CDN download request");

        let response = self.http.execute(request).await.map_err(Error::Http)?;
        let status = response.status();

        if !status.is_success() {
            let text = String::from_utf8_lossy(response.body()).to_string();
            return Err(Error::Cdn(format!("CDN download {status}: {text}")));
        }

        Ok(response.into_body())
    }
}

/// Check if a `GetUpdatesResponse` indicates a session-expired error.
pub fn is_session_expired(resp: &GetUpdatesResponse) -> bool {
    resp.errcode == Some(SESSION_EXPIRED_ERRCODE) || resp.ret == Some(SESSION_EXPIRED_ERRCODE)
}

/// Check if a `GetUpdatesResponse` indicates an API error.
pub fn is_api_error(resp: &GetUpdatesResponse) -> bool {
    (resp.ret.is_some() && resp.ret != Some(0))
        || (resp.errcode.is_some() && resp.errcode != Some(0))
}
