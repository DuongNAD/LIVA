//! Đường khởi động DÙNG CHUNG cho hai vỏ: gateway (`liva-native-core/src/main.rs`)
//! và app desktop (`liva-desktop/src-tauri/src/lib.rs`).
//!
//! Vì sao module này tồn tại: trước 26/07/2026 hai vỏ **tự dựng `AppState`
//! riêng** bằng hai đoạn mã gần như chép nguyên — mở DB, resolve khoá, nạp
//! STT/TTS/LLM/vision/embedder, rồi spawn dịch vụ nền. Hai bản sao đó đã trôi
//! lệch thật, và lệch theo hướng khó thấy nhất:
//!
//! | Thứ bị lệch | gateway | app desktop (thứ người dùng CHẠY) |
//! |---|---|---|
//! | Giải phóng TTS khi rảnh 5 phút | có | **KHÔNG** → session ONNX giữ RAM mãi |
//! | Bot Telegram | có | **KHÔNG** → đặt token xong bot không chạy, không báo lỗi |
//! | Log lỗi âm thanh | `tracing::error!` | `eprintln!` (không qua bộ lọc log) |
//! | Log khi nạp được embedder | có | im lặng |
//! | Vòng hạ lớp GPU khi chơi game | dùng biến đã parse lúc boot | tự parse lại env |
//!
//! (Báo lỗi boot thì KHÔNG lệch: `die_tauri_boot` vốn đã bồi `db_error_hint`
//! y như gateway. Ghi lại ở đây để bảng trên không bị đọc rộng hơn sự thật.)
//!
//! Và điều làm nó nguy hiểm: `scripts/e2e-gateway.mjs` kiểm **gateway**, còn
//! người dùng chạy **app desktop**. Nghĩa là mọi thứ lệch đều nằm đúng ở phía
//! không ai kiểm.
//!
//! Cách chia: [`build_app_state`] dựng trạng thái (thuần, không spawn gì),
//! [`spawn_background_services`] bật dịch vụ nền. Phần **thật sự** khác nhau
//! giữa hai vỏ — cách báo lỗi (stderr+exit vs hộp thoại), cách hiện escrow, ai
//! đọc stdin — nằm lại ở vỏ, và đi vào đây qua [`ServiceOptions`].

use crate::{AppState, db, env_flag, governor, llm, stt, telegram, tts, webrtc};
use std::sync::Arc;
use tracing::{error, info};

/// Lỗi khởi động, đủ để vỏ tự chọn cách hiển thị.
///
/// Tách `context` khỏi `detail` vì hai vỏ trình bày khác nhau: gateway in một
/// khối stderr rồi `exit(1)`, còn app desktop dựng hộp thoại. Cái chung là NỘI
/// DUNG — kể cả gợi ý khắc phục, thứ trước đây chỉ gateway mới có.
pub struct BootError {
    pub context: String,
    pub detail: String,
}

impl BootError {
    fn new(context: impl Into<String>, detail: impl std::fmt::Display) -> Self {
        Self {
            context: context.into(),
            detail: detail.to_string(),
        }
    }

    /// Lỗi DB, kèm gợi ý khắc phục suy ra từ chính thông điệp gốc.
    ///
    /// Nguyên nhân gần như luôn là thiếu `vec0` (sqlite-vec) — thứ mà thông
    /// điệp của rusqlite giấu kín. Cả hai vỏ trước đây đều tự bồi gợi ý này ở
    /// hàm `die*` của mình; đưa vào đây để vỏ thứ ba (nếu có) không phải nhớ.
    fn db(detail: impl std::fmt::Display) -> Self {
        let d = detail.to_string();
        Self::new(
            format!(
                "Không khởi tạo được cơ sở dữ liệu{}",
                crate::db_error_hint(&d)
            ),
            d,
        )
    }
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.context, self.detail)
    }
}

