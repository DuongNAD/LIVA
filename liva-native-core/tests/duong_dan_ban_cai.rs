//! Bất biến của **bản CÀI**: chạy ngoài cây mã nguồn vẫn phải tìm được model,
//! đọc được config, và ghi config vào chỗ ghi được.
//!
//! # Vì sao cần một file test riêng
//!
//! Mọi test khác trong repo chạy với cwd nằm **trong** cây mã nguồn, nên bộ dò
//! `""` / `".."` / `"../.."` luôn tìm thấy thứ nó cần và mọi thứ xanh. Bản cài
//! thì cwd là thư mục cài (`%LOCALAPPDATA%\LIVA`) hoặc bất kỳ đâu shortcut trỏ
//! tới — không có `models/`, không có `data/liva-config.json` ở bất kỳ cấp nào.
//! Đó là lý do một installer dựng được vẫn cho ra ứng dụng không dùng được:
//! **không có phép kiểm nào từng chạy ở cwd đó.**
//!
//! Các test dưới đây dựng đúng tình huống ấy bằng cwd giả, nên chúng đỏ với mã
//! cũ và xanh với mã mới — chứ không phải xanh sẵn từ đầu.
//!
//! Ba thứ ở đây là trạng thái TOÀN CỤC của tiến trình (cwd, `LIVA_HOME`,
//! `LOCALAPPDATA`), nên mọi test phải giữ cùng một khoá. Cùng lý do với
//! `KHOA_CWD` trong `lib.rs`.

use std::path::{Path, PathBuf};

static KHOA: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn nam_khoa() -> std::sync::MutexGuard<'static, ()> {
    KHOA.lock().unwrap_or_else(|e| e.into_inner())
}

fn thu_muc_tam(ten: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "liva_bancai_{ten}_{}_{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("tạo thư mục tạm");
    d
}

/// Gốc repo, dò lên từ cwd của `cargo test` (`liva-native-core/`).
fn goc_repo() -> Option<PathBuf> {
    let mut d = std::env::current_dir().ok()?;
    loop {
        if d.join("liva-native-core").is_dir() && d.join("liva-desktop").is_dir() {
            return Some(d);
        }
        d = d.parent()?.to_path_buf();
    }
}

/// Chạy `f` với cwd = `cwd_gia` và `LIVA_HOME` = `home`, rồi trả lại nguyên trạng.
///
/// Trả lại cwd **trước** khi assert là cố ý: một assert đỏ sẽ panic, và nếu cwd
/// chưa được trả thì mọi test sau trong cùng tiến trình đỏ theo vì lý do khác
/// hẳn — che mất lỗi thật.
fn trong_moi_truong_ban_cai<T>(cwd_gia: &Path, home: &Path, f: impl FnOnce() -> T) -> T {
    let cwd_cu = std::env::current_dir().expect("cwd");
    let home_cu = std::env::var("LIVA_HOME").ok();

    std::env::set_current_dir(cwd_gia).expect("đổi cwd");
    unsafe { std::env::set_var("LIVA_HOME", home) };

    let ket_qua = f();

    std::env::set_current_dir(&cwd_cu).expect("trả cwd");
    match home_cu {
        Some(v) => unsafe { std::env::set_var("LIVA_HOME", v) },
        None => unsafe { std::env::remove_var("LIVA_HOME") },
    }
    ket_qua
}

/// Chạy `f` với `LOCALAPPDATA` giả và KHÔNG có `LIVA_HOME`.
///
/// Dùng cho nhóm test phân biệt "cài mới" với "nâng cấp": cả hai chỉ khác nhau ở
/// những gì đã nằm sẵn dưới `%LOCALAPPDATA%`.
fn voi_localappdata<T>(local: &Path, f: impl FnOnce() -> T) -> T {
    let cu_local = std::env::var("LOCALAPPDATA").ok();
    let cu_home = std::env::var("LIVA_HOME").ok();
    unsafe { std::env::set_var("LOCALAPPDATA", local) };
    unsafe { std::env::remove_var("LIVA_HOME") };

    let kq = f();

    match cu_local {
        Some(v) => unsafe { std::env::set_var("LOCALAPPDATA", v) },
        None => unsafe { std::env::remove_var("LOCALAPPDATA") },
    }
    if let Some(v) = cu_home {
        unsafe { std::env::set_var("LIVA_HOME", v) }
    }
    kq
}

