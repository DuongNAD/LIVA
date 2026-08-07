//! Preflight runtime: cái gì sẽ CHẠY ĐƯỢC trên máy này, và thiếu gì thì mất gì.
//!
//! # Vì sao có module này khi đã có `npm run doctor`
//!
//! Hai bộ kiểm trả lời hai câu hỏi khác nhau, và không thay được nhau:
//!
//! - `scripts/models.mjs doctor` soi **file model trên đĩa** — 11 năng lực, kèm
//!   kích thước tham chiếu và lệnh `fetch` để tải. Đó là câu hỏi "tôi đã tải đủ
//!   chưa".
//! - Module này soi **môi trường chạy** — thứ `doctor` không thể biết vì nó là
//!   script Node: binary này build ở profile nào, có CUDA không, GPU có thật
//!   không, `espeak-ng`/`ffmpeg` có trên PATH không, `vec0` có nạp được không,
//!   khoá mã hoá có phải khoá mặc định công khai không.
//!
//! Đúng những chế độ hỏng đã tốn thời gian thật trong đợt U1–U1c: vision trả lỗi
//! ở build debug (CRT-mix), vision mất **~80 s mỗi lượt thay vì ~1,4 s** khi
//! GPU không vào cuộc (cả hai đều là số đo, không phải suy ra — xem
//! `docs/03-danh-gia/05-nang-cap-toan-dien.md` mục U1a/U1b), và bản CUDA
//! **không khởi động nổi** khi thiếu `cudart64_12.dll` — exit 127, không một
//! dòng thông báo. Không có cái nào là "thiếu file model".
//!
//! # Nguyên tắc
//!
//! **Luôn `exit 0`.** Đây là bộ báo cáo, không phải cổng kiểm. Thiếu gì thì nói
//! thiếu gì; quyết định chặn hay không là việc của người gọi. Ngược lại với
//! `doctor` (thoát 1 khi thiếu file bắt buộc) — cố ý khác nhau.

use std::path::{Path, PathBuf};

/// Một dòng trong báo cáo.
struct Muc {
    /// Năng lực người dùng nhìn thấy, không phải tên module.
    ten: &'static str,
    /// `true` = dùng được, `false` = mất năng lực, `None` = không kết luận được.
    ok: Option<bool>,
    /// Trạng thái đo được, một dòng.
    trang_thai: String,
    /// Hệ quả khi thiếu / ghi chú hành động. Rỗng thì bỏ dòng này.
    he_qua: String,
}

impl Muc {
    fn moi(ten: &'static str, ok: Option<bool>, trang_thai: impl Into<String>) -> Self {
        Self {
            ten,
            ok,
            trang_thai: trang_thai.into(),
            he_qua: String::new(),
        }
    }
    fn vi(mut self, he_qua: impl Into<String>) -> Self {
        self.he_qua = he_qua.into();
        self
    }
}

/// Tìm một chương trình trên `PATH` (thêm `.exe` trên Windows).
fn tren_path(ten: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &["exe", "cmd", "bat"]
    } else {
        &[""]
    };
    let path = std::env::var_os("PATH")?;
    for thu_muc in std::env::split_paths(&path) {
        for ext in exts {
            let ung_vien = if ext.is_empty() {
                thu_muc.join(ten)
            } else {
                thu_muc.join(format!("{ten}.{ext}"))
            };
            if ung_vien.is_file() {
                return Some(ung_vien);
            }
        }
    }
    None
}

/// Rút gọn đường dẫn cho dễ đọc: bỏ tiền tố thư mục repo nếu có.
fn gon(p: &Path) -> String {
    let s = p.display().to_string();
    match std::env::current_dir() {
        Ok(cwd) => s
            .strip_prefix(&format!("{}\\", cwd.display()))
            .or_else(|| s.strip_prefix(&format!("{}/", cwd.display())))
            .unwrap_or(&s)
            .to_string(),
        Err(_) => s,
    }
}

