//! E2E cho kho skill (rung G2) qua **lớp lệnh** `handle_command`.
//!
//! Vì sao cần dù `src/skills/` đã có 26 unit test: các test đó gọi thẳng vào
//! module. Không cái nào chứng minh năm arm `skills:*` tồn tại trong
//! `handle_command` — mà một arm quên nối sẽ rơi vào nhánh `_ =>` và trả
//! "Unknown command", đúng loại lỗi chỉ lộ ra khi có người gõ lệnh thật. Cùng lý
//! do `tests/mcp_client_e2e.rs` có mục kiểm dispatch riêng.
//!
//! Kho skill dùng ở đây là **cây tạm tự dựng**, không phải `.claude/skills` của
//! repo: `skills:pin_ids` GHI đĩa, nên test tuyệt đối không được trỏ nó vào cây
//! nguồn.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{AppState, db, handle_command, llm, stt, tts};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn state_test() -> Arc<AppState> {
    let mock = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
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
            mock,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

/// Cây skill tạm: hai skill hợp lệ + một cái hỏng (để kiểm nó bị bỏ qua chứ không
/// làm hỏng cả lượt quét).
fn cay_skill_tam() -> PathBuf {
    let goc = std::env::temp_dir().join(format!(
        "liva-g2-cmd-{}-{}",
        std::process::id(),
        uuid_don_gian()
    ));
    let viet = |dir: &Path, s: &str| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), s).unwrap();
    };
    viet(
        &goc.join("review-diff"),
        "---\nname: review-diff\ndescription: Review a pull request diff carefully\n---\n\nlook at the diff and comment on risky hunks\n",
    );
    viet(
        &goc.join("nested/migrate-db"),
        "---\nname: migrate-db\ndescription: Add a SQLite migration safely\n---\n\nPRAGMA user_version, one transaction per step\n",
    );
    viet(&goc.join("hong"), "khong co front matter\n");
    goc
}

/// Các test dưới đây đặt `LIVA_SKILLS_DIR`, mà `std::env` là trạng thái **toàn
/// cục của tiến trình** — libtest chạy song song trong MỘT binary, nên không tuần
/// tự hoá là chúng đọc mất kho của nhau. Cùng khuôn `LOCK` mà
/// `lib.rs::env_flag_tests` đã dùng.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Giữ `LIVA_SKILLS_DIR` trỏ vào một kho, và **tự phục hồi khi drop** — kể cả khi
/// test panic giữa đường. Dùng guard chứ không dùng closure để không thể quên
/// phục hồi.
struct KhoTam {
    _g: std::sync::MutexGuard<'static, ()>,
    cu: Option<String>,
}

impl KhoTam {
    fn moi(root: &str) -> Self {
        let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cu = std::env::var("LIVA_SKILLS_DIR").ok();
        unsafe { std::env::set_var("LIVA_SKILLS_DIR", root) };
        Self { _g: g, cu }
    }
}

impl Drop for KhoTam {
    fn drop(&mut self) {
        match &self.cu {
            Some(v) => unsafe { std::env::set_var("LIVA_SKILLS_DIR", v) },
            None => unsafe { std::env::remove_var("LIVA_SKILLS_DIR") },
        }
    }
}

fn uuid_don_gian() -> String {
    // Tránh phụ thuộc `uuid` ở tầng test: đủ ngẫu nhiên để không đụng thư mục.
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )
}

