//! Qwen3-VL-2B unified-core PoC / integration check (roadmap v2 step 3).
//! Exercises the PRODUCTION path — `LlamaRouterManager::swap_model` (auto-detects
//! ChatML), `compile_prompt` (text), and `answer_with_image` (vision via mtmd) —
//! so it validates exactly what ships, not a hand-written harness.
//!
//! Run:  cargo run --release --bin qwen3vl_probe [image.png]   (no arg → screen)
//! Env:  LIVA_QWENVL_DIR (default E:\AI_Models\Qwen3-VL-2B-Instruct-GGUF),
//!       LIVA_QWENVL_LM, LIVA_QWENVL_MMPROJ, LIVA_QWENVL_NGL (default 0 = CPU),
//!       LIVA_QWENVL_NCTX (default 8192), LIVA_QWENVL_SKIP_VISION=1.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use liva_native_core::llm::engine::VisionImage;
use liva_native_core::llm::{ChatMessage, LlamaRouterManager, compile_prompt, persona};

fn stream_print(piece: &str) -> bool {
    print!("{piece}");
    let _ = std::io::stdout().flush();
    true
}

#[tokio::main]
async fn main() {
    let dir = std::env::var("LIVA_QWENVL_DIR")
        .unwrap_or_else(|_| "E:\\AI_Models\\Qwen3-VL-2B-Instruct-GGUF".to_string());
    let lm = std::env::var("LIVA_QWENVL_LM")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&dir).join("Qwen3-VL-2B-Instruct-Q4_K_M.gguf"));
    let mmproj = std::env::var("LIVA_QWENVL_MMPROJ")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(&dir).join("mmproj-F16.gguf"));
    let ngl: u32 = std::env::var("LIVA_QWENVL_NGL").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let n_ctx: usize =
        std::env::var("LIVA_QWENVL_NCTX").ok().and_then(|s| s.parse().ok()).unwrap_or(8192);
    let skip_vision = std::env::var("LIVA_QWENVL_SKIP_VISION").as_deref() == Ok("1");

    if !lm.exists() {
        eprintln!("LM GGUF not found: {:?}", lm);
        std::process::exit(1);
    }

    eprintln!("[step] swap_model (ngl={ngl}, n_ctx={n_ctx}) ...");
    let t0 = Instant::now();
    let mut mgr = LlamaRouterManager::new(n_ctx, ngl).unwrap();
    mgr.swap_model(&lm, Some(n_ctx), Some(ngl), Some(false))
        .await
        .expect("swap_model");
    mgr.set_mmproj_path(Some(mmproj));
    eprintln!("[step] model ready in {:.2}s (CHATML auto-detected)", t0.elapsed().as_secs_f32());

    // ── (1) TEXT via the production compile_prompt + generate_completion ────
    println!("\n=== TEXT (compile_prompt → ChatML → generate) ===");
    let text_q = "Bạn là ai, và giúp được gì khi tôi đang bận chơi game?";
    let messages = vec![
        ChatMessage { role: "system".to_string(), content: persona::PERSONA_LIVA.to_string() },
        ChatMessage { role: "user".to_string(), content: text_q.to_string() },
    ];
    let prompt = compile_prompt(&messages).expect("compile_prompt");
    println!("User: {text_q}\nLIVA: ");
    let t = Instant::now();
    let out = mgr
        .generate_completion(&prompt, 0.7, 0.8, stream_print)
        .expect("generate_completion");
    let dt = t.elapsed().as_secs_f32();
    println!(
        "\n[text] {} tok in {dt:.2}s ({:.1} tok/s)",
        out.completion_tokens,
        out.completion_tokens as f32 / dt.max(0.001)
    );

    if skip_vision {
        println!("\n(vision skipped)");
        return;
    }

    // ── (2) VISION via the production answer_with_image ─────────────────────
    println!("\n=== VISION (answer_with_image → mtmd) ===");
    let vis_q = "Trên màn hình đang hiển thị gì? Mô tả ngắn gọn bằng tiếng Việt.";
    println!("User: {vis_q}\nLIVA: ");
    let t = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let vout = if let Some(path) = args.get(1) {
        eprintln!("[step] image file: {path}");
        let bytes = std::fs::read(path).expect("read image file");
        mgr.answer_with_image(vis_q, VisionImage::Encoded(&bytes), 0.7, 0.8, stream_print)
    } else {
        eprintln!("[step] context-aware capture (LIVA_VISION_REGION={}) ...",
            std::env::var("LIVA_VISION_REGION").unwrap_or_else(|_| "auto".into()));
        let (vw, vh, rgb) = liva_native_core::vision::capture::capture_for_vision()
            .expect("capture_for_vision");
        eprintln!("[step] captured region {}x{}", vw, vh);
        mgr.answer_with_image(
            vis_q,
            VisionImage::Rgb { width: vw, height: vh, data: &rgb },
            0.7,
            0.8,
            stream_print,
        )
    }
    .expect("answer_with_image");
    let vdt = t.elapsed().as_secs_f32();
    println!(
        "\n[vision] {} prompt tok, {} gen tok in {vdt:.2}s ({:.1} tok/s)",
        vout.prompt_tokens,
        vout.completion_tokens,
        vout.completion_tokens as f32 / vdt.max(0.001)
    );

    println!("\nPoC done: production path — swap_model + compile_prompt (text) + answer_with_image (vision).");
}
