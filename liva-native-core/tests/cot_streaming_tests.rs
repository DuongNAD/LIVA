//! Integration tests for Real-time Streaming CoT Demuxer and Tauri UI Stream Binding
//! (Features F7, F9).

use liva_native_core::llm::output_filter::{
    ReasoningStreamSplitter, StreamChunk, TauriIpcChunk, VisibleOutputFilter,
};

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (F7, F9)
// ---------------------------------------------------------------------------

#[test]
fn test_standard_think_separation() {
    let mut splitter = ReasoningStreamSplitter::new();
    let chunks = splitter
        .process_token("Xin chào! <think>đang phân tích yêu cầu</think> Tôi có thể giúp gì?");
    let mut final_chunks = chunks;
    final_chunks.extend(splitter.flush());

    assert_eq!(
        final_chunks,
        vec![
            StreamChunk::VisibleText("Xin chào! ".into()),
            StreamChunk::Reasoning("đang phân tích yêu cầu".into()),
            StreamChunk::VisibleText(" Tôi có thể giúp gì?".into()),
        ]
    );
}

#[test]
fn test_token_split_control_markers() {
    let mut splitter = ReasoningStreamSplitter::new();
    let input_tokens = ["<th", "ink>suy", " nghĩ 1", "</th", "ink>Ket qua"];

    let mut emitted = Vec::new();
    for token in input_tokens {
        emitted.extend(splitter.process_token(token));
    }
    emitted.extend(splitter.flush());

    let reasoning_text: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::Reasoning(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let visible_text: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::VisibleText(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(reasoning_text, "suy nghĩ 1");
    assert_eq!(visible_text, "Ket qua");
}

#[test]
fn test_multichannel_tags() {
    let mut splitter = ReasoningStreamSplitter::new();
    let raw =
        "<|channel|>analysis<|message|>internal strategy<|channel|>final<|message|>User response";

    let mut emitted = splitter.process_token(raw);
    emitted.extend(splitter.flush());

    assert_eq!(
        emitted,
        vec![
            StreamChunk::Reasoning("internal strategy".into()),
            StreamChunk::VisibleText("User response".into()),
        ]
    );
}

#[test]
fn test_prompt_tail_preopened_reasoning() {
    let mut splitter = ReasoningStreamSplitter::from_prompt_tail("assistant\n<THINK>\n");
    assert!(splitter.is_in_thought());
    let mut emitted = splitter.process_token("bước đầu tiên</think>Kết quả hoàn tất");
    emitted.extend(splitter.flush());

    assert_eq!(
        emitted,
        vec![
            StreamChunk::Reasoning("bước đầu tiên".into()),
            StreamChunk::VisibleText("Kết quả hoàn tất".into()),
        ]
    );
}

#[test]
fn test_case_insensitive_demuxing() {
    for (open, close) in [
        ("<THINK>", "</THINK>"),
        ("<Thought>", "</Thought>"),
        ("<ANALYSIS>", "</ANALYSIS>"),
        ("<Reasoning>", "</Reasoning>"),
    ] {
        let mut splitter = ReasoningStreamSplitter::new();
        let stream_text = format!("Start {open}private logic{close} Finish");
        let mut emitted = splitter.process_token(&stream_text);
        emitted.extend(splitter.flush());

        assert_eq!(
            emitted,
            vec![
                StreamChunk::VisibleText("Start ".into()),
                StreamChunk::Reasoning("private logic".into()),
                StreamChunk::VisibleText(" Finish".into()),
            ],
            "Failed for tags: {open} ... {close}"
        );
    }
}

#[test]
fn test_tauri_ipc_chunk_serialization() {
    let reasoning_ipc = TauriIpcChunk {
        text_chunk: "suy nghĩ bước 1...".to_string(),
        is_thought: true,
    };

    let json_str = serde_json::to_string(&reasoning_ipc).expect("serialized");
    assert!(json_str.contains("\"textChunk\":\"suy nghĩ bước 1...\""));
    assert!(json_str.contains("\"isThought\":true"));

    let deserialized: TauriIpcChunk = serde_json::from_str(&json_str).expect("deserialized");
    assert_eq!(reasoning_ipc, deserialized);
}

#[test]
fn test_tauri_ipc_conversion_method() {
    let reasoning = StreamChunk::Reasoning("logic trace".to_string());
    let visible = StreamChunk::VisibleText("hello user".to_string());
    let heartbeat = StreamChunk::Heartbeat;

    let ipc_reasoning = reasoning.to_tauri_ipc_chunk().expect("ipc reasoning");
    assert_eq!(ipc_reasoning.text_chunk, "logic trace");
    assert!(ipc_reasoning.is_thought);

    let ipc_visible = visible.to_tauri_ipc_chunk().expect("ipc visible");
    assert_eq!(ipc_visible.text_chunk, "hello user");
    assert!(!ipc_visible.is_thought);

    assert!(heartbeat.to_tauri_ipc_chunk().is_none());
}

// ---------------------------------------------------------------------------
// TIER 2: BOUNDARY & CORNER CASES (F7, F9)
// ---------------------------------------------------------------------------

#[test]
fn test_unclosed_think_tag_at_eof() {
    let mut splitter = ReasoningStreamSplitter::new();
    let mut emitted = splitter.process_token("Visible before <think>incomplete thought");
    emitted.extend(splitter.flush());

    let visible: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::VisibleText(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let reasoning: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::Reasoning(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(visible, "Visible before ");
    assert_eq!(reasoning, "incomplete thought");
    assert!(!visible.contains("<think>"));
}

#[test]
fn test_single_byte_marker_streaming() {
    let mut splitter = ReasoningStreamSplitter::new();
    let full_text = "ABC<think>123</think>XYZ";

    let mut emitted = Vec::new();
    for ch in full_text.chars() {
        let s = ch.to_string();
        emitted.extend(splitter.process_token(&s));
    }
    emitted.extend(splitter.flush());

    let visible: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::VisibleText(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let reasoning: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::Reasoning(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(visible, "ABCXYZ");
    assert_eq!(reasoning, "123");
}

#[test]
fn test_nested_or_consecutive_tags() {
    let mut splitter = ReasoningStreamSplitter::new();
    let raw = "<think>r1</think><think>r2</think>Visible";
    let mut emitted = splitter.process_token(raw);
    emitted.extend(splitter.flush());

    let reasoning_parts: Vec<_> = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::Reasoning(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(reasoning_parts, vec!["r1", "r2"]);
}

#[test]
fn test_vietnamese_unicode_boundary_streaming() {
    let mut splitter = ReasoningStreamSplitter::new();
    // Test multi-byte Vietnamese Unicode characters across chunk boundaries
    let chunks = [
        "Đang ",
        "xử ",
        "lý: <th",
        "ink>tiếp ",
        "tục suy ",
        "nghĩ về ",
        "giải ",
        "pháp</th",
        "ink>Hoàn ",
        "tất!",
    ];

    let mut emitted = Vec::new();
    for chunk in chunks {
        emitted.extend(splitter.process_token(chunk));
    }
    emitted.extend(splitter.flush());

    let visible: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::VisibleText(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let reasoning: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::Reasoning(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(visible, "Đang xử lý: Hoàn tất!");
    assert_eq!(reasoning, "tiếp tục suy nghĩ về giải pháp");
}

#[test]
fn test_byte_level_streaming_with_split_utf8_continuation_bytes() {
    let mut splitter = ReasoningStreamSplitter::new();
    let full_string =
        "Xin chào các bạn <think>đang tính toán logic phức tạp: 1 + 1 = 2</think> kết quả là 2.";

    let mut emitted = Vec::new();
    // Feed one raw byte at a time (splitting multi-byte UTF-8 sequences in the middle)
    for byte in full_string.as_bytes() {
        emitted.extend(splitter.process_bytes(&[*byte]));
    }
    emitted.extend(splitter.flush());

    let visible: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::VisibleText(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let reasoning: String = emitted
        .iter()
        .filter_map(|c| match c {
            StreamChunk::Reasoning(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(visible, "Xin chào các bạn  kết quả là 2.");
    assert_eq!(reasoning, "đang tính toán logic phức tạp: 1 + 1 = 2");
}

#[test]
fn test_backward_compatible_visible_output_filter() {
    let mut filter = VisibleOutputFilter::from_prompt_tail("assistant\n<THINK>\n");
    let mut visible = filter.push("internal reasoning</think>Public response");
    visible.push_str(&filter.finish());
    assert_eq!(visible, "Public response");
}