/// Đường thị giác: **bốn** điều kiện độc lập, và chỉ đủ cả bốn mới dùng được
/// thật. Đây là năng lực duy nhất mà "có chạy" và "dùng được" là hai chuyện
/// khác nhau — 80 s mỗi lượt về mặt kỹ thuật là chạy, về mặt hội thoại là không.
///
/// Điều kiện thứ tư (`n_gpu_layers`) là chỗ suýt thành một "xanh giả" nữa: có
/// đủ release + CUDA + GPU mà `LIVA_LLM_N_GPU_LAYERS` để mặc định thì vẫn ~80 s,
/// vì mặc định là **0** (`boot.rs:177-180`) và `MtmdContextParams.use_gpu` được
/// đặt bằng `n_gpu_layers > 0` — tức bộ mã hoá ảnh nằm hẳn trên CPU, GPU đứng
/// không. Đúng kiểu suy giảm im lặng mà U3 sinh ra để bắt, nên nó phải là một
/// **phép kiểm**, không phải một câu nhắc trong lời khuyên.
///
/// Nhận `la_debug`/`co_cuda` làm tham số chứ không đọc `cfg!` trực tiếp: mỗi
/// binary chỉ tồn tại ở MỘT tổ hợp cfg, nên nếu đọc `cfg!` trong thân hàm thì
/// hầu hết các nhánh không thể chạm tới bằng test — mà chính mấy nhánh đó là
/// nơi chứa lời khuyên khắc phục dễ viết sai nhất.
fn muc_vision(la_debug: bool, co_cuda: bool, co_gpu: bool, n_gpu_layers: u32) -> Muc {
    const TEN: &str = "Nhìn màn hình (vision:ask)";

    if la_debug {
        return Muc::moi(TEN, Some(false), "build DEBUG").vi(
            "Trả lỗi ngay, không chạy: CMake dùng CRT debug còn Rust dùng CRT release \
             ⇒ lệch bảng file-descriptor, bộ nạp mmproj abort. Cần `cargo build --release`.",
        );
    }
    if !co_cuda {
        return Muc::moi(TEN, Some(false), "release nhưng build KHÔNG có CUDA")
            .vi("~80 s mỗi lượt trên CPU. Build lại với `--features cuda` \
             (ghim `CUDAARCHS` cho GPU của bạn để binary không phình 202 MB).");
    }
    if !co_gpu {
        return Muc::moi(TEN, Some(false), "có CUDA nhưng KHÔNG thấy GPU").vi(
            "Rơi về CPU: ~80 s mỗi lượt — chạy được nhưng ngoài ngưỡng hội thoại. \
             Kiểm driver NVIDIA; log llama.cpp phải có `ggml_cuda_init: found 1 CUDA devices`.",
        );
    }
    if n_gpu_layers == 0 {
        return Muc::moi(
            TEN,
            Some(false),
            "đủ release + CUDA + GPU, nhưng n_gpu_layers sẽ là 0",
        )
        .vi(
            // Ba lý do khác nhau cùng ra 0, và nói nhầm lý do là đẩy người dùng
            // đi săn một vấn đề họ không có. Ca "chưa có model" phải tách riêng:
            // `gpu_layers_theo_vram` trả 0 ngay khi `can == 0`, tức khi không đo
            // được kích thước model — trên máy mới cài thì đó là lý do DUY NHẤT,
            // và VRAM hoàn toàn không liên quan.
            if liva_native_core::configured_router_model_path().is_none_or(|p| !p.is_file()) {
                "CHƯA CÓ MODEL nên chưa tính được — đây là hệ quả của dòng \
                 `Model chat` bên dưới, không phải vấn đề GPU. Phép tự chọn cần \
                 kích thước model + projector để biết nhét được bao nhiêu lớp; \
                 không có file thì nó trả 0. Tải model xong dòng này tự xanh; \
                 đừng đi chỉnh VRAM."
            } else {
                "Vẫn ~80 s mỗi lượt: `use_gpu` của bộ nạp ảnh bằng \
                 `n_gpu_layers > 0` ⇒ GPU đứng không. Dòng này KHÔNG còn nghĩa là \
                 'bạn quên đặt biến môi trường' — từ `533f3c6` lõi tự chọn theo \
                 VRAM trống. Model CÓ trên đĩa mà vẫn ra 0 ⇒ VRAM trống không đủ \
                 cho model + projector + dự phòng, hoặc không đọc được VRAM. Xem \
                 dòng log `GPU:` lúc khởi động để biết vế nào. Ép bằng \
                 `$env:LIVA_LLM_N_GPU_LAYERS = \"999\"` nếu bạn biết máy kham được."
            },
        );
    }
    Muc::moi(
        TEN,
        Some(true),
        format!("release + CUDA + GPU, n_gpu_layers = {n_gpu_layers}"),
    )
    // Số đo, không phải ước lượng: `e2e-vision-ipc --release`, RTX 5060 Ti,
    // gemma-4-E4B, bản CUDA 9 kiến trúc mặc định — p50 937 ms trên 3 lượt
    // (min 844 · max 2031, lượt đầu đắt hơn vì dựng MtmdContext).
    //
    // Số cũ ở đây là ~1,4 s, đo trên bản ghim `CUDAARCHS=120a-real`. Giữ nguyên
    // nó sau khi bản phát hành đổi sang 9 kiến trúc là để một con số cũ nói về
    // một cấu hình không còn được phát hành — nên nó phải đi cùng cấu hình đã đo.
    .vi("Đo được p50 ~0,9 s mỗi lượt ở cấu hình này (RTX 5060 Ti, mẫu 3 lượt).")
}

