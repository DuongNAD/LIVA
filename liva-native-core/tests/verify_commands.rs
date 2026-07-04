use std::sync::Arc;
use liva_native_core::{
    handle_command, AppState, DatabasePool, EncryptionEngine, SttManager, LlamaRouterManager, TtsAudioPlayer
};
use serde_json::json;

#[tokio::test]
async fn test_verify_handle_commands() {
    // Initialize mock state
    let db = DatabasePool::new_in_memory().expect("failed to create in-memory db");
    let crypto = EncryptionEngine::new("00000000000000000000000000000000");
    let stt = tokio::sync::Mutex::new(SttManager::new("non_existent_dir"));
    let tts = tokio::sync::Mutex::new(None);
    let tts_player = TtsAudioPlayer::new(None);
    let llm = tokio::sync::Mutex::new(LlamaRouterManager::new(2048, 0).expect("failed to create LLM manager"));
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("data/vault"));

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
    });

    // 1. Verify standard I/O command execution for integration:smart_home_control
    // A. Valid payload
    let payload_valid = json!({ "device": "light", "action": "on" });
    let res_valid = handle_command(state.clone(), "integration:smart_home_control", payload_valid, None, None)
        .await
        .expect("should successfully execute smart home command");
    assert_eq!(
        res_valid,
        json!({ "result": "Device 'light' successfully turned 'on'." })
    );

    // B. Invalid payload (invalid device)
    let payload_invalid_device = json!({ "device": "tv", "action": "on" });
    let res_invalid_device = handle_command(state.clone(), "integration:smart_home_control", payload_invalid_device, None, None).await;
    assert!(res_invalid_device.is_err(), "should fail on invalid device");
    let err_msg = res_invalid_device.unwrap_err();
    assert!(
        err_msg.contains("Validation error"),
        "error should contain validation error message: {}", err_msg
    );

    // C. Strict validation (extra fields)
    let payload_extra_fields = json!({ "device": "light", "action": "on", "extra_field": "unwanted" });
    let res_extra_fields = handle_command(state.clone(), "integration:smart_home_control", payload_extra_fields, None, None).await;
    assert!(res_extra_fields.is_err(), "should fail on extra fields due to deny_unknown_fields");
    let err_msg_extra = res_extra_fields.unwrap_err();
    assert!(
        err_msg_extra.contains("unknown field `extra_field`"),
        "error should mention unknown field: {}", err_msg_extra
    );

    // 2. Verify standard I/O command execution for telegram:send_text
    // A. Without TELEGRAM_BOT_TOKEN
    unsafe { std::env::remove_var("TELEGRAM_BOT_TOKEN"); }
    let payload_tg = json!({ "chatId": "1234567", "text": "Hello World" });
    let res_tg_no_token = handle_command(state.clone(), "telegram:send_text", payload_tg.clone(), None, None).await;
    assert!(res_tg_no_token.is_err(), "should fail if token is missing");
    assert_eq!(res_tg_no_token.unwrap_err(), "Bot token missing");

    // B. With TELEGRAM_BOT_TOKEN
    unsafe { std::env::set_var("TELEGRAM_BOT_TOKEN", "123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ"); }
    let res_tg_with_token = handle_command(state.clone(), "telegram:send_text", payload_tg, None, None)
        .await
        .expect("should execute send_text command when token is present");
    assert_eq!(res_tg_with_token, json!({ "success": true }));

    // 3. Verify new UI query commands
    let res_get_config = handle_command(state.clone(), "get_config", json!({}), None, None).await.unwrap();
    assert!(res_get_config.get("avatar").is_some());

    let res_get_ai_config = handle_command(state.clone(), "get_ai_config", json!({}), None, None).await.unwrap();
    assert!(res_get_ai_config.get("provider").is_some());

    let res_get_voice_status = handle_command(state.clone(), "get_voice_status", json!({}), None, None).await.unwrap();
    assert_eq!(res_get_voice_status, json!({ "stt": "ready", "tts": "ready" }));

    let res_get_voice_profiles = handle_command(state.clone(), "get_voice_profiles", json!({}), None, None).await.unwrap();
    assert_eq!(res_get_voice_profiles, json!([]));

    let res_get_system_status = handle_command(state.clone(), "get_system_status", json!({}), None, None).await.unwrap();
    assert!(res_get_system_status.get("healthChecks").is_some());

    let res_get_skills_list = handle_command(state.clone(), "get_skills_list", json!({}), None, None).await.unwrap();
    assert!(res_get_skills_list.is_array());

    let res_get_user_profile = handle_command(state.clone(), "get_user_profile", json!({}), None, None).await.unwrap();
    assert!(res_get_user_profile.get("name").is_some());

    let res_get_tasks = handle_command(state.clone(), "get_tasks", json!({}), None, None).await.unwrap();
    assert!(res_get_tasks.get("tasks").is_some());

    let res_get_avatar_models = handle_command(state.clone(), "get_avatar_models", json!({}), None, None).await.unwrap();
    assert_eq!(res_get_avatar_models, json!({ "models3d": [], "models2d": [] }));

    let res_get_memory_data = handle_command(state.clone(), "get_memory_data", json!({}), None, None).await.unwrap();
    assert!(res_get_memory_data.get("l0").is_some());
    assert!(res_get_memory_data.get("facts").is_some());
    assert!(res_get_memory_data.get("events").is_some());

    // 4. Verify Task Commands CRUD flow
    // A. Add Task
    let add_res = handle_command(
        state.clone(),
        "add_task",
        json!({
            "title": "Test Task 1",
            "description": "Integration testing tasks",
            "priority": "high",
            "status": "pending"
        }),
        None,
        None
    ).await.expect("add_task should succeed");
    
    assert_eq!(add_res["success"], true);
    let task_id = add_res["id"].as_str().expect("id should be a string").to_string();
    assert!(!task_id.is_empty());

    // B. Get Tasks and verify task is present
    let tasks_res = handle_command(state.clone(), "get_tasks", json!({}), None, None).await.unwrap();
    let tasks_list = tasks_res["tasks"].as_array().expect("tasks should be an array");
    let found_task = tasks_list.iter().find(|t| t["id"] == task_id).expect("task should be in tasks list");
    assert_eq!(found_task["title"], "Test Task 1");
    assert_eq!(found_task["description"], "Integration testing tasks");
    assert_eq!(found_task["priority"], "high");
    assert_eq!(found_task["status"], "pending");

    // C. Update Task
    let update_res = handle_command(
        state.clone(),
        "update_task",
        json!({
            "id": task_id,
            "updates": {
                "status": "in-progress",
                "description": "Updated description"
            }
        }),
        None,
        None
    ).await.expect("update_task should succeed");
    assert_eq!(update_res["success"], true);

    // Verify task is updated
    let tasks_res_updated = handle_command(state.clone(), "get_tasks", json!({}), None, None).await.unwrap();
    let found_task_updated = tasks_res_updated["tasks"].as_array().unwrap().iter().find(|t| t["id"] == task_id).unwrap();
    assert_eq!(found_task_updated["status"], "in-progress");
    assert_eq!(found_task_updated["description"], "Updated description");

    // D. Delete Task
    let delete_res = handle_command(
        state.clone(),
        "delete_task",
        json!({ "id": task_id }),
        None,
        None
    ).await.expect("delete_task should succeed");
    assert_eq!(delete_res["success"], true);

    // Verify task is deleted
    let tasks_res_deleted = handle_command(state.clone(), "get_tasks", json!({}), None, None).await.unwrap();
    let found_deleted = tasks_res_deleted["tasks"].as_array().unwrap().iter().any(|t| t["id"] == task_id);
    assert!(!found_deleted);
}
