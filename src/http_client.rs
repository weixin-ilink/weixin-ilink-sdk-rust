use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::error::HttpError;

/// Response future type for `HttpClient`.
pub type HttpFuture = Pin<Box<dyn Future<Output = Result<http::Response<Vec<u8>>, HttpError>> + Send>>;

/// Pluggable HTTP client trait (object-safe).
///
/// Implement this trait to use a custom HTTP backend.
/// A default implementation for `reqwest::Client` is provided.
pub trait HttpClient: Send + Sync + 'static {
    /// Execute an HTTP request and return the response.
    fn execute(&self, request: http::Request<Vec<u8>>) -> HttpFuture;
}

// ── reqwest implementation ──────────────────────────────────────────────────

impl HttpClient for reqwest::Client {
    fn execute(&self, request: http::Request<Vec<u8>>) -> HttpFuture {
        let this = self.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();

            let url = parts.uri.to_string();
            let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
                .map_err(|e| HttpError::Other(format!("invalid method: {e}")))?;

            let mut builder = this.request(method, &url);
            for (key, value) in &parts.headers {
                builder = builder.header(key.as_str(), value.as_bytes());
            }
            builder = builder.body(body);

            let response = builder.send().await.map_err(|e| {
                if e.is_timeout() {
                    HttpError::Timeout
                } else {
                    HttpError::Request(e.to_string())
                }
            })?;

            let status = response.status().as_u16();
            let mut http_response = http::Response::builder().status(status);
            for (key, value) in response.headers() {
                http_response = http_response.header(key.as_str(), value.as_bytes());
            }
            let bytes = response
                .bytes()
                .await
                .map_err(|e| HttpError::Request(e.to_string()))?;
            http_response
                .body(bytes.to_vec())
                .map_err(|e| HttpError::Other(e.to_string()))
        })
    }
}

/// Create a default `reqwest::Client` with sensible defaults.
pub fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(5)
        .build()
        .expect("failed to build default reqwest client")
}