/// Cả năm arm `skills:*` phải tồn tại và làm đúng việc, qua đúng lớp lệnh.
#[tokio::test]
async fn nam_lenh_skills_da_noi_vao_dispatch() {
    let goc = cay_skill_tam();
    let _kho = KhoTam::moi(&goc.to_string_lossy());
    let state = state_test();

    // ── sync: đọc đĩa → DB ─────────────────────────────────────────────────
    let s = handle_command(
        Arc::clone(&state),
        "skills:sync",
        json!({}),
        None,
        None,
    )
    .await
    .expect("skills:sync phải chạy");
    assert_eq!(s["skills"], json!(2), "2 skill hợp lệ, skill hỏng bị bỏ qua");
    assert_eq!(s["newVersions"], json!(2), "lần đầu ⇒ 2 version gốc");

    // Sync lại: KHÔNG được sinh version mới. Đây là ca thường gặp nhất.
    let s2 = handle_command(
        Arc::clone(&state),
        "skills:sync",
        json!({}),
        None,
        None,
    )
    .await
    .expect("sync lần hai");
    assert_eq!(s2["skills"], json!(2));
    assert_eq!(s2["newVersions"], json!(0), "nội dung không đổi ⇒ 0 version mới");

    // ── list ───────────────────────────────────────────────────────────────
    let l = handle_command(Arc::clone(&state), "skills:list", json!({}), None, None)
        .await
        .expect("skills:list");
    assert_eq!(l["count"], json!(2));
    let ten: Vec<String> = l["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ten, vec!["migrate-db", "review-diff"], "sắp theo name");
    let skill_id = l["skills"][0]["skillId"].as_str().unwrap().to_string();
    assert!(
        l["skills"][0]["currentVersionId"].is_string(),
        "phải có version hiện hành"
    );

    // ── search ─────────────────────────────────────────────────────────────
    let r = handle_command(
        Arc::clone(&state),
        "skills:search",
        json!({ "query": "sqlite migration", "topK": 1 }),
        None,
        None,
    )
    .await
    .expect("skills:search");
    assert_eq!(r["results"][0]["name"], json!("migrate-db"));
    // `AppState.embedder` là None trong test ⇒ không rerank, và lệnh phải NÓI RA.
    assert_eq!(
        r["reranked"], json!(false),
        "thiếu model embedding thì phải báo chưa rerank, không im lặng"
    );

    // ── history ────────────────────────────────────────────────────────────
    let h = handle_command(
        Arc::clone(&state),
        "skills:history",
        json!({ "skillId": skill_id }),
        None,
        None,
    )
    .await
    .expect("skills:history");
    assert_eq!(h["versions"].as_array().unwrap().len(), 1);
    assert!(h["versions"][0]["parentId"].is_null(), "version gốc không có cha");

    // ── pin_ids: hành động GHI ĐĨA ──────────────────────────────────────────
    assert!(
        !goc.join("review-diff/.skill_id").exists(),
        "trước khi pin thì KHÔNG được có .skill_id — sync/search/list phải thuần đọc"
    );
    let pin = handle_command(
        Arc::clone(&state),
        "skills:pin_ids",
        json!({}),
        None,
        None,
    )
    .await
    .expect("skills:pin_ids");
    assert_eq!(pin["pinned"], json!(2));
    assert_eq!(pin["skipped"], json!(0));
    assert!(goc.join("review-diff/.skill_id").is_file());

    // Pin lần hai: không đụng file đã có.
    let pin2 = handle_command(
        Arc::clone(&state),
        "skills:pin_ids",
        json!({}),
        None,
        None,
    )
    .await
    .expect("pin lần hai");
    assert_eq!(pin2["pinned"], json!(0));
    assert_eq!(pin2["skipped"], json!(2));

    let _ = std::fs::remove_dir_all(&goc);
}

/// Thiếu tham số bắt buộc phải cho lỗi CHỈ ĐƯỜNG, và tuyệt đối không phải
/// "Unknown command" — đó là dấu hiệu arm chưa được nối.
#[tokio::test]
async fn thieu_tham_so_thi_bao_loi_chi_duong() {
    let _kho = KhoTam::moi("khong-ton-tai-dau-g2-xxx");
    let state = state_test();

    let e = handle_command(Arc::clone(&state), "skills:search", json!({}), None, None)
        .await
        .expect_err("thiếu query phải lỗi");
    assert!(e.contains("query"), "{e}");
    assert!(!e.contains("Unknown command"), "arm chưa được nối: {e}");

    let e = handle_command(Arc::clone(&state), "skills:history", json!({}), None, None)
        .await
        .expect_err("thiếu skillId phải lỗi");
    assert!(e.contains("skills:list"), "lỗi phải chỉ cách xem danh sách: {e}");
}

/// Thư mục skill không tồn tại phải báo lỗi đọc được, không panic.
#[tokio::test]
async fn thu_muc_khong_ton_tai_thi_bao_loi_doc_duoc() {
    let _kho = KhoTam::moi("khong-ton-tai-dau-g2-xxx");
    let state = state_test();
    let e = handle_command(
        Arc::clone(&state),
        "skills:sync",
        json!({}),
        None,
        None,
    )
    .await
    .expect_err("phải lỗi");
    assert!(e.contains("không phải thư mục"), "{e}");
}

/// Skill trống rỗng: `list`/`history` phải trả rỗng chứ không lỗi.
#[tokio::test]
async fn kho_rong_thi_tra_rong() {
    let _kho = KhoTam::moi("khong-ton-tai-dau-g2-xxx");
    let state = state_test();
    let l = handle_command(Arc::clone(&state), "skills:list", json!({}), None, None)
        .await
        .expect("list trên kho rỗng");
    assert_eq!(l["count"], json!(0));

    let h = handle_command(
        Arc::clone(&state),
        "skills:history",
        json!({ "skillId": "khong-co" }),
        None,
        None,
    )
    .await
    .expect("history cho id lạ không được lỗi");
    assert_eq!(h["versions"].as_array().unwrap().len(), 0);
}
