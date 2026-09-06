//! Empirical Adversarial Challenger Test Suite (Milestone 1, Challenger 2)
//! 
//! Rigorous empirical stress testing and benchmarking:
//! 1. Vector Search Performance & Filtering at Scale (600+ vectors, large top_k, filtered vs unfiltered, domain/category/type indexing, score ordering, concurrent readers)
//! 2. WebSocket & Audio Frame Streaming Robustness:
//!    - format_ai_stream_chunk against empty, massive (5MB), Unicode/Vietnamese, JSON-injection, and control-character payloads
//!    - speaker_frames against empty, tiny, massive (1M+ samples), non-standard/extreme sample rates, NaN/Inf floats, and bit-level roundtrip verification

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{
    DatabasePool, MetadataFilter, search_similar_vectors, upsert_vector,
};
use liva_native_core::webrtc::frame::{
    OP_SPEAKER_OUT, SpeakerEpochGate, speaker_frames, speaker_turn_epoch,
};
use liva_native_core::websocket::format_ai_stream_chunk;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// SUITE 1: Vector Search Performance, Scale & Filter Stress
// ============================================================================

#[test]
fn test_challenger_vector_search_large_scale_unfiltered_and_filtered_bench() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db init");
    let crypto = EncryptionEngine::new("0123456789abcdef0123456789abcdef");
    let conn = pool.writer.get().expect("acquire writer connection");

    let domains = ["Personal", "Work", "Health", "Finance", "System"];
    let categories = ["Banking", "Invoices", "Reports", "Notes", "Logs", "General"];
    let types = ["fact", "observation", "rule"];

    let total_vectors = 600;
    println!("\n=== Vector Scale Benchmark: Inserting {} 384-d vectors ===", total_vectors);
    let start_insert = Instant::now();

    for i in 0..total_vectors {
        let mut vec = vec![0.0f32; 384];
        if i == 0 {
            // Target probe vector is identical to probe [1.0, 0, 0, ...]
            vec[0] = 1.0f32;
        } else {
            // Distinct non-zero angle vectors
            let dim_idx = (i % 383) + 1; // indices 1..383
            vec[dim_idx] = 1.0f32;
            vec[0] = 0.5f32 / (1.0f32 + (i as f32) * 0.05f32);
        }

        let domain = domains[i % domains.len()];
        let category = categories[i % categories.len()];
        let vtype = types[i % types.len()];

        upsert_vector(
            &conn,
            &crypto,
            &format!("vec_id_{:04}", i),
            vtype,
            &format!("Payload content for vector {} domain={} cat={}", i, domain, category),
            &vec,
            Some(domain),
            Some(category),
            None,
            None,
            None,
        )
        .expect("upsert_vector must succeed");
    }

    let insert_duration = start_insert.elapsed();
    println!("Inserted {} vectors in {:?}", total_vectors, insert_duration);

    // Construct target probe vector aligned with index 0
    let mut probe_vec = vec![0.0f32; 384];
    probe_vec[0] = 1.0f32;

    // 1. Unfiltered Large Result Set Tests (top_k = 10, 50, 100, 200, 500)
    for top_k in [10, 50, 100, 200, 500] {
        let t0 = Instant::now();
        let results = search_similar_vectors(
            &conn,
            &crypto,
            &probe_vec,
            top_k,
            &MetadataFilter::default(),
        )
        .expect("unfiltered search must succeed");
        let elapsed = t0.elapsed();

        assert_eq!(
            results.len(),
            top_k.min(total_vectors),
            "Expected exactly {} results for top_k={}",
            top_k.min(total_vectors),
            top_k
        );

        // Verify score ordering (descending)
        for w in results.windows(2) {
            assert!(
                w[0].score >= w[1].score,
                "Results must be strictly sorted by score descending: {} vs {}",
                w[0].score,
                w[1].score
            );
        }

        // Top match should be vec_id_0000
        assert_eq!(results[0].vec_id, "vec_id_0000");
        assert!(results[0].score > 0.99, "Top-1 vector should have near-perfect similarity");

        println!(
            "  [Unfiltered] top_k={:3} -> {} results returned in {:?} (Top-1: {}, Score: {:.4})",
            top_k,
            results.len(),
            elapsed,
            results[0].vec_id,
            results[0].score
        );
    }

    // 2. Domain Filter Tests
    for domain_name in domains {
        let filter = MetadataFilter {
            domain: Some(domain_name.to_string()),
            ..Default::default()
        };

        let t0 = Instant::now();
        let results = search_similar_vectors(&conn, &crypto, &probe_vec, 50, &filter)
            .expect("domain filtered search");
        let elapsed = t0.elapsed();

        assert!(!results.is_empty(), "Domain {} must return results", domain_name);
        for item in &results {
            assert_eq!(item.domain, domain_name, "All results must match filtered domain");
        }

        println!(
            "  [Domain Filter: {:8}] top_k=50 -> {} results in {:?}",
            domain_name,
            results.len(),
            elapsed
        );
    }

    // 3. Category Filter Tests
    for category_name in categories {
        let filter = MetadataFilter {
            category: Some(category_name.to_string()),
            ..Default::default()
        };

        let t0 = Instant::now();
        let results = search_similar_vectors(&conn, &crypto, &probe_vec, 50, &filter)
            .expect("category filtered search");
        let elapsed = t0.elapsed();

        assert!(!results.is_empty(), "Category {} must return results", category_name);
        for item in &results {
            assert_eq!(item.category, category_name, "All results must match filtered category");
        }

        println!(
            "  [Category Filter: {:8}] top_k=50 -> {} results in {:?}",
            category_name,
            results.len(),
            elapsed
        );
    }

    // 4. Composite (Domain + Category + Type) Filter Tests
    // i=1 has domain "Work", category "Invoices", type "observation"
    let composite_filter = MetadataFilter {
        domain: Some("Work".to_string()),
        category: Some("Invoices".to_string()),
        r#type: Some("observation".to_string()),
        ..Default::default()
    };

    let t0 = Instant::now();
    let composite_results = search_similar_vectors(&conn, &crypto, &probe_vec, 50, &composite_filter)
        .expect("composite filtered search");
    let comp_elapsed = t0.elapsed();

    assert!(!composite_results.is_empty(), "Composite filter must find matching items");
    for item in &composite_results {
        assert_eq!(item.domain, "Work");
        assert_eq!(item.category, "Invoices");
        assert_eq!(item.r#type, "observation");
    }
    println!(
        "  [Composite Filter] (Work+Invoices+observation) -> {} results in {:?}",
        composite_results.len(),
        comp_elapsed
    );

    // 5. Non-matching Filter Test (must return empty cleanly, no SQL error)
    let non_matching_filter = MetadataFilter {
        domain: Some("NonExistentDomainXYZ".to_string()),
        ..Default::default()
    };
    let empty_res = search_similar_vectors(&conn, &crypto, &probe_vec, 100, &non_matching_filter)
        .expect("non-matching filter search");
    assert!(empty_res.is_empty(), "Non-matching filter must return 0 results");

    // 6. Large top_k query up to sqlite-vec limit (top_k = 1000 on 600 vectors)
    let large_k_res = search_similar_vectors(&conn, &crypto, &probe_vec, 1000, &MetadataFilter::default())
        .expect("large top_k search");
    assert_eq!(large_k_res.len(), total_vectors);
}

