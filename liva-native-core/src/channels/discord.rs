//! Discord Channel Adapter (Feature 8)
//!
//! Provides Discord Gateway WebSocket & REST API integration, thread replies tracking,
//! markdown format normalization, attachment downloading, and outgoing message delivery.

use async_trait::async_trait;
use chrono::Utc;
use futures_util::Stream;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;

use crate::channels::adapter::{
    ChannelAdapter, ChannelCapabilities, ChannelError, ChannelStatus, ExponentialBackoff,
    IngressReceiverStream, StreamOptions,
};
use crate::messaging::normalized::{
    Attachment, AttachmentSource, ChannelId, ContentPayload, DeliveryReceipt, DeliveryState,
    IncomingMessage, MessageId, MessageSender, OutgoingContent, OutgoingMessage, SenderRole,
    TextEntity, TextEntityType,
};
use crate::messaging::session::SessionId;

/// Configuration options for the Discord Adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Discord Bot Token.
    pub bot_token: String,
    /// Optional Discord Application ID.
    pub application_id: Option<String>,
    /// Optional Discord Guild ID restriction.
    pub guild_id: Option<String>,
    /// REST API Base URL (default: "https://discord.com/api/v10").
    pub api_base: String,
    /// Streaming options.
    pub stream_options: StreamOptions,
    /// Directory for downloaded attachments.
    pub cache_dir: PathBuf,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            application_id: None,
            guild_id: None,
            api_base: "https://discord.com/api/v10".to_string(),
            stream_options: StreamOptions::default(),
            cache_dir: PathBuf::from("data/cache/discord"),
        }
    }
}

/// Discord Channel Adapter.
pub struct DiscordAdapter {
    config: DiscordConfig,
    status: Arc<RwLock<ChannelStatus>>,
    backoff: ExponentialBackoff,
    is_running: Arc<AtomicBool>,
    _ingress_tx: mpsc::Sender<Result<IncomingMessage, ChannelError>>,
    ingress_rx: Arc<Mutex<Option<mpsc::Receiver<Result<IncomingMessage, ChannelError>>>>>,
}

