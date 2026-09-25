//! Milestone M2 Adversarial Challenge Test Suite:
//! Multi-Model Idle Memory Reclamation, Lazy Reloading & Non-Blocking Concurrency.
//!
//! Empirically challenges and stress-tests:
//! 1. Parakeet STT idle unload lifecycle, streaming inhibition & lazy reload.
//! 2. VieNeu TTS idle unload, speaker config retention & seamless synthesis reload.
//! 3. Boot Task #6 non-blocking safety and try_lock concurrency under active tasks.
//! 4. `get_memory_status` IPC command schema, dynamic state tracking & ACL authorization.

use liva_native_core::boot::{VoiceIdleUnloadReport, check_voice_idle_unload};
use liva_native_core::commands::config;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::llm::LlamaRouterManager;
use liva_native_core::resolve_resource_path;
use liva_native_core::stt::{SttManager, resolve_parakeet_paths};
use liva_native_core::tts::TtsManager;
use liva_native_core::tts::audio::TtsAudioPlayer;
use liva_native_core::tts::vieneu::VieNeuVoice;
use liva_native_core::vision::capture::{MockScreenCapturer, PixelFormat};
use liva_native_core::vision::{VisionConfig, VisionManager};
use liva_native_core::{AppState, CommandPrincipal, authorize_command};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Helper to create a fully initialized in-memory AppState for testing.
fn create_test_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("failed to create in-memory db");
    let crypto = EncryptionEngine::new("00000000000000000000000000000000");
    let nemotron_dir = resolve_resource_path("models/nemotron-asr");
    let stt = tokio::sync::Mutex::new(SttManager::new(nemotron_dir));
    let tts = tokio::sync::Mutex::new(None);
    let tts_player = TtsAudioPlayer::new(None);
    let llm = AppState::mock_llm();
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
        "data/vault",
    ));
    let mock_capturer = Arc::new(MockScreenCapturer::new(64, 64, PixelFormat::Rgba));
    let vision_manager = VisionManager::new(mock_capturer, VisionConfig::default());

    Arc::new(AppState {
        db,
        crypto,
        stt,
        tts,
        tts_player,
        llm,
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
        embedder: liva_native_core::AppState::empty_embedder(),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

/// Helper to resolve VieNeu model directory from repo root or crate directory.
fn resolve_vieneu_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("models/vieneu"),
        PathBuf::from("../models/vieneu"),
        resolve_resource_path("models/vieneu"),
    ];
    for p in &candidates {
        if p.join("config.json").exists() && p.join("voices_v3_turbo.json").exists() {
            return Some(p.clone());
        }
    }
    None
}

/// Helper to resolve Parakeet STT model paths.
fn resolve_parakeet_model_paths() -> Option<(PathBuf, PathBuf)> {
    let (model, vocab) = resolve_parakeet_paths();
    if model.exists() && vocab.exists() {
        return Some((model, vocab));
    }
    let fallback_model = PathBuf::from("../models/parakeet_vi.onnx");
    let fallback_vocab = PathBuf::from("../models/parakeet_vi_vocab.json");
    if fallback_model.exists() && fallback_vocab.exists() {
        return Some((fallback_model, fallback_vocab));
    }
    None
}

// ===========================================================================
// SECTION 1: PARAKEET STT IDLE RECLAMATION & LAZY RELOADING
// ===========================================================================

