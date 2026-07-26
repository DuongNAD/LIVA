use liva_native_core::{
    DatabasePool,
    agent::{graph::StateGraph, memory::SqliteCheckpointer, state::AgentState},
    mcp::protocol::{CallToolRequest, ToolContent},
    mcp::server::NativeMcpServer,
};
use serde_json::json;
use std::sync::Arc;

struct TempDirGuard {
    path: std::path::PathBuf,
}
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[tokio::test]
async fn test_case_1_native_mcp_server() {
    // Determine temp vault path
    let rand_val = rand::random::<u32>();
    let vault_path = std::env::temp_dir().join(format!("mcp_vault_{}", rand_val));
    tokio::fs::create_dir_all(&vault_path).await.unwrap();
    let _guard = TempDirGuard {
        path: vault_path.clone(),
    };

    let mcp_server = NativeMcpServer::new(vault_path.to_str().unwrap());

    // Write a file using write_markdown
    let write_req = CallToolRequest {
        name: "write_markdown".to_string(),
        arguments: json!({
            "path": "hello.md",
            "content": "Hello, world!"
        }),
    };
    let write_res = mcp_server.call_tool(write_req).await.unwrap();
    assert!(!write_res.is_error);
    if let ToolContent::Text { text } = &write_res.content[0] {
        assert_eq!(text, "Success");
    } else {
        panic!("expected text response");
    }

    // Verify its existence on disk
    let file_path = vault_path.join("hello.md");
    assert!(file_path.exists());
    let disk_content = tokio::fs::read_to_string(&file_path).await.unwrap();
    assert_eq!(disk_content, "Hello, world!");

    // Read it back using read_markdown
    let read_req = CallToolRequest {
        name: "read_markdown".to_string(),
        arguments: json!({
            "path": "hello.md"
        }),
    };
    let read_res = mcp_server.call_tool(read_req).await.unwrap();
    assert!(!read_res.is_error);
    if let ToolContent::Text { text } = &read_res.content[0] {
        assert_eq!(text, "Hello, world!");
    } else {
        panic!("expected text response");
    }

    // Write another file using write_markdown in a subfolder
    let write_nested_req = CallToolRequest {
        name: "write_markdown".to_string(),
        arguments: json!({
            "path": "subfolder/nested.txt",
            "content": "Secret query key: banana"
        }),
    };
    let write_nested_res = mcp_server.call_tool(write_nested_req).await.unwrap();
    assert!(!write_nested_res.is_error);

    // Call search_vault tool with query "banana"
    let search_req = CallToolRequest {
        name: "search_vault".to_string(),
        arguments: json!({
            "query": "banana"
        }),
    };
    let search_res = mcp_server.call_tool(search_req).await.unwrap();
    assert!(!search_res.is_error);
    if let ToolContent::Text { text } = &search_res.content[0] {
        assert!(
            text.contains("subfolder/nested.txt"),
            "Search text should list the matching file. Got: {}",
            text
        );
    } else {
        panic!("expected text response");
    }
}

async fn node_one(mut state: AgentState) -> Result<AgentState, String> {
    state.context.insert("step1".to_string(), json!("done"));
    Ok(state)
}

async fn node_two(mut state: AgentState) -> Result<AgentState, String> {
    state.context.insert("step2".to_string(), json!("done"));
    Ok(state)
}

