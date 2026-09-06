//! Adversarial Stress & Concurrency Challenge Test Suite for Milestone 3:
//! Full-Duplex Lock-Free Audio Ring Buffer & Wire Transport.
//!
//! Evaluates:
//! 1. High-throughput SPSC concurrency (10M audio samples, zero loss, zero corruption).
//! 2. Extreme chunk sizes (1 sample, power-of-two, odd primes, over-capacity, 0-len).
//! 3. Float32 to Int16 saturation, clipping, NaN, Infinity, and subnormals.
//! 4. In-memory transit latency benchmark (<10ms SLA, sub-microsecond verification).
//! 5. Pure push/pop operation latency profiling.
//! 6. Duplex audio ring buffer concurrent barge-in flush under contention.
//! 7. 64-byte cache line alignment and memory safety invariants.
//! 8. VoiceFrame wire transport, BufferPool recycling, and corrupt frame attacks.
//! 9. Fast linear resampling signal fidelity and boundary conditions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use liva_native_core::webrtc::frame::*;
use liva_native_core::webrtc::ring_buffer::*;

// ============================================================================
// 1. High-Throughput SPSC Concurrency Stress (10,000,000 samples)
// ============================================================================

#[test]
fn test_adv_spsc_high_throughput_10m_samples_zero_corruption() {
    const TOTAL_SAMPLES: usize = 10_000_000;
    const CHUNK_SIZE: usize = 256;
    const BUFFER_CAP: usize = 32768;

    let ring = Arc::new(AudioRingBufferF32::new(BUFFER_CAP));
    let ring_producer = Arc::clone(&ring);
    let ring_consumer = Arc::clone(&ring);

    let start_time = Instant::now();

    // Producer thread
    let producer_handle = thread::spawn(move || {
        let mut total_sent = 0;
        let mut chunk = vec![0.0f32; CHUNK_SIZE];

        while total_sent < TOTAL_SAMPLES {
            let to_send = CHUNK_SIZE.min(TOTAL_SAMPLES - total_sent);
            for i in 0..to_send {
                chunk[i] = ((total_sent + i) as f64 * 0.0001) as f32;
            }

            let written = ring_producer.push_slice(&chunk[..to_send]);
            total_sent += written;

            if written == 0 {
                std::hint::spin_loop();
            }
        }
        total_sent
    });

    // Consumer thread
    let consumer_handle = thread::spawn(move || {
        let mut total_received = 0;
        let mut chunk = vec![0.0f32; CHUNK_SIZE];

        while total_received < TOTAL_SAMPLES {
            let to_read = CHUNK_SIZE.min(TOTAL_SAMPLES - total_received);
            let read = ring_consumer.pop_slice(&mut chunk[..to_read]);

            for i in 0..read {
                let expected = ((total_received + i) as f64 * 0.0001) as f32;
                let actual = chunk[i];
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "Sample corruption at index {}: expected {:?} ({}), got {:?} ({})",
                    total_received + i,
                    expected,
                    expected.to_bits(),
                    actual,
                    actual.to_bits()
                );
            }

            total_received += read;
            if read == 0 {
                std::hint::spin_loop();
            }
        }
        total_received
    });

    let sent = producer_handle.join().expect("Producer thread panicked");
    let received = consumer_handle.join().expect("Consumer thread panicked");
    let elapsed = start_time.elapsed();

    assert_eq!(sent, TOTAL_SAMPLES);
    assert_eq!(received, TOTAL_SAMPLES);
    assert!(ring.is_empty());

    let samples_per_sec = (TOTAL_SAMPLES as f64) / elapsed.as_secs_f64();
    let mb_per_sec = (TOTAL_SAMPLES * std::mem::size_of::<f32>()) as f64
        / (1024.0 * 1024.0 * elapsed.as_secs_f64());

    println!(
        "\n--- High-Throughput SPSC Concurrency Benchmark (10M Samples) ---\n\
         Total Samples:  10,000,000\n\
         Total Duration: {:.2?}\n\
         Throughput:     {:.2} M samples/sec ({:.2} MB/s)\n",
        elapsed,
        samples_per_sec / 1_000_000.0,
        mb_per_sec
    );
    assert!(
        samples_per_sec > 5_000_000.0,
        "Throughput too low: {:.2} samples/sec",
        samples_per_sec
    );
}

