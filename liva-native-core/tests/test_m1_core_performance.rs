use bytes::BytesMut;
use liva_native_core::db::{
    DatabasePool, MetadataFilter, WalCheckpointMode, search_similar_vectors, upsert_vector,
};
use liva_native_core::llm::engine::compute_common_prefix_len;
use liva_native_core::webrtc::frame::{
    BufferPool, OP_MIC_IN, OP_SPEAKER_OUT, VoiceFrame, speaker_frames,
};
use liva_native_core::websocket::format_ai_stream_chunk;
use llama_cpp_2::token::LlamaToken;
use std::sync::Arc;

#[test]
fn test_m1_buffer_pool_and_zero_copy_encode_into() {
    let pool = BufferPool::new(4096, 16);
    assert_eq!(pool.idle_count(), 0);

    // 1. Acquire pooled buffer and encode frame directly into it
    let mut pooled = pool.acquire_buffer();
    let payload_data = b"PCM audio payload 16khz test data";
    VoiceFrame::encode_into(&mut pooled, OP_SPEAKER_OUT, 42, payload_data)
        .expect("encode_into must succeed");

    let encoded_bytes = pooled.into_bytes();
    assert_eq!(encoded_bytes.len(), 9 + payload_data.len());
    assert_eq!(encoded_bytes[0], OP_SPEAKER_OUT);

    // 2. Decode and verify payload integrity
    let mut decode_buf = BytesMut::from(&encoded_bytes[..]);
    let frame = VoiceFrame::decode(&mut decode_buf)
        .expect("decode must succeed")
        .expect("frame must be complete");

    assert_eq!(frame.op_code, OP_SPEAKER_OUT);
    assert_eq!(frame.seq_id, 42);
    assert_eq!(&frame.payload[..], payload_data);

    // 3. Verify global BufferPool acquire and into_bytes interface contracts
    let mut global_pooled = BufferPool::acquire();
    VoiceFrame::encode_into(&mut global_pooled, OP_MIC_IN, 100, b"mic input")
        .expect("global encode_into must succeed");
    let global_bytes = global_pooled.into_bytes();
    assert_eq!(global_bytes[0], OP_MIC_IN);
}

#[test]
fn test_m1_format_ai_stream_chunk_compact_json() {
    let token_chunk = "Xin chào các bạn!";
    let json_output = format_ai_stream_chunk(token_chunk, false)
        .expect("serialization must succeed");

    assert!(json_output.contains("\"event\":\"ai_stream_chunk\""));
    assert!(json_output.contains("\"textChunk\":\"Xin chào các bạn!\""));
    assert!(json_output.contains("\"isThought\":false"));
}

#[test]
fn test_m1_speaker_frames_fast_serialization() {
    let turn_epoch = 101;
    let sample_rate = 16000;
    let samples = vec![0.5f32; 3200]; // 200ms of audio = 2 chunks of 100ms (1600 samples each)

    let frames = speaker_frames(turn_epoch, sample_rate, &samples);
    assert_eq!(frames.len(), 2);
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame.op_code, OP_SPEAKER_OUT);
        assert_eq!(frame.seq_id, i as u32);
        assert_eq!(frame.payload.len(), 8 + 1600 * 4);
    }
}

#[tokio::test]
async fn test_m1_sqlite_wal_checkpoint_and_high_reader_concurrency() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db must initialize");

    // 1. Perform write operation
    {
        let conn = pool.writer.get().expect("acquire writer connection");
        conn.execute(
            "INSERT OR REPLACE INTO facts (key, value, createdAt, updatedAt, source) \
             VALUES ('perf_test_key', 'perf_test_value', '2026-09-01', '2026-09-01', 'test')",
            [],
        )
        .expect("insert fact must succeed");
    }

    // 2. Perform WAL checkpoint on writer connection
    let checkpoint_res = pool.checkpoint().expect("checkpoint must succeed");
    assert_eq!(checkpoint_res.busy, 0);

    let truncate_res = pool
        .wal_checkpoint(WalCheckpointMode::Truncate)
        .expect("truncate checkpoint must succeed");
    assert_eq!(truncate_res.busy, 0);

    // 3. Test high reader concurrency (8 concurrent tasks reading concurrently)
    let pool_arc = Arc::new(pool);
    let mut handles = Vec::new();

    for task_id in 0..16 {
        let pool_clone = Arc::clone(&pool_arc);
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                let conn = pool_clone.readers.get().expect("reader connection");
                let count: i64 = conn
                    .query_row("SELECT count(*) FROM facts", [], |row| row.get(0))
                    .expect("query must succeed");
                assert!(count >= 1, "task {} read count = {}", task_id, count);
            }
        }));
    }

    for handle in handles {
        handle.await.expect("reader task must complete cleanly");
    }
}

