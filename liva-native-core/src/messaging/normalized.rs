//! Unified Ingress & Egress Message Normalizer
//!
//! Provides canonical data structures and conversions for multi-channel messaging
//! across Telegram, WhatsApp, Discord, Slack, WebSockets, CLI, and custom channels.

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use uuid::Uuid;

use super::session::SessionId;

/// Unique identifier for messages within the LIVA native engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub Uuid);

impl MessageId {
    /// Generate a new random MessageId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Construct from an existing Uuid.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Access the underlying Uuid.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MessageId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(MessageId)
    }
}

/// Enumeration of all supported messaging and control channels in LIVA.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum ChannelId {
    Telegram,
    WhatsApp,
    Discord,
    Slack,
    WebSocketWidget,
    WebSocketDashboard,
    WebSocketCompanion(String),
    LocalCli,
    Custom(String),
}

impl ChannelId {
    /// Return the canonical string name of the channel.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Telegram => "telegram",
            Self::WhatsApp => "whatsapp",
            Self::Discord => "discord",
            Self::Slack => "slack",
            Self::WebSocketWidget => "websocket_widget",
            Self::WebSocketDashboard => "websocket_dashboard",
            Self::WebSocketCompanion(_) => "websocket_companion",
            Self::LocalCli => "local_cli",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// Whether this channel is a local WebSocket client.
    pub fn is_websocket(&self) -> bool {
        matches!(
            self,
            Self::WebSocketWidget | Self::WebSocketDashboard | Self::WebSocketCompanion(_)
        )
    }

    /// Whether this channel is an external chat platform.
    pub fn is_chat_platform(&self) -> bool {
        matches!(
            self,
            Self::Telegram | Self::WhatsApp | Self::Discord | Self::Slack
        )
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WebSocketCompanion(id) => write!(f, "websocket_companion:{}", id),
            Self::Custom(name) => write!(f, "custom:{}", name),
            other => write!(f, "{}", other.as_str()),
        }
    }
}

/// Sender identity associated with an incoming message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSender {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(default)]
    pub is_bot: bool,
    pub role: SenderRole,
}

impl MessageSender {
    /// Create a standard user sender.
    pub fn user(id: impl Into<String>, display_name: Option<String>) -> Self {
        Self {
            id: id.into(),
            display_name,
            handle: None,
            is_bot: false,
            role: SenderRole::User,
        }
    }

    /// Create an administrator sender.
    pub fn admin(id: impl Into<String>, display_name: Option<String>) -> Self {
        Self {
            id: id.into(),
            display_name,
            handle: None,
            is_bot: false,
            role: SenderRole::Admin,
        }
    }

    /// Create a system / bot sender.
    pub fn system(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: Some("System".to_string()),
            handle: Some("system".to_string()),
            is_bot: true,
            role: SenderRole::System,
        }
    }
}

/// Role classification of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderRole {
    User,
    Admin,
    Companion,
    System,
}

impl SenderRole {
    /// Whether the sender has administrative privilege.
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin | Self::System)
    }
}

/// Content payload format of a normalized message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ContentPayload {
    Text(String),
    RichText {
        text: String,
        entities: Vec<TextEntity>,
    },
    VoiceNote {
        duration_ms: u32,
        mime_type: String,
        sample_rate: u32,
    },
    Image {
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    File {
        filename: String,
        size_bytes: u64,
        mime_type: String,
    },
    Location {
        latitude: f64,
        longitude: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    InteractiveResponse {
        action_id: String,
        value: String,
    },
    Custom(serde_json::Value),
}

impl ContentPayload {
    /// Extract text representation of content, if available.
    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::RichText { text, .. } => Some(text.as_str()),
            Self::Image {
                caption: Some(c), ..
            } => Some(c.as_str()),
            _ => None,
        }
    }

    /// Check if the content is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(t) => t.trim().is_empty(),
            Self::RichText { text, entities } => text.trim().is_empty() && entities.is_empty(),
            _ => false,
        }
    }

    /// Check if content represents audio / voice.
    pub fn is_voice(&self) -> bool {
        matches!(self, Self::VoiceNote { .. })
    }

    /// Check if content represents an image.
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }
}

/// Rich text formatting annotation entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEntity {
    pub offset: usize,
    pub length: usize,
    pub entity_type: TextEntityType,
}

/// Rich text entity types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEntityType {
    Mention,
    Hashtag,
    BotCommand,
    Url,
    Email,
    Bold,
    Italic,
    Code,
    Pre,
}

