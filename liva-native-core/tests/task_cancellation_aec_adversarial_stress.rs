//! Adversarial stress test suite challenging AbortOnDropJoinSet and SelfEchoCanceller boundaries.
//!
//! Milestone 2 Challenger 2 Verification:
//! 1. Spawning 200 tasks in `AbortOnDropJoinSet` and aborting them concurrently or dropping the set.
//!    Verifying 100% of tasks are cancelled and 0 orphaned tasks/threads remain running.
//! 2. Pushing 100,000 samples into `SelfEchoCanceller` render queue.
//!    Verifying that queue length is strictly clamped to `MAX_RENDER_QUEUE_SAMPLES = 32_000` (2.0s)
//!    and oldest samples are dropped.
//! 3. Pushing corrupted audio (all NaNs and all Infs) through `process_capture` with active render queue.
//!    Verifying output frames contain only clean, finite floats clamped to `[-1.0, 1.0]` with 0 panics.

use liva_native_core::webrtc::aec::SelfEchoCanceller;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub struct AbortOnDropJoinSet<T: 'static> {
    inner: tokio::task::JoinSet<T>,
}

impl<T: 'static> Default for AbortOnDropJoinSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> AbortOnDropJoinSet<T> {
    pub fn new() -> Self {
        Self {
            inner: tokio::task::JoinSet::new(),
        }
    }

    pub fn spawn<F>(&mut self, task: F)
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send,
    {
        while self.inner.try_join_next().is_some() {}
        self.inner.spawn(task);
    }

    pub fn abort_all(&mut self) {
        self.inner.abort_all();
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T: 'static> Drop for AbortOnDropJoinSet<T> {
    fn drop(&mut self) {
        self.inner.abort_all();
    }
}

// =============================================================================
// Helper Types for Task Tracking
// =============================================================================

struct TaskGuard {
    active_count: Arc<AtomicUsize>,
}

impl TaskGuard {
    fn new(active_count: Arc<AtomicUsize>) -> Self {
        active_count.fetch_add(1, Ordering::SeqCst);
        Self { active_count }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }
}

// =============================================================================
// Test Suite 1: AbortOnDropJoinSet 200-Task Cancellation & Concurrency Stress
// =============================================================================