#[test]
fn test_parakeet_stt_idle_unload_and_streaming_inhibition_lifecycle() {
    let Some((model_path, vocab_path)) = resolve_parakeet_model_paths() else {
        eprintln!("[SKIP] Parakeet model files not found; skipping test.");
        return;
    };

    unsafe {
        std::env::set_var("LIVA_PARAKEET_MODEL_PATH", &model_path);
        std::env::set_var("LIVA_PARAKEET_VOCAB_PATH", &vocab_path);
    }

    let nemotron_dir = resolve_resource_path("models/nemotron-asr");
    let mut stt = SttManager::new(nemotron_dir);
    stt.set_language("vi-VN").expect("set language vi-VN");

    // Initially, Parakeet must NOT be loaded (lazy load guarantee)
    assert!(
        !stt.is_parakeet_loaded(),
        "Parakeet must not be loaded initially before any audio processing"
    );

    // Initial check_idle_unload should be false (nothing to unload)
    assert!(
        !stt.check_idle_unload(Duration::from_secs(300)),
        "check_idle_unload must return false when model is not loaded"
    );

    // 1. Ingest first intermediate audio chunk (is_last = false)
    let chunk = vec![0.0f32; 2560]; // 160ms audio at 16kHz
    let feed_res = stt.feed_audio(&chunk, false);
    assert!(
        feed_res.is_ok(),
        "feed_audio intermediate chunk should succeed: {:?}",
        feed_res.err()
    );

    // Parakeet must now be resident in memory (~2.4GB weights)
    assert!(
        stt.is_parakeet_loaded(),
        "Parakeet must be lazily instantiated in memory after first audio feed"
    );

    // CRITICAL OBSERVATION / BUG FINDING:
    // On the very first chunk when stt.engine.is_none(), feed_audio_inner sets is_streaming = true,
    // but then calls self.init()?, which calls self.reset_stream(), wiping is_streaming back to false!
    // On chunk 2, self.engine is already initialized, so self.init() is skipped and is_streaming stays true.
    let feed_res2 = stt.feed_audio(&chunk, false);
    assert!(feed_res2.is_ok());

    // 2. ADVERSARIAL STRESS: check_idle_unload MUST be inhibited while is_streaming == true
    // Even if timeout is 0 seconds (simulating extreme idle timeout expiration),
    // ongoing streaming must refuse to drop weights and disrupt the speaker!
    let unloaded_during_stream = stt.check_idle_unload(Duration::ZERO);
    assert!(
        !unloaded_during_stream,
        "check_idle_unload MUST NOT unload weights while speech streaming is in-flight"
    );
    assert!(
        stt.is_parakeet_loaded(),
        "Parakeet weights must remain in RAM during active audio stream"
    );

    // Repeated sweep attempts during streaming must all fail closed
    for _ in 0..5 {
        assert!(
            !stt.check_idle_unload(Duration::ZERO),
            "Streaming guard must consistently inhibit unload across multiple sweeps"
        );
    }

    // 3. Complete the stream by feeding final chunk (is_last = true)
    let final_res = stt.feed_audio(&chunk, true);
    assert!(
        final_res.is_ok(),
        "feed_audio final chunk should succeed: {:?}",
        final_res.err()
    );

    // Stream has ended, but recent activity (< 300s) must prevent premature unloading
    assert!(
        !stt.check_idle_unload(Duration::from_secs(300)),
        "Recently active STT model (< 300s) must not be unloaded"
    );
    assert!(
        stt.is_parakeet_loaded(),
        "Parakeet weights must stay warm during active window"
    );

    // 4. Timeout elapsed (Duration::ZERO simulates >= 300s timeout reached)
    let unloaded = stt.check_idle_unload(Duration::ZERO);
    assert!(
        unloaded,
        "check_idle_unload must return true when idle timeout is reached"
    );
    assert!(
        !stt.is_parakeet_loaded(),
        "Parakeet weights must be dropped from RAM (reclaiming ~2.4GB)"
    );

    // 5. Idempotent check: calling check_idle_unload again on unloaded state returns false
    assert!(
        !stt.check_idle_unload(Duration::ZERO),
        "check_idle_unload must return false when already unloaded"
    );

    // 6. LAZY RELOAD VERIFICATION:
    // Feed subsequent audio frame — Parakeet must seamlessly re-instantiate from disk!
    let reload_res = stt.feed_audio(&chunk, true);
    assert!(
        reload_res.is_ok(),
        "feed_audio must succeed after idle unload"
    );
    assert!(
        stt.is_parakeet_loaded(),
        "Parakeet must be lazily re-instantiated upon subsequent audio feed"
    );

    // 7. Explicit unload_parakeet test
    stt.unload_parakeet();
    assert!(
        !stt.is_parakeet_loaded(),
        "unload_parakeet must immediately drop weights"
    );
}

