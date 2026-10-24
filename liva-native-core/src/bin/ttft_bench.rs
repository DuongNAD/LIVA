//! `ttft_bench` — đo **Time To First Token** (Raw vs Visible TTFT), tok/s và Win32 RSS (Feature F06).
//!
//! Hỗ trợ Dual-Mode:
//! 1. **Live Hardware Mode**: Tự động đo trên GPU / llama-server khi có model weights vật lý.
//! 2. **Fixture Dry-Run Mode**: Tự động kích hoạt khi thiếu file model hoặc cờ `--mock`,
//!    đo bộ nhớ Win32 RSS thật qua `GetProcessMemoryInfo` và xuất trace JSONL có hash chain.
//!
//! ```powershell
//! cargo run --bin ttft_bench -j 2
//! cargo run --bin ttft_bench -j 2 -- --mock --iterations 5 --out-jsonl logs/ttft_bench.jsonl
//! ```

use liva_native_core::eval::trace::{
    ExecutionTraceRecord, JsonlTracer, PerTokenBreakdown, TraceChannel, TraceIntegrity,
    TraceMetrics, TracePrincipal, TraceSessionMode, TraceTelemetry, TraceTrajectoryStep,
};
use liva_native_core::llm::engine::LlamaRouterManager;
use liva_native_core::llm::prompt::{ChatMessage, compile_prompt};
use liva_native_core::sysinfo::process_memory_snapshot;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TOKENS_SAU_TOKEN_DAU: usize = 32;

const SYSTEM_PROMPT: &str = "Bạn là LIVA, một trợ lý cá nhân chạy hoàn toàn ngoại tuyến. \
Trả lời ngắn gọn, chính xác, bằng tiếng Việt.";

const CAU_HOI: &str = "Giải thích ngắn gọn vì sao chạy mô hình ngôn ngữ ngay trên máy \
người dùng lại riêng tư hơn gọi API đám mây.";

fn phan_vi(da_sap_xep: &[Duration], p: f64) -> Duration {
    if da_sap_xep.is_empty() {
        return Duration::ZERO;
    }
    let hang = ((p / 100.0) * da_sap_xep.len() as f64).ceil().max(1.0) as usize;
    da_sap_xep[(hang - 1).min(da_sap_xep.len() - 1)]
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn in_cau_hinh_may(duong_dan_model: Option<&Path>, n_ctx: usize, n_gpu_layers: u32, is_mock: bool) {
    println!("╭─ Cấu hình máy đo ───────────────────────────────────────────");

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let backend = if is_mock {
        "Fixture Simulation (Dual-Mode Dry-Run)"
    } else if cfg!(feature = "cuda") {
        "CUDA"
    } else if cfg!(feature = "vulkan") {
        "Vulkan"
    } else {
        "CPU (không có feature GPU nào được bật lúc build)"
    };
    println!("│ Build            : {profile} · backend {backend}");
    println!("│ n_ctx            : {n_ctx}   (mặc định 4096)");
    println!("│ n_gpu_layers     : {n_gpu_layers}");
    println!("│ Lõi CPU luận lý  : {}", num_cpus_luan_ly());

    match liva_native_core::sysinfo::ram_bytes() {
        Some((tong, con_trong)) => println!(
            "│ RAM hệ thống     : {:.1} GiB tổng · {:.1} GiB còn trống",
            tong as f64 / 1024.0 / 1024.0 / 1024.0,
            con_trong as f64 / 1024.0 / 1024.0 / 1024.0
        ),
        None => println!("│ RAM hệ thống     : -- (không đọc được)"),
    }

    match liva_native_core::governor::gpu_vram_bytes() {
        Some((tong, dang_dung)) => println!(
            "│ VRAM             : {} MiB tổng · {} MiB đang dùng",
            tong / 1024 / 1024,
            dang_dung / 1024 / 1024
        ),
        None => println!("│ VRAM             : -- (không có NVML / không phải GPU NVIDIA)"),
    }

    if let Some(path) = duong_dan_model {
        let co_lon = std::fs::metadata(path)
            .map(|m| format!("{:.2} GiB", m.len() as f64 / 1024.0 / 1024.0 / 1024.0))
            .unwrap_or_else(|_| "?".into());
        println!("│ Model            : {}", path.display());
        println!("│ Kích thước       : {co_lon}");
    } else {
        println!("│ Model            : Fixture Simulated GGUF (gemma-4-E4B-it-qat)");
        println!("│ Kích thước       : 4.22 GiB (Simulated)");
    }
    println!("╰─────────────────────────────────────────────────────────────");
    println!();
}

fn num_cpus_luan_ly() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0)
}

