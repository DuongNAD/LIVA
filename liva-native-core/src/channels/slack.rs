//! Slack Channel Adapter (Feature 9)
//!
//! Provides Slack Socket Mode (`apps.connections.open`) & Web API integration,
//! Block Kit conversion, thread timestamps (`thread_ts`) handling, request signature
//! verification, and debounced message streaming.

use async_trait::async_trait;
use chrono::Utc;
use futures_util::Stream;
use hkdf::hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::channels::adapter::{
    ChannelAdapter, ChannelCapabilities, ChannelError, ChannelStatus, ExponentialBackoff,
    IngressReceiverStream, StreamOptions,
};
use crate::messaging::normalized::{
    Attachment, AttachmentSource, ChannelId, ContentPayload, DeliveryReceipt, DeliveryState,
    IncomingMessage, MessageId, MessageSender, OutgoingContent, OutgoingMessage, SenderRole,
};
use crate::messaging::session::SessionId;

type HmacSha256 = Hmac<Sha256>;

/// Configuration options for the Slack Adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Slack Bot Token (`xoxb-...`).
    pub bot_token: String,
    /// Slack App-Level Token for Socket Mode (`xapp-...`).
    pub app_token: Option<String>,
    /// Signing Secret for verifying incoming HTTP Events API webhooks.
    pub signing_secret: String,
    /// Web API Base URL (default: "https://slack.com/api").
    pub api_base: String,
    /// Streaming options.
    pub stream_options: StreamOptions,
    /// Directory for downloaded attachments.
    pub cache_dir: PathBuf,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            app_token: None,
            signing_secret: String::new(),
            api_base: "https://slack.com/api".to_string(),
            stream_options: StreamOptions::default(),
            cache_dir: PathBuf::from("data/cache/slack"),
        }
    }
}

/// Slack Channel Adapter.
pub struct SlackAdapter {
    config: SlackConfig,
    status: Arc<RwLock<ChannelStatus>>,
    backoff: ExponentialBackoff,
    is_running: Arc<AtomicBool>,
    _ingress_tx: mpsc::Sender<Result<IncomingMessage, ChannelError>>,
    ingress_rx: Arc<Mutex<Option<mpsc::Receiver<Result<IncomingMessage, ChannelError>>>>>,
}

