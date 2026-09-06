//! Challenger Empirical Security & Adversarial Test Suite for Milestone 2 (Features 5–9)
//! Targets:
//! - `liva-native-core/src/channels/adapter.rs`
//! - `liva-native-core/src/channels/telegram.rs`
//! - `liva-native-core/src/channels/whatsapp.rs`
//! - `liva-native-core/src/channels/discord.rs`
//! - `liva-native-core/src/channels/slack.rs`

use chrono::Utc;
use hkdf::hmac::{Hmac, KeyInit, Mac};
use liva_native_core::channels::adapter::{
    ChannelAdapter, ChannelError, ChannelStatus, ExponentialBackoff,
};
use liva_native_core::channels::discord::DiscordAdapter;
use liva_native_core::channels::slack::SlackAdapter;
use liva_native_core::channels::telegram::{TelegramAdapter, TelegramConfig};
use liva_native_core::channels::whatsapp::WhatsAppAdapter;
use liva_native_core::messaging::normalized::{
    AttachmentSource, ChannelId, ContentPayload, DeliveryState, MessageRecipient, OutgoingMessage,
};
use liva_native_core::messaging::session::SessionId;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// SUITE 1: WEBHOOK SIGNATURE VERIFICATION & TAMPERING ATTACKS
// ============================================================================

#[tokio::test]
async fn test_sec_whatsapp_webhook_signature_valid_vs_tampered_payload() {
    let secret = "correct_meta_app_secret_9988";
    let adapter = WhatsAppAdapter::from_secret(secret);

    let original_payload = serde_json::json!({
        "object": "whatsapp_business_account",
        "entry": [{
            "id": "10001",
            "changes": [{
                "value": {
                    "messaging_product": "whatsapp",
                    "metadata": { "display_phone_number": "1555", "phone_number_id": "100" },
                    "contacts": [{ "profile": { "name": "Bảo Ngọc" }, "wa_id": "84901234567" }],
                    "messages": [{
                        "from": "84901234567",
                        "id": "wamid.VALID_001",
                        "timestamp": "1725180000",
                        "text": { "body": "Chuyển khoản 1,000,000 VND" },
                        "type": "text"
                    }]
                },
                "field": "messages"
            }]
        }]
    });

    let original_bytes = serde_json::to_vec(&original_payload).unwrap();

    // Compute legitimate HMAC signature
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&original_bytes);
    let valid_sig_hex = hex::encode(mac.finalize().into_bytes());

    let valid_headers = HashMap::from([(
        "x-hub-signature-256".to_string(),
        format!("sha256={}", valid_sig_hex),
    )]);

    // 1. Legitimate webhook passes verification
    let res = adapter.handle_webhook(&valid_headers, &original_bytes).await;
    assert!(res.is_ok(), "Valid webhook should pass HMAC verification");
    let msg = res.unwrap().expect("parsed message");
    assert_eq!(msg.channel_message_id, "wamid.VALID_001");
    assert_eq!(msg.content.text_content(), Some("Chuyển khoản 1,000,000 VND"));

    // 2. Tampered payload attack: modify amount from 1M to 100M VND with same header
    let tampered_payload = serde_json::json!({
        "object": "whatsapp_business_account",
        "entry": [{
            "id": "10001",
            "changes": [{
                "value": {
                    "messaging_product": "whatsapp",
                    "metadata": { "display_phone_number": "1555", "phone_number_id": "100" },
                    "contacts": [{ "profile": { "name": "Bảo Ngọc" }, "wa_id": "84901234567" }],
                    "messages": [{
                        "from": "84901234567",
                        "id": "wamid.VALID_001",
                        "timestamp": "1725180000",
                        "text": { "body": "Chuyển khoản 100,000,000 VND" },
                        "type": "text"
                    }]
                },
                "field": "messages"
            }]
        }]
    });
    let tampered_bytes = serde_json::to_vec(&tampered_payload).unwrap();
    let res_tampered = adapter.handle_webhook(&valid_headers, &tampered_bytes).await;
    assert!(
        matches!(res_tampered, Err(ChannelError::InvalidSignature(_))),
        "Tampered payload with original signature MUST be rejected"
    );
}

