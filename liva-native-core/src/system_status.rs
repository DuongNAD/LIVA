use std::sync::Arc;

use crate::{AppState, configured_router_model_path, governor, sysinfo, telegram, websocket};

/// Bảng sức khoẻ hệ thống cho Dashboard — **chỉ số đo thật**.
///
/// Bản trước in cứng 12 trường (`cpuUsage: 12`, `totalMemory: 16e9`,
/// `uptime: 3600`, `rssMemory: 100_000_000`, `voiceEngine.latencyMs: 5`, mọi
/// service `"online"`, `telegram: "online"`…). Chỉ `modelLoaded`/`model` là
/// thật. `SystemView.vue` poll nó **3 giây một lần** để vẽ 8 đèn xanh, nên
/// người dùng luôn thấy một hệ thống khoẻ mạnh — kể cả khi không có model nào,
/// không có ai kết nối, và bot Telegram chưa bao giờ chạy.
///
/// Hai quy ước của hàm này:
///
/// 1. **Không đo được thì `null`/`"unknown"`, không điền số mặc định.** UI đã
///    sẵn sàng cho việc đó (`?? -1`, `|| '--'`), nên một ô trống nói thật rẻ hơn
///    một con số đẹp nói dối.
/// 2. **`try_lock`, không `lock().await`.** Bản trước chờ `state.llm.lock()`:
///    trong lúc LLM đang sinh chữ, lock bị giữ suốt lượt sinh, nên một lệnh
///    "xem trạng thái" biến thành lệnh chờ vài giây — mà UI thì poll mỗi 3s,
///    hàng đợi dồn lại. Lock đang bận **cũng là thông tin thật**: báo `"busy"`.
///
/// Tách khỏi `handle_command` (đang là một `match` ~1 400 dòng) để test được
/// riêng và để phần thân này còn đọc được.
pub async fn system_status(state: Arc<AppState>) -> Result<serde_json::Value, String> {
    use serde_json::json;

    // --- LLM ---------------------------------------------------------------
    let (ai_status, ai_detail, model_name, model_loaded) = match state.llm.try_lock() {
        Ok(m) => {
            let name = m
                .current_model_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let loaded = m.engine.is_some();
            let detail = if loaded {
                format!(
                    "n_ctx {} · {} lớp GPU · mmproj {}",
                    m.n_ctx,
                    m.n_gpu_layers,
                    if m.mmproj_path.is_some() {
                        "có"
                    } else {
                        "không"
                    }
                )
            } else {
                "chưa nạp model".to_string()
            };
            (
                if loaded { "online" } else { "offline" },
                detail,
                Some(name),
                Some(loaded),
            )
        }
        // Lock bận = engine đang sinh chữ. Đó là "đang chạy", không phải "hỏng".
        //
        // Tên model vẫn báo được: lấy từ CẤU HÌNH, tức chính file mà autoload và
        // `llm:swap_model` nạp. Nếu để `null` ở đây thì ô "Model" trên Dashboard
        // sẽ nhấp nháy về `--` mỗi lần LIVA trả lời — mất một thông tin ổn định
        // chỉ vì một lock tạm thời. `modelLoaded` thì vẫn `null`: cái đó đúng là
        // không biết được khi không cầm được lock.
        Err(_) => (
            "busy",
            "đang sinh chữ".to_string(),
            configured_router_model_path()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())),
            None,
        ),
    };

    // --- STT ---------------------------------------------------------------
    // Báo engine THỰC SỰ đã nạp/fallback trong SttManager, không suy đoán lại
    // từ biến môi trường cấu hình — cấu hình muốn Parakeet nhưng load thất bại
    // thì runtime đang chạy Nemotron và Dashboard phải nói đúng điều đó.
    let (stt_status, stt_detail) = match state.stt.try_lock() {
        Ok(s) if s.model_dir.exists() => {
            let (engine, fallback_reason) = s.active_vietnamese_engine();
            let detail = match fallback_reason {
                Some(reason) => format!("{engine} · đã lùi engine: {reason}"),
                None if engine == "Parakeet-vi" => format!("{engine} · model đã nạp"),
                None if engine == "chưa xác định" => engine.to_string(),
                None => format!("{engine} · model có sẵn"),
            };
            ("online", detail)
        }
        Ok(s) => (
            "offline",
            format!("thiếu model tại {}", s.model_dir.display()),
        ),
        Err(_) => ("busy", "đang nhận dạng".to_string()),
    };

    // --- TTS + phụ trợ thoại ------------------------------------------------
    let (tts_status, tts_detail) = match state.tts.try_lock() {
        Ok(guard) => match guard.as_ref() {
            Some(t) => {
                let backends = t.loaded_backends();
                if backends.is_empty() {
                    (
                        "offline",
                        "TtsManager có nhưng KHÔNG backend nào nạp được".to_string(),
                    )
                } else {
                    (
                        "online",
                        format!("{} · giọng {}", backends.join(" → "), t.language()),
                    )
                }
            }
            None => ("offline", "TTS không khởi tạo được".to_string()),
        },
        Err(_) => ("busy", "đang phát".to_string()),
    };

    // Module phụ trợ có nạp được không. `try_lock` lỗi nghĩa là module đang
    // ĐƯỢC DÙNG ⇒ nó chắc chắn tồn tại. Viết bằng macro vì mỗi `Mutex` bọc một
    // kiểu khác nhau, mà closure Rust không nhận tham số `impl Trait`.
    macro_rules! co_module {
        ($m:expr) => {
            match $m.try_lock() {
                Ok(g) => g.is_some(),
                Err(_) => true,
            }
        };
    }
    let vad = co_module!(state.vad);
    let denoise = co_module!(state.denoiser);
    let aec = co_module!(state.aec);
    let turn_shadow = co_module!(state.turn_shadow);
    let embedder = co_module!(state.embedder);

    let voice_status = if stt_status == "offline" || tts_status == "offline" {
        "degraded"
    } else if stt_status == "busy" || tts_status == "busy" {
        "busy"
    } else {
        "online"
    };
    let voice_detail = format!(
        "TTS: {tts_detail} · VAD {} · khử ồn {} · AEC {} · turn-shadow {}",
        bat_tat(vad),
        bat_tat(denoise),
        bat_tat(aec),
        bat_tat(turn_shadow),
    );

    // --- DB ----------------------------------------------------------------
    // Truy vấn SQLite là I/O chặn — phải nằm trong `spawn_blocking`, nếu không
    // một lệnh poll 3 giây/lần sẽ chặn luồng async của cả runtime.
    let db_probe = {
        let db = state.db.clone();
        tokio::task::spawn_blocking(move || -> Result<(String, bool, i64), String> {
            let conn = db.readers.get().map_err(|e| e.to_string())?;
            let journal: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            // Cùng cách phát hiện vec0 mà `db::load_sqlite_vec` dùng.
            let vec0 = conn
                .query_row("SELECT vec_version()", [], |r| r.get::<_, String>(0))
                .is_ok();
            let facts: i64 = conn
                .query_row("SELECT count(*) FROM facts", [], |r| r.get(0))
                .unwrap_or(-1);
            Ok((journal, vec0, facts))
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {e}"))?
    };
    let (mem_status, mem_detail) = match &db_probe {
        Ok((journal, vec0, facts)) => (
            if *vec0 { "online" } else { "degraded" },
            format!(
                "journal {journal} · vec0 {} · {} ký ức · RAG {}",
                if *vec0 { "có" } else { "THIẾU" },
                if *facts < 0 {
                    "?".to_string()
                } else {
                    facts.to_string()
                },
                bat_tat(embedder),
            ),
        ),
        Err(e) => ("offline", format!("không mở được DB: {e}")),
    };

    // --- GPU / VRAM ---------------------------------------------------------
    let vram = governor::gpu_vram_bytes();
    let gpu_pct = governor::system_gpu_percent();
    let (vram_status, vram_detail) = match vram {
        Some((tong, dung)) if tong > 0 => (
            "online",
            format!(
                "VRAM {:.1}/{:.1} GB ({}%){}",
                dung as f64 / 1024.0_f64.powi(3),
                tong as f64 / 1024.0_f64.powi(3),
                dung * 100 / tong,
                match gpu_pct {
                    Some(p) => format!(" · tải ngoài {p}%"),
                    None => String::new(),
                }
            ),
        ),
        // Không có NVIDIA/driver thì KHÔNG biết gì về VRAM. Bản trước báo
        // "online · 0% utilized" trên mọi máy, kể cả máy chỉ có iGPU.
        _ => (
            "unknown",
            "không đọc được NVML (không có GPU NVIDIA hoặc thiếu driver)".to_string(),
        ),
    };

    // --- Cổng vào / kỹ năng / điều khiển từ xa -------------------------------
    let ws_clients = websocket::ws_client_count();
    // Lấy độ dài từ CHÍNH mảng mà `get_skills_list` trả về, để hai lệnh không
    // bao giờ nói hai con số khác nhau.
    let skills_loaded = state.mcp_server.list_skills().len();
    let mcp_tools = state.mcp_server.list_tools().tools.len();

    let tg_token = std::env::var("TELEGRAM_BOT_TOKEN")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let tg_running = telegram::bot_running();

    // --- Số đo hệ thống ------------------------------------------------------
    // `system_cpu_percent` so sánh hai mẫu liên tiếp nên lần gọi ĐẦU trả None —
    // UI poll 3s nên ô CPU trống đúng một nhịp rồi có số. Không lấp bằng 0.
    // MỘT lần lấy mẫu cho cả hai số. Gọi `system_cpu_percent()` rồi gọi tiếp
    // một hàm nữa sẽ làm hàm sau chỉ còn khoảng thời gian ~0 để chia — xem
    // cảnh báo ở `governor::cpu_sample`.
    let (cpu, liva_cpu) = match governor::cpu_sample() {
        Some((ngoai, cua_liva)) => (Some(ngoai), Some(cua_liva)),
        None => (None, None),
    };
    let ram = sysinfo::ram_bytes();
    let proc_mem = sysinfo::process_memory_bytes();

    // Record real-time hardware telemetry sample
    let telemetry_prof = crate::telemetry::global_telemetry();
    telemetry_prof.record_resource_sample(
        cpu.map(|c| c as f32),
        liva_cpu.map(|c| c as f32),
        gpu_pct,
        proc_mem.map(|(rss, _)| rss),
        proc_mem.map(|(_, commit)| commit),
    );

    let latest_ai_lat = telemetry_prof.latest_ttft_ms();
    let latest_voice_lat = telemetry_prof.latest_audio_latency_ms();
    let recent_telemetry_events = telemetry_prof.get_recent_events(Some(20));

    Ok(json!({
        "healthChecks": {
            "gateway": {
                "status": "online",
                "wsClients": ws_clients,
                "skillsLoaded": skills_loaded,
                "detail": format!("{ws_clients} client · {skills_loaded} kỹ năng · {mcp_tools} công cụ MCP"),
            },
            "aiEngine": {
                "status": ai_status,
                // Đo độ trễ sinh chữ đòi phải CHẠY một lượt suy luận.
                // Trả về độ trễ đo được gần nhất từ telemetry nếu có, hoặc null nếu chưa chạy.
                "latencyMs": latest_ai_lat,
                "detail": ai_detail,
            },
            // Không có "orchestrator" nào trong lõi Rust; thứ có thật là tầng
            // dispatch của `handle_command`. Nếu bạn đọc được phản hồi này thì
            // nó đang chạy — đó là toàn bộ những gì khẳng định được.
            "orchestrator": { "status": "online", "detail": "dispatch in-process" },
            "voiceEngine": {
                "status": voice_status,
                "latencyMs": latest_voice_lat,
                "detail": voice_detail,
            },
            "memory": { "status": mem_status, "detail": mem_detail },
            "vramGuard": {
                "status": vram_status,
                "detail": vram_detail,
                "isYielded": governor::game_mode_active_now(),
            },
            // Tên "whisper" là di sản của UI; engine thật là Nemotron/Parakeet.
            "whisper": { "status": stt_status, "detail": stt_detail },
            "remoteControl": {
                "enabled": tg_token,
                "telegram": {
                    "status": match (tg_token, tg_running) {
                        (false, _) => "not_configured",
                        (true, true) => "online",
                        // Có token mà bot không chạy: đúng tình trạng của vỏ
                        // Tauri, vì chỉ `main.rs` spawn bot.
                        (true, false) => "standby",
                    },
                },
                // Không có tích hợp Zalo trong mã nguồn. Trước đây báo
                // "offline" — nghe như một dịch vụ đang tắt, không phải một
                // dịch vụ chưa từng tồn tại.
                "zalo": { "status": "not_configured" },
            },
        },
        "osStats": {
            "cpuUsage": cpu,
            // Phần CPU của CHÍNH LIVA, cùng mẫu số với `cpuUsage` (U16). Có hai
            // số cạnh nhau mới nói được điều đáng nói: "máy bận 92 %, LIVA
            // chiếm 3 %". Một mình `cpuUsage` chỉ chứng minh máy đang bận, chứ
            // không chứng minh LIVA rẻ.
            "livaCpuUsage": liva_cpu,
            "gpuUsage": gpu_pct,
            "totalMemory": ram.map(|(t, _)| t),
            "freeMemory": ram.map(|(_, f)| f),
        },
        "telemetry": recent_telemetry_events,
        "uptime": sysinfo::process_uptime_secs(),
        // `memoryUsage` = commit charge. Rust không có heap do runtime quản lý
        // nên không có gì báo cáo dưới cái tên "heap" — xem `sysinfo`.
        "memoryUsage": proc_mem.map(|(_, commit)| commit),
        "rssMemory": proc_mem.map(|(rss, _)| rss),
        "engineMode": "native",
        "modelLoaded": model_loaded,
        "model": model_name,
    }))
}

/// `"có"`/`"không"` cho phần `detail` — gọn hơn `if` lặp lại sáu lần.
fn bat_tat(v: bool) -> &'static str {
    if v { "có" } else { "không" }
}
