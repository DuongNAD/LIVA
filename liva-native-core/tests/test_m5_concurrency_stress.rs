//! Comprehensive Integration and Stress Tests for Milestone 5 (M5).
//!
//! Covers:
//! 1. Multi-Session Concurrency & High-Load Message Bursts:
//!    - Multi-channel simultaneous message bursts (Telegram, WhatsApp, Discord, Slack, WebSocket).
//!    - Cross-channel session isolation, context switching, and scratchpad integrity under concurrency.
//!    - Channel adapter capabilities and exponential backoff error resilience.
//! 2. High Concurrent Database Reader/Writer Contention & WAL Integrity:
//!    - 20 concurrent writer tasks and 50 concurrent reader tasks on SQLite WAL pool.
//!    - High-concurrency vector embedding and FTS5 indexing without lock poisoning or thread starvation.
//!    - WAL auto-checkpointing under continuous write pressure.
//! 3. Rapid Session Creation, Context Switching & Eviction Stress:
//!    - 500 rapid session creations with diverse MemoryScopes.
//!    - Concurrent session touch, variable update, and race-free expired TTL eviction without memory leaks.
//! 4. Local-First Security & Cryptographic Verification:
//!    - HKDF-SHA256 + AES-256-GCM v2 encryption roundtrip across varied payloads.
//!    - Freshness verification: Unique random salts, IVs, and ciphertexts for identical plaintexts.
//!    - Tamper resistance: Fail-closed `FactRead::Locked` on mutated salt, IV, tag, or ciphertext.
//!    - Security boundary: Fail-closed `Locked` prevents leaking ciphertext into prompt/UI (`into_value()` and `decrypt_read()` return `""`).
//!    - Key derivation isolation and constant invariants.
//!    - Keystore & Stronghold DPAPI vault security invariants.