#[test]
fn test_parakeet_stt_reset_stream_clears_streaming_inhibition() {
    let Some((model_path, vocab_path)) = resolve_parakeet_model_paths() else {
        eprintln!("[SKIP] Parakeet model files not found; skipping test.");
        return;
    };

    unsafe {
        std::env::set_var("LIVA_PARAKEET_MODEL_PATH", &model_path);
        std::env::set_var("LIVA_PARAKEET_VOCAB_PATH", &vocab_path);
    }

    let nemotron_dir = resolve_resource_path("models/nemotron-asr");
    let mut stt = SttManager::new(nemotron_dir);
    stt.set_language("vi-VN").expect("set language");

    // Put into streaming state with is_last = false
    // Chunk 1 triggers self.init() which resets is_streaming; chunk 2 preserves is_streaming = true
    let chunk = vec![0.0f32; 2560];
    let _ = stt.feed_audio(&chunk, false);
    let _ = stt.feed_audio(&chunk, false);
    assert!(stt.is_parakeet_loaded());
    assert!(!stt.check_idle_unload(Duration::ZERO));

    // Abrupt stream reset (e.g. client disconnect or barge-in interruption)
    stt.reset_stream();

    // After reset_stream(), streaming flag is cleared; idle unload must now succeed!
    let unloaded = stt.check_idle_unload(Duration::ZERO);
    assert!(
        unloaded,
        "reset_stream must clear streaming flag and allow idle unload"
    );
    assert!(!stt.is_parakeet_loaded());
}

// ===========================================================================
// SECTION 2: VIENEU TTS IDLE RECLAMATION & LAZY RELOADING
// ===========================================================================

#[test]
fn test_vieneu_tts_idle_unload_retains_configs_and_reloads_on_synthesize() {
    let Some(vieneu_dir) = resolve_vieneu_dir() else {
        eprintln!("[SKIP] VieNeu model dir not found; skipping test.");
        return;
    };

    // 1. Load VieNeu voice
    let mut voice = VieNeuVoice::load(&vieneu_dir, None)
        .expect("VieNeuVoice::load should succeed with model directory");

    assert!(
        voice.is_loaded(),
        "VieNeuVoice must have all 4 ONNX sessions active after initial load"
    );
    let original_voice_name = voice.voice_name().to_string();
    assert!(
        !original_voice_name.is_empty(),
        "VieNeu voice name should not be empty"
    );
    assert_eq!(
        voice.sample_rate(),
        48_000,
        "VieNeu sample rate must be 48 kHz"
    );

    // 2. Inactivity check with long timeout (300s) -> should NOT unload
    assert!(
        !voice.check_idle_unload(Duration::from_secs(300)),
        "VieNeu must not unload when idle duration has not elapsed"
    );
    assert!(voice.is_loaded());

    // 3. Trigger idle unload (Duration::ZERO simulates >= 300s idle timeout)
    let unloaded = voice.check_idle_unload(Duration::ZERO);
    assert!(
        unloaded,
        "check_idle_unload must return true and unload 4 ONNX sessions"
    );
    assert!(
        !voice.is_loaded(),
        "VieNeuVoice::is_loaded must be false after idle unload (~500MB reclaimed)"
    );

    // 4. CRITICAL INVARIANT: Voice configuration & metadata MUST remain intact in RAM!
    assert_eq!(
        voice.voice_name(),
        original_voice_name,
        "Voice name must be preserved across session unloads"
    );
    assert_eq!(
        voice.sample_rate(),
        48_000,
        "Sample rate must be preserved across session unloads"
    );

    // Calling check_idle_unload again is idempotent
    assert!(
        !voice.check_idle_unload(Duration::ZERO),
        "Subsequent check_idle_unload must return false when already unloaded"
    );

    // 5. Test explicit ensure_sessions() reload
    voice
        .ensure_sessions()
        .expect("ensure_sessions must reload 4 ONNX sessions");
    assert!(
        voice.is_loaded(),
        "is_loaded must be true after ensure_sessions"
    );

    // 6. Test seamless lazy reload triggered by synthesize()
    voice.unload_sessions();
    assert!(
        !voice.is_loaded(),
        "Sessions must be unloaded before synthesize test"
    );

    // synthesize() must automatically invoke ensure_sessions()? and produce valid audio
    let synth_res = voice.synthesize("LIVA");
    assert!(
        synth_res.is_ok(),
        "synthesize must succeed and seamlessly reload sessions: {:?}",
        synth_res.err()
    );
    let (samples, phonemes) = synth_res.unwrap();
    assert!(
        !samples.is_empty(),
        "synthesize must produce non-empty audio samples"
    );
    assert!(
        !phonemes.is_empty(),
        "synthesize must produce non-empty phoneme string"
    );
    assert!(
        voice.is_loaded(),
        "VieNeu sessions must be active in RAM after synthesize"
    );
}

