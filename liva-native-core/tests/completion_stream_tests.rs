use liva_native_core::llm::engine::{CompletionOutput, CompletionStream};

fn completion() -> CompletionOutput {
    CompletionOutput {
        text: "answer".into(),
        prompt_tokens: 10,
        completion_tokens: 1,
    }
}

#[test]
fn open_heartbeat_emits_nothing_and_visible_chunk_keeps_wire_shape() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);
    let mut stream = CompletionStream::new(&tx);
    assert!(stream.forward("", &serde_json::json!({"token": ""})));
    assert!(rx.try_recv().is_err());
    let chunk = serde_json::json!({"token": "answer", "done": false});
    assert!(stream.forward("answer", &chunk));
    assert_eq!(
        rx.try_recv().unwrap(),
        serde_json::to_string(&chunk).unwrap()
    );
    assert_eq!(stream.finish(Ok(completion())).unwrap().text, "answer");
}

#[test]
fn closed_stream_never_reports_partial_completion_as_success() {
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(1);
    let mut stream = CompletionStream::new(&tx);
    drop(rx);
    assert!(!stream.forward("answer", &"answer"));
    assert!(stream.finish(Ok(completion())).is_err());
}

#[test]
fn stream_preserves_inference_errors_without_success_envelope() {
    let (tx, _rx) = tokio::sync::mpsc::channel::<String>(1);
    let stream = CompletionStream::new(&tx);
    assert_eq!(
        stream.finish(Err("budget exceeded".into())).unwrap_err(),
        "budget exceeded"
    );
}

#[test]
fn closed_channel_cancels_hidden_reasoning_heartbeat() {
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(1);
    drop(rx);
    let mut stream = CompletionStream::new(&tx);
    assert!(!stream.forward("", &serde_json::json!({"token": "", "done": false})));
}