#[tokio::test]
async fn test_case_2_state_graph_and_checkpointer() {
    // a. Initialize DatabasePool::new_in_memory() and SqliteCheckpointer
    let db = Arc::new(DatabasePool::new_in_memory().expect("failed to create in-memory db"));
    let checkpointer = SqliteCheckpointer::new(db.clone());

    // b. Add nodes to StateGraph and connect edges
    let mut graph = StateGraph::new();
    graph.add_node("node1", node_one);
    graph.add_node("node2", node_two);
    graph.add_edge("node1", "node2");
    graph.set_entry_point("node1");

    // c. Run the graph, verify that the state is updated correctly
    let initial_state = AgentState::default();
    let final_state = graph.run(initial_state).await.unwrap();
    assert_eq!(final_state.current_node, "__END__");
    assert_eq!(
        final_state.context.get("step1").unwrap().as_str().unwrap(),
        "done"
    );
    assert_eq!(
        final_state.context.get("step2").unwrap().as_str().unwrap(),
        "done"
    );

    // d. Save the state using SqliteCheckpointer::save_checkpoint for a custom thread ID
    let thread_id = "test-thread-graph-123";
    checkpointer
        .save_checkpoint(thread_id, &final_state)
        .await
        .unwrap();

    // e. Load it back using SqliteCheckpointer::load_checkpoint and assert it is correct
    let loaded_state_opt = checkpointer.load_checkpoint(thread_id).await.unwrap();
    assert!(loaded_state_opt.is_some());
    let loaded_state = loaded_state_opt.unwrap();
    assert_eq!(loaded_state.current_node, final_state.current_node);
    assert_eq!(loaded_state.context, final_state.context);
    assert_eq!(loaded_state.messages, final_state.messages);

    // f. Obtain a database connection from the pool and query the agent_checkpoints table
    let conn = db.readers.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT thread_id, state_json FROM agent_checkpoints WHERE thread_id = ?1")
        .unwrap();
    let mut rows = stmt.query(rusqlite::params![thread_id]).unwrap();
    let row = rows.next().unwrap().unwrap();
    let tid: String = row.get(0).unwrap();
    let state_json: String = row.get(1).unwrap();

    assert_eq!(tid, thread_id);
    let db_state: AgentState = serde_json::from_str(&state_json).unwrap();
    assert_eq!(db_state.current_node, final_state.current_node);
    assert_eq!(db_state.context, final_state.context);
    assert_eq!(db_state.messages, final_state.messages);
}

#[tokio::test]
async fn test_case_3_path_traversal_prevention() {
    let rand_val = rand::random::<u32>();
    let vault_path = std::env::temp_dir().join(format!("mcp_vault_traversal_{}", rand_val));
    tokio::fs::create_dir_all(&vault_path).await.unwrap();
    let _guard = TempDirGuard {
        path: vault_path.clone(),
    };

    let mcp_server = NativeMcpServer::new(vault_path.to_str().unwrap());

    // Valid relative path should succeed
    let write_req = CallToolRequest {
        name: "write_markdown".to_string(),
        arguments: json!({
            "path": "subfolder/hello.md",
            "content": "allowed"
        }),
    };
    let write_res = mcp_server.call_tool(write_req).await.unwrap();
    assert!(!write_res.is_error);

    // Traversal path with ParentDir should fail
    let bad_req_1 = CallToolRequest {
        name: "read_markdown".to_string(),
        arguments: json!({
            "path": "../outside.md"
        }),
    };
    let bad_res_1 = mcp_server.call_tool(bad_req_1).await;
    assert!(bad_res_1.is_err());
    assert!(bad_res_1.err().unwrap().contains("traversal detected"));

    // Traversal path with root-relative / absolute should fail
    let bad_req_2 = CallToolRequest {
        name: "read_markdown".to_string(),
        arguments: json!({
            "path": "/etc/passwd"
        }),
    };
    let bad_res_2 = mcp_server.call_tool(bad_req_2).await;
    assert!(bad_res_2.is_err());
    assert!(bad_res_2.err().unwrap().contains("traversal detected"));

    // Windows root-relative (e.g. \Windows\win.ini) should fail
    let bad_req_3 = CallToolRequest {
        name: "read_markdown".to_string(),
        arguments: json!({
            "path": "\\Windows\\win.ini"
        }),
    };
    let bad_res_3 = mcp_server.call_tool(bad_req_3).await;
    assert!(bad_res_3.is_err());
    assert!(bad_res_3.err().unwrap().contains("traversal detected"));
}

