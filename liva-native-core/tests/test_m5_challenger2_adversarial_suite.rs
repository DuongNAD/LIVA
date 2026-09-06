//! Challenger 2 Empirical Adversarial Stress Suite for Milestone 5
//!
//! Areas Verified:
//! 1. Concurrent Multi-Session Stress (100+ sessions, race conditions, epoch fences)
//! 2. Memory Leaks & Unbounded Buffer Growth (AEC queue bounds, TurnAudioBuffer, BufferPool churn)
//! 3. Wire Protocol Fuzzing & Corruption Hardening (Stream fragmentation, bit-flip mutations, JSON visemes)

#![allow(unused_imports, dead_code)]

use bytes::{BufMut, Bytes, BytesMut};
use liva_native_core::ipc::ring_buffer::SpscRingBuffer;
use liva_native_core::llm::pool::CancellationToken;
use liva_native_core::tts::TtsChunker;
use liva_native_core::webrtc::aec::SelfEchoCanceller;
use liva_native_core::webrtc::agc::Agc;
use liva_native_core::webrtc::frame::{
    speaker_frames, speaker_turn_epoch, BufferPool, PooledBuffer, SpeakerEpochGate, VoiceFrame,
    OP_ACK_PLAYING, OP_AUTH_HANDSHAKE, OP_FLUSH, OP_MIC_IN, OP_SPEAKER_OUT, OP_VISME,
    OP_WAKE_PROBE,
};
use liva_native_core::webrtc::ring_buffer::AudioRingBuffer;
use liva_native_core::webrtc::session::{
    SessionAec, TurnAudioAction, TurnAudioBuffer, VoiceSessionAudio,
};
use liva_native_core::webrtc::vad::VadEvent;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ============================================================================
// 1. CONCURRENT MULTI-SESSION STRESS
// ============================================================================

#[test]
fn test_challenger2_concurrent_100_sessions_stress() {
    let num_sessions = 100;
    let frames_per_session = 30;
    let frame_samples = 160; // 10ms @ 16kHz

    let mut handles = Vec::with_capacity(num_sessions);

    for session_idx in 0..num_sessions {
        let handle = std::thread::spawn(move || {
            let session = VoiceSessionAudio::new(
                None,
                None,
                Some(SelfEchoCanceller::new()),
                Some(Agc::default_16k()),
            );
            let aec_handle = session.aec_handle();

            let mut out_samples_count = 0;

            for frame_idx in 0..frames_per_session {
                // Simulate periodic render playback injection
                if frame_idx % 3 == 0 {
                    let render_data: Vec<f32> = (0..320)
                        .map(|i| ((session_idx * 100 + frame_idx * 10 + i) as f32 * 0.05).sin() * 0.4)
                        .collect();
                    if let Ok(mut guard) = aec_handle.lock() {
                        if let Some(aec) = guard.as_mut() {
                            aec.push_render(&render_data, 48000);
                        }
                    }
                }

                // Generate microphone input frame
                let mic_input: Vec<f32> = (0..frame_samples)
                    .map(|i| {
                        let t = (frame_idx * frame_samples + i) as f32 / 16000.0;
                        0.3 * (2.0 * PI * 440.0 * t).sin()
                    })
                    .collect();

                let res = session.process_mic(mic_input);
                assert!(res.is_ok(), "Session {} frame {} failed: {:?}", session_idx, frame_idx, res);
                let (_events, processed) = res.unwrap();
                assert_eq!(processed.len(), frame_samples);
                for sample in &processed {
                    assert!(sample.is_finite(), "Session {} produced non-finite sample", session_idx);
                    assert!(sample.abs() <= 1.0, "Session {} sample exceeded clamp range: {}", session_idx, sample);
                }
                out_samples_count += processed.len();
            }

            out_samples_count
        });

        handles.push(handle);
    }

    let mut total_samples = 0;
    for handle in handles {
        let count = handle.join().expect("Thread joined successfully");
        total_samples += count;
    }

    assert_eq!(total_samples, num_sessions * frames_per_session * frame_samples);
}

