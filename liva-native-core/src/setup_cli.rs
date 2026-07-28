//! `--setup-models`: tải model từ chính binary, không cần Node.
//!
//! # Vì sao là module của BINARY, không phải của lib
//!
//! Cùng lý do với `preflight`: nó chỉ phục vụ dòng lệnh. Logic thật nằm ở
//! `liva_native_core::setup` (lib) nên vỏ Tauri dùng chung được — ở đây chỉ có
//! phần vẽ ra terminal.
//!
//! Đây cũng là đường **kiểm chứng được ngay** cho quy trình tải: cửa sổ thiết
//! lập của bản cài gọi đúng `setup::fetch_missing` này, chỉ khác chỗ hiển thị.

use liva_native_core::setup;

/// In số byte theo độ lớn — cùng quy tắc với `scripts/models.mjs` để hai bộ
/// công cụ không báo hai con số trông khác nhau cho cùng một file.
fn co(n: u64) -> String {
    const G: f64 = 1024.0 * 1024.0 * 1024.0;
    const M: f64 = 1024.0 * 1024.0;
    let f = n as f64;
    if f >= G {
        format!("{:.2} GB", f / G)
    } else if f >= 10.0 * M {
        format!("{:.1} MB", f / M)
    } else if f >= M {
        format!("{:.2} MB", f / M)
    } else {
        format!("{:.0} KB", f / 1024.0)
    }
}

fn co_tham_so(ten: &str) -> bool {
    std::env::args().skip(1).any(|a| a == ten)
}

pub async fn chay() -> i32 {
    let profile = if co_tham_so("--profile-full") || co_tham_so("--full") {
        "full"
    } else {
        "minimal"
    };
    let force = co_tham_so("--force");

    let m = match setup::load_manifest() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("LỖI: {e}");
            return 1;
        }
    };

    let llm_dir = liva_native_core::configured_models_dir();
    let res_root = liva_native_core::resource_write_root();

    println!("\nLIVA — chuẩn bị model (profile \"{profile}\")\n");
    println!("  Thư mục tài nguyên : {}", res_root.display());
    println!("  Thư mục LLM        : {}", llm_dir.display());

    let truoc = setup::status(&m, profile, &llm_dir, &res_root);
    if truoc.missing.is_empty() {
        println!("\n  ✅ Đã đủ model cho profile này. Không phải tải gì.");
        return 0;
    }
    println!(
        "\n  Cần tải: {} file, ~{}\n",
        truoc.missing.iter().filter(|f| f.downloadable).count(),
        co(truoc
            .missing
            .iter()
            .filter(|f| f.downloadable)
            .map(|f| f.bytes)
            .sum::<u64>())
    );

    let la_tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let mut file_dang_tai = String::new();
    let mut moc = std::time::Instant::now() - std::time::Duration::from_secs(1);

    let tt = setup::fetch_missing(
        &m,
        profile,
        &llm_dir,
        &res_root,
        force,
        |p: setup::Progress| {
            if p.dest != file_dang_tai {
                if !file_dang_tai.is_empty() && la_tty {
                    println!();
                }
                file_dang_tai = p.dest.clone();
                println!("  [{}/{}] {}", p.index, p.total_files, p.dest);
            }
            // Thanh tiến trình chỉ vẽ khi ra terminal thật: `\r` không xoá được
            // gì khi bị pipe vào log, nó chỉ dính mọi dòng lại với nhau.
            if la_tty && moc.elapsed() >= std::time::Duration::from_millis(400) {
                moc = std::time::Instant::now();
                let pt = if p.total > 0 {
                    format!("{:.1}%", (p.downloaded as f64 / p.total as f64) * 100.0)
                } else {
                    co(p.downloaded)
                };
                print!("\r      {:>7}  {}   ", pt, co(p.downloaded));
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
        },
    )
    .await;

    if la_tty && !file_dang_tai.is_empty() {
        println!();
    }
    println!();

    if !tt.skipped_manual.is_empty() {
        println!("  Phải tự chuẩn bị (không có nguồn tải công khai):");
        for d in &tt.skipped_manual {
            println!("    · {d}");
        }
        println!();
    }

    if !tt.failed.is_empty() {
        println!("  ❌ {} file tải hỏng:", tt.failed.len());
        for e in &tt.failed {
            println!("    · {e}");
        }
        println!("\n  Chạy lại lệnh này — phần đã tải được giữ lại và tải tiếp.");
        return 1;
    }

    let sau = setup::status(&m, profile, &llm_dir, &res_root);
    println!("  ✅ {} file xong.", tt.downloaded);
    if sau.blocking {
        println!("  ⚠ Vẫn còn thiếu model BẮT BUỘC — xem danh sách bên trên.");
        return 1;
    }
    0
}