/// Binary or reference attachment attached to a normalized message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub source: AttachmentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl Attachment {
    /// Construct a new attachment from a local file path.
    pub fn from_local_path(
        filename: impl Into<String>,
        path: PathBuf,
        mime_type: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            size_bytes,
            source: AttachmentSource::LocalPath(path),
            sha256: None,
        }
    }

    /// Construct a new attachment from a remote URL.
    pub fn from_url(
        filename: impl Into<String>,
        url: impl Into<String>,
        mime_type: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            size_bytes,
            source: AttachmentSource::RemoteUrl(url.into()),
            sha256: None,
        }
    }

    /// Construct a new attachment with inline bytes.
    pub fn from_inline_bytes(
        filename: impl Into<String>,
        bytes: Bytes,
        mime_type: impl Into<String>,
    ) -> Self {
        let size_bytes = bytes.len() as u64;
        Self {
            id: Uuid::new_v4().to_string(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            size_bytes,
            source: AttachmentSource::Inline(bytes),
            sha256: None,
        }
    }
}

/// Source location for attachment binaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source_type", content = "location", rename_all = "snake_case")]
pub enum AttachmentSource {
    #[serde(skip)]
    Inline(Bytes),
    RemoteUrl(String),
    LocalPath(PathBuf),
    VaultRef(String),
}

/// Fully normalized incoming message entering the LIVA routing pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub id: MessageId,
    pub channel: ChannelId,
    pub channel_message_id: String,
    pub session_id: SessionId,
    pub sender: MessageSender,
    pub timestamp: DateTime<Utc>,
    pub content: ContentPayload,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl IncomingMessage {
    /// Construct a standard text incoming message.
    pub fn text(
        channel: ChannelId,
        channel_message_id: impl Into<String>,
        session_id: SessionId,
        sender: MessageSender,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            channel,
            channel_message_id: channel_message_id.into(),
            session_id,
            sender,
            timestamp: Utc::now(),
            content: ContentPayload::Text(text.into()),
            attachments: Vec::new(),
            reply_to_message_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Add attachments to the incoming message.
    pub fn with_attachments(mut self, attachments: Vec<Attachment>) -> Self {
        self.attachments = attachments;
        self
    }

    /// Set reply-to parent channel message ID.
    pub fn with_reply_to(mut self, reply_to_id: impl Into<String>) -> Self {
        self.reply_to_message_id = Some(reply_to_id.into());
        self
    }

    /// Attach arbitrary metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Check if the message contains a bot command (e.g. starts with '/').
    pub fn is_command(&self) -> bool {
        if let Some(txt) = self.content.text_content() {
            txt.trim_start().starts_with('/')
        } else {
            false
        }
    }

    /// Extract text summary for logging / prompt indexing.
    pub fn summary(&self) -> String {
        if let Some(txt) = self.content.text_content() {
            let char_count = txt.chars().count();
            if char_count > 80 {
                let truncated: String = txt.chars().take(80).collect();
                format!("{}...", truncated)
            } else {
                txt.to_string()
            }
        } else {
            format!("[Attachment/Media: {} item(s)]", self.attachments.len())
        }
    }
}

/// Fully normalized outgoing message dispatched to channel adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub id: MessageId,
    pub recipient: MessageRecipient,
    pub session_id: SessionId,
    pub content: OutgoingContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_channel_message_id: Option<String>,
    pub urgency: DeliveryUrgency,
    pub created_at: DateTime<Utc>,
}

impl OutgoingMessage {
    /// Construct a standard text outgoing message.
    pub fn text(
        recipient: MessageRecipient,
        session_id: SessionId,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            recipient,
            session_id,
            content: OutgoingContent::Text(text.into()),
            reply_to_channel_message_id: None,
            urgency: DeliveryUrgency::Standard,
            created_at: Utc::now(),
        }
    }

    /// Construct a streaming chunk message.
    pub fn stream_chunk(
        recipient: MessageRecipient,
        session_id: SessionId,
        text: impl Into<String>,
        is_final: bool,
    ) -> Self {
        Self {
            id: MessageId::new(),
            recipient,
            session_id,
            content: OutgoingContent::StreamChunk {
                text: text.into(),
                is_final,
            },
            reply_to_channel_message_id: None,
            urgency: DeliveryUrgency::Immediate,
            created_at: Utc::now(),
        }
    }

    /// Construct a media attachment outgoing message.
    pub fn media(
        recipient: MessageRecipient,
        session_id: SessionId,
        attachment: Attachment,
    ) -> Self {
        Self {
            id: MessageId::new(),
            recipient,
            session_id,
            content: OutgoingContent::Media(attachment),
            reply_to_channel_message_id: None,
            urgency: DeliveryUrgency::Standard,
            created_at: Utc::now(),
        }
    }

    /// Set reply-to parent channel message ID.
    pub fn with_reply_to(mut self, reply_to_id: impl Into<String>) -> Self {
        self.reply_to_channel_message_id = Some(reply_to_id.into());
        self
    }

    /// Set urgency level.
    pub fn with_urgency(mut self, urgency: DeliveryUrgency) -> Self {
        self.urgency = urgency;
        self
    }
}

