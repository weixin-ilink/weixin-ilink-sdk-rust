use thiserror::Error;

/// Session expired error code from the iLink API.
pub const SESSION_EXPIRED_ERRCODE: i32 = -14;

#[derive(Debug, Error)]
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

    #[error("CDN error: {0}")]
    Cdn(String),

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
pub enum HttpError {
    #[error("request failed: {0}")]
    Request(String),

    #[error("HTTP {status}: {body}")]
    Status { status: u16, body: String },

    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