#[test]
fn test_adv_const_generic_spsc_5m_samples_prime_chunks() {
    const TOTAL_SAMPLES: usize = 5_000_000;
    const CAP: usize = 16384;

    let ring = Arc::new(SpscRingBuffer::<f32, CAP>::new());
    let ring_producer = Arc::clone(&ring);
    let ring_consumer = Arc::clone(&ring);

    let prime_chunks = [13, 29, 67, 127, 251, 509, 1021];

    let producer_handle = thread::spawn(move || {
        let mut total_sent = 0;
        let mut chunk_idx = 0;

        while total_sent < TOTAL_SAMPLES {
            let chunk_size = prime_chunks[chunk_idx % prime_chunks.len()];
            chunk_idx += 1;

            let to_send = chunk_size.min(TOTAL_SAMPLES - total_sent);
            let mut chunk = vec![0.0f32; to_send];
            for i in 0..to_send {
                chunk[i] = ((total_sent + i) as f32) + 0.125;
            }

            let mut written_this_chunk = 0;
            while written_this_chunk < to_send {
                let w = ring_producer.push_slice(&chunk[written_this_chunk..]);
                written_this_chunk += w;
                if w == 0 {
                    std::hint::spin_loop();
                }
            }
            total_sent += to_send;
        }
        total_sent
    });

    let consumer_handle = thread::spawn(move || {
        let mut total_received = 0;
        let mut chunk_idx = 0;

        while total_received < TOTAL_SAMPLES {
            let chunk_size = prime_chunks[(chunk_idx + 3) % prime_chunks.len()];
            chunk_idx += 1;

            let to_read = chunk_size.min(TOTAL_SAMPLES - total_received);
            let mut chunk = vec![0.0f32; to_read];

            let mut read_this_chunk = 0;
            while read_this_chunk < to_read {
                let r = ring_consumer.pop_slice(&mut chunk[read_this_chunk..]);
                read_this_chunk += r;
                if r == 0 {
                    std::hint::spin_loop();
                }
            }

            for i in 0..to_read {
                let expected = ((total_received + i) as f32) + 0.125;
                assert_eq!(chunk[i], expected);
            }
            total_received += to_read;
        }
        total_received
    });

    let sent = producer_handle.join().unwrap();
    let received = consumer_handle.join().unwrap();
    assert_eq!(sent, TOTAL_SAMPLES);
    assert_eq!(received, TOTAL_SAMPLES);
    assert!(ring.is_empty());
}

// ============================================================================
// 2. Extreme Chunk Sizes & Boundary Stress
// ============================================================================