#[tokio::test]
async fn test_case_4_stategraph_llama_nlp() {
    use std::path::Path;
    use tokio::sync::mpsc;

    let llm_model_dir = std::env::var("LIVA_LLM_MODEL_DIR").unwrap_or_else(|_| {
        let paths = ["models", "../models", "../../models"];
        for p in &paths {
            if Path::new(p)
                .join("gemma-4-26B-A4B-it-UD-Q6_K.gguf")
                .exists()
            {
                return p.to_string();
            }
        }
        "models".to_string()
    });
    let model_path = Path::new(&llm_model_dir).join("gemma-4-26B-A4B-it-UD-Q6_K.gguf");

    if !model_path.exists() {
        println!(
            "Skipping test: model file {:?} not found on disk.",
            model_path
        );
        return;
    }

    let db = Arc::new(DatabasePool::new_in_memory().expect("failed to create in-memory db"));
    let crypto =
        liva_native_core::crypto::EncryptionEngine::new("00000000000000000000000000000000");
    let stt_manager = liva_native_core::stt::SttManager::new("non_existent_dir");

    let mut llm_manager = liva_native_core::llm::LlamaRouterManager::new(2048, 0).unwrap();
    llm_manager
        .swap_model(&model_path, Some(2048), Some(0), Some(false))
        .await
        .unwrap();

    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
        "test_vault",
    ));

    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        1920,
        1080,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        mock_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );

    let state_shared = Arc::new(liva_native_core::AppState {
        db: (*db).clone(),
        crypto,
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: liva_native_core::tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
        embedder: tokio::sync::Mutex::new(None),
    });

    let (llm_chunk_tx, mut llm_chunk_rx) = mpsc::channel::<String>(100);
    let session_id = 999u64;
    let active_session_id = Arc::new(std::sync::atomic::AtomicU64::new(session_id));

    let graph = liva_native_core::agent::graph::build_pipeline_graph(
        state_shared.clone(),
        liva_native_core::agent::graph::ConversationMemoryScope::new(
            "integration-owner",
            "integration-conversation-1",
        )
        .unwrap(),
        llm_chunk_tx,
        session_id,
        active_session_id,
    );

    // Test Scenario A: Smart Home Command
    let initial_state_1 = liva_native_core::agent::state::AgentState {
        messages: vec![json!({"role": "user", "content": "please turn on the light"})],
        current_node: "router".to_string(),
        context: std::collections::HashMap::new(),
    };

    let final_state_1 = graph.run(initial_state_1).await.unwrap();
    assert_eq!(final_state_1.current_node, "__END__");
    assert_eq!(
        final_state_1
            .context
            .get("device")
            .unwrap()
            .as_str()
            .unwrap(),
        "light"
    );
    assert_eq!(
        final_state_1
            .context
            .get("action")
            .unwrap()
            .as_str()
            .unwrap(),
        "on"
    );
    assert!(final_state_1.messages.len() >= 3);
    assert_eq!(final_state_1.messages[0]["role"], "user");
    assert_eq!(final_state_1.messages[1]["role"], "tool");
    assert!(
        final_state_1.messages[1]["content"]
            .as_str()
            .unwrap()
            .contains("successfully turned 'on'")
    );

    let mut tokens_received = Vec::new();
    while let Ok(token) = llm_chunk_rx.try_recv() {
        tokens_received.push(token);
    }
    assert!(!tokens_received.is_empty());

    // Test Scenario B: Standard Chat Command
    let (llm_chunk_tx_2, mut llm_chunk_rx_2) = mpsc::channel::<String>(100);
    let active_session_id_2 = Arc::new(std::sync::atomic::AtomicU64::new(session_id));
    let graph_2 = liva_native_core::agent::graph::build_pipeline_graph(
        state_shared.clone(),
        liva_native_core::agent::graph::ConversationMemoryScope::new(
            "integration-owner",
            "integration-conversation-2",
        )
        .unwrap(),
        llm_chunk_tx_2,
        session_id,
        active_session_id_2,
    );

    let initial_state_2 = liva_native_core::agent::state::AgentState {
        messages: vec![json!({"role": "user", "content": "hello, tell me a joke"})],
        current_node: "router".to_string(),
        context: std::collections::HashMap::new(),
    };

    let final_state_2 = graph_2.run(initial_state_2).await.unwrap();
    assert_eq!(final_state_2.current_node, "__END__");
    assert!(final_state_2.context.is_empty());
    assert_eq!(final_state_2.messages.len(), 2);
    assert_eq!(final_state_2.messages[0]["role"], "user");
    assert_eq!(final_state_2.messages[1]["role"], "assistant");

    let mut tokens_received_2 = Vec::new();
    while let Ok(token) = llm_chunk_rx_2.try_recv() {
        tokens_received_2.push(token);
    }
    assert!(!tokens_received_2.is_empty());
}

// (test_case_5_report_html_existence removed 2026-07: it asserted a generated
// build artifact `static/report.html` that was deleted in the repo cleanup —
// the artifact is now gitignored and no runtime behavior depended on it.)