#[test]
fn test_vieneu_tts_voice_switching_while_unloaded_succeeds() {
    let Some(vieneu_dir) = resolve_vieneu_dir() else {
        eprintln!("[SKIP] VieNeu model dir not found; skipping test.");
        return;
    };

    let mut voice = VieNeuVoice::load(&vieneu_dir, None).expect("load VieNeu");
    voice.unload_sessions();
    assert!(!voice.is_loaded());

    // Switch voice preset while sessions are UNLOADED.
    // Because tied xvec projections and config_json remain in RAM, voice switching
    // does not need to touch ONNX sessions.
    let switch_res = voice.set_voice("Trúc Ly");
    assert!(
        switch_res.is_ok(),
        "set_voice must succeed even when sessions are unloaded: {:?}",
        switch_res.err()
    );
    assert_eq!(voice.voice_name(), "Trúc Ly");
    assert!(
        !voice.is_loaded(),
        "set_voice should not prematurely reload ONNX sessions"
    );

    // Subsequent synthesis lazily reloads sessions with the new voice configuration
    let synth_res = voice.synthesize("Xin chào");
    assert!(synth_res.is_ok());
    assert!(voice.is_loaded());
}

#[test]
fn test_tts_manager_vieneu_idle_lifecycle_through_wrapper() {
    let Some(vieneu_dir) = resolve_vieneu_dir() else {
        eprintln!("[SKIP] VieNeu model dir not found; skipping test.");
        return;
    };

    let vieneu_voice = VieNeuVoice::load(&vieneu_dir, None).expect("load VieNeu");
    let mut manager =
        TtsManager::from_bin("khong-ton-tai-model.onnx", "khong-ton-tai-voice.bin", None)
            .expect("TtsManager should initialize gracefully");

    // Attach VieNeu engine
    manager.set_vieneu_engine(Some(Arc::new(std::sync::Mutex::new(vieneu_voice))));

    assert!(manager.vieneu_is_loaded());
    assert!(manager.loaded_backends().contains(&"VieNeu"));

    // Idle unload with Duration::ZERO must unload VieNeu
    let unloaded = manager.check_idle_unload_timeout(Duration::ZERO);
    assert!(unloaded, "TtsManager must report unload success");
    assert!(
        !manager.vieneu_is_loaded(),
        "TtsManager::vieneu_is_loaded must return false after unload"
    );

    // Idempotent second sweep
    assert!(!manager.check_idle_unload_timeout(Duration::ZERO));
}

