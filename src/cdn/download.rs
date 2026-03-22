use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::cdn::aes_ecb::{decrypt_aes_ecb, parse_aes_key};
use crate::client::ILinkClient;
use crate::error::{Error, Result};
use crate::http::HttpClient;

/// Download and AES-128-ECB decrypt a CDN media file.
///
/// `aes_key_base64` is the `CdnMedia.aes_key` field from the API (base64 encoded).
pub async fn download_and_decrypt<H: HttpClient>(
    client: &ILinkClient<H>,
    encrypt_query_param: &str,
    aes_key_base64: &str,
) -> Result<Vec<u8>> {
    let key = parse_aes_key(aes_key_base64)?;

    tracing::debug!("CDN download + decrypt");
    let encrypted = client.cdn_download(encrypt_query_param).await?;
    tracing::debug!(encrypted_bytes = encrypted.len(), "downloaded, decrypting");

    let decrypted = decrypt_aes_ecb(&encrypted, &key)?;
    tracing::debug!(decrypted_bytes = decrypted.len(), "decrypted");

    Ok(decrypted)
}

/// Download and decrypt using a hex-encoded AES key (from `ImageItem.aeskey`).
///
/// This is the preferred path for inbound images, where the key is provided
/// as a raw hex string rather than base64.
pub async fn download_and_decrypt_hex_key<H: HttpClient>(
    client: &ILinkClient<H>,
    encrypt_query_param: &str,
    aeskey_hex: &str,
) -> Result<Vec<u8>> {
    let aes_key_base64 = BASE64.encode(hex::decode(aeskey_hex).map_err(|e| {
        Error::Aes(format!("invalid hex aeskey: {e}"))
    })?);
    download_and_decrypt(client, encrypt_query_param, &aes_key_base64).await
}

/// Download plain (unencrypted) bytes from the CDN.
pub async fn download_plain<H: HttpClient>(
    client: &ILinkClient<H>,
    encrypt_query_param: &str,
) -> Result<Vec<u8>> {
    client.cdn_download(encrypt_query_param).await
}