#[cfg(feature = "experimental")]
#[tokio::test]
async fn test_case_6_swarm_duplex_collaboration_no_deadlock() {
    use liva_native_core::agent::dispatcher::{
        AgentDispatcher, AgentMessage, AgentRole, SwarmAgent,
    };
    use std::time::Duration;
    use tokio::sync::mpsc;

    let dispatcher = AgentDispatcher::new();

    // Create channels for Orchestrator, Research, and Code agents
    let (orch_tx, mut orch_rx) = mpsc::channel(100);
    let (res_tx, res_rx) = mpsc::channel(100);
    let (code_tx, code_rx) = mpsc::channel(100);

    // Register with dispatcher
    dispatcher
        .register_agent(AgentRole::Orchestrator, orch_tx)
        .await;
    dispatcher.register_agent(AgentRole::Research, res_tx).await;
    dispatcher.register_agent(AgentRole::Code, code_tx).await;

    // Start agents
    let research_agent = SwarmAgent::new(AgentRole::Research, dispatcher.clone(), res_rx);
    let code_agent = SwarmAgent::new(AgentRole::Code, dispatcher.clone(), code_rx);

    let research_handle = research_agent.start();
    let code_handle = code_agent.start();

    // Orchestrator sends initial task to Research agent (delegation path)
    let trace_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();

    let msg = AgentMessage {
        message_id: request_id.clone(),
        trace_id: trace_id.clone(),
        from: AgentRole::Orchestrator,
        to: AgentRole::Research,
        content: "Please investigate how to implement a safe queue.".to_string(),
        correlation_id: None,
    };

    dispatcher.dispatch(msg).await.unwrap();

    // Orchestrator awaits final result from Research Agent
    let response = tokio::time::timeout(Duration::from_secs(3), orch_rx.recv())
        .await
        .expect("Test timed out waiting for swarm response")
        .expect("No response received");

    assert_eq!(response.correlation_id, Some(request_id));
    assert!(response.content.contains("Research results:"));
    assert!(response.content.contains("// Auto-generated Rust Code"));

    // Orchestrator sends second task to Research agent (non-delegation path)
    let request_id_2 = uuid::Uuid::new_v4().to_string();
    let msg_2 = AgentMessage {
        message_id: request_id_2.clone(),
        trace_id: trace_id.clone(),
        from: AgentRole::Orchestrator,
        to: AgentRole::Research,
        content: "Please investigate standard agent patterns.".to_string(),
        correlation_id: None,
    };

    dispatcher.dispatch(msg_2).await.unwrap();

    let response_2 = tokio::time::timeout(Duration::from_secs(3), orch_rx.recv())
        .await
        .expect("Test timed out waiting for second response")
        .expect("No second response received");

    assert_eq!(response_2.correlation_id, Some(request_id_2));
    assert!(response_2.content.contains("Research findings on:"));
    assert!(!response_2.content.contains("// Auto-generated Rust Code"));

    // Cleanup
    research_handle.abort();
    code_handle.abort();
}