// ===========================================================================
// SECTION 3: BOOT TASK #6 NON-BLOCKING SAFETY & TRY_LOCK CONCURRENCY
// ===========================================================================

#[tokio::test]
async fn test_check_voice_idle_unload_non_blocking_when_stt_held() {
    let state = create_test_state();

    // Simulate an active streaming transcription task holding state.stt lock
    let stt_guard = state.stt.lock().await;

    // Execute check_voice_idle_unload with strict timeout (50ms ceiling)
    let start = Instant::now();
    let report = tokio::time::timeout(Duration::from_millis(50), async {
        check_voice_idle_unload(&state, Duration::from_secs(300))
    })
    .await
    .expect(
        "check_voice_idle_unload must return immediately (<50ms) without blocking or deadlocking",
    );

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "check_voice_idle_unload took {:?} (must be instantaneous via try_lock)",
        elapsed
    );

    // STT was busy: it must be skipped safely
    assert!(
        !report.stt_unloaded,
        "STT engine must NOT be unloaded when locked by active worker"
    );

    drop(stt_guard);
}

#[tokio::test]
async fn test_check_voice_idle_unload_non_blocking_when_tts_held() {
    let state = create_test_state();

    // Simulate an active speech synthesis task holding state.tts lock
    let tts_guard = state.tts.lock().await;

    let start = Instant::now();
    let report = tokio::time::timeout(Duration::from_millis(50), async {
        check_voice_idle_unload(&state, Duration::from_secs(300))
    })
    .await
    .expect("check_voice_idle_unload must return immediately (<50ms) when TTS is locked");

    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_millis(50));

    // TTS was busy: it must be skipped safely
    assert!(
        !report.tts_unloaded,
        "TTS engine must NOT be unloaded when locked by active worker"
    );

    drop(tts_guard);
}

#[tokio::test]
async fn test_check_voice_idle_unload_non_blocking_when_both_held() {
    let state = create_test_state();

    // Lock both STT and TTS concurrently
    let stt_guard = state.stt.lock().await;
    let tts_guard = state.tts.lock().await;

    let report = tokio::time::timeout(Duration::from_millis(50), async {
        check_voice_idle_unload(&state, Duration::ZERO)
    })
    .await
    .expect("Must not deadlock when both locks are contended");

    assert_eq!(
        report,
        VoiceIdleUnloadReport {
            tts_unloaded: false,
            stt_unloaded: false,
            approx_reclaimed_bytes: 0,
        },
        "Must report zero unloaded models when both locks are contended"
    );

    drop(stt_guard);
    drop(tts_guard);
}

#[tokio::test]
async fn test_check_voice_idle_unload_concurrency_stress_no_deadlock() {
    let state = create_test_state();

    let mut handles = Vec::new();

    // Spawn 10 concurrent tasks simulating bursty hot audio processing (frequent lock acquisition)
    for _ in 0..10 {
        let state_clone = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                {
                    let stt = state_clone.stt.lock().await;
                    let _ = stt.language(); // Read operation
                    tokio::task::yield_now().await;
                }
                {
                    let tts = state_clone.tts.lock().await;
                    let _ = tts.is_some();
                    tokio::task::yield_now().await;
                }
            }
        }));
    }

    // Spawn 10 concurrent maintenance tasks sweeping check_voice_idle_unload
    for _ in 0..10 {
        let state_clone = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                let _ = check_voice_idle_unload(&state_clone, Duration::from_secs(300));
                tokio::task::yield_now().await;
            }
        }));
    }

    // All tasks must join cleanly within 10 seconds without deadlock or lock starvation
    let join_all = futures_util::future::join_all(handles);
    tokio::time::timeout(Duration::from_secs(10), join_all)
        .await
        .expect("All concurrent tasks must finish without deadlock");
}

// ===========================================================================
// SECTION 4: `get_memory_status` IPC COMMAND EMPIRICAL VERIFICATION
// ===========================================================================