#[tokio::test]
async fn test_sec_whatsapp_webhook_signature_header_variations_and_malformations() {
    let secret = "test_meta_app_secret";
    let adapter = WhatsAppAdapter::from_secret(secret);
    let body = b"{\"sample\":\"webhook_data\"}";

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let sig_hex = hex::encode(mac.finalize().into_bytes());

    // 1. Missing header completely
    let empty_headers = HashMap::new();
    let res_missing = adapter.handle_webhook(&empty_headers, body).await;
    assert!(matches!(res_missing, Err(ChannelError::InvalidSignature(_))));

    // 2. Case variations of header key (RFC standard)
    for header_name in &["x-hub-signature-256", "X-Hub-Signature-256", "X-HUB-SIGNATURE-256"] {
        let headers = HashMap::from([(header_name.to_string(), format!("sha256={}", sig_hex))]);
        let res = adapter.handle_webhook(&headers, body).await;
        assert!(res.is_ok(), "Header casing {:?} should be supported", header_name);
    }

    // 3. Malformed prefix (e.g. sha1=, sha512=, missing sha256=)
    let malformed_prefixes = vec![
        format!("sha1={}", sig_hex),
        format!("sha512={}", sig_hex),
        format!("bearer {}", sig_hex),
        sig_hex.clone(),
        format!("sha256:{}", sig_hex),
    ];

    for malformed_header in malformed_prefixes {
        let headers = HashMap::from([("x-hub-signature-256".to_string(), malformed_header.clone())]);
        let res = adapter.handle_webhook(&headers, body).await;
        assert!(
            matches!(res, Err(ChannelError::InvalidSignature(_))),
            "Malformed signature format {:?} must fail closed",
            malformed_header
        );
    }

    // 4. Bitflipped signature
    let mut bad_sig = sig_hex.clone();
    bad_sig.replace_range(0..1, if bad_sig.starts_with('a') { "b" } else { "a" });
    let bad_headers = HashMap::from([(
        "x-hub-signature-256".to_string(),
        format!("sha256={}", bad_sig),
    )]);
    let res_bad = adapter.handle_webhook(&bad_headers, body).await;
    assert!(matches!(res_bad, Err(ChannelError::InvalidSignature(_))));

    // 5. Unconfigured app_secret in adapter
    let unconfigured_adapter = WhatsAppAdapter::from_secret("");
    let res_unconfigured = unconfigured_adapter.handle_webhook(&bad_headers, body).await;
    assert!(matches!(res_unconfigured, Err(ChannelError::AuthError(_))));
}

// ============================================================================
// SUITE 2: SLACK REPLAY ATTACKS & TIMESTAMP BOUNDARY TESTING
// ============================================================================

#[tokio::test]
async fn test_sec_slack_timestamp_replay_window_boundary() {
    let secret = "slack_secret_key_boundary_test";
    let adapter = SlackAdapter::from_credentials("xoxb-token", secret);

    let payload = serde_json::json!({
        "event": {
            "type": "message",
            "channel": "C12345",
            "user": "U12345",
            "text": "Critical deployment trigger",
            "ts": "1725180000.000100"
        }
    });
    let body = serde_json::to_vec(&payload).unwrap();
    let now = Utc::now().timestamp();

    // Helper closure to generate Slack headers for a given timestamp
    let make_headers = |ts: i64| -> HashMap<String, String> {
        let ts_str = ts.to_string();
        let sig_basestring = format!("v0:{}:", ts_str);
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(sig_basestring.as_bytes());
        mac.update(&body);
        let sig_hex = hex::encode(mac.finalize().into_bytes());

        HashMap::from([
            ("x-slack-request-timestamp".to_string(), ts_str),
            ("x-slack-signature".to_string(), format!("v0={}", sig_hex)),
        ])
    };

    // 1. Valid timestamp: exactly now -> PASS
    let h_now = make_headers(now);
    let res_now = adapter.handle_webhook(&h_now, &body).await;
    assert!(res_now.is_ok(), "Current timestamp should pass");

    // 2. Valid timestamp: 250 seconds in the past -> PASS (within 300s window)
    let h_past_250 = make_headers(now - 250);
    assert!(adapter.handle_webhook(&h_past_250, &body).await.is_ok());

    // 3. Valid timestamp: 250 seconds in future (clock skew) -> PASS (within 300s window)
    let h_future_250 = make_headers(now + 250);
    assert!(adapter.handle_webhook(&h_future_250, &body).await.is_ok());

    // 4. Boundary: exactly 300 seconds past -> PASS
    let h_past_300 = make_headers(now - 300);
    assert!(adapter.handle_webhook(&h_past_300, &body).await.is_ok());

    // 5. Replay attack: 301 seconds in past -> MUST FAIL
    let h_past_301 = make_headers(now - 301);
    let res_replay = adapter.handle_webhook(&h_past_301, &body).await;
    assert!(
        matches!(res_replay, Err(ChannelError::InvalidSignature(_))),
        "Timestamp 301s old MUST be rejected as replay attack"
    );

    // 6. Extreme past: 1 year old replay attack -> MUST FAIL
    let h_past_1yr = make_headers(now - 31536000);
    assert!(matches!(
        adapter.handle_webhook(&h_past_1yr, &body).await,
        Err(ChannelError::InvalidSignature(_))
    ));

    // 7. Extreme past: Unix Epoch 0 -> MUST FAIL
    let h_epoch_0 = make_headers(0);
    assert!(matches!(
        adapter.handle_webhook(&h_epoch_0, &body).await,
        Err(ChannelError::InvalidSignature(_))
    ));

    // 8. Future replay: 301 seconds into future -> MUST FAIL
    let h_future_301 = make_headers(now + 301);
    assert!(matches!(
        adapter.handle_webhook(&h_future_301, &body).await,
        Err(ChannelError::InvalidSignature(_))
    ));
}

