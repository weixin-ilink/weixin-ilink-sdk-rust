use std::path::Path;

use md5::{Digest, Md5};
use rand::RngExt;

use crate::cdn::aes_ecb::{aes_ecb_padded_size, encrypt_aes_ecb};
use crate::client::ILinkClient;
use crate::error::{Error, Result};
use crate::http_client::HttpClient;
use crate::types::{GetUploadUrlRequest, UploadMediaType};

/// Information about a successfully uploaded file.
#[derive(Debug, Clone)]
pub struct UploadedFile {
    pub filekey: String,
    /// CDN download encrypted_query_param.
    pub download_param: String,
    /// AES-128 key as hex string.
    pub aeskey: String,
    /// Plaintext file size in bytes.
    pub file_size: u64,
    /// Ciphertext file size in bytes (after AES-ECB padding).
    pub ciphertext_size: u64,
}

const CDN_UPLOAD_MAX_RETRIES: u32 = 3;

/// Upload in-memory bytes to the Weixin CDN.
///
/// Core pipeline: hash → gen aeskey → getUploadUrl → encrypt → CDN POST with retry.
pub async fn upload_bytes<H: HttpClient>(
    client: &ILinkClient<H>,
    plaintext: &[u8],
    to_user_id: &str,
    media_type: UploadMediaType,
) -> Result<UploadedFile> {
    let rawsize = plaintext.len() as u64;
    let rawfilemd5 = {
        let mut hasher = Md5::new();
        hasher.update(plaintext);
        format!("{:x}", hasher.finalize())
    };
    let filesize = aes_ecb_padded_size(plaintext.len()) as u64;
    let filekey = hex::encode(rand::rng().random::<[u8; 16]>());
    let aeskey_bytes: [u8; 16] = rand::rng().random();
    let aeskey_hex = hex::encode(aeskey_bytes);

    tracing::debug!(rawsize, filesize, %rawfilemd5, %filekey, "uploading bytes");

    let upload_url_resp = client
        .get_upload_url(&GetUploadUrlRequest {
            filekey: Some(filekey.clone()),
            media_type: Some(media_type),
            to_user_id: Some(to_user_id.to_string()),
            rawsize: Some(rawsize),
            rawfilemd5: Some(rawfilemd5),
            filesize: Some(filesize),
            thumb_rawsize: None,
            thumb_rawfilemd5: None,
            thumb_filesize: None,
            no_need_thumb: Some(true),
            aeskey: Some(aeskey_hex.clone()),
            base_info: None,
        })
        .await?;

    if let Some(ret) = upload_url_resp.ret {
        if ret != 0 {
            return Err(Error::Api {
                ret,
                errcode: None,
                message: upload_url_resp
                    .errmsg
                    .unwrap_or_else(|| "getUploadUrl failed".into()),
            });
        }
    }

    let upload_param = upload_url_resp
        .upload_param
        .ok_or_else(|| Error::Cdn {
            message: "getUploadUrl returned no upload_param".into(),
            status_code: None,
        })?;

    let ciphertext = encrypt_aes_ecb(plaintext, &aeskey_bytes);

    let mut download_param = None;
    let mut last_err = None;

    for attempt in 1..=CDN_UPLOAD_MAX_RETRIES {
        match client
            .cdn_upload(&upload_param, &filekey, &ciphertext)
            .await
        {
            Ok(param) => {
                download_param = Some(param);
                tracing::debug!(attempt, "CDN upload success");
                break;
            }
            Err(e) => {
                let is_client_error = e.is_cdn_4xx_error();
                tracing::error!(attempt, error = %e, "CDN upload failed");
                last_err = Some(e);
                if is_client_error {
                    break;
                }
                if attempt < CDN_UPLOAD_MAX_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    let download_param = download_param
        .ok_or_else(|| {
            last_err.unwrap_or_else(|| Error::Cdn {
                message: "CDN upload failed".into(),
                status_code: None,
            })
        })?;

    Ok(UploadedFile {
        filekey,
        download_param,
        aeskey: aeskey_hex,
        file_size: rawsize,
        ciphertext_size: filesize,
    })
}

/// Upload a local file to the Weixin CDN.
pub async fn upload_media<H: HttpClient>(
    client: &ILinkClient<H>,
    path: &Path,
    to_user_id: &str,
    media_type: UploadMediaType,
) -> Result<UploadedFile> {
    let plaintext = tokio::fs::read(path).await?;
    tracing::debug!(path = %path.display(), "read file for upload");
    upload_bytes(client, &plaintext, to_user_id, media_type).await
}

/// Upload a local image to the CDN.
pub async fn upload_image<H: HttpClient>(
    client: &ILinkClient<H>,
    path: &Path,
    to_user_id: &str,
) -> Result<UploadedFile> {
    upload_media(client, path, to_user_id, UploadMediaType::Image).await
}

/// Upload a local video to the CDN.
pub async fn upload_video<H: HttpClient>(
    client: &ILinkClient<H>,
    path: &Path,
    to_user_id: &str,
) -> Result<UploadedFile> {
    upload_media(client, path, to_user_id, UploadMediaType::Video).await
}

/// Upload a local file attachment to the CDN.
pub async fn upload_file<H: HttpClient>(
    client: &ILinkClient<H>,
    path: &Path,
    to_user_id: &str,
) -> Result<UploadedFile> {
    upload_media(client, path, to_user_id, UploadMediaType::File).await
}