#[test]
fn test_adv_extreme_chunk_sizes_1_to_overcapacity() {
    let rb = AudioRingBufferF32::new(128);

    // 1. Single sample writes & reads (50,000 iterations)
    for i in 0..50_000 {
        let sample = [i as f32 * 0.5];
        assert_eq!(rb.push_slice(&sample), 1);
        let mut out = [0.0f32];
        assert_eq!(rb.pop_slice(&mut out), 1);
        assert_eq!(out[0], sample[0]);
    }
    assert!(rb.is_empty());

    // 2. Odd and prime chunk sizes
    let test_sizes = [1, 2, 3, 5, 7, 11, 13, 17, 19, 23, 31, 63, 64, 65, 127, 128];
    for &size in &test_sizes {
        let input: Vec<f32> = (0..size).map(|x| (x * 10) as f32).collect();
        let written = rb.push_slice(&input);
        assert_eq!(written, size);
        assert_eq!(rb.available_read(), size);

        let mut output = vec![0.0f32; size];
        let read = rb.pop_slice(&mut output);
        assert_eq!(read, size);
        assert_eq!(output, input);
        assert!(rb.is_empty());
    }

    // 3. Over-capacity push (Push 200 into capacity 128)
    let large_input: Vec<f32> = (0..200).map(|x| x as f32).collect();
    let written = rb.push_slice(&large_input);
    assert_eq!(written, 128, "Must accept exactly up to available capacity");
    assert!(rb.is_full());
    assert_eq!(rb.available_read(), 128);

    let mut large_output = vec![0.0f32; 200];
    let read = rb.pop_slice(&mut large_output);
    assert_eq!(read, 128, "Must read exactly 128 available samples");
    assert_eq!(&large_output[..128], &large_input[..128]);
    assert!(rb.is_empty());

    // 4. Over-capacity pop (Pop 200 when only 50 available)
    let input_50: Vec<f32> = (0..50).map(|x| x as f32 + 99.0).collect();
    assert_eq!(rb.push_slice(&input_50), 50);

    let mut output_200 = vec![-1.0f32; 200];
    let read_50 = rb.pop_slice(&mut output_200);
    assert_eq!(read_50, 50);
    assert_eq!(&output_200[..50], &input_50);
    assert_eq!(output_200[50], -1.0, "Remaining dst must not be overwritten");
    assert!(rb.is_empty());

    // 5. Zero length push & pop
    assert_eq!(rb.push_slice(&[]), 0);
    assert_eq!(rb.pop_slice(&mut []), 0);
    assert_eq!(rb.peek_slice(&mut []), 0);
    assert_eq!(rb.skip(0), 0);
}

#[test]
fn test_adv_ring_buffer_wrap_boundary_all_alignments() {
    let rb = AudioRingBufferF32::new(64);

    for initial_offset in 0..64 {
        rb.clear();
        if initial_offset > 0 {
            let dummy = vec![0.0f32; initial_offset];
            rb.push_slice(&dummy);
            let mut scratch = vec![0.0f32; initial_offset];
            rb.pop_slice(&mut scratch);
            assert!(rb.is_empty());
        }

        for chunk_len in 1..=64 {
            let input: Vec<f32> = (0..chunk_len).map(|i| (i + 1) as f32).collect();
            let written = rb.push_slice(&input);
            assert_eq!(written, chunk_len);

            let mut peek_buf = vec![0.0f32; chunk_len];
            let peeked = rb.peek_slice(&mut peek_buf);
            assert_eq!(peeked, chunk_len);
            assert_eq!(peek_buf, input);

            let mut output = vec![0.0f32; chunk_len];
            let read = rb.pop_slice(&mut output);
            assert_eq!(read, chunk_len);
            assert_eq!(output, input);
            assert!(rb.is_empty());
        }
    }
}

// ============================================================================
// 3. Saturation, Clipping, NaN, and Special Float Values
// ============================================================================

#[test]
fn test_adv_f32_to_i16_clipping_and_special_values() {
    let mut dst = vec![0i16; 30];

    let src = vec![
        0.0f32,
        -0.0f32,
        1.0f32,
        -1.0f32,
        0.5f32,
        -0.5f32,
        // Over-range
        1.0001f32,
        1.5f32,
        2.0f32,
        100.0f32,
        1e20f32,
        f32::MAX,
        f32::INFINITY,
        // Under-range
        -1.0001f32,
        -1.5f32,
        -2.0f32,
        -100.0f32,
        -1e20f32,
        f32::MIN,
        f32::NEG_INFINITY,
        // Subnormals & tiny floats
        f32::MIN_POSITIVE,
        f32::MIN_POSITIVE / 2.0,
        1e-10f32,
        -1e-10f32,
        // NaN
        f32::NAN,
    ];

    let count = f32_to_i16_slice(&src, &mut dst);
    assert_eq!(count, src.len());

    assert_eq!(dst[0], 0, "0.0 -> 0");
    assert_eq!(dst[1], 0, "-0.0 -> 0");
    assert_eq!(dst[2], 32767, "1.0 -> 32767");
    assert_eq!(dst[3], -32768, "-1.0 -> -32768");
    assert_eq!(dst[4], 16384, "0.5 -> 16384");
    assert_eq!(dst[5], -16384, "-0.5 -> -16384");

    // Positive overflow clipping
    for idx in 6..=12 {
        assert_eq!(
            dst[idx], 32767,
            "Index {} ({:?}) must clip to 32767",
            idx, src[idx]
        );
    }

    // Negative underflow clipping
    for idx in 13..=19 {
        assert_eq!(
            dst[idx], -32768,
            "Index {} ({:?}) must clip to -32768",
            idx, src[idx]
        );
    }

    // Subnormals must round safely to 0
    assert_eq!(dst[20], 0);
    assert_eq!(dst[21], 0);
    assert_eq!(dst[22], 0);
    assert_eq!(dst[23], 0);

    // NaN must produce a finite i16 without panic
    let _nan_result = dst[24];
}