#[tokio::test]
async fn test_sec_slack_signature_and_timestamp_malformed_inputs() {
    let secret = "slack_signing_secret_malformed";
    let adapter = SlackAdapter::from_credentials("xoxb-token", secret);
    let body = b"{\"type\":\"event_callback\"}";

    // 1. Missing timestamp header
    let h1 = HashMap::from([("x-slack-signature".to_string(), "v0=dummy".to_string())]);
    let res1 = adapter.handle_webhook(&h1, body).await;
    assert!(matches!(res1, Err(ChannelError::InvalidSignature(_))));

    // 2. Non-numeric timestamp header
    let malformed_timestamps = vec!["not_a_number", "1725180000.5", "NaN", "null", "--100"];
    for bad_ts in malformed_timestamps {
        let h2 = HashMap::from([
            ("x-slack-signature".to_string(), "v0=dummy".to_string()),
            ("x-slack-request-timestamp".to_string(), bad_ts.to_string()),
        ]);
        let res2 = adapter.handle_webhook(&h2, body).await;
        assert!(
            matches!(res2, Err(ChannelError::InvalidSignature(_))),
            "Non-numeric timestamp {:?} must fail signature verification",
            bad_ts
        );
    }

    // 3. Forged Slack signature
    let now = Utc::now().timestamp();
    let h3 = HashMap::from([
        ("x-slack-request-timestamp".to_string(), now.to_string()),
        (
            "x-slack-signature".to_string(),
            "v0=0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        ),
    ]);
    let res3 = adapter.handle_webhook(&h3, body).await;
    assert!(matches!(res3, Err(ChannelError::InvalidSignature(_))));
}

// ============================================================================
// SUITE 3: CORRUPTED PAYLOADS, TRUNCATION & ADVERSARIAL PAYLOAD FUZZING
// ============================================================================

#[tokio::test]
async fn test_sec_fuzz_invalid_json_bytes_rejected_as_unsupported_payload() {
    let tg = TelegramAdapter::from_token("dummy_token");
    let dc = DiscordAdapter::from_token("dummy_token");
    let sl = SlackAdapter::from_credentials("dummy_token", ""); // empty secret skips sig

    let invalid_json_payloads: Vec<&[u8]> = vec![
        b"",
        b"{",
        b"}",
        b"{\"message\":",
        b"\xFF\xFE\x00\x00BinaryCorruptedData\xDE\xAD\xBE\xEF",
        b"{\"nested\": {\"nested\": {\"broken\": ",
        b"<xml><not>json</not></xml>",
    ];

    for payload in invalid_json_payloads {
        // Telegram
        let tg_res = tg.handle_webhook(&HashMap::new(), payload).await;
        assert!(
            matches!(tg_res, Err(ChannelError::UnsupportedPayload(_))),
            "Telegram must return UnsupportedPayload on malformed JSON: {:?}",
            String::from_utf8_lossy(payload)
        );

        // Discord
        let dc_res = dc.handle_webhook(&HashMap::new(), payload).await;
        assert!(
            matches!(dc_res, Err(ChannelError::UnsupportedPayload(_))),
            "Discord must return UnsupportedPayload on malformed JSON: {:?}",
            String::from_utf8_lossy(payload)
        );

        // Slack
        let sl_res = sl.handle_webhook(&HashMap::new(), payload).await;
        assert!(
            matches!(sl_res, Err(ChannelError::UnsupportedPayload(_))),
            "Slack must return UnsupportedPayload on malformed JSON: {:?}",
            String::from_utf8_lossy(payload)
        );
    }
}

#[tokio::test]
async fn test_sec_fuzz_valid_json_non_event_structures_handled_safely() {
    let tg = TelegramAdapter::from_token("dummy_token");
    let dc = DiscordAdapter::from_token("dummy_token");
    let sl = SlackAdapter::from_credentials("dummy_token", "");

    let non_event_payloads = vec![
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!([1, 2, 3]),
        serde_json::json!({}),
        serde_json::json!({"random_field": "unrelated_value"}),
        serde_json::json!(123456),
        serde_json::json!("plain_string_value"),
    ];

    for json_val in non_event_payloads {
        let body = serde_json::to_vec(&json_val).unwrap();

        let tg_res = tg.handle_webhook(&HashMap::new(), &body).await.unwrap();
        assert!(tg_res.is_none(), "Non-message Telegram JSON should return Ok(None)");

        let dc_res = dc.handle_webhook(&HashMap::new(), &body).await.unwrap();
        assert!(dc_res.is_none(), "Non-message Discord JSON should return Ok(None)");

        let sl_res = sl.handle_webhook(&HashMap::new(), &body).await.unwrap();
        assert!(sl_res.is_none(), "Non-message Slack JSON should return Ok(None)");
    }
}

#[tokio::test]
async fn test_sec_slack_url_verification_and_bot_loop_suppression() {
    let sl = SlackAdapter::from_credentials("xoxb-dummy", "");

    // 1. URL verification challenge during Slack App installation
    let challenge_payload = serde_json::json!({
        "token": "Jhj5dZrVaK7ZwHHjRyZWjbDl",
        "challenge": "3eZbrw1aBm2rZgRNFDxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P",
        "type": "url_verification"
    });
    let body = serde_json::to_vec(&challenge_payload).unwrap();
    let res = sl.handle_webhook(&HashMap::new(), &body).await.unwrap();
    assert!(res.is_none(), "Challenge handshake must return Ok(None)");

    // 2. Bot message suppression (prevent infinite loops with other bots)
    let bot_msg_payload = serde_json::json!({
        "event": {
            "type": "message",
            "subtype": "bot_message",
            "bot_id": "B0123456",
            "channel": "C01",
            "text": "Automated alert",
            "ts": "1725180000.0001"
        }
    });
    let body_bot = serde_json::to_vec(&bot_msg_payload).unwrap();
    let res_bot = sl.handle_webhook(&HashMap::new(), &body_bot).await.unwrap();
    assert!(res_bot.is_none(), "Bot subtype message must be suppressed to avoid loops");

    // 3. Message deleted suppression
    let deleted_msg_payload = serde_json::json!({
        "event": {
            "type": "message",
            "subtype": "message_deleted",
            "channel": "C01",
            "ts": "1725180000.0001"
        }
    });
    let body_del = serde_json::to_vec(&deleted_msg_payload).unwrap();
    let res_del = sl.handle_webhook(&HashMap::new(), &body_del).await.unwrap();
    assert!(res_del.is_none(), "Deleted subtype message must be suppressed");
}

#[tokio::test]
async fn test_sec_telegram_whitelist_access_control_adversarial() {
    let mut config = TelegramConfig::default();
    config.bot_token = "valid_bot_token".to_string();
    config.allowed_user_ids = vec!["111222".to_string(), "333444".to_string()];

    let adapter = TelegramAdapter::new(config);

    // 1. Authorized user (111222)
    let auth_update = serde_json::json!({
        "update_id": 1,
        "message": {
            "message_id": 100,
            "from": { "id": 111222, "first_name": "Admin" },
            "chat": { "id": 111222, "type": "private" },
            "text": "Run maintenance task"
        }
    });
    let body_auth = serde_json::to_vec(&auth_update).unwrap();
    let res_auth = adapter.handle_webhook(&HashMap::new(), &body_auth).await.unwrap();
    assert!(res_auth.is_some(), "Authorized user must be processed");
    assert_eq!(res_auth.unwrap().sender.id, "111222");

    // 2. Unauthorized attacker (999999)
    let unauth_update = serde_json::json!({
        "update_id": 2,
        "message": {
            "message_id": 101,
            "from": { "id": 999999, "first_name": "Attacker" },
            "chat": { "id": 999999, "type": "private" },
            "text": "/delete_all_data"
        }
    });
    let body_unauth = serde_json::to_vec(&unauth_update).unwrap();
    let res_unauth = adapter.handle_webhook(&HashMap::new(), &body_unauth).await.unwrap();
    assert!(
        res_unauth.is_none(),
        "Unauthorized Telegram sender MUST be dropped cleanly without processing"
    );
}

#[tokio::test]
async fn test_sec_discord_markdown_entities_fuzzing() {
    let adapter = DiscordAdapter::from_token("token");

    let adversarial_markdowns = vec![
        ("Hello <@123456789> and <#987654321>", 2),
        ("Check ```rust\nfn main() {}\n``` for details", 1),
        ("Multiple urls https://liva.ai and http://test.org in text", 2),
        ("```Unclosed code block without end", 0),
        ("No entities here just plain text", 0),
        ("<@invalid_non_numeric> <#also_not_num>", 0),
    ];

    for (text, expected_count) in adversarial_markdowns {
        let event = serde_json::json!({
            "d": {
                "id": "msg_001",
                "channel_id": "c1",
                "author": { "id": "u1", "username": "alice", "bot": false },
                "content": text
            }
        });
        let body = serde_json::to_vec(&event).unwrap();
        let msg = adapter.handle_webhook(&HashMap::new(), &body).await.unwrap().unwrap();

        match &msg.content {
            ContentPayload::Text(_) => assert_eq!(expected_count, 0),
            ContentPayload::RichText { entities, .. } => assert_eq!(entities.len(), expected_count),
            _ => panic!("Expected text or richtext"),
        }
    }
}

// ============================================================================
// SUITE 4: EXPONENTIAL BACKOFF RECOVERY & MATHEMATICAL BOUNDARIES
// ============================================================================

#[test]
fn test_sec_exponential_backoff_mathematical_boundaries_and_capping() {
    let initial = 500;
    let max = 15000;
    let backoff = ExponentialBackoff::new(initial, max).with_factor(2.0).with_jitter(0.1);

    // Attempt 0: 500 * 2^0 = 500 + 10% = 550ms
    assert_eq!(backoff.calculate_delay(0), 550);

    // Attempt 1: 500 * 2^1 = 1000 + 10% = 1100ms
    assert_eq!(backoff.calculate_delay(1), 1100);

    // Attempt 2: 500 * 2^2 = 2000 + 10% = 2200ms
    assert_eq!(backoff.calculate_delay(2), 2200);

    // Attempt 3: 500 * 2^3 = 4000 + 10% = 4400ms
    assert_eq!(backoff.calculate_delay(3), 4400);

    // Attempt 4: 500 * 2^4 = 8000 + 10% = 8800ms
    assert_eq!(backoff.calculate_delay(4), 8800);

    // Attempt 5: 500 * 2^5 = 16000 -> Capped at 15000 + 10% = 16500ms
    assert_eq!(backoff.calculate_delay(5), 16500);

    // High attempts (10, 50, 100, 1000): MUST REMAIN CAPPED AT EXACTLY 16500ms
    for attempt in &[10, 50, 100, 1000] {
        assert_eq!(
            backoff.calculate_delay(*attempt),
            16500,
            "Delay for attempt {} must remain strictly capped",
            attempt
        );
    }
}

#[test]
fn test_sec_exponential_backoff_jitter_and_factor_customization() {
    // Zero jitter backoff
    let backoff_no_jitter = ExponentialBackoff::new(100, 1000)
        .with_factor(3.0)
        .with_jitter(0.0);

    assert_eq!(backoff_no_jitter.calculate_delay(0), 100);
    assert_eq!(backoff_no_jitter.calculate_delay(1), 300);
    assert_eq!(backoff_no_jitter.calculate_delay(2), 900);
    assert_eq!(backoff_no_jitter.calculate_delay(3), 1000); // capped at max
}

#[tokio::test]
async fn test_sec_exponential_backoff_multithreaded_concurrency() {
    let backoff = Arc::new(ExponentialBackoff::new(100, 10000).with_jitter(0.1));
    let mut handles = Vec::new();

    // Spawn 50 threads concurrently advancing backoff
    for _ in 0..50 {
        let b = backoff.clone();
        handles.push(tokio::spawn(async move {
            b.next_delay()
        }));
    }

    for h in handles {
        let d = h.await.unwrap();
        assert!(d.as_millis() >= 110);
    }

    assert_eq!(backoff.current_attempt(), 50);

    // Reset backoff
    backoff.reset();
    assert_eq!(backoff.current_attempt(), 0);
}

// ============================================================================
// SUITE 5: DISCONNECTION & STATE MACHINE TRANSITIONS
// ============================================================================

#[tokio::test]
async fn test_sec_adapter_state_machine_disconnect_fail_closed_delivery() {
    let mut adapter = TelegramAdapter::from_token("123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11");

    // 1. Initial status: Disconnected
    assert_eq!(adapter.status().await, ChannelStatus::Disconnected);

    // 2. Attempting to send message while disconnected -> MUST FAIL
    let recipient = MessageRecipient::direct(ChannelId::Telegram, "123456789");
    let out_msg = OutgoingMessage::text(recipient.clone(), SessionId::new(), "Should fail");
    let res_disconn = adapter.send_message(out_msg).await;
    assert!(
        matches!(res_disconn, Err(ChannelError::DeliveryFailed(_))),
        "Sending message while disconnected MUST fail"
    );

    // 3. Connect -> Connected
    adapter.connect().await.expect("connect succeeds");
    assert_eq!(adapter.status().await, ChannelStatus::Connected);

    // 4. Send message while connected -> SUCCESS
    let out_msg_ok = OutgoingMessage::text(recipient.clone(), SessionId::new(), "Should succeed");
    let receipt = adapter.send_message(out_msg_ok).await.expect("send succeeds");
    assert_eq!(receipt.state, DeliveryState::Sent);

    // 5. Disconnect -> Disconnected
    adapter.disconnect().await.expect("disconnect succeeds");
    assert_eq!(adapter.status().await, ChannelStatus::Disconnected);

    // 6. Sending after disconnect -> MUST FAIL AGAIN
    let out_msg_after = OutgoingMessage::text(recipient, SessionId::new(), "Should fail after disconnect");
    let res_after = adapter.send_message(out_msg_after).await;
    assert!(
        matches!(res_after, Err(ChannelError::DeliveryFailed(_))),
        "Sending message after disconnect MUST fail"
    );
}

#[tokio::test]
async fn test_sec_adapter_empty_target_id_rejected() {
    let mut adapter = DiscordAdapter::from_token("token");
    adapter.connect().await.unwrap();

    let empty_recipient = MessageRecipient::direct(ChannelId::Discord, "");
    let msg = OutgoingMessage::text(empty_recipient, SessionId::new(), "Test empty target");

    let res = adapter.send_message(msg).await;
    assert!(
        matches!(res, Err(ChannelError::DeliveryFailed(_))),
        "Empty recipient target_id must be rejected"
    );
}

#[tokio::test]
async fn test_sec_poll_stream_double_consumption_fails_gracefully() {
    let mut adapter = TelegramAdapter::from_token("token");

    // First call consumes the internal receiver stream -> SUCCESS
    let stream1 = adapter.poll_stream().await;
    assert!(stream1.is_ok(), "First poll_stream call should succeed");

    // Second call on the same adapter instance -> MUST return ChannelError::Internal
    let stream2 = adapter.poll_stream().await;
    assert!(
        matches!(stream2, Err(ChannelError::Internal(_))),
        "Double poll_stream call MUST fail gracefully"
    );
}

// ============================================================================
// SUITE 6: UNIFIED NORMALIZATION & ATTACHMENT PIPELINE INTEGRITY
// ============================================================================

#[tokio::test]
async fn test_sec_whatsapp_voice_and_media_normalization() {
    let adapter = WhatsAppAdapter::from_secret("");

    let payload = serde_json::json!({
        "entry": [{
            "changes": [{
                "value": {
                    "contacts": [{ "profile": { "name": "Alice" }, "wa_id": "84901234567" }],
                    "messages": [{
                        "from": "84901234567",
                        "id": "wamid.AUDIO_001",
                        "timestamp": "1725180000",
                        "type": "voice",
                        "voice": {
                            "id": "media_voice_id_123",
                            "mime_type": "audio/ogg; codecs=opus"
                        }
                    }]
                }
            }]
        }]
    });

    let msg = adapter.parse_webhook_payload(&payload).unwrap().expect("parsed voice message");

    assert_eq!(msg.channel, ChannelId::WhatsApp);
    assert_eq!(msg.channel_message_id, "wamid.AUDIO_001");
    assert!(msg.content.is_voice());
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].id, "media_voice_id_123");
    assert_eq!(msg.attachments[0].filename, "wa_voice_wamid.AUDIO_001.ogg");
}