/// Trạng thái đã dựng xong, cộng những thứ vỏ phải tự giữ hoặc tự hiển thị.
pub struct Boot {
    pub state: Arc<AppState>,
    /// `Some(hex)` khi khoá mã hoá vừa được SINH MỚI ⇒ vỏ phải cho người dùng
    /// sao lưu đúng một lần. Gateway in ra stderr, app desktop mở hộp thoại —
    /// đó là khác biệt có thật, nên để vỏ quyết.
    pub escrow_hex: Option<String>,
    /// Nguồn khóa + tổng số bản ghi nhạy cảm đã rekey/khóa-chết, để vỏ log.
    pub crypto_source: &'static str,
    pub crypto_rekeyed: usize,
    pub crypto_locked: usize,
    /// `OutputStream` của rodio. **Drop nó là LIVA câm** — vỏ phải giữ sống
    /// suốt đời tiến trình. Trả về đây thay vì tự `mem::forget` bên trong: một
    /// hàm thư viện rò rỉ bộ nhớ sau lưng người gọi là thứ không nên có.
    pub audio_stream: Option<rodio::OutputStream>,
    /// Số lớp GPU ở chế độ thường, đọc một lần lúc dựng LLM. Truyền tiếp vào
    /// [`ServiceOptions`] để vòng hạ-lớp-khi-chơi-game so đúng với cấu hình đã
    /// dùng, thay vì đọc lại env và có cơ hội lệch.
    pub llm_n_gpu_layers: u32,
}