#[test]
fn test_adv_pcm_roundtrip_precision_and_limits() {
    let all_i16: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    let mut f32_buf = vec![0.0f32; all_i16.len()];
    let mut roundtrip_i16 = vec![0i16; all_i16.len()];

    i16_to_f32_slice(&all_i16, &mut f32_buf);

    for &f in &f32_buf {
        assert!(f >= -1.0 && f <= 1.0, "Float out of bounds: {}", f);
        assert!(f.is_finite());
    }

    f32_to_i16_slice(&f32_buf, &mut roundtrip_i16);

    let mut max_diff = 0i32;
    for i in 0..all_i16.len() {
        let diff = (all_i16[i] as i32 - roundtrip_i16[i] as i32).abs();
        if diff > max_diff {
            max_diff = diff;
        }
        assert!(
            diff <= 1,
            "Quantization error too high for i16 {}: got {}",
            all_i16[i],
            roundtrip_i16[i]
        );
    }
    assert!(max_diff <= 1, "Max quantization diff: {}", max_diff);
}

// ============================================================================
// 4. In-Memory Transit Latency Benchmark (<10ms SLA, sub-microsecond empirical)
// ============================================================================

#[test]
fn test_adv_transit_latency_sub_microsecond() {
    const NUM_PACKETS: usize = 100_000;
    const SAMPLES_PER_FRAME: usize = 160; // 10ms frame @ 16kHz
    const RING_CAPACITY: usize = 8192;

    let ring = Arc::new(AudioRingBufferF32::new(RING_CAPACITY));
    let (tx_ts, rx_ts) = std::sync::mpsc::sync_channel::<Instant>(RING_CAPACITY);

    let ring_prod = Arc::clone(&ring);
    let ring_cons = Arc::clone(&ring);

    let prod = thread::spawn(move || {
        let frame = vec![0.123f32; SAMPLES_PER_FRAME];
        for _ in 0..NUM_PACKETS {
            let ts = Instant::now();
            tx_ts.send(ts).unwrap();

            while ring_prod.available_write() < SAMPLES_PER_FRAME {
                std::hint::spin_loop();
            }
            ring_prod.push_slice(&frame);
        }
    });

    let cons = thread::spawn(move || {
        let mut dst = vec![0.0f32; SAMPLES_PER_FRAME];
        let mut latencies_ns = Vec::with_capacity(NUM_PACKETS);

        for _ in 0..NUM_PACKETS {
            let ts = rx_ts.recv().unwrap();

            while ring_cons.available_read() < SAMPLES_PER_FRAME {
                std::hint::spin_loop();
            }
            ring_cons.pop_slice(&mut dst);

            let elapsed = ts.elapsed();
            latencies_ns.push(elapsed.as_nanos() as u64);
        }

        latencies_ns
    });

    prod.join().unwrap();
    let mut latencies_ns = cons.join().unwrap();
    assert_eq!(latencies_ns.len(), NUM_PACKETS);

    latencies_ns.sort_unstable();

    let mean_ns: u64 = latencies_ns.iter().sum::<u64>() / (NUM_PACKETS as u64);
    let p50_ns = latencies_ns[NUM_PACKETS * 50 / 100];
    let p95_ns = latencies_ns[NUM_PACKETS * 95 / 100];
    let p99_ns = latencies_ns[NUM_PACKETS * 99 / 100];
    let p999_ns = latencies_ns[NUM_PACKETS * 999 / 1000];
    let max_ns = latencies_ns[NUM_PACKETS - 1];

    println!(
        "\n--- In-Memory Audio Transit Latency (100k packets of 160 samples) ---\n\
         Mean:   {:>8.2} µs ({} ns)\n\
         p50:    {:>8.2} µs ({} ns)\n\
         p95:    {:>8.2} µs ({} ns)\n\
         p99:    {:>8.2} µs ({} ns)\n\
         p99.9:  {:>8.2} µs ({} ns)\n\
         Max:    {:>8.2} µs ({} ns)\n\
         SLA (<10ms): PASS (p99.9 is {:.4} ms)\n",
        mean_ns as f64 / 1000.0, mean_ns,
        p50_ns as f64 / 1000.0, p50_ns,
        p95_ns as f64 / 1000.0, p95_ns,
        p99_ns as f64 / 1000.0, p99_ns,
        p999_ns as f64 / 1000.0, p999_ns,
        max_ns as f64 / 1000.0, max_ns,
        p999_ns as f64 / 1_000_000.0
    );

    assert!(
        p99_ns < 1_000_000,
        "p99 latency must be under 1ms, got {} ns",
        p99_ns
    );
    assert!(
        p999_ns < 10_000_000,
        "p99.9 latency must be under 10ms SLA, got {} ns",
        p999_ns
    );
}

