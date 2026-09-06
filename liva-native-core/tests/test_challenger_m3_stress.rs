//! Adversarial Stress & Concurrency Test Suite for Milestone 3 (Audio Ring Buffer & Wire Transport)
//! Written by Challenger 2 for empirical verification.

use bytes::{Bytes, BytesMut};
use liva_native_core::webrtc::frame::{
    BufferPool, VoiceFrame, OP_MIC_IN, OP_SPEAKER_OUT,
};
use liva_native_core::webrtc::ring_buffer::{
    f32_to_i16_slice, i16_to_f32_slice, resample_linear_16k_to_24k, resample_linear_24k_to_16k,
    AudioRingBufferF32, DuplexAudioRingBuffer,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_adversarial_simultaneous_full_duplex_throughput_and_isolation() {
    let duplex = Arc::new(DuplexAudioRingBuffer::with_capacity(32768, 16000, 24000));
    let num_samples = 500_000;
    let chunk_size = 160;

    let duplex_cap_prod = Arc::clone(&duplex);
    let cap_prod = thread::spawn(move || {
        let mut sent = 0usize;
        let mut chunk = vec![0.0f32; chunk_size];
        while sent < num_samples {
            let to_send = chunk_size.min(num_samples - sent);
            for i in 0..to_send {
                // Capture channel marker: 1.0 + sample_index
                chunk[i] = 1.0 + ((sent + i) as f32 * 0.0001);
            }
            let written = duplex_cap_prod.push_capture(&chunk[..to_send]);
            sent += written;
            if written == 0 {
                thread::yield_now();
            }
        }
    });

    let duplex_cap_cons = Arc::clone(&duplex);
    let cap_cons = thread::spawn(move || {
        let mut received = 0usize;
        let mut dst = vec![0.0f32; chunk_size];
        while received < num_samples {
            let to_read = chunk_size.min(num_samples - received);
            let read = duplex_cap_cons.pop_capture(&mut dst[..to_read]);
            for i in 0..read {
                let expected = 1.0 + ((received + i) as f32 * 0.0001);
                assert!(
                    (dst[i] - expected).abs() < 1e-4,
                    "Capture channel corruption at {}: expected {}, got {}",
                    received + i,
                    expected,
                    dst[i]
                );
            }
            received += read;
            if read == 0 {
                thread::yield_now();
            }
        }
        received
    });

    let duplex_play_prod = Arc::clone(&duplex);
    let play_prod = thread::spawn(move || {
        let mut sent = 0usize;
        let mut chunk = vec![0.0f32; chunk_size];
        while sent < num_samples {
            let to_send = chunk_size.min(num_samples - sent);
            for i in 0..to_send {
                // Playback channel marker: -2.0 - sample_index (negative)
                chunk[i] = -2.0 - ((sent + i) as f32 * 0.0001);
            }
            let written = duplex_play_prod.push_playback(&chunk[..to_send]);
            sent += written;
            if written == 0 {
                thread::yield_now();
            }
        }
    });

    let duplex_play_cons = Arc::clone(&duplex);
    let play_cons = thread::spawn(move || {
        let mut received = 0usize;
        let mut dst = vec![0.0f32; chunk_size];
        while received < num_samples {
            let to_read = chunk_size.min(num_samples - received);
            let read = duplex_play_cons.pop_playback(&mut dst[..to_read]);
            for i in 0..read {
                let expected = -2.0 - ((received + i) as f32 * 0.0001);
                assert!(
                    (dst[i] - expected).abs() < 1e-4,
                    "Playback channel corruption at {}: expected {}, got {}",
                    received + i,
                    expected,
                    dst[i]
                );
            }
            received += read;
            if read == 0 {
                thread::yield_now();
            }
        }
        received
    });

    cap_prod.join().expect("cap_prod join");
    let total_cap = cap_cons.join().expect("cap_cons join");
    play_prod.join().expect("play_prod join");
    let total_play = play_cons.join().expect("play_cons join");

    assert_eq!(total_cap, num_samples);
    assert_eq!(total_play, num_samples);
    assert!(duplex.capture_ring.is_empty());
    assert!(duplex.playback_ring.is_empty());
}

#[test]
fn test_adversarial_barge_in_flush_during_active_duplex_traffic() {
    let duplex = Arc::new(DuplexAudioRingBuffer::with_capacity(16384, 16000, 24000));
    let stop = Arc::new(AtomicBool::new(false));
    let total_flushes = Arc::new(AtomicUsize::new(0));
    let cap_samples_verified = Arc::new(AtomicUsize::new(0));

    // Capture Producer
    let d_cprod = Arc::clone(&duplex);
    let s_cprod = Arc::clone(&stop);
    let cap_prod = thread::spawn(move || {
        let mut seq = 0u64;
        let mut chunk = vec![0.0f32; 160];
        while !s_cprod.load(Ordering::Relaxed) {
            for i in 0..160 {
                chunk[i] = (seq + i as u64) as f32;
            }
            let written = d_cprod.push_capture(&chunk);
            seq += written as u64;
            if written == 0 {
                thread::yield_now();
            }
        }
        seq
    });

    // Capture Consumer: verifies strictly monotonic continuous sequence
    let d_ccons = Arc::clone(&duplex);
    let s_ccons = Arc::clone(&stop);
    let v_ccons = Arc::clone(&cap_samples_verified);
    let cap_cons = thread::spawn(move || {
        let mut expected_seq = 0u64;
        let mut dst = vec![0.0f32; 160];
        while !s_ccons.load(Ordering::Relaxed) || !d_ccons.capture_ring.is_empty() {
            let read = d_ccons.pop_capture(&mut dst);
            for i in 0..read {
                let actual = dst[i];
                let expected = expected_seq as f32;
                assert_eq!(
                    actual, expected,
                    "Capture sequence violated despite playback flushes! expected {}, got {}",
                    expected, actual
                );
                expected_seq += 1;
            }
            if read == 0 {
                thread::yield_now();
            }
        }
        v_ccons.store(expected_seq as usize, Ordering::Relaxed);
        expected_seq
    });

    // Playback Producer: continuously spamming speaker frames
    let d_pprod = Arc::clone(&duplex);
    let s_pprod = Arc::clone(&stop);
    let play_prod = thread::spawn(move || {
        let chunk = vec![999.0f32; 240];
        while !s_pprod.load(Ordering::Relaxed) {
            d_pprod.push_playback(&chunk);
            thread::yield_now();
        }
    });

    // Playback Consumer: continually draining speaker frames
    let d_pcons = Arc::clone(&duplex);
    let s_pcons = Arc::clone(&stop);
    let play_cons = thread::spawn(move || {
        let mut dst = vec![0.0f32; 240];
        while !s_pcons.load(Ordering::Relaxed) {
            d_pcons.pop_playback(&mut dst);
            thread::yield_now();
        }
    });

    // Interrupter Thread: rapidly firing barge-in flushes
    let d_flush = Arc::clone(&duplex);
    let s_flush = Arc::clone(&stop);
    let f_count = Arc::clone(&total_flushes);
    let flusher = thread::spawn(move || {
        let mut flushes = 0;
        for _ in 0..500 {
            if s_flush.load(Ordering::Relaxed) {
                break;
            }
            d_flush.flush_playback();
            flushes += 1;
            f_count.store(flushes, Ordering::Relaxed);
            thread::sleep(Duration::from_micros(200));
        }
    });

    flusher.join().expect("flusher join");
    thread::sleep(Duration::from_millis(50));
    stop.store(true, Ordering::Relaxed);

    cap_prod.join().expect("cap_prod join");
    let total_cap_read = cap_cons.join().expect("cap_cons join");
    play_prod.join().expect("play_prod join");
    play_cons.join().expect("play_cons join");

    let num_flushes = total_flushes.load(Ordering::Relaxed);
    assert!(num_flushes >= 400, "Must have executed >= 400 flushes, got {}", num_flushes);
    assert!(total_cap_read > 50_000, "Must have processed > 50,000 continuous capture samples");
    println!("Verified {} capture samples across {} barge-in flushes", total_cap_read, num_flushes);
}

#[test]
fn test_adversarial_multi_session_isolation_20_threads() {
    const NUM_SESSIONS: usize = 10;
    const SAMPLES_PER_SESSION: usize = 100_000;

    let sessions: Vec<Arc<DuplexAudioRingBuffer>> = (0..NUM_SESSIONS)
        .map(|_| Arc::new(DuplexAudioRingBuffer::with_capacity(8192, 16000, 24000)))
        .collect();

    let mut prod_handles = Vec::new();
    let mut cons_handles = Vec::new();

    for (session_id, session) in sessions.iter().enumerate() {
        let s_prod = Arc::clone(session);
        let h_prod = thread::spawn(move || {
            let chunk_size = 160;
            let mut sent = 0;
            let mut buf = vec![0.0f32; chunk_size];
            while sent < SAMPLES_PER_SESSION {
                let to_send = chunk_size.min(SAMPLES_PER_SESSION - sent);
                for i in 0..to_send {
                    // Unique signature for this session
                    buf[i] = ((session_id + 1) * 10_000 + (sent + i)) as f32;
                }
                let w = s_prod.push_capture(&buf[..to_send]);
                sent += w;
                if w == 0 {
                    thread::yield_now();
                }
            }
        });
        prod_handles.push(h_prod);

        let s_cons = Arc::clone(session);
        let h_cons = thread::spawn(move || {
            let chunk_size = 160;
            let mut recv = 0;
            let mut dst = vec![0.0f32; chunk_size];
            while recv < SAMPLES_PER_SESSION {
                let to_read = chunk_size.min(SAMPLES_PER_SESSION - recv);
                let r = s_cons.pop_capture(&mut dst[..to_read]);
                for i in 0..r {
                    let expected = ((session_id + 1) * 10_000 + (recv + i)) as f32;
                    assert_eq!(
                        dst[i], expected,
                        "Cross-talk in session {}: expected {}, got {}",
                        session_id, expected, dst[i]
                    );
                }
                recv += r;
                if r == 0 {
                    thread::yield_now();
                }
            }
            recv
        });
        cons_handles.push(h_cons);
    }

    for h in prod_handles {
        h.join().expect("prod thread join");
    }
    for h in cons_handles {
        h.join().expect("cons thread join");
    }
}

#[test]
fn test_adversarial_ring_buffer_boundary_conditions() {
    let rb = AudioRingBufferF32::new(128);

    // 0-length slice operations
    assert_eq!(rb.push_slice(&[]), 0);
    assert_eq!(rb.pop_slice(&mut []), 0);
    assert_eq!(rb.peek_slice(&mut []), 0);
    assert_eq!(rb.skip(0), 0);

    // Fill to exact capacity
    let full_chunk = vec![1.23f32; 128];
    assert_eq!(rb.push_slice(&full_chunk), 128);
    assert_eq!(rb.available_write(), 0);
    assert!(rb.is_full());

    // Push when full returns 0 and increments overrun metric
    let extra = vec![4.56f32; 10];
    assert_eq!(rb.push_slice(&extra), 0);
    let (_, overruns, _, _) = rb.metrics();
    assert_eq!(overruns, 1);

    // Pop partial
    let mut out = vec![0.0f32; 64];
    assert_eq!(rb.pop_slice(&mut out), 64);
    assert_eq!(rb.available_read(), 64);
    assert_eq!(rb.available_write(), 64);

    // Push wrapping around the end
    let wrap_chunk = vec![7.89f32; 64];
    assert_eq!(rb.push_slice(&wrap_chunk), 64);
    assert!(rb.is_full());

    // Clear
    rb.clear();
    assert!(rb.is_empty());
    assert_eq!(rb.available_read(), 0);
    assert_eq!(rb.available_write(), 128);
}

#[test]
fn test_adversarial_pcm_conversions_extreme_floats() {
    let extreme_inputs = vec![
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1e20,
        -1e20,
        1.0,
        -1.0,
        0.0,
        0.5,
        -0.5,
        1e-38, // subnormal
    ];
    let mut i16_out = vec![0i16; extreme_inputs.len()];
    f32_to_i16_slice(&extreme_inputs, &mut i16_out);

    // Verify all outputs are valid i16s and clipped safely
    assert_eq!(i16_out[5], 32767);
    assert_eq!(i16_out[6], -32768);
    assert_eq!(i16_out[7], 0);

    let mut f32_roundtrip = vec![0.0f32; i16_out.len()];
    i16_to_f32_slice(&i16_out, &mut f32_roundtrip);
    assert!(f32_roundtrip.iter().all(|s| s.is_finite()));
}

#[test]
fn test_adversarial_linear_resampler_stress() {
    // Prime lengths
    for len in [1, 3, 7, 13, 17, 31, 127, 257, 1021, 32771] {
        let input: Vec<f32> = (0..len).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut out_24k = Vec::new();
        resample_linear_16k_to_24k(&input, &mut out_24k);
        assert_eq!(out_24k.len(), (len * 3) / 2);
        assert!(out_24k.iter().all(|s| s.is_finite()));

        let mut out_16k = Vec::new();
        resample_linear_24k_to_16k(&out_24k, &mut out_16k);
        assert_eq!(out_16k.len(), (out_24k.len() * 2) / 3);
        assert!(out_16k.iter().all(|s| s.is_finite()));
    }
}

#[test]
fn test_adversarial_wire_transport_concurrent_pool_and_framing() {
    let pool = Arc::new(BufferPool::new(4096, 32));
    let num_threads = 8;
    let frames_per_thread = 5_000;

    let mut handles = Vec::new();
    for thread_idx in 0..num_threads {
        let p = Arc::clone(&pool);
        let h = thread::spawn(move || {
            for seq in 0..frames_per_thread {
                let payload = format!("audio_data_{}_{}", thread_idx, seq);
                let frame = VoiceFrame {
                    op_code: if seq % 2 == 0 { OP_MIC_IN } else { OP_SPEAKER_OUT },
                    seq_id: seq as u32,
                    payload: Bytes::from(payload.clone()),
                };

                let encoded = frame.encode_pooled(&p).expect("encode pooled");
                let mut decode_buf = BytesMut::from(&encoded[..]);
                let decoded = VoiceFrame::decode(&mut decode_buf)
                    .expect("decode ok")
                    .expect("frame present");

                assert_eq!(decoded.op_code, frame.op_code);
                assert_eq!(decoded.seq_id, frame.seq_id);
                assert_eq!(&decoded.payload[..], payload.as_bytes());
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("worker join");
    }
}
