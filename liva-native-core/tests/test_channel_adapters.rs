//! E2E Test Suite: Multi-Channel Adapters (Telegram, WhatsApp, Discord, Slack)
//! Covers Feature 5 (Multi-Channel Base Adapter Trait), Feature 6 (Telegram), Feature 7 (WhatsApp),
//! Feature 8 (Discord), and Feature 9 (Slack)
//! Tiers 1 & 2 Test Suite

use hkdf::hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Channel Adapter Trait & Supporting Types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelId {
    Telegram,
    WhatsApp,
    Discord,
    Slack,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    pub streaming_text: bool,
    pub binary_attachments: bool,
    pub voice_notes: bool,
    pub interactive_buttons: bool,
    pub typing_indicator: bool,
    pub thread_replies: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChannelStatus {
    Connected,
    Disconnected,
    Reconnecting {
        attempt: u32,
        next_retry_ms: u64,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Authentication error: {0}")]
    AuthError(String),
    #[error("Message delivery failed: {0}")]
    DeliveryFailed(String),
    #[error("Webhook signature invalid: {0}")]
    InvalidSignature(String),
    #[error("Channel rate limited; retry after {0}s")]
    RateLimited(u64),
    #[error("Unsupported payload type: {0}")]
    UnsupportedPayload(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub id: String,
    pub channel: ChannelId,
    pub channel_message_id: String,
    pub sender_id: String,
    pub text: String,
    pub attachments_count: usize,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub id: String,
    pub channel: ChannelId,
    pub target_id: String,
    pub text: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryReceipt {
    pub message_id: String,
    pub channel: ChannelId,
    pub channel_message_id: String,
    pub state: DeliveryState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Sent,
    Delivered,
    Failed(String),
}

#[async_trait::async_trait]
pub trait ChannelAdapter: Send + Sync + 'static {
    fn channel_id(&self) -> ChannelId;
    fn capabilities(&self) -> ChannelCapabilities;
    async fn connect(&mut self) -> Result<(), ChannelError>;
    async fn disconnect(&mut self) -> Result<(), ChannelError>;
    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError>;
    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<Option<IncomingMessage>, ChannelError>;
    async fn status(&self) -> ChannelStatus;
}

// ============================================================================
// Exponential Backoff Calculator
// ============================================================================

pub struct BackoffCalculator {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub factor: f64,
    pub jitter: f64,
}

impl BackoffCalculator {
    pub fn new(initial_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            initial_delay_ms,
            max_delay_ms,
            factor: 2.0,
            jitter: 0.1,
        }
    }

    pub fn calculate_delay(&self, attempt: u32) -> u64 {
        let base = (self.initial_delay_ms as f64) * self.factor.powi(attempt as i32);
        let capped = base.min(self.max_delay_ms as f64);
        let jitter_amount = capped * self.jitter;
        // Deterministic middle point for verification
        (capped + jitter_amount).round() as u64
    }
}

// ============================================================================
// Mock Channel Adapters for Opaque-Box E2E Testing
// ============================================================================

pub struct MockTelegramAdapter {
    status: ChannelStatus,
    bot_token: String,
}

impl MockTelegramAdapter {
    pub fn new(bot_token: &str) -> Self {
        Self {
            status: ChannelStatus::Disconnected,
            bot_token: bot_token.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for MockTelegramAdapter {
    fn channel_id(&self) -> ChannelId {
        ChannelId::Telegram
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            streaming_text: true,
            binary_attachments: true,
            voice_notes: true,
            interactive_buttons: true,
            typing_indicator: true,
            thread_replies: false,
        }
    }

    async fn connect(&mut self) -> Result<(), ChannelError> {
        if self.bot_token.is_empty() {
            return Err(ChannelError::AuthError("Empty Telegram bot token".to_string()));
        }
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError> {
        if self.status != ChannelStatus::Connected {
            return Err(ChannelError::DeliveryFailed("Telegram adapter not connected".to_string()));
        }
        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::Telegram,
            channel_message_id: format!("tg_msg_{}", Uuid::new_v4()),
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

        if let Some(msg_obj) = json_val.get("message") {
            let text = msg_obj.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let from_id = msg_obj.get("from").and_then(|f| f.get("id")).and_then(|id| id.as_i64()).unwrap_or(0).to_string();
            let msg_id = msg_obj.get("message_id").and_then(|id| id.as_i64()).unwrap_or(0).to_string();

            return Ok(Some(IncomingMessage {
                id: Uuid::new_v4().to_string(),
                channel: ChannelId::Telegram,
                channel_message_id: msg_id,
                sender_id: from_id,
                text,
                attachments_count: 0,
                thread_id: None,
            }));
        }

        Ok(None)
    }

    async fn status(&self) -> ChannelStatus {
        self.status.clone()
    }
}

pub struct MockWhatsAppAdapter {
    status: ChannelStatus,
    app_secret: String,
}

impl MockWhatsAppAdapter {
    pub fn new(app_secret: &str) -> Self {
        Self {
            status: ChannelStatus::Connected,
            app_secret: app_secret.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for MockWhatsAppAdapter {
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
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError> {
        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::WhatsApp,
            channel_message_id: format!("wamid.{}", Uuid::new_v4()),
            state: DeliveryState::Delivered,
        })
    }

    async fn handle_webhook(
        &self,
        headers: &HashMap<String, String>,
        body: &[u8],
    ) -> Result<Option<IncomingMessage>, ChannelError> {
        // Verify X-Hub-Signature-256 header: sha256=<hex_hmac>
        let sig_header = headers.get("x-hub-signature-256")
            .or_else(|| headers.get("X-Hub-Signature-256"))
            .ok_or_else(|| ChannelError::InvalidSignature("Missing signature header".to_string()))?;

        let expected_prefix = "sha256=";
        if !sig_header.starts_with(expected_prefix) {
            return Err(ChannelError::InvalidSignature("Invalid signature header format".to_string()));
        }
        let given_sig = &sig_header[expected_prefix.len()..];

        let mut mac = HmacSha256::new_from_slice(self.app_secret.as_bytes()).unwrap();
        mac.update(body);
        let computed_sig = hex::encode(mac.finalize().into_bytes());

        if given_sig != computed_sig {
            return Err(ChannelError::InvalidSignature("Signature mismatch".to_string()));
        }

        let json_val: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| ChannelError::UnsupportedPayload(e.to_string()))?;

        // Extract WhatsApp message
        if let Some(entry) = json_val.get("entry").and_then(|e| e.get(0)) {
            if let Some(changes) = entry.get("changes").and_then(|c| c.get(0)) {
                if let Some(messages) = changes.get("value").and_then(|v| v.get("messages")).and_then(|m| m.get(0)) {
                    let from = messages.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let text = messages.get("text").and_then(|t| t.get("body")).and_then(|b| b.as_str()).unwrap_or("").to_string();
                    let wamid = messages.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    return Ok(Some(IncomingMessage {
                        id: Uuid::new_v4().to_string(),
                        channel: ChannelId::WhatsApp,
                        channel_message_id: wamid,
                        sender_id: from,
                        text,
                        attachments_count: 0,
                        thread_id: None,
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn status(&self) -> ChannelStatus {
        self.status.clone()
    }
}

pub struct MockDiscordAdapter {
    status: ChannelStatus,
}

impl MockDiscordAdapter {
    pub fn new() -> Self {
        Self { status: ChannelStatus::Connected }
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for MockDiscordAdapter {
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
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError> {
        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::Discord,
            channel_message_id: format!("discord_sn_{}", Uuid::new_v4()),
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

        let content = json_val.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let author_id = json_val.get("author").and_then(|a| a.get("id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let id = json_val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let thread_id = json_val.get("thread").and_then(|t| t.get("id")).and_then(|v| v.as_str()).map(|s| s.to_string());

        Ok(Some(IncomingMessage {
            id: Uuid::new_v4().to_string(),
            channel: ChannelId::Discord,
            channel_message_id: id,
            sender_id: author_id,
            text: content,
            attachments_count: 0,
            thread_id,
        }))
    }

    async fn status(&self) -> ChannelStatus {
        self.status.clone()
    }
}

pub struct MockSlackAdapter {
    status: ChannelStatus,
}

impl MockSlackAdapter {
    pub fn new() -> Self {
        Self { status: ChannelStatus::Connected }
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for MockSlackAdapter {
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
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<DeliveryReceipt, ChannelError> {
        Ok(DeliveryReceipt {
            message_id: msg.id,
            channel: ChannelId::Slack,
            channel_message_id: format!("1725180000.{}", rand::random::<u32>()),
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

        if let Some(event) = json_val.get("event") {
            let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let user = event.get("user").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ts = event.get("ts").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let thread_ts = event.get("thread_ts").and_then(|v| v.as_str()).map(|s| s.to_string());

            return Ok(Some(IncomingMessage {
                id: Uuid::new_v4().to_string(),
                channel: ChannelId::Slack,
                channel_message_id: ts,
                sender_id: user,
                text,
                attachments_count: 0,
                thread_id: thread_ts,
            }));
        }

        Ok(None)
    }

    async fn status(&self) -> ChannelStatus {
        self.status.clone()
    }
}

// ============================================================================
// Tier 1: Feature Coverage Tests
// ============================================================================

#[tokio::test]
async fn test_tier1_channel_adapter_lifecycles() {
    let mut tg = MockTelegramAdapter::new("valid_bot_token_123");
    assert_eq!(tg.status().await, ChannelStatus::Disconnected);

    tg.connect().await.unwrap();
    assert_eq!(tg.status().await, ChannelStatus::Connected);

    let out_msg = OutgoingMessage {
        id: "msg-001".to_string(),
        channel: ChannelId::Telegram,
        target_id: "123456789".to_string(),
        text: "Hello from LIVA core".to_string(),
        thread_id: None,
    };

    let receipt = tg.send_message(out_msg).await.unwrap();
    assert_eq!(receipt.channel, ChannelId::Telegram);
    assert_eq!(receipt.state, DeliveryState::Sent);

    tg.disconnect().await.unwrap();
    assert_eq!(tg.status().await, ChannelStatus::Disconnected);
}

#[tokio::test]
async fn test_tier1_channel_capabilities_matrix() {
    let tg = MockTelegramAdapter::new("token");
    let wa = MockWhatsAppAdapter::new("secret");
    let dc = MockDiscordAdapter::new();
    let sl = MockSlackAdapter::new();

    // Telegram: voice notes yes, threads no
    assert!(tg.capabilities().voice_notes);
    assert!(!tg.capabilities().thread_replies);

    // WhatsApp: voice notes yes, streaming no
    assert!(wa.capabilities().voice_notes);
    assert!(!wa.capabilities().streaming_text);

    // Discord: thread replies yes, voice notes no
    assert!(dc.capabilities().thread_replies);
    assert!(!dc.capabilities().voice_notes);

    // Slack: thread replies yes, streaming yes
    assert!(sl.capabilities().thread_replies);
    assert!(sl.capabilities().streaming_text);
}

#[tokio::test]
async fn test_tier1_whatsapp_webhook_parsing_and_signature() {
    let secret = "whatsapp_app_secret_key_123";
    let wa = MockWhatsAppAdapter::new(secret);

    let payload = serde_json::json!({
        "object": "whatsapp_business_account",
        "entry": [{
            "id": "WHATSAPP_BUSINESS_ACCOUNT_ID",
            "changes": [{
                "value": {
                    "messaging_product": "whatsapp",
                    "metadata": {"display_phone_number": "15550234567", "phone_number_id": "100000000000000"},
                    "messages": [{
                        "from": "84901234567",
                        "id": "wamid.HBgLM...",
                        "timestamp": "1725180000",
                        "text": {"body": "Báo cáo chi tiêu tháng 8"},
                        "type": "text"
                    }]
                },
                "field": "messages"
            }]
        }]
    });

    let body_bytes = serde_json::to_vec(&payload).unwrap();

    // Compute valid HMAC-SHA256 signature
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&body_bytes);
    let sig_hex = hex::encode(mac.finalize().into_bytes());

    let headers = HashMap::from([
        ("x-hub-signature-256".to_string(), format!("sha256={}", sig_hex)),
    ]);

    let incoming = wa.handle_webhook(&headers, &body_bytes).await.unwrap().expect("parsed message");
    assert_eq!(incoming.channel, ChannelId::WhatsApp);
    assert_eq!(incoming.sender_id, "84901234567");
    assert_eq!(incoming.text, "Báo cáo chi tiêu tháng 8");
}

#[tokio::test]
async fn test_tier1_discord_and_slack_thread_handling() {
    let dc = MockDiscordAdapter::new();
    let sl = MockSlackAdapter::new();

    // Discord message in thread
    let dc_body = serde_json::to_vec(&serde_json::json!({
        "id": "11223344",
        "content": "Deploying build v2.4",
        "author": {"id": "user_dev_01"},
        "thread": {"id": "thread_channel_99"}
    })).unwrap();

    let dc_msg = dc.handle_webhook(&HashMap::new(), &dc_body).await.unwrap().unwrap();
    assert_eq!(dc_msg.thread_id, Some("thread_channel_99".to_string()));

    // Slack message in thread
    let sl_body = serde_json::to_vec(&serde_json::json!({
        "event": {
            "type": "message",
            "user": "U123456",
            "text": "Bug fix confirmed in staging",
            "ts": "1725181111.000200",
            "thread_ts": "1725180000.000100"
        }
    })).unwrap();

    let sl_msg = sl.handle_webhook(&HashMap::new(), &sl_body).await.unwrap().unwrap();
    assert_eq!(sl_msg.thread_id, Some("1725180000.000100".to_string()));
}

// ============================================================================
// Tier 2: Boundary Value Analysis & Error Handling
// ============================================================================

#[tokio::test]
async fn test_tier2_exponential_backoff_delays() {
    let calc = BackoffCalculator::new(100, 5000);

    // Initial delay: 100 * 2^0 + 10% = 110ms
    let d0 = calc.calculate_delay(0);
    assert_eq!(d0, 110);

    // Attempt 1: 100 * 2^1 + 10% = 220ms
    let d1 = calc.calculate_delay(1);
    assert_eq!(d1, 220);

    // Attempt 2: 100 * 2^2 + 10% = 440ms
    let d2 = calc.calculate_delay(2);
    assert_eq!(d2, 440);

    // Attempt 3: 100 * 2^3 + 10% = 880ms
    let d3 = calc.calculate_delay(3);
    assert_eq!(d3, 880);

    // High attempt: should cap at max_delay_ms (5000) + 10% jitter = 5500ms
    let d10 = calc.calculate_delay(10);
    assert_eq!(d10, 5500);
}

#[tokio::test]
async fn test_tier2_whatsapp_tampered_signature_fail_closed() {
    let secret = "correct_secret";
    let wa = MockWhatsAppAdapter::new(secret);
    let body = b"{\"sample\":\"payload\"}";

    // Forged signature
    let headers = HashMap::from([
        ("x-hub-signature-256".to_string(), "sha256=0000000000000000000000000000000000000000000000000000000000000000".to_string()),
    ]);

    let res = wa.handle_webhook(&headers, body).await;
    assert_eq!(res.unwrap_err(), ChannelError::InvalidSignature("Signature mismatch".to_string()));
}

#[tokio::test]
async fn test_tier2_telegram_empty_token_fails_connect() {
    let mut tg = MockTelegramAdapter::new("");
    let res = tg.connect().await;
    assert_eq!(res.unwrap_err(), ChannelError::AuthError("Empty Telegram bot token".to_string()));
}

#[tokio::test]
async fn test_tier2_disconnected_adapter_send_fails_gracefully() {
    let tg = MockTelegramAdapter::new("token");
    // Adapter is disconnected
    let out_msg = OutgoingMessage {
        id: "msg-1".to_string(),
        channel: ChannelId::Telegram,
        target_id: "123".to_string(),
        text: "test".to_string(),
        thread_id: None,
    };

    let res = tg.send_message(out_msg).await;
    assert_eq!(res.unwrap_err(), ChannelError::DeliveryFailed("Telegram adapter not connected".to_string()));
}

// ============================================================================
// Native channels::* Integration Test Suite
// ============================================================================

#[tokio::test]
async fn test_tier1_native_telegram_adapter_integration() {
    use liva_native_core::channels::adapter::ChannelAdapter as NativeChannelAdapter;
    use liva_native_core::channels::telegram::TelegramAdapter;
    use liva_native_core::messaging::normalized::{
        ChannelId as CoreChannelId, MessageRecipient, OutgoingMessage as CoreOutgoingMessage,
    };
    use liva_native_core::messaging::session::SessionId;

    let mut tg = TelegramAdapter::from_token("123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11");
    tg.connect().await.expect("native tg connect");

    let recipient = MessageRecipient::direct(CoreChannelId::Telegram, "123456789");
    let out = CoreOutgoingMessage::text(recipient, SessionId::new(), "Hello native telegram");
    let receipt = tg.send_message(out).await.expect("native tg send");
    assert_eq!(receipt.channel, CoreChannelId::Telegram);

    tg.disconnect().await.expect("native tg disconnect");
}

#[tokio::test]
async fn test_tier1_native_whatsapp_adapter_signature_and_normalization() {
    use liva_native_core::channels::adapter::ChannelAdapter as NativeChannelAdapter;
    use liva_native_core::channels::whatsapp::WhatsAppAdapter;
    use liva_native_core::messaging::normalized::ChannelId as CoreChannelId;

    let secret = "native_secret_123";
    let wa = WhatsAppAdapter::from_secret(secret);

    let payload = serde_json::json!({
        "object": "whatsapp_business_account",
        "entry": [{
            "id": "10000",
            "changes": [{
                "value": {
                    "messaging_product": "whatsapp",
                    "metadata": { "display_phone_number": "1555", "phone_number_id": "100" },
                    "contacts": [{ "profile": { "name": "Alice" }, "wa_id": "84901234567" }],
                    "messages": [{
                        "from": "84901234567",
                        "id": "wamid.123",
                        "timestamp": "1725180000",
                        "text": { "body": "Native WhatsApp message" },
                        "type": "text"
                    }]
                },
                "field": "messages"
            }]
        }]
    });
    let body = serde_json::to_vec(&payload).unwrap();

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&body);
    let sig_hex = hex::encode(mac.finalize().into_bytes());

    let headers = HashMap::from([(
        "x-hub-signature-256".to_string(),
        format!("sha256={}", sig_hex),
    )]);

    let incoming = wa.handle_webhook(&headers, &body).await.unwrap().expect("parsed");
    assert_eq!(incoming.channel, CoreChannelId::WhatsApp);
    assert_eq!(incoming.content.text_content(), Some("Native WhatsApp message"));
}

#[tokio::test]
async fn test_tier1_native_discord_adapter_threads_and_mentions() {
    use liva_native_core::channels::adapter::ChannelAdapter as NativeChannelAdapter;
    use liva_native_core::channels::discord::DiscordAdapter;
    use liva_native_core::messaging::normalized::ChannelId as CoreChannelId;

    let dc = DiscordAdapter::from_token("bot_tok");

    let event = serde_json::json!({
        "d": {
            "id": "msg_dc_001",
            "channel_id": "chan_001",
            "author": { "id": "u1", "username": "alice", "bot": false },
            "content": "Check out <@123456789> in thread",
            "thread": { "id": "thread_dc_99" }
        }
    });
    let body = serde_json::to_vec(&event).unwrap();

    let msg = dc.handle_webhook(&HashMap::new(), &body).await.unwrap().unwrap();
    assert_eq!(msg.channel, CoreChannelId::Discord);
    assert_eq!(msg.metadata.get("thread_id"), Some(&serde_json::json!("thread_dc_99")));
}

#[tokio::test]
async fn test_tier1_native_slack_adapter_blocks_and_thread_ts() {
    use liva_native_core::channels::adapter::ChannelAdapter as NativeChannelAdapter;
    use liva_native_core::channels::slack::SlackAdapter;
    use liva_native_core::messaging::normalized::ChannelId as CoreChannelId;

    let sl = SlackAdapter::from_credentials("xoxb-tok", "");

    let payload = serde_json::json!({
        "event": {
            "type": "message",
            "channel": "C01",
            "user": "U01",
            "text": "Slack message in thread",
            "ts": "1725181111.000200",
            "thread_ts": "1725180000.000100"
        }
    });
    let body = serde_json::to_vec(&payload).unwrap();

    let msg = sl.handle_webhook(&HashMap::new(), &body).await.unwrap().unwrap();
    assert_eq!(msg.channel, CoreChannelId::Slack);
    assert_eq!(msg.reply_to_message_id.as_deref(), Some("1725180000.000100"));
}