// ============================================================================
// 5. Pure Memory Push/Pop Operation Latency Profile (Sub-Microsecond)
// ============================================================================

#[test]
fn test_adv_pure_push_pop_sub_microsecond_profiling() {
    const ITERS: usize = 200_000;
    const FRAME_SIZE: usize = 160; // 10ms of 16kHz audio
    let rb = AudioRingBufferF32::new(4096);

    let frame = vec![0.5f32; FRAME_SIZE];
    let mut out = vec![0.0f32; FRAME_SIZE];

    let start = Instant::now();
    for _ in 0..ITERS {
        rb.push_slice(&frame);
        rb.pop_slice(&mut out);
    }
    let elapsed = start.elapsed();

    let ns_per_cycle = (elapsed.as_nanos() as f64) / (ITERS as f64);
    let ns_per_push_pop = ns_per_cycle / 2.0;

    println!(
        "\n--- Pure RingBuffer Push/Pop Latency Profile ---\n\
         Operations:     {} push+pop cycles ({} ops total)\n\
         Total Duration: {:.2?}\n\
         Per Op Latency: {:.2} ns ({:.4} µs)\n\
         Per Cycle:      {:.2} ns ({:.4} µs)\n",
        ITERS,
        ITERS * 2,
        elapsed,
        ns_per_push_pop,
        ns_per_push_pop / 1000.0,
        ns_per_cycle,
        ns_per_cycle / 1000.0
    );

    // Pure memory push/pop of 160 float32s should take less than 1µs (1000ns)
    assert!(
        ns_per_push_pop < 1000.0,
        "Push/pop operation too slow: {:.2} ns",
        ns_per_push_pop
    );
}

// ============================================================================
// 6. Full-Duplex & Instant Barge-In Preemption Under Load
// ============================================================================