/// Dựng toàn bộ báo cáo. Thuần đọc — không mở DB, không nạp model.
fn thu_thap() -> Vec<Muc> {
    let mut muc = Vec::new();

    // ── Profile build & tăng tốc ─────────────────────────────────────────
    let mut profile = Muc::moi(
        "Profile build",
        Some(!cfg!(debug_assertions)),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    );
    if cfg!(debug_assertions) {
        profile =
            profile.vi("Debug đủ cho mọi thứ TRỪ vision. Đường thoại/LLM/bộ nhớ chạy bình thường.");
    }
    muc.push(profile);

    let vram = liva_native_core::governor::gpu_vram_bytes();
    let co_gpu = vram.is_some();
    muc.push(match vram {
        // `gpu_vram_bytes` trả `(tổng, đang dùng)` — thứ tự này dễ đảo, và đảo thì
        // ra số vô nghĩa kiểu "16311 / 1843 MiB đang dùng" chứ không phải lỗi build.
        Some((tong, dung)) => Muc::moi(
            "GPU (NVML)",
            Some(true),
            format!(
                "thấy thiết bị — đang dùng {} / {} MiB VRAM",
                dung / 1_048_576,
                tong / 1_048_576
            ),
        ),
        None => Muc::moi("GPU (NVML)", None, "không đọc được").vi(
            "Hoặc không có GPU NVIDIA, hoặc NVML không nạp được. Chỉ ảnh hưởng \
             tốc độ vision và nhánh GPU của governor game-mode.",
        ),
    });

    // GỌI quyết định của `boot.rs`, đừng chép lại nó. Bản trước chép — đúng cho
    // tới `533f3c6`, commit thêm nhánh tự chọn theo VRAM, và từ đó preflight
    // doạ "~80 s mỗi lượt" trên đúng cái máy đang chạy vision 937 ms.
    //
    // Chuỗi ưu tiên giữ y hệt runtime: biến môi trường thắng, parse lỗi cũng
    // rơi về `gpu_layers_mac_dinh()` chứ không rơi về 0 — vì đó là thứ
    // `boot.rs` làm, và preflight không được lạc quan HAY bi quan hơn lõi.
    let n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or_else(liva_native_core::boot::gpu_layers_mac_dinh);
    muc.push(muc_vision(
        cfg!(debug_assertions),
        cfg!(feature = "cuda"),
        co_gpu,
        n_gpu_layers,
    ));

    // ── Nhị phân ngoài: hai thứ `doctor` không kiểm ─────────────────────
    let espeak = std::env::var("LIVA_ESPEAK_PATH")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| tren_path("espeak-ng"));
    muc.push(match espeak {
        Some(p) => Muc::moi("espeak-ng (G2P cho TTS)", Some(true), gon(&p)),
        None => Muc::moi("espeak-ng (G2P cho TTS)", Some(false), "không thấy").vi(
            "TTS Piper/VieNeu cần nó để chuyển chữ thành âm vị — thiếu là câu nói \
             bị sai ngữ điệu hoặc lỗi hẳn. Cài espeak-ng, hoặc trỏ `LIVA_ESPEAK_PATH`.",
        ),
    });

    muc.push(match tren_path("ffmpeg") {
        Some(p) => Muc::moi("ffmpeg (voice Telegram)", Some(true), gon(&p)),
        None => Muc::moi("ffmpeg (voice Telegram)", Some(false), "không thấy")
            .vi("Chỉ ảnh hưởng tin nhắn THOẠI qua Telegram; chat chữ vẫn chạy."),
    });

    // ── vec0: thứ duy nhất CHẶN BOOT ────────────────────────────────────
    let exe_dir = std::env::current_exe().ok();
    let ung_vien =
        liva_native_core::db::vec0_candidate_paths(exe_dir.as_deref().and_then(|p| p.parent()));
    let vec0_thay = ung_vien.iter().find(|c| Path::new(c).is_file());
    muc.push(match vec0_thay {
        Some(p) => Muc::moi("sqlite-vec (vec0)", Some(true), gon(Path::new(p))),
        None => Muc::moi(
            "sqlite-vec (vec0)",
            Some(false),
            format!("không thấy ({} chỗ đã tìm)", ung_vien.len()),
        )
        .vi(
            "Đây là thứ duy nhất trong bảng này CHẶN KHỞI ĐỘNG — không có nó thì \
             không mở được DB. Chạy `npm ci` (gói `sqlite-vec` cung cấp nó).",
        ),
    });

    // ── Khoá mã hoá ─────────────────────────────────────────────────────
    let khoa = std::env::var("LIVA_ENCRYPTION_KEY").ok();
    muc.push(match khoa.as_deref() {
        None => Muc::moi("Khoá mã hoá dữ liệu cá nhân", None, "không đặt qua env").vi(
            "Sẽ dùng khoá thiết bị (DPAPI) khi khởi động thật. Preflight không \
             mở keystore nên không kết luận thay được.",
        ),
        Some(k) if k == liva_native_core::crypto::DEFAULT_ENCRYPTION_KEY => Muc::moi(
            "Khoá mã hoá dữ liệu cá nhân",
            Some(false),
            "đang dùng KHOÁ MẶC ĐỊNH (công khai trong source)",
        )
        .vi(
            "Mã hoá facts, transcript, checkpoint và outbox gần như không bảo vệ gì — \
             ai đọc được file DB cũng giải mã được.",
        ),
        Some(_) => Muc::moi(
            "Khoá mã hoá dữ liệu cá nhân",
            Some(true),
            "khoá riêng từ env",
        ),
    });

    // ── Cấu hình: phải đứng TRƯỚC hai dòng model, vì nếu không tìm thấy thì
    //    hai dòng đó nói về giá trị mặc định trong code, không phải cái app dùng.
    //
    //    Đây là một cái bẫy thật, không phải giả thiết: `config_file_path()` dò
    //    cwd rồi hai cấp trên, hụt thì rơi về `DEFAULT_ROUTER_MODEL`. Chạy
    //    preflight từ sai thư mục sẽ thấy một dòng ✓ nói về hằng số trong mã chứ
    //    không phải về model mà app đang thật sự nạp — hai thứ đó trùng nhau
    //    HÔM NAY (cùng là `gemma-4-E4B` từ 02/08/2026) nên cái bẫy vô hình; ngày
    //    chúng lệch nhau lại là ngày dòng ✓ này nói dối mà không ai thấy.
    let cfg = liva_native_core::config_file_path();
    let co_cfg = cfg.exists();
    muc.push(if co_cfg {
        Muc::moi("Cấu hình (liva-config.json)", Some(true), gon(&cfg))
    } else {
        Muc::moi(
            "Cấu hình (liva-config.json)",
            Some(false),
            "không thấy — hai dòng model dưới là MẶC ĐỊNH TRONG CODE",
        )
        .vi(
            "Dò `data/liva-config.json` từ cwd và hai cấp trên đều hụt. Chạy \
             preflight từ thư mục gốc repo, nếu không thì hai dòng model dưới \
             không nói về thứ app thực sự nạp.",
        )
    });

    // ── Model từ config (chi tiết để `doctor` lo) ───────────────────────
    let router = liva_native_core::configured_router_model_path();
    muc.push(match router {
        Some(p) if p.is_file() => Muc::moi("Model chat (router GGUF)", Some(true), gon(&p)),
        Some(p) => Muc::moi("Model chat (router GGUF)", Some(false), gon(&p))
            .vi("Không có file ⇒ không có não: `chat:completion` và cả vision đều lỗi."),
        None => Muc::moi("Model chat (router GGUF)", Some(false), "chưa cấu hình")
            .vi("Đặt `ai.localModelsDir` + `ai.routerModel` trong `data/liva-config.json`."),
    });

    let mmproj = liva_native_core::configured_mmproj_path();
    muc.push(match mmproj {
        Some(p) if p.is_file() => Muc::moi("Bộ chiếu thị giác (mmproj)", Some(true), gon(&p)),
        Some(p) => Muc::moi("Bộ chiếu thị giác (mmproj)", Some(false), gon(&p))
            .vi("Thiếu ⇒ `vision:ask` lỗi ngay cả trên build release có GPU."),
        None => Muc::moi("Bộ chiếu thị giác (mmproj)", Some(false), "chưa cấu hình")
            .vi("Đặt `ai.mmprojModel` trong `data/liva-config.json` nếu muốn dùng vision."),
    });

    // ── Telegram ────────────────────────────────────────────────────────
    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    let allow = std::env::var("TELEGRAM_ALLOWED_IDS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .count();
    muc.push(match (token.is_some(), allow) {
        (false, _) => Muc::moi("Bot Telegram", None, "không đặt token — bot sẽ không chạy"),
        (true, 0) => Muc::moi(
            "Bot Telegram",
            Some(false),
            "có token nhưng allow-list RỖNG",
        )
        .vi(
            "Allow-list là fail-closed: rỗng nghĩa là bot TỪ CHỐI mọi người, kể cả bạn. \
             Đặt `TELEGRAM_ALLOWED_IDS`.",
        ),
        (true, n) => Muc::moi(
            "Bot Telegram",
            Some(true),
            format!("có token, {n} ID được phép"),
        )
        .vi("Chạy ở cả hai vỏ (gateway và app desktop)."),
    });

    muc
}

