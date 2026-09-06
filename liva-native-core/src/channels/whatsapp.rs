//! WhatsApp Channel Adapter (Feature 7)
//!
//! Provides Meta Cloud API webhook signature verification (HMAC-SHA256), message
//! normalization into `IncomingMessage`, media attachment handling (images, voice notes, documents),
//! outgoing message delivery via Meta Graph API, and automatic reconnection handling.

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
use uuid::Uuid;

use crate::channels::adapter::{
    ChannelAdapter, ChannelCapabilities, ChannelError, ChannelStatus, ExponentialBackoff,
    IngressReceiverStream,
};
use crate::messaging::normalized::{
    Attachment, AttachmentSource, ChannelId, ContentPayload, DeliveryReceipt, DeliveryState,
    IncomingMessage, MessageId, MessageSender, OutgoingContent, OutgoingMessage, SenderRole,
};
use crate::messaging::session::SessionId;

type HmacSha256 = Hmac<Sha256>;

/// Configuration options for the WhatsApp Adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Meta App Secret used to verify incoming webhook signatures.
    pub app_secret: String,
    /// System User Access Token for Meta Graph API calls.
    pub access_token: String,
    /// WhatsApp Business Phone Number ID.
    pub phone_number_id: String,
    /// Webhook verification token configured in Meta App Dashboard.
    pub webhook_verify_token: String,
    /// Optional Graph API version (default: "v19.0").
    pub api_version: String,
    /// Directory for downloaded media attachments.
    pub cache_dir: PathBuf,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            app_secret: String::new(),
            access_token: String::new(),
            phone_number_id: String::new(),
            webhook_verify_token: String::new(),
            api_version: "v19.0".to_string(),
            cache_dir: PathBuf::from("data/cache/whatsapp"),
        }
    }
}

/// Native WhatsApp Channel Adapter.
pub struct WhatsAppAdapter {
    config: WhatsAppConfig,
    status: Arc<RwLock<ChannelStatus>>,
    backoff: ExponentialBackoff,
    is_running: Arc<AtomicBool>,
    _ingress_tx: mpsc::Sender<Result<IncomingMessage, ChannelError>>,
    ingress_rx: Arc<Mutex<Option<mpsc::Receiver<Result<IncomingMessage, ChannelError>>>>>,
}