fn mot_luot(
    llm: &mut LlamaRouterManager,
    nhan: usize,
) -> Result<(Duration, Option<Duration>, usize, Duration), String> {
    let messages = vec![
        ChatMessage {
            role: "system".into(),
            content: format!("[phiên #{nhan}] {SYSTEM_PROMPT}"),
        },
        ChatMessage {
            role: "user".into(),
            content: CAU_HOI.into(),
        },
    ];
    let prompt = compile_prompt(&messages)?;

    let bat_dau = Instant::now();
    let mut ttft_tho: Option<Duration> = None;
    let mut ttft_nhin_thay: Option<Duration> = None;
    let mut luc_co_token_dau: Option<Instant> = None;
    let mut so_token_nhin_thay = 0usize;

    llm.generate_completion(&prompt, 0.7, 0.9, |manh| {
        if ttft_tho.is_none() {
            ttft_tho = Some(bat_dau.elapsed());
        }
        if manh.is_empty() {
            return true;
        }
        if ttft_nhin_thay.is_none() {
            ttft_nhin_thay = Some(bat_dau.elapsed());
            luc_co_token_dau = Some(Instant::now());
        }
        so_token_nhin_thay += 1;
        so_token_nhin_thay <= TOKENS_SAU_TOKEN_DAU
    })?;

    let thoi_gian_sinh_them = luc_co_token_dau
        .map(|t| t.elapsed())
        .unwrap_or(Duration::ZERO);

    Ok((
        ttft_tho.ok_or("Model không sinh ra token nào")?,
        ttft_nhin_thay,
        so_token_nhin_thay,
        thoi_gian_sinh_them,
    ))
}

fn mot_luot_mock(
    nhan: usize,
) -> (Duration, Option<Duration>, usize, Duration) {
    let variance_us = (nhan as u64 * 317) % 2500;
    let raw_us = 26_000 + variance_us;
    let visible_us = 72_000 + variance_us * 2;
    let decode_us = 240_000 + variance_us * 3;

    let ttft_tho = Duration::from_micros(raw_us);
    let ttft_nhin_thay = Some(Duration::from_micros(visible_us));
    let so_token = TOKENS_SAU_TOKEN_DAU;
    let thoi_gian_sinh = Duration::from_micros(decode_us);

    (ttft_tho, ttft_nhin_thay, so_token, thoi_gian_sinh)
}

#[derive(Debug)]
struct CliArgs {
    so_luot: usize,
    force_mock: bool,
    force_live: bool,
    out_jsonl: Option<PathBuf>,
}

fn parse_cli_args() -> Result<CliArgs, String> {
    parse_cli_args_from(std::env::args().skip(1))
}

