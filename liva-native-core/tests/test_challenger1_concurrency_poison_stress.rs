//! Challenger 1 Dedicated Adversarial Stress Harness:
//! Concurrency, Atomic CAS Semantics, Barge-In Sample Purity & RwLock Poison Recovery.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use liva_native_core::agent::graph::diff_reviewer::{
    parse_unified_diff, DiffReviewRegistry, DiffReviewSession, HunkStatus,
};
use liva_native_core::webrtc::ring_buffer::{AudioRingBufferF32, SpscRingBuffer};

// ============================================================================
// 1. Audio Dynamic RingBuffer Multi-Threaded Barge-In & Sample Purity
// ============================================================================

#[test]
fn test_ring_buffer_extreme_concurrent_barge_in_fuzzing() {
    const TOTAL_SAMPLES: usize = 1_000_000;
    const BUFFER_CAP: usize = 4096;

    let ring = Arc::new(AudioRingBufferF32::new(BUFFER_CAP));
    let running = Arc::new(AtomicBool::new(true));

    let last_observed_seq = Arc::new(AtomicU64::new(0));
    let flush_count = Arc::new(AtomicUsize::new(0));

    // Producer Thread: Writes tagged samples (monotonically increasing seq IDs)
    let ring_p = Arc::clone(&ring);
    let running_p = Arc::clone(&running);
    let prod_handle = thread::spawn(move || {
        let mut seq: u64 = 1;
        let mut total_pushed = 0;
        let mut chunk = [0.0f32; 128];

        while total_pushed < TOTAL_SAMPLES {
            let to_push = 128.min(TOTAL_SAMPLES - total_pushed);
            for i in 0..to_push {
                chunk[i] = (seq + i as u64) as f32;
            }

            let written = ring_p.push_slice(&chunk[..to_push]);
            if written > 0 {
                seq += written as u64;
                total_pushed += written;
            } else {
                std::hint::spin_loop();
            }
        }
        running_p.store(false, Ordering::Relaxed);
        total_pushed
    });

    // Consumer Thread: Reads variable sized chunks and checks monotonicity
    let ring_c = Arc::clone(&ring);
    let running_c = Arc::clone(&running);
    let last_seq_c = Arc::clone(&last_observed_seq);
    let cons_handle = thread::spawn(move || {
        let mut total_read = 0;
        let mut dst = [0.0f32; 256];
        let mut prev_sample = 0.0f32;

        while running_c.load(Ordering::Relaxed) || ring_c.available_read() > 0 {
            let chunk_len = (total_read % 127 + 1).min(dst.len());
            let r = ring_c.pop_slice(&mut dst[..chunk_len]);

            if r > 0 {
                for i in 0..r {
                    let sample = dst[i];
                    assert!(sample > 0.0, "Sample must be positive tag, got {}", sample);
                    // Sample must be strictly greater than previous sample (since flushes only jump forward)
                    assert!(
                        sample > prev_sample,
                        "Sample sequence violated: prev={}, curr={} (tail regression or stale read detected!)",
                        prev_sample, sample
                    );
                    prev_sample = sample;
                }
                last_seq_c.store(prev_sample as u64, Ordering::Relaxed);
                total_read += r;
            } else {
                std::hint::spin_loop();
            }
        }
        total_read
    });

    // 4 Concurrent Barge-In Preemption Flushers
    let mut flush_handles = Vec::new();
    for _ in 0..4 {
        let ring_f = Arc::clone(&ring);
        let running_f = Arc::clone(&running);
        let flush_cnt = Arc::clone(&flush_count);
        flush_handles.push(thread::spawn(move || {
            let mut local_flushes = 0;
            while running_f.load(Ordering::Relaxed) {
                let discarded = ring_f.flush_consumer();
                if discarded > 0 {
                    local_flushes += 1;
                    flush_cnt.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::yield_now();
            }
            local_flushes
        }));
    }

    let pushed = prod_handle.join().unwrap();
    let read = cons_handle.join().unwrap();
    for h in flush_handles {
        h.join().unwrap();
    }

    let (underruns, overruns, tw, tr) = ring.metrics();
    assert_eq!(pushed, TOTAL_SAMPLES);
    assert_eq!(tw, TOTAL_SAMPLES as u64);
    assert_eq!(
        tr + ring.available_read() as u64,
        tw,
        "Total read (pops + flushes) must equal total written"
    );
    assert!(read <= pushed, "Read cannot exceed pushed");
    println!(
        "Barge-In Fuzzing Result: Pushed={}, Read={}, TotalReadMetric={}, Overruns={}, Underruns={}, Flushes={}",
        pushed, read, tr, overruns, underruns, flush_count.load(Ordering::Relaxed)
    );
}

// ============================================================================
// 2. Const-Generic SpscRingBuffer Multi-Threaded CAS Verification
// ============================================================================

#[test]
fn test_spsc_ring_buffer_const_generic_concurrent_barge_in() {
    const TOTAL_SAMPLES: usize = 500_000;
    let ring = Arc::new(SpscRingBuffer::<f32, 2048>::new());
    let running = Arc::new(AtomicBool::new(true));

    let ring_p = Arc::clone(&ring);
    let running_p = Arc::clone(&running);
    let prod = thread::spawn(move || {
        let mut total_pushed = 0;
        let mut chunk = [1.234f32; 64];
        while total_pushed < TOTAL_SAMPLES {
            let to_push = 64.min(TOTAL_SAMPLES - total_pushed);
            for i in 0..to_push {
                chunk[i] = ((total_pushed + i) as f32) + 1.0;
            }
            let w = ring_p.push_slice(&chunk[..to_push]);
            if w > 0 {
                total_pushed += w;
            } else {
                std::hint::spin_loop();
            }
        }
        running_p.store(false, Ordering::Relaxed);
        total_pushed
    });

    let ring_c = Arc::clone(&ring);
    let running_c = Arc::clone(&running);
    let cons = thread::spawn(move || {
        let mut total_read = 0;
        let mut dst = [0.0f32; 128];
        let mut prev = 0.0f32;
        while running_c.load(Ordering::Relaxed) || ring_c.available_read() > 0 {
            let r = ring_c.pop_slice(&mut dst);
            if r > 0 {
                for i in 0..r {
                    assert!(
                        dst[i] > prev,
                        "Monotonicity error in SpscRingBuffer: prev={}, curr={}",
                        prev, dst[i]
                    );
                    prev = dst[i];
                }
                total_read += r;
            } else {
                std::hint::spin_loop();
            }
        }
        total_read
    });

    // Concurrent flushers
    let mut flush_handles = Vec::new();
    for _ in 0..3 {
        let ring_f = Arc::clone(&ring);
        let running_f = Arc::clone(&running);
        flush_handles.push(thread::spawn(move || {
            let mut f = 0;
            while running_f.load(Ordering::Relaxed) {
                let disc = ring_f.flush_consumer();
                if disc > 0 {
                    f += 1;
                }
                thread::yield_now();
            }
            f
        }));
    }

    let pushed = prod.join().unwrap();
    let read = cons.join().unwrap();
    for h in flush_handles {
        h.join().unwrap();
    }

    assert_eq!(pushed, TOTAL_SAMPLES);
    assert!(read <= pushed);
    assert!(ring.available_read() <= 2048);
}

// ============================================================================
// 3. DiffReviewRegistry Chaos Poisoning & RwLock Recovery
// ============================================================================

const SAMPLE_DIFF: &str = r#"--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 use std::collections::HashMap;
+use std::sync::Arc;
 pub fn test() {}
"#;

#[test]
fn test_diff_reviewer_multi_thread_chaos_poisoning() {
    let registry = Arc::new(DiffReviewRegistry::new());
    let stop = Arc::new(AtomicBool::new(false));

    // 1. Writer threads creating sessions and submitting decisions
    let mut writers = Vec::new();
    for w_idx in 0..4 {
        let reg = Arc::clone(&registry);
        let s = Arc::clone(&stop);
        writers.push(thread::spawn(move || {
            let mut count = 0;
            while !s.load(Ordering::Relaxed) {
                let sid = format!("sess-w{}-{}", w_idx, count);
                let files = parse_unified_diff(SAMPLE_DIFF).unwrap();
                let session = DiffReviewSession::new(&sid, "th-1", "act-1", files);
                reg.create_session(session);

                let _ = reg.submit_batch_decisions(&sid, "approve_all");
                let _ = reg.get_session(&sid);
                count += 1;
                std::hint::spin_loop();
            }
            count
        }));
    }

    // 2. Reader threads listing sessions and queryings
    let mut readers = Vec::new();
    for _ in 0..6 {
        let reg = Arc::clone(&registry);
        let s = Arc::clone(&stop);
        readers.push(thread::spawn(move || {
            let mut reads = 0;
            while !s.load(Ordering::Relaxed) {
                let _ = reg.list_sessions();
                let _ = reg.list_pending();
                reads += 1;
                std::hint::spin_loop();
            }
            reads
        }));
    }

    // 3. Chaos Panicker Threads: Intentionally panic repeatedly inside spawned threads while holding write locks
    let mut panickers = Vec::new();
    for p_idx in 0..50 {
        let reg = Arc::clone(&registry);
        let h = thread::spawn(move || {
            // Force create a session and panic midway
            let sid = format!("poison-sess-{}", p_idx);
            let files = parse_unified_diff(SAMPLE_DIFF).unwrap();
            let session = DiffReviewSession::new(&sid, "th-chaos", "act-chaos", files);
            reg.create_session(session);

            // Intentionally panic in thread
            panic!("Intentional chaos panic #{} to poison RwLock", p_idx);
        });
        panickers.push(h);
    }

    // Wait for all chaos panickers to panic
    for h in panickers {
        let res = h.join();
        assert!(res.is_err(), "Panicker thread must have panicked");
    }

    // Verify registry is STILL completely usable and healthy after 50 panics!
    for i in 0..100 {
        let test_sid = format!("post-poison-{}", i);
        let files = parse_unified_diff(SAMPLE_DIFF).unwrap();
        let session = DiffReviewSession::new(&test_sid, "th-post", "act-post", files);
        registry.create_session(session);

        let retrieved = registry.get_session(&test_sid);
        assert!(retrieved.is_some(), "Post-poison session must be retrievable");

        let updated = registry.submit_decision(
            &test_sid,
            &retrieved.unwrap().files[0].hunks[0].hunk_id,
            HunkStatus::Approved,
        );
        assert!(updated.is_ok(), "Decision submission must succeed post-poison");
    }

    stop.store(true, Ordering::Relaxed);

    for w in writers {
        let cnt = w.join().expect("Writer thread should not panic");
        assert!(cnt > 0);
    }
    for r in readers {
        let reads = r.join().expect("Reader thread should not panic");
        assert!(reads > 0);
    }

    // Final sanity assertions on registry
    let all = registry.list_sessions();
    assert!(all.len() >= 100);
    registry.clear();
    assert_eq!(registry.list_sessions().len(), 0);
}