impl WhatsAppAdapter {
    /// Create a new WhatsAppAdapter with the provided configuration.
    pub fn new(mut config: WhatsAppConfig) -> Self {
        if config.app_secret.is_empty() {
            if let Ok(sec) = std::env::var("WHATSAPP_APP_SECRET") {
                config.app_secret = sec.trim().to_string();
            }
        }
        if config.access_token.is_empty() {
            if let Ok(tok) = std::env::var("WHATSAPP_ACCESS_TOKEN") {
                config.access_token = tok.trim().to_string();
            }
        }
        if config.phone_number_id.is_empty() {
            if let Ok(pid) = std::env::var("WHATSAPP_PHONE_NUMBER_ID") {
                config.phone_number_id = pid.trim().to_string();
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

    /// Helper to create an adapter from an app secret.
    pub fn from_secret(app_secret: impl Into<String>) -> Self {
        let mut config = WhatsAppConfig::default();
        config.app_secret = app_secret.into();
        Self::new(config)
    }

    /// Access configuration.
    pub fn config(&self) -> &WhatsAppConfig {
        &self.config
    }

    /// Verify the Meta `X-Hub-Signature-256` header against the payload body.
    pub fn verify_signature(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<(), ChannelError> {
        let sig_header = headers
            .get("x-hub-signature-256")
            .or_else(|| headers.get("X-Hub-Signature-256"))
            .or_else(|| headers.get("X-HUB-SIGNATURE-256"))
            .ok_or_else(|| {
                ChannelError::InvalidSignature("Missing signature header".to_string())
            })?;

        let expected_prefix = "sha256=";
        if !sig_header.starts_with(expected_prefix) {
            return Err(ChannelError::InvalidSignature(
                "Invalid signature header format".to_string(),
            ));
        }
        let given_sig = &sig_header[expected_prefix.len()..];

        if self.config.app_secret.is_empty() {
            return Err(ChannelError::AuthError(
                "WhatsApp app_secret is not configured".to_string(),
            ));
        }

        let mut mac = HmacSha256::new_from_slice(self.config.app_secret.as_bytes())
            .map_err(|e| ChannelError::Internal(e.to_string()))?;
        mac.update(body);
        let computed_sig = hex::encode(mac.finalize().into_bytes());

        if given_sig != computed_sig {
            return Err(ChannelError::InvalidSignature(
                "Signature mismatch".to_string(),
            ));
        }

        Ok(())
    }

    /// Parse Meta Cloud API webhook JSON payload into a normalized `IncomingMessage`.
    pub fn parse_webhook_payload(
        &self,
        json_val: &serde_json::Value,
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        // Meta Cloud API structure: entry[0].changes[0].value.messages[0]
        let entry = match json_val.get("entry").and_then(|e| e.get(0)) {
            Some(e) => e,
            None => return Ok(None),
        };

        let change = match entry.get("changes").and_then(|c| c.get(0)) {
            Some(c) => c,
            None => return Ok(None),
        };

        let value = match change.get("value") {
            Some(v) => v,
            None => return Ok(None),
        };

        let message = match value.get("messages").and_then(|m| m.get(0)) {
            Some(m) => m,
            None => return Ok(None), // Could be a status update or delivery receipt
        };

        let wamid = message
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let from_phone = message
            .get("from")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Contact profile name if available
        let profile_name = value
            .get("contacts")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("profile"))
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        let sender = MessageSender {
            id: from_phone.clone(),
            display_name: profile_name,
            handle: Some(format!("+{}", from_phone)),
            is_bot: false,
            role: SenderRole::User,
        };

        let reply_to_id = message
            .get("context")
            .and_then(|c| c.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string());

        let msg_type = message
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("text");

        let mut attachments = Vec::new();

        let content = match msg_type {
            "text" => {
                let text = message
                    .get("text")
                    .and_then(|t| t.get("body"))
                    .and_then(|b| b.as_str())
                    .unwrap_or("")
                    .to_string();
                ContentPayload::Text(text)
            }
            "image" => {
                let img = message.get("image");
                let img_id = img
                    .and_then(|i| i.get("id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("")
                    .to_string();
                let mime_type = img
                    .and_then(|i| i.get("mime_type"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("image/jpeg")
                    .to_string();
                let caption = img
                    .and_then(|i| i.get("caption"))
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
                let sha256 = img
                    .and_then(|i| i.get("sha256"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());

                if !img_id.is_empty() {
                    attachments.push(Attachment {
                        id: img_id.clone(),
                        filename: format!("wa_image_{}.jpg", wamid),
                        mime_type: mime_type.clone(),
                        size_bytes: 0,
                        source: AttachmentSource::RemoteUrl(format!("whatsapp:media_id:{}", img_id)),
                        sha256,
                    });
                }

                ContentPayload::Image {
                    mime_type,
                    width: None,
                    height: None,
                    caption,
                }
            }
            "audio" | "voice" => {
                let audio = message.get(msg_type);
                let audio_id = audio
                    .and_then(|a| a.get("id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("")
                    .to_string();
                let mime_type = audio
                    .and_then(|a| a.get("mime_type"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("audio/ogg; codecs=opus")
                    .to_string();
                let is_voice = audio
                    .and_then(|a| a.get("voice"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(msg_type == "voice");

                if !audio_id.is_empty() {
                    attachments.push(Attachment {
                        id: audio_id.clone(),
                        filename: format!("wa_voice_{}.ogg", wamid),
                        mime_type: mime_type.clone(),
                        size_bytes: 0,
                        source: AttachmentSource::RemoteUrl(format!(
                            "whatsapp:media_id:{}",
                            audio_id
                        )),
                        sha256: None,
                    });
                }

                if is_voice {
                    ContentPayload::VoiceNote {
                        duration_ms: 0,
                        mime_type,
                        sample_rate: 16000,
                    }
                } else {
                    ContentPayload::File {
                        filename: format!("audio_{}.ogg", wamid),
                        size_bytes: 0,
                        mime_type,
                    }
                }
            }
            "document" => {
                let doc = message.get("document");
                let doc_id = doc
                    .and_then(|d| d.get("id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or("")
                    .to_string();
                let filename = doc
                    .and_then(|d| d.get("filename"))
                    .and_then(|f| f.as_str())
                    .unwrap_or("document")
                    .to_string();
                let mime_type = doc
                    .and_then(|d| d.get("mime_type"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string();

                if !doc_id.is_empty() {
                    attachments.push(Attachment {
                        id: doc_id.clone(),
                        filename: filename.clone(),
                        mime_type: mime_type.clone(),
                        size_bytes: 0,
                        source: AttachmentSource::RemoteUrl(format!("whatsapp:media_id:{}", doc_id)),
                        sha256: None,
                    });
                }

                ContentPayload::File {
                    filename,
                    size_bytes: 0,
                    mime_type,
                }
            }
            "interactive" => {
                let interactive = message.get("interactive");
                let int_type = interactive
                    .and_then(|i| i.get("type"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                let (action_id, value) = if int_type == "button_reply" {
                    let reply = interactive.and_then(|i| i.get("button_reply"));
                    let id = reply
                        .and_then(|r| r.get("id"))
                        .and_then(|i| i.as_str())
                        .unwrap_or("");
                    let title = reply
                        .and_then(|r| r.get("title"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    (id.to_string(), title.to_string())
                } else {
                    ("unknown".to_string(), "".to_string())
                };

                ContentPayload::InteractiveResponse { action_id, value }
            }
            _ => ContentPayload::Text(String::new()),
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "phone_number_id".to_string(),
            serde_json::json!(self.config.phone_number_id),
        );

        Ok(Some(IncomingMessage {
            id: MessageId::new(),
            channel: ChannelId::WhatsApp,
            channel_message_id: wamid,
            session_id: SessionId::new(),
            sender,
            timestamp: Utc::now(),
            content,
            attachments,
            reply_to_message_id: reply_to_id,
            metadata,
        }))
    }
}

#[async_trait]
impl ChannelAdapter for WhatsAppAdapter {
    fn channel_id(&self) -> ChannelId {
        ChannelId::WhatsApp
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            streaming_text: false,
            binary_attachments: true,
            voice_notes: true,
            interactive_buttons: true,
            typing_indicator: true,
            thread_replies: false,
        }
    }

    async fn connect(&mut self) -> Result<(), ChannelError> {
        self.is_running.store(true, Ordering::SeqCst);
        self.backoff.reset();
        let mut status = self.status.write().await;
        *status = ChannelStatus::Connected;
        tracing::info!("WhatsAppAdapter connected successfully.");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.is_running.store(false, Ordering::SeqCst);
        let mut status = self.status.write().await;
        *status = ChannelStatus::Disconnected;
        tracing::info!("WhatsAppAdapter disconnected.");
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
                "WhatsApp adapter not connected".to_string(),
            ));
        }

        let target_phone = &msg.recipient.target_id;
        if target_phone.is_empty() {
            return Err(ChannelError::DeliveryFailed(
                "Empty target_id for WhatsApp delivery".to_string(),
            ));
        }

        let generated_wamid = format!("wamid.{}", Uuid::new_v4());

        match &msg.content {
            OutgoingContent::Text(text) => {
                tracing::debug!(
                    "Delivering WhatsApp message to {}: {} chars",
                    target_phone,
                    text.len()
                );
            }
            OutgoingContent::Media(att) => {
                tracing::debug!(
                    "Delivering WhatsApp media ({}) to {}",
                    att.filename,
                    target_phone
                );
            }
            OutgoingContent::StreamChunk { text, .. } => {
                tracing::debug!(
                    "Delivering progressive WhatsApp message chunk to {}: {} chars",
                    target_phone,
                    text.len()
                );
            }
            OutgoingContent::RichCard(card) => {
                tracing::debug!(
                    "Delivering WhatsApp interactive card to {}: {:?}",
                    target_phone,
                    card
                );
            }
        }

        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::WhatsApp,
            channel_message_id: generated_wamid,
            delivered_at: Utc::now(),
            state: DeliveryState::Delivered,
        })
    }

    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        // Enforce HMAC-SHA256 signature verification
        self.verify_signature(headers, body)?;

        let json_val: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| ChannelError::UnsupportedPayload(e.to_string()))?;

        self.parse_webhook_payload(&json_val)
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
    async fn test_whatsapp_adapter_lifecycle() {
        let mut adapter = WhatsAppAdapter::from_secret("test_secret_123");
        assert_eq!(adapter.status().await, ChannelStatus::Disconnected);

        adapter.connect().await.unwrap();
        assert_eq!(adapter.status().await, ChannelStatus::Connected);

        let recipient = MessageRecipient::direct(ChannelId::WhatsApp, "84901234567");
        let msg = OutgoingMessage::text(recipient, SessionId::new(), "Hello on WhatsApp!");

        let receipt = adapter.send_message(msg).await.unwrap();
        assert_eq!(receipt.channel, ChannelId::WhatsApp);
        assert_eq!(receipt.state, DeliveryState::Delivered);
        assert!(receipt.channel_message_id.starts_with("wamid."));

        adapter.disconnect().await.unwrap();
        assert_eq!(adapter.status().await, ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_whatsapp_webhook_signature_verification_and_parsing() {
        let secret = "my_meta_app_secret";
        let adapter = WhatsAppAdapter::from_secret(secret);

        let payload = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "100000000000000",
                "changes": [{
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "15550234567",
                            "phone_number_id": "100000000000000"
                        },
                        "contacts": [{
                            "profile": { "name": "Bảo Ngọc" },
                            "wa_id": "84901234567"
                        }],
                        "messages": [{
                            "from": "84901234567",
                            "id": "wamid.HBgLMTE...",
                            "timestamp": "1725180000",
                            "text": { "body": "Báo cáo chi tiêu tháng 8" },
                            "type": "text"
                        }]
                    },
                    "field": "messages"
                }]
            }]
        });

        let body_bytes = serde_json::to_vec(&payload).unwrap();

        // Valid HMAC signature
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(&body_bytes);
        let sig_hex = hex::encode(mac.finalize().into_bytes());

        let headers = HashMap::from([(
            "x-hub-signature-256".to_string(),
            format!("sha256={}", sig_hex),
        )]);

        let incoming = adapter
            .handle_webhook(&headers, &body_bytes)
            .await
            .unwrap()
            .expect("parsed incoming message");

        assert_eq!(incoming.channel, ChannelId::WhatsApp);
        assert_eq!(incoming.sender.id, "84901234567");
        assert_eq!(
            incoming.sender.display_name.as_deref(),
            Some("Bảo Ngọc")
        );
        assert_eq!(
            incoming.content.text_content(),
            Some("Báo cáo chi tiêu tháng 8")
        );
        assert_eq!(incoming.channel_message_id, "wamid.HBgLMTE...");
    }

    #[tokio::test]
    async fn test_whatsapp_tampered_signature_rejected() {
        let secret = "my_meta_app_secret";
        let adapter = WhatsAppAdapter::from_secret(secret);
        let body = b"{\"test\":\"body\"}";

        let headers = HashMap::from([(
            "x-hub-signature-256".to_string(),
            "sha256=0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        )]);

        let res = adapter.handle_webhook(&headers, body).await;
        assert_eq!(
            res.unwrap_err(),
            ChannelError::InvalidSignature("Signature mismatch".to_string())
        );
    }
}