/// **CÀI MỚI** — dữ liệu người dùng KHÔNG được nằm trong thư mục cài.
///
/// NSIS cài vào `$LOCALAPPDATA\${PRODUCTNAME}` = `%LOCALAPPDATA%\LIVA`
/// (`target/release/nsis/x64/installer.nsi:503`). Nếu thư mục dữ liệu cũng là
/// `%LOCALAPPDATA%\LIVA` thì 2,3 GB model nằm lẫn trong thư mục cài, và câu
/// "dữ liệu sống sót qua gỡ cài đặt" trong tài liệu là sai.
///
/// Bẫy tinh vi: bộ cài ĐẶT SẴN `data\models-manifest.json` ở đó. Một phép kiểm
/// ngây thơ kiểu "thư mục data có tồn tại không" sẽ thấy nó và kết luận nhầm là
/// máy này đang nâng cấp từ bản cũ — nên test dựng đúng cái file đó.
#[test]
fn cai_moi_khong_dung_thu_muc_cai_lam_thu_muc_du_lieu() {
    let _g = nam_khoa();
    let local = thu_muc_tam("localappdata");
    // Đúng những gì bộ cài để lại, không hơn.
    std::fs::create_dir_all(local.join("LIVA").join("data")).unwrap();
    std::fs::write(local.join("LIVA/data/models-manifest.json"), b"{}").unwrap();

    let home = voi_localappdata(&local, liva_native_core::user_home_dir).expect("phải có home");

    assert_eq!(
        home,
        local.join("com.liva.cognitive-os"),
        "cài mới phải dùng thư mục dữ liệu riêng theo bundle id, không phải thư mục cài"
    );
    assert_ne!(
        home,
        local.join("LIVA"),
        "models-manifest.json do BỘ CÀI đặt không phải dấu vết dữ liệu người dùng"
    );
    let _ = std::fs::remove_dir_all(&local);
}

/// **NÂNG CẤP** — máy đã có dữ liệu ở chỗ cũ thì phải tiếp tục dùng chỗ cũ.
///
/// Đổi neo mà bỏ qua vế này là làm người đang dùng mất sạch: ký ức, cấu hình và
/// vài GB model vẫn nằm nguyên ở `%LOCALAPPDATA%\LIVA`, chỉ là không ai đọc nữa.
#[test]
fn nang_cap_van_dung_thu_muc_du_lieu_cu() {
    let _g = nam_khoa();
    for dau_vet in [
        "LIVA/data/liva-config.json",
        "LIVA/data/agents/liva_core/structured_memory.sqlite",
    ] {
        let local = thu_muc_tam("localappdata_cu");
        let p = local.join(dau_vet);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"x").unwrap();

        let home = voi_localappdata(&local, liva_native_core::user_home_dir).expect("phải có home");

        assert_eq!(
            home,
            local.join("LIVA"),
            "có {dau_vet} là dữ liệu thật của người dùng — phải giữ neo cũ"
        );
        let _ = std::fs::remove_dir_all(&local);
    }
}

/// Model đã tải cũng là dữ liệu thật: bỏ qua nó là bắt người dùng tải lại 2,3 GB.
#[test]
fn nang_cap_nhan_ra_model_da_tai_o_cho_cu() {
    let _g = nam_khoa();
    let local = thu_muc_tam("localappdata_model");
    std::fs::create_dir_all(local.join("LIVA/models/nemotron-asr")).unwrap();
    std::fs::write(local.join("LIVA/models/nemotron-asr/encoder.onnx"), b"x").unwrap();

    let home = voi_localappdata(&local, liva_native_core::user_home_dir).expect("phải có home");

    assert_eq!(
        home,
        local.join("LIVA"),
        "model đã tải phải giữ được neo cũ"
    );
    let _ = std::fs::remove_dir_all(&local);
}

/// Đã di trú sang chỗ mới rồi thì chỗ cũ còn sót cũng không được kéo ngược lại.
#[test]
fn co_ca_hai_thi_uu_tien_cho_moi() {
    let _g = nam_khoa();
    let local = thu_muc_tam("localappdata_ca_hai");
    for p in [
        "LIVA/data/liva-config.json",
        "com.liva.cognitive-os/data/liva-config.json",
    ] {
        let f = local.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"x").unwrap();
    }

    let home = voi_localappdata(&local, liva_native_core::user_home_dir).expect("phải có home");

    assert_eq!(home, local.join("com.liva.cognitive-os"));
    let _ = std::fs::remove_dir_all(&local);
}