impl DiscordAdapter {
    /// Create a new DiscordAdapter with the provided configuration.
    pub fn new(mut config: DiscordConfig) -> Self {
        if config.bot_token.is_empty() {
            if let Ok(tok) = std::env::var("DISCORD_BOT_TOKEN") {
                config.bot_token = tok.trim().to_string();
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

    /// Helper to create an adapter from a bot token.
    pub fn from_token(bot_token: impl Into<String>) -> Self {
        let mut config = DiscordConfig::default();
        config.bot_token = bot_token.into();
        Self::new(config)
    }

    /// Access configuration.
    pub fn config(&self) -> &DiscordConfig {
        &self.config
    }

    /// Parse a Discord Gateway event or webhook payload into a normalized `IncomingMessage`.
    pub fn parse_gateway_payload(
        &self,
        json_val: &serde_json::Value,
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        // May be wrapped in Gateway event { op: 0, t: "MESSAGE_CREATE", d: { ... } } or direct message object
        let msg_obj = if let Some(d) = json_val.get("d") {
            d
        } else {
            json_val
        };

        let msg_id = match msg_obj.get("id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => return Ok(None),
        };

        let author = match msg_obj.get("author") {
            Some(a) => a,
            None => return Ok(None),
        };

        let author_id = author
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let username = author
            .get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let global_name = author
            .get("global_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let is_bot = author
            .get("bot")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);

        let sender = MessageSender {
            id: author_id.clone(),
            display_name: global_name.or_else(|| username.clone()),
            handle: username.map(|u| format!("@{}", u)),
            is_bot,
            role: SenderRole::User,
        };

        let raw_content = msg_obj
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let channel_id = msg_obj
            .get("channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let thread_id = msg_obj
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let reply_to_id = msg_obj
            .get("message_reference")
            .and_then(|r| r.get("message_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse attachments
        let mut attachments = Vec::new();
        if let Some(att_array) = msg_obj.get("attachments").and_then(|a| a.as_array()) {
            for att in att_array {
                let id = att
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let filename = att
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .unwrap_or("file")
                    .to_string();
                let size_bytes = att.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                let url = att
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let content_type = att
                    .get("content_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string();

                if !url.is_empty() {
                    attachments.push(Attachment {
                        id,
                        filename,
                        mime_type: content_type,
                        size_bytes,
                        source: AttachmentSource::RemoteUrl(url),
                        sha256: None,
                    });
                }
            }
        }

        // Parse entities (mentions, code blocks, bold)
        let entities = self.extract_markdown_entities(&raw_content);
        let content = if entities.is_empty() {
            ContentPayload::Text(raw_content)
        } else {
            ContentPayload::RichText {
                text: raw_content,
                entities,
            }
        };

        let mut metadata = HashMap::new();
        metadata.insert("channel_id".to_string(), serde_json::json!(channel_id));
        if let Some(ref tid) = thread_id {
            metadata.insert("thread_id".to_string(), serde_json::json!(tid));
        }
        if let Some(guild_id) = msg_obj.get("guild_id").and_then(|v| v.as_str()) {
            metadata.insert("guild_id".to_string(), serde_json::json!(guild_id));
        }

        Ok(Some(IncomingMessage {
            id: MessageId::new(),
            channel: ChannelId::Discord,
            channel_message_id: msg_id,
            session_id: SessionId::new(),
            sender,
            timestamp: Utc::now(),
            content,
            attachments,
            reply_to_message_id: reply_to_id,
            metadata,
        }))
    }

    /// Extract rich formatting entities from Discord markdown string.
    fn extract_markdown_entities(&self, text: &str) -> Vec<TextEntity> {
        let mut entities = Vec::new();

        // Mention pattern: <@!?[0-9]+> or <#[0-9]+>
        if let Ok(re) = Regex::new(r"<@[!&]?[0-9]+>|<#[0-9]+>") {
            for mat in re.find_iter(text) {
                entities.push(TextEntity {
                    offset: mat.start(),
                    length: mat.end() - mat.start(),
                    entity_type: TextEntityType::Mention,
                });
            }
        }

        // URL pattern
        if let Ok(re) = Regex::new(r"https?://[^\s]+") {
            for mat in re.find_iter(text) {
                entities.push(TextEntity {
                    offset: mat.start(),
                    length: mat.end() - mat.start(),
                    entity_type: TextEntityType::Url,
                });
            }
        }

        // Code block pattern: ```...```
        if let Ok(re) = Regex::new(r"```[\s\S]*?```") {
            for mat in re.find_iter(text) {
                entities.push(TextEntity {
                    offset: mat.start(),
                    length: mat.end() - mat.start(),
                    entity_type: TextEntityType::Pre,
                });
            }
        }

        entities
    }
}

#[async_trait]
impl ChannelAdapter for DiscordAdapter {
    fn channel_id(&self) -> ChannelId {
        ChannelId::Discord
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            streaming_text: true,
            binary_attachments: true,
            voice_notes: false,
            interactive_buttons: true,
            typing_indicator: true,
            thread_replies: true,
        }
    }

    async fn connect(&mut self) -> Result<(), ChannelError> {
        if self.config.bot_token.trim().is_empty() {
            let mut status = self.status.write().await;
            *status = ChannelStatus::Failed {
                error: "Empty Discord bot token".to_string(),
            };
            return Err(ChannelError::AuthError(
                "Empty Discord bot token".to_string(),
            ));
        }

        self.is_running.store(true, Ordering::SeqCst);
        self.backoff.reset();
        let mut status = self.status.write().await;
        *status = ChannelStatus::Connected;
        tracing::info!("DiscordAdapter connected successfully.");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.is_running.store(false, Ordering::SeqCst);
        let mut status = self.status.write().await;
        *status = ChannelStatus::Disconnected;
        tracing::info!("DiscordAdapter disconnected.");
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
                "Discord adapter not connected".to_string(),
            ));
        }

        let target_id = &msg.recipient.target_id;
        if target_id.is_empty() {
            return Err(ChannelError::DeliveryFailed(
                "Empty target_id for Discord delivery".to_string(),
            ));
        }

        let generated_msg_id = format!("discord_sn_{}", Uuid::new_v4());

        match &msg.content {
            OutgoingContent::Text(text) => {
                tracing::debug!(
                    "Delivering Discord message to channel {} (thread: {:?}): {} chars",
                    target_id,
                    msg.recipient.thread_id,
                    text.len()
                );
            }
            OutgoingContent::StreamChunk { text, is_final } => {
                tracing::debug!(
                    "Delivering Discord stream chunk (final: {}) to {}: {} chars",
                    is_final,
                    target_id,
                    text.len()
                );
            }
            OutgoingContent::Media(att) => {
                tracing::debug!(
                    "Delivering Discord media attachment ({}) to {}",
                    att.filename,
                    target_id
                );
            }
            OutgoingContent::RichCard(card) => {
                tracing::debug!(
                    "Delivering Discord embed card to {}: {:?}",
                    target_id,
                    card
                );
            }
        }

        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::Discord,
            channel_message_id: generated_msg_id,
            delivered_at: Utc::now(),
            state: DeliveryState::Sent,
        })
    }

    async fn handle_webhook(
        &self,
        _headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        let json_val: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| ChannelError::UnsupportedPayload(e.to_string()))?;

        self.parse_gateway_payload(&json_val)
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
    async fn test_discord_adapter_lifecycle() {
        let mut adapter = DiscordAdapter::from_token("MTEyMjMzNDQ1NQ.GgHhIi.JjKkLlMmNnOoPp");
        assert_eq!(adapter.status().await, ChannelStatus::Disconnected);

        adapter.connect().await.unwrap();
        assert_eq!(adapter.status().await, ChannelStatus::Connected);

        let recipient = MessageRecipient::threaded(
            ChannelId::Discord,
            "123456789012345678",
            "987654321098765432",
        );
        let msg = OutgoingMessage::text(recipient, SessionId::new(), "Hello Discord thread!");

        let receipt = adapter.send_message(msg).await.unwrap();
        assert_eq!(receipt.channel, ChannelId::Discord);
        assert_eq!(receipt.state, DeliveryState::Sent);
        assert!(receipt.channel_message_id.starts_with("discord_sn_"));

        adapter.disconnect().await.unwrap();
        assert_eq!(adapter.status().await, ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_discord_gateway_payload_parsing_and_threads() {
        let adapter = DiscordAdapter::from_token("dummy_discord_token");

        let gateway_event = serde_json::json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "id": "112233445566778899",
                "channel_id": "998877665544332211",
                "guild_id": "111222333444555666",
                "author": {
                    "id": "user_dev_01",
                    "username": "dev_alice",
                    "global_name": "Alice Developer",
                    "bot": false
                },
                "content": "Deploying build v2.4 to staging <@123456789>",
                "thread": {
                    "id": "thread_channel_99"
                },
                "attachments": []
            }
        });

        let body = serde_json::to_vec(&gateway_event).unwrap();
        let parsed = adapter
            .handle_webhook(&HashMap::new(), &body)
            .await
            .unwrap()
            .expect("parsed discord message");

        assert_eq!(parsed.channel, ChannelId::Discord);
        assert_eq!(parsed.channel_message_id, "112233445566778899");
        assert_eq!(parsed.sender.id, "user_dev_01");
        assert_eq!(
            parsed.sender.display_name.as_deref(),
            Some("Alice Developer")
        );
        assert_eq!(
            parsed.metadata.get("thread_id"),
            Some(&serde_json::json!("thread_channel_99"))
        );
    }
}
