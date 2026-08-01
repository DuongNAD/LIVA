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
use std::sync::atomic::{AtomicU64, Ordering};

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
    // Tránh phụ thuộc `uuid` ở tầng test.
    //
    // Đồng hồ treo tường MỘT MÌNH thì không đủ, và bản trước sai đúng ở chỗ đó:
    // `SystemTime::now()` trên Windows có độ phân giải 100 ns, nên các lời gọi
    // song song rơi vào cùng một tick và trả về **cùng một giá trị** — đo được
    // 24–37 % trùng trên 4 000 lời gọi từ 8 luồng. Vì `pid` cũng giống nhau
    // trong một tiến trình, hai test có thể nhận cùng một cây skill tạm, rồi
    // test xong trước xoá cây của test kia.
    //
    // Bộ đếm đơn điệu của tiến trình làm trùng lặp trở thành **không thể** thay
    // vì chỉ khó xảy ra; giữ phần nano ở đầu để tên vẫn đọc được và vẫn xếp
    // theo thời gian. Khác tiến trình thì đã khác `pid`.
    static DEM: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        DEM.fetch_add(1, Ordering::Relaxed)
    )
}

/// HỒI QUY — bộ sinh id phải duy nhất, KỂ CẢ khi bị gọi song song.
///
/// Ba test dưới đây gọi [`cay_skill_tam`] rồi, ở cuối, `remove_dir_all` cây của
/// chính mình. Tên cây chỉ gồm `pid` + đồng hồ treo tường, mà `pid` giống nhau
/// trong cùng tiến trình và `SystemTime::now()` trên Windows có độ phân giải
/// **100 ns** (giá trị đo được `1785214586634962900` chia hết cho 100). libtest
/// chạy các test song song, nên hai test rơi vào cùng một tick sẽ nhận **cùng
/// một đường dẫn** — rồi test xong trước xoá cây của test kia, và test kia chết
/// ở `skills:sync` với `không phải thư mục: …`.
///
/// Đây không phải giả thuyết: đã bắt được hai lần trong phiên rà soát
/// 28/07/2026, ở hai test khác nhau nhưng cùng một chữ ký lỗi —
/// `nam_lenh_skills_da_noi_vao_dispatch` và
/// `lenh_doc_so_cai_tach_lan_quan_sat_khoi_van_de` — cả hai báo
/// `sync: "không phải thư mục: …\liva-g2-cmd-41100-18c65b35443907d4"`.
/// Tần suất ~2/57 lần chạy và **chỉ khi máy có tải**, tức runner CI là môi
/// trường thuận lợi nhất cho nó.
///
/// Test này kiểm thẳng bất biến gốc thay vì đi kiểm hệ quả: đo trên **bộ sinh
/// id**, không phải trên cây thư mục — dựng cây có I/O nên mỗi vòng lặp mất
/// hàng chục micro giây, đủ để không bao giờ đụng tick và test sẽ xanh giả.
#[test]
fn uuid_don_gian_phai_duy_nhat_ke_ca_khi_goi_song_song() {
    const SO_LUONG: usize = 8;
    const MOI_LUONG: usize = 500;

    let tay: Vec<_> = (0..SO_LUONG)
        .map(|_| std::thread::spawn(|| (0..MOI_LUONG).map(|_| uuid_don_gian()).collect::<Vec<_>>()))
        .collect();

    let mut tat_ca = Vec::with_capacity(SO_LUONG * MOI_LUONG);
    for t in tay {
        tat_ca.extend(t.join().expect("luong con phai ket thuc binh thuong"));
    }

    let duy_nhat: std::collections::HashSet<&String> = tat_ca.iter().collect();
    assert_eq!(
        duy_nhat.len(),
        tat_ca.len(),
        "{} / {} id bi TRUNG — ten cay skill tam se dung nhau va test nay xoa cay cua test kia",
        tat_ca.len() - duy_nhat.len(),
        tat_ca.len()
    );
}