/// `LIVA_HOME` thắng cả hai — đường thoát đã tài liệu hoá cho người muốn để
/// model sang ổ khác.
#[test]
fn liva_home_thang_moi_suy_luan_khac() {
    let _g = nam_khoa();
    let local = thu_muc_tam("localappdata_override");
    std::fs::create_dir_all(local.join("LIVA/data")).unwrap();
    std::fs::write(local.join("LIVA/data/liva-config.json"), b"x").unwrap();
    let rieng = thu_muc_tam("o_khac");

    let cu_local = std::env::var("LOCALAPPDATA").ok();
    let cu_home = std::env::var("LIVA_HOME").ok();
    unsafe { std::env::set_var("LOCALAPPDATA", &local) };
    unsafe { std::env::set_var("LIVA_HOME", &rieng) };
    let home = liva_native_core::user_home_dir();
    match cu_local {
        Some(v) => unsafe { std::env::set_var("LOCALAPPDATA", v) },
        None => unsafe { std::env::remove_var("LOCALAPPDATA") },
    }
    match cu_home {
        Some(v) => unsafe { std::env::set_var("LIVA_HOME", v) },
        None => unsafe { std::env::remove_var("LIVA_HOME") },
    }

    assert_eq!(home, Some(rieng.clone()));
    let _ = std::fs::remove_dir_all(&local);
    let _ = std::fs::remove_dir_all(&rieng);
}

/// **C1** — model tải về thư mục người dùng phải tìm được từ bản cài.
///
/// Mã cũ chỉ dò `""`/`".."`/`"../.."` theo cwd ⇒ trả lại đúng chuỗi đầu vào,
/// một đường dẫn không tồn tại, và STT/TTS/VAD/wake im lặng hỏng.
#[test]
fn ban_cai_tim_duoc_model_trong_thu_muc_nguoi_dung() {
    let _g = nam_khoa();
    let cwd_gia = thu_muc_tam("cwd");
    let home = thu_muc_tam("home");
    std::fs::create_dir_all(home.join("models").join("nemotron-asr")).unwrap();
    std::fs::write(home.join("models/nemotron-asr/encoder.onnx"), b"x").unwrap();

    let thay = trong_moi_truong_ban_cai(&cwd_gia, &home, || {
        liva_native_core::resolve_resource_path("models/nemotron-asr/encoder.onnx")
    });

    assert!(
        thay.exists(),
        "bản cài không tìm ra model đã tải về {}: nhận {:?}",
        home.display(),
        thay
    );
    let _ = std::fs::remove_dir_all(&cwd_gia);
    let _ = std::fs::remove_dir_all(&home);
}

/// **C1b** — tài nguyên đóng gói kèm installer nằm cạnh exe / trong `resources/`.
///
/// Thuần (nhận `exe_dir` làm tham số) để test được mà không phải giả lập vị trí
/// executable — cùng cách `db::vec0_candidate_paths` đã làm cho `vec0.dll`.
#[test]
fn ung_vien_phu_ca_canh_exe_va_resources() {
    let exe = PathBuf::from(r"C:\Users\ai\AppData\Local\LIVA");
    let home = PathBuf::from(r"C:\Users\ai\AppData\Local\LIVA");
    let ds = liva_native_core::resource_candidate_paths(
        "data/models-manifest.json",
        Some(&home),
        Some(&exe),
    );

    let co = |p: PathBuf| ds.contains(&p);
    assert!(
        co(exe.join("data/models-manifest.json")),
        "thiếu ứng viên cạnh exe: {ds:?}"
    );
    assert!(
        co(exe.join("resources").join("data/models-manifest.json")),
        "thiếu ứng viên trong resources/ — đây là nơi bundle.resources của Tauri đặt file: {ds:?}"
    );
    assert_eq!(
        ds[0],
        PathBuf::from("data/models-manifest.json"),
        "cwd phải được thử TRƯỚC — nếu không, bản dev sẽ đọc tài nguyên của bản cài"
    );
}