#[tokio::test]
async fn test_sec_telegram_multi_resolution_photo_picks_largest() {
    let adapter = TelegramAdapter::from_token("token");

    let photo_update = serde_json::json!({
        "update_id": 10,
        "message": {
            "message_id": 888,
            "from": { "id": 12345, "first_name": "Bob" },
            "chat": { "id": 12345, "type": "private" },
            "caption": "High res diagram",
            "photo": [
                { "file_id": "thumb_id_small", "width": 90, "height": 90, "file_size": 1024 },
                { "file_id": "medium_id", "width": 320, "height": 320, "file_size": 10240 },
                { "file_id": "highest_res_file_id", "width": 1280, "height": 1280, "file_size": 102400 }
            ]
        }
    });

    let body = serde_json::to_vec(&photo_update).unwrap();
    let parsed = adapter.handle_webhook(&HashMap::new(), &body).await.unwrap().expect("parsed photo");

    assert_eq!(parsed.attachments.len(), 1);
    assert_eq!(parsed.attachments[0].id, "highest_res_file_id");
    assert_eq!(parsed.attachments[0].size_bytes, 102400);
    assert_eq!(parsed.attachments[0].filename, "photo_888.jpg");

    match parsed.content {
        ContentPayload::Image { width, height, caption, .. } => {
            assert_eq!(width, Some(1280));
            assert_eq!(height, Some(1280));
            assert_eq!(caption.as_deref(), Some("High res diagram"));
        }
        _ => panic!("Expected Image payload"),
    }
}