#[tokio::test]
async fn test_get_memory_status_json_contract_and_structure() {
    let state = create_test_state();

    let res = config::handle(Arc::clone(&state), "get_memory_status", json!({}))
        .await
        .expect("get_memory_status command execution failed");

    // 1. Verify JSON root sections
    let os_stats = res
        .get("osStats")
        .expect("Missing 'osStats' field in get_memory_status response");
    let proc_mem = res
        .get("processMemory")
        .expect("Missing 'processMemory' field in get_memory_status response");
    let model_mem = res
        .get("modelMemory")
        .expect("Missing 'modelMemory' field in get_memory_status response");

    // 2. Verify OS stats
    assert!(
        os_stats.get("totalMemory").is_some(),
        "Missing osStats.totalMemory"
    );
    assert!(
        os_stats.get("freeMemory").is_some(),
        "Missing osStats.freeMemory"
    );
    if let Some(total) = os_stats.get("totalMemory").and_then(|v| v.as_u64()) {
        assert!(total > 0, "totalMemory must be > 0 bytes");
    }

    // 3. Verify process memory
    assert!(
        proc_mem.get("rssMemory").is_some(),
        "Missing processMemory.rssMemory"
    );
    assert!(
        proc_mem.get("commitCharge").is_some(),
        "Missing processMemory.commitCharge"
    );
    if let Some(rss) = proc_mem.get("rssMemory").and_then(|v| v.as_u64()) {
        assert!(rss > 0, "rssMemory must be > 0 bytes");
    }

    // 4. Verify model residency booleans and reclaimable bytes
    let stt_loaded = model_mem
        .get("sttParakeetLoaded")
        .and_then(|v| v.as_bool())
        .expect("sttParakeetLoaded must be boolean");
    let tts_kokoro_loaded = model_mem
        .get("ttsKokoroLoaded")
        .and_then(|v| v.as_bool())
        .expect("ttsKokoroLoaded must be boolean");
    let tts_vieneu_loaded = model_mem
        .get("ttsVieneuLoaded")
        .and_then(|v| v.as_bool())
        .expect("ttsVieneuLoaded must be boolean");
    let reclaimable = model_mem
        .get("reclaimableApproxBytes")
        .and_then(|v| v.as_u64())
        .expect("reclaimableApproxBytes must be u64");

    // In initial mock state, nothing is loaded
    assert!(!stt_loaded);
    assert!(!tts_kokoro_loaded);
    assert!(!tts_vieneu_loaded);
    assert_eq!(reclaimable, 0);
}