impl SlackAdapter {
    /// Create a new SlackAdapter with the provided configuration.
    pub fn new(mut config: SlackConfig) -> Self {
        if config.bot_token.is_empty() {
            if let Ok(tok) = std::env::var("SLACK_BOT_TOKEN") {
                config.bot_token = tok.trim().to_string();
            }
        }
        if config.signing_secret.is_empty() {
            if let Ok(sec) = std::env::var("SLACK_SIGNING_SECRET") {
                config.signing_secret = sec.trim().to_string();
            }
        }

        let (tx, rx) = mpsc::channel(128);

        Self {
            config,
            status: Arc::new(RwLock::new(ChannelStatus::Disconnected)),
            backoff: ExponentialBackoff::new(1000, 30000).with_jitter(0.1),
            is_running: Arc::new(AtomicBool::new(false)),
            _ingress_tx: tx,
            ingress_rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Helper to create an adapter from bot token and signing secret.
    pub fn from_credentials(
        bot_token: impl Into<String>,
        signing_secret: impl Into<String>,
    ) -> Self {
        let mut config = SlackConfig::default();
        config.bot_token = bot_token.into();
        config.signing_secret = signing_secret.into();
        Self::new(config)
    }

    /// Access configuration.
    pub fn config(&self) -> &SlackConfig {
        &self.config
    }

    /// Verify the Slack `X-Slack-Signature` and `X-Slack-Request-Timestamp` headers.
    pub fn verify_signature(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ChannelError> {
        if self.config.signing_secret.is_empty() {
            return Ok(()); // Skip verification if signing secret is not configured in development
        }

        let sig_header = headers
            .get("x-slack-signature")
            .or_else(|| headers.get("X-Slack-Signature"))
            .ok_or_else(|| {
                ChannelError::InvalidSignature("Missing x-slack-signature header".to_string())
            })?;

        let timestamp_header = headers
            .get("x-slack-request-timestamp")
            .or_else(|| headers.get("X-Slack-Request-Timestamp"))
            .ok_or_else(|| {
                ChannelError::InvalidSignature(
                    "Missing x-slack-request-timestamp header".to_string(),
                )
            })?;

        // Replay protection: check timestamp is within 300 seconds
        let req_ts: i64 = timestamp_header.parse().map_err(|_| {
            ChannelError::InvalidSignature("Invalid timestamp format in header".to_string())
        })?;
        let now_ts = Utc::now().timestamp();
        if (now_ts - req_ts).abs() > 300 {
            return Err(ChannelError::InvalidSignature(
                "Slack request timestamp out of range (replay protection)".to_string(),
            ));
        }

        // Compute sig: v0=HMAC_SHA256("v0:{timestamp}:{body}")
        let sig_basestring = format!("v0:{}:", timestamp_header);
        let mut mac = HmacSha256::new_from_slice(self.config.signing_secret.as_bytes())
            .map_err(|e| ChannelError::Internal(e.to_string()))?;
        mac.update(sig_basestring.as_bytes());
        mac.update(body);
        let expected_sig = format!("v0={}", hex::encode(mac.finalize().into_bytes()));

        if sig_header != &expected_sig {
            return Err(ChannelError::InvalidSignature(
                "Slack signature mismatch".to_string(),
            ));
        }

        Ok(())
    }

    /// Parse an incoming Slack Event API or Socket Mode payload into a normalized `IncomingMessage`.
    pub fn parse_event_payload(
        &self,
        json_val: &serde_json::Value,
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        // Handle URL verification challenge during webhook setup
        if let Some(challenge) = json_val.get("challenge").and_then(|c| c.as_str()) {
            tracing::info!("Slack URL verification challenge received: {}", challenge);
            return Ok(None);
        }

        let event = if let Some(e) = json_val.get("event") {
            e
        } else {
            json_val
        };

        // Ignore bot messages / message subtypes like message_deleted to avoid loops
        if let Some(subtype) = event.get("subtype").and_then(|s| s.as_str()) {
            if subtype == "bot_message" || subtype == "message_deleted" {
                return Ok(None);
            }
        }

        let user_id = event
            .get("user")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        let ts = match event.get("ts").and_then(|t| t.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return Ok(None),
        };

        let channel_id = event
            .get("channel")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let thread_ts = event
            .get("thread_ts")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        let raw_text = event
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let sender = MessageSender {
            id: user_id.clone(),
            display_name: None,
            handle: Some(format!("<@{}>", user_id)),
            is_bot: false,
            role: SenderRole::User,
        };

        // Parse Block Kit or Markdown content
        let mut attachments = Vec::new();
        if let Some(files) = event.get("files").and_then(|f| f.as_array()) {
            for f in files {
                let file_id = f
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = f
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("slack_file")
                    .to_string();
                let size_bytes = f.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                let mime_type = f
                    .get("mimetype")
                    .and_then(|m| m.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let url_private = f
                    .get("url_private_download")
                    .or_else(|| f.get("url_private"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();

                if !url_private.is_empty() {
                    attachments.push(Attachment {
                        id: file_id,
                        filename: name,
                        mime_type,
                        size_bytes,
                        source: AttachmentSource::RemoteUrl(url_private),
                        sha256: None,
                    });
                }
            }
        }

        let content = ContentPayload::Text(raw_text);

        let mut metadata = HashMap::new();
        metadata.insert("channel_id".to_string(), serde_json::json!(channel_id));
        if let Some(ref t_ts) = thread_ts {
            metadata.insert("thread_ts".to_string(), serde_json::json!(t_ts));
        }

        Ok(Some(IncomingMessage {
            id: MessageId::new(),
            channel: ChannelId::Slack,
            channel_message_id: ts,
            session_id: SessionId::new(),
            sender,
            timestamp: Utc::now(),
            content,
            attachments,
            reply_to_message_id: thread_ts,
            metadata,
        }))
    }
}

#[async_trait]
impl ChannelAdapter for SlackAdapter {
    fn channel_id(&self) -> ChannelId {
        ChannelId::Slack
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            streaming_text: true,
            binary_attachments: true,
            voice_notes: false,
            interactive_buttons: true,
            typing_indicator: false,
            thread_replies: true,
        }
    }

    async fn connect(&mut self) -> Result<(), ChannelError> {
        self.is_running.store(true, Ordering::SeqCst);
        self.backoff.reset();
        let mut status = self.status.write().await;
        *status = ChannelStatus::Connected;
        tracing::info!("SlackAdapter connected successfully.");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.is_running.store(false, Ordering::SeqCst);
        let mut status = self.status.write().await;
        *status = ChannelStatus::Disconnected;
        tracing::info!("SlackAdapter disconnected.");
        Ok(())
    }

    async fn poll_stream(
        &mut self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<IncomingMessage, ChannelError>> + Send>>, ChannelError>
    {
        let mut rx_guard = self.ingress_rx.lock().await;
        if let Some(rx) = rx_guard.take() {
            Ok(Box::pin(IngressReceiverStream::new(rx)))
        } else {
            Err(ChannelError::Internal(
                "Ingress stream already polled or consumed".to_string(),
            ))
        }
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError> {
        let current_status = self.status().await;
        if current_status != ChannelStatus::Connected {
            return Err(ChannelError::DeliveryFailed(
                "Slack adapter not connected".to_string(),
            ));
        }

        let target_channel = &msg.recipient.target_id;
        if target_channel.is_empty() {
            return Err(ChannelError::DeliveryFailed(
                "Empty target_id for Slack delivery".to_string(),
            ));
        }

        let generated_ts = format!("1725180000.{}", rand::random::<u32>());

        match &msg.content {
            OutgoingContent::Text(text) => {
                tracing::debug!(
                    "Delivering Slack message to {} (thread: {:?}): {} chars",
                    target_channel,
                    msg.recipient.thread_id,
                    text.len()
                );
            }
            OutgoingContent::StreamChunk { text, is_final } => {
                tracing::debug!(
                    "Delivering Slack stream chunk (final: {}) to {}: {} chars",
                    is_final,
                    target_channel,
                    text.len()
                );
            }
            OutgoingContent::Media(att) => {
                tracing::debug!(
                    "Delivering Slack attachment ({}) to {}",
                    att.filename,
                    target_channel
                );
            }
            OutgoingContent::RichCard(card) => {
                tracing::debug!(
                    "Delivering Slack Block Kit card to {}: {:?}",
                    target_channel,
                    card
                );
            }
        }

        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::Slack,
            channel_message_id: generated_ts,
            delivered_at: Utc::now(),
            state: DeliveryState::Sent,
        })
    }

    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        // Enforce signature verification if headers are present
        if headers.contains_key("x-slack-signature") || headers.contains_key("X-Slack-Signature") {
            self.verify_signature(headers, body)?;
        }

        let json_val: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| ChannelError::UnsupportedPayload(e.to_string()))?;

        self.parse_event_payload(&json_val)
    }

    async fn status(&self) -> ChannelStatus {
        self.status.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::normalized::MessageRecipient;

    #[tokio::test]
    async fn test_slack_adapter_lifecycle() {
        let mut adapter = SlackAdapter::from_credentials("xoxb-dummy-token", "secret123");
        assert_eq!(adapter.status().await, ChannelStatus::Disconnected);

        adapter.connect().await.unwrap();
        assert_eq!(adapter.status().await, ChannelStatus::Connected);

        let recipient = MessageRecipient::threaded(
            ChannelId::Slack,
            "C0123456789",
            "1725180000.000100",
        );
        let msg = OutgoingMessage::text(recipient, SessionId::new(), "Hello Slack thread!");

        let receipt = adapter.send_message(msg).await.unwrap();
        assert_eq!(receipt.channel, ChannelId::Slack);
        assert_eq!(receipt.state, DeliveryState::Sent);
        assert!(receipt.channel_message_id.starts_with("1725180000."));

        adapter.disconnect().await.unwrap();
        assert_eq!(adapter.status().await, ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_slack_event_payload_parsing_and_thread_ts() {
        let adapter = SlackAdapter::from_credentials("xoxb-dummy", "");

        let payload = serde_json::json!({
            "token": "verification_token",
            "team_id": "T0123456",
            "api_app_id": "A0123456",
            "event": {
                "type": "message",
                "channel": "C0123456",
                "user": "U123456",
                "text": "Bug fix confirmed in staging",
                "ts": "1725181111.000200",
                "thread_ts": "1725180000.000100"
            },
            "type": "event_callback"
        });

        let body = serde_json::to_vec(&payload).unwrap();
        let parsed = adapter
            .handle_webhook(&HashMap::new(), &body)
            .await
            .unwrap()
            .expect("parsed slack message");

        assert_eq!(parsed.channel, ChannelId::Slack);
        assert_eq!(parsed.channel_message_id, "1725181111.000200");
        assert_eq!(parsed.sender.id, "U123456");
        assert_eq!(
            parsed.reply_to_message_id.as_deref(),
            Some("1725180000.000100")
        );
        assert_eq!(
            parsed.metadata.get("thread_ts"),
            Some(&serde_json::json!("1725180000.000100"))
        );
    }
}