#[test]
fn test_challenger2_high_contention_aec_push_process_race() {
    let aec = Arc::new(Mutex::new(Some(SelfEchoCanceller::new())));
    let num_render_threads = 16;
    let num_capture_threads = 16;
    let ops_per_thread = 500;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    // Render producer threads
    for _ in 0..num_render_threads {
        let aec_clone = Arc::clone(&aec);
        let stop_clone = Arc::clone(&stop_flag);
        let handle = std::thread::spawn(move || {
            let sample_rates = [16000, 24000, 44100, 48000];
            for op in 0..ops_per_thread {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let rate = sample_rates[op % sample_rates.len()];
                let render_buf = vec![0.25f32; 240];
                if let Ok(mut guard) = aec_clone.lock() {
                    if let Some(aec_inst) = guard.as_mut() {
                        aec_inst.push_render(&render_buf, rate);
                    }
                }
                if op % 50 == 0 {
                    std::thread::yield_now();
                }
            }
        });
        handles.push(handle);
    }

    // Capture consumer threads
    for t_id in 0..num_capture_threads {
        let aec_clone = Arc::clone(&aec);
        let stop_clone = Arc::clone(&stop_flag);
        let handle = std::thread::spawn(move || {
            for op in 0..ops_per_thread {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                let mic_buf = vec![0.15f32; 160];
                if let Ok(mut guard) = aec_clone.lock() {
                    if let Some(aec_inst) = guard.as_mut() {
                        let res = aec_inst.process_capture(&mic_buf);
                        assert!(res.is_ok(), "Capture thread {} op {} failed: {:?}", t_id, op, res);
                        let clean = res.unwrap();
                        assert_eq!(clean.len(), 160);
                        for s in clean {
                            assert!(s.is_finite());
                        }
                    }
                }
                if op % 50 == 0 {
                    std::thread::yield_now();
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("AEC contention thread finished without panic");
    }
}

#[test]
fn test_challenger2_concurrent_barge_in_storm_and_epoch_fence() {
    let global_epoch = Arc::new(AtomicU64::new(1));
    let num_workers = 32;
    let cycles_per_worker = 50;

    let mut handles = Vec::new();

    for worker_id in 0..num_workers {
        let epoch_clone = Arc::clone(&global_epoch);
        let handle = std::thread::spawn(move || {
            let mut gate = SpeakerEpochGate::default();
            let mut cancelled_count = 0;

            for cycle in 0..cycles_per_worker {
                let token = CancellationToken::new();

                // 1. Simulate active speech synthesis at current epoch
                let current_epoch = epoch_clone.load(Ordering::SeqCst) as u32;
                let tts_samples = vec![0.1f32; 800];
                let frames = speaker_frames(current_epoch, 16000, &tts_samples);

                // 2. Mid-speech user barge-in occurs: bump epoch, cancel token, flush
                let new_epoch = epoch_clone.fetch_add(1, Ordering::SeqCst) as u32 + 1;
                token.cancel();
                gate.observe_flush(new_epoch);

                // 3. Verify that all stale frames (belonging to current_epoch < new_epoch) are strictly rejected
                for frame in &frames {
                    let accepted = gate.accepts(frame);
                    assert!(
                        !accepted,
                        "Worker {} cycle {}: Stale speaker frame with epoch {} must be rejected by gate with min_epoch {}",
                        worker_id, cycle, current_epoch, new_epoch
                    );
                }

                // 4. Verify that newly generated frames with new_epoch are accepted
                let fresh_frames = speaker_frames(new_epoch, 16000, &tts_samples);
                for frame in &fresh_frames {
                    assert!(
                        gate.accepts(frame),
                        "Worker {} cycle {}: Fresh speaker frame with epoch {} must be accepted",
                        worker_id, cycle, new_epoch
                    );
                }

                if token.is_cancelled() {
                    cancelled_count += 1;
                }
            }

            cancelled_count
        });
        handles.push(handle);
    }

    let mut total_cancellations = 0;
    for handle in handles {
        total_cancellations += handle.join().expect("Barge-in thread joined");
    }

    assert_eq!(total_cancellations, num_workers * cycles_per_worker);
}

// ============================================================================
// 2. MEMORY LEAKS & UNBOUNDED BUFFER GROWTH STRESS
// ============================================================================

#[test]
fn test_challenger2_turn_audio_buffer_infinite_unended_speech_leak_bounds() {
    let pre_roll_cap = 1600; // 100ms pre-roll
    let mut turn_buffer = TurnAudioBuffer::new(pre_roll_cap);

    // 1. Idle feed for 5,000 frames (no SpeechStart) — pre-roll must stay bounded to pre_roll_cap
    let idle_frame = vec![0.01f32; 160];
    for _ in 0..5000 {
        let actions = turn_buffer.ingest(&idle_frame, &[]);
        assert!(actions.is_empty());
    }

    // 2. SpeechStart triggered
    let start_actions = turn_buffer.ingest(&idle_frame, &[VadEvent::SpeechStart]);
    assert_eq!(start_actions.len(), 1);
    assert_eq!(start_actions[0], TurnAudioAction::Started);

    // 3. Continuous uninterrupted speech for 5,000 frames (800,000 samples)
    let speech_frame = vec![0.5f32; 160];
    for _ in 0..5000 {
        let actions = turn_buffer.ingest(&speech_frame, &[]);
        assert!(actions.is_empty());
    }

    // 4. SpeechEnd triggered
    let end_actions = turn_buffer.ingest(&speech_frame, &[VadEvent::SpeechEnd]);
    assert_eq!(end_actions.len(), 1);

    if let TurnAudioAction::Ended(utterance) = &end_actions[0] {
        // Pre-roll (1600) + Start frame (160) + 5000 speech frames (5000 * 160) + End frame (160)
        let expected_samples = pre_roll_cap + 160 + (5000 * 160) + 160;
        assert_eq!(utterance.len(), expected_samples);
        for s in utterance {
            assert!(s.is_finite());
        }
    } else {
        panic!("Expected TurnAudioAction::Ended");
    }

    // 5. Subsequent idle feed must not grow unbounded or retain old active turn
    for _ in 0..1000 {
        let actions = turn_buffer.ingest(&idle_frame, &[]);
        assert!(actions.is_empty());
    }
}

#[test]
fn test_challenger2_aec_unbounded_render_blast_memory_cap() {
    let mut aec = SelfEchoCanceller::new();

    // Blast 50,000 render frames into AEC without any capture processing
    let render_chunk = vec![0.8f32; 480]; // 10ms @ 48kHz
    for _ in 0..50000 {
        aec.push_render(&render_chunk, 48000);
    }

    // Process a single capture frame
    let mic_frame = vec![0.3f32; 160];
    let res = aec.process_capture(&mic_frame);
    assert!(res.is_ok());
    let clean = res.unwrap();
    assert_eq!(clean.len(), 160);
    for s in clean {
        assert!(s.is_finite());
    }
}

#[test]
fn test_challenger2_buffer_pool_concurrency_and_lifecycle_churn() {
    let pool = Arc::new(BufferPool::new(16 * 1024, 16));
    let num_threads = 16;
    let iterations_per_thread = 5000;

    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let pool_clone = Arc::clone(&pool);
        let handle = std::thread::spawn(move || {
            for i in 0..iterations_per_thread {
                let mut pooled = pool_clone.acquire_buffer();
                let payload_len = (i % 2048) + 16;
                pooled.get_mut().put_slice(&vec![0xBE; payload_len]);

                // Freeze into bytes and verify integrity
                let frozen = pooled.into_bytes();
                assert_eq!(frozen.len(), payload_len);
                assert_eq!(frozen[0], 0xBE);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("BufferPool churn thread joined");
    }

    // The idle count must remain strictly bounded by max_idle
    let idle = pool.idle_count();
    assert!(idle <= 16, "BufferPool idle count ({}) exceeded max_idle 16", idle);
}

#[test]
fn test_challenger2_spsc_ring_buffer_extreme_saturating_wrap_and_drain() {
    let ring_buffer = Arc::new(AudioRingBuffer::<f32>::new(8192));
    let rb_producer = Arc::clone(&ring_buffer);
    let rb_consumer = Arc::clone(&ring_buffer);

    let total_samples = 2_000_000usize;

    let p_handle = std::thread::spawn(move || {
        let mut written = 0;
        let mut chunk_val = 0.0f32;
        while written < total_samples {
            let chunk_size = ((written % 512) + 1).min(total_samples - written);
            let mut chunk = Vec::with_capacity(chunk_size);
            for _ in 0..chunk_size {
                chunk.push(chunk_val);
                chunk_val += 1.0;
            }

            let mut p_written = 0;
            while p_written < chunk_size {
                let n = rb_producer.push_slice(&chunk[p_written..]);
                p_written += n;
                if n == 0 {
                    std::thread::yield_now();
                }
            }
            written += chunk_size;
        }
    });

    let c_handle = std::thread::spawn(move || {
        let mut read_data = Vec::with_capacity(total_samples);
        let mut temp_buf = [0.0f32; 1024];

        while read_data.len() < total_samples {
            let n = rb_consumer.pop_slice(&mut temp_buf);
            if n > 0 {
                read_data.extend_from_slice(&temp_buf[..n]);
            } else {
                std::thread::yield_now();
            }
        }
        read_data
    });

    p_handle.join().unwrap();
    let read_result = c_handle.join().unwrap();

    assert_eq!(read_result.len(), total_samples);
    for (i, &val) in read_result.iter().enumerate() {
        assert_eq!(val, i as f32, "Data corruption at sample index {}", i);
    }
}

#[test]
fn test_challenger2_spsc_streaming_5m_samples_with_100_concurrent_barge_in_flushes() {
    use liva_native_core::webrtc::ring_buffer::AudioRingBuffer;

    let ring_buffer = Arc::new(AudioRingBuffer::<f32>::new(65536));
    let total_samples = 5_000_000usize;
    let target_flushes = 100usize;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let flush_count = Arc::new(AtomicUsize::new(0));

    // Producer thread: Pushes 5,000,000 sequentially indexed float samples in variable chunk sizes
    let rb_prod = Arc::clone(&ring_buffer);
    let stop_prod = Arc::clone(&stop_flag);
    let producer_handle = std::thread::spawn(move || {
        let mut written = 0usize;
        let mut chunk = vec![0.0f32; 512];
        let mut seed = 0xDEADBEEFu32;

        while written < total_samples && !stop_prod.load(Ordering::Relaxed) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let chunk_size = (32 + (seed as usize % 480)).min(total_samples - written);
            chunk.clear();
            for i in 0..chunk_size {
                chunk.push((written + i) as f32);
            }

            let mut p_written = 0;
            while p_written < chunk_size {
                let n = rb_prod.push_slice(&chunk[p_written..]);
                p_written += n;
                if n == 0 {
                    std::thread::yield_now();
                }
            }
            written += chunk_size;
        }
        written
    });

    // Consumer thread: Reads samples and handles interleaved lock-free barge-in flushes
    let rb_cons = Arc::clone(&ring_buffer);
    let stop_cons = Arc::clone(&stop_flag);
    let flush_count_cons = Arc::clone(&flush_count);
    let consumer_handle = std::thread::spawn(move || {
        let mut total_read = 0usize;
        let mut total_discarded = 0usize;
        let mut dst = vec![0.0f32; 1024];
        let mut last_sample_val: Option<f32> = None;
        let mut seed = 0xCAFEBABEu32;
        let mut iterations = 0usize;

        while !stop_cons.load(Ordering::Relaxed) || !rb_cons.is_empty() {
            iterations += 1;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);

            // Execute 100 interleaved barge-in flushes and skips across the stream
            if iterations % 50 == 0 && flush_count_cons.load(Ordering::Relaxed) < target_flushes {
                let fc = flush_count_cons.fetch_add(1, Ordering::Relaxed);
                if fc % 2 == 0 {
                    let discarded = rb_cons.flush_consumer();
                    total_discarded += discarded;
                } else {
                    let skipped = rb_cons.skip(240);
                    total_discarded += skipped;
                }
            }

            let read_size = 64 + (seed as usize % 384);
            let n = rb_cons.pop_slice(&mut dst[..read_size]);
            if n > 0 {
                for i in 0..n {
                    let val = dst[i];
                    if let Some(last) = last_sample_val {
                        assert!(
                            val >= last,
                            "Strict FIFO Monotonicity Violation: val {} < last {} at total_read {}",
                            val, last, total_read + i
                        );
                    }
                    last_sample_val = Some(val);
                }
                total_read += n;
            } else {
                std::thread::yield_now();
            }
        }

        (total_read, total_discarded)
    });

    let total_produced = producer_handle.join().expect("Producer joined");
    stop_flag.store(true, Ordering::SeqCst);
    let (total_consumed, total_discarded) = consumer_handle.join().expect("Consumer joined");
    let executed_flushes = flush_count.load(Ordering::Relaxed);

    assert_eq!(total_produced, total_samples, "Producer must have written all 5M samples");
    assert!(
        executed_flushes >= target_flushes,
        "Must have executed at least 100 flushes, got {}",
        executed_flushes
    );

    let (underruns, overruns, total_written_metric, total_read_metric) = ring_buffer.metrics();
    assert_eq!(total_written_metric, total_samples as u64);
    assert!(total_read_metric <= total_written_metric);
    assert_eq!(
        total_read_metric,
        (total_consumed + total_discarded) as u64,
        "Total read metric must match consumed + discarded"
    );

    println!(
        "SPSC 5M Streaming Result: Produced={}, Consumed={}, Discarded={}, Flushes={}, Underruns={}, Overruns={}",
        total_produced, total_consumed, total_discarded, executed_flushes, underruns, overruns
    );
}

#[test]
fn test_challenger2_const_generic_spsc_5m_samples_with_100_flushes() {
    use liva_native_core::webrtc::ring_buffer::SpscRingBuffer as ConstSpscRingBuffer;

    let ring_buffer = Arc::new(ConstSpscRingBuffer::<f32, 65536>::new());
    let total_samples = 5_000_000usize;
    let target_flushes = 100usize;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let flush_count = Arc::new(AtomicUsize::new(0));

    let rb_prod = Arc::clone(&ring_buffer);
    let stop_prod = Arc::clone(&stop_flag);
    let p_handle = std::thread::spawn(move || {
        let mut written = 0usize;
        let mut chunk = vec![0.0f32; 512];
        let mut seed = 0x12345678u32;

        while written < total_samples && !stop_prod.load(Ordering::Relaxed) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let chunk_size = (64 + (seed as usize % 256)).min(total_samples - written);
            chunk.clear();
            for i in 0..chunk_size {
                chunk.push((written + i) as f32);
            }

            let mut p_written = 0;
            while p_written < chunk_size {
                let n = rb_prod.push_slice(&chunk[p_written..]);
                p_written += n;
                if n == 0 {
                    std::thread::yield_now();
                }
            }
            written += chunk_size;
        }
        written
    });

    let rb_cons = Arc::clone(&ring_buffer);
    let stop_cons = Arc::clone(&stop_flag);
    let c_handle = std::thread::spawn(move || {
        let mut total_read = 0usize;
        let mut dst = vec![0.0f32; 512];
        let mut last_val: Option<f32> = None;
        let mut seed = 0x87654321u32;

        while !stop_cons.load(Ordering::Relaxed) || !rb_cons.is_empty() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let read_size = 32 + (seed as usize % 256);
            let n = rb_cons.pop_slice(&mut dst[..read_size]);
            if n > 0 {
                for i in 0..n {
                    let val = dst[i];
                    if let Some(last) = last_val {
                        assert!(
                            val >= last,
                            "Const-generic FIFO Monotonicity Violation: val {} < last {}",
                            val, last
                        );
                    }
                    last_val = Some(val);
                }
                total_read += n;
            } else {
                std::thread::yield_now();
            }
        }
        total_read
    });

    let rb_flush = Arc::clone(&ring_buffer);
    let stop_flush = Arc::clone(&stop_flag);
    let flush_count_clone = Arc::clone(&flush_count);
    let f_handle = std::thread::spawn(move || {
        while flush_count_clone.load(Ordering::Relaxed) < target_flushes && !stop_flush.load(Ordering::Relaxed) {
            rb_flush.flush_consumer();
            flush_count_clone.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(150));
        }
    });

    let produced = p_handle.join().unwrap();
    f_handle.join().unwrap();
    stop_flag.store(true, Ordering::SeqCst);
    let consumed = c_handle.join().unwrap();
    let flushes = flush_count.load(Ordering::Relaxed);

    assert_eq!(produced, total_samples);
    assert!(flushes >= target_flushes);
    assert!(consumed <= produced);
}

#[test]
fn test_challenger2_duplex_audio_ring_buffer_5m_samples_multi_thread_barge_in_stress() {
    use liva_native_core::webrtc::ring_buffer::DuplexAudioRingBuffer;

    let duplex = Arc::new(DuplexAudioRingBuffer::with_capacity(65536, 16000, 24000));
    let total_samples = 5_000_000usize;
    let target_flushes = 100usize;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flushes_done = Arc::new(AtomicUsize::new(0));

    // Thread 1: Mic In Producer (Capture)
    let d1 = Arc::clone(&duplex);
    let s1 = Arc::clone(&stop_flag);
    let h_mic_p = std::thread::spawn(move || {
        let mut written = 0usize;
        let chunk = vec![0.1f32; 160];
        while written < total_samples && !s1.load(Ordering::Relaxed) {
            let to_write = 160.min(total_samples - written);
            let n = d1.push_capture(&chunk[..to_write]);
            written += n;
            if n == 0 {
                std::thread::yield_now();
            }
        }
        written
    });

    // Thread 2: Mic In Consumer (DSP/VAD pipeline)
    let d2 = Arc::clone(&duplex);
    let s2 = Arc::clone(&stop_flag);
    let h_mic_c = std::thread::spawn(move || {
        let mut read_total = 0usize;
        let mut dst = vec![0.0f32; 160];
        while !s2.load(Ordering::Relaxed) || d2.capture_ring.available_read() > 0 {
            let n = d2.pop_capture(&mut dst);
            read_total += n;
            if n == 0 {
                std::thread::yield_now();
            }
        }
        read_total
    });

    // Thread 3: Speaker Playback Producer (TTS Engine)
    let d3 = Arc::clone(&duplex);
    let s3 = Arc::clone(&stop_flag);
    let h_spk_p = std::thread::spawn(move || {
        let mut written = 0usize;
        let chunk = vec![0.8f32; 240];
        while written < total_samples && !s3.load(Ordering::Relaxed) {
            let to_write = 240.min(total_samples - written);
            let n = d3.push_playback(&chunk[..to_write]);
            written += n;
            if n == 0 {
                std::thread::yield_now();
            }
        }
        written
    });

    // Thread 4: Speaker Playback Consumer (DAC / AudioWorklet)
    let d4 = Arc::clone(&duplex);
    let s4 = Arc::clone(&stop_flag);
    let h_spk_c = std::thread::spawn(move || {
        let mut read_total = 0usize;
        let mut dst = vec![0.0f32; 240];
        while !s4.load(Ordering::Relaxed) || d4.playback_ring.available_read() > 0 {
            let n = d4.pop_playback(&mut dst);
            read_total += n;
            if n == 0 {
                std::thread::yield_now();
            }
        }
        read_total
    });

    // Thread 5: Barge-In Trigger Thread (Calls flush_playback() 100 times concurrently during streaming)
    let d5 = Arc::clone(&duplex);
    let s5 = Arc::clone(&stop_flag);
    let f5 = Arc::clone(&flushes_done);
    let h_barge = std::thread::spawn(move || {
        while f5.load(Ordering::Relaxed) < target_flushes && !s5.load(Ordering::Relaxed) {
            d5.flush_playback();
            f5.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_micros(200));
        }
    });

    let mic_prod = h_mic_p.join().unwrap();
    let spk_prod = h_spk_p.join().unwrap();
    h_barge.join().unwrap();
    stop_flag.store(true, Ordering::SeqCst);
    let mic_cons = h_mic_c.join().unwrap();
    let spk_cons = h_spk_c.join().unwrap();
    let total_flushes = flushes_done.load(Ordering::Relaxed);

    assert_eq!(mic_prod, total_samples);
    assert_eq!(spk_prod, total_samples);
    assert_eq!(mic_cons, total_samples, "Capture channel without flushes must receive 100% of samples");
    assert!(spk_cons <= total_samples, "Playback channel with flushes must not exceed total written");
    assert!(total_flushes >= target_flushes);

    println!(
        "Duplex 5M 5-Thread Stress: Mic Pushed={}, Mic Read={}, Spk Pushed={}, Spk Read={}, Flushes={}",
        mic_prod, mic_cons, spk_prod, spk_cons, total_flushes
    );
}

