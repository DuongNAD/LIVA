//! Integration tests for runtime stability fixes (B1, B2, B5).

use liva_native_core::llm::embed::check_embed_tokens_fit;
use liva_native_core::llm::engine::check_prompt_fits;
use liva_native_core::llm::nen_sinh_tiep;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// B1: Embedding token length guard (check_embed_tokens_fit vs check_prompt_fits)
// ---------------------------------------------------------------------------

#[test]
fn test_b1_embedding_guard_allows_full_n_ctx_and_rejects_overflow() {
    let n_ctx = 2048;

    // Embedding không sinh thêm token nào nên cho phép dùng toàn bộ n_ctx:
    assert!(
        check_embed_tokens_fit(2048, n_ctx).is_ok(),
        "Embedding phai cho phep dung toi da bang n_ctx"
    );

    // check_prompt_fits (danh cho text gen) tru RESERVE_FOR_COMPLETION (512),
    // nen 2048 token se bi tu choi boi check_prompt_fits nhung phai duoc chap nhan boi check_embed_tokens_fit:
    assert!(
        check_prompt_fits(2048, n_ctx).is_err(),
        "Text generation phai tu choi vi can chua cho cho output"
    );

    // Khi vuot qua n_ctx, check_embed_tokens_fit phai tra ve loi ro rang thay vi de llama.cpp abort():
    let err = check_embed_tokens_fit(2049, n_ctx).unwrap_err();
    assert!(err.contains("2049 token"));
    assert!(err.contains("n_ctx = 2048"));
}

// ---------------------------------------------------------------------------
// B2: Early stream termination when receiver drops channel (gọi trực tiếp nen_sinh_tiep)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_b2_stream_callback_aborts_when_receiver_dropped() {
    let (tx, rx) = mpsc::channel::<String>(10);

    // Drop receiver giả lập client đóng tab / ngắt SSE
    drop(rx);

    // Gọi trực tiếp code sản xuất `nen_sinh_tiep` (dùng chung cho cả 4 call site):
    let callback_result = tokio::task::spawn_blocking(move || {
        let chunk = serde_json::json!({
            "event": "ai_stream_chunk",
            "payload": { "textChunk": "hello", "isThought": false }
        });
        nen_sinh_tiep(&tx, &chunk)
    })
    .await
    .expect("blocking task failed");

    assert!(
        !callback_result,
        "nen_sinh_tiep phai tra ve false khi client da ngat ket noi de giai phong lock"
    );
}

// ---------------------------------------------------------------------------
// B2: Stream callback continues when receiver is healthy (gọi trực tiếp nen_sinh_tiep)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_b2_stream_callback_continues_when_receiver_alive() {
    let (tx, mut rx) = mpsc::channel::<String>(10);

    // Gọi trực tiếp code sản xuất `nen_sinh_tiep`:
    let callback_result = tokio::task::spawn_blocking(move || {
        let chunk = serde_json::json!({
            "event": "ai_stream_chunk",
            "payload": { "textChunk": "hello", "isThought": false }
        });
        nen_sinh_tiep(&tx, &chunk)
    })
    .await
    .expect("blocking task failed");

    assert!(
        callback_result,
        "nen_sinh_tiep phai tra ve true khi client van dang nhan stream"
    );

    let received = rx.recv().await.expect("phai nhan duoc chunk");
    assert!(received.contains("hello"));
}
