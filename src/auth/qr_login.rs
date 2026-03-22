use std::time::Duration;

use crate::auth::credential::{normalize_account_id, AccountData, CredentialStore};
use crate::client::ILinkClient;
use crate::error::{Error, Result};
use crate::http::HttpClient;

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
    /// Normalized account ID (filesystem-safe).
    pub account_id: String,
    /// Raw account ID from server (e.g. `xxx@im.bot`).
    pub raw_account_id: String,
    /// Base URL for the API (may differ per account).
    pub base_url: Option<String>,
    /// The user ID of the person who scanned the QR code.
    pub user_id: Option<String>,
}

/// Manages a QR code login session.
pub struct QrLoginSession<'a, H: HttpClient> {
    client: &'a ILinkClient<H>,
    bot_type: String,
    qrcode: String,
    qrcode_url: String,
    timeout: Duration,
}

impl<'a, H: HttpClient> QrLoginSession<'a, H> {
    /// Start a new QR login session.
    pub async fn start(client: &'a ILinkClient<H>) -> Result<QrLoginSession<'a, H>> {
        Self::start_with_bot_type(client, DEFAULT_BOT_TYPE).await
    }

    /// Start with a custom bot_type.
    pub async fn start_with_bot_type(
        client: &'a ILinkClient<H>,
        bot_type: &str,
    ) -> Result<QrLoginSession<'a, H>> {
        let resp = client.get_bot_qrcode(bot_type).await?;
        tracing::info!("QR code obtained");

        Ok(QrLoginSession {
            client,
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

    /// The raw qrcode token (used internally for status polling).
    pub fn qrcode_token(&self) -> &str {
        &self.qrcode
    }

    /// Set the login timeout (default: 480 seconds).
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Print the QR code to the terminal.
    pub fn print_qr_to_terminal(&self) {
        use qrcode::QrCode;

        match QrCode::new(self.qrcode_url.as_bytes()) {
            Ok(code) => {
                let string = code
                    .render::<char>()
                    .quiet_zone(false)
                    .module_dimensions(2, 1)
                    .build();
                println!("{string}");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to render QR code");
                println!("QR Code URL: {}", self.qrcode_url);
            }
        }
    }

    /// Wait for the user to scan the QR code and complete login.
    ///
    /// This blocks until login succeeds, times out, or the QR expires
    /// (with automatic refresh up to 3 times).
    pub async fn wait_for_login(mut self) -> Result<LoginResult> {
        let deadline = tokio::time::Instant::now() + self.timeout;
        let mut qr_refresh_count: u32 = 1;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::LoginTimeout);
            }

            let status = match self.client.get_qrcode_status(&self.qrcode).await {
                Ok(s) => s,
                Err(Error::Http(crate::error::HttpError::Timeout(_))) => {
                    // Long-poll timeout is normal; retry.
                    continue;
                }
                Err(e) => return Err(e),
            };

            match status.status.as_str() {
                "wait" => {}
                "scaned" => {
                    tracing::info!("QR code scanned, waiting for confirmation...");
                }
                "expired" => {
                    qr_refresh_count += 1;
                    if qr_refresh_count > MAX_QR_REFRESH {
                        return Err(Error::QrExpired);
                    }
                    tracing::info!(
                        refresh = qr_refresh_count,
                        max = MAX_QR_REFRESH,
                        "QR expired, refreshing"
                    );
                    let resp = self.client.get_bot_qrcode(&self.bot_type).await?;
                    self.qrcode = resp.qrcode;
                    self.qrcode_url = resp.qrcode_img_content;
                    self.print_qr_to_terminal();
                }
                "confirmed" => {
                    let bot_id = status
                        .ilink_bot_id
                        .ok_or_else(|| Error::Other("server did not return ilink_bot_id".into()))?;
                    let bot_token = status
                        .bot_token
                        .ok_or_else(|| Error::Other("server did not return bot_token".into()))?;

                    tracing::info!(account_id = %bot_id, "login confirmed");

                    return Ok(LoginResult {
                        bot_token,
                        account_id: normalize_account_id(&bot_id),
                        raw_account_id: bot_id,
                        base_url: status.baseurl,
                        user_id: status.ilink_user_id,
                    });
                }
                other => {
                    tracing::warn!(status = other, "unknown QR status");
                }
            }

            tokio::time::sleep(QR_POLL_INTERVAL).await;
        }
    }

    /// Convenience: wait for login and persist credentials.
    pub async fn wait_and_save(self, store: &CredentialStore) -> Result<LoginResult> {
        let result = self.wait_for_login().await?;
        store.save_account(
            &result.account_id,
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
