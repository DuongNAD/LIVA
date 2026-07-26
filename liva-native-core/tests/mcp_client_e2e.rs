//! E2E cho MCP **client** (rung G0): spawn một MCP server stdio THẬT rồi nói
//! JSON-RPC với nó qua pipe.
//!
//! Vì sao không phải unit test: các unit test trong `mcp/client.rs` kiểm phần
//! chuẩn hoá và tương quan id bằng `Value` dựng tay. Không cái nào chứng minh
//! được rằng `Command::spawn` chạy, handshake đi qua pipe thật, stderr được
//! drain nên server con không treo, và vòng đọc sống sót qua rác. Đúng khoảng
//! trống mà `scripts/e2e-gateway.mjs` lấp cho tầng WebSocket.
//!
//! Bia thử: `scripts/e2e-mcp-server.mjs` — nó CỐ TÌNH thô bạo (id kiểu số, dòng
//! không phải JSON, notification không ai gọi, hồi âm về ngược thứ tự,
//! `id: null`). Đọc đầu file đó để biết từng ca phục vụ điều gì.
//!
//! Không có `node` thì test BỎ QUA kèm log ồn ào, không im lặng đạt.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::mcp::client::{
    McpClientRegistry, McpServerConfig, McpStdioClient, parse_config,
};
use liva_native_core::mcp::protocol::{CallToolRequest, ToolContent};
use liva_native_core::{AppState, db, handle_command, llm, stt, tts};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn duong_dan_mock() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts")
        .join("e2e-mcp-server.mjs")
}

/// `false` + log ồn nếu thiếu `node`. CI có node (`npm ci`), máy dev cũng vậy
/// (husky), nên ca thiếu là ngoại lệ — nhưng nó phải NÓI ra, không được trông
/// giống một lần chạy đạt.
fn co_node() -> bool {
    match std::process::Command::new("node").arg("--version").output() {
        Ok(out) if out.status.success() => true,
        _ => {
            eprintln!(
                "\n!!! BỎ QUA mcp_client_e2e: không chạy được `node --version`.\n\
                 !!! Phần e2e của G0 KHÔNG được kiểm chứng trong lần chạy này.\n"
            );
            false
        }
    }
}

fn cau_hinh_mock() -> McpServerConfig {
    let json = format!(
        r#"{{"mcpServers":{{"mock":{{"command":"node","args":[{}]}}}}}}"#,
        serde_json::to_string(&duong_dan_mock().to_string_lossy().to_string())
            .expect("escape đường dẫn")
    );
    parse_config(&json)
        .expect("cấu hình mock phải phân tích được")
        .remove("mock")
        .expect("phải có mục 'mock'")
}

fn text_dau_tien(content: &[ToolContent]) -> Option<&str> {
    content.iter().find_map(|c| match c {
        ToolContent::Text { text } => Some(text.as_str()),
        _ => None,
    })
}