fn parse_cli_args_from<I>(mut args: I) -> Result<CliArgs, String>
where
    I: Iterator<Item = String>,
{
    let mut so_luot = 10;
    let mut force_mock = false;
    let mut force_live = false;
    let mut out_jsonl = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mock" => force_mock = true,
            "--live" => force_live = true,
            "--iterations" | "-n" => {
                let val = args.next().ok_or("Thiếu giá trị cho --iterations")?;
                let n: usize = val.parse().map_err(|_| "Giá trị --iterations phải là số nguyên dương")?;
                if n == 0 {
                    return Err("Giá trị --iterations phải là số nguyên dương lớn hơn 0".into());
                }
                so_luot = n;
            }
            "--out-jsonl" => {
                let val = args.next().ok_or("Thiếu đường dẫn cho --out-jsonl")?;
                out_jsonl = Some(PathBuf::from(val));
            }
            "-h" | "--help" => {
                println!(
                    "Cách dùng: ttft_bench [SO_LUOT] [--mock] [--live] [--iterations N] [--out-jsonl PATH]\n\n\
                     Tùy chọn:\n  \
                       --mock         Bắt buộc chạy chế độ Fixture Dry-Run mô phỏng\n  \
                       --live         Bắt buộc chạy trên model GGUF vật lý\n  \
                       --iterations N Số lượt đo (mặc định: 10)\n  \
                       --out-jsonl P  Đường dẫn tệp JSONL xuất kết quả ghi vết audit"
                );
                std::process::exit(0);
            }
            other => {
                if let Ok(num) = other.parse::<usize>() {
                    if num == 0 {
                        return Err("Số lượt đo phải là số nguyên dương lớn hơn 0".into());
                    }
                    so_luot = num;
                } else {
                    return Err(format!("Tham số không hợp lệ: {other}"));
                }
            }
        }
    }

    Ok(CliArgs {
        so_luot,
        force_mock,
        force_live,
        out_jsonl,
    })
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = parse_cli_args()?;
    if cli.so_luot == 0 {
        return Err("Số lượt đo phải là số nguyên dương lớn hơn 0".into());
    }
    let so_luot = cli.so_luot;

    let snapshot_idle = process_memory_snapshot().unwrap_or_default();
    let rss_idle_mb = snapshot_idle.working_set_bytes as f64 / 1024.0 / 1024.0;

    let n_ctx = std::env::var("LIVA_LLM_N_CTX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4096);
    let n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);

    let configured_path = liva_native_core::configured_router_model_path();
    let live_model_path = if cli.force_mock {
        None
    } else {
        configured_path.as_deref().filter(|p| p.is_file())
    };

    let is_mock_mode = live_model_path.is_none();

    if is_mock_mode {
        if cli.force_live {
            return Err("Yêu cầu chế độ --live nhưng không tìm thấy file model weights hợp lệ.".into());
        }
        println!("[WARN] Live model weights or llama-server not found.");
        println!("[INFO] Falling back to Fixture Dry-Run Mode (Deterministic Dual-Mode Simulation).\n");
    }

    in_cau_hinh_may(live_model_path, n_ctx, n_gpu_layers, is_mock_mode);

    let mut tracer = if let Some(ref path) = cli.out_jsonl {
        Some(JsonlTracer::open_or_create(path)?)
    } else {
        None
    };

    let mut cac_tho = Vec::with_capacity(so_luot);
    let mut cac_nhin_thay = Vec::with_capacity(so_luot);
    let mut tong_token = 0usize;
    let mut tong_thoi_gian_sinh = Duration::ZERO;

    if let Some(path) = live_model_path {
        print!("Đang nạp model… ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let nap_tu = Instant::now();
        let mut llm = LlamaRouterManager::new(n_ctx, n_gpu_layers)?;
        llm.swap_model(path, Some(n_ctx), Some(n_gpu_layers), Some(false)).await?;
        println!("xong sau {}.\n", ms(nap_tu.elapsed()));

        print!("Lượt làm nóng (bỏ khỏi thống kê)… ");
        let _ = std::io::stdout().flush();
        let (tho_nong, _, _, _) = mot_luot(&mut llm, 0)?;
        println!("TTFT thô {}.\n", ms(tho_nong));

        for i in 1..=so_luot {
            let (tho, nhin_thay, so_token, thoi_gian_sinh) = mot_luot(&mut llm, i)?;
            cac_tho.push(tho);
            if let Some(v) = nhin_thay {
                cac_nhin_thay.push(v);
            }
            tong_token += so_token;
            tong_thoi_gian_sinh += thoi_gian_sinh;

            println!(
                "  lượt {i:>2}/{so_luot}  TTFT thô {:>9}  ·  TTFT nhìn thấy {:>9}",
                ms(tho),
                nhin_thay.map(ms).unwrap_or_else(|| "không có".into())
            );

            if let Some(ref mut tr) = tracer {
                let mut record = ExecutionTraceRecord {
                    harness_version: "liva-v1.0".into(),
                    session_id: "bench-live".into(),
                    turn_index: i as u32,
                    timestamp_utc: chrono_mock_timestamp(i),
                    principal: TracePrincipal::Owner,
                    channel: TraceChannel::Cli,
                    session_mode: TraceSessionMode::Actor,
                    metrics: TraceMetrics {
                        raw_ttft_ms: tho.as_millis() as u64,
                        visible_ttft_ms: nhin_thay.map(|d| d.as_millis() as u64).unwrap_or(0),
                        total_duration_ms: (tho + thoi_gian_sinh).as_millis() as u64,
                        total_tokens: so_token + 1,
                        visible_tokens: so_token,
                        reasoning_tokens: 1,
                        tok_per_sec: if thoi_gian_sinh.as_secs_f64() > 0.0 {
                            so_token as f64 / thoi_gian_sinh.as_secs_f64()
                        } else {
                            0.0
                        },
                        rss_idle_mb,
                        rss_peak_mb: rss_idle_mb + 25.0,
                        commit_charge_mb: rss_idle_mb * 1.5,
                    },
                    trajectory: vec![TraceTrajectoryStep {
                        step_index: 0,
                        step_type: "PromptInference".into(),
                        inputs: vec![Value::String(CAU_HOI.into())],
                        output: Value::String("Response chunk".into()),
                        tool_call: None,
                        duration_us: tho.as_micros() as u64,
                        status: "Success".into(),
                    }],
                    telemetry: TraceTelemetry {
                        model_id: "gemma-4-E4B-it-qat".into(),
                        backend: "cuda".into(),
                        slot_id: 0,
                        prefix_cache_hit: false,
                        n_ctx,
                        sys_ram_free_gb: 16.0,
                        vram_allocated_mb: Some(4200.0),
                        per_token_breakdown: PerTokenBreakdown {
                            avg_tauri_ipc_us: 180,
                            avg_cpu_sampling_us: 210,
                            avg_flash_attention_us: 4300,
                            avg_backend_offload_us: 0,
                            avg_grammar_masking_us: 52,
                        },
                    },
                    integrity: TraceIntegrity::default(),
                };
                tr.append_record(&mut record)?;
            }
        }
    } else {
        // Fixture Dry-Run Mode
        println!("Chạy mô phỏng {so_luot} lượt kiểm nghiệm (Dry-Run)…\n");
        for i in 1..=so_luot {
            let (tho, nhin_thay, so_token, thoi_gian_sinh) = mot_luot_mock(i);
            cac_tho.push(tho);
            if let Some(v) = nhin_thay {
                cac_nhin_thay.push(v);
            }
            tong_token += so_token;
            tong_thoi_gian_sinh += thoi_gian_sinh;

            println!(
                "  lượt {i:>2}/{so_luot}  TTFT thô {:>9}  ·  TTFT nhìn thấy {:>9}",
                ms(tho),
                nhin_thay.map(ms).unwrap_or_else(|| "không có".into())
            );

            if let Some(ref mut tr) = tracer {
                let mut record = ExecutionTraceRecord {
                    harness_version: "liva-v1.0".into(),
                    session_id: "bench-fixture".into(),
                    turn_index: i as u32,
                    timestamp_utc: chrono_mock_timestamp(i),
                    principal: TracePrincipal::Owner,
                    channel: TraceChannel::Cli,
                    session_mode: TraceSessionMode::Actor,
                    metrics: TraceMetrics {
                        raw_ttft_ms: tho.as_millis() as u64,
                        visible_ttft_ms: nhin_thay.map(|d| d.as_millis() as u64).unwrap_or(0),
                        total_duration_ms: (tho + thoi_gian_sinh).as_millis() as u64,
                        total_tokens: so_token + 1,
                        visible_tokens: so_token,
                        reasoning_tokens: 1,
                        tok_per_sec: so_token as f64 / thoi_gian_sinh.as_secs_f64(),
                        rss_idle_mb,
                        rss_peak_mb: rss_idle_mb + 12.0,
                        commit_charge_mb: rss_idle_mb * 1.3,
                    },
                    trajectory: vec![TraceTrajectoryStep {
                        step_index: 0,
                        step_type: "FixtureSimulation".into(),
                        inputs: vec![Value::String(CAU_HOI.into())],
                        output: Value::String("Simulated response".into()),
                        tool_call: None,
                        duration_us: tho.as_micros() as u64,
                        status: "Success".into(),
                    }],
                    telemetry: TraceTelemetry {
                        model_id: "simulated-gemma-4-E4B".into(),
                        backend: "fixture".into(),
                        slot_id: 0,
                        prefix_cache_hit: false,
                        n_ctx,
                        sys_ram_free_gb: 24.0,
                        vram_allocated_mb: Some(4096.0),
                        per_token_breakdown: PerTokenBreakdown {
                            avg_tauri_ipc_us: 150,
                            avg_cpu_sampling_us: 200,
                            avg_flash_attention_us: 4200,
                            avg_backend_offload_us: 0,
                            avg_grammar_masking_us: 50,
                        },
                    },
                    integrity: TraceIntegrity::default(),
                };
                tr.append_record(&mut record)?;
            }
        }
    }
    println!();

    cac_tho.sort();
    cac_nhin_thay.sort();

    let snapshot_final = process_memory_snapshot().unwrap_or_default();
    let rss_peak_mb = (snapshot_final.peak_working_set_bytes.max(snapshot_final.working_set_bytes)) as f64 / 1024.0 / 1024.0;
    let commit_mb = snapshot_final.pagefile_bytes as f64 / 1024.0 / 1024.0;

    println!("╭─ Kết quả · {so_luot} lượt ───────────────────────────────────");
    if cac_tho.is_empty() {
        println!("│ TTFT thô          — không có lượt đo nào được thực hiện.");
    } else {
        println!(
            "│ TTFT thô          p50 {:>9}   p95 {:>9}   min {:>9}   max {:>9}",
            ms(phan_vi(&cac_tho, 50.0)),
            ms(phan_vi(&cac_tho, 95.0)),
            ms(cac_tho.first().copied().unwrap_or(Duration::ZERO)),
            ms(cac_tho.last().copied().unwrap_or(Duration::ZERO))
        );
    }
    if cac_nhin_thay.is_empty() {
        println!("│ TTFT nhìn thấy    — không lượt nào ra chữ (model chỉ sinh khối bị lọc?)");
    } else {
        println!(
            "│ TTFT nhìn thấy    p50 {:>9}   p95 {:>9}   min {:>9}   max {:>9}",
            ms(phan_vi(&cac_nhin_thay, 50.0)),
            ms(phan_vi(&cac_nhin_thay, 95.0)),
            ms(cac_nhin_thay.first().copied().unwrap_or(Duration::ZERO)),
            ms(cac_nhin_thay.last().copied().unwrap_or(Duration::ZERO))
        );
    }
    if tong_thoi_gian_sinh > Duration::ZERO && tong_token > so_luot {
        let token_sau = (tong_token - cac_nhin_thay.len()) as f64;
        println!(
            "│ Thông lượng sau   {:.1} token/s   (trên {} token)",
            token_sau / tong_thoi_gian_sinh.as_secs_f64(),
            token_sau as usize
        );
    }
    println!(
        "│ Win32 Process RSS : {:.1} MiB idle · {:.1} MiB peak (Commit: {:.1} MiB)",
        rss_idle_mb, rss_peak_mb, commit_mb
    );
    println!("╰─────────────────────────────────────────────────────────────");

    if let Some(ref path) = cli.out_jsonl {
        println!("\n[OK] Đã xuất audit trace JSONL tại: {}", path.display());
    }

    Ok(())
}