/// Cả năm arm `skills:*` phải tồn tại và làm đúng việc, qua đúng lớp lệnh.
#[tokio::test]
async fn nam_lenh_skills_da_noi_vao_dispatch() {
    let goc = cay_skill_tam();
    let _kho = KhoTam::moi(&goc.to_string_lossy());
    let state = state_test();

    // ── sync: đọc đĩa → DB ─────────────────────────────────────────────────
    let s = handle_command(Arc::clone(&state), "skills:sync", json!({}), None, None)
        .await
        .expect("skills:sync phải chạy");
    assert_eq!(
        s["skills"],
        json!(2),
        "2 skill hợp lệ, skill hỏng bị bỏ qua"
    );
    assert_eq!(s["newVersions"], json!(2), "lần đầu ⇒ 2 version gốc");

    // Sync lại: KHÔNG được sinh version mới. Đây là ca thường gặp nhất.
    let s2 = handle_command(Arc::clone(&state), "skills:sync", json!({}), None, None)
        .await
        .expect("sync lần hai");
    assert_eq!(s2["skills"], json!(2));
    assert_eq!(
        s2["newVersions"],
        json!(0),
        "nội dung không đổi ⇒ 0 version mới"
    );

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
        r["reranked"],
        json!(false),
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
    assert!(
        h["versions"][0]["parentId"].is_null(),
        "version gốc không có cha"
    );

    // ── pin_ids: hành động GHI ĐĨA ──────────────────────────────────────────
    assert!(
        !goc.join("review-diff/.skill_id").exists(),
        "trước khi pin thì KHÔNG được có .skill_id — sync/search/list phải thuần đọc"
    );
    let pin = handle_command(Arc::clone(&state), "skills:pin_ids", json!({}), None, None)
        .await
        .expect("skills:pin_ids");
    assert_eq!(pin["pinned"], json!(2));
    assert_eq!(pin["skipped"], json!(0));
    assert!(goc.join("review-diff/.skill_id").is_file());

    // Pin lần hai: không đụng file đã có.
    let pin2 = handle_command(Arc::clone(&state), "skills:pin_ids", json!({}), None, None)
        .await
        .expect("pin lần hai");
    assert_eq!(pin2["pinned"], json!(0));
    assert_eq!(pin2["skipped"], json!(2));

    let _ = std::fs::remove_dir_all(&goc);
}

/// G3 qua lớp lệnh: ghi tín hiệu → prior dịch chuyển thứ tự truy hồi.
///
/// Test này đi hết chuỗi thật: `skills:signal` ghi DB → `signal_tallies` đếm vấn đề
/// phân biệt → `rank_skills_with_prior` hoà vào thứ hạng → JSON trả ra. Unit test
/// trong `src/skills/` không chứng minh được chuỗi đó có được NỐI hay không.
///
/// Nó cũng ghim **độ nhạy thật** của prior, chứ không chỉ "có đổi thứ tự": một tín
/// hiệu là CHƯA đủ để lật, hai mới đủ. Con số đó là hệ quả của `BAO_HOA = 2` và
/// `LAMBDA_HANG = 3`; đổi hai hằng đó mà không đổi test là làm hỏng lặng lẽ.
#[tokio::test]
async fn tin_hieu_chat_luong_dich_chuyen_thu_tu_truy_hoi() {
    let goc = cay_skill_tam();
    let _kho = KhoTam::moi(&goc.to_string_lossy());
    let state = state_test();

    handle_command(Arc::clone(&state), "skills:sync", json!({}), None, None)
        .await
        .expect("sync");

    // Nền: `migrate-db` là skill khớp câu hỏi (không có embedder ⇒ BM25 thuần).
    let tim = |st: Arc<AppState>| async move {
        handle_command(
            st,
            "skills:search",
            json!({ "query": "sqlite migration", "topK": 2 }),
            None,
            None,
        )
        .await
        .expect("search")
    };
    let nen = tim(Arc::clone(&state)).await;
    assert_eq!(nen["results"][0]["name"], json!("migrate-db"), "tiền đề");
    assert_eq!(
        nen["priorApplied"],
        json!(false),
        "sổ cái rỗng ⇒ prior không tác động"
    );
    assert_eq!(nen["results"][0]["qualityPenalty"], json!(0.0));

    let id_migrate = nen["results"][0]["skillId"].as_str().unwrap().to_string();

    // ── Một tín hiệu: CHƯA đủ lật ───────────────────────────────────────────
    let ghi = |st: Arc<AppState>, id: String, mk: &'static str| async move {
        handle_command(
            st,
            "skills:signal",
            json!({
                "skillId": id,
                "kind": "tool_failure_affects_skill",
                "evidenceStatus": "confirmed",
                "mergeKey": mk,
            }),
            None,
            None,
        )
        .await
        .expect("skills:signal")
    };
    let s1 = ghi(Arc::clone(&state), id_migrate.clone(), "van-de-A").await;
    assert!(s1["signalId"].is_i64(), "phải trả id dòng vừa ghi: {s1}");

    let mot = tim(Arc::clone(&state)).await;
    assert_eq!(mot["priorApplied"], json!(true), "đã có tín hiệu");
    assert_eq!(
        mot["results"][0]["name"],
        json!("migrate-db"),
        "MỘT lỗi chưa đủ lật một skill đúng — prior phải là thứ phá thế cân bằng, \
         không phải thứ loại bỏ"
    );

    // ── Cùng vấn đề, quan sát thêm 10 lần: VẪN chưa đủ ──────────────────────
    for _ in 0..10 {
        ghi(Arc::clone(&state), id_migrate.clone(), "van-de-A").await;
    }
    let lap = tim(Arc::clone(&state)).await;
    assert_eq!(
        lap["results"][0]["name"],
        json!("migrate-db"),
        "11 lần quan sát CÙNG một vấn đề vẫn là MỘT vấn đề — nếu đây đỏ thì prior \
         đang đếm dòng chứ không đếm merge_key"
    );

    // ── Vấn đề THỨ HAI: giờ mới lật ─────────────────────────────────────────
    ghi(Arc::clone(&state), id_migrate.clone(), "van-de-B").await;
    let hai = tim(Arc::clone(&state)).await;
    assert_eq!(
        hai["results"][0]["name"],
        json!("review-diff"),
        "hai vấn đề phân biệt ⇒ skill hỏng phải tụt xuống dưới skill sạch"
    );
    // Nhưng nó vẫn còn trong kết quả, và giải thích được vì sao bị tụt.
    let m = hai["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == json!("migrate-db"))
        .expect("bị dìm, không bị loại");
    assert_eq!(m["relevanceRank"], json!(0), "liên quan nhất vẫn là nó");
    assert!(
        m["qualityPenalty"].as_f64().unwrap() >= 0.5,
        "và mức phạt phải đọc được: {m}"
    );

    let _ = std::fs::remove_dir_all(&goc);
}