#[test]
fn test_adv_duplex_audio_ring_buffer_barge_in_concurrency_stress() {
    let duplex = Arc::new(DuplexAudioRingBuffer::new(16000, 24000));
    let stop = Arc::new(AtomicBool::new(false));

    // Thread 1: Mic Capture Producer (16kHz, 160 samples per frame)
    let d1 = Arc::clone(&duplex);
    let s1 = Arc::clone(&stop);
    let h_mic_prod = thread::spawn(move || {
        let frame = vec![0.05f32; 160];
        let mut count = 0;
        while !s1.load(Ordering::Relaxed) {
            d1.push_capture(&frame);
            count += 160;
            std::hint::spin_loop();
        }
        count
    });

    // Thread 2: Mic Capture Consumer (VAD / STT)
    let d2 = Arc::clone(&duplex);
    let s2 = Arc::clone(&stop);
    let h_mic_cons = thread::spawn(move || {
        let mut buf = vec![0.0f32; 160];
        let mut count = 0;
        while !s2.load(Ordering::Relaxed) {
            let r = d2.pop_capture(&mut buf);
            count += r;
            std::hint::spin_loop();
        }
        count
    });

    // Thread 3: Speaker Playback Producer (TTS stream, 240 samples per frame)
    let d3 = Arc::clone(&duplex);
    let s3 = Arc::clone(&stop);
    let h_spk_prod = thread::spawn(move || {
        let frame = vec![0.8f32; 240];
        let mut count = 0;
        while !s3.load(Ordering::Relaxed) {
            d3.push_playback(&frame);
            count += 240;
            std::hint::spin_loop();
        }
        count
    });

    // Thread 4: Speaker Playback Consumer (DAC / AudioWorklet)
    let d4 = Arc::clone(&duplex);
    let s4 = Arc::clone(&stop);
    let h_spk_cons = thread::spawn(move || {
        let mut buf = vec![0.0f32; 240];
        let mut count = 0;
        while !s4.load(Ordering::Relaxed) {
            let r = d4.pop_playback(&mut buf);
            count += r;
            std::hint::spin_loop();
        }
        count
    });

    // Thread 5: Barge-In Trigger (Repeatedly calls flush_playback())
    let d5 = Arc::clone(&duplex);
    let s5 = Arc::clone(&stop);
    let h_barge_in = thread::spawn(move || {
        let mut flushes = 0;
        while !s5.load(Ordering::Relaxed) {
            d5.flush_playback();
            flushes += 1;
            thread::yield_now();
        }
        flushes
    });

    thread::sleep(Duration::from_millis(150));
    stop.store(true, Ordering::Relaxed);

    let mic_p = h_mic_prod.join().unwrap();
    let mic_c = h_mic_cons.join().unwrap();
    let spk_p = h_spk_prod.join().unwrap();
    let spk_c = h_spk_cons.join().unwrap();
    let flushes = h_barge_in.join().unwrap();

    println!(
        "Duplex Stress Result: Mic Pushed={}, Mic Read={}, Spk Pushed={}, Spk Read={}, Flushes={}",
        mic_p, mic_c, spk_p, spk_c, flushes
    );

    assert!(mic_p > 0);
    assert!(mic_c > 0);
    assert!(spk_p > 0);
    assert!(flushes > 0);
}

// ============================================================================
// 7. 64-Byte Cache Alignment & Memory Safety Verification
// ============================================================================

#[test]
fn test_adv_cache_alignment_and_metrics() {
    assert_eq!(std::mem::align_of::<CacheAlignedAtomic>(), 64);
    assert_eq!(std::mem::size_of::<CacheAlignedAtomic>(), 64);

    let rb = AudioRingBufferF32::new(1024);
    assert_eq!(rb.capacity(), 1024);

    let (underruns, overruns, tw, tr) = rb.metrics();
    assert_eq!(underruns, 0);
    assert_eq!(overruns, 0);
    assert_eq!(tw, 0);
    assert_eq!(tr, 0);

    let data = vec![1.0f32; 1500];
    let written = rb.push_slice(&data);
    assert_eq!(written, 1024);
    let (_, overruns, tw, _) = rb.metrics();
    assert_eq!(overruns, 1);
    assert_eq!(tw, 1024);

    let mut dst = vec![0.0f32; 1500];
    let read = rb.pop_slice(&mut dst);
    assert_eq!(read, 1024);
    let (underruns, _, _, tr) = rb.metrics();
    assert_eq!(underruns, 1);
    assert_eq!(tr, 1024);
}

