//! Hàng rào vault phải **fail-closed** trước symlink/junction.
//!
//! ## Vì sao có file này
//!
//! `NativeMcpServer::resolve_path` là hàng rào ghim-dưới-vault DUY NHẤT của lõi,
//! và nó đứng giữa allow-list Telegram với toàn bộ ổ đĩa: `/cat` và `/ls` gọi
//! thẳng vào nó (`telegram.rs:247`, `:291`), tức đây là bề mặt **qua Internet**.
//!
//! Hai lớp kiểm cũ đều thuần **cú pháp**: chặn tuyệt đối/`..`, rồi `starts_with`
//! sau `join`. Cả hai đọc *chuỗi*, không đọc *đĩa*. Một junction (Windows) hoặc
//! symlink (Unix) nằm TRONG vault trỏ ra ngoài đi lọt cả hai: chuỗi
//! `thoat/secret.txt` không có `..`, không tuyệt đối, và nằm dưới vault — chỉ có
//! filesystem mới biết nó dẫn đi đâu.
//!
//! Trên Windows điều này đặc biệt đáng lo: `mklink /J` tạo junction **không cần
//! quyền admin**, khác với symlink.
//!
//! ## Vì sao là file test riêng
//!
//! Phép kiểm này cần thư mục tạm thật, một tiến trình con (`mklink`), và dọn dẹp
//! có điều kiện — không thuộc về `mod sandbox_tests` trong `server.rs`, vốn là
//! các phép kiểm thuần cú pháp chạy trên đường dẫn không tồn tại.
//!
//! ## Kỷ luật của chính test này
//!
//! - **Không im lặng bỏ qua.** Tạo junction hỏng thì test ĐỎ kèm stderr thật,
//!   không phải `return` sớm. Một phép kiểm an ninh tự bỏ qua mình là tệ hơn
//!   không có: nó in màu xanh.
//! - **Dọn dẹp chỉ gỡ liên kết, KHÔNG bao giờ đụng đích bên ngoài.** Gỡ nhầm
//!   bằng `remove_dir_all` xuyên qua junction là đúng cái lỗi
//!   "Destructive Git Rollback" mà vault dự án đã ghi. Test tự khẳng định lại
//!   điều đó: sau khi dọn, tệp bí mật bên ngoài phải CÒN NGUYÊN.
//! - Mọi assert chạy SAU khi dọn, để một phép kiểm đỏ không để lại junction.

use liva_native_core::mcp::protocol::CallToolRequest;
use liva_native_core::mcp::server::NativeMcpServer;
use std::fs;
use std::path::{Path, PathBuf};

/// Thư mục tạm riêng cho một lần chạy. Không dùng crate ngoài (`tempfile`) —
/// yêu cầu là không thêm phụ thuộc mới.
fn thu_muc_rieng(nhan: &str) -> PathBuf {
    let nano = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!(
        "liva_mcp_sandbox_{nhan}_{}_{nano}",
        std::process::id()
    ));
    fs::create_dir_all(&p).expect("tao duoc thu muc tam");
    p
}

/// Tạo liên kết thư mục `lien_ket` → `dich`.
///
/// Windows: junction qua `cmd /c mklink /J`. Cố ý KHÔNG dùng
/// `std::os::windows::fs::symlink_dir` — nó đòi quyền tạo symlink mà máy dev và
/// CI windows-latest thường không có, nên sẽ biến phép kiểm an ninh này thành
/// một lần bỏ qua âm thầm. Junction thì ai cũng tạo được, và chính vì thế nó mới
/// là đường tấn công đáng lo.
#[cfg(windows)]
fn tao_lien_ket_thu_muc(lien_ket: &Path, dich: &Path) {
    let ra = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(lien_ket)
        .arg(dich)
        .output()
        .expect("chay duoc mklink");
    assert!(
        ra.status.success(),
        "mklink /J that bai — KHONG duoc bo qua phep kiem an ninh nay.\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&ra.stdout),
        String::from_utf8_lossy(&ra.stderr)
    );
}