#[tokio::test]
async fn test_sec_telegram_callback_query_normalization() {
    let adapter = TelegramAdapter::from_token("token");

    let cq_update = serde_json::json!({
        "update_id": 11,
        "callback_query": {
            "id": "cq_action_confirm_pay",
            "from": { "id": 98765, "first_name": "Charlie", "username": "charlie_boss" },
            "message": {
                "message_id": 555,
                "chat": { "id": 98765, "type": "private" },
                "text": "Do you confirm payment?"
            },
            "data": "CONFIRM:ORDER_99"
        }
    });

    let body = serde_json::to_vec(&cq_update).unwrap();
    let parsed = adapter.handle_webhook(&HashMap::new(), &body).await.unwrap().expect("parsed callback query");

    assert_eq!(parsed.channel_message_id, "cq_cq_action_confirm_pay");
    assert_eq!(parsed.reply_to_message_id.as_deref(), Some("555"));
    assert_eq!(parsed.sender.id, "98765");
    assert_eq!(parsed.sender.handle.as_deref(), Some("@charlie_boss"));

    match parsed.content {
        ContentPayload::InteractiveResponse { action_id, value } => {
            assert_eq!(action_id, "cq_action_confirm_pay");
            assert_eq!(value, "CONFIRM:ORDER_99");
        }
        _ => panic!("Expected InteractiveResponse payload"),
    }
}