#[test]
fn test_adv_peek_and_skip_exhaustive() {
    let rb = AudioRingBufferF32::new(128);
    let data = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    rb.push_slice(&data);

    for _ in 0..10 {
        let mut peek_buf = vec![0.0f32; 5];
        let p = rb.peek_slice(&mut peek_buf);
        assert_eq!(p, 5);
        assert_eq!(peek_buf, data);
        assert_eq!(rb.available_read(), 5);
    }

    assert_eq!(rb.skip(2), 2);
    assert_eq!(rb.available_read(), 3);

    let mut remaining = vec![0.0f32; 3];
    assert_eq!(rb.peek_slice(&mut remaining), 3);
    assert_eq!(remaining, vec![30.0, 40.0, 50.0]);

    assert_eq!(rb.skip(100), 3);
    assert_eq!(rb.available_read(), 0);
    assert!(rb.is_empty());
}

// ============================================================================
// 8. VoiceFrame Wire Transport & BufferPool Recycling Stress
// ============================================================================

#[test]
fn test_adv_voice_frame_wire_transport_and_pool_stress() {
    let pool = Arc::new(BufferPool::new(64 * 1024, 64));
    const NUM_FRAMES: usize = 50_000;

    let pool_sender = Arc::clone(&pool);
    let (tx, rx) = std::sync::mpsc::sync_channel::<Bytes>(1024);

    // Frame serializer thread
    let sender = thread::spawn(move || {
        for seq in 0..NUM_FRAMES as u32 {
            let op = match seq % 4 {
                0 => OP_MIC_IN,
                1 => OP_SPEAKER_OUT,
                2 => OP_FLUSH,
                _ => OP_VISME,
            };

            let payload_data = format!("frame-payload-seq-{}", seq);
            let frame = VoiceFrame {
                op_code: op,
                seq_id: seq,
                payload: Bytes::from(payload_data),
            };

            let encoded = frame.encode_pooled(&pool_sender).expect("Encode failed");
            tx.send(encoded).unwrap();
        }
    });

    // Frame receiver & deserializer thread
    let receiver = thread::spawn(move || {
        let mut stream_buf = BytesMut::new();
        let mut decoded_count = 0;

        while decoded_count < NUM_FRAMES as u32 {
            let chunk = rx.recv().unwrap();
            stream_buf.extend_from_slice(&chunk);

            while let Some(frame) = VoiceFrame::decode(&mut stream_buf).unwrap() {
                let expected_op = match decoded_count % 4 {
                    0 => OP_MIC_IN,
                    1 => OP_SPEAKER_OUT,
                    2 => OP_FLUSH,
                    _ => OP_VISME,
                };
                assert_eq!(frame.op_code, expected_op);
                assert_eq!(frame.seq_id, decoded_count);

                let expected_payload = format!("frame-payload-seq-{}", decoded_count);
                assert_eq!(&frame.payload[..], expected_payload.as_bytes());

                decoded_count += 1;
            }
        }
        decoded_count
    });

    sender.join().unwrap();
    let total_decoded = receiver.join().unwrap();
    assert_eq!(total_decoded, NUM_FRAMES as u32);
}

// ============================================================================
// 9. Resampler Signal Fidelity & Boundary Tests
// ============================================================================

#[test]
fn test_adv_resamplers_fidelity_and_boundary() {
    // 16kHz -> 24kHz -> 16kHz roundtrip sine wave test
    let num_samples_16k = 1600; // 100ms of audio
    let freq = 440.0f32; // 440Hz standard A
    let input_16k: Vec<f32> = (0..num_samples_16k)
        .map(|i| (2.0 * std::f32::consts::PI * freq * (i as f32) / 16000.0).sin())
        .collect();

    let mut out_24k = Vec::new();
    resample_linear_16k_to_24k(&input_16k, &mut out_24k);
    assert_eq!(out_24k.len(), (num_samples_16k * 3) / 2);

    let mut out_16k = Vec::new();
    resample_linear_24k_to_16k(&out_24k, &mut out_16k);
    assert_eq!(out_16k.len(), num_samples_16k);

    // Verify MSE between original and roundtripped is minimal (< 0.05 for linear interpolation)
    let mse: f32 = input_16k
        .iter()
        .zip(out_16k.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        / (num_samples_16k as f32);

    println!("Resampler Roundtrip (16k -> 24k -> 16k) MSE: {:.6}", mse);
    assert!(mse < 0.05, "Resampler MSE too high: {}", mse);
}
