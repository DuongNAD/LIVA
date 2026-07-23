use std::sync::Arc;
use serde_json::json;
use liva_native_core::{handle_command, AppState, db, crypto, stt, llm, tts};

#[tokio::main]
async fn main() {
    println!("========================================================");
    println!("LIVA INTEGRATIONS - FUNCTIONAL CORRECTNESS VERIFICATION");
    println!("========================================================");

    // Set up dummy environment variables for tests
    unsafe {
        std::env::set_var("LIVA_ENCRYPTION_KEY", "00000000000000000000000000000000");
        std::env::set_var("TELEGRAM_BOT_TOKEN", "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11");
    }

    // Initialize mock application state
    let db = db::DatabasePool::new_in_memory().unwrap();
    let stt_manager = stt::SttManager::new("data/models/nemotron-asr");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).unwrap();
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        1920,
        1080,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        mock_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );
    let state = Arc::new(AppState {
        db,
        crypto: crypto::EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: std::sync::Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(vision_manager),
    });

    // 1. Verify smart home direct execution and validation boundaries
    println!("\n[1] Testing Smart Home Validation Boundaries...");
    
    // Test case 1.1: báo TRUNG THỰC (chưa có phần cứng → không thành công giả)
    let payload = json!({ "device": "light", "action": "on" });
    let res = liva_native_core::integrations::smart_home::execute(payload);
    assert!(res.is_ok());
    let msg = res.unwrap();
    assert!(
        msg.contains("CHƯA") && msg.contains("light") && msg.contains("on") && !msg.contains("successfully"),
        "smart_home phải báo trung thực, không thành công giả: {msg}"
    );
    println!("  -> Honest (not-connected) case passed!");

    // Test case 1.2: Invalid device
    let payload = json!({ "device": "tv", "action": "on" });
    let res = liva_native_core::integrations::smart_home::execute(payload);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Validation error"));
    println!("  -> Invalid device case passed!");

    // Test case 1.3: Extra fields rejection (strict validation)
    let payload = json!({ "device": "light", "action": "on", "extra": 123 });
    let res = liva_native_core::integrations::smart_home::execute(payload);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("unknown field `extra`"));
    println!("  -> Strict validation (extra fields rejection) case passed!");

    // 2. Verify handle_command for integration:smart_home_control
    println!("\n[2] Testing handle_command: integration:smart_home_control...");
    let payload = json!({ "device": "ac", "action": "off" });
    let res = handle_command(state.clone(), "integration:smart_home_control", payload, None, None).await;
    assert!(res.is_ok());
    let val = res.unwrap();
    let result_str = val["result"].as_str().unwrap_or_default();
    assert!(
        result_str.contains("CHƯA") && result_str.contains("ac") && result_str.contains("off"),
        "handle_command smart_home phải báo trung thực: {result_str}"
    );
    println!("  -> handle_command for smart home control passed!");

    // 3. Verify handle_command for telegram:send_text
    println!("\n[3] Testing handle_command: telegram:send_text...");
    let payload = json!({ "chatId": "123456", "text": "Hello from LIVA verification script!" });
    let res = handle_command(state.clone(), "telegram:send_text", payload, None, None).await;
    assert!(res.is_ok());
    let val = res.unwrap();
    assert_eq!(val, json!({ "success": true }));
    println!("  -> handle_command for telegram:send_text passed!");

    println!("\n========================================================");
    println!("ALL INTEGRATION VERIFICATION TESTS PASSED SUCCESSFULLY!");
    println!("========================================================");
}
