const OPEN_MARKERS: &[&str] = &[
    "<think>",
    "<thought>",
    "<analysis>",
    "<reasoning>",
    "<channel_thought>",
    "<antthinking>",
    "<reflection>",
    "<|thought|>",
    "<|think|>",
    "<|reasoning|>",
    "<|channel_thought|>",
    "<|channel|>analysis<|message|>",
    "<|channel|>thought<|message|>",
    "<|channel|>reasoning<|message|>",
    "<|channel|>internal<|message|>",
    "<|channel|>commentary<|message|>",
    "<|channel|>cot<|message|>",
    "<|channel>analysis",
    "<|channel>thought",
    "<|channel>reasoning",
    "<|channel>internal",
    "<|channel>commentary",
    "<|channel>cot",
];
const CLOSE_MARKERS: &[&str] = &[
    "</think>",
    "</thought>",
    "</analysis>",
    "</reasoning>",
    "</channel_thought>",
    "</antthinking>",
    "</reflection>",
    "<|/thought|>",
    "<|/think|>",
    "<|/reasoning|>",
    "<|/channel_thought|>",
    "<|end_of_thought|>",
    "<|end_thought|>",
    "<|channel|>final<|message|>",
    "<|channel|>output<|message|>",
    "<|channel|>response<|message|>",
    "<|channel|>main<|message|>",
    "<|channel|>assistant<|message|>",
    "<|channel>final",
    "<|channel>output",
    "<|channel>response",
    "<|channel>main",
    "<|channel>assistant",
];

#[derive(Debug, Default)]
pub struct VisibleOutputFilter {
    hidden: bool,
    pending: String,
}

impl VisibleOutputFilter {
    /// Detect chat templates that already opened a reasoning channel in the
    /// assistant prefix, so the first generated token remains private.
    pub fn from_prompt_tail(prompt: &str) -> Self {
        let prompt = prompt.trim_end().to_ascii_lowercase();
        Self {
            hidden: OPEN_MARKERS.iter().any(|marker| prompt.ends_with(marker)),
            pending: String::new(),
        }
    }

    /// Consume one raw token piece and return only user-visible output.
    pub fn push(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        let mut visible = String::new();

        loop {
            let markers = if self.hidden {
                CLOSE_MARKERS
            } else {
                OPEN_MARKERS
            };
            if let Some((position, marker_len)) = earliest_marker(&self.pending, markers) {
                if !self.hidden {
                    visible.push_str(&self.pending[..position]);
                }
                self.pending.drain(..position + marker_len);
                self.hidden = !self.hidden;
                continue;
            }

            let retained = marker_prefix_suffix_len(&self.pending, markers);
            let emit_len = self.pending.len() - retained;
            if !self.hidden {
                visible.push_str(&self.pending[..emit_len]);
            }
            self.pending.drain(..emit_len);
            break;
        }

        visible
    }