#[cfg(unix)]
fn tao_lien_ket_thu_muc(lien_ket: &Path, dich: &Path) {
    std::os::unix::fs::symlink(dich, lien_ket).expect("tao duoc symlink thu muc");
}

/// Gỡ liên kết mà **không** đi xuyên qua nó.
///
/// `remove_dir` gỡ đúng một mục: với junction/symlink thư mục nó xoá bản thân
/// liên kết, không đụng nội dung đích.
fn go_lien_ket(lien_ket: &Path) {
    let _ = fs::remove_dir(lien_ket);
}

#[test]
fn junction_trong_vault_khong_duoc_dan_ra_ngoai() {
    let goc = thu_muc_rieng("escape");
    let vault = goc.join("vault");
    let ngoai = goc.join("ngoai");
    fs::create_dir_all(vault.join("ghi-chu")).expect("tao vault");
    fs::create_dir_all(&ngoai).expect("tao thu muc ngoai");

    let bi_mat = ngoai.join("bi-mat.txt");
    fs::write(&bi_mat, "LIVA_ENCRYPTION_KEY=that-su-bi-mat").expect("ghi tep bi mat");
    fs::write(vault.join("ghi-chu/co-that.md"), "# ghi chu").expect("ghi tep trong vault");

    let lien_ket = vault.join("thoat");
    tao_lien_ket_thu_muc(&lien_ket, &ngoai);

    // Junction phải thật sự dẫn ra ngoài, nếu không cả phép kiểm là vô nghĩa.
    let doc_duoc_qua_lien_ket = fs::read_to_string(lien_ket.join("bi-mat.txt")).is_ok();

    let s = NativeMcpServer::new(vault.to_str().expect("duong dan vault la UTF-8"));

    // ── Các phép đo, thu trước và assert sau khi dọn ───────────────────────
    let khai_thac_tep = s.resolve_path("thoat/bi-mat.txt");
    let khai_thac_thu_muc = s.resolve_path("thoat");
    let doc_binh_thuong = s.resolve_path("ghi-chu/co-that.md");
    // Ghi vào đường dẫn CHƯA tồn tại vẫn phải được phép — đây là ca dùng thật
    // của `write_markdown`, và một bản vá fail-closed vụng về sẽ giết nó.
    let ghi_moi = s.resolve_path("ghi-chu/thu-muc-moi/chua-ton-tai.md");
    let ghi_moi_sau_lien_ket = s.resolve_path("thoat/tep-moi.md");
    let goc_vault = s.resolve_path("");

    // ── Dọn: gỡ liên kết TRƯỚC, rồi mới xoá cây ────────────────────────────
    go_lien_ket(&lien_ket);
    let bi_mat_con_nguyen = bi_mat.is_file();
    let _ = fs::remove_dir_all(&goc);

    // ── Assert ─────────────────────────────────────────────────────────────
    assert!(
        bi_mat_con_nguyen,
        "don dep da di XUYEN QUA lien ket va xoa dich ben ngoai — sai nghiem trong"
    );
    assert!(
        doc_duoc_qua_lien_ket,
        "junction/symlink khong dan ra ngoai that => phep kiem nay khong chung minh gi"
    );

    assert!(
        khai_thac_tep.is_err(),
        "THOAT SANDBOX: doc duoc tep ngoai vault qua junction, tra ve {khai_thac_tep:?}"
    );
    assert!(
        khai_thac_thu_muc.is_err(),
        "THOAT SANDBOX: chinh thu muc junction phai bi tu choi, tra ve {khai_thac_thu_muc:?}"
    );
    assert!(
        ghi_moi_sau_lien_ket.is_err(),
        "THOAT SANDBOX: ghi duoc ra ngoai vault qua junction, tra ve {ghi_moi_sau_lien_ket:?}"
    );

    assert!(
        doc_binh_thuong.is_ok(),
        "tep that trong vault van phai doc duoc: {doc_binh_thuong:?}"
    );
    assert!(
        ghi_moi.is_ok(),
        "duong dan con CHUA ton tai trong vault van phai ghi duoc: {ghi_moi:?}"
    );
    assert!(
        goc_vault.is_ok(),
        "chuoi rong = goc vault (cho /ls mac dinh): {goc_vault:?}"
    );
}