/// **C2** — bản cài phải ghi được config, và ghi ra ngoài thư mục cài.
///
/// Mã cũ trả `data/liva-config.json` **tương đối** ⇒ ghi vào thư mục cài (mất
/// khi gỡ/nâng cấp), hoặc vào cwd bất kỳ mà shortcut trỏ tới.
#[test]
fn ban_cai_ghi_duoc_config_ngoai_thu_muc_cai() {
    let _g = nam_khoa();
    let cwd_gia = thu_muc_tam("cwd");
    let home = thu_muc_tam("home");

    let duong = trong_moi_truong_ban_cai(&cwd_gia, &home, liva_native_core::config_file_path);

    assert!(
        duong.is_absolute(),
        "config phải là đường dẫn tuyệt đối, không phụ thuộc cwd — nhận {duong:?}"
    );
    assert!(
        duong.starts_with(&home),
        "config phải nằm dưới thư mục dữ liệu người dùng {} — nhận {duong:?}",
        home.display()
    );
    // Ghi thật: đây mới là điều kiện người dùng cần (lưu API key, đổi model).
    std::fs::create_dir_all(duong.parent().unwrap()).expect("tạo thư mục config");
    std::fs::write(&duong, b"{}").expect("phải ghi được config");

    let _ = std::fs::remove_dir_all(&cwd_gia);
    let _ = std::fs::remove_dir_all(&home);
}

/// **C2b** — config và database phải dùng CHUNG một neo.
///
/// Tách hai neo là cách sinh ra "sổ danh bạ trống" của commit 46afef4, chỉ đổi
/// đối tượng từ database sang config.
#[test]
fn config_va_du_lieu_dung_chung_mot_neo() {
    let _g = nam_khoa();
    let cwd_gia = thu_muc_tam("cwd");
    let home = thu_muc_tam("home");

    let (cfg, data) = trong_moi_truong_ban_cai(&cwd_gia, &home, || {
        (
            liva_native_core::config_file_path(),
            liva_native_core::data_dir(),
        )
    });

    assert_eq!(
        cfg.parent().map(Path::to_path_buf),
        Some(data),
        "config phải nằm ngay trong thư mục dữ liệu, không phải một cây khác"
    );
    let _ = std::fs::remove_dir_all(&cwd_gia);
    let _ = std::fs::remove_dir_all(&home);
}

/// **C3** — mặc định vault không được là đường dẫn tuyệt đối của máy dev.
///
/// `boot.rs` cũ ghi cứng `E:\Project\LIVA\teamwork_projects\...`, một ổ đĩa
/// không tồn tại trên máy người dùng.
#[test]
fn vault_mac_dinh_khong_phai_o_dia_may_dev() {
    let _g = nam_khoa();
    let cwd_gia = thu_muc_tam("cwd");
    let home = thu_muc_tam("home");

    let vault = trong_moi_truong_ban_cai(&cwd_gia, &home, liva_native_core::default_vault_path);

    let s = vault.to_string_lossy().to_lowercase();
    assert!(
        !s.contains("e:\\project\\liva") && !s.contains("e:/project/liva"),
        "mặc định vault còn trỏ vào máy dev: {vault:?}"
    );
    assert!(
        vault.starts_with(&home),
        "ngoài cây mã nguồn, vault phải nằm dưới thư mục người dùng — nhận {vault:?}"
    );
    let _ = std::fs::remove_dir_all(&cwd_gia);
    let _ = std::fs::remove_dir_all(&home);
}

/// **Chống hồi quy cho DEV** — chạy trong repo phải KHÔNG đổi hành vi.
///
/// Đây là nửa còn lại của yêu cầu: sửa cho bản cài mà làm lệch bản dev thì mọi
/// phiên làm việc sau đều trả giá.
#[test]
fn trong_repo_van_dung_config_cua_repo() {
    let _g = nam_khoa();
    let Some(goc) = goc_repo() else {
        return; // không nhận ra bố cục repo
    };
    if !goc.join("data/liva-config.json").exists() {
        return; // checkout không có config — không kết luận được
    }

    let cwd_cu = std::env::current_dir().expect("cwd");
    let mut ket_qua = Vec::new();
    for noi in ["", "liva-native-core", "liva-desktop/src-tauri"] {
        let dich = goc.join(noi);
        if !dich.is_dir() {
            continue;
        }
        std::env::set_current_dir(&dich).expect("đổi cwd");
        ket_qua.push((
            noi,
            liva_native_core::config_file_path().canonicalize().ok(),
        ));
    }
    std::env::set_current_dir(&cwd_cu).expect("trả cwd");

    let mong_doi = goc.join("data/liva-config.json").canonicalize().ok();
    assert!(mong_doi.is_some(), "config repo phải canonicalize được");
    for (noi, thay) in ket_qua {
        assert_eq!(
            thay, mong_doi,
            "chạy từ {noi:?} trong repo phải vẫn đọc data/liva-config.json của repo"
        );
    }
}
