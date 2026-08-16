use serde::{Deserialize, Serialize};

pub const OPEN_TAGS: &[&str] = &[
    "<think>",
    "<thought>",
    "<analysis>",
    "<reasoning>",
    "<channel_thought>",
    "<|channel>analysis",
    "<|channel>thought",
    "<|channel|>analysis<|message|>",
    "<|channel|>analysis",
    "<|channel|>thought<|message|>",
    "<|channel|>thought",
];

pub const CLOSE_TAGS: &[&str] = &[
    "</think>",
    "</thought>",
    "</analysis>",
    "</reasoning>",
    "</channel_thought>",
    "<|channel>final",
    "<|channel|>final<|message|>",
    "<|channel|>final",
];

/// Strongly-typed stream chunk representing demuxed token channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum StreamChunk {
    /// Internal reasoning token piece (from inside <think>...</think>).
    Reasoning(String),
    /// User-visible content piece.
    VisibleText(String),
    /// Control signal / heartbeat.
    Heartbeat,
}

/// Tauri IPC streaming payload matching the frontend `ai_stream_chunk` event format for `WidgetApp.vue`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TauriIpcChunk {
    #[serde(rename = "textChunk")]
    pub text_chunk: String,
    #[serde(rename = "isThought")]
    pub is_thought: bool,
}

impl StreamChunk {
    /// Convert a stream chunk to a Tauri IPC compatible payload if applicable.
    pub fn to_tauri_ipc_chunk(&self) -> Option<TauriIpcChunk> {
        match self {
            StreamChunk::Reasoning(s) => Some(TauriIpcChunk {
                text_chunk: s.clone(),
                is_thought: true,
            }),
            StreamChunk::VisibleText(s) => Some(TauriIpcChunk {
                text_chunk: s.clone(),
                is_thought: false,
            }),
            StreamChunk::Heartbeat => None,
        }
    }
}

/// Real-time streaming Chain-of-Thought (CoT) and Visible Text Demuxer.
#[derive(Debug, Default)]
pub struct ReasoningStreamSplitter {
    in_thought: bool,
    pending: String,
    byte_buffer: Vec<u8>,
}

impl ReasoningStreamSplitter {
    /// Create a new splitter starting in visible text mode.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a splitter with explicit initial reasoning state.
    pub fn with_initial_thought(in_thought: bool) -> Self {
        Self {
            in_thought,
            pending: String::new(),
            byte_buffer: Vec::new(),
        }
    }

    /// Detect chat templates that already opened a reasoning channel in the
    /// assistant prefix (e.g. `assistant\n<think>\n`), so generation begins in thought mode.
    pub fn from_prompt_tail(prompt: &str) -> Self {
        let prompt = prompt.trim_end().to_ascii_lowercase();
        let in_thought = OPEN_TAGS.iter().any(|marker| prompt.ends_with(marker));
        Self {
            in_thought,
            pending: String::new(),
            byte_buffer: Vec::new(),
        }
    }

    /// Return true if the demuxer is currently inside a reasoning block.
    pub fn is_in_thought(&self) -> bool {
        self.in_thought
    }

    /// Process a stream token string and produce typed `StreamChunk`s.
    pub fn process_token(&mut self, token: &str) -> Vec<StreamChunk> {
        self.pending.push_str(token);
        self.extract_chunks()
    }

    /// Process raw streaming bytes with UTF-8 split boundary resilience.
    pub fn process_bytes(&mut self, bytes: &[u8]) -> Vec<StreamChunk> {
        self.byte_buffer.extend_from_slice(bytes);

        // Find longest valid UTF-8 prefix in byte_buffer
        let mut valid_up_to = 0;
        let total_len = self.byte_buffer.len();

        while valid_up_to < total_len {
            match std::str::from_utf8(&self.byte_buffer[valid_up_to..]) {
                Ok(_) => {
                    valid_up_to = total_len;
                    break;
                }
                Err(e) => {
                    let valid_len = e.valid_up_to();
                    valid_up_to += valid_len;
                    if e.error_len().is_none() {
                        // Incomplete UTF-8 character at the end of the buffer (1-3 bytes)
                        break;
                    } else {
                        // Invalid byte: skip it safely
                        valid_up_to += 1;
                    }
                }
            }
        }

        if valid_up_to > 0 {
            if let Ok(valid_str) = std::str::from_utf8(&self.byte_buffer[..valid_up_to]) {
                self.pending.push_str(valid_str);
            }
            self.byte_buffer.drain(..valid_up_to);
        }

        self.extract_chunks()
    }