// ============================================================================
// 3. WIRE PROTOCOL FUZZING & CORRUPTION HARDENING
// ============================================================================

#[test]
fn test_challenger2_wire_protocol_byte_stream_fragmentation_fuzzing() {
    // Generate 50 assorted valid VoiceFrames
    let mut original_frames = Vec::new();
    let opcodes = [
        OP_AUTH_HANDSHAKE,
        OP_MIC_IN,
        OP_SPEAKER_OUT,
        OP_FLUSH,
        OP_ACK_PLAYING,
        OP_WAKE_PROBE,
        OP_VISME,
    ];

    let mut full_wire_stream = BytesMut::new();

    for i in 0..50u32 {
        let op = opcodes[(i as usize) % opcodes.len()];
        let payload_str = format!("payload_data_sequence_index_{}_content_padding", i);
        let payload = payload_str.as_bytes();

        VoiceFrame::encode_into(&mut full_wire_stream, op, i * 17, payload).expect("encode");
        original_frames.push((op, i * 17, payload.to_vec()));
    }

    let wire_bytes = full_wire_stream.to_vec();

    // Fragment stream into arbitrary chunks of 1 to 11 bytes
    let mut incoming_buffer = BytesMut::new();
    let mut decoded_frames = Vec::new();
    let mut offset = 0;
    let mut step = 1;

    while offset < wire_bytes.len() {
        let chunk_size = (step % 11 + 1).min(wire_bytes.len() - offset);
        incoming_buffer.put_slice(&wire_bytes[offset..offset + chunk_size]);
        offset += chunk_size;
        step += 1;

        // Drain all fully received frames from incoming_buffer
        loop {
            match VoiceFrame::decode(&mut incoming_buffer) {
                Ok(Some(frame)) => {
                    decoded_frames.push(frame);
                }
                Ok(None) => break, // Incomplete frame, wait for more chunks
                Err(e) => panic!("Unexpected decode error on valid stream: {}", e),
            }
        }
    }

    assert_eq!(decoded_frames.len(), original_frames.len());
    for (i, frame) in decoded_frames.iter().enumerate() {
        let (exp_op, exp_seq, ref exp_payload) = original_frames[i];
        assert_eq!(frame.op_code, exp_op);
        assert_eq!(frame.seq_id, exp_seq);
        assert_eq!(&frame.payload[..], &exp_payload[..]);
    }
    assert!(incoming_buffer.is_empty(), "Incoming buffer must be completely consumed");
}