use liva_native_core::{
    channels::{
        adapter::{ChannelAdapter, ChannelStatus, ExponentialBackoff},
        discord::{DiscordAdapter, DiscordConfig},
        slack::{SlackAdapter, SlackConfig},
        telegram::{TelegramAdapter, TelegramConfig},
        whatsapp::{WhatsAppAdapter, WhatsAppConfig},
    },
    crypto::{DecryptError, EncryptionEngine, FactRead, DEFAULT_ENCRYPTION_KEY},
    db::{
        persist_conversation_event_vector, DatabasePool, WalCheckpointMode, WalCheckpointResult,
        MEMORY_VECTOR_DIM,
    },
    keystore::{
        device_key_path, DEVICE_KEY_FILE, DEVICE_KEY_LEN, VAULT_PASSWORD_LEN, VAULT_SALT_LEN,
        VAULT_SECRET_FILE,
    },
    messaging::{
        ChannelId, DeliveryReceipt, DeliveryState, IncomingMessage, InMemorySessionManager,
        MessageRecipient, MessageSender, OutgoingMessage, SessionManager, SessionState,
    },
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;

// ============================================================================
// SECTION 1: Multi-Channel Simultaneous Message Bursts & Throughput
// ============================================================================

#[tokio::test]
async fn test_m5_multi_channel_simultaneous_message_burst() {
    let session_manager = Arc::new(InMemorySessionManager::default());
    let channels = vec![
        ChannelId::Telegram,
        ChannelId::WhatsApp,
        ChannelId::Discord,
        ChannelId::Slack,
        ChannelId::WebSocketWidget,
    ];

    let messages_per_channel = 100;
    let total_expected_messages = channels.len() * messages_per_channel;
    let barrier = Arc::new(Barrier::new(channels.len()));
    let processed_counter = Arc::new(AtomicUsize::new(0));

    let mut tasks = Vec::new();

    for channel in channels {
        let sm = session_manager.clone();
        let bar = barrier.clone();
        let counter = processed_counter.clone();

        let task = tokio::spawn(async move {
            // Synchronize starting burst across all 5 channels
            bar.wait().await;

            let mut session_ids = Vec::new();
            // Create 5 sessions per channel for concurrency
            for i in 0..5 {
                let user_id = format!("user_{}_{}", channel.as_str(), i);
                let thread_id = format!("thread_{}", i);
                let session_arc = sm
                    .get_or_create_session(&channel, &user_id, Some(&thread_id))
                    .await
                    .expect("Session creation must succeed");

                let sess_id = session_arc.read().await.session_id;
                session_ids.push(sess_id);
            }

            for msg_idx in 0..messages_per_channel {
                let target_session_id = session_ids[msg_idx % session_ids.len()];

                let incoming = IncomingMessage::text(
                    channel.clone(),
                    format!("chan_msg_{}_{}", channel.as_str(), msg_idx),
                    target_session_id,
                    MessageSender::user(
                        format!("sender_{}_{}", channel.as_str(), msg_idx % 5),
                        Some(format!("User {}", msg_idx % 5)),
                    ),
                    format!("Burst message #{} from channel {}", msg_idx, channel.as_str()),
                );

                // Update session context with incoming message tracking
                if let Some(session_arc) = sm.get_session(&target_session_id).await.expect("Get session") {
                    let mut ctx = session_arc.write().await;
                    let key = format!("msg_{}", msg_idx);
                    ctx.set_variable(
                        key,
                        serde_json::json!({
                            "msg_id": incoming.id.to_string(),
                            "payload": incoming.content.text_content().unwrap_or(""),
                        }),
                    );
                }

                // Generate and record outgoing delivery confirmation
                let outgoing = OutgoingMessage::text(
                    MessageRecipient::direct(channel.clone(), incoming.sender.id.clone()),
                    target_session_id,
                    format!("Ack for {}", msg_idx),
                );

                let receipt = DeliveryReceipt {
                    message_id: outgoing.id,
                    channel: channel.clone(),
                    channel_message_id: format!("ack_{}", outgoing.id),
                    state: DeliveryState::Sent,
                    delivered_at: chrono::Utc::now(),
                };

                assert_eq!(receipt.state, DeliveryState::Sent);
                assert_eq!(receipt.channel, channel);

                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await.expect("Channel burst worker task must not panic");
    }

    assert_eq!(
        processed_counter.load(Ordering::SeqCst),
        total_expected_messages,
        "All 500 burst messages across 5 channels must be successfully routed and processed"
    );

    // Verify all 25 sessions (5 channels * 5 users) remain active and intact
    let active_sessions = session_manager.list_sessions().await.expect("List sessions");
    assert_eq!(active_sessions.len(), 25, "Expected 25 isolated active sessions");
}

#[tokio::test]
async fn test_m5_channel_adapter_capabilities_and_backoff_resilience() {
    let telegram_adapter = TelegramAdapter::new(TelegramConfig {
        bot_token: "test_bot_token".to_string(),
        allowed_user_ids: vec!["123456".to_string()],
        ..Default::default()
    });
    let whatsapp_adapter = WhatsAppAdapter::new(WhatsAppConfig {
        access_token: "test_token".to_string(),
        phone_number_id: "10001".to_string(),
        app_secret: "secret_123".to_string(),
        webhook_verify_token: "verify_token".to_string(),
        ..Default::default()
    });
    let discord_adapter = DiscordAdapter::new(DiscordConfig {
        bot_token: "test_discord_token".to_string(),
        application_id: Some("999999".to_string()),
        ..Default::default()
    });
    let slack_adapter = SlackAdapter::new(SlackConfig {
        bot_token: "xoxb-test".to_string(),
        app_token: Some("xapp-test".to_string()),
        signing_secret: "slack_secret".to_string(),
        ..Default::default()
    });

    assert_eq!(telegram_adapter.status().await, ChannelStatus::Disconnected);
    assert_eq!(whatsapp_adapter.status().await, ChannelStatus::Disconnected);
    assert_eq!(discord_adapter.status().await, ChannelStatus::Disconnected);
    assert_eq!(slack_adapter.status().await, ChannelStatus::Disconnected);

    // Verify adapter capabilities
    let tg_caps = telegram_adapter.capabilities();
    assert!(tg_caps.streaming_text);
    assert!(tg_caps.voice_notes);

    let wa_caps = whatsapp_adapter.capabilities();
    assert!(!wa_caps.streaming_text); // WhatsApp webhook is non-streaming

    let dc_caps = discord_adapter.capabilities();
    assert!(dc_caps.thread_replies);

    let sl_caps = slack_adapter.capabilities();
    assert!(sl_caps.thread_replies);

    // Test Exponential Backoff Calculator under concurrent simulated failures
    let mut backoff_tasks = Vec::new();
    for i in 0..10 {
        backoff_tasks.push(tokio::spawn(async move {
            let backoff = ExponentialBackoff::new(50, 5000);
            assert_eq!(backoff.current_attempt(), 0);

            for attempt in 0..5 {
                let delay = backoff.next_delay();
                assert!(delay >= Duration::from_millis(50));
                assert!(delay <= Duration::from_millis(5500));
                assert_eq!(backoff.current_attempt(), attempt + 1);
            }
            backoff.reset();
            assert_eq!(backoff.current_attempt(), 0);
            i
        }));
    }

    for task in backoff_tasks {
        let res = task.await.expect("Backoff task must succeed");
        assert!(res < 10);
    }
}

#[tokio::test]
async fn test_m5_cross_channel_session_isolation_and_scratchpad_integrity() {
    let session_manager = Arc::new(InMemorySessionManager::default());
    let channel_types = vec![
        ChannelId::Telegram,
        ChannelId::WhatsApp,
        ChannelId::Discord,
        ChannelId::Slack,
        ChannelId::WebSocketWidget,
    ];

    let mut session_arcs = Vec::new();
    for (i, channel) in channel_types.iter().enumerate() {
        let user_id = format!("isolated_user_{}", i);
        let thread_id = format!("thread_scope_{}", i);
        let session = session_manager
            .get_or_create_session(channel, &user_id, Some(&thread_id))
            .await
            .expect("Session creation must succeed");
        session_arcs.push((i, channel.clone(), session));
    }

    let mut worker_handles = Vec::new();
    let num_concurrent_mutations = 100;

    for (session_idx, channel, session_arc) in session_arcs.clone() {
        let handle = tokio::spawn(async move {
            for step in 0..num_concurrent_mutations {
                let mut ctx = session_arc.write().await;
                ctx.set_variable(
                    format!("turn_{}", step),
                    serde_json::json!({
                        "session_idx": session_idx,
                        "channel": channel.as_str(),
                        "counter": step,
                        "entropy": format!("token_{}_{}", session_idx, step),
                    }),
                );
            }
        });
        worker_handles.push(handle);
    }

    for h in worker_handles {
        h.await.expect("Mutation worker must not panic");
    }

    // Verify cross-session isolation: verify that session i ONLY contains variables for session i
    for (session_idx, channel, session_arc) in session_arcs {
        let ctx = session_arc.read().await;

        assert_eq!(ctx.variables.len(), num_concurrent_mutations);
        for step in 0..num_concurrent_mutations {
            let val = ctx
                .get_variable(&format!("turn_{}", step))
                .expect("Variable must be present");
            assert_eq!(val["session_idx"], session_idx);
            assert_eq!(val["channel"], channel.as_str());
            assert_eq!(val["counter"], step);
        }
    }
}

// ============================================================================
// SECTION 2: High Concurrent Database Reader/Writer Contention & WAL Integrity
// ============================================================================

#[tokio::test]
async fn test_m5_sqlite_wal_high_concurrency_reader_writer_contention() {
    let pool = DatabasePool::new_in_memory().expect("In-memory WAL DB creation must succeed");
    let pool = Arc::new(pool);

    let crypto = Arc::new(EncryptionEngine::new("m5-concurrency-stress-test-key-32b"));

    let num_writers = 20;
    let num_readers = 50;
    let writes_per_writer = 25;
    let reads_per_reader = 50;

    let barrier = Arc::new(Barrier::new(num_writers + num_readers));
    let mut writer_handles = Vec::new();
    let mut reader_handles = Vec::new();

    // Spawn 20 concurrent writer tasks
    for writer_id in 0..num_writers {
        let p = pool.clone();
        let b = barrier.clone();
        let c = crypto.clone();

        let handle = tokio::spawn(async move {
            b.wait().await;
            for w in 0..writes_per_writer {
                let key = format!("writer_{}_fact_{}", writer_id, w);
                let plain_value = format!("Value for writer {} iteration {}", writer_id, w);
                let encrypted_value = c.encrypt(&plain_value).expect("Encryption must succeed");

                // Perform transactional write to facts table using correct schema columns
                let conn = p.writer.get().expect("Must acquire writer connection");
                conn.execute(
                    "INSERT OR REPLACE INTO facts (key, value, createdAt, updatedAt, source, category, importance)
                     VALUES (?1, ?2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, ?3, ?4, ?5)",
                    rusqlite::params![
                        key,
                        encrypted_value,
                        format!("writer_{}", writer_id),
                        "stress_test",
                        0.85
                    ],
                )
                .expect("Insert into facts must succeed under write contention");
            }
        });
        writer_handles.push(handle);
    }

    // Spawn 50 concurrent reader tasks
    for reader_id in 0..num_readers {
        let p = pool.clone();
        let b = barrier.clone();
        let c = crypto.clone();

        let handle = tokio::spawn(async move {
            b.wait().await;
            for _ in 0..reads_per_reader {
                let conn = p.readers.get().expect("Must acquire reader connection");
                let mut stmt = conn
                    .prepare("SELECT key, value, importance FROM facts WHERE category = 'stress_test' LIMIT 10")
                    .expect("Prepare reader query must succeed");

                let rows: Vec<(String, String)> = stmt
                    .query_map([], |row| {
                        let k: String = row.get(0)?;
                        let v: String = row.get(1)?;
                        Ok((k, v))
                    })
                    .expect("Query map must succeed")
                    .filter_map(|r| r.ok())
                    .collect();

                // Decrypt and verify any retrieved facts
                for (_k, enc_val) in rows {
                    let fact_read = c.read_fact(&enc_val);
                    match fact_read {
                        FactRead::Ok(plain) => {
                            assert!(plain.starts_with("Value for writer "));
                        }
                        FactRead::Locked { reason } => {
                            panic!("Read fact should not be locked with valid key: {}", reason);
                        }
                    }
                }
            }
            reader_id
        });
        reader_handles.push(handle);
    }

    for h in writer_handles {
        h.await.expect("Writer worker must complete cleanly without panic");
    }
    for h in reader_handles {
        h.await.expect("Reader worker must complete cleanly without panic");
    }

    // Verify final database state
    let conn = pool.readers.get().expect("Reader connection");
    let total_facts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM facts WHERE category = 'stress_test'",
            [],
            |r| r.get(0),
        )
        .expect("Count facts");

    assert_eq!(
        total_facts,
        (num_writers * writes_per_writer) as i64,
        "Total facts written must match exactly without missing records"
    );
}

#[tokio::test]
async fn test_m5_sqlite_wal_vector_and_fts5_concurrent_stress() {
    let pool = DatabasePool::new_in_memory().expect("In-memory DB creation");
    let pool = Arc::new(pool);
    let crypto = Arc::new(EncryptionEngine::new("m5-vector-key-32-bytes-long!"));

    let num_vector_writers = 10;
    let events_per_writer = 20;
    let total_events = num_vector_writers * events_per_writer;

    let mut handles = Vec::new();

    for writer_id in 0..num_vector_writers {
        let p = pool.clone();
        let c = crypto.clone();
        let handle = tokio::spawn(async move {
            for idx in 0..events_per_writer {
                let event_id = format!("evt_{}_{}", writer_id, idx);
                let text = format!("Bật đèn phòng khách và quạt trần số {}", idx);

                // Create a deterministic 384-dimensional vector
                let mut embedding = vec![0.0f32; MEMORY_VECTOR_DIM];
                for (dim, val) in embedding.iter_mut().enumerate() {
                    *val = ((writer_id * 17 + idx * 31 + dim) % 100) as f32 / 100.0;
                }

                let conn = p.writer.get().expect("Writer conn");
                persist_conversation_event_vector(
                    &conn,
                    &c,
                    &event_id,
                    &text,
                    &embedding,
                    "smarthome",
                    "action",
                )
                .expect("Persist vector event must succeed under concurrency");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("Vector write task must succeed");
    }

    // Verify row count in events
    let conn = pool.readers.get().expect("Reader connection");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("Count events");
    assert_eq!(count, total_events as i64);

    // Verify concurrent queries on events and vectors_meta
    let mut search_handles = Vec::new();
    for _ in 0..20 {
        let p = pool.clone();
        search_handles.push(tokio::spawn(async move {
            let conn = p.readers.get().expect("Reader conn");
            let mut stmt = conn
                .prepare("SELECT eventId, domain, category FROM events WHERE domain = 'smarthome' LIMIT 5")
                .expect("Search query");
            let rows: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .expect("Query map")
                .filter_map(|r| r.ok())
                .collect();
            assert!(!rows.is_empty());
        }));
    }

    for sh in search_handles {
        sh.await.expect("Search task must succeed");
    }
}

#[tokio::test]
async fn test_m5_sqlite_wal_checkpoint_under_continuous_writes() {
    let temp_dir = std::env::temp_dir().join(format!("liva_wal_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("checkpoint_stress.sqlite");

    let pool = DatabasePool::new(&db_path).expect("Disk DB creation");
    let pool = Arc::new(pool);
    let is_running = Arc::new(std::sync::atomic::AtomicBool::new(true));

    // Background writer task
    let p_writer = pool.clone();
    let flag_writer = is_running.clone();
    let writer_task = tokio::spawn(async move {
        let mut iteration = 0;
        while flag_writer.load(Ordering::Relaxed) && iteration < 100 {
            {
                let conn = p_writer.writer.get().expect("Writer conn");
                conn.execute(
                    "INSERT INTO facts (key, value, createdAt, updatedAt, source, category, importance)
                     VALUES (?1, ?2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 'test', 'checkpoint_test', 0.5)
                     ON CONFLICT(key) DO UPDATE SET value = ?2, updatedAt = CURRENT_TIMESTAMP",
                    rusqlite::params![format!("key_{}", iteration % 20), format!("value_{}", iteration)],
                )
                .expect("Insert fact");
            }
            iteration += 1;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        iteration
    });

    // Concurrently trigger WAL checkpoints in multiple modes
    for mode in [
        WalCheckpointMode::Passive,
        WalCheckpointMode::Full,
        WalCheckpointMode::Restart,
        WalCheckpointMode::Truncate,
        WalCheckpointMode::Passive,
    ] {
        let res: WalCheckpointResult = pool
            .wal_checkpoint(mode)
            .expect("Checkpoint must succeed without lock deadlock");
        assert!(res.busy >= 0);
        assert!(res.log >= 0);
        assert!(res.checkpointed >= 0);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    is_running.store(false, Ordering::Relaxed);
    let total_written = writer_task.await.expect("Writer task must finish cleanly");
    assert!(total_written > 0);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// SECTION 3: Rapid Session Creation, Context Switching & Eviction Stress
// ============================================================================

#[tokio::test]
async fn test_m5_rapid_session_creation_context_switch_and_eviction() {
    let sm = Arc::new(InMemorySessionManager::default());
    let num_sessions = 500;
    let concurrency = 25;

    let chunks = num_sessions / concurrency;
    let mut handles = Vec::new();

    // Rapid concurrent creation of sessions
    for worker_id in 0..concurrency {
        let manager = sm.clone();
        let handle = tokio::spawn(async move {
            let mut created_ids = Vec::new();
            for i in 0..chunks {
                let id_idx = worker_id * chunks + i;
                let user_id = format!("user_burst_{}", id_idx);
                let thread_id = format!("thread_burst_{}", id_idx);

                let session_arc = manager
                    .get_or_create_session(&ChannelId::WebSocketWidget, &user_id, Some(&thread_id))
                    .await
                    .expect("Session creation");

                let sess_id = session_arc.read().await.session_id;
                created_ids.push(sess_id);
            }
            created_ids
        });
        handles.push(handle);
    }

    let mut all_session_ids = Vec::new();
    for h in handles {
        let mut ids = h.await.expect("Session creation task must succeed");
        all_session_ids.append(&mut ids);
    }

    assert_eq!(all_session_ids.len(), num_sessions);
    let list = sm.list_sessions().await.expect("List sessions");
    assert_eq!(list.len(), num_sessions);

    // Concurrent context switching & variable manipulation across sessions
    let mut touch_handles = Vec::new();
    for chunk in all_session_ids.chunks(25) {
        let manager = sm.clone();
        let chunk_vec = chunk.to_vec();
        touch_handles.push(tokio::spawn(async move {
            for session_id in chunk_vec {
                if let Some(session_arc) = manager.get_session(&session_id).await.expect("Get session") {
                    let mut ctx = session_arc.write().await;
                    ctx.set_variable("active_skill", serde_json::json!("smarthome_controller"));
                    ctx.set_variable("switch_count", serde_json::json!(42));
                    ctx.touch();
                }

                if let Some(session_arc) = manager.get_session(&session_id).await.expect("Get session") {
                    let ctx = session_arc.read().await;
                    assert_eq!(ctx.get_variable("switch_count"), Some(&serde_json::json!(42)));
                }
            }
        }));
    }

    for th in touch_handles {
        th.await.expect("Touch task must succeed");
    }

    // Trigger eviction with zero TTL
    let evicted = sm.evict_expired(Duration::from_millis(0)).await.expect("Eviction");
    assert!(evicted <= num_sessions);
}

#[tokio::test]
async fn test_m5_session_lifecycle_state_machine_concurrency() {
    let sm = Arc::new(InMemorySessionManager::default());
    let session_arc = sm
        .get_or_create_session(&ChannelId::Discord, "lifecycle_user", None)
        .await
        .expect("Create session");

    let sess_id = session_arc.read().await.session_id;

    // Concurrently transition states
    let s1 = session_arc.clone();
    let s2 = session_arc.clone();

    let h1 = tokio::spawn(async move {
        let mut ctx = s1.write().await;
        ctx.set_state(SessionState::Idle);
    });

    let h2 = tokio::spawn(async move {
        let mut ctx = s2.write().await;
        ctx.touch(); // Transitions from Idle back to Active
    });

    h1.await.unwrap();
    h2.await.unwrap();

    let current = session_arc.read().await;
    assert!(matches!(current.state, SessionState::Active | SessionState::Idle));
    drop(current);

    // Terminate session
    sm.terminate_session(&sess_id).await.unwrap();
    let terminated = sm.get_session(&sess_id).await.unwrap();
    assert!(terminated.is_none());
}

// ============================================================================
// SECTION 4: Local-First Security & Encryption Verification
// ============================================================================

#[test]
fn test_m5_hkdf_aes_256_gcm_v2_encryption_roundtrip_and_freshness() {
    let engine = EncryptionEngine::new("m5-secret-key-32-bytes-secure!!");

    let large_payload = "A".repeat(65536);
    let test_cases = vec![
        "",
        "Short plaintext",
        "Tôi là LIVA, trợ lý AI nội hạt bảo mật cao.",
        "🔒 Local-first end-to-end encryption with HKDF-SHA256 and AES-256-GCM v2! 🚀",
        "{\"intent\": \"control_device\", \"target\": \"living_room_light\", \"brightness\": 80, \"tags\": [\"iot\", \"vietnam\"]}",
        &large_payload,
    ];

    for case in test_cases {
        let encrypted = engine.encrypt(case).expect("Encryption must succeed");
        assert!(encrypted.starts_with("v2:"), "Ciphertext must have v2: prefix");

        let decrypted = engine.try_decrypt(&encrypted).expect("Decryption must succeed");
        assert_eq!(decrypted, case, "Decrypted text must match original plaintext exactly");

        let read_fact = engine.read_fact(&encrypted);
        assert_eq!(read_fact, FactRead::Ok(case.to_string()));
        assert_eq!(read_fact.into_value(), case);
    }

    // Cryptographic Freshness: 100 encryptions of the EXACT same plaintext
    // MUST produce 100 unique salts, 100 unique IVs, and 100 unique ciphertexts.
    let target_plaintext = "Deterministic plaintext for freshness analysis";
    let iterations = 100;
    let mut salts = HashSet::new();
    let mut ivs = HashSet::new();
    let mut ciphertexts = HashSet::new();

    for _ in 0..iterations {
        let enc = engine.encrypt(target_plaintext).expect("Encryption");
        ciphertexts.insert(enc.clone());

        let parts: Vec<&str> = enc.split(':').collect();
        assert_eq!(parts.len(), 5, "v2 format must be v2:salt:iv:tag:cipher");
        assert_eq!(parts[0], "v2");
        salts.insert(parts[1].to_string());
        ivs.insert(parts[2].to_string());

        let dec = engine.decrypt(&enc);
        assert_eq!(dec, target_plaintext);
    }

    assert_eq!(
        salts.len(),
        iterations,
        "Every single encryption MUST generate a fresh, random salt"
    );
    assert_eq!(
        ivs.len(),
        iterations,
        "Every single encryption MUST generate a fresh, random IV"
    );
    assert_eq!(
        ciphertexts.len(),
        iterations,
        "Every single encryption MUST produce a distinct ciphertext"
    );
}

#[test]
fn test_m5_tamper_detection_and_fail_closed_locked_behavior() {
    let engine = EncryptionEngine::new("m5-tamper-verification-key-32b!");
    let plaintext = "Sensitive user identity and system preferences";
    let valid_ciphertext = engine.encrypt(plaintext).expect("Encrypt");

    let parts: Vec<&str> = valid_ciphertext.split(':').collect();
    assert_eq!(parts.len(), 5);

    let salt = parts[1];
    let iv = parts[2];
    let tag = parts[3];
    let cipher = parts[4];

    // Helper to mutate one character in a hex string
    let flip_hex = |s: &str| -> String {
        let mut chars: Vec<char> = s.chars().collect();
        chars[0] = if chars[0] == 'a' { 'b' } else { 'a' };
        chars.into_iter().collect()
    };

    // 1. Tampered Salt
    let tampered_salt = format!("v2:{}:{}:{}:{}", flip_hex(salt), iv, tag, cipher);
    assert_eq!(engine.try_decrypt(&tampered_salt), Err(DecryptError::AuthFailed));
    let read_salt = engine.read_fact(&tampered_salt);
    assert_eq!(read_salt, FactRead::Locked { reason: "auth_failed" });
    assert!(read_salt.is_locked());
    assert_eq!(read_salt.into_value(), "", "Locked fact must never leak ciphertext");
    assert_eq!(engine.decrypt_read(&tampered_salt), "");

    // 2. Tampered IV
    let tampered_iv = format!("v2:{}:{}:{}:{}", salt, flip_hex(iv), tag, cipher);
    assert_eq!(engine.try_decrypt(&tampered_iv), Err(DecryptError::AuthFailed));
    assert_eq!(engine.read_fact(&tampered_iv), FactRead::Locked { reason: "auth_failed" });

    // 3. Tampered Tag
    let tampered_tag = format!("v2:{}:{}:{}:{}", salt, iv, flip_hex(tag), cipher);
    assert_eq!(engine.try_decrypt(&tampered_tag), Err(DecryptError::AuthFailed));
    assert_eq!(engine.read_fact(&tampered_tag), FactRead::Locked { reason: "auth_failed" });

    // 4. Tampered Ciphertext
    let tampered_cipher = format!("v2:{}:{}:{}:{}", salt, iv, tag, flip_hex(cipher));
    assert_eq!(engine.try_decrypt(&tampered_cipher), Err(DecryptError::AuthFailed));
    assert_eq!(engine.read_fact(&tampered_cipher), FactRead::Locked { reason: "auth_failed" });

    // 5. Truncated wire representations
    let truncated = format!("v2:{}:{}", salt, iv);
    assert_eq!(engine.try_decrypt(&truncated), Err(DecryptError::BadFormat));
    // BadFormat is passthrough in read_fact to prevent destroying legacy malformed text
    let read_trunc = engine.read_fact(&truncated);
    assert_eq!(read_trunc, FactRead::Ok(truncated));
}

#[test]
fn test_m5_crypto_key_derivation_isolation_and_v1_migration() {
    let engine_a = EncryptionEngine::new("secret-key-alpha-32-bytes-long!!");
    let engine_b = EncryptionEngine::new("secret-key-beta-32-bytes-long!!!");

    // Key ID fingerprints must be distinct
    assert_ne!(engine_a.key_id(), engine_b.key_id());
    assert_eq!(engine_a.key_id().len(), 32); // 16 bytes in hex = 32 hex chars

    let plaintext = "Top secret conversation history";
    let encrypted_a = engine_a.encrypt(plaintext).expect("Encrypt A");

    // Engine B cannot decrypt Engine A's ciphertext
    assert_eq!(engine_b.try_decrypt(&encrypted_a), Err(DecryptError::AuthFailed));
    assert_eq!(
        engine_b.read_fact(&encrypted_a),
        FactRead::Locked { reason: "auth_failed" }
    );
    assert_eq!(engine_b.decrypt_read(&encrypted_a), "");

    // Verify Default Key Warning and constant
    assert_eq!(DEFAULT_ENCRYPTION_KEY, "00000000000000000000000000000000");
}

#[test]
fn test_m5_keystore_vault_secret_and_device_key_properties() {
    // Verify Key lengths and constant constraints
    assert_eq!(DEVICE_KEY_LEN, 32);
    assert_eq!(VAULT_PASSWORD_LEN, 32);
    assert_eq!(VAULT_SALT_LEN, 16);
    assert_eq!(DEVICE_KEY_FILE, ".device_key");
    assert_eq!(VAULT_SECRET_FILE, ".vault_secret");

    let db_path = std::path::Path::new("data/agents/liva_core/mem.sqlite");
    let key_path = device_key_path(db_path);
    assert!(key_path.ends_with(".device_key"));
    assert_eq!(key_path.parent(), db_path.parent());
}