/// In báo cáo rồi trả về mã thoát — **luôn 0**, xem ghi chú đầu module.
pub fn chay() -> i32 {
    let muc = thu_thap();

    println!();
    println!("LIVA preflight — môi trường chạy trên máy này");
    println!();

    let rong = muc
        .iter()
        .map(|m| m.ten.chars().count())
        .max()
        .unwrap_or(24);
    let mut mat = 0usize;
    let mut khong_ro = 0usize;

    for m in &muc {
        let dau = match m.ok {
            Some(true) => "✓",
            Some(false) => "✗",
            None => "?",
        };
        if m.ok == Some(false) {
            mat += 1;
        }
        if m.ok.is_none() {
            khong_ro += 1;
        }
        println!("  {dau} {:<width$}  {}", m.ten, m.trang_thai, width = rong);
        if !m.he_qua.is_empty() {
            // Ngắt dòng thủ công để không phụ thuộc crate wrap nào.
            for dong in chia_dong(&m.he_qua, 78) {
                println!("      {dong}");
            }
        }
    }

    println!();
    if mat == 0 {
        println!("  Không thiếu gì trong phạm vi preflight.");
    } else {
        println!(
            "  {mat} hạng mục mất năng lực{}.",
            if khong_ro > 0 {
                format!(", {khong_ro} không kết luận được")
            } else {
                String::new()
            }
        );
    }
    println!("  Model trên đĩa (11 năng lực, kèm lệnh tải):  npm run doctor");
    println!();

    // Cố ý 0: báo cáo, không phải cổng kiểm. Xem ghi chú đầu module.
    0
}