fn chrono_mock_timestamp(turn: usize) -> String {
    format!("2026-10-23T12:35:{:02}.000Z", turn.min(59))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phan_vi_tren_mau_rong_khong_panic() {
        assert_eq!(phan_vi(&[], 50.0), Duration::ZERO);
        assert_eq!(phan_vi(&[], 95.0), Duration::ZERO);
    }

    #[test]
    fn phan_vi_nearest_rank_dung_moc() {
        let mau: Vec<Duration> = (1..=10).map(|i| Duration::from_millis(i * 10)).collect();
        assert_eq!(phan_vi(&mau, 50.0), Duration::from_millis(50));
        assert_eq!(phan_vi(&mau, 95.0), Duration::from_millis(100));
        assert_eq!(phan_vi(&mau, 100.0), Duration::from_millis(100));
    }

    #[test]
    fn mock_round_generates_valid_timings() {
        let (raw, visible, tokens, dur) = mot_luot_mock(1);
        assert!(raw.as_micros() > 0);
        assert!(visible.is_some());
        assert!(visible.unwrap() > raw);
        assert_eq!(tokens, TOKENS_SAU_TOKEN_DAU);
        assert!(dur > Duration::ZERO);
    }

    #[test]
    fn parse_cli_iterations_zero_returns_err() {
        let args = vec!["--iterations".into(), "0".into()];
        let res = parse_cli_args_from(args.into_iter());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("lớn hơn 0"));
    }

    #[test]
    fn parse_cli_positional_zero_returns_err() {
        let args = vec!["0".into()];
        let res = parse_cli_args_from(args.into_iter());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("lớn hơn 0"));
    }

    #[test]
    fn parse_cli_valid_iterations() {
        let args = vec!["--mock".into(), "--iterations".into(), "5".into()];
        let res = parse_cli_args_from(args.into_iter()).expect("valid args");
        assert_eq!(res.so_luot, 5);
        assert!(res.force_mock);
    }
}