/// Dựng toàn bộ [`AppState`]: DB, khoá mã hoá, audio, STT/TTS/LLM, MCP, vision,
/// embedder, và các thành phần thoại. KHÔNG spawn gì — xem
/// [`spawn_background_services`].
///
/// Chặn luồng (mở DB, nạp model ONNX/GGUF). Cả hai vỏ đều gọi nó ở giai đoạn
/// khởi động tuần tự, đúng như mã cũ, nên không đổi hành vi.
/// Bao nhiêu lớp LLM đẩy lên GPU khi người dùng KHÔNG đặt
/// `LIVA_LLM_N_GPU_LAYERS`.
///
/// # Vì sao không ghim cứng, cả 0 lẫn 99
///
/// Mặc định cũ là **0** — di sản từ thời build chỉ có CPU, không có dòng nào
/// biện minh. Hậu quả đo được ngày 02/08/2026: bản dựng CÓ CUDA vẫn để toàn bộ
/// 4 GB trọng số nằm ở RAM, `vision:ask` mất **64 s** thay vì **877 ms** — người
/// dùng phải tự biết mà đặt một biến môi trường không ai nói cho họ.
///
/// Nhưng ghim **99** cũng sai, và sai theo kiểu tệ hơn. [U1b] đã đo ca "build
/// CUDA, máy KHÔNG có GPU" → rơi về CPU sạch. Ca **chưa ai đo** là "có GPU
/// nhưng quá nhỏ": ở đó llama.cpp nhiều khả năng **CUDA OOM cứng** chứ không
/// rơi về CPU — và đó đúng là cấu hình của beta tester chạy laptop.
///
/// Nên quyết định bằng **số đo**: hỏi VRAM trống qua NVML (đã có sẵn cho ô VRAM
/// Guard trên Dashboard), so với kích thước thật của model + projector. Không
/// đọc được VRAM ⇒ **0**, vì không biết thì đừng đánh cược.
///
/// [U1b]: ../../docs/03-danh-gia/05-nang-cap-toan-dien.md
fn gpu_layers_mac_dinh() -> u32 {
    let can = [
        crate::configured_router_model_path(),
        crate::configured_mmproj_path(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
    .sum::<u64>();

    let Some((tong, dang_dung)) = crate::governor::gpu_vram_bytes() else {
        tracing::info!(
            "GPU: không đọc được VRAM (không có NVIDIA/driver) ⇒ chạy LLM trên CPU. \
             Ép bằng LIVA_LLM_N_GPU_LAYERS nếu bạn biết máy mình kham được."
        );
        return 0;
    };
    let trong = tong.saturating_sub(dang_dung);
    let layers = gpu_layers_theo_vram(trong, can);
    tracing::info!(
        "GPU: VRAM trống {} MiB · model+projector {} MiB · dự phòng {} MiB ⇒ n_gpu_layers={}",
        trong / (1024 * 1024),
        can / (1024 * 1024),
        DU_PHONG_VRAM / (1024 * 1024),
        layers
    );
    layers
}

/// Chỗ chừa cho KV cache + compute buffer, ngoài phần trọng số.
///
/// 2 GiB là số **thận trọng có chủ đích**, không phải số đo: đo thật ngày
/// 02/08 cho compute buffer 565 MiB ở `n_ctx` mặc định, nhưng KV cache lớn theo
/// `LIVA_LLM_N_CTX` mà hàm này không biết trước. Chọn sai về phía rộng thì mất
/// tốc độ; chọn sai về phía hẹp thì **OOM lúc đang phục vụ**, tệ hơn nhiều.
const DU_PHONG_VRAM: u64 = 2 * 1024 * 1024 * 1024;

/// Tách khỏi [`gpu_layers_mac_dinh`] để test được mà không cần GPU thật.
///
/// `can == 0` nghĩa là không đọc nổi kích thước model ⇒ trả 0: cùng nguyên tắc
/// "không biết thì đừng đánh cược".
fn gpu_layers_theo_vram(vram_trong: u64, can: u64) -> u32 {
    if can == 0 {
        return 0;
    }
    // `checked_add` chứ KHÔNG `saturating_add`: bão hoà làm `can + dự phòng`
    // quấn về `u64::MAX`, rồi `vram_trong < u64::MAX` thành false và hàm trả 99
    // — tức "không đủ" bị đọc ngược thành "đủ". Test
    // `khong_tran_khi_can_gan_u64_max` bắt được đúng ca đó ở bản đầu.
    let Some(can_tong) = can.checked_add(DU_PHONG_VRAM) else {
        return 0;
    };
    if vram_trong < can_tong {
        return 0;
    }
    // 99 = "tất cả các lớp". llama.cpp tự kẹp xuống số lớp thật của model.
    99
}

pub fn build_app_state() -> Result<Boot, BootError> {
    // Mặc định neo vào `data_dir()` — KHÔNG phải đường dẫn tương đối theo cwd.
    //
    // Bản cũ dùng `"data/agents/liva_core/…"` trần, nên mỗi cwd sinh một database
    // riêng: chạy từ gốc repo, từ `liva-native-core/`, và `tauri dev` (cwd là
    // `src-tauri/`) cho ba bản khác nhau cùng tồn tại. Người dùng thêm một liên
    // hệ rồi khởi động kiểu khác thì sổ danh bạ trống — không lỗi, không log.
    // Xem `crate::data_dir` để biết vì sao neo này khác bộ dò dùng cho model.
    let db_path = std::env::var("LIVA_DB_PATH").unwrap_or_else(|_| {
        crate::data_dir()
            .join("agents")
            .join("liva_core")
            .join("structured_memory.sqlite")
            .to_string_lossy()
            .into_owned()
    });
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Báo — KHÔNG tự di trú. Gộp hai file SQLite là thao tác mất mát tiềm tàng;
    // người dùng phải là người chọn giữ bản nào. Im lặng ở đây chính là cách lỗi
    // cũ ẩn mình suốt nhiều tuần.
    for lac in crate::stray_database_paths(std::path::Path::new(&db_path)) {
        let co = std::fs::metadata(&lac).map(|m| m.len()).unwrap_or(0);
        tracing::warn!(
            "Có database khác ở {} ({} byte) — KHÔNG được dùng. Trước đây đường dẫn DB \
             đi theo thư mục chạy nên mỗi cách khởi động sinh một bản riêng. Đang dùng: {}. \
             Nếu dữ liệu bạn cần nằm ở bản kia, chép đè thủ công hoặc trỏ LIVA_DB_PATH vào nó.",
            lac.display(),
            co,
            db_path
        );
    }

    // Mặc định false = DB trên đĩa. KHÔNG dùng `.is_ok()`: nó chỉ hỏi biến có
    // tồn tại hay không, nên `LIVA_DB_IN_MEMORY=false` — đúng như .env.example
    // hướng dẫn — lại bật in-memory và xoá sạch dữ liệu mỗi lần khởi động.
    let in_memory = env_flag("LIVA_DB_IN_MEMORY", false);
    let db = if in_memory {
        db::DatabasePool::new_in_memory().map_err(BootError::db)?
    } else {
        db::DatabasePool::new(&db_path).map_err(BootError::db)?
    };

    // BỎ KHOÁ MẶC ĐỊNH: khoá thật từ env → khoá thiết bị DPAPI (sinh mới nếu
    // chưa có), rồi rekey facts về nó để cứu dữ liệu đang mã bằng khoá cũ.
    let boot_crypto = crate::resolve_and_rekey(&db, std::path::Path::new(&db_path), in_memory)
        .map_err(|e| {
            BootError::new(
                "Không thiết lập được khoá mã hoá. Nếu Windows vừa bị cài lại/đổi user, \
                 đặt LIVA_ENCRYPTION_KEY = khoá đã sao lưu để khôi phục",
                e,
            )
        })?;

    let (audio_stream, audio_handle) = match rodio::OutputStream::try_default() {
        Ok((s, h)) => (Some(s), Some(h)),
        // Không có thiết bị ra âm thanh KHÔNG phải lỗi chí mạng: chat, bộ nhớ,
        // vision vẫn chạy; chỉ là không nghe được tiếng.
        Err(e) => {
            error!("Không mở được thiết bị âm thanh mặc định: {e}. LIVA sẽ không phát tiếng.");
            (None, None)
        }
    };
    let sink = audio_handle
        .as_ref()
        .and_then(|h| match rodio::Sink::try_new(h) {
            Ok(s) => Some(s),
            Err(e) => {
                error!("Không tạo được rodio Sink: {e}. LIVA sẽ không phát tiếng.");
                None
            }
        });

    // Đường dẫn model tương đối phải resolve theo gốc repo THẬT: gateway chạy
    // từ repo root hoặc liva-native-core, còn Tauri chạy với cwd =
    // liva-desktop/src-tauri.
    let env_path = |key: &str, mac_dinh: &str| -> String {
        crate::resolve_resource_path(&std::env::var(key).unwrap_or_else(|_| mac_dinh.to_string()))
            .to_string_lossy()
            .into_owned()
    };
    let stt_model_dir = env_path("LIVA_STT_MODEL_DIR", "models/nemotron-asr");
    let tts_model_path = env_path("LIVA_TTS_MODEL_PATH", "models/kokoro-v1.0.onnx");
    let tts_voice_path = env_path(
        "LIVA_TTS_VOICE_PATH",
        "node_modules/kokoro-js/voices/af_heart.bin",
    );

    let stt_manager = stt::SttManager::new(&stt_model_dir);
    let shared_sink = sink.map(Arc::new);
    let tts_player = tts::audio::TtsAudioPlayer::new(shared_sink.clone());
    let tts_manager = match tts::TtsManager::from_bin(&tts_model_path, &tts_voice_path, shared_sink)
    {
        Ok(m) => Some(m),
        Err(e) => {
            error!("Không khởi tạo được TtsManager: {e}. Lệnh TTS sẽ báo lỗi.");
            None
        }
    };

    let llm_n_ctx = std::env::var("LIVA_LLM_N_CTX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4096);
    let llm_n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or_else(gpu_layers_mac_dinh);
    let llm_manager = llm::LlamaRouterManager::new(llm_n_ctx, llm_n_gpu_layers)
        .map_err(|e| BootError::new("Không khởi tạo được engine LLM (llama.cpp)", e))?;

    // Mặc định KHÔNG còn là đường dẫn tuyệt đối của máy dev: xem
    // `crate::default_vault_path`. Trên máy người dùng, `E:\Project\LIVA\...` là
    // một ổ đĩa không tồn tại, nên MCP khởi tạo xong là trỏ vào hư không.
    let vault_path = std::env::var("LIVA_VAULT_PATH")
        .unwrap_or_else(|_| crate::default_vault_path().to_string_lossy().into_owned());
    let mcp_server = Arc::new(crate::mcp::server::NativeMcpServer::new(&vault_path));

    let vision_manager = crate::vision::VisionManager::new(
        Arc::new(crate::vision::capture::NativeScreenCapturer::new(0)),
        crate::vision::VisionConfig::default(),
    );

    // Model embedding cho bộ nhớ dài hạn. Thiếu model KHÔNG phải lỗi chí mạng:
    // recall/persist bị bỏ qua và hệ thống chạy đúng như trước khi có RAG.
    let embedder = {
        let dir = llm::embedder::resolve_model_dir();
        match llm::embedder::EmbeddingEngine::load(&dir) {
            Ok(e) => {
                info!("Đã nạp model embedding từ {dir:?} — bộ nhớ dài hạn BẬT");
                Some(e)
            }
            Err(e) => {
                tracing::warn!("Bộ nhớ dài hạn TẮT: {e}");
                None
            }
        }
    };

    let voice = webrtc::session::VoiceRuntimeComponents::from_env(&stt_model_dir);

    let state = Arc::new(AppState {
        db,
        crypto: boot_crypto.engine,
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(tts_manager),
        tts_player,
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(voice.vad),
        denoiser: tokio::sync::Mutex::new(voice.denoiser),
        turn_shadow: tokio::sync::Mutex::new(voice.turn_shadow),
        aec: tokio::sync::Mutex::new(voice.aec),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
        embedder: tokio::sync::Mutex::new(embedder),
    });

    Ok(Boot {
        state,
        escrow_hex: boot_crypto.escrow_hex,
        crypto_source: boot_crypto.source,
        crypto_rekeyed: boot_crypto.rekeyed + boot_crypto.personal_data_rekeyed,
        crypto_locked: boot_crypto.locked + boot_crypto.personal_data_locked,
        audio_stream,
        llm_n_gpu_layers,
    })
}

/// Những chỗ hai vỏ THẬT SỰ khác nhau. Mọi thứ ngoài struct này là dùng chung.
pub struct ServiceOptions {
    /// Kênh ghi ra stdout của gateway. Bot Telegram đẩy vài thông điệp qua đây.
    /// App desktop không có stdout IPC ⇒ `None`, bot vẫn chạy bình thường.
    pub ipc_tx: Option<tokio::sync::mpsc::Sender<String>>,
    /// Gọi ngay sau khi WebSocket bind xong. App desktop dùng để emit
    /// `gateway-ready` cho cửa sổ; gateway không cần gì.
    pub on_gateway_ready: Option<Box<dyn Fn(std::net::SocketAddr) + Send + Sync>>,
    /// Trao session authority cho vỏ tin cậy sau khi WebSocket bind xong.
    /// Gateway độc lập không có WebView đặc quyền nên để `None`.
    pub on_websocket_sessions_ready:
        Option<Box<dyn Fn(crate::websocket::WebSocketSessionAuthority) + Send + Sync>>,
    /// Số lớp GPU ở chế độ thường — lấy từ [`Boot::llm_n_gpu_layers`].
    pub llm_n_gpu_layers: u32,
}

impl ServiceOptions {
    pub fn new(llm_n_gpu_layers: u32) -> Self {
        Self {
            ipc_tx: None,
            on_gateway_ready: None,
            on_websocket_sessions_ready: None,
            llm_n_gpu_layers,
        }
    }
}

/// Bật mọi dịch vụ nền và trả về handle để tắt sạch lúc thoát.
///
/// Danh sách này là **nguồn sự thật duy nhất** cho câu hỏi "LIVA chạy những gì
/// ở nền". Thêm dịch vụ mới thì thêm ở đây — không thêm vào một vỏ rồi quên vỏ
/// kia, đó chính là cách bốn khác biệt trong bảng đầu module sinh ra.
pub fn spawn_background_services(
    state: Arc<AppState>,
    opts: ServiceOptions,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut tasks = Vec::new();

    // 1. Chốt phóng chiếu event→vector ngoài đường nóng của chat.
    tasks.push(crate::memory_consolidation::spawn_projection_consumer(
        state.db.clone(),
    ));

    // 2. Retention chỉ chạy khi có policy opt-in. Mặc định không tự xóa dữ liệu.
    if let Some(policy) = crate::memory_retention::RetentionPolicy::from_env() {
        tasks.push(crate::memory_retention::spawn_retention_sweeper(
            state.db.clone(),
            policy,
        ));
    }

    // 3. Tự nạp model router đã cấu hình, để chat dùng được mà không phải gọi
    //    `llm:swap_model` bằng tay.
    {
        let state = state.clone();
        tasks.push(tokio::spawn(async move {
            crate::load_configured_router_model(state, false).await;
        }));
    }

    // 4. Hạ lớp GPU khi có game chạy: trả VRAM lại cho game, khôi phục khi
    //    thoát. Chỉ nạp lại khi trạng thái game ĐỔI (nạp lại rất đắt).
    {
        let state = state.clone();
        let normal_layers = opts.llm_n_gpu_layers;
        let game_layers = std::env::var("LIVA_GAME_N_GPU_LAYERS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        tasks.push(tokio::spawn(async move {
            if normal_layers == 0 || game_layers == normal_layers {
                return; // cấu hình CPU-only hoặc không có chênh lệch
            }
            let mut last_active: Option<bool> = None;
            loop {
                let active = governor::game_mode_active_now();
                if last_active != Some(active) {
                    let target = if active { game_layers } else { normal_layers };
                    // Chỉ chốt trạng thái khi model THẬT SỰ đã về đúng số lớp;
                    // chưa nạp xong thì thử lại ở nhịp sau.
                    if crate::reload_llm_gpu_layers(state.clone(), target).await {
                        last_active = Some(active);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }));
    }

    // 5. Máy chủ WebSocket (thoại + IPC).
    {
        let state = state.clone();
        let on_ready = opts.on_gateway_ready;
        let on_sessions_ready = opts.on_websocket_sessions_ready;
        tasks.push(tokio::spawn(async move {
            match crate::websocket::WebSocketServer::bind_from_env().await {
                Ok(server) => {
                    if let Some(cb) = on_sessions_ready {
                        cb(server.session_authority());
                    }
                    if let Some(cb) = on_ready {
                        cb(server.local_addr());
                    }
                    if let Err(error) = server.run(state).await {
                        error!("WebSocket server dừng: {error}");
                    }
                }
                Err(error) => error!("WebSocket server bind lỗi: {error}"),
            }
        }));
    }

    // 6. Giải phóng session TTS khi rảnh 5 phút.
    //
    //    Trước 26/07/2026 việc này CHỈ có ở gateway. App desktop — thứ người
    //    dùng thật sự chạy, và chạy cả ngày — không bao giờ trả lại session
    //    ONNX của Kokoro/Piper.
    {
        let state = state.clone();
        tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Some(tts) = state.tts.lock().await.as_ref() {
                    tts.check_idle_unload();
                }
            }
        }));
    }

    // 7. Bot Telegram khi có token.
    //
    //    Cũng chỉ có ở gateway trước đây: đặt `TELEGRAM_BOT_TOKEN` rồi mở app
    //    desktop thì bot **không chạy và không báo gì**. Nay hễ có token là bot
    //    chạy, ở bất kỳ vỏ nào — `get_system_status` phân biệt được
    //    "đã cấu hình" với "đang chạy" qua `telegram::bot_running()`.
    if let Some(token) = std::env::var("TELEGRAM_BOT_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
    {
        let allowed_ids: std::collections::HashSet<String> = std::env::var("TELEGRAM_ALLOWED_IDS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let state = state.clone();
        let ipc_tx = opts.ipc_tx;
        tasks.push(tokio::spawn(async move {
            Arc::new(telegram::TelegramBotManager::new(
                token,
                allowed_ids,
                state,
                ipc_tx,
            ))
            .start()
            .await;
        }));
    }

    // 8. Governor ưu tiên CPU: hạ tiến trình xuống BELOW_NORMAL khi có game.
    //
    //    Là `std::thread` chứ không phải task tokio vì `SetPriorityClass` tác
    //    động lên tiến trình và vòng lặp chỉ ngủ — không cần runtime async, và
    //    không nên chiếm một worker thread để ngủ.
    {
        let gov = Arc::new(governor::Governor::from_env());
        std::thread::spawn(move || {
            loop {
                let _ = gov.game_mode_active();
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        });
    }

    tasks
}

/// Huỷ mọi dịch vụ nền và đợi chúng dừng hẳn.
pub async fn stop_background_services(tasks: Vec<tokio::task::JoinHandle<()>>) {
    for task in &tasks {
        task.abort();
    }
    for task in tasks {
        match task.await {
            Ok(()) => {}
            Err(error) if error.is_cancelled() => {}
            Err(error) => tracing::warn!(%error, "dịch vụ nền dừng kèm lỗi"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gợi ý khắc phục phải nằm TRONG lỗi, không phải do mỗi vỏ tự nhớ bồi
    /// thêm — nếu không, vỏ nào quên thì vỏ đó đưa người dùng câu lỗi khó hiểu
    /// của rusqlite.
    #[test]
    fn loi_db_luon_kem_goi_y_khac_phuc() {
        let e = BootError::db("no such module: vec0");
        assert!(
            e.context.len() > "Không khởi tạo được cơ sở dữ liệu".len(),
            "phải có gợi ý bồi thêm, được: {}",
            e.context
        );
        assert!(e.to_string().contains("vec0"));
    }

    /// `ServiceOptions::new` phải mặc định là "không có gì đặc thù vỏ" — thêm
    /// một dịch vụ nền mới không được vô tình đòi vỏ phải cấu hình gì.
    #[test]
    fn service_options_mac_dinh_khong_doi_hoi_gi_o_vo() {
        let o = ServiceOptions::new(0);
        assert!(o.ipc_tx.is_none());
        assert!(o.on_gateway_ready.is_none());
        assert!(o.on_websocket_sessions_ready.is_none());
        assert_eq!(o.llm_n_gpu_layers, 0);
    }

    /// Khoá hồi quy cho chính việc gộp này.
    ///
    /// Không có gì trong trình biên dịch ngăn ai đó chép lại 155 dòng dựng
    /// `AppState` vào một vỏ — đó đúng là cách hai bản sao cũ ra đời. Test này
    /// đọc thẳng mã nguồn của hai vỏ và bắt lỗi ngay khi có vỏ tự dựng lại.
    ///
    /// Thiếu file thì BỎ QUA chứ không fail: crate này build được độc lập, và
    /// một test đỏ vì lý do đó chỉ dạy người ta bỏ qua test.
    #[test]
    fn khong_vo_nao_tu_dung_lai_app_state() {
        let goc = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let vo = [
            goc.join("src").join("main.rs"),
            goc.join("..")
                .join("liva-desktop")
                .join("src-tauri")
                .join("src")
                .join("lib.rs"),
        ];
        for duong_dan in vo {
            let Ok(ma) = std::fs::read_to_string(&duong_dan) else {
                continue; // không có file → không phải cây làm việc đầy đủ
            };
            // Bỏ phần `#[cfg(test)]`: test của vỏ được phép tự dựng state tối
            // thiểu, đó không phải đường khởi động production.
            let production = ma.split("#[cfg(test)]").next().unwrap_or("");
            assert!(
                !production.contains("AppState {"),
                "{} tự dựng AppState — phải đi qua boot::build_app_state()",
                duong_dan.display()
            );
            assert!(
                production.contains("build_app_state"),
                "{} không gọi boot::build_app_state()",
                duong_dan.display()
            );
            assert!(
                production.contains("spawn_background_services"),
                "{} không gọi boot::spawn_background_services()",
                duong_dan.display()
            );
        }
    }

    /// Huỷ danh sách rỗng phải là no-op, không treo.
    #[tokio::test]
    async fn dung_dich_vu_khi_danh_sach_rong() {
        stop_background_services(Vec::new()).await;
    }

    /// Mọi task đều bị huỷ, kể cả task đang ngủ vô hạn (đúng hình dạng của các
    /// vòng lặp poll ở trên).
    #[tokio::test]
    async fn huy_ca_task_dang_ngu_vo_han() {
        let mut tasks = Vec::new();
        for _ in 0..3 {
            tasks.push(tokio::spawn(async {
                std::future::pending::<()>().await;
            }));
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stop_background_services(tasks),
        )
        .await
        .expect("stop_background_services phải trả về, không được treo");
    }

    const GIB: u64 = 1024 * 1024 * 1024;

    /// Ba ca quyết định, và ca thứ hai là ca beta tester chạy laptop — thứ
    /// `LIVA_LLM_N_GPU_LAYERS=99` ghim cứng sẽ làm OOM.
    #[test]
    fn gpu_layers_chi_bat_khi_vram_du_ca_du_phong() {
        // Máy case: 16 GiB trống, model+projector ~5 GiB ⇒ thừa sức.
        assert_eq!(super::gpu_layers_theo_vram(16 * GIB, 5 * GIB), 99);

        // Laptop 6 GiB: đủ chứa trọng số nhưng KHÔNG đủ dự phòng 2 GiB.
        // Đây chính là ca sẽ OOM nếu ghim cứng 99.
        assert_eq!(super::gpu_layers_theo_vram(6 * GIB, 5 * GIB), 0);

        // Sát ranh giới: cần đúng bằng trống ⇒ vẫn từ chối, vì `<` là so với
        // can + dự phòng chứ không phải với can.
        assert_eq!(super::gpu_layers_theo_vram(5 * GIB, 5 * GIB), 0);
        assert_eq!(super::gpu_layers_theo_vram(7 * GIB, 5 * GIB), 99);
    }

    #[test]
    fn khong_biet_kich_thuoc_model_thi_khong_danh_cuoc() {
        // `can == 0` = không đọc nổi metadata file. Dù VRAM có bao nhiêu cũng
        // không được bật — bật ở đây là đoán, và đoán sai thì OOM.
        assert_eq!(super::gpu_layers_theo_vram(64 * GIB, 0), 0);
    }

    #[test]
    fn khong_tran_khi_can_gan_u64_max() {
        // `can + DU_PHONG` phải dùng saturating_add: tràn sẽ quấn về số nhỏ và
        // biến điều kiện thành "đủ VRAM", đúng ngược ý.
        assert_eq!(super::gpu_layers_theo_vram(u64::MAX, u64::MAX), 0);
    }
}