#[tokio::test]
async fn test_sec_whatsapp_interactive_button_reply_normalization() {
    let adapter = WhatsAppAdapter::from_secret("");

    let payload = serde_json::json!({
        "entry": [{
            "changes": [{
                "value": {
                    "contacts": [{ "profile": { "name": "David" }, "wa_id": "84911223344" }],
                    "messages": [{
                        "from": "84911223344",
                        "id": "wamid.BTN_001",
                        "timestamp": "1725180000",
                        "type": "interactive",
                        "interactive": {
                            "type": "button_reply",
                            "button_reply": {
                                "id": "btn_approve_tx",
                                "title": "Phê duyệt giao dịch"
                            }
                        }
                    }]
                }
            }]
        }]
    });

    let msg = adapter.parse_webhook_payload(&payload).unwrap().expect("parsed interactive message");

    assert_eq!(msg.channel, ChannelId::WhatsApp);
    assert_eq!(msg.channel_message_id, "wamid.BTN_001");
    match msg.content {
        ContentPayload::InteractiveResponse { action_id, value } => {
            assert_eq!(action_id, "btn_approve_tx");
            assert_eq!(value, "Phê duyệt giao dịch");
        }
        _ => panic!("Expected InteractiveResponse payload"),
    }
}

#[tokio::test]
async fn test_sec_slack_file_attachments_download_urls() {
    let sl = SlackAdapter::from_credentials("xoxb-dummy", "");

    let payload = serde_json::json!({
        "event": {
            "type": "message",
            "channel": "C999",
            "user": "U999",
            "text": "Here is the audit report and dataset",
            "ts": "1725189999.000100",
            "files": [
                {
                    "id": "F001",
                    "name": "audit_report.pdf",
                    "size": 524288,
                    "mimetype": "application/pdf",
                    "url_private_download": "https://files.slack.com/files-pri/T01-F001/download/audit_report.pdf"
                },
                {
                    "id": "F002",
                    "name": "metrics.csv",
                    "size": 10240,
                    "mimetype": "text/csv",
                    "url_private": "https://files.slack.com/files-pri/T01-F002/metrics.csv"
                }
            ]
        }
    });

    let body = serde_json::to_vec(&payload).unwrap();
    let msg = sl.handle_webhook(&HashMap::new(), &body).await.unwrap().expect("parsed slack message");

    assert_eq!(msg.attachments.len(), 2);
    assert_eq!(msg.attachments[0].filename, "audit_report.pdf");
    assert_eq!(msg.attachments[0].size_bytes, 524288);
    assert_eq!(msg.attachments[0].mime_type, "application/pdf");
    assert_eq!(
        msg.attachments[0].source,
        AttachmentSource::RemoteUrl("https://files.slack.com/files-pri/T01-F001/download/audit_report.pdf".into())
    );

    assert_eq!(msg.attachments[1].filename, "metrics.csv");
    assert_eq!(
        msg.attachments[1].source,
        AttachmentSource::RemoteUrl("https://files.slack.com/files-pri/T01-F002/metrics.csv".into())
    );
}

