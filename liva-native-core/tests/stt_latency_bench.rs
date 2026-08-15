//! First-chunk STT latency benchmark, isolated from the parallel test sweep.
//!
//! # Why this is a separate binary, and why it is `#[ignore]`
//!
//! This measures wall-clock ONNX inference latency, so its result is only meaningful when
//! nothing else is competing for the CPU. Cargo runs test binaries - and the tests inside each
//! binary - in parallel by default, so under a normal `cargo test` this same measurement moves
//! by 4-7x depending on what else happens to be running:
//!
//! ```text
//!   fully parallel with the rest of the suite   P50 268.7 / 504.8 / 534.0 / 586.5 ms
//!   serialized, other binaries still running    P50 107.1 / 120.8 / 129.4 ms
//!   alone, single-threaded (this benchmark)     P50  74.2 /  79.6 / 105.0 ms
//! ```
//!
//! A bound wide enough to survive the first case cannot detect a regression, and a bound set
//! for the third case fails constantly in the first. Neither is a threshold problem, so neither
//! is fixed by changing the number: the measurement has to stop happening under contention.
//! Living in its own binary is not sufficient on its own - cargo would still run that binary
//! alongside the others - so the benchmark is `#[ignore]`d out of the default sweep and run
//! deliberately:
//!
//! ```text
//!   cargo test --release --manifest-path liva-native-core/Cargo.toml \
//!       --test stt_latency_bench -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! Run it on an otherwise idle machine. In CI give it its own step, not a shared runner slot.
//!
//! The correctness half of the STT suite - cadence, monotonicity, WER, anti-hallucination -
//! stays in `stt_adversarial_stress.rs` and still runs in parallel on every `cargo test`, where
//! contention does not affect the outcome.

mod common;

use common::{find_audio_file, load_audio_wav_16k, resolve_model_paths};
use liva_native_core::stt::parakeet::ParakeetVi;
use std::time::{Duration, Instant};

/// First-chunk latency against the 150ms uncontended steady-state budget.
///
/// 150ms is the product budget this benchmark exists to police, and running uncontended is what
/// makes asserting it honest: measured P50 74-105ms, so roughly 1.4x headroom. If this fails,
/// either the model path genuinely regressed or the machine was not idle - check the printed
/// P50 against the band above before assuming the former.
#[test]
#[ignore = "wall-clock benchmark: run alone with --ignored --test-threads=1, see module docs"]
fn bench_parakeet_first_chunk_latency_budget() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        eprintln!("Skipping benchmark: Parakeet model files not found");
        return;
    }

    let mut pk = ParakeetVi::load(&model_path, &vocab_path).expect("Failed to load ParakeetVi");
    let audio_path = find_audio_file("data/benchmarks/fleurs-vi/audio/0000.wav");
    let audio = load_audio_wav_16k(&audio_path).expect("Failed to load audio");

    const CHUNK_SIZE: usize = 2560; // 160ms @ 16kHz
    let first_chunk = &audio[0..CHUNK_SIZE.min(audio.len())];

    // Warm-up run: the first inference pays one-off model and allocator costs that are not part
    // of steady-state latency.
    let _ = pk.feed_chunk(first_chunk, false);
    pk.reset_stream();

    let mut latencies: Vec<Duration> = Vec::new();
    for _ in 0..10 {
        pk.reset_stream();
        let t0 = Instant::now();
        let _ = pk.feed_chunk(first_chunk, false).expect("feed_chunk failed");
        latencies.push(t0.elapsed());
    }

    latencies.sort();
    let min_lat = latencies[0];
    let p50_lat = latencies[latencies.len() / 2];
    let max_lat = latencies[latencies.len() - 1];

    println!(
        "[STT Latency Bench] First-Chunk Latency (160ms frame): Min={:?}, P50={:?}, Max={:?}",
        min_lat, p50_lat, max_lat
    );

    // Asserted on P50 only. `min <= p50` always holds, so a min bound at the same number could
    // never fail on its own, and the max is a single-sample tail that reflects scheduler noise
    // more than the model.
    // Debug builds are unoptimized; the strict number is the release contract.
    const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
    assert!(
        p50_lat < Duration::from_millis(150) * SLOWDOWN,
        "First-chunk latency p50 ({:?}) must be < 150ms on CPU. If the machine was not idle, \
         this measures contention rather than the model - see the module docs.",
        p50_lat
    );
}