#[tokio::test]
async fn test_get_memory_status_dynamic_state_tracking() {
    let state = create_test_state();

    // 1. Initial baseline: 0 bytes reclaimable
    let res0 = config::handle(Arc::clone(&state), "get_memory_status", json!({}))
        .await
        .unwrap();
    assert_eq!(
        res0["modelMemory"]["reclaimableApproxBytes"].as_u64(),
        Some(0)
    );

    // 2. Load Parakeet STT if model files exist
    if let Some((model_path, vocab_path)) = resolve_parakeet_model_paths() {
        unsafe {
            std::env::set_var("LIVA_PARAKEET_MODEL_PATH", &model_path);
            std::env::set_var("LIVA_PARAKEET_VOCAB_PATH", &vocab_path);
        }
        {
            let mut stt = state.stt.lock().await;
            stt.set_language("vi-VN").unwrap();
            let _ = stt.feed_audio(&vec![0.0f32; 2560], false);
            assert!(stt.is_parakeet_loaded());
        }

        // Query get_memory_status: should reflect Parakeet loaded (~2.4GB)
        let res1 = config::handle(Arc::clone(&state), "get_memory_status", json!({}))
            .await
            .unwrap();
        assert_eq!(
            res1["modelMemory"]["sttParakeetLoaded"].as_bool(),
            Some(true)
        );
        assert!(
            res1["modelMemory"]["reclaimableApproxBytes"]
                .as_u64()
                .unwrap()
                >= 2_400_000_000,
            "reclaimableApproxBytes must account for Parakeet (>= 2.4GB)"
        );
    }

    // 3. Attach VieNeu TTS if model files exist
    if let Some(vieneu_dir) = resolve_vieneu_dir() {
        let vieneu = VieNeuVoice::load(&vieneu_dir, None).expect("load VieNeu");
        let mut tts_mgr =
            TtsManager::from_bin("khong-ton-tai.onnx", "khong-ton-tai.bin", None).unwrap();
        tts_mgr.set_vieneu_engine(Some(Arc::new(std::sync::Mutex::new(vieneu))));

        {
            let mut tts_slot = state.tts.lock().await;
            *tts_slot = Some(tts_mgr);
        }

        let res2 = config::handle(Arc::clone(&state), "get_memory_status", json!({}))
            .await
            .unwrap();
        assert_eq!(res2["modelMemory"]["ttsVieneuLoaded"].as_bool(), Some(true));
        assert!(
            res2["modelMemory"]["reclaimableApproxBytes"]
                .as_u64()
                .unwrap()
                >= 500_000_000,
            "reclaimableApproxBytes must account for VieNeu (>= 500MB)"
        );

        // 4. Sweep idle unload: models should drop and reclaimableApproxBytes must return to 0
        let report = check_voice_idle_unload(&state, Duration::ZERO);
        assert!(report.tts_unloaded || report.stt_unloaded);

        let res3 = config::handle(Arc::clone(&state), "get_memory_status", json!({}))
            .await
            .unwrap();
        assert_eq!(
            res3["modelMemory"]["ttsVieneuLoaded"].as_bool(),
            Some(false)
        );
        assert_eq!(
            res3["modelMemory"]["sttParakeetLoaded"].as_bool(),
            Some(false)
        );
        assert_eq!(
            res3["modelMemory"]["reclaimableApproxBytes"].as_u64(),
            Some(0),
            "reclaimableApproxBytes must return to 0 after all models are unloaded"
        );
    }
}

#[tokio::test]
async fn test_get_memory_status_non_blocking_when_stt_lock_held() {
    let state = create_test_state();

    // Hold state.stt lock
    let _guard = state.stt.lock().await;

    // get_memory_status must return promptly without deadlocking
    let res = tokio::time::timeout(Duration::from_millis(100), async {
        config::handle(Arc::clone(&state), "get_memory_status", json!({})).await
    })
    .await
    .expect("get_memory_status must not deadlock when STT lock is held")
    .expect("get_memory_status execution should succeed");

    // Under contention, sttParakeetLoaded defaults conservatively to true
    assert_eq!(
        res["modelMemory"]["sttParakeetLoaded"].as_bool(),
        Some(true),
        "Under STT lock contention, get_memory_status must safely report true"
    );
}

#[tokio::test]
async fn test_get_memory_status_authorization_acl() {
    // 1. Widget is authorized
    let auth_widget = authorize_command(CommandPrincipal::TauriWidget, "get_memory_status");
    assert!(
        auth_widget.is_ok(),
        "Widget principal must be authorized to call get_memory_status"
    );

    // 2. Dashboard is authorized
    let auth_dash = authorize_command(CommandPrincipal::TauriDashboard, "get_memory_status");
    assert!(
        auth_dash.is_ok(),
        "Dashboard principal must be authorized to call get_memory_status"
    );

    // 3. Setup is NOT authorized (least-privilege boundary)
    let auth_setup = authorize_command(CommandPrincipal::TauriSetup, "get_memory_status");
    assert!(
        auth_setup.is_err(),
        "Setup principal must NOT be authorized to inspect memory status"
    );

    // 4. Verification via root handle_command_as dispatcher
    let state = create_test_state();
    let res = liva_native_core::handle_command_as(
        CommandPrincipal::TauriWidget,
        state,
        "get_memory_status",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        res.is_ok(),
        "handle_command_as with TauriWidget principal should execute successfully"
    );

    // 5. Verification via root handle_command_as dispatcher with Setup principal (must fail)
    let state2 = create_test_state();
    let res_setup = liva_native_core::handle_command_as(
        CommandPrincipal::TauriSetup,
        state2,
        "get_memory_status",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        res_setup.is_err(),
        "handle_command_as with TauriSetup principal must fail-closed under ACL"
    );
}

