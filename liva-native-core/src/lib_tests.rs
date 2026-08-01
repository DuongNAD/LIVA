use super::*;

#[cfg(test)]
mod data_dir_tests {
    use super::{data_dir, stray_database_paths};

    /// cwd là trạng thái TOÀN CỤC của tiến trình test, và một test dưới đây đổi
    /// nó. Mọi test đọc `data_dir()` phải giữ khoá này — nếu không, chúng đua
    /// nhau và đỏ ngẫu nhiên. Đã dính thật một lần trước khi thêm khoá vào test
    /// thứ hai; cùng đúng lớp lỗi vừa sửa ở `messaging::outbox`.
    static KHOA_CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn nam_khoa() -> std::sync::MutexGuard<'static, ()> {
        KHOA_CWD.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Bất biến chịu lực: **cùng một máy, khác thư mục chạy ⇒ CÙNG một database.**
    ///
    /// Đây là thứ bản cũ vi phạm và sinh ra ba database song song. Test đổi cwd
    /// sang từng điểm vào thật rồi đòi `data_dir()` trỏ về cùng một chỗ.
    #[test]
    fn moi_thu_muc_chay_deu_cho_cung_mot_thu_muc_du_lieu() {
        let _g = nam_khoa();
        let cu = std::env::current_dir().expect("cwd");

        // ⚠️ Neo vào GỐC REPO, không vào cwd. `cargo test` chạy với cwd là
        // `liva-native-core/`, nên `cu.join("liva-native-core")` không tồn tại
        // và bản đầu của test này chỉ tìm thấy MỘT điểm vào rồi thoát sớm —
        // xanh kể cả khi lỗi còn nguyên (đã thử: tiêm lại hành vi cũ, vẫn xanh).
        let goc = {
            let mut d = cu.clone();
            loop {
                if d.join("liva-native-core").is_dir() && d.join("liva-desktop").is_dir() {
                    break Some(d);
                }
                match d.parent() {
                    Some(p) => d = p.to_path_buf(),
                    None => break None,
                }
            }
        };
        let Some(goc) = goc else {
            return; // không nhận ra bố cục repo trên máy này
        };

        // Chạy từ gốc repo hay từ crate con đều phải ra cùng một nơi.
        let mut thay = Vec::new();
        for noi in ["", "liva-native-core", "liva-desktop/src-tauri"] {
            let dich = goc.join(noi);
            if !dich.is_dir() {
                continue;
            }
            std::env::set_current_dir(&dich).expect("đổi cwd");
            if let Ok(that) = data_dir().canonicalize() {
                thay.push((noi, that));
            }
        }
        std::env::set_current_dir(&cu).expect("trả cwd");

        if thay.len() < 2 {
            return; // không đủ điểm vào trên máy này để so
        }
        let dau = &thay[0].1;
        for (noi, duong) in &thay[1..] {
            assert_eq!(
                duong, dau,
                "chạy từ {noi:?} cho thư mục dữ liệu khác — đây đúng là lỗi đã sinh ra ba database"
            );
        }
    }

    /// Chỗ đang dùng KHÔNG được tự báo là lạc — nếu không, log sẽ kêu mỗi lần khởi động.
    #[test]
    fn khong_tu_bao_chinh_minh_la_lac() {
        let _g = nam_khoa();
        let dang_dung = data_dir()
            .join("agents")
            .join("liva_core")
            .join("structured_memory.sqlite");
        let lac = stray_database_paths(&dang_dung);
        for p in &lac {
            assert_ne!(
                p.canonicalize().ok(),
                dang_dung.canonicalize().ok(),
                "database đang dùng bị đếm là lạc"
            );
        }
    }
}

#[cfg(test)]
mod env_flag_tests {
    use super::env_flag;