/// Toàn bộ vòng đời một MCP server ngoài, trong một test.
///
/// Gộp làm một chứ không tách nhiều `#[test]`: mỗi lần connect là một tiến
/// trình `node` mới, và các bước sau (`tools/call`) chỉ có nghĩa khi handshake
/// trước đó đã xong trên CÙNG kết nối.
#[tokio::test]
async fn vong_doi_mcp_server_ngoai() {
    if !co_node() {
        return;
    }

    let client = McpStdioClient::connect("mock", &cau_hinh_mock())
        .await
        .expect("phải nối và handshake được với mock server");

    // ── 1. Handshake đã chạy THẬT, không chỉ spawn xong tiến trình ──────────
    let hs = client
        .handshake_info()
        .expect("handshake_info phải có sau connect");
    assert_eq!(hs.protocol_version, "2024-11-05");
    assert_eq!(
        hs.server_info.get("name").and_then(|v| v.as_str()),
        Some("liva-mock-mcp"),
        "serverInfo phải là của mock server"
    );

    // ── 2. tools/list qua ca id kiểu SỐ + rác chen giữa ────────────────────
    // Mock echo id của `tools/list` dưới dạng number, và ngay trước đó đã bơm
    // một dòng không phải JSON + một notification. Cả ba đều đi qua vòng đọc
    // trước khi hồi âm này về được.
    let tools = client
        .list_tools()
        .await
        .expect("tools/list phải đi qua được id kiểu số và rác chen giữa");
    let ten: Vec<&str> = tools.tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(ten, vec!["echo", "no_desc", "slow"]);

    let no_desc = tools
        .tools
        .iter()
        .find(|t| t.name == "no_desc")
        .expect("tool thiếu description vẫn phải có mặt");
    assert!(
        no_desc.description.is_empty(),
        "thiếu description → chuỗi rỗng, không phải fail cả danh sách"
    );

    // ── 3. tools/call cơ bản ───────────────────────────────────────────────
    let res = client
        .call_tool(CallToolRequest {
            name: "echo".to_string(),
            arguments: serde_json::json!({ "a": 1 }),
        })
        .await
        .expect("echo phải chạy");
    assert!(!res.is_error);
    assert_eq!(text_dau_tien(&res.content), Some(r#"{"a":1}"#));

    // ── 4. Tương quan id: hồi âm về NGƯỢC thứ tự gửi ───────────────────────
    // `slow` trả sau 400ms, `echo` trả ngay. Client không tương quan id sẽ ghép
    // hồi âm của echo vào request slow — và test này là chỗ duy nhất bắt được.
    let goi = |ten: &str, ghi_chu: &str| {
        client.call_tool(CallToolRequest {
            name: ten.to_string(),
            arguments: serde_json::json!({ "ghiChu": ghi_chu }),
        })
    };
    let (cham, nhanh) = tokio::join!(goi("slow", "cham"), goi("echo", "nhanh"));
    assert_eq!(
        text_dau_tien(&cham.expect("slow phải thành công").content),
        Some("cham nhung dung"),
        "hồi âm của slow bị ghép sai — tương quan id hỏng"
    );
    assert!(
        text_dau_tien(&nhanh.expect("echo phải thành công").content)
            .is_some_and(|t| t.contains("nhanh")),
        "hồi âm của echo bị ghép sai — tương quan id hỏng"
    );

    // ── 5. Lỗi JSON-RPC nổi lên tới người gọi ──────────────────────────────
    let loi = client
        .call_tool(CallToolRequest {
            name: "boom".to_string(),
            arguments: serde_json::json!({}),
        })
        .await
        .expect_err("boom phải trả Err");
    assert!(
        loi.contains("-32001") && loi.contains("that bai"),
        "lỗi phải mang cả code và message của server, nhận được: {loi}"
    );

    // ── 6. isError của tool KHÁC lỗi giao thức ─────────────────────────────
    let tool_loi = client
        .call_tool(CallToolRequest {
            name: "tool_error".to_string(),
            arguments: serde_json::json!({}),
        })
        .await
        .expect("lỗi trong tool vẫn là result hợp lệ, không phải Err");
    assert!(tool_loi.is_error, "cờ isError phải tới được người gọi");

    // ── 7. content dạng ảnh: `mimeType` trên dây ───────────────────────────
    let anh = client
        .call_tool(CallToolRequest {
            name: "img".to_string(),
            arguments: serde_json::json!({}),
        })
        .await
        .expect("content dạng ảnh phải deserialize được");
    match anh.content.first().expect("phải có 1 phần tử") {
        ToolContent::Image { mime_type, data } => {
            assert_eq!(mime_type, "image/png", "phải đọc được `mimeType` camelCase");
            assert_eq!(data, "QUJD");
        }
        other => panic!("phải là Image, nhận được {other:?}"),
    }

    // ── 8. content loại lạ không được làm hỏng cả lời gọi ──────────────────
    let la = client
        .call_tool(CallToolRequest {
            name: "weird".to_string(),
            arguments: serde_json::json!({}),
        })
        .await
        .expect("một phần tử content lạ KHÔNG được làm fail cả lời gọi");
    assert!(
        matches!(la.content.first(), Some(ToolContent::Unsupported)),
        "loại content chưa hỗ trợ phải rơi vào nhánh Unsupported"
    );
    assert_eq!(
        text_dau_tien(&la.content),
        Some("phan text van con"),
        "phần text bên cạnh vẫn phải tới được người gọi"
    );

    // ── 9. Drain stderr: bẫy số 1 của G0 ───────────────────────────────────
    // `noisy` đổ ~400KB ra stderr bằng `writeSync` TRƯỚC khi trả lời. Buffer
    // pipe của HĐH chỉ ~64KB, nên client không drain stderr sẽ chặn tiến trình
    // con ngay giữa đợt ghi đó: nó không bao giờ gửi được hồi âm, và người gọi
    // chỉ biết khi hết timeout 30s — trông y hệt "model đang suy nghĩ".
    let t_stderr = Instant::now();
    let on_ao = client
        .call_tool(CallToolRequest {
            name: "noisy".to_string(),
            arguments: serde_json::json!({}),
        })
        .await
        .expect("server đổ đầy stderr KHÔNG được làm treo — task drain phải chạy");
    let mat_stderr = t_stderr.elapsed();
    assert_eq!(
        text_dau_tien(&on_ao.content),
        Some("da bom stderr xong ma khong treo")
    );
    assert!(
        mat_stderr < Duration::from_secs(10),
        "mất {mat_stderr:?} — gần timeout nghĩa là stderr KHÔNG được drain"
    );

    // ── 10. Lỗi `id: null` tới NGAY, không chờ hết timeout ─────────────────
    // Đây là bẫy đã ghi trong prompt G0: `JsonRpcResponse.id` là String không
    // Option, nên hồi âm id:null từng biến mất ở tầng serde và biểu hiện ra như
    // "server không trả lời". Mốc 5s tách hai ca đó ra: timeout mặc định là 30s.
    let t0 = Instant::now();
    let null_id = client
        .call_tool(CallToolRequest {
            name: "nullid".to_string(),
            arguments: serde_json::json!({}),
        })
        .await;
    let mat = t0.elapsed();
    let thong_diep = null_id.expect_err("hồi âm id:null phải thành Err");
    assert!(
        thong_diep.contains("-32700"),
        "phải là đúng lỗi server đã nói ra, không phải một lỗi khác tình cờ cũng \
         nhanh; nhận được: {thong_diep}"
    );
    assert!(
        mat < Duration::from_secs(5),
        "lỗi id:null phải tới ngay (đã mất {mat:?}) — nếu ~30s thì nó đã bị bỏ và \
         người gọi chỉ chờ timeout"
    );

    assert!(!client.is_closed(), "server con phải còn sống suốt test");
}

/// Registry: lọc cấu hình, nối lười, và báo lỗi đọc được khi gọi tên lạ.
#[tokio::test]
async fn registry_loc_cau_hinh_va_noi_lai_khi_can() {
    if !co_node() {
        return;
    }

    let duong_dan = std::env::temp_dir().join(format!(
        "liva-mcp-e2e-{}-{}.json",
        std::process::id(),
        "registry"
    ));
    let noi_dung = format!(
        r#"{{
          "mcpServers": {{
            "mock": {{ "command": "node", "args": [{}] }},
            "tat": {{ "command": "khong-ton-tai", "disabled": true }},
            "_cho_giu_cho": {{ "_comment": "placeholder", "command": "khong-ton-tai" }}
          }}
        }}"#,
        serde_json::to_string(&duong_dan_mock().to_string_lossy().to_string()).expect("escape")
    );
    std::fs::write(&duong_dan, noi_dung).expect("ghi được cấu hình tạm");

    let registry = McpClientRegistry::new(duong_dan.clone());

    // Trước khi gọi: khai báo có, kết nối chưa — nối lười.
    let truoc = registry.list_servers().await;
    assert_eq!(truoc["configExists"], serde_json::json!(true));
    let hang = truoc["servers"].as_array().expect("servers là mảng");
    assert_eq!(
        hang.len(),
        1,
        "mục `disabled` và mục `_`-prefix phải bị loại, còn lại: {hang:?}"
    );
    assert_eq!(hang[0]["name"], serde_json::json!("mock"));
    assert_eq!(hang[0]["connected"], serde_json::json!(false));

    // Gọi lần đầu → nối.
    let tools = registry
        .list_tools("mock")
        .await
        .expect("registry phải tự nối khi dùng lần đầu");
    assert_eq!(tools.tools.len(), 3);

    let sau = registry.list_servers().await;
    assert_eq!(sau["servers"][0]["connected"], serde_json::json!(true));
    assert_eq!(
        sau["servers"][0]["protocolVersion"],
        serde_json::json!("2024-11-05"),
        "list_servers phải chứng minh được handshake đã chạy"
    );

    // Gọi lần hai → DÙNG LẠI, không spawn thêm tiến trình.
    registry
        .call_tool(
            "mock",
            CallToolRequest {
                name: "echo".to_string(),
                arguments: serde_json::json!({ "lan": 2 }),
            },
        )
        .await
        .expect("lần gọi thứ hai phải dùng lại kết nối cũ");

    // Tên lạ: lỗi phải nói ra những tên đang có, không chỉ "not found".
    let loi = registry
        .list_tools("khong-co-server-nay")
        .await
        .expect_err("tên lạ phải lỗi");
    assert!(
        loi.contains("mock"),
        "lỗi phải liệt kê server đang có để người dùng sửa được, nhận được: {loi}"
    );

    let _ = std::fs::remove_file(&duong_dan);
}