// ===========================================================================
// SECTION 5: EMPIRICAL DEFECT PROOFS & ADVERSARIAL FINDINGS
// ===========================================================================

#[test]
fn test_bug_proof_parakeet_feed_audio_init_wipes_streaming_flag_on_chunk1() {
    let Some((model_path, vocab_path)) = resolve_parakeet_model_paths() else {
        return;
    };
    unsafe {
        std::env::set_var("LIVA_PARAKEET_MODEL_PATH", &model_path);
        std::env::set_var("LIVA_PARAKEET_VOCAB_PATH", &vocab_path);
    }
    let nemotron_dir = resolve_resource_path("models/nemotron-asr");
    let mut stt = SttManager::new(nemotron_dir);
    stt.set_language("vi-VN").expect("set language");

    // Feed a SINGLE chunk with is_last = false (streaming beginning)
    let chunk = vec![0.0f32; 2560];
    let _ = stt.feed_audio(&chunk, false);

    // DEFECT DISCOVERY:
    // When stt.engine is None (default at startup), feed_audio_inner sets is_streaming = true,
    // but then immediately calls self.init()?, which unconditionally calls self.reset_stream().
    // self.reset_stream() wipes self.is_streaming back to false!
    //
    // Empirical proof: check_idle_unload(ZERO) returns true and drops Parakeet weights
    // EVEN THOUGH speech streaming has just begun!
    let prematurely_unloaded = stt.check_idle_unload(Duration::ZERO);
    assert!(
        prematurely_unloaded,
        "[EMPIRICAL FINDING] On chunk 1, self.init() wiped self.is_streaming, allowing premature unload"
    );
    assert!(
        !stt.is_parakeet_loaded(),
        "[EMPIRICAL FINDING] Parakeet weights were prematurely dropped mid-stream on chunk 1"
    );
}

#[test]
fn test_bug_proof_parakeet_feed_chunk_omits_streaming_protection() {
    let Some((model_path, vocab_path)) = resolve_parakeet_model_paths() else {
        return;
    };
    unsafe {
        std::env::set_var("LIVA_PARAKEET_MODEL_PATH", &model_path);
        std::env::set_var("LIVA_PARAKEET_VOCAB_PATH", &vocab_path);
    }
    let nemotron_dir = resolve_resource_path("models/nemotron-asr");
    let mut stt = SttManager::new(nemotron_dir);
    stt.set_language("vi-VN").expect("set language");

    let chunk = vec![0.0f32; 2560];
    let _ = stt.feed_chunk(&chunk, false);
    assert!(stt.is_parakeet_loaded());

    // DEFECT DISCOVERY:
    // In feed_chunk(), when Parakeet is used, the method delegates directly to
    // self.parakeet.feed_chunk() without setting self.is_streaming = true!
    //
    // Empirical proof: check_idle_unload(ZERO) returns true during active feed_chunk streaming!
    let prematurely_unloaded = stt.check_idle_unload(Duration::ZERO);
    assert!(
        prematurely_unloaded,
        "[EMPIRICAL FINDING] feed_chunk never sets self.is_streaming = true, leaving Parakeet unprotected"
    );
    assert!(
        !stt.is_parakeet_loaded(),
        "[EMPIRICAL FINDING] Parakeet dropped while streaming via feed_chunk"
    );
}