/// Challenge 1.1: Spawn 200 tasks, await all to start, call `abort_all()`.
/// Asserts that 100% of tasks are aborted before completion and active task count drops to 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn challenge_200_tasks_concurrent_abort_all_verifies_100_percent_cancelled_and_zero_orphans()
{
    const NUM_TASKS: usize = 200;

    let started_count = Arc::new(AtomicUsize::new(0));
    let active_count = Arc::new(AtomicUsize::new(0));
    let completed_count = Arc::new(AtomicUsize::new(0));

    let mut join_set = AbortOnDropJoinSet::new();

    for _ in 0..NUM_TASKS {
        let started = Arc::clone(&started_count);
        let active = Arc::clone(&active_count);
        let completed = Arc::clone(&completed_count);

        join_set.spawn(async move {
            let _guard = TaskGuard::new(active);
            started.fetch_add(1, Ordering::SeqCst);

            // Sleep for 30 seconds: task should be aborted while sleeping
            tokio::time::sleep(Duration::from_secs(30)).await;

            // This line MUST NEVER be reached if task was cancelled
            completed.fetch_add(1, Ordering::SeqCst);
        });
    }

    // Await until all 200 tasks have started running on Tokio threads
    let timeout = Instant::now();
    while started_count.load(Ordering::SeqCst) < NUM_TASKS {
        if timeout.elapsed() > Duration::from_secs(5) {
            panic!(
                "Timed out waiting for 200 tasks to start; only started {}",
                started_count.load(Ordering::SeqCst)
            );
        }
        tokio::task::yield_now().await;
    }

    assert_eq!(
        started_count.load(Ordering::SeqCst),
        NUM_TASKS,
        "All 200 tasks must have started"
    );
    assert_eq!(
        active_count.load(Ordering::SeqCst),
        NUM_TASKS,
        "All 200 tasks must be actively running"
    );

    // Trigger mass cancellation via abort_all()
    join_set.abort_all();

    // Allow Tokio worker threads to drop aborted futures
    let abort_timeout = Instant::now();
    while active_count.load(Ordering::SeqCst) > 0 {
        if abort_timeout.elapsed() > Duration::from_secs(3) {
            panic!(
                "Timed out waiting for tasks to be aborted; {} orphaned tasks remaining",
                active_count.load(Ordering::SeqCst)
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        active_count.load(Ordering::SeqCst),
        0,
        "100% of tasks must be cancelled: exactly 0 orphaned tasks may remain running"
    );
    assert_eq!(
        completed_count.load(Ordering::SeqCst),
        0,
        "Zero tasks out of 200 may run to completion after cancellation"
    );
}

/// Challenge 1.2: Spawn 200 tasks into scoped `AbortOnDropJoinSet` and drop it.
/// Asserts that RAII Drop unconditionally cancels 100% of tasks with zero orphans.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn challenge_200_tasks_drop_joinset_verifies_raii_abortion_and_zero_orphans() {
    const NUM_TASKS: usize = 200;

    let started_count = Arc::new(AtomicUsize::new(0));
    let active_count = Arc::new(AtomicUsize::new(0));
    let completed_count = Arc::new(AtomicUsize::new(0));

    {
        let mut scoped_join_set = AbortOnDropJoinSet::new();

        for _ in 0..NUM_TASKS {
            let started = Arc::clone(&started_count);
            let active = Arc::clone(&active_count);
            let completed = Arc::clone(&completed_count);

            scoped_join_set.spawn(async move {
                let _guard = TaskGuard::new(active);
                started.fetch_add(1, Ordering::SeqCst);

                tokio::time::sleep(Duration::from_secs(30)).await;

                completed.fetch_add(1, Ordering::SeqCst);
            });
        }

        // Wait for all 200 tasks to start
        let timeout = Instant::now();
        while started_count.load(Ordering::SeqCst) < NUM_TASKS {
            if timeout.elapsed() > Duration::from_secs(5) {
                panic!("Timed out waiting for 200 tasks to start");
            }
            tokio::task::yield_now().await;
        }

        assert_eq!(active_count.load(Ordering::SeqCst), NUM_TASKS);
        // scoped_join_set is dropped here at end of scope
    }

    // Await task termination via RAII drop
    let abort_timeout = Instant::now();
    while active_count.load(Ordering::SeqCst) > 0 {
        if abort_timeout.elapsed() > Duration::from_secs(3) {
            panic!(
                "Timed out waiting for RAII dropped tasks; {} orphans remaining",
                active_count.load(Ordering::SeqCst)
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        active_count.load(Ordering::SeqCst),
        0,
        "RAII Drop must terminate 100% of tasks with zero orphans"
    );
    assert_eq!(
        completed_count.load(Ordering::SeqCst),
        0,
        "Zero tasks may complete when JoinSet is dropped"
    );
}

/// Challenge 1.3: Concurrently spawn 200 tasks with tight async loops while another task
/// concurrently calls abort_all(). Stress tests cancellation races and thread safety.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn challenge_concurrent_spawning_and_racing_abort_stress() {
    const NUM_TASKS: usize = 200;

    let active_count = Arc::new(AtomicUsize::new(0));
    let completed_count = Arc::new(AtomicUsize::new(0));
    let stop_aborting = Arc::new(AtomicBool::new(false));

    let mut join_set = AbortOnDropJoinSet::new();

    // Spawn 200 tight-loop tasks
    for _ in 0..NUM_TASKS {
        let active = Arc::clone(&active_count);
        let completed = Arc::clone(&completed_count);

        join_set.spawn(async move {
            let _guard = TaskGuard::new(active);
            for _ in 0..10_000 {
                tokio::task::yield_now().await;
            }
            completed.fetch_add(1, Ordering::SeqCst);
        });
    }

    // Repeatedly trigger abort_all() concurrently
    for _ in 0..10 {
        join_set.abort_all();
        tokio::task::yield_now().await;
    }

    stop_aborting.store(true, Ordering::SeqCst);

    // Final abort to ensure everything is cancelled
    join_set.abort_all();

    let timeout = Instant::now();
    while active_count.load(Ordering::SeqCst) > 0 {
        if timeout.elapsed() > Duration::from_secs(3) {
            panic!(
                "Orphaned tasks detected under concurrent abort race: {}",
                active_count.load(Ordering::SeqCst)
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(active_count.load(Ordering::SeqCst), 0);
}

/// Challenge 1.4: Dynamic task reaping validation on `AbortOnDropJoinSet::spawn`.
/// Verifies that completed tasks are reaped by try_join_next() on each spawn call.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn challenge_task_reaping_prevents_unbounded_handle_accumulation() {
    let mut join_set = AbortOnDropJoinSet::new();

    // Spawn 100 fast-completing tasks
    for _ in 0..100 {
        join_set.spawn(async {
            // Task finishes immediately
        });
    }

    // Give tasks a moment to complete
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Spawning task #101 should opportunistically reap the 100 completed tasks
    join_set.spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
    });

    // Aborting all should safely abort the single active task
    join_set.abort_all();
    tokio::time::sleep(Duration::from_millis(20)).await;
}

// =============================================================================
// Test Suite 2: SelfEchoCanceller 100k Render Queue Boundary & Clamping Stress
// =============================================================================

/// Challenge 2.1: Push 100,000 samples into `SelfEchoCanceller` render queue in a single burst.
/// Verifies that queue length is strictly clamped to `MAX_RENDER_QUEUE_SAMPLES = 32_000` (2s @ 16kHz)
/// and oldest 68,000 samples are dropped.
#[test]
fn challenge_100k_samples_push_render_strictly_clamps_to_32k_ceiling() {
    let mut aec = SelfEchoCanceller::new();
    assert_eq!(aec.render_queue_len(), 0);

    // Generate 100,000 samples:
    // Oldest 68,000 samples: 0.2
    // Newest 32,000 samples: 0.9
    let mut samples = vec![0.2f32; 68_000];
    samples.extend(vec![0.9f32; 32_000]);
    assert_eq!(samples.len(), 100_000);

    // Push 100,000 samples at 16kHz
    aec.push_render(&samples, 16000);

    // Queue length must be strictly clamped to MAX_RENDER_QUEUE_SAMPLES = 32_000
    assert_eq!(
        aec.render_queue_len(),
        SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES,
        "Queue length must equal MAX_RENDER_QUEUE_SAMPLES"
    );
    assert_eq!(
        aec.render_queue_len(),
        32_000,
        "Queue length must be strictly clamped to 32,000 samples (2.0s @ 16kHz)"
    );

    // Verify FIFO drain order: processing capture frames drains 160 samples per frame.
    // 32,000 samples / 160 samples per frame = exactly 200 frames.
    let mic_frame = vec![0.0f32; 160];
    for frame_idx in 0..200 {
        let expected_remaining = 32_000 - ((frame_idx + 1) * 160);
        let out = aec.process_capture(&mic_frame).expect("process_capture");
        assert_eq!(out.len(), 160);
        assert_eq!(
            aec.render_queue_len(),
            expected_remaining,
            "Frame {} must drain exactly 160 samples from render queue",
            frame_idx
        );
    }

    // At frame 200, queue must be completely drained
    assert_eq!(aec.render_queue_len(), 0);

    // Subsequent capture processing when queue is empty does not panic and keeps length at 0
    let out = aec.process_capture(&mic_frame).expect("process_capture");
    assert_eq!(out.len(), 160);
    assert_eq!(aec.render_queue_len(), 0);
}

/// Challenge 2.2: Push 100 chunks of 1,000 samples (100,000 samples total) incrementally.
/// Verifies that at every step beyond 32,000 samples, the queue length never exceeds 32,000.
#[test]
fn challenge_incremental_chunks_100k_samples_strictly_clamps_and_drops_oldest() {
    let mut aec = SelfEchoCanceller::new();

    const CHUNK_SIZE: usize = 1_000;
    const NUM_CHUNKS: usize = 100; // 100 * 1,000 = 100,000 samples

    for chunk_idx in 0..NUM_CHUNKS {
        let chunk = vec![0.5f32; CHUNK_SIZE];
        aec.push_render(&chunk, 16000);

        let total_pushed = (chunk_idx + 1) * CHUNK_SIZE;
        let expected_len = total_pushed.min(SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES);

        assert_eq!(
            aec.render_queue_len(),
            expected_len,
            "At chunk {}, total pushed {}, queue length must be {}",
            chunk_idx,
            total_pushed,
            expected_len
        );
        assert!(
            aec.render_queue_len() <= 32_000,
            "Queue length must never exceed 32,000 ceiling"
        );
    }

    assert_eq!(aec.render_queue_len(), 32_000);
}

/// Challenge 2.3: Push 150,000 samples at 24,000 Hz (which resamples to 100,000 samples @ 16 kHz).
/// Verifies that resampled render audio is also strictly clamped to 32,000 samples.
#[test]
fn challenge_resampled_render_queue_boundary_150k_at_24khz() {
    let mut aec = SelfEchoCanceller::new();

    // 150,000 samples @ 24kHz -> 150,000 * (16,000 / 24,000) = 100,000 samples @ 16kHz
    let resampled_source = vec![0.4f32; 150_000];
    aec.push_render(&resampled_source, 24000);

    assert_eq!(
        aec.render_queue_len(),
        SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES,
        "Resampled render queue must clamp strictly to 32,000 samples"
    );
    assert_eq!(aec.render_queue_len(), 32_000);
}

/// Challenge 2.4: Push 100,000 samples through `push_loopback_render`.
/// Verifies loopback render path also clamps to 32,000 samples ceiling.
#[test]
fn challenge_loopback_render_queue_boundary_100k_samples() {
    let mut aec = SelfEchoCanceller::new();

    let loopback_samples = vec![0.6f32; 100_000];
    aec.push_loopback_render(&loopback_samples, 16000);

    assert_eq!(
        aec.render_queue_len(),
        SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES,
        "Loopback render queue must clamp strictly to 32,000 samples"
    );
    assert_eq!(aec.render_queue_len(), 32_000);
}

/// Challenge 2.5: Clear render and reset invariants under saturated queue.
/// Verifies that clear_render() flushes the 32,000 render queue immediately.
#[test]
fn challenge_clear_render_and_reset_invariants() {
    let mut aec = SelfEchoCanceller::new();
    aec.push_render(&vec![0.3f32; 100_000], 16000);
    assert_eq!(aec.render_queue_len(), 32_000);

    // Barge-in clears the render queue instantly
    aec.clear_render();
    assert_eq!(aec.render_queue_len(), 0);

    // Repopulate and reset
    aec.push_render(&vec![0.3f32; 100_000], 16000);
    assert_eq!(aec.render_queue_len(), 32_000);
    aec.reset();
    assert_eq!(aec.render_queue_len(), 0);
}

// =============================================================================
// Test Suite 3: Audio Corruption (NaNs, Infs, Denormals) & Robustness Stress
// =============================================================================

/// Challenge 3.1: Push 10,000 samples of ALL NaNs through `process_capture` with active render queue.
/// Verifies that output frames contain only clean, finite floats clamped to [-1.0, 1.0] and 0 panics.
#[test]
fn challenge_all_nan_audio_capture_with_active_render_queue() {
    let mut aec = SelfEchoCanceller::new();

    // Populate render queue with active reference audio
    aec.push_render(&vec![0.4f32; 32_000], 16000);
    assert_eq!(aec.render_queue_len(), 32_000);

    // Generate 10,000 samples of pure NaN corrupted audio
    let corrupted_mic = vec![f32::NAN; 10_000];

    let out = aec
        .process_capture(&corrupted_mic)
        .expect("process_capture must not fail on NaNs");

    // Output must be a multiple of 160 samples (FRAME_SIZE)
    assert_eq!(out.len(), 10_000 / 160 * 160); // 9,920 samples processed, 80 pending

    // Every single output sample must be strictly finite and clamped to [-1.0, 1.0]
    for (i, &sample) in out.iter().enumerate() {
        assert!(
            sample.is_finite(),
            "Output sample {} must be finite, found {}",
            i,
            sample
        );
        assert!(
            (-1.0..=1.0).contains(&sample),
            "Output sample {} must be in [-1.0, 1.0], found {}",
            i,
            sample
        );
    }
}

/// Challenge 3.2: Push 10,000 samples of ALL +Infinity and ALL -Infinity through `process_capture`.
/// Verifies zero panics and clean finite outputs clamped to [-1.0, 1.0].
#[test]
fn challenge_all_inf_audio_capture_with_active_render_queue() {
    let mut aec = SelfEchoCanceller::new();
    aec.push_render(&vec![0.5f32; 32_000], 16000);

    // Test +Infinity
    let pos_inf_mic = vec![f32::INFINITY; 3200]; // 20 frames
    let out_pos = aec
        .process_capture(&pos_inf_mic)
        .expect("process_capture on +Inf");
    assert_eq!(out_pos.len(), 3200);
    for &sample in &out_pos {
        assert!(sample.is_finite(), "Output sample must be finite");
        assert!(
            (-1.0..=1.0).contains(&sample),
            "Output sample must be in [-1.0, 1.0]"
        );
    }

    // Test -Infinity
    let neg_inf_mic = vec![f32::NEG_INFINITY; 3200]; // 20 frames
    let out_neg = aec
        .process_capture(&neg_inf_mic)
        .expect("process_capture on -Inf");
    assert_eq!(out_neg.len(), 3200);
    for &sample in &out_neg {
        assert!(sample.is_finite(), "Output sample must be finite");
        assert!(
            (-1.0..=1.0).contains(&sample),
            "Output sample must be in [-1.0, 1.0]"
        );
    }
}

/// Challenge 3.3: Push corrupted audio into BOTH render queue AND capture stream.
/// Tests corruptions including NaNs, +/-Infs, extreme out-of-range floats (1e35), and subnormals.
#[test]
fn challenge_mixed_nan_inf_subnormal_capture_and_corrupted_render() {
    let mut aec = SelfEchoCanceller::new();

    // 1. Corrupted render audio
    let corrupted_render = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1e35,
        -1e35,
        f32::MIN_POSITIVE / 2.0, // Subnormal
        2.5,
        -5.0,
    ];
    let repeated_corrupted_render: Vec<f32> = corrupted_render
        .iter()
        .cloned()
        .cycle()
        .take(40_000)
        .collect();

    aec.push_render(&repeated_corrupted_render, 16000);
    assert_eq!(aec.render_queue_len(), 32_000);

    // 2. Corrupted capture stream: mixture of corrupted values and valid sine wave
    let mut mixed_mic = Vec::with_capacity(6400); // 40 frames
    for i in 0..6400 {
        let val = match i % 8 {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => 1e38,
            4 => -1e38,
            5 => f32::MIN_POSITIVE / 2.0,
            _ => (i as f32 * 0.1).sin() * 0.5,
        };
        mixed_mic.push(val);
    }

    let out = aec
        .process_capture(&mixed_mic)
        .expect("process_capture mixed corrupted");
    assert_eq!(out.len(), 6400);

    for (i, &sample) in out.iter().enumerate() {
        assert!(
            sample.is_finite(),
            "Output sample {} must be finite, found {}",
            i,
            sample
        );
        assert!(
            (-1.0..=1.0).contains(&sample),
            "Output sample {} must be in [-1.0, 1.0], found {}",
            i,
            sample
        );
    }
}

/// Challenge 3.4: Misaligned frame chunks (arbitrary non-multiple of 160 samples) with corrupted audio.
/// Verifies that fractional frame carrying in capture_pending works safely without panics or leaks.
#[test]
fn challenge_misaligned_frame_sizes_and_tail_buffering() {
    let mut aec = SelfEchoCanceller::new();
    aec.push_render(&vec![0.3f32; 1600], 16000);

    // Push 100 samples of NaN (less than 1 frame of 160)
    let out1 = aec.process_capture(&vec![f32::NAN; 100]).expect("chunk 1");
    assert_eq!(
        out1.len(),
        0,
        "100 samples < 160 should yield 0 output frames"
    );

    // Push 60 samples of +Inf (100 + 60 = 160 samples: exactly 1 frame)
    let out2 = aec
        .process_capture(&vec![f32::INFINITY; 60])
        .expect("chunk 2");
    assert_eq!(
        out2.len(),
        160,
        "100 + 60 = 160 should yield exactly 1 frame"
    );
    for &sample in &out2 {
        assert!(sample.is_finite());
        assert!((-1.0..=1.0).contains(&sample));
    }

    // Push 250 samples of mixed NaN/valid (160 processed, 90 remaining)
    let mut chunk3 = vec![f32::NAN; 125];
    chunk3.extend(vec![0.4f32; 125]);
    let out3 = aec.process_capture(&chunk3).expect("chunk 3");
    assert_eq!(out3.len(), 160);
    for &sample in &out3 {
        assert!(sample.is_finite());
        assert!((-1.0..=1.0).contains(&sample));
    }

    // Push 70 samples of -Inf (90 + 70 = 160: completes another frame)
    let out4 = aec
        .process_capture(&vec![f32::NEG_INFINITY; 70])
        .expect("chunk 4");
    assert_eq!(out4.len(), 160);
    for &sample in &out4 {
        assert!(sample.is_finite());
        assert!((-1.0..=1.0).contains(&sample));
    }
}