    /// Finish fail-closed: an unfinished control-marker prefix is discarded.
    pub fn finish(&mut self) -> String {
        // `push()` only retains a suffix when it could be the beginning of a
        // control marker split across tokens. At EOF that fragment is
        // ambiguous, so discard it rather than exposing protocol text.
        self.pending.clear();
        String::new()
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

fn earliest_marker(input: &str, markers: &[&str]) -> Option<(usize, usize)> {
    let input = input.to_ascii_lowercase();
    markers
        .iter()
        .filter_map(|marker| input.find(marker).map(|position| (position, marker.len())))
        .min_by_key(|(position, len)| (*position, std::cmp::Reverse(*len)))
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
    #[test]
    fn thought_tag_bi_chia_nhieu_chunk_khong_lo_noi_suy_luan() {
        let mut filter = super::VisibleOutputFilter::default();
        let mut visible = String::new();

        for chunk in ["<thi", "nk>bi mat", "</th", "ink>Ket ", "qua"] {
            visible.push_str(&filter.push(chunk));
        }
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Ket qua");
    }

    #[test]
    fn analysis_channel_bi_an_con_final_channel_duoc_giu() {
        let mut filter = super::VisibleOutputFilter::default();
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
            let mut filter = super::VisibleOutputFilter::default();
            let mut visible = filter.push(raw);
            visible.push_str(&filter.finish());
            assert_eq!(visible, "Answer", "raw={raw}");
        }
    }

    #[test]
    fn callback_van_duoc_goi_de_huy_turn_trong_luc_model_dang_suy_luan_an() {
        let mut filter = super::VisibleOutputFilter::default();
        let mut callbacks = 0;

        let keep_running =
            super::forward_filtered_piece(&mut filter, "<think>secret", &mut |visible| {
                callbacks += 1;
                assert!(visible.is_empty());
                false
            });

        assert!(!keep_running);
        assert_eq!(callbacks, 1);
    }

    #[test]
    fn prompt_da_mo_think_channel_thi_token_dau_tien_mac_dinh_la_noi_bo() {
        let mut filter = super::VisibleOutputFilter::from_prompt_tail("assistant\n<THINK>\n");
        let mut visible = filter.push("secret reasoning</think>Final answer");
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Final answer");
    }

    #[test]
    fn stream_ket_thuc_giua_control_tag_thi_bo_fail_closed() {
        let mut filter = super::VisibleOutputFilter::default();
        let mut visible = filter.push("Answer<thi");
        visible.push_str(&filter.finish());

        assert_eq!(visible, "Answer");
    }

    #[test]
    fn test_extended_reasoning_tags_and_channels_filtering() {
        let test_cases = [
            ("<|thought|>internal reasoning<|/thought|>Visible text", "Visible text"),
            ("<|think|>step 1<|/think|>Visible text", "Visible text"),
            ("<|reasoning|>logic<|/reasoning|>Visible text", "Visible text"),
            ("<|channel_thought|>deep logic<|/channel_thought|>Visible text", "Visible text"),
            ("<|thought|>deep logic<|end_of_thought|>Visible text", "Visible text"),
            ("<|think|>deep logic<|end_thought|>Visible text", "Visible text"),
            ("<antThinking>internal thoughts</antThinking>Visible text", "Visible text"),
            ("<reflection>evaluating approach</reflection>Visible text", "Visible text"),
            ("<|channel|>thought<|message|>thinking...<|channel|>final<|message|>Visible text", "Visible text"),
            ("<|channel|>reasoning<|message|>thinking...<|channel|>output<|message|>Visible text", "Visible text"),
            ("<|channel|>internal<|message|>thinking...<|channel|>response<|message|>Visible text", "Visible text"),
            ("<|channel|>commentary<|message|>thinking...<|channel|>main<|message|>Visible text", "Visible text"),
            ("<|channel|>cot<|message|>thinking...<|channel|>assistant<|message|>Visible text", "Visible text"),
            ("<|channel>reasoningthinking...<|channel>outputVisible text", "Visible text"),
            ("<|channel>cotthinking...<|channel>responseVisible text", "Visible text"),
            ("<|channel>internalthinking...<|channel>assistantVisible text", "Visible text"),
        ];

        for (raw, expected) in test_cases {
            let mut filter = super::VisibleOutputFilter::default();
            let mut visible = filter.push(raw);
            visible.push_str(&filter.finish());
            assert_eq!(visible, expected, "Failed for raw: {raw}");
        }
    }

    #[test]
    fn test_extended_tags_case_insensitivity() {
        let test_cases = [
            ("<ANTTHINKING>secret</ANTTHINKING>Answer", "Answer"),
            ("<REFLECTION>reflecting</REFLECTION>Answer", "Answer"),
            ("<|THOUGHT|>secret<|/THOUGHT|>Answer", "Answer"),
            ("<|CHANNEL|>THOUGHT<|MESSAGE|>secret<|CHANNEL|>FINAL<|MESSAGE|>Answer", "Answer"),
            ("<|CHANNEL>COTsecret<|CHANNEL>OUTPUTAnswer", "Answer"),
        ];

        for (raw, expected) in test_cases {
            let mut filter = super::VisibleOutputFilter::default();
            let mut visible = filter.push(raw);
            visible.push_str(&filter.finish());
            assert_eq!(visible, expected, "Failed for case raw: {raw}");
        }
    }

    #[test]
    fn test_split_chunk_streaming_with_new_special_tokens() {
        // Test arbitrary split streaming across chunks
        let chunks = [
            "<antTh",
            "inking>step 1",
            " reasoning</antTh",
            "inking>Final ",
            "result",
        ];
        let mut filter = super::VisibleOutputFilter::default();
        let mut visible = String::new();
        for chunk in chunks {
            visible.push_str(&filter.push(chunk));
        }
        visible.push_str(&filter.finish());
        assert_eq!(visible, "Final result");

        // Test channel syntax split streaming
        let channel_chunks = [
            "<|chan",
            "nel|>thou",
            "ght<|mess",
            "age|>secret analysis",
            "<|chan",
            "nel|>fin",
            "al<|mess",
            "age|>Clean output",
        ];
        let mut filter2 = super::VisibleOutputFilter::default();
        let mut visible2 = String::new();
        for chunk in channel_chunks {
            visible2.push_str(&filter2.push(chunk));
        }
        visible2.push_str(&filter2.finish());
        assert_eq!(visible2, "Clean output");

        // Test thought and end_of_thought split streaming
        let thought_chunks = [
            "<|tho",
            "ught|>deep thought",
            "<|end_of_",
            "thought|>Hello world",
        ];
        let mut filter3 = super::VisibleOutputFilter::default();
        let mut visible3 = String::new();
        for chunk in thought_chunks {
            visible3.push_str(&filter3.push(chunk));
        }
        visible3.push_str(&filter3.finish());
        assert_eq!(visible3, "Hello world");
    }

    #[test]
    fn test_from_prompt_tail_with_new_tokens() {
        for prompt_tail in [
            "assistant\n<antThinking>\n",
            "assistant\n<|thought|>",
            "assistant\n<reflection>\n",
            "assistant\n<|channel|>thought<|message|>",
            "assistant\n<|channel>thought",
        ] {
            let mut filter = super::VisibleOutputFilter::from_prompt_tail(prompt_tail);
            let mut visible = filter.push("internal reasoning</antthinking>Final answer");
            visible.push_str(&filter.finish());
            assert_eq!(visible, "Final answer", "Failed for prompt tail: {prompt_tail}");
        }
    }

    #[test]
    fn test_fail_closed_partial_new_tokens_at_eof() {
        // Open marker partial prefixes held back and discarded on EOF
        for (raw, expected) in [
            ("Answer<antThin", "Answer"),
            ("Answer<|channel|>tho", "Answer"),
            ("Answer<|chan", "Answer"),
            ("Answer<reflect", "Answer"),
            ("Answer<|tho", "Answer"),
        ] {
            let mut filter = super::VisibleOutputFilter::default();
            let mut visible = filter.push(raw);
            visible.push_str(&filter.finish());
            assert_eq!(visible, expected, "Failed for partial EOF: {raw}");
        }

        // Close marker partial prefixes inside reasoning block held back and discarded on EOF
        let mut hidden_filter = super::VisibleOutputFilter::from_prompt_tail("assistant\n<think>\n");
        let mut visible_hidden = hidden_filter.push("reasoning text<|end_of_");
        visible_hidden.push_str(&hidden_filter.finish());
        assert_eq!(visible_hidden, "");
    }
}