    /// Extract chunks from `self.pending`.
    fn extract_chunks(&mut self) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();

        loop {
            let markers = if self.in_thought {
                CLOSE_TAGS
            } else {
                OPEN_TAGS
            };

            if let Some((pos, len)) = earliest_marker(&self.pending, markers) {
                if pos > 0 {
                    let text = self.pending[..pos].to_string();
                    if self.in_thought {
                        chunks.push(StreamChunk::Reasoning(text));
                    } else {
                        chunks.push(StreamChunk::VisibleText(text));
                    }
                }
                self.pending.drain(..pos + len);
                self.in_thought = !self.in_thought;
                continue;
            }

            let retained = marker_prefix_suffix_len(&self.pending, markers);
            let mut emit_len = self.pending.len().saturating_sub(retained);
            emit_len = floor_char_boundary(&self.pending, emit_len);

            if emit_len > 0 {
                let text = self.pending[..emit_len].to_string();
                if self.in_thought {
                    chunks.push(StreamChunk::Reasoning(text));
                } else {
                    chunks.push(StreamChunk::VisibleText(text));
                }
                self.pending.drain(..emit_len);
            }
            break;
        }

        chunks
    }

    /// Flush remaining buffer at EOF with fail-closed protocol tag protection.
    pub fn flush(&mut self) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();

        // Process any remaining bytes if valid UTF-8
        if !self.byte_buffer.is_empty() {
            if let Ok(s) = std::str::from_utf8(&self.byte_buffer) {
                self.pending.push_str(s);
            }
            self.byte_buffer.clear();
        }

        if !self.pending.is_empty() {
            if self.in_thought {
                // If in thought mode and pending ends with a partial close tag, strip that partial tag
                let lower = self.pending.to_ascii_lowercase();
                let mut matched_len = 0;
                for close_tag in CLOSE_TAGS {
                    for len in (1..close_tag.len()).rev() {
                        if lower.ends_with(&close_tag[..len]) && len > matched_len {
                            matched_len = len;
                        }
                    }
                }
                let mut emit_len = self.pending.len().saturating_sub(matched_len);
                emit_len = floor_char_boundary(&self.pending, emit_len);
                if emit_len > 0 {
                    chunks.push(StreamChunk::Reasoning(self.pending[..emit_len].to_string()));
                }
            } else {
                // If in visible mode, check if pending is an ambiguous open tag prefix
                let lower = self.pending.to_ascii_lowercase();
                let is_open_prefix = OPEN_TAGS.iter().any(|m| m.starts_with(&lower));
                if !is_open_prefix {
                    chunks.push(StreamChunk::VisibleText(std::mem::take(&mut self.pending)));
                }
            }
            self.pending.clear();
        }

        chunks
    }
}

/// Backward-compatible filter that exposes only visible text to downstream consumers.
#[derive(Debug, Default)]
pub struct VisibleOutputFilter {
    splitter: ReasoningStreamSplitter,
}

impl VisibleOutputFilter {
    /// Detect chat templates that already opened a reasoning channel in the
    /// assistant prefix, so the first generated token remains private.
    pub fn from_prompt_tail(prompt: &str) -> Self {
        Self {
            splitter: ReasoningStreamSplitter::from_prompt_tail(prompt),
        }
    }

    /// Consume one raw token piece and return only user-visible output.
    pub fn push(&mut self, chunk: &str) -> String {
        let chunks = self.splitter.process_token(chunk);
        let mut visible = String::new();
        for piece in chunks {
            if let StreamChunk::VisibleText(s) = piece {
                visible.push_str(&s);
            }
        }
        visible
    }

    /// Finish fail-closed: an unfinished control-marker prefix is discarded.
    pub fn finish(&mut self) -> String {
        let chunks = self.splitter.flush();
        let mut visible = String::new();
        for piece in chunks {
            if let StreamChunk::VisibleText(s) = piece {
                visible.push_str(&s);
            }
        }
        visible
    }
}

pub(crate) fn forward_filtered_piece<F>(
    filter: &mut VisibleOutputFilter,
    piece: &str,
    callback: &mut F,
) -> bool
where
    F: FnMut(&str) -> bool,
{
    let visible = filter.push(piece);
    callback(&visible)
}