    /// Các test env_flag phải chạy tuần tự: std::env là trạng thái toàn cục
    /// dùng chung cho cả tiến trình test.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_var<F: FnOnce()>(key: &str, val: Option<&str>, f: F) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var(key).ok();
        match val {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        f();
        match old {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    /// Đây CHÍNH LÀ bug F5: .env.example hướng dẫn ghi , code cũ dùng
    ///  nên hiểu thành BẬT và xoá sạch dữ liệu người dùng.
    #[test]
    fn f5_gia_tri_false_phai_la_tat() {
        with_var("LIVA_TEST_FLAG", Some("false"), || {
            assert!(!env_flag("LIVA_TEST_FLAG", false), "=false phải là TẮT");
            assert!(
                !env_flag("LIVA_TEST_FLAG", true),
                "=false phải thắng cả default=true"
            );
        });
    }

    #[test]
    fn nhan_moi_dang_bat() {
        for v in ["1", "true", "TRUE", "Yes", "ON", "  on  "] {
            with_var("LIVA_TEST_FLAG", Some(v), || {
                assert!(env_flag("LIVA_TEST_FLAG", false), "{:?} phải là BẬT", v);
            });
        }
    }

    #[test]
    fn nhan_moi_dang_tat() {
        for v in ["0", "false", "FALSE", "No", "OFF", " off "] {
            with_var("LIVA_TEST_FLAG", Some(v), || {
                assert!(!env_flag("LIVA_TEST_FLAG", true), "{:?} phải là TẮT", v);
            });
        }
    }

    #[test]
    fn khong_dat_bien_thi_dung_default() {
        with_var("LIVA_TEST_FLAG", None, || {
            assert!(!env_flag("LIVA_TEST_FLAG", false));
            assert!(env_flag("LIVA_TEST_FLAG", true));
        });
    }

    #[test]
    fn gia_tri_la_hoac_rong_thi_dung_default_khong_panic() {
        for v in ["", "  ", "maybe", "2", "tru"] {
            with_var("LIVA_TEST_FLAG", Some(v), || {
                assert!(
                    env_flag("LIVA_TEST_FLAG", true),
                    "{:?} phải rơi về default=true",
                    v
                );
                assert!(
                    !env_flag("LIVA_TEST_FLAG", false),
                    "{:?} phải rơi về default=false",
                    v
                );
            });
        }
    }
}

#[cfg(test)]
mod tracing_filter_tests {
    use super::tracing_env_filter;

    /// `std::env` là trạng thái toàn cục dùng chung cả tiến trình test.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_rust_log<F: FnOnce()>(val: Option<&str>, f: F) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("RUST_LOG").ok();
        match val {
            Some(v) => unsafe { std::env::set_var("RUST_LOG", v) },
            None => unsafe { std::env::remove_var("RUST_LOG") },
        }
        f();
        match old {
            Some(v) => unsafe { std::env::set_var("RUST_LOG", v) },
            None => unsafe { std::env::remove_var("RUST_LOG") },
        }
    }

    /// Không đặt RUST_LOG PHẢI ra `info` — đây là hành vi cũ
    /// (`.with_max_level(Level::INFO)`), và đổi nó đi là thay đổi ngầm cho mọi
    /// người đang chạy. `EnvFilter::from_default_env()` trơn sẽ ra ERROR-only,
    /// đúng cái bẫy hàm này tồn tại để tránh.
    #[test]
    fn khong_dat_rust_log_thi_la_info_dung_nhu_truoc() {
        with_rust_log(None, || {
            assert_eq!(tracing_env_filter().to_string(), "info");
        });
        with_rust_log(Some("   "), || {
            assert_eq!(
                tracing_env_filter().to_string(),
                "info",
                "RUST_LOG rỗng cũng phải rơi về info"
            );
        });
    }

    /// Đây là điều KHÔNG làm được trước 26/07/2026: mọi `debug!` trong crate là
    /// code chết vì subscriber hard-code INFO.
    #[test]
    fn bat_duoc_debug_cho_mot_module() {
        with_rust_log(Some("info,liva_native_core::mcp=debug"), || {
            let s = tracing_env_filter().to_string();
            assert!(
                s.contains("liva_native_core::mcp=debug"),
                "phải giữ nguyên directive, nhận được: {s}"
            );
        });
    }

    /// RUST_LOG sai cú pháp không được âm thầm đổi hành vi log, cũng không được
    /// làm chết tiến trình — rơi về `info` (và hàm `eprintln!` cảnh báo).
    #[test]
    fn rust_log_sai_cu_phap_thi_roi_ve_info_khong_panic() {
        for xau in ["=", "info,=debug", "liva=khong_phai_muc_log"] {
            with_rust_log(Some(xau), || {
                assert_eq!(
                    tracing_env_filter().to_string(),
                    "info",
                    "{xau:?} phải rơi về info"
                );
            });
        }
    }
}

#[cfg(test)]
mod origin_allowed_tests {
    use super::{DEFAULT_WS_ALLOWED_ORIGINS, origin_allowed};

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn without_extra<F: FnOnce()>(f: F) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_WS_ALLOWED_ORIGINS").ok();
        unsafe { std::env::remove_var("LIVA_WS_ALLOWED_ORIGINS") };
        f();
        if let Some(v) = old {
            unsafe { std::env::set_var("LIVA_WS_ALLOWED_ORIGINS", v) }
        }
    }

