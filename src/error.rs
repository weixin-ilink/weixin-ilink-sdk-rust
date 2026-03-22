use thiserror::Error;

/// Session expired error code from the iLink API.
pub const SESSION_EXPIRED_ERRCODE: i32 = -14;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("HTTP error: {0}")]
    Http(#[from] HttpError),

    #[error("API error: ret={ret}, errcode={errcode:?}, message={message}")]
    Api {
        ret: i32,
        errcode: Option<i32>,
        message: String,
    },

    #[error("session expired (errcode {SESSION_EXPIRED_ERRCODE})")]
    SessionExpired,

    #[error("CDN error: {message}")]
    Cdn {
        message: String,
        /// HTTP status code from CDN, if available.
        status_code: Option<u16>,
    },

    #[error("AES error: {0}")]
    Aes(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("missing context_token: required for sending messages")]
    MissingContextToken,

    #[error("account not configured: please login first")]
    NotConfigured,

    #[error("login timeout")]
    LoginTimeout,

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("QR code expired")]
    QrExpired,

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HttpError {
    #[error("request failed: {0}")]
    Request(String),

    #[error("HTTP {status}: {body}")]
    Status { status: u16, body: String },

    #[error("request timed out")]
    Timeout,

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Returns true if this is a CDN client error (4xx).
    pub fn is_cdn_4xx_error(&self) -> bool {
        matches!(self, Error::Cdn { status_code: Some(s), .. } if (400..500).contains(s))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