/// Cửa thứ hai của cùng lỗ hổng: `search_vault` tự duyệt cây, KHÔNG đi qua
/// `resolve_path`.
///
/// Bộ duyệt cũ dùng `path.is_dir()` — đi xuyên junction — nên nó bò ra ngoài
/// vault, đọc `.md`/`.txt` ở đó và trả **tên file khớp** về. Nội dung không ra
/// theo, nhưng tên khớp là đủ để làm máy tiên tri: hỏi đủ nhiều truy vấn thì suy
/// ra được nội dung tệp ngoài vault.
#[tokio::test]
async fn search_vault_khong_bo_ra_ngoai_qua_junction() {
    let goc = thu_muc_rieng("search");
    let vault = goc.join("vault");
    let ngoai = goc.join("ngoai");
    fs::create_dir_all(&vault).expect("tao vault");
    fs::create_dir_all(&ngoai).expect("tao thu muc ngoai");

    // Chuỗi chỉ tồn tại ở tệp NGOÀI vault.
    let tu_khoa = "CHUOI_CHI_CO_O_NGOAI_VAULT_9f3a";
    fs::write(ngoai.join("bi-mat.md"), format!("khoa: {tu_khoa}")).expect("ghi tep ngoai");
    fs::write(vault.join("trong-vault.md"), "khong co gi dac biet").expect("ghi tep trong vault");

    let lien_ket = vault.join("thoat");
    tao_lien_ket_thu_muc(&lien_ket, &ngoai);
    let doc_duoc_qua_lien_ket = fs::read_to_string(lien_ket.join("bi-mat.md")).is_ok();

    let s = NativeMcpServer::new(vault.to_str().expect("duong dan vault la UTF-8"));
    let ket_qua = s
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": tu_khoa }),
        })
        .await;

    go_lien_ket(&lien_ket);
    let bi_mat_con_nguyen = ngoai.join("bi-mat.md").is_file();
    let _ = fs::remove_dir_all(&goc);

    assert!(
        bi_mat_con_nguyen,
        "don dep da di XUYEN QUA lien ket va xoa dich ben ngoai — sai nghiem trong"
    );
    assert!(
        doc_duoc_qua_lien_ket,
        "junction khong dan ra ngoai that => phep kiem nay khong chung minh gi"
    );

    let ket_qua = ket_qua.expect("search_vault phai chay duoc");
    let van_ban = format!("{:?}", ket_qua.content);
    assert!(
        !van_ban.contains("bi-mat"),
        "RO RI: search_vault liet ke tep NGOAI vault qua junction — {van_ban}"
    );
}

/// Vault chưa tồn tại thì không có gì để mà thoát — hàng rào phải giữ nguyên
/// hành vi cũ (thuần cú pháp), không được từ chối tất cả.
///
/// Đây là hợp đồng mà `mod sandbox_tests` trong `server.rs` đang dựa vào
/// (`NativeMcpServer::new("vault_test_goc")`), và cũng là ca thật khi người dùng
/// chưa tạo thư mục vault lần nào.
#[test]
fn vault_chua_ton_tai_van_giai_duoc_duong_dan_hop_le() {
    let s = NativeMcpServer::new("vault_chua_he_ton_tai_bao_gio");
    assert!(s.resolve_path("ghi-chu.md").is_ok());
    assert!(s.resolve_path("a/b/c.md").is_ok());
    assert!(s.resolve_path("../.env").is_err(), "van phai chan `..`");
}
