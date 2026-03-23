use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::cdn::upload::{upload_file, upload_image, upload_video, UploadedFile};
use crate::client::ILinkClient;
use crate::error::{Error, Result};
use crate::http_client::HttpClient;
use crate::types::*;
use crate::util::media_type::mime_from_path;

fn generate_client_id() -> String {
    format!("weixin-ilink-{}", uuid::Uuid::new_v4())
}

fn build_text_message(to: &str, text: &str, context_token: &str) -> SendMessageRequest {
    let item_list = if text.is_empty() {
        None
    } else {
        Some(vec![MessageItem {
            item_type: Some(MessageItemType::Text),
            text_item: Some(TextItem {
                text: Some(text.to_string()),
            }),
            ..Default::default()
        }])
    };

    SendMessageRequest {
        msg: Message {
            from_user_id: Some(String::new()),
            to_user_id: Some(to.to_string()),
            client_id: Some(generate_client_id()),
            message_type: Some(MessageType::Bot),
            message_state: Some(MessageState::Finish),
            item_list,
            context_token: Some(context_token.to_string()),
            ..Default::default()
        },
        base_info: None,
    }
}

fn build_media_item_request(
    to: &str,
    item: MessageItem,
    context_token: &str,
) -> SendMessageRequest {
    SendMessageRequest {
        msg: Message {
            from_user_id: Some(String::new()),
            to_user_id: Some(to.to_string()),
            client_id: Some(generate_client_id()),
            message_type: Some(MessageType::Bot),
            message_state: Some(MessageState::Finish),
            item_list: Some(vec![item]),
            context_token: Some(context_token.to_string()),
            ..Default::default()
        },
        base_info: None,
    }
}

fn image_message_item(uploaded: &UploadedFile) -> MessageItem {
    MessageItem {
        item_type: Some(MessageItemType::Image),
        image_item: Some(ImageItem {
            media: Some(CdnMedia {
                encrypt_query_param: Some(uploaded.download_param.clone()),
                aes_key: Some(BASE64.encode(uploaded.aeskey.as_bytes())),
                encrypt_type: Some(1),
            }),
            mid_size: Some(uploaded.ciphertext_size),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn video_message_item(uploaded: &UploadedFile) -> MessageItem {
    MessageItem {
        item_type: Some(MessageItemType::Video),
        video_item: Some(VideoItem {
            media: Some(CdnMedia {
                encrypt_query_param: Some(uploaded.download_param.clone()),
                aes_key: Some(BASE64.encode(uploaded.aeskey.as_bytes())),
                encrypt_type: Some(1),
            }),
            video_size: Some(uploaded.ciphertext_size),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn file_message_item(uploaded: &UploadedFile, file_name: &str) -> MessageItem {
    MessageItem {
        item_type: Some(MessageItemType::File),
        file_item: Some(FileItem {
            media: Some(CdnMedia {
                encrypt_query_param: Some(uploaded.download_param.clone()),
                aes_key: Some(BASE64.encode(uploaded.aeskey.as_bytes())),
                encrypt_type: Some(1),
            }),
            file_name: Some(file_name.to_string()),
            len: Some(uploaded.file_size.to_string()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Send a plain text message. Returns the generated client_id as message ID.
pub async fn send_text<H: HttpClient>(
    client: &ILinkClient<H>,
    to: &str,
    text: &str,
    context_token: &str,
) -> Result<String> {
    if context_token.is_empty() {
        return Err(Error::MissingContextToken);
    }
    let req = build_text_message(to, text, context_token);
    let msg_id = req.msg.client_id.clone().unwrap_or_default();
    client.send_message(req).await?;
    Ok(msg_id)
}

/// Send a text message optionally followed by media items.
///
/// If `text` is non-empty, sends the text as a separate message first,
/// then sends each media item individually.
async fn send_with_media<H: HttpClient>(
    client: &ILinkClient<H>,
    to: &str,
    text: &str,
    media_item: MessageItem,
    context_token: &str,
) -> Result<String> {
    if context_token.is_empty() {
        return Err(Error::MissingContextToken);
    }

    if !text.is_empty() {
        let text_req = build_text_message(to, text, context_token);
        client.send_message(text_req).await?;
    }

    let req = build_media_item_request(to, media_item, context_token);
    let msg_id = req.msg.client_id.clone().unwrap_or_default();
    client.send_message(req).await?;
    Ok(msg_id)
}

/// Send an image message. Uploads the file to CDN first.
pub async fn send_image<H: HttpClient>(
    client: &ILinkClient<H>,
    to: &str,
    path: &Path,
    text: &str,
    context_token: &str,
) -> Result<String> {
    let uploaded = upload_image(client, path, to).await?;
    send_with_media(client, to, text, image_message_item(&uploaded), context_token).await
}

/// Send a video message. Uploads the file to CDN first.
pub async fn send_video<H: HttpClient>(
    client: &ILinkClient<H>,
    to: &str,
    path: &Path,
    text: &str,
    context_token: &str,
) -> Result<String> {
    let uploaded = upload_video(client, path, to).await?;
    send_with_media(client, to, text, video_message_item(&uploaded), context_token).await
}

/// Send a file attachment. Uploads the file to CDN first.
pub async fn send_file<H: HttpClient>(
    client: &ILinkClient<H>,
    to: &str,
    path: &Path,
    text: &str,
    context_token: &str,
) -> Result<String> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let uploaded = upload_file(client, path, to).await?;
    send_with_media(
        client,
        to,
        text,
        file_message_item(&uploaded, &file_name),
        context_token,
    )
    .await
}

/// Send a media file, auto-detecting type from the file extension.
///
/// Routes to `send_image`, `send_video`, or `send_file` based on MIME type.
pub async fn send_media<H: HttpClient>(
    client: &ILinkClient<H>,
    to: &str,
    path: &Path,
    text: &str,
    context_token: &str,
) -> Result<String> {
    let mime = mime_from_path(path);
    if mime.starts_with("image/") {
        send_image(client, to, path, text, context_token).await
    } else if mime.starts_with("video/") {
        send_video(client, to, path, text, context_token).await
    } else {
        send_file(client, to, path, text, context_token).await
    }
}
