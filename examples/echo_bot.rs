use std::path::PathBuf;
use std::sync::Arc;

use tokio_stream::StreamExt;
use tracing_subscriber::EnvFilter;

use weixin_ilink_sdk::auth::{QrLoginSession, TerminalLoginHandler};
use weixin_ilink_sdk::cdn;
use weixin_ilink_sdk::messaging::{UpdateEvent, UpdatesStream, UpdatesStreamOptions, send_text};
use weixin_ilink_sdk::store::Store;
use weixin_ilink_sdk::types::MessageItemType;
use weixin_ilink_sdk::ILinkClient;

fn db_path() -> String {
    if let Ok(p) = std::env::var("ILINK_DB_PATH") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join(".ilink-sdk");
    std::fs::create_dir_all(&dir).expect("failed to create state dir");
    dir.join("ilink.db").to_string_lossy().to_string()
}

fn downloads_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join(".ilink-sdk").join("downloads");
    std::fs::create_dir_all(&dir).expect("failed to create downloads dir");
    dir
}

async fn handle_media(
    client: &ILinkClient,
    from: &str,
    ctx_token: &str,
    item: &weixin_ilink_sdk::types::MessageItem,
    type_name: &str,
    message_id: Option<u64>,
    create_time_ms: Option<u64>,
) {
    let (encrypt_param, aes_key, hex_key) = match item.item_type {
        Some(MessageItemType::Image) => {
            let img = item.image_item.as_ref().unwrap();
            let param = img.media.as_ref().and_then(|m| m.encrypt_query_param.as_deref());
            let aes = img.media.as_ref().and_then(|m| m.aes_key.as_deref());
            let hex = img.aeskey.as_deref();
            tracing::info!(
                encrypt_query_param = ?param.map(|p| &p[..p.len().min(60)]),
                has_aes_key = aes.is_some(),
                has_hex_key = hex.is_some(),
                mid_size = img.mid_size,
                hd_size = img.hd_size,
                thumb_size = img.thumb_size,
                "图片 CDN 信息"
            );
            (param, aes, hex)
        }
        Some(MessageItemType::Voice) => {
            let v = item.voice_item.as_ref().unwrap();
            let param = v.media.as_ref().and_then(|m| m.encrypt_query_param.as_deref());
            let aes = v.media.as_ref().and_then(|m| m.aes_key.as_deref());
            tracing::info!(
                encrypt_query_param = ?param.map(|p| &p[..p.len().min(60)]),
                encode_type = v.encode_type,
                playtime_ms = v.playtime,
                voice_text = v.text,
                "语音 CDN 信息"
            );
            (param, aes, None)
        }
        Some(MessageItemType::File) => {
            let f = item.file_item.as_ref().unwrap();
            let param = f.media.as_ref().and_then(|m| m.encrypt_query_param.as_deref());
            let aes = f.media.as_ref().and_then(|m| m.aes_key.as_deref());
            tracing::info!(
                encrypt_query_param = ?param.map(|p| &p[..p.len().min(60)]),
                file_name = f.file_name,
                len = f.len,
                "文件 CDN 信息"
            );
            (param, aes, None)
        }
        Some(MessageItemType::Video) => {
            let v = item.video_item.as_ref().unwrap();
            let param = v.media.as_ref().and_then(|m| m.encrypt_query_param.as_deref());
            let aes = v.media.as_ref().and_then(|m| m.aes_key.as_deref());
            tracing::info!(
                encrypt_query_param = ?param.map(|p| &p[..p.len().min(60)]),
                video_size = v.video_size,
                play_length = v.play_length,
                "视频 CDN 信息"
            );
            (param, aes, None)
        }
        _ => (None, None, None),
    };

    if let Some(param) = encrypt_param {
        let result = if let Some(hex_key) = hex_key {
            cdn::download_and_decrypt_hex_key(client, param, hex_key).await
        } else if let Some(aes_key) = aes_key {
            cdn::download_and_decrypt(client, param, aes_key).await
        } else {
            cdn::download_plain(client, param).await
        };

        match result {
            Ok(mut data) => {
                let mut ext = match item.item_type {
                    Some(MessageItemType::Image) => "jpg",
                    Some(MessageItemType::Voice) => "silk",
                    Some(MessageItemType::Video) => "mp4",
                    Some(MessageItemType::File) => {
                        let name = item.file_item.as_ref().and_then(|f| f.file_name.as_deref()).unwrap_or("file.bin");
                        name.rsplit('.').next().unwrap_or("bin")
                    }
                    _ => "bin",
                };
                if item.item_type == Some(MessageItemType::Voice) {
                    use weixin_ilink_sdk::voice::{DefaultSilkDecoder, SilkDecoder, build_wav, DEFAULT_VOICE_SAMPLE_RATE};
                    let sample_rate = item.voice_item.as_ref().and_then(|v| v.sample_rate).unwrap_or(DEFAULT_VOICE_SAMPLE_RATE);
                    match DefaultSilkDecoder.decode(&data, sample_rate) {
                        Ok(pcm) => {
                            data = build_wav(&pcm, sample_rate);
                            ext = "wav";
                            tracing::info!(pcm_bytes = pcm.len(), wav_bytes = data.len(), "SILK → WAV 转换成功");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "SILK → WAV 转换失败，保存原始 SILK");
                        }
                    }
                }

                let ts = create_time_ms.unwrap_or(0);
                let mid = message_id.unwrap_or(0);
                let filename = format!("{type_name}_{ts}_{mid}.{ext}");
                let save_path = downloads_dir().join(&filename);
                let size = data.len();
                if let Err(e) = tokio::fs::write(&save_path, &data).await {
                    tracing::error!(error = %e, "保存文件失败");
                } else {
                    tracing::info!(path = %save_path.display(), size, "媒体已下载保存");
                    if !ctx_token.is_empty() {
                        let voice_text = item.voice_item.as_ref().and_then(|v| v.text.as_deref());
                        let reply = if let Some(vt) = voice_text {
                            format!("收到{type_name}，已保存 ({size} bytes)\n转文字: {vt}")
                        } else {
                            format!("收到{type_name}，已保存 ({size} bytes)")
                        };
                        let _ = send_text(client, from, &reply, ctx_token).await;
                    }
                    return;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "下载媒体失败");
            }
        }
    }

    if !ctx_token.is_empty() {
        let reply = format!("收到{type_name} 👍");
        let _ = send_text(client, from, &reply, ctx_token).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let store = Store::open_local(&db_path()).await?;
    tracing::info!(db = %db_path(), "database opened");

    // Try to load existing account, otherwise do QR login.
    let accounts = store.list_accounts().await?;
    let (client, bot_id) = if let Some(id) = accounts.into_iter().next() {
        let row = store.load_account(&id).await?.expect("account listed but not found");
        tracing::info!(ilink_bot_id = %id, "loaded existing account");
        let mut b = ILinkClient::builder().token(&row.token);
        if let Some(url) = &row.base_url {
            b = b.base_url(url);
        }
        (Arc::new(b.build()), id)
    } else {
        tracing::info!("no saved account, starting QR login...");
        let tmp_http = weixin_ilink_sdk::http_client::default_http_client();
        let session = QrLoginSession::start(
            &tmp_http,
            weixin_ilink_sdk::client::DEFAULT_BASE_URL,
            None,
        ).await?;
        let result = session.wait_for_login_with(&TerminalLoginHandler).await?;

        store.save_account(
            &result.ilink_bot_id,
            &result.bot_token,
            result.base_url.as_deref(),
            result.user_id.as_deref(),
        ).await?;

        let mut b = ILinkClient::builder().token(&result.bot_token);
        if let Some(url) = &result.base_url {
            b = b.base_url(url);
        }
        let bot_id = result.ilink_bot_id;
        (Arc::new(b.build()), bot_id)
    };

    // Load sync cursor.
    let initial_buf = match store.load_sync_buf(&bot_id).await? {
        Some(row) => {
            tracing::info!(
                bytes = row.get_updates_buf.len(),
                last_saved = %row.updated_at,
                "resuming from saved sync buf"
            );
            row.get_updates_buf
        }
        None => String::new(),
    };

    println!("Echo Bot 已启动，等待消息...\n");

    let mut stream = UpdatesStream::new(
        client.clone(),
        UpdatesStreamOptions {
            initial_buf,
            poll_timeout: None,
        },
    );

    while let Some(event) = stream.next().await {
        match event {
            Ok(UpdateEvent::Message(msg)) => {
                let from = msg.from_user_id.as_deref().unwrap_or("unknown");
                let ctx_token = msg.context_token.as_deref().unwrap_or("");

                let item_types: Vec<&str> = msg
                    .item_list
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|item| match item.item_type {
                        Some(MessageItemType::Text) => "Text",
                        Some(MessageItemType::Image) => "Image",
                        Some(MessageItemType::Voice) => "Voice",
                        Some(MessageItemType::File) => "File",
                        Some(MessageItemType::Video) => "Video",
                        None | Some(_) => "Unknown",
                    })
                    .collect();
                let types_str = item_types.join(",");
                tracing::info!(from, types = %types_str, "收到消息");

                if let Some(text) = msg.text() {
                    tracing::info!(from, text, "收到文本");
                    if !ctx_token.is_empty() {
                        let reply = format!("Echo: {text}");
                        match send_text(&client, from, &reply, ctx_token).await {
                            Ok(msg_id) => tracing::info!(msg_id, to = from, "回复已发送"),
                            Err(e) => tracing::error!(error = %e, to = from, "发送回复失败"),
                        }
                    }
                } else if let Some(media) = msg.media_item() {
                    let type_name = match media.item_type {
                        Some(MessageItemType::Image) => "图片",
                        Some(MessageItemType::Voice) => "语音",
                        Some(MessageItemType::Video) => "视频",
                        Some(MessageItemType::File) => "文件",
                        _ => "媒体",
                    };
                    tracing::info!(from, type_name, "收到媒体");
                    handle_media(&client, from, ctx_token, media, type_name, msg.message_id, msg.create_time_ms).await;
                } else {
                    tracing::debug!(from, "收到空消息，忽略");
                }
            }
            Ok(UpdateEvent::BufUpdated(buf)) => {
                if let Err(e) = store.save_sync_buf(&bot_id, &buf).await {
                    tracing::error!(error = %e, "failed to persist sync buf");
                }
                tracing::debug!(bytes = buf.len(), "sync buf updated");
            }
            Ok(UpdateEvent::SessionExpired) => {
                tracing::error!("session expired, will retry after pause");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!(error = %e, "stream error");
            }
        }
    }

    Ok(())
}