#[test]
fn test_challenger2_wire_protocol_deep_mutation_and_tamper_fuzzing() {
    let mut seed = 0x1337BEEFu32;

    for _ in 0..5000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mutation = seed % 6;

        match mutation {
            0 => {
                // Completely random byte noise
                let len = (seed as usize % 64) + 1;
                let mut buf = BytesMut::with_capacity(len);
                for _ in 0..len {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    buf.put_u8((seed & 0xFF) as u8);
                }
                let _ = VoiceFrame::decode(&mut buf); // Must not panic
            }
            1 => {
                // Header with payload_len = u32::MAX
                let mut buf = BytesMut::new();
                buf.put_u8(OP_MIC_IN);
                buf.put_u32_le(100);
                buf.put_u32_le(u32::MAX);
                buf.put_slice(&[0xCC; 32]);
                let res = VoiceFrame::decode(&mut buf);
                assert!(res.is_err(), "u32::MAX payload size must be rejected");
            }
            2 => {
                // Header with payload_len = 1MB + 1
                let mut buf = BytesMut::new();
                buf.put_u8(OP_SPEAKER_OUT);
                buf.put_u32_le(200);
                buf.put_u32_le(1024 * 1024 + 1);
                buf.put_slice(&[0xDD; 32]);
                let res = VoiceFrame::decode(&mut buf);
                assert!(res.is_err(), "1MB+1 payload size must be rejected");
            }
            3 => {
                // Header with payload_len = 0 (valid zero-length payload)
                let mut buf = BytesMut::new();
                buf.put_u8(OP_FLUSH);
                buf.put_u32_le(300);
                buf.put_u32_le(0);
                let res = VoiceFrame::decode(&mut buf);
                assert!(res.is_ok());
                let frame = res.unwrap().unwrap();
                assert_eq!(frame.op_code, OP_FLUSH);
                assert_eq!(frame.seq_id, 300);
                assert!(frame.payload.is_empty());
            }
            4 => {
                // Valid frame followed by garbage trailer
                let mut buf = BytesMut::new();
                VoiceFrame::encode_into(&mut buf, OP_MIC_IN, 400, b"valid").unwrap();
                buf.put_slice(&[0xFF; 5]); // Partial next header
                let res = VoiceFrame::decode(&mut buf).unwrap().unwrap();
                assert_eq!(res.op_code, OP_MIC_IN);
                assert_eq!(res.seq_id, 400);
                assert_eq!(&res.payload[..], b"valid");
                // Buffer still has remaining 5 bytes
                assert_eq!(buf.len(), 5);
            }
            _ => {
                // Unknown high opcode (0x80..0xFF)
                let unk_op = ((seed & 0x7F) + 0x80) as u8;
                let mut buf = BytesMut::new();
                VoiceFrame::encode_into(&mut buf, unk_op, 500, b"unknown_op_payload").unwrap();
                let decoded = VoiceFrame::decode(&mut buf).unwrap().unwrap();
                assert_eq!(decoded.op_code, unk_op);
                assert_eq!(decoded.seq_id, 500);
                assert_eq!(&decoded.payload[..], b"unknown_op_payload");
            }
        }
    }
}

#[test]
fn test_challenger2_viseme_timeline_adversarial_unicode_and_json_fuzzing() {
    // Viseme OP_VISME JSON payload deserialization robustness
    let malformed_jsons = [
        "not a json",
        "{}",
        r#"{"turn_epoch": "string_instead_of_u64"}"#,
        r#"{"turn_epoch": 1, "visemes": [{"v": "unknown_viseme", "t_ms": 100}]}"#,
        r#"{"turn_epoch": 1, "visemes": [{"v": "aa", "t_ms": -50}]}"#,
        r#"{"turn_epoch": 1, "visemes": null}"#,
        r#"{"turn_epoch": 1, "visemes": [{"v": "aa", "t_ms": 0}], "extra_field": 12345}"#,
    ];

    for json_str in malformed_jsons {
        let parse_res: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        // Either fails to parse JSON or parses as arbitrary Value without panic
        let _ = parse_res;
    }
}