#[tokio::test]
async fn test_challenger_concurrent_vector_searches_multi_reader_pool() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db init");
    let crypto = EncryptionEngine::new("0123456789abcdef0123456789abcdef");

    // Seed 100 vectors
    {
        let conn = pool.writer.get().expect("writer");
        for i in 0..100 {
            let mut v = vec![0.0f32; 384];
            v[i % 384] = 1.0f32;
            upsert_vector(
                &conn,
                &crypto,
                &format!("c_vec_{}", i),
                "fact",
                &format!("Concurrent fact {}", i),
                &v,
                Some(if i % 2 == 0 { "Personal" } else { "Work" }),
                Some(if i % 3 == 0 { "Finance" } else { "General" }),
                None,
                None,
                None,
            )
            .expect("seed vector");
        }
    }

    let pool_arc = Arc::new(pool);
    let crypto_arc = Arc::new(crypto);
    let num_tasks = 20;
    let queries_per_task = 25;
    let mut handles = Vec::new();

    for task_id in 0..num_tasks {
        let pool_clone = Arc::clone(&pool_arc);
        let crypto_clone = Arc::clone(&crypto_arc);

        handles.push(tokio::spawn(async move {
            let mut query_vec = vec![0.0f32; 384];
            query_vec[task_id % 384] = 1.0f32;

            for q in 0..queries_per_task {
                let conn = pool_clone.readers.get().expect("reader connection");
                let filter = if q % 2 == 0 {
                    MetadataFilter {
                        domain: Some("Work".to_string()),
                        ..Default::default()
                    }
                } else {
                    MetadataFilter::default()
                };

                let res = search_similar_vectors(
                    &conn,
                    &crypto_clone,
                    &query_vec,
                    10,
                    &filter,
                )
                .expect("concurrent search must succeed");

                assert!(!res.is_empty());
                if filter.domain.is_some() {
                    assert_eq!(res[0].domain, "Work");
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task join");
    }
}

// ============================================================================
// SUITE 2: format_ai_stream_chunk Stress, Injection & Unicode Resilience
// ============================================================================

#[test]
fn test_challenger_format_ai_stream_chunk_malformed_and_extreme_payloads() {
    // 1. Empty string chunk
    for is_thought in [false, true] {
        let json = format_ai_stream_chunk("", is_thought).expect("empty string format");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(parsed["event"], "ai_stream_chunk");
        assert_eq!(parsed["payload"]["textChunk"], "");
        assert_eq!(parsed["payload"]["isThought"], is_thought);
    }

    // 2. Control Characters & Null Bytes
    let control_chars: String = (0x00u8..=0x1Fu8).map(|b| b as char).collect();
    let json_ctrl = format_ai_stream_chunk(&control_chars, false).expect("control chars format");
    let parsed_ctrl: serde_json::Value = serde_json::from_str(&json_ctrl).expect("valid json");
    assert_eq!(parsed_ctrl["payload"]["textChunk"], control_chars);

    // 3. Complex Multi-byte Vietnamese, Emoji, ZWJ & RTL strings
    let complex_unicode = "Tiếng Việt có dấu: ế, ắ, ồ, ừ, ỳ. Emojis: 👨‍👩‍👧‍👦 🚀 🦀 🦄. RTL: עִבְרִית / العربية. Math: ∑ ∫ √ π.";
    let json_unicode = format_ai_stream_chunk(complex_unicode, true).expect("unicode format");
    let parsed_unicode: serde_json::Value = serde_json::from_str(&json_unicode).expect("valid json");
    assert_eq!(parsed_unicode["payload"]["textChunk"], complex_unicode);
    assert_eq!(parsed_unicode["payload"]["isThought"], true);

    // 4. JSON Injection & Escaped payloads
    let injection_payloads = [
        r#"{"event":"fake_event","hacked":true}"#,
        r#"text with "quotes", \backslashes\, \n newlines, \r carriage returns, \t tabs"#,
        r#"</script><script>alert('XSS')</script>"#,
        r#"\u0000\u001f\ufffd"#,
        r#"}{]["":,,"#,
    ];

    for injection in &injection_payloads {
        let json_inj = format_ai_stream_chunk(injection, false).expect("injection format");
        let parsed_inj: serde_json::Value = serde_json::from_str(&json_inj).expect("valid json");
        assert_eq!(parsed_inj["event"], "ai_stream_chunk");
        assert_eq!(parsed_inj["payload"]["textChunk"], *injection);
    }

    // 5. Massive payload: 4.8 Megabytes text chunk
    let massive_text = "LIVA_STREAM_CHUNK_TOKEN_".repeat(200_000); // 4.8 MB
    let start_huge = Instant::now();
    let json_massive = format_ai_stream_chunk(&massive_text, false).expect("massive format");
    let huge_elapsed = start_huge.elapsed();
    println!("Formatted 4.8MB text chunk in {:?}", huge_elapsed);

    let parsed_massive: serde_json::Value = serde_json::from_str(&json_massive).expect("valid json");
    assert_eq!(
        parsed_massive["payload"]["textChunk"].as_str().map(|s| s.len()),
        Some(massive_text.len())
    );

    // 6. High throughput burst: 50,000 chunks serialized
    let t0 = Instant::now();
    for i in 0..50_000 {
        let chunk = format_ai_stream_chunk("token", i % 2 == 0).unwrap();
        assert!(!chunk.is_empty());
    }
    let burst_elapsed = t0.elapsed();
    println!("50,000 stream chunk serializations in {:?} ({:.2} ns/chunk)", burst_elapsed, burst_elapsed.as_nanos() as f64 / 50_000.0);
}

// ============================================================================
// SUITE 3: speaker_frames Robustness, Boundary & Bit-Level Precision Stress
// ============================================================================

#[test]
fn test_challenger_speaker_frames_boundaries_and_extreme_parameters() {
    // 1. Empty samples slice
    let empty_frames = speaker_frames(1, 16000, &[]);
    assert!(empty_frames.is_empty(), "Empty input must return empty vec");

    // 2. Single sample
    let single_sample = [0.75f32];
    let single_frames = speaker_frames(10, 16000, &single_sample);
    assert_eq!(single_frames.len(), 1);
    assert_eq!(single_frames[0].op_code, OP_SPEAKER_OUT);
    assert_eq!(single_frames[0].seq_id, 0);
    assert_eq!(single_frames[0].payload.len(), 8 + 4);
    assert_eq!(speaker_turn_epoch(&single_frames[0]), Some(10));

    // Decode and verify single float sample bit-level equality
    let sample_bytes = &single_frames[0].payload[8..12];
    let decoded_f32 = f32::from_le_bytes(sample_bytes.try_into().unwrap());
    assert_eq!(decoded_f32, 0.75f32);

    // 3. Various standard and extreme sample rates
    let test_rates = [
        0u32,       // Extreme 0 (guarded by clamp to 1)
        1u32,       // Extreme 1
        8000u32,    // 8 kHz (Telephony)
        16000u32,   // 16 kHz (LIVA STT/TTS standard)
        24000u32,   // 24 kHz (VieNeu TTS standard)
        44100u32,   // 44.1 kHz (CD Audio)
        48000u32,   // 48 kHz (Pro Audio)
        96000u32,   // 96 kHz
        192000u32,  // 192 kHz
        u32::MAX,   // Extreme max (clamped to max_samples_per_frame)
    ];

    let dummy_audio = vec![0.1f32; 4800]; // 4800 samples

    for &rate in &test_rates {
        let frames = speaker_frames(42, rate, &dummy_audio);
        assert!(!frames.is_empty(), "Must produce frames for sample_rate={}", rate);

        for (seq, frame) in frames.iter().enumerate() {
            assert_eq!(frame.op_code, OP_SPEAKER_OUT);
            assert_eq!(frame.seq_id, seq as u32);
            assert!(frame.payload.len() >= 8, "Frame must have 8-byte header");
            assert!(frame.payload.len() <= 1024 * 1024, "Frame payload must not exceed 1MB");

            // Verify header contents
            let epoch_read = u32::from_le_bytes(frame.payload[0..4].try_into().unwrap());
            let rate_read = u32::from_le_bytes(frame.payload[4..8].try_into().unwrap());
            assert_eq!(epoch_read, 42);
            assert_eq!(rate_read, rate);
        }
    }

    // 4. Extreme Float Values: NaN, +Inf, -Inf, Min/Max floats
    let extreme_floats = vec![
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MIN,
        f32::MAX,
        f32::MIN_POSITIVE,
        -0.0f32,
        0.0f32,
    ];

    let extreme_frames = speaker_frames(99, 16000, &extreme_floats);
    assert_eq!(extreme_frames.len(), 1);
    let payload_audio = &extreme_frames[0].payload[8..];
    let reconstructed: &[f32] = bytemuck::cast_slice(payload_audio);
    assert_eq!(reconstructed.len(), extreme_floats.len());

    assert!(reconstructed[0].is_nan());
    assert_eq!(reconstructed[1], f32::INFINITY);
    assert_eq!(reconstructed[2], f32::NEG_INFINITY);
    assert_eq!(reconstructed[3], f32::MIN);
    assert_eq!(reconstructed[4], f32::MAX);
    assert_eq!(reconstructed[5], f32::MIN_POSITIVE);

    // 5. Massive Audio Stream: 1,000,000 samples (4 Megabytes of f32 PCM audio)
    let million_samples = vec![0.42f32; 1_000_000];
    let start_million = Instant::now();
    let million_frames = speaker_frames(777, 16000, &million_samples);
    let million_elapsed = start_million.elapsed();

    // At 16kHz, 100ms = 1600 samples. 1,000,000 / 1600 = 625 frames exactly.
    assert_eq!(million_frames.len(), 625);
    println!(
        "Processed 1,000,000 audio samples (625 frames, 4MB) in {:?}",
        million_elapsed
    );

    // Verify all sequence IDs are strictly continuous
    for (i, f) in million_frames.iter().enumerate() {
        assert_eq!(f.seq_id, i as u32);
        assert_eq!(f.payload.len(), 8 + 1600 * 4);
    }

    // 6. SpeakerEpochGate Integration
    let mut gate = SpeakerEpochGate::default();
    let frame_epoch_10 = &speaker_frames(10, 16000, &[0.1, 0.2])[0];
    let frame_epoch_20 = &speaker_frames(20, 16000, &[0.1, 0.2])[0];

    assert!(gate.accepts(frame_epoch_10));
    assert!(gate.accepts(frame_epoch_20));

    // Observe flush for epoch 15
    gate.observe_flush(15);
    assert!(!gate.accepts(frame_epoch_10), "Epoch 10 must be rejected after flush(15)");
    assert!(gate.accepts(frame_epoch_20), "Epoch 20 must be accepted after flush(15)");
}
