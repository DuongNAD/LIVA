use std::hint::black_box;
use std::time::{Duration, Instant};
use liva_native_core::vision::diff::{find_changes, find_changes_u32, BoundingBox};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const STRIDE_PIXELS: usize = WIDTH;
const STRIDE_BYTES: usize = WIDTH * 4;

fn run_find_changes_bench<F>(name: &str, mut func: F)
where
    F: FnMut() -> Option<BoundingBox>,
{
    // Warmup
    for _ in 0..100 {
        black_box(func());
    }

    let iterations = 1000;
    let mut times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        let res = func();
        let elapsed = start.elapsed();
        black_box(res);
        times.push(elapsed);
    }

    times.sort();
    let min = times[0];
    let max = times[times.len() - 1];
    let total: Duration = times.iter().sum();
    let avg = total / iterations as u32;
    let median = times[times.len() / 2];

    println!("--------------------------------------------------");
    println!("Benchmark: {}", name);
    println!("  Min   : {:?}", min);
    println!("  Max   : {:?}", max);
    println!("  Mean  : {:?}", avg);
    println!("  Median: {:?}", median);
}

fn main() {
    println!("Starting Screen Vision Row-Column Narrowing Scanning Benchmarks");
    println!("Dimensions: {}x{} (1080p)", WIDTH, HEIGHT);
    println!("==================================================");

    // =========================================================================
    // 1. find_changes benchmarks (using u32 pixels)
    // =========================================================================
    let frame_a_pixels = vec![0u32; WIDTH * HEIGHT];

    // Case A: 0% changes (fast-path)
    let frame_b_pixels_0 = vec![0u32; WIDTH * HEIGHT];
    run_find_changes_bench("find_changes (0% change)", || {
        find_changes(&frame_a_pixels, &frame_b_pixels_0, WIDTH, HEIGHT, STRIDE_PIXELS).unwrap()
    });

    // Case B: Tiny local change (10x10 button click)
    // Place a 10x10 change at coordinates (900, 500)
    let mut frame_b_pixels_10x10 = vec![0u32; WIDTH * HEIGHT];
    for y in 500..510 {
        for x in 900..910 {
            frame_b_pixels_10x10[y * STRIDE_PIXELS + x] = 1;
        }
    }
    run_find_changes_bench("find_changes (10x10 change)", || {
        find_changes(&frame_a_pixels, &frame_b_pixels_10x10, WIDTH, HEIGHT, STRIDE_PIXELS).unwrap()
    });

    // Case C: Full screen refresh (100% changed)
    let frame_b_pixels_100 = vec![1u32; WIDTH * HEIGHT];
    run_find_changes_bench("find_changes (100% change)", || {
        find_changes(&frame_a_pixels, &frame_b_pixels_100, WIDTH, HEIGHT, STRIDE_PIXELS).unwrap()
    });

    // =========================================================================
    // 2. find_changes_u32 benchmarks (using u8 bytes)
    // =========================================================================
    let frame_a_bytes = vec![0u8; WIDTH * HEIGHT * 4];

    // Case A: 0% changes (fast-path)
    let frame_b_bytes_0 = vec![0u8; WIDTH * HEIGHT * 4];
    run_find_changes_bench("find_changes_u32 (0% change)", || {
        find_changes_u32(&frame_a_bytes, &frame_b_bytes_0, WIDTH, HEIGHT, STRIDE_BYTES).unwrap()
    });

    // Case B: Tiny local change (10x10 button click)
    // Place a 10x10 change at coordinates (900, 500)
    let mut frame_b_bytes_10x10 = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 500..510 {
        for x in 900..910 {
            let offset = (y * WIDTH + x) * 4;
            frame_b_bytes_10x10[offset] = 1;
            frame_b_bytes_10x10[offset + 1] = 1;
            frame_b_bytes_10x10[offset + 2] = 1;
            frame_b_bytes_10x10[offset + 3] = 1;
        }
    }
    find_changes_u32(&frame_a_bytes, &frame_b_bytes_10x10, WIDTH, HEIGHT, STRIDE_BYTES).unwrap(); // sanity check
    run_find_changes_bench("find_changes_u32 (10x10 change)", || {
        find_changes_u32(&frame_a_bytes, &frame_b_bytes_10x10, WIDTH, HEIGHT, STRIDE_BYTES).unwrap()
    });

    // Case C: Full screen refresh (100% changed)
    let frame_b_bytes_100 = vec![1u8; WIDTH * HEIGHT * 4];
    run_find_changes_bench("find_changes_u32 (100% change)", || {
        find_changes_u32(&frame_a_bytes, &frame_b_bytes_100, WIDTH, HEIGHT, STRIDE_BYTES).unwrap()
    });

    println!("==================================================");
    println!("Benchmark Complete!");
}
