use std::time::Duration;

use crate::auth::credential::{AccountData, CredentialStore};
use crate::error::{Error, HttpError, Result};
use crate::http_client::HttpClient;
use crate::types::{QrCodeResponse, QrStatus, QrStatusResponse};

/// Default `bot_type` for iLink QR login.
pub const DEFAULT_BOT_TYPE: &str = "3";
const QR_POLL_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_LOGIN_TIMEOUT: Duration = Duration::from_secs(480);
const MAX_QR_REFRESH: u32 = 3;

/// Result of a successful QR login.
#[derive(Debug, Clone)]
pub struct LoginResult {
    /// The bot token for API authentication.
    pub bot_token: String,
    /// Bot ID from server (e.g. `abc@im.bot`).
    pub ilink_bot_id: String,
    /// Base URL for the API (may differ per account).
    pub base_url: Option<String>,
    /// The user ID of the person who scanned the QR code.
    pub user_id: Option<String>,
}

/// Handler trait for login lifecycle events.
///
/// Implement only the methods you care about — all have empty defaults.
///
/// ```ignore
/// struct MyHandler;
/// impl LoginHandler for MyHandler {
///     fn on_qrcode(&self, url: &str) {
///         println!("Please scan: {url}");
///     }
/// }
/// let client = ILinkClient::builder().login(&MyHandler).await?;
/// ```
pub trait LoginHandler: Send + Sync {
    /// Called when a QR code is available (initial or refreshed).
    fn on_qrcode(&self, _url: &str) {}

    /// Called when the QR code has been scanned (waiting for confirmation).
    fn on_scanned(&self) {}

    /// Called when a QR code expires and is being refreshed.
    fn on_expired(&self, _refresh_count: u32, _max_refreshes: u32) {}
}

/// Default handler that prints QR code to terminal using Unicode half-block rendering.
pub struct TerminalLoginHandler;

impl LoginHandler for TerminalLoginHandler {
    fn on_qrcode(&self, url: &str) {
        use qrcode::QrCode;
        use qrcode::render::unicode;

        match QrCode::new(url.as_bytes()) {
            Ok(code) => {
                let string = code
                    .render::<unicode::Dense1x2>()
                    .dark_color(unicode::Dense1x2::Light)
                    .light_color(unicode::Dense1x2::Dark)
                    .quiet_zone(false)
                    .build();
                println!("{string}");
            }
            Err(_) => {
                println!("QR Code URL: {url}");
            }
        }
    }

    fn on_scanned(&self) {
        println!("已扫码，等待确认...");
    }

    fn on_expired(&self, refresh_count: u32, max_refreshes: u32) {
        println!("二维码已过期，正在刷新... ({refresh_count}/{max_refreshes})");
    }
}

/// A no-op handler that silently ignores all events.
pub struct SilentLoginHandler;

impl LoginHandler for SilentLoginHandler {}

/// Manages a QR code login session.
///
/// Independent of `ILinkClient` — only needs an `HttpClient` and a base URL.
/// For most users, prefer `ILinkClient::builder().login(handler)` instead.
pub struct QrLoginSession<'a, H: HttpClient> {
    http: &'a H,
    base_url: String,
    route_tag: Option<String>,
    bot_type: String,
    qrcode: String,
    qrcode_url: String,
    timeout: Duration,
}