fn floor_char_boundary(s: &str, mut index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    while index > 0 && !s.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn earliest_marker(input: &str, markers: &[&str]) -> Option<(usize, usize)> {
    let input = input.to_ascii_lowercase();
    markers
        .iter()
        .filter_map(|marker| input.find(marker).map(|position| (position, marker.len())))
        .min_by_key(|(position, _)| *position)
}

fn marker_prefix_suffix_len(input: &str, markers: &[&str]) -> usize {
    let input = input.to_ascii_lowercase();
    let input = input.as_bytes();
    markers
        .iter()
        .map(|marker| marker.as_bytes())
        .flat_map(|marker| {
            let max_len = input.len().min(marker.len().saturating_sub(1));
            (1..=max_len)
                .rev()
                .find(|&len| input.ends_with(&marker[..len]))
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thought_tag_bi_chia_nhieu_chunk_khong_lo_noi_suy_luan() {
        let mut filter = VisibleOutputFilter::default();
        let mut visible = String::new();

        for chunk in ["<thi", "nk>bi mat", "</th", "ink>Ket ", "qua"] {
            visible.push_str(&filter.push(chunk));
        }
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Ket qua");
    }

    #[test]
    fn analysis_channel_bi_an_con_final_channel_duoc_giu() {
        let mut filter = VisibleOutputFilter::default();
        let mut visible = String::new();

        for chunk in [
            "<|chan",
            "nel|>analysis<|message|>secret",
            "<|channel|>fi",
            "nal<|message|>Answer",
        ] {
            visible.push_str(&filter.push(chunk));
        }
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Answer");
    }

    #[test]
    fn cac_tag_reasoning_da_co_trong_client_deu_bi_an_khong_phan_biet_hoa_thuong() {
        for raw in [
            "<THOUGHT>secret</THOUGHT>Answer",
            "<|channel>thoughtsecret</channel_thought>Answer",
            "<analysis>secret</analysis>Answer",
            "<reasoning>secret</reasoning>Answer",
        ] {
            let mut filter = VisibleOutputFilter::default();
            let mut visible = filter.push(raw);
            visible.push_str(&filter.finish());
            assert_eq!(visible, "Answer", "raw={raw}");
        }
    }

    #[test]
    fn callback_van_duoc_goi_de_huy_turn_trong_luc_model_dang_suy_luan_an() {
        let mut filter = VisibleOutputFilter::default();
        let mut callbacks = 0;

        let keep_running = forward_filtered_piece(&mut filter, "<think>secret", &mut |visible| {
            callbacks += 1;
            assert!(visible.is_empty());
            false
        });

        assert!(!keep_running);
        assert_eq!(callbacks, 1);
    }

    #[test]
    fn prompt_da_mo_think_channel_thi_token_dau_tien_mac_dinh_la_noi_bo() {
        let mut filter = VisibleOutputFilter::from_prompt_tail("assistant\n<THINK>\n");
        let mut visible = filter.push("secret reasoning</think>Final answer");
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Final answer");
    }

    #[test]
    fn stream_ket_thuc_giua_control_tag_thi_bo_fail_closed() {
        let mut filter = VisibleOutputFilter::default();
        let mut visible = filter.push("Answer<thi");
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Answer");
    }

    #[test]
    fn splitter_multi_byte_vietnamese_characters_no_panic() {
        let mut splitter = ReasoningStreamSplitter::new();
        // Vietnamese string: "Đang suy nghĩ về giải pháp..."
        let pieces = [
            "Đang",
            " ",
            "<think>",
            "suy nghĩ",
            " ",
            "về",
            " ",
            "giải pháp",
            "</think>",
            "Xong!",
        ];

        let mut emitted = Vec::new();
        for p in pieces {
            emitted.extend(splitter.process_token(p));
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

        assert_eq!(visible, "Đang Xong!");
        assert_eq!(reasoning, "suy nghĩ về giải pháp");
    }

    #[test]
    fn splitter_byte_by_byte_utf8_streaming_no_panic() {
        let mut splitter = ReasoningStreamSplitter::new();
        let input = "Chào <think>đang tính toán</think> bạn nhé!";
        let mut emitted = Vec::new();

        for b in input.as_bytes() {
            emitted.extend(splitter.process_bytes(&[*b]));
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

        assert_eq!(visible, "Chào  bạn nhé!");
        assert_eq!(reasoning, "đang tính toán");
    }
}
