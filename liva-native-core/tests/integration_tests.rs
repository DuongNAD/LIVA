use std::sync::Arc;
use liva_native_core::{
    DatabasePool,
    mcp::server::NativeMcpServer,
    mcp::protocol::{CallToolRequest, ToolContent},
    agent::{
        graph::StateGraph,
        state::AgentState,
        memory::SqliteCheckpointer,
    },
};
use serde_json::json;

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
    let _guard = TempDirGuard { path: vault_path.clone() };

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
        assert!(text.contains("subfolder/nested.txt"), "Search text should list the matching file. Got: {}", text);
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
    assert_eq!(final_state.context.get("step1").unwrap().as_str().unwrap(), "done");
    assert_eq!(final_state.context.get("step2").unwrap().as_str().unwrap(), "done");

    // d. Save the state using SqliteCheckpointer::save_checkpoint for a custom thread ID
    let thread_id = "test-thread-graph-123";
    checkpointer.save_checkpoint(thread_id, &final_state).await.unwrap();

    // e. Load it back using SqliteCheckpointer::load_checkpoint and assert it is correct
    let loaded_state_opt = checkpointer.load_checkpoint(thread_id).await.unwrap();
    assert!(loaded_state_opt.is_some());
    let loaded_state = loaded_state_opt.unwrap();
    assert_eq!(loaded_state.current_node, final_state.current_node);
    assert_eq!(loaded_state.context, final_state.context);
    assert_eq!(loaded_state.messages, final_state.messages);

    // f. Obtain a database connection from the pool and query the agent_checkpoints table
    let conn = db.readers.get().unwrap();
    let mut stmt = conn.prepare("SELECT thread_id, state_json FROM agent_checkpoints WHERE thread_id = ?1").unwrap();
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
    let _guard = TempDirGuard { path: vault_path.clone() };

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
            if Path::new(p).join("gemma-4-26B-A4B-it-UD-Q6_K.gguf").exists() {
                return p.to_string();
            }
        }
        "models".to_string()
    });
    let model_path = Path::new(&llm_model_dir).join("gemma-4-26B-A4B-it-UD-Q6_K.gguf");

    if !model_path.exists() {
        println!("Skipping test: model file {:?} not found on disk.", model_path);
        return;
    }

    let db = Arc::new(DatabasePool::new_in_memory().expect("failed to create in-memory db"));
    let crypto = liva_native_core::crypto::EncryptionEngine::new("00000000000000000000000000000000");
    let stt_manager = liva_native_core::stt::SttManager::new("non_existent_dir");

    let mut llm_manager = liva_native_core::llm::LlamaRouterManager::new(2048, 0).unwrap();
    llm_manager.swap_model(&model_path, Some(2048), Some(0), Some(false)).await.unwrap();

    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault"));

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
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
    });

    let (llm_chunk_tx, mut llm_chunk_rx) = mpsc::channel::<String>(100);
    let session_id = 999u64;
    let active_session_id = Arc::new(std::sync::atomic::AtomicU64::new(session_id));

    let graph = liva_native_core::agent::graph::build_pipeline_graph(
        state_shared.clone(),
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
    assert_eq!(final_state_1.context.get("device").unwrap().as_str().unwrap(), "light");
    assert_eq!(final_state_1.context.get("action").unwrap().as_str().unwrap(), "on");
    assert!(final_state_1.messages.len() >= 3);
    assert_eq!(final_state_1.messages[0]["role"], "user");
    assert_eq!(final_state_1.messages[1]["role"], "tool");
    assert!(final_state_1.messages[1]["content"].as_str().unwrap().contains("successfully turned 'on'"));

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

#[tokio::test]
async fn test_case_6_swarm_duplex_collaboration_no_deadlock() {
    use tokio::sync::mpsc;
    use std::time::Duration;
    use liva_native_core::agent::dispatcher::{AgentDispatcher, AgentMessage, AgentRole, SwarmAgent};

    let dispatcher = AgentDispatcher::new();

    // Create channels for Orchestrator, Research, and Code agents
    let (orch_tx, mut orch_rx) = mpsc::channel(100);
    let (res_tx, res_rx) = mpsc::channel(100);
    let (code_tx, code_rx) = mpsc::channel(100);

    // Register with dispatcher
    dispatcher.register_agent(AgentRole::Orchestrator, orch_tx).await;
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