    #[test]
    fn cho_qua_cac_origin_mac_dinh() {
        without_extra(|| {
            for o in DEFAULT_WS_ALLOWED_ORIGINS {
                assert!(origin_allowed(Some(o)), "{} phai duoc phep", o);
            }
        });
    }

    /// Đây là ca tấn công thật: một trang web bất kỳ mở WebSocket tới 8002.
    #[test]
    fn chan_trang_web_la() {
        without_extra(|| {
            for o in [
                "https://evil.example",
                "http://evil.example",
                "null",
                "http://localhost:3000",
                "http://localhost:5174",
            ] {
                assert!(!origin_allowed(Some(o)), "{} phai bi chan", o);
            }
        });
    }

    /// Không có Origin = client gốc (Tauri, verify_duplex) → cho qua. Đây là
    /// đánh đổi có chủ ý, test này khoá lại hành vi đó cho khỏi đổi ngầm.
    #[test]
    fn khong_co_origin_thi_cho_qua() {
        without_extra(|| assert!(origin_allowed(None)));
    }

    #[test]
    fn origin_rong_thi_chan() {
        without_extra(|| {
            assert!(!origin_allowed(Some("")));
            assert!(!origin_allowed(Some("   ")));
        });
    }

    #[test]
    fn khong_khop_tien_to_hay_hau_to() {
        without_extra(|| {
            // ke tan cong dat domain chua chuoi hop le
            assert!(!origin_allowed(Some("http://localhost:5173.evil.example")));
            assert!(!origin_allowed(Some(
                "https://evil.example/http://localhost:5173"
            )));
            assert!(!origin_allowed(Some("http://localhost:51730")));
        });
    }

    #[test]
    fn mo_rong_bang_bien_moi_truong() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_WS_ALLOWED_ORIGINS").ok();
        unsafe {
            std::env::set_var(
                "LIVA_WS_ALLOWED_ORIGINS",
                " http://my.app , http://other.app ",
            )
        };
        assert!(origin_allowed(Some("http://my.app")));
        assert!(origin_allowed(Some("http://other.app")));
        assert!(!origin_allowed(Some("http://third.app")));
        // dau phay thua khong duoc bien thanh chuoi rong khop tat ca
        unsafe { std::env::set_var("LIVA_WS_ALLOWED_ORIGINS", ",,") };
        assert!(!origin_allowed(Some("https://evil.example")));
        match old {
            Some(v) => unsafe { std::env::set_var("LIVA_WS_ALLOWED_ORIGINS", v) },
            None => unsafe { std::env::remove_var("LIVA_WS_ALLOWED_ORIGINS") },
        }
    }
}

#[cfg(test)]
mod validate_model_path_tests {
    use super::validate_model_path;
    use std::path::Path;

    #[test]
    fn cho_phep_gguf_trong_thu_muc_model() {
        let dir = Path::new("models_root");
        assert!(validate_model_path(Path::new("router.gguf"), dir).is_ok());
        assert!(validate_model_path(Path::new("sub/expert.gguf"), dir).is_ok());
        assert!(
            validate_model_path(Path::new("A.GGUF"), dir).is_ok(),
            "duoi khong phan biet hoa thuong"
        );
    }

    #[test]
    fn chan_traversal_va_duoi_sai() {
        let dir = Path::new("models_root");
        // C2: đây là các payload đường-dẫn-tuỳ-ý phải bị chặn trước khi tới
        // parser C++ của llama.cpp.
        assert!(
            validate_model_path(Path::new("../secret.gguf"), dir).is_err(),
            ".."
        );
        assert!(
            validate_model_path(Path::new("sub/../../x.gguf"), dir).is_err(),
            ".. giua"
        );
        assert!(
            validate_model_path(Path::new("router.txt"), dir).is_err(),
            "duoi khong phai gguf"
        );
        assert!(
            validate_model_path(Path::new("no_ext"), dir).is_err(),
            "khong co duoi"
        );
    }
}

