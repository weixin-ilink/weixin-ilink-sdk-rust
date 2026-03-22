use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio_stream::Stream;

use crate::client::{ILinkClient, is_api_error, is_session_expired};
use crate::error::Result;
use crate::http_client::HttpClient;
use crate::types::{GetUpdatesResponse, Message};

const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const BACKOFF_DELAY: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(2);
const SESSION_PAUSE_DURATION: Duration = Duration::from_secs(3600);

/// Options for the updates stream.
#[derive(Debug, Clone)]
pub struct UpdatesStreamOptions {
    /// Initial `get_updates_buf` to resume from a previous session.
    pub initial_buf: String,
    /// Long-poll timeout per request.
    pub poll_timeout: Option<Duration>,
}

impl Default for UpdatesStreamOptions {
    fn default() -> Self {
        Self {
            initial_buf: String::new(),
            poll_timeout: None,
        }
    }
}

/// An event from the updates stream.
#[derive(Debug)]
#[non_exhaustive]
pub enum UpdateEvent {
    /// A new inbound message.
    Message(Message),
    /// The `get_updates_buf` was updated — persist this for resumption.
    BufUpdated(String),
    /// The session has expired; the stream will pause and retry.
    SessionExpired,
}

type PollFuture = Pin<Box<dyn std::future::Future<Output = Result<GetUpdatesResponse>> + Send>>;

enum StreamState {
    Idle,
    Polling(PollFuture),
    Sleeping(Pin<Box<tokio::time::Sleep>>),
    #[allow(dead_code)]
    Done,
}

/// A stream of inbound messages from `getUpdates` long-poll.
///
/// Handles:
/// - Automatic `get_updates_buf` continuation
/// - Error backoff (3 consecutive failures → 30s pause)
/// - Session expired pause (1 hour)
/// - Server-suggested poll timeout
/// - Auto-caching context tokens for `push_text`
pub struct UpdatesStream<H: HttpClient> {
    client: std::sync::Arc<ILinkClient<H>>,
    buf: String,
    poll_timeout: Option<Duration>,
    pending: Vec<Message>,
    state: StreamState,
    consecutive_failures: u32,
}

impl<H: HttpClient> UpdatesStream<H> {
    pub fn new(client: std::sync::Arc<ILinkClient<H>>, opts: UpdatesStreamOptions) -> Self {
        Self {
            client,
            buf: opts.initial_buf,
            poll_timeout: opts.poll_timeout,
            pending: Vec::new(),
            state: StreamState::Idle,
            consecutive_failures: 0,
        }
    }

    /// Get the current `get_updates_buf` for persistence.
    pub fn current_buf(&self) -> &str {
        &self.buf
    }
}

// UpdatesStream is Unpin because all fields are either Unpin or behind Box/Arc.
impl<H: HttpClient> Unpin for UpdatesStream<H> {}

impl<H: HttpClient> Stream for UpdatesStream<H> {
    type Item = Result<UpdateEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(msg) = this.pending.pop() {
            return Poll::Ready(Some(Ok(UpdateEvent::Message(msg))));
        }

        loop {
            match &mut this.state {
                StreamState::Done => return Poll::Ready(None),

                StreamState::Idle => {
                    let client = this.client.clone();
                    let buf = this.buf.clone();
                    let timeout = this.poll_timeout;
                    let fut: PollFuture = Box::pin(async move {
                        client.get_updates(&buf, timeout).await
                    });
                    this.state = StreamState::Polling(fut);
                }

                StreamState::Polling(fut) => {
                    match fut.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => {
                            this.consecutive_failures += 1;
                            tracing::error!(
                                failures = this.consecutive_failures,
                                error = %e,
                                "getUpdates error"
                            );
                            let delay = if this.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                this.consecutive_failures = 0;
                                BACKOFF_DELAY
                            } else {
                                RETRY_DELAY
                            };
                            this.state = StreamState::Sleeping(Box::pin(tokio::time::sleep(delay)));
                        }
                        Poll::Ready(Ok(resp)) => {
                            if is_session_expired(&resp) {
                                tracing::error!("session expired, pausing for 1 hour");
                                this.consecutive_failures = 0;
                                this.state = StreamState::Sleeping(Box::pin(
                                    tokio::time::sleep(SESSION_PAUSE_DURATION),
                                ));
                                return Poll::Ready(Some(Ok(UpdateEvent::SessionExpired)));
                            }

                            if is_api_error(&resp) {
                                this.consecutive_failures += 1;
                                tracing::error!(
                                    ret = resp.ret,
                                    errcode = resp.errcode,
                                    errmsg = ?resp.errmsg,
                                    failures = this.consecutive_failures,
                                    "getUpdates API error"
                                );
                                let delay = if this.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                    this.consecutive_failures = 0;
                                    BACKOFF_DELAY
                                } else {
                                    RETRY_DELAY
                                };
                                this.state = StreamState::Sleeping(Box::pin(tokio::time::sleep(delay)));
                                continue;
                            }

                            this.consecutive_failures = 0;

                            if let Some(t) = resp.longpolling_timeout_ms {
                                if t > 0 {
                                    this.poll_timeout = Some(Duration::from_millis(t));
                                }
                            }

                            let buf_updated =
                                if let Some(new_buf) = &resp.get_updates_buf {
                                    if !new_buf.is_empty() && *new_buf != this.buf {
                                        this.buf = new_buf.clone();
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                            if let Some(msgs) = resp.msgs {
                                for msg in &msgs {
                                    if let (Some(from), Some(ctx)) =
                                        (&msg.from_user_id, &msg.context_token)
                                    {
                                        if !from.is_empty() && !ctx.is_empty() {
                                            this.client.set_context_token(from, ctx);
                                        }
                                    }
                                }
                                this.pending = msgs;
                                this.pending.reverse();
                            }

                            this.state = StreamState::Idle;

                            if buf_updated {
                                return Poll::Ready(Some(Ok(UpdateEvent::BufUpdated(
                                    this.buf.clone(),
                                ))));
                            }

                            if let Some(msg) = this.pending.pop() {
                                return Poll::Ready(Some(Ok(UpdateEvent::Message(msg))));
                            }
                        }
                    }
                }

                StreamState::Sleeping(sleep) => {
                    match sleep.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            this.state = StreamState::Idle;
                        }
                    }
                }
            }
        }
    }
}