impl<'a, H: HttpClient> QrLoginSession<'a, H> {
    /// Start a new QR login session.
    pub async fn start(
        http: &'a H,
        base_url: &str,
        route_tag: Option<&str>,
    ) -> Result<QrLoginSession<'a, H>> {
        Self::start_with_bot_type(http, base_url, route_tag, DEFAULT_BOT_TYPE).await
    }

    /// Start with a custom bot_type.
    pub async fn start_with_bot_type(
        http: &'a H,
        base_url: &str,
        route_tag: Option<&str>,
        bot_type: &str,
    ) -> Result<QrLoginSession<'a, H>> {
        let base = base_url.trim_end_matches('/').to_string();
        let resp = fetch_qrcode(http, &base, route_tag, bot_type).await?;
        tracing::info!("QR code obtained");

        Ok(QrLoginSession {
            http,
            base_url: base,
            route_tag: route_tag.map(String::from),
            bot_type: bot_type.to_string(),
            qrcode: resp.qrcode,
            qrcode_url: resp.qrcode_img_content,
            timeout: DEFAULT_LOGIN_TIMEOUT,
        })
    }

    /// The QR code URL (for rendering in terminal or displaying to user).
    pub fn qrcode_url(&self) -> &str {
        &self.qrcode_url
    }

    /// Set the login timeout (default: 480 seconds).
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Wait for QR scan with the default terminal handler.
    pub async fn wait_for_login(self) -> Result<LoginResult> {
        self.wait_for_login_with(&TerminalLoginHandler).await
    }

    /// Wait for QR scan with a custom handler.
    pub async fn wait_for_login_with(
        mut self,
        handler: &dyn LoginHandler,
    ) -> Result<LoginResult> {
        handler.on_qrcode(&self.qrcode_url);

        let deadline = tokio::time::Instant::now() + self.timeout;
        let mut qr_refresh_count: u32 = 0;
        let mut scanned_notified = false;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::LoginTimeout);
            }

            let status = match poll_qr_status(
                self.http,
                &self.base_url,
                self.route_tag.as_deref(),
                &self.qrcode,
            )
            .await
            {
                Ok(s) => s,
                Err(Error::Http(HttpError::Timeout)) => continue,
                Err(e) => return Err(e),
            };

            match status.status {
                QrStatus::Wait => {}
                QrStatus::Scaned => {
                    if !scanned_notified {
                        scanned_notified = true;
                        handler.on_scanned();
                    }
                }
                QrStatus::Expired => {
                    qr_refresh_count += 1;
                    if qr_refresh_count > MAX_QR_REFRESH {
                        return Err(Error::QrExpired);
                    }
                    handler.on_expired(qr_refresh_count, MAX_QR_REFRESH);

                    let resp = fetch_qrcode(
                        self.http,
                        &self.base_url,
                        self.route_tag.as_deref(),
                        &self.bot_type,
                    )
                    .await?;
                    self.qrcode = resp.qrcode;
                    self.qrcode_url = resp.qrcode_img_content;
                    scanned_notified = false;

                    handler.on_qrcode(&self.qrcode_url);
                }
                QrStatus::Confirmed => {
                    let bot_id = status
                        .ilink_bot_id
                        .ok_or_else(|| Error::Other("server did not return ilink_bot_id".into()))?;
                    let bot_token = status
                        .bot_token
                        .ok_or_else(|| Error::Other("server did not return bot_token".into()))?;

                    tracing::info!(ilink_bot_id = %bot_id, "login confirmed");

                    return Ok(LoginResult {
                        bot_token,
                        ilink_bot_id: bot_id,
                        base_url: status.baseurl,
                        user_id: status.ilink_user_id,
                    });
                }
            }

            tokio::time::sleep(QR_POLL_INTERVAL).await;
        }
    }

    /// Convenience: wait for login and persist credentials.
    pub async fn wait_and_save(
        self,
        store: &CredentialStore,
    ) -> Result<LoginResult> {
        self.wait_and_save_with(&TerminalLoginHandler, store).await
    }

    /// Wait with custom handler and persist credentials.
    pub async fn wait_and_save_with(
        self,
        handler: &dyn LoginHandler,
        store: &CredentialStore,
    ) -> Result<LoginResult> {
        let result = self.wait_for_login_with(handler).await?;
        store.save_account(
            &result.ilink_bot_id,
            &AccountData {
                token: Some(result.bot_token.clone()),
                saved_at: Some(chrono::Utc::now()),
                base_url: result.base_url.clone(),
                user_id: result.user_id.clone(),
            },
        )?;
        Ok(result)
    }
}

// ── Raw HTTP helpers (no ILinkClient dependency) ────────────────────────────

async fn fetch_qrcode<H: HttpClient>(
    http: &H,
    base_url: &str,
    route_tag: Option<&str>,
    bot_type: &str,
) -> Result<QrCodeResponse> {
    let url = format!(
        "{base_url}/ilink/bot/get_bot_qrcode?bot_type={}",
        urlencoding::encode(bot_type)
    );
    let mut builder = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url);
    if let Some(tag) = route_tag {
        builder = builder.header("SKRouteTag", tag);
    }
    let request = builder.body(Vec::new()).expect("failed to build request");

    let response = http.execute(request).await.map_err(Error::Http)?;
    if !response.status().is_success() {
        let text = String::from_utf8_lossy(response.body()).to_string();
        return Err(Error::Http(HttpError::Status {
            status: response.status().as_u16(),
            body: text,
        }));
    }
    Ok(serde_json::from_slice(response.body())?)
}

async fn poll_qr_status<H: HttpClient>(
    http: &H,
    base_url: &str,
    route_tag: Option<&str>,
    qrcode: &str,
) -> Result<QrStatusResponse> {
    let url = format!(
        "{base_url}/ilink/bot/get_qrcode_status?qrcode={}",
        urlencoding::encode(qrcode)
    );
    let mut builder = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .header("iLink-App-ClientVersion", "1");
    if let Some(tag) = route_tag {
        builder = builder.header("SKRouteTag", tag);
    }
    let request = builder.body(Vec::new()).expect("failed to build request");

    let response = http.execute(request).await.map_err(Error::Http)?;
    if !response.status().is_success() {
        let text = String::from_utf8_lossy(response.body()).to_string();
        return Err(Error::Http(HttpError::Status {
            status: response.status().as_u16(),
            body: text,
        }));
    }
    Ok(serde_json::from_slice(response.body())?)
}