#[cfg(test)]
mod config_update_tests {
    use super::update_config_file_at;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_config_path(label: &str) -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "liva-config-{label}-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn malformed_config_is_preserved_instead_of_overwritten() {
        let path = temp_config_path("malformed");
        let original = "{ definitely-not-json";
        std::fs::write(&path, original).expect("write malformed fixture");

        let result = update_config_file_at(&path, &serde_json::json!({"ai": {"topP": 0.8}}));

        assert!(result.is_err(), "malformed config must fail closed");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved fixture"),
            original
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn non_object_patch_cannot_replace_the_entire_config() {
        let path = temp_config_path("non-object");
        let original = serde_json::json!({
            "ai": {"provider": "local"},
            "voice": {"enabled": true}
        })
        .to_string();
        std::fs::write(&path, &original).expect("write config fixture");

        let result = update_config_file_at(&path, &serde_json::Value::Null);

        assert!(result.is_err(), "config patch must be a JSON object");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved config"),
            original
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn non_object_existing_config_is_preserved_instead_of_replaced() {
        let path = temp_config_path("non-object-existing");
        let original = serde_json::json!(["unexpected", "root"]).to_string();
        std::fs::write(&path, &original).expect("write non-object config fixture");

        let result = update_config_file_at(&path, &serde_json::json!({"ai": {"topP": 0.8}}));

        assert!(
            result.is_err(),
            "an existing config with a non-object root must fail closed"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved config"),
            original
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_config_patches_do_not_lose_each_other() {
        let path = temp_config_path("concurrent");
        std::fs::write(
            &path,
            serde_json::json!({"ai": {"temperature": 0.3}, "ui": {"theme": "dark"}}).to_string(),
        )
        .expect("write initial config");

        let first_path = path.clone();
        let first = tokio::task::spawn_blocking(move || {
            update_config_file_at(&first_path, &serde_json::json!({"ai": {"topP": 0.8}}))
        });
        let second_path = path.clone();
        let second = tokio::task::spawn_blocking(move || {
            update_config_file_at(
                &second_path,
                &serde_json::json!({"ui": {"widgetPosition": "top-left"}}),
            )
        });

        first
            .await
            .expect("first writer task")
            .expect("first patch");
        second
            .await
            .expect("second writer task")
            .expect("second patch");

        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read merged config"))
                .expect("config remains valid JSON");
        assert_eq!(config["ai"]["temperature"], 0.3);
        assert_eq!(config["ai"]["topP"], 0.8);
        assert_eq!(config["ui"]["theme"], "dark");
        assert_eq!(config["ui"]["widgetPosition"], "top-left");
        let _ = std::fs::remove_file(path);
    }
}

/// Khoá hồi quy cho [`system_status`].
///
/// Loại test này cố ý viết theo kiểu "hằng số cũ KHÔNG được xuất hiện lại": lỗi
/// ở đây không phải lỗi logic mà là lỗi **bịa số**, và cách duy nhất để nó không
/// lặng lẽ quay lại là ghim từng giá trị giả cũ vào một assert có tên.
#[cfg(test)]
mod system_status_tests {
    use super::*;

    /// `AppState` tối thiểu: không TTS, không VAD/denoise/AEC/embedder, STT trỏ
    /// vào thư mục không tồn tại. Đây đúng là hình trạng của một máy vừa clone
    /// về chưa tải model — và bảng sức khoẻ phải nói ĐÚNG điều đó.
    fn state_toi_thieu() -> Arc<AppState> {
        unsafe {
            std::env::set_var("LIVA_ENCRYPTION_KEY", "00000000000000000000000000000000");
            std::env::remove_var("TELEGRAM_BOT_TOKEN");
        }
        let capturer = Arc::new(vision::capture::MockScreenCapturer::new(
            8,
            8,
            vision::capture::PixelFormat::Rgba,
        ));
        Arc::new(AppState {
            db: db::DatabasePool::new_in_memory().expect("in-memory db"),
            crypto: crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(stt::SttManager::new("khong_ton_tai_dau_ca")),
            tts: tokio::sync::Mutex::new(None),
            tts_player: tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(
                llm::LlamaRouterManager::new(512, 0).expect("llm manager"),
            ),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(mcp::server::NativeMcpServer::new("test_vault")),
            vision: tokio::sync::Mutex::new(vision::VisionManager::new(
                capturer,
                vision::VisionConfig::default(),
            )),
            embedder: tokio::sync::Mutex::new(None),
        })
    }

    /// Mười hai giá trị bịa của bản cũ, từng cái một.
    #[tokio::test]
    async fn khong_con_mot_hang_so_bia_dat_nao() {
        let s = system_status(state_toi_thieu()).await.expect("status");
        let hc = &s["healthChecks"];

        // Không ai kết nối ⇒ 0, không phải 1.
        assert_eq!(hc["gateway"]["wsClients"], 0);
        // Độ trễ chỉ đo được bằng cách CHẠY suy luận ⇒ không có số thì để null.
        assert!(hc["aiEngine"]["latencyMs"].is_null(), "latencyMs 10 giả");
        assert!(hc["voiceEngine"]["latencyMs"].is_null(), "latencyMs 5 giả");
        // Không có token ⇒ chưa cấu hình, không phải "online".
        assert_eq!(hc["remoteControl"]["telegram"]["status"], "not_configured");
        assert_eq!(hc["remoteControl"]["enabled"], false);
        // Zalo chưa từng tồn tại trong mã nguồn — "offline" nghe như đang tắt.
        assert_eq!(hc["remoteControl"]["zalo"]["status"], "not_configured");
        // Không có gRPC ở đâu cả.
        assert_ne!(s["engineMode"], "native_grpc");

        assert_ne!(s["osStats"]["cpuUsage"], 12, "cpuUsage cứng 12");
        assert_ne!(
            s["osStats"]["totalMemory"], 16_000_000_000u64,
            "RAM cứng 16 GB"
        );
        assert_ne!(
            s["osStats"]["freeMemory"], 8_000_000_000u64,
            "RAM trống cứng 8 GB"
        );
        assert_ne!(s["uptime"], 3600, "uptime cứng 1 giờ");
        assert_ne!(s["memoryUsage"], 50_000_000, "memoryUsage cứng 50 MB");
        assert_ne!(s["rssMemory"], 100_000_000, "rssMemory cứng 100 MB");
    }

    /// Máy chưa có model thì bảng phải BÁO LÀ CHƯA CÓ, không phải 8 đèn xanh.
    #[tokio::test]
    async fn may_thieu_model_khong_duoc_bao_toan_online() {
        let s = system_status(state_toi_thieu()).await.expect("status");
        let hc = &s["healthChecks"];

        assert_eq!(hc["whisper"]["status"], "offline", "STT thiếu model");
        assert_eq!(
            hc["voiceEngine"]["status"], "degraded",
            "thoại phải xuống cấp"
        );
        assert!(
            hc["whisper"]["detail"]
                .as_str()
                .is_some_and(|d| d.contains("thiếu model")),
            "detail phải nói thiếu ở đâu, được: {:?}",
            hc["whisper"]["detail"]
        );
        // NVML không có trên CI ⇒ "unknown", KHÔNG phải "online · 0% utilized".
        assert_ne!(
            hc["vramGuard"]["detail"], "0% utilized",
            "VRAM cứng 0% đã quay lại"
        );
    }

    /// Số nào không đo được phải là `null` — UI đã sẵn sàng hiện `--` cho null,
    /// nhưng sẽ vẽ một con số nếu ta trả 0.
    ///
    /// **Bản trước ĐỎ trên CI ngày 29/07/2026 vì chính nó sai, không phải mã
    /// sai:** nó đòi cả bốn trường phải `null` **hoặc > 0`, rồi nổ với
    /// `cpuUsage phải là null hoặc số dương thật, được: Number(0)`.
    ///
    /// Nhưng `cpuUsage` là **tải CPU NGOÀI LIVA** — nó trừ đi phần LIVA tự dùng
    /// (`GetProcessTimes`). Trên một runner rảnh thì **0 là số đo THẬT**, không
    /// phải số giả. Test đã gộp hai thứ khác hẳn nhau: *"0 vì không đo được"* và
    /// *"0 vì đúng là bằng 0"*. Nó xanh trên máy dev (luôn có gì đó chạy nền) và
    /// đỏ trên máy rảnh — đúng lớp "xanh cục bộ / đỏ CI" mà phiên này đã gặp ba
    /// lần ở ba chỗ khác nhau.
    ///
    /// Bản này tách theo **đơn vị**, vì ngưỡng hợp lệ phụ thuộc đơn vị:
    /// - **phần trăm** (`cpuUsage`, `livaCpuUsage`, `gpuUsage`): `null` hoặc
    ///   `0..=100`. Số 0 hợp lệ; cận trên bắt được lớp lỗi "đảo thứ tự
    ///   (tổng, đang dùng)" đã cắn ở bẫy 1 của U3.
    /// - **số byte** (`totalMemory`): `null` hoặc `> 0` — không máy nào có 0 byte
    ///   RAM tổng, nên ở đây 0 ĐÚNG là dấu hiệu số giả. Giữ nguyên độ nghiêm.
    /// - `freeMemory`: khẳng định thứ mạnh hơn "dương" — nó phải **≤
    ///   `totalMemory`**. Bất biến này bắt được cả số giả lẫn ca đảo cặp
    ///   `(tổng, trống)`, thứ mà một phép kiểm "> 0" cho qua im lặng.
    #[tokio::test]
    async fn khong_do_duoc_thi_null_chu_khong_phai_khong() {
        let s = system_status(state_toi_thieu()).await.expect("status");

        for truong in ["cpuUsage", "livaCpuUsage", "gpuUsage"] {
            let v = &s["osStats"][truong];
            assert!(
                v.is_null() || v.as_u64().is_some_and(|n| n <= 100),
                "{truong} là phần trăm ⇒ phải null hoặc 0..=100, được: {v:?}"
            );
        }

        let tong = &s["osStats"]["totalMemory"];
        assert!(
            tong.is_null() || tong.as_u64().is_some_and(|n| n > 0),
            "totalMemory phải null hoặc > 0 (máy nào cũng có RAM), được: {tong:?}"
        );

        let trong = &s["osStats"]["freeMemory"];
        assert!(
            trong.is_null() || trong.as_u64().is_some(),
            "freeMemory phải null hoặc là số, được: {trong:?}"
        );
        if let (Some(t), Some(f)) = (tong.as_u64(), trong.as_u64()) {
            assert!(
                f <= t,
                "freeMemory ({f}) > totalMemory ({t}) — cặp (tổng, trống) bị đảo?"
            );
        }

        for truong in ["uptime", "memoryUsage", "rssMemory"] {
            let v = &s[truong];
            assert!(
                v.is_null() || v.as_u64().is_some(),
                "{truong} phải là null hoặc số, được: {v:?}"
            );
        }
    }

    /// DB in-memory dựng được ⇒ ô "memory" phải đọc số THẬT từ DB đó.
    #[tokio::test]
    async fn o_memory_doc_so_that_tu_db() {
        let s = system_status(state_toi_thieu()).await.expect("status");
        let detail = s["healthChecks"]["memory"]["detail"]
            .as_str()
            .expect("memory.detail")
            .to_string();
        assert!(detail.contains("ký ức"), "phải đếm ký ức thật: {detail}");
        assert!(
            detail.contains("journal"),
            "phải báo journal mode: {detail}"
        );
        assert_ne!(
            s["healthChecks"]["memory"]["detail"], "WAL Active",
            "chuỗi cứng 'WAL Active' đã quay lại"
        );
    }

    /// Lock bận không được làm lệnh trạng thái đứng chờ: giữ `state.llm` rồi gọi
    /// `system_status` vẫn phải trả về ngay, với `"busy"`.
    #[tokio::test]
    async fn lock_ban_thi_bao_busy_chu_khong_dung_cho() {
        let state = state_toi_thieu();
        let giu = state.llm.lock().await; // mô phỏng một lượt sinh chữ đang chạy

        let s = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            system_status(state.clone()),
        )
        .await
        .expect("system_status KHÔNG được chờ lock")
        .expect("status");

        assert_eq!(s["healthChecks"]["aiEngine"]["status"], "busy");
        // Không cầm được lock thì KHÔNG biết engine đã nạp hay chưa — `null`,
        // không đoán bừa `true`.
        assert!(
            s["modelLoaded"].is_null(),
            "bận thì không đoán trạng thái nạp"
        );
        drop(giu);
    }
}