/// `AppState` tối thiểu — sao theo `tests/websocket_transport.rs`. Không cần
/// model weights: `handle_command` chỉ chạm `state` ở các nhánh khác.
fn state_test() -> Arc<AppState> {
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    Arc::new(AppState {
        db: db::DatabasePool::new_in_memory().expect("DB in-memory"),
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt::SttManager::new("non-existent-model")),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm::LlamaRouterManager::new(2048, 0).expect("LLM manager")),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

/// Ba lệnh `mcp_client:*` ĐÃ nối vào `handle_command`.
///
/// Vì sao cần dù đã có e2e ở trên: các test kia gọi `McpClientRegistry` trực
/// tiếp. Không cái nào chứng minh lớp *dispatch* biết ba lệnh này — mà một arm
/// quên nối sẽ rơi vào nhánh `_ =>` và trả "Unknown command", đúng loại lỗi chỉ
/// lộ ra khi có người gõ lệnh thật.
///
/// Chỉ kiểm tới ranh giới xác thực tham số: đi xa hơn sẽ spawn tiến trình con
/// theo `mcp_config.json` của MÁY đang chạy, tức test phụ thuộc môi trường.
#[tokio::test]
async fn ba_lenh_mcp_client_da_noi_vao_dispatch() {
    let state = state_test();

    // `list_servers` không cần tham số. Không kiểm giá trị `configExists` vì nó
    // phụ thuộc cwd/`LIVA_MCP_CONFIG` của máy chạy test — chỉ kiểm KHUÔN.
    let ds = handle_command(
        Arc::clone(&state),
        "mcp_client:list_servers",
        serde_json::json!({}),
        None,
        None,
    )
    .await
    .expect("mcp_client:list_servers phải chạy được");
    assert!(ds.get("configPath").is_some(), "phải nói rõ đọc file nào");
    assert!(ds["servers"].is_array());
    assert!(ds["configExists"].is_boolean());

    // Thiếu `server` → lỗi CHỈ ĐƯỜNG, và quan trọng hơn: không phải "Unknown
    // command", tức arm đã tồn tại.
    for lenh in ["mcp_client:list_tools", "mcp_client:call_tool"] {
        let loi = handle_command(Arc::clone(&state), lenh, serde_json::json!({}), None, None)
            .await
            .expect_err("thiếu 'server' phải lỗi");
        assert!(
            loi.contains("mcp_client:list_servers"),
            "{lenh}: lỗi phải chỉ cách xem danh sách, nhận được: {loi}"
        );
        assert!(
            !loi.contains("Unknown command"),
            "{lenh}: CHƯA nối vào handle_command"
        );
    }

    // Hàng rào allowlist phải nằm TRONG arm, không chỉ tồn tại như một hàm.
    //
    // Hai ca dưới đây chứng minh nối dây mà không cần server nào chạy: guard nổ
    // TRƯỚC khi spawn/kết nối, nên nếu nó không được gọi thì lỗi trả về sẽ là
    // "không có server MCP tên..." chứ không phải lỗi allowlist.
    let loi = handle_command(
        Arc::clone(&state),
        "mcp_client:call_tool",
        serde_json::json!({ "server": "bat-ky", "name": "write_file", "arguments": {} }),
        None,
        None,
    )
    .await
    .expect_err("tool trên server ngoài phải bị allowlist chặn");
    assert!(
        loi.contains("LIVA_MCP_AUTOEXEC=bat-ky/write_file"),
        "phải là lỗi ALLOWLIST và phải nói cách mở; nhận được: {loi}"
    );

    let loi = handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        serde_json::json!({ "name": "write_markdown", "arguments": { "path": "a.md", "content": "x" } }),
        None,
        None,
    )
    .await
    .expect_err("write_markdown qua lớp lệnh phải bị allowlist chặn");
    assert!(
        loi.contains("LIVA_MCP_AUTOEXEC=native/write_markdown"),
        "nhận được: {loi}"
    );

    // Nhưng tool nội bộ chỉ-đọc thì vẫn phải qua được — hàng rào không được biến
    // thành "chặn tất".
    handle_command(
        Arc::clone(&state),
        "mcp:call_tool",
        serde_json::json!({ "name": "search_vault", "arguments": { "query": "gi cung duoc" } }),
        None,
        None,
    )
    .await
    .expect("search_vault phải vẫn gọi được qua lớp lệnh");

    // Có `server`, thiếu `name` → chặn ở xác thực tham số, KHÔNG spawn gì.
    let loi = handle_command(
        state,
        "mcp_client:call_tool",
        serde_json::json!({ "server": "bat-ky" }),
        None,
        None,
    )
    .await
    .expect_err("thiếu 'name' phải lỗi");
    assert!(
        loi.contains("mcp_client:list_tools"),
        "lỗi thiếu tên tool phải chỉ sang list_tools, nhận được: {loi}"
    );
}

/// Thiếu `mcp_config.json` KHÔNG được là lỗi bí ẩn: nó phải chỉ ra file mẫu.
#[tokio::test]
async fn thieu_cau_hinh_thi_bao_cach_sua() {
    let registry = McpClientRegistry::new(PathBuf::from("khong-ton-tai-mcp_config.json"));

    let anh = registry.list_servers().await;
    assert_eq!(anh["configExists"], serde_json::json!(false));
    assert!(
        anh["error"]
            .as_str()
            .is_some_and(|e| e.contains("mcp_config.example.json")),
        "list_servers phải nói cách sửa, nhận được: {:?}",
        anh["error"]
    );

    let loi = registry
        .list_tools("bat-ky")
        .await
        .expect_err("không có cấu hình thì không nối được");
    assert!(loi.contains("mcp_config.example.json"), "nhận được: {loi}");
}