/// Destination recipient specification for outgoing messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageRecipient {
    pub channel: ChannelId,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

impl MessageRecipient {
    /// Create a direct recipient.
    pub fn direct(channel: ChannelId, target_id: impl Into<String>) -> Self {
        Self {
            channel,
            target_id: target_id.into(),
            thread_id: None,
        }
    }

    /// Create a threaded recipient.
    pub fn threaded(
        channel: ChannelId,
        target_id: impl Into<String>,
        thread_id: impl Into<String>,
    ) -> Self {
        Self {
            channel,
            target_id: target_id.into(),
            thread_id: Some(thread_id.into()),
        }
    }
}

/// Outgoing message content types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum OutgoingContent {
    Text(String),
    StreamChunk {
        text: String,
        is_final: bool,
    },
    Media(Attachment),
    RichCard(serde_json::Value),
}

/// Message delivery urgency priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryUrgency {
    Immediate,
    Standard,
    Background,
}

/// Delivery receipt confirming outbound message handling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryReceipt {
    pub message_id: MessageId,
    pub channel: ChannelId,
    pub channel_message_id: String,
    pub delivered_at: DateTime<Utc>,
    pub state: DeliveryState,
}

/// Outbound message delivery states.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", content = "details", rename_all = "snake_case")]
pub enum DeliveryState {
    Sent,
    Delivered,
    Read,
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_serde_roundtrip() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).expect("serialize MessageId");
        let parsed: MessageId = serde_json::from_str(&json).expect("deserialize MessageId");
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_channel_id_properties() {
        assert!(ChannelId::Telegram.is_chat_platform());
        assert!(ChannelId::WhatsApp.is_chat_platform());
        assert!(ChannelId::Discord.is_chat_platform());
        assert!(ChannelId::Slack.is_chat_platform());
        assert!(!ChannelId::WebSocketWidget.is_chat_platform());
        assert!(ChannelId::WebSocketWidget.is_websocket());
        assert!(ChannelId::WebSocketDashboard.is_websocket());
        assert!(ChannelId::WebSocketCompanion("node_1".into()).is_websocket());
        assert_eq!(ChannelId::Telegram.as_str(), "telegram");
    }

    #[test]
    fn test_incoming_message_builder_and_summary() {
        let session_id = SessionId::new();
        let sender = MessageSender::user("user_123", Some("Alice".to_string()));
        let msg = IncomingMessage::text(
            ChannelId::Telegram,
            "tg_msg_999",
            session_id,
            sender,
            "/help please show available commands",
        )
        .with_metadata("is_group", serde_json::json!(false))
        .with_reply_to("tg_msg_998");

        assert!(msg.is_command());
        assert_eq!(
            msg.summary(),
            "/help please show available commands"
        );
        assert_eq!(msg.channel_message_id, "tg_msg_999");
        assert_eq!(msg.reply_to_message_id.as_deref(), Some("tg_msg_998"));
        assert_eq!(msg.metadata.get("is_group"), Some(&serde_json::json!(false)));
    }

    #[test]
    fn test_outgoing_message_serialization() {
        let session_id = SessionId::new();
        let recipient = MessageRecipient::direct(ChannelId::Discord, "channel_456");
        let out = OutgoingMessage::text(recipient.clone(), session_id, "Hello from LIVA native core!")
            .with_urgency(DeliveryUrgency::Immediate);

        let json = serde_json::to_string(&out).expect("serialize OutgoingMessage");
        let parsed: OutgoingMessage = serde_json::from_str(&json).expect("deserialize OutgoingMessage");

        assert_eq!(out.id, parsed.id);
        assert_eq!(parsed.recipient.target_id, "channel_456");
        assert_eq!(parsed.urgency, DeliveryUrgency::Immediate);
        assert_eq!(
            parsed.content,
            OutgoingContent::Text("Hello from LIVA native core!".into())
        );
    }

    #[test]
    fn test_attachment_creation() {
        let data = Bytes::from_static(b"test audio content");
        let att = Attachment::from_inline_bytes("note.wav", data, "audio/wav");
        assert_eq!(att.filename, "note.wav");
        assert_eq!(att.size_bytes, 18);
        assert_eq!(att.mime_type, "audio/wav");
    }

    #[test]
    fn test_delivery_receipt() {
        let id = MessageId::new();
        let receipt = DeliveryReceipt {
            message_id: id,
            channel: ChannelId::Telegram,
            channel_message_id: "1234".to_string(),
            delivered_at: Utc::now(),
            state: DeliveryState::Delivered,
        };
        let json = serde_json::to_string(&receipt).expect("serialize DeliveryReceipt");
        let parsed: DeliveryReceipt = serde_json::from_str(&json).expect("deserialize DeliveryReceipt");
        assert_eq!(receipt.message_id, parsed.message_id);
        assert_eq!(receipt.state, parsed.state);
    }
}