/// Ngắt chuỗi theo khoảng trắng, không cắt giữa từ.
fn chia_dong(s: &str, rong: usize) -> Vec<String> {
    let mut ra = Vec::new();
    let mut dong = String::new();
    for tu in s.split_whitespace() {
        if !dong.is_empty() && dong.chars().count() + 1 + tu.chars().count() > rong {
            ra.push(std::mem::take(&mut dong));
        }
        if !dong.is_empty() {
            dong.push(' ');
        }
        dong.push_str(tu);
    }
    if !dong.is_empty() {
        ra.push(dong);
    }
    ra
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chia_dong_khong_cat_giua_tu() {
        let d = chia_dong("mot hai ba bon nam sau bay", 10);
        assert!(d.iter().all(|x| x.chars().count() <= 10), "{d:?}");
        assert_eq!(d.join(" "), "mot hai ba bon nam sau bay");
    }

    #[test]
    fn chia_dong_tu_dai_hon_be_rong_khong_mat_chu() {
        // Một từ dài hơn cả bề rộng vẫn phải xuất hiện nguyên vẹn, không bị cắt.
        let d = chia_dong("ngan aaaaaaaaaaaaaaaaaaaa ngan", 5);
        assert_eq!(d.join(" "), "ngan aaaaaaaaaaaaaaaaaaaa ngan");
    }

    #[test]
    fn preflight_luon_tra_ve_0() {
        // Hợp đồng của module: báo cáo, KHÔNG chặn. Test này khoá điều đó lại —
        // đổi sang `exit 1` khi thiếu là đổi hợp đồng, phải sửa test có chủ đích.
        assert_eq!(chay(), 0);
    }

    #[test]
    fn thu_thap_khong_rong_va_moi_muc_co_ten() {
        let m = thu_thap();
        assert!(m.len() >= 8, "quá ít hạng mục: {}", m.len());
        assert!(m.iter().all(|x| !x.ten.is_empty()));
    }

    #[test]
    fn vision_chi_dung_duoc_khi_du_ca_bon_dieu_kien() {
        // Bảng chân lý ĐẦY ĐỦ trên 4 biến (16 tổ hợp, `n_gpu_layers` rút về
        // 0/khác-0). Duyệt hết chứ không liệt kê tay: liệt kê tay là cách một
        // tổ hợp bị bỏ sót mà test vẫn xanh.
        let mut so_ok = 0;
        for la_debug in [true, false] {
            for co_cuda in [true, false] {
                for co_gpu in [true, false] {
                    for n in [0u32, 999] {
                        let m = muc_vision(la_debug, co_cuda, co_gpu, n);
                        let mong_doi = !la_debug && co_cuda && co_gpu && n > 0;
                        assert_eq!(
                            m.ok,
                            Some(mong_doi),
                            "debug={la_debug} cuda={co_cuda} gpu={co_gpu} layers={n}"
                        );
                        if mong_doi {
                            so_ok += 1;
                        } else {
                            assert!(
                                !m.he_qua.is_empty(),
                                "nhánh ✗ nào cũng phải nói cách khắc phục: \
                                 debug={la_debug} cuda={co_cuda} gpu={co_gpu} layers={n}"
                            );
                        }
                    }
                }
            }
        }
        // Đúng MỘT tổ hợp trong 16 là dùng được. Chốt con số lại để nếu ai nới
        // điều kiện ra thì test đỏ chứ không trôi im.
        assert_eq!(so_ok, 1, "chỉ được đúng 1/16 tổ hợp là ✓");
    }

    #[test]
    fn nhanh_debug_khong_bao_gio_khuyen_sai_viec() {
        // Ở build debug, lời khuyên đúng là "build release" — KHÔNG phải "cài
        // driver" hay "bật cuda", vì đổi hai thứ đó ở debug vẫn không chạy được.
        let m = muc_vision(true, false, false, 0);
        assert!(m.he_qua.contains("--release"), "{}", m.he_qua);
    }

    #[test]
    fn n_gpu_layers_bang_0_khong_bao_gio_la_xanh() {
        // Chốt riêng cái bẫy đã suýt lọt: đủ release+CUDA+GPU mà layers=0 thì
        // vision vẫn ~80 s. Nếu ai đó "đơn giản hoá" điều kiện này đi, test đỏ.
        let m = muc_vision(false, true, true, 0);
        assert_eq!(m.ok, Some(false));
        assert!(m.he_qua.contains("LIVA_LLM_N_GPU_LAYERS"), "{}", m.he_qua);
    }
}
