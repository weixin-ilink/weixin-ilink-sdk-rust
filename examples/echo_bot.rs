use std::path::PathBuf;
use std::sync::Arc;

use tokio_stream::StreamExt;
use tracing_subscriber::EnvFilter;

use weixin_ilink_sdk::auth::{CredentialStore, QrLoginSession};
use weixin_ilink_sdk::messaging::{UpdateEvent, UpdatesStream, UpdatesStreamOptions, send_text};
use weixin_ilink_sdk::ILinkClient;

fn state_dir() -> PathBuf {
    let dir = dirs_or_default();
    std::fs::create_dir_all(&dir).expect("failed to create state dir");
    dir
}

fn dirs_or_default() -> PathBuf {
    if let Ok(d) = std::env::var("ILINK_STATE_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".ilink-sdk")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let store = CredentialStore::new(state_dir());

    // Try to load existing account, otherwise do QR login.
    let accounts = store.list_accounts()?;
    let (token, base_url) = if let Some(id) = accounts.first() {
        let data = store.load_account(id)?.expect("account in index but no data");
        tracing::info!(account_id = %id, "loaded existing account");
        (
            data.token.expect("no token in saved account"),
            data.base_url,
        )
    } else {
        tracing::info!("no saved account, starting QR login...");
        let client = ILinkClient::builder().build();
        let session = QrLoginSession::start(&client).await?;

        println!("\n请使用微信扫描以下二维码:\n");
        session.print_qr_to_terminal();
        println!("\n等待扫码...\n");

        let result = session.wait_and_save(&store).await?;
        println!("登录成功! account_id={}", result.account_id);
        (result.bot_token, result.base_url)
    };

    // Build authenticated client.
    let mut builder = ILinkClient::builder().token(&token);
    if let Some(url) = &base_url {
        builder = builder.base_url(url);
    }
    let client = Arc::new(builder.build());

    // Load persisted get_updates_buf if any.
    let buf_path = state_dir().join("sync_buf.txt");
    let initial_buf = std::fs::read_to_string(&buf_path).unwrap_or_default();
    if !initial_buf.is_empty() {
        tracing::info!(bytes = initial_buf.len(), "resuming from saved sync buf");
    }

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
                let text = msg.text().unwrap_or("");

                tracing::info!(from, text, "收到消息");

                if text.is_empty() {
                    // Media-only message, just acknowledge.
                    if let Some(media) = msg.media_item() {
                        let type_name = match media.typed() {
                            Some(weixin_ilink_sdk::types::MessageItemType::Image) => "图片",
                            Some(weixin_ilink_sdk::types::MessageItemType::Voice) => "语音",
                            Some(weixin_ilink_sdk::types::MessageItemType::Video) => "视频",
                            Some(weixin_ilink_sdk::types::MessageItemType::File) => "文件",
                            _ => "媒体",
                        };
                        if !ctx_token.is_empty() {
                            let reply = format!("收到{type_name} 👍");
                            if let Err(e) = send_text(&client, from, &reply, ctx_token).await {
                                tracing::error!(error = %e, "发送回复失败");
                            }
                        }
                    }
                    continue;
                }

                if ctx_token.is_empty() {
                    tracing::warn!(from, "missing context_token, cannot reply");
                    continue;
                }

                // Echo back.
                let reply = format!("Echo: {text}");
                match send_text(&client, from, &reply, ctx_token).await {
                    Ok(msg_id) => {
                        tracing::info!(msg_id, to = from, "回复已发送");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, to = from, "发送回复失败");
                    }
                }
            }
            Ok(UpdateEvent::BufUpdated(buf)) => {
                // Persist sync buf for next restart.
                if let Err(e) = std::fs::write(&buf_path, &buf) {
                    tracing::error!(error = %e, "failed to persist sync buf");
                }
                tracing::debug!(bytes = buf.len(), "sync buf updated");
            }
            Ok(UpdateEvent::SessionExpired) => {
                tracing::error!("session expired, will retry after pause");
            }
            Err(e) => {
                tracing::error!(error = %e, "stream error");
            }
        }
    }

    Ok(())
}