#[tokio::test]
async fn test_sec_discord_guild_and_thread_metadata() {
    let dc = DiscordAdapter::from_token("tok");

    let event = serde_json::json!({
        "d": {
            "id": "msg_dc_full",
            "channel_id": "channel_general_123",
            "guild_id": "guild_corp_999",
            "author": { "id": "user_dev", "username": "alice", "global_name": "Alice Developer", "bot": false },
            "content": "Running benchmark test in thread",
            "thread": { "id": "thread_perf_456" }
        }
    });

    let body = serde_json::to_vec(&event).unwrap();
    let msg = dc.handle_webhook(&HashMap::new(), &body).await.unwrap().expect("parsed discord");

    assert_eq!(msg.metadata.get("channel_id"), Some(&serde_json::json!("channel_general_123")));
    assert_eq!(msg.metadata.get("guild_id"), Some(&serde_json::json!("guild_corp_999")));
    assert_eq!(msg.metadata.get("thread_id"), Some(&serde_json::json!("thread_perf_456")));
    assert_eq!(msg.sender.display_name.as_deref(), Some("Alice Developer"));
    assert_eq!(msg.sender.handle.as_deref(), Some("@alice"));
}

#[tokio::test]
async fn test_sec_channel_status_display_and_equality() {
    let s_conn = ChannelStatus::Connected;
    let s_disc = ChannelStatus::Disconnected;
    let s_rec = ChannelStatus::Reconnecting { attempt: 5, next_retry_ms: 3200 };
    let s_fail = ChannelStatus::Failed { error: "Network reset".into() };

    assert_eq!(s_conn.to_string(), "connected");
    assert_eq!(s_disc.to_string(), "disconnected");
    assert_eq!(s_rec.to_string(), "reconnecting (attempt 5, next in 3200ms)");
    assert_eq!(s_fail.to_string(), "failed: Network reset");

    assert_ne!(s_conn, s_disc);
    assert_ne!(s_conn, s_rec);
    assert_ne!(s_conn, s_fail);
}