#[test]
fn test_m1_vector_query_speedup_unfiltered_and_filtered() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db must initialize");
    let crypto =
        liva_native_core::crypto::EncryptionEngine::new("0123456789abcdef0123456789abcdef");

    let conn = pool.writer.get().expect("acquire writer");

    // Insert 5 test vectors
    for i in 0..5 {
        let mut vec = vec![0.0f32; 384];
        vec[i] = 1.0f32;
        let domain = if i % 2 == 0 { "Personal" } else { "Work" };
        let category = if i % 2 == 0 { "Finance" } else { "Projects" };

        upsert_vector(
            &conn,
            &crypto,
            &format!("vec_{}", i),
            "fact",
            &format!("Content for vector {}", i),
            &vec,
            Some(domain),
            Some(category),
            None,
            None,
            None,
        )
        .expect("upsert_vector must succeed");
    }

    // 1. Unfiltered search (fast path with eliminated subquery)
    let query_vec = {
        let mut v = vec![0.0f32; 384];
        v[0] = 1.0f32;
        v
    };
    let results_unfiltered = search_similar_vectors(
        &conn,
        &crypto,
        &query_vec,
        3,
        &MetadataFilter::default(),
    )
    .expect("unfiltered search must succeed");

    assert!(!results_unfiltered.is_empty());
    assert_eq!(results_unfiltered[0].vec_id, "vec_0");

    // 2. Filtered search (using covering indices)
    let filtered_domain = MetadataFilter {
        domain: Some("Personal".to_string()),
        ..Default::default()
    };
    let results_filtered = search_similar_vectors(
        &conn,
        &crypto,
        &query_vec,
        3,
        &filtered_domain,
    )
    .expect("filtered search must succeed");

    assert!(!results_filtered.is_empty());
    for r in &results_filtered {
        assert_eq!(r.domain, "Personal");
    }
}

#[test]
fn test_m1_kv_cache_prefix_reuse_token_matching() {
    let t_sys1 = LlamaToken(101);
    let t_sys2 = LlamaToken(102);
    let t_sys3 = LlamaToken(103);
    let t_user1 = LlamaToken(201);
    let t_user2 = LlamaToken(202);
    let t_user3 = LlamaToken(203);

    // Multi-session scenario:
    // Session A prompt: [System1, System2, System3, UserA1, UserA2]
    let session_a_tokens = vec![t_sys1, t_sys2, t_sys3, t_user1, t_user2];

    // Session B prompt with same system prompt prefix: [System1, System2, System3, UserB1]
    let session_b_tokens = vec![t_sys1, t_sys2, t_sys3, t_user3];

    // Check common prefix calculation
    let common_prefix_len = compute_common_prefix_len(&session_a_tokens, &session_b_tokens);
    assert_eq!(
        common_prefix_len, 3,
        "System prompt prefix of length 3 must match exactly"
    );

    // Completely disjoint session prompt
    let session_c_tokens = vec![t_user1, t_user2];
    assert_eq!(
        compute_common_prefix_len(&session_a_tokens, &session_c_tokens),
        0,
        "Disjoint prompt must have prefix length 0"
    );

    // Identical prompt
    assert_eq!(
        compute_common_prefix_len(&session_a_tokens, &session_a_tokens),
        5,
        "Identical prompt must have 100% prefix match"
    );
}
