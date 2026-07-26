const OPEN_MARKERS: &[&str] = &[
    "<think>",
    "<thought>",
    "<analysis>",
    "<reasoning>",
    "<channel_thought>",
    "<|channel>analysis",
    "<|channel>thought",
    "<|channel|>analysis<|message|>",
];
const CLOSE_MARKERS: &[&str] = &[
    "</think>",
    "</thought>",
    "</analysis>",
    "</reasoning>",
    "</channel_thought>",
    "<|channel>final",
    "<|channel|>final<|message|>",
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
}