/// Hồi quy F1 — khoá checkpoint phải ổn định qua các lượt VAD.
///
/// Bug gốc (`webrtc/pipeline.rs`): `thread_id` được lấy từ `session_id`, mà
/// `session_id` lại TĂNG ở mỗi sự kiện VAD (`cancel_active_operations` gọi từ
/// `handle_vad_start` / `handle_vad_end` / `handle_interrupted`). Hệ quả:
/// `load_checkpoint` luôn trả `None`, trợ lý không nhớ gì từ lượt trước, còn
/// bảng `agent_checkpoints` phình một dòng cho mỗi câu nói.
///
/// Test này mô phỏng 3 lượt nói liên tiếp và khẳng định hai điều:
///   1. Dùng khoá TĂNG DẦN (hành vi cũ) thì lượt sau không đọc được lượt trước
///      và sinh ra 3 dòng rác — tức là tái hiện đúng bug.
///   2. Dùng khoá ỔN ĐỊNH (hành vi mới) thì lượt sau đọc được lượt trước và
///      cả phiên chỉ để lại ĐÚNG MỘT dòng.
#[tokio::test]
async fn test_f1_checkpoint_key_must_be_stable_across_vad_turns() {
    let db = Arc::new(DatabasePool::new_in_memory().expect("failed to create in-memory db"));
    let checkpointer = SqliteCheckpointer::new(db.clone());

    let turn_state = |n: usize| AgentState {
        messages: vec![json!({"role": "user", "content": format!("cau noi thu {}", n)})],
        current_node: "__END__".to_string(),
        context: std::collections::HashMap::new(),
    };

    let count_rows = |thread_like: &str| {
        let conn = db.readers.get().unwrap();
        conn.query_row(
            "SELECT count(*) FROM agent_checkpoints WHERE thread_id LIKE ?1",
            rusqlite::params![thread_like],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };

    // ── 1. Hành vi CŨ: khoá tăng mỗi lượt (session_id) ───────────────────────
    for session_id in 1..=3u64 {
        let key = format!("buggy-{}", session_id);
        // Lượt sau cố đọc lại lượt trước — với khoá tăng dần thì không thấy gì.
        let seen = checkpointer.load_checkpoint(&key).await.unwrap();
        assert!(
            seen.is_none(),
            "khoá tăng dần lẽ ra không đọc được gì ở lượt {}",
            session_id
        );
        checkpointer
            .save_checkpoint(&key, &turn_state(session_id as usize))
            .await
            .unwrap();
    }
    assert_eq!(
        count_rows("buggy-%"),
        3,
        "hành vi cũ phải để lại 3 dòng rác — nếu khác thì test không còn tái hiện đúng bug"
    );

    // ── 2. Hành vi MỚI: khoá ổn định theo kết nối (conversation_id) ───────────
    let conversation_id = "conv-fixed-0001";
    for turn in 1..=3usize {
        let loaded = checkpointer.load_checkpoint(conversation_id).await.unwrap();
        if turn == 1 {
            assert!(loaded.is_none(), "lượt đầu chưa có gì để đọc");
        } else {
            let prev = loaded.expect("lượt sau PHẢI đọc được checkpoint của lượt trước");
            let content = prev.messages[0]["content"].as_str().unwrap().to_string();
            assert_eq!(
                content,
                format!("cau noi thu {}", turn - 1),
                "phải đọc đúng nội dung lượt liền trước"
            );
        }
        checkpointer
            .save_checkpoint(conversation_id, &turn_state(turn))
            .await
            .unwrap();
    }
    assert_eq!(
        count_rows("conv-fixed-%"),
        1,
        "cả phiên chỉ được để lại ĐÚNG MỘT dòng checkpoint"
    );
}

/// Hằng số chiều vector của DB và của model embedding phải luôn khớp nhau.
/// Nếu ai đó đổi một bên mà quên bên kia, test này đỏ ngay.
///
/// Đặt ở integration test chứ không ở `db.rs`: ba binary (`verify_round2`,
/// `voice_profile`, `voice_stress`) include `db.rs` qua `#[path]`, nên mọi
/// tham chiếu `crate::llm::…` trong đó sẽ làm chúng không biên dịch được.
#[test]
fn chieu_vector_db_va_embedder_phai_khop() {
    assert_eq!(
        liva_native_core::db::MEMORY_VECTOR_DIM,
        liva_native_core::llm::embedder::EMBEDDING_DIM,
        "db::MEMORY_VECTOR_DIM va llm::embedder::EMBEDDING_DIM phai bang nhau"
    );
}

/// Dựng một AppState tối thiểu cho test lớp lệnh: DB in-memory, không LLM/TTS
/// thật, vision dùng capturer giả. Chỉ đủ để gọi handle_command.
fn build_test_state(vault_path: &str) -> Arc<liva_native_core::AppState> {
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        640,
        480,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    Arc::new(liva_native_core::AppState {
        db: DatabasePool::new_in_memory().expect("in-memory db"),
        crypto: liva_native_core::EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(liva_native_core::SttManager::new("non_existent_dir")),
        tts: tokio::sync::Mutex::new(None),
        tts_player: liva_native_core::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(
            liva_native_core::LlamaRouterManager::new(512, 0).expect("llm manager"),
        ),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(NativeMcpServer::new(vault_path)),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

/// 2.7 — `mcp:list_tools` / `mcp:call_tool` phải đi qua `handle_command`.
///
/// Trước đây `NativeMcpServer` được dựng trong `AppState` nhưng KHÔNG có nhánh
/// nào trong `handle_command` gọi tới, nên cả 4 tool là code mồ côi: chỉ test
/// gọi trực tiếp, không client nào chạm được.
///
/// Test này kiểm qua đúng lớp lệnh mà client thật dùng, chứ không gọi tắt vào
/// `NativeMcpServer` — nếu ai đó gỡ arm đi, test đỏ.
#[tokio::test]
async fn test_mcp_di_qua_handle_command() {
    // KHAI BÁO QUYỀN TƯỜNG MINH cho `write_markdown`.
    //
    // Từ 26/07/2026 nhánh `mcp:call_tool` có hàng rào allowlist
    // (`tool_calling::guard_direct_call`), và `write_markdown` CỐ Ý không nằm
    // trong `NATIVE_AUTOEXEC` vì nó ghi file. Test này kiểm vòng ghi→đọc nên nó
    // phải tự nói ra là nó muốn quyền đó — đúng cơ chế caller hợp pháp phải dùng,
    // KHÔNG phải cửa hậu cho test.
    //
    // An toàn để đặt env ở đây: mỗi file trong `tests/` biên dịch thành một
    // binary RIÊNG, nên biến này không rò sang test của `lib` (nơi
    // `ghi_file_khong_bao_gio_tu_chay_theo_mac_dinh` khẳng định mặc định là chặn).
    // Trong chính binary này, nới quyền không làm sai khẳng định nào của test khác.
    unsafe { std::env::set_var("LIVA_MCP_AUTOEXEC", "native/write_markdown") };

    let rand_val = rand::random::<u32>();
    let vault_path = std::env::temp_dir().join(format!("mcp_cmd_vault_{}", rand_val));
    tokio::fs::create_dir_all(&vault_path).await.unwrap();
    let _guard = TempDirGuard {
        path: vault_path.clone(),
    };

    let state = build_test_state(vault_path.to_str().unwrap());

    // 1. list_tools — phải thấy đủ 4 tool
    let tools = liva_native_core::handle_command(
        Arc::clone(&state),
        "mcp:list_tools",
        json!({}),
        None,
        None,
    )
    .await
    .expect("mcp:list_tools phai thanh cong");
    let names: Vec<String> = tools["tools"]
        .as_array()
        .expect("truong 'tools' phai la mang")
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "read_markdown",
        "write_markdown",
        "search_vault",
        "control_smarthome",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "thieu tool {expected}: {names:?}"
        );
    }

    // 2. call_tool ghi rồi đọc lại — vòng tròn đầy đủ qua lớp lệnh
    let w = liva_native_core::handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        json!({"name": "write_markdown", "arguments": {"path": "ghi_chu.md", "content": "xin chao"}}),
        None,
        None,
    )
    .await
    .expect("write_markdown phai thanh cong");
    assert_eq!(w["isError"], false, "ghi file khong duoc bao loi: {w}");

    let r = liva_native_core::handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        json!({"name": "read_markdown", "arguments": {"path": "ghi_chu.md"}}),
        None,
        None,
    )
    .await
    .expect("read_markdown phai thanh cong");
    assert_eq!(
        r["content"][0]["text"], "xin chao",
        "doc lai phai ra dung noi dung"
    );

    // 3. Path traversal phải bị chặn NGAY cả khi đi qua lớp lệnh
    let bad = liva_native_core::handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        json!({"name": "read_markdown", "arguments": {"path": "../../../../etc/passwd"}}),
        None,
        None,
    )
    .await;
    assert!(
        bad.is_err(),
        "duong dan traversal phai bi tu choi, nhan duoc: {bad:?}"
    );

    // 4. Thiếu 'name' phải báo lỗi chỉ ra cách khắc phục
    let no_name = liva_native_core::handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        json!({"arguments": {}}),
        None,
        None,
    )
    .await
    .unwrap_err();
    assert!(
        no_name.contains("mcp:list_tools"),
        "phai goi y cach xem danh sach: {no_name}"
    );

    // 5. Tool không tồn tại
    let unknown = liva_native_core::handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        json!({"name": "khong_ton_tai", "arguments": {}}),
        None,
        None,
    )
    .await;
    assert!(unknown.is_err(), "tool la phai bi tu choi");
}