/// `skills:signals` phải phơi ra CẢ HAI con số — số lần quan sát và số vấn đề — vì
/// chỗ chúng lệch nhau chính là thông tin: một sự cố đang lặp.
#[tokio::test]
async fn lenh_doc_so_cai_tach_lan_quan_sat_khoi_van_de() {
    let goc = cay_skill_tam();
    let _kho = KhoTam::moi(&goc.to_string_lossy());
    let state = state_test();

    handle_command(Arc::clone(&state), "skills:sync", json!({}), None, None)
        .await
        .expect("sync");
    let l = handle_command(Arc::clone(&state), "skills:list", json!({}), None, None)
        .await
        .expect("list");
    let id = l["skills"][0]["skillId"].as_str().unwrap().to_string();

    for _ in 0..4 {
        handle_command(
            Arc::clone(&state),
            "skills:signal",
            json!({ "skillId": id, "kind": "tool_call_failed", "mergeKey": "cung-mot-cai" }),
            None,
            None,
        )
        .await
        .expect("signal");
    }

    let r = handle_command(
        Arc::clone(&state),
        "skills:signals",
        json!({ "skillId": id }),
        None,
        None,
    )
    .await
    .expect("skills:signals");

    assert_eq!(r["observations"][0]["count"], json!(4), "4 LẦN");
    assert_eq!(r["issues"][0]["distinctIssues"], json!(1), "nhưng 1 VẤN ĐỀ");
    assert!(r["qualityPenalty"].as_f64().unwrap() > 0.0);
    assert!(r["weightTotal"].as_f64().unwrap() > 0.0);

    // Skill sạch: đọc được, không lỗi, và phạt bằng 0.
    let id2 = l["skills"][1]["skillId"].as_str().unwrap().to_string();
    let sach = handle_command(
        Arc::clone(&state),
        "skills:signals",
        json!({ "skillId": id2 }),
        None,
        None,
    )
    .await
    .expect("signals cho skill sạch");
    assert_eq!(sach["qualityPenalty"], json!(0.0));
    assert!(sach["issues"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&goc);
}

/// Hai arm G3 mới cũng phải cho lỗi CHỈ ĐƯỜNG khi thiếu tham số — cùng chuẩn với
/// năm arm G2.
#[tokio::test]
async fn hai_lenh_g3_thieu_tham_so_thi_bao_loi_chi_duong() {
    let _kho = KhoTam::moi("khong-ton-tai-dau-g3-xxx");
    let state = state_test();

    let e = handle_command(Arc::clone(&state), "skills:signal", json!({}), None, None)
        .await
        .expect_err("thiếu skillId phải lỗi");
    assert!(e.contains("skills:list"), "{e}");
    assert!(!e.contains("Unknown command"), "arm chưa được nối: {e}");

    // Có skillId nhưng thiếu `kind`: lỗi phải LIỆT KÊ bốn loại hợp lệ, vì người gõ
    // lệnh không có cách nào đoán ra chúng.
    let e = handle_command(
        Arc::clone(&state),
        "skills:signal",
        json!({ "skillId": "x" }),
        None,
        None,
    )
    .await
    .expect_err("thiếu kind phải lỗi");
    assert!(
        e.contains("tool_failure_affects_skill"),
        "phải liệt kê loại: {e}"
    );

    let e = handle_command(Arc::clone(&state), "skills:signals", json!({}), None, None)
        .await
        .expect_err("thiếu skillId phải lỗi");
    assert!(!e.contains("Unknown command"), "arm chưa được nối: {e}");
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
    assert!(
        e.contains("skills:list"),
        "lỗi phải chỉ cách xem danh sách: {e}"
    );
}

/// Thư mục skill không tồn tại phải báo lỗi đọc được, không panic.
#[tokio::test]
async fn thu_muc_khong_ton_tai_thi_bao_loi_doc_duoc() {
    let _kho = KhoTam::moi("khong-ton-tai-dau-g2-xxx");
    let state = state_test();
    let e = handle_command(Arc::clone(&state), "skills:sync", json!({}), None, None)
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
