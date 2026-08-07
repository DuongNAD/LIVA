/// Mọi tag mà LLM được phép phát ra.
///
/// Danh sách này phải TRÙNG KHỚP `AVATAR_EMOTIONS` + `AVATAR_ACTIONS` trong
/// `liva-ui/src/utils/avatarControlTags.ts`. Lệch một tên là hỏng một chiều:
/// UI nuốt tag mà TTS vẫn đọc lên, hoặc ngược lại.
const AVATAR_CONTROL_TAGS: [&str; 11] = [
    // Cảm xúc
    "happy",
    "sad",
    "angry",
    "surprised",
    "neutral",
    "relaxed",
    // Hành động
    "wave",
    "nod",
    "jump",
    "come_closer",
    "step_back",
];

fn is_control_tag(tag: &str) -> bool {
    AVATAR_CONTROL_TAGS.contains(&tag) || is_animation_tag_syntax(tag)
}

fn is_animation_tag_syntax(tag: &str) -> bool {
    let Some(id) = tag.strip_prefix("anim:") else {
        return false;
    };
    !id.is_empty() && id.len() <= 6 && id.bytes().all(|byte| byte.is_ascii_digit())
}

/// `partial` còn có thể lớn lên thành một tag thật không? Dùng để chặn việc
/// giữ văn bản lại vô hạn khi gặp một ngoặc mở không bao giờ đóng.
fn is_viable_tag_prefix(partial: &str) -> bool {
    AVATAR_CONTROL_TAGS
        .iter()
        .any(|tag| tag.starts_with(partial))
        || "anim:".starts_with(partial)
        || partial
            .strip_prefix("anim:")
            .is_some_and(|id| id.len() <= 6 && id.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Lọc tag điều khiển avatar ra khỏi văn bản trước khi đưa vào TTS.
///
/// Hai chế độ, khác nhau có chủ đích:
///
/// - **Tiền tố** (trước khi có chữ hiển thị): nuốt *mọi* cụm trong ngoặc vuông,
///   quen hay lạ. Một `[dance]` do model bịa ra tuyệt đối không được đọc lên,
///   và ở đầu câu trả lời thì không có ngoặc hợp lệ nào cần bảo vệ.
/// - **Thân** (sau khi đã có chữ): chỉ nuốt ngoặc có nội dung khớp danh sách
///   trắng. Đây là thứ cho phép đổi cảm xúc giữa lượt trả lời, mà
///   `Kết quả [2 + 2] là 4.` vẫn được đọc nguyên vẹn.
#[derive(Debug)]
pub(crate) struct AvatarSpeechFilter {
    pending: String,
    reading_control_prefix: bool,
}

impl Default for AvatarSpeechFilter {
    fn default() -> Self {
        Self {
            pending: String::new(),
            reading_control_prefix: true,
        }
    }
}

impl AvatarSpeechFilter {
    pub(crate) fn push(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);

        while self.reading_control_prefix {
            let trimmed_len = self.pending.len() - self.pending.trim_start().len();
            if trimmed_len > 0 {
                self.pending.drain(..trimmed_len);
            }
            if self.pending.is_empty() {
                return String::new();
            }
            if !self.pending.starts_with('[') {
                self.reading_control_prefix = false;
                break;
            }

            let Some(closing_bracket) = self.pending.find(']') else {
                return String::new();
            };
            self.pending.drain(..=closing_bracket);
        }

        let mut out = String::new();
        self.drain_body(&mut out);
        out
    }

    /// Lọc theo danh sách trắng trên phần thân câu trả lời.
    fn drain_body(&mut self, out: &mut String) {
        loop {
            let Some(open) = self.pending.find('[') else {
                out.push_str(&self.pending);
                self.pending.clear();
                return;
            };

            // `[` và `]` đều là ASCII nên mọi chỉ số dưới đây rơi đúng biên ký tự.
            let Some(rel_closing) = self.pending[open + 1..].find(']') else {
                // Chưa thấy `]`. Chỉ giữ phần đuôi lại khi nó còn có thể trở
                // thành tag, để luồng bị cắt kiểu `…[ha` + `ppy] …` vẫn ghép
                // đúng — còn `Kết quả [2 + 2` thì phát ra ngay, không bắt TTS
                // chờ một ngoặc sẽ không bao giờ đóng.
                if is_viable_tag_prefix(&self.pending[open + 1..]) {
                    out.push_str(&self.pending[..open]);
                    self.pending.drain(..open);
                } else {
                    out.push_str(&self.pending);
                    self.pending.clear();
                }
                return;
            };

            let closing = open + 1 + rel_closing;
            out.push_str(&self.pending[..open]);
            if !is_control_tag(&self.pending[open + 1..closing]) {
                // Ngoặc thật trong văn bản — trả lại nguyên vẹn.
                out.push_str(&self.pending[open..=closing]);
            }
            self.pending.drain(..=closing);
        }
    }

    /// Hết luồng. Phần còn treo chỉ có thể là một ngoặc tiền tố chưa đóng hoặc
    /// một đoạn tag còn dở đang được giữ lại — không bao giờ là văn bản thường,
    /// vì `drain_body` đã nhả mọi thứ không thể thành tag. Nên chỗ này đóng an
    /// toàn: `[ha` bị cắt cụt thì bỏ, chứ không đọc lên thành "ha".
    pub(crate) fn finish(&mut self) -> String {
        self.pending.clear();
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{AVATAR_CONTROL_TAGS, AvatarSpeechFilter};

    #[test]
    fn action_tag_bi_cat_doi_khong_di_vao_tts() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(filter.push("[wa"), "");
        assert_eq!(filter.push("ve]Xin chào."), "Xin chào.");
    }

    #[test]
    fn nhieu_tag_va_tag_la_o_dau_cau_deu_bi_loai() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(
            filter.push("[happy] [jump] [dance] Tuyệt quá!"),
            "Tuyệt quá!"
        );
    }

    #[test]
    fn dau_ngoac_sau_khi_van_ban_bat_dau_duoc_giu_nguyen() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(
            filter.push("Kết quả [2 + 2] là 4."),
            "Kết quả [2 + 2] là 4."
        );
    }

    #[test]
    fn stream_ket_thuc_giua_tag_thi_bo_fail_closed() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(filter.push("[ju"), "");
        assert_eq!(filter.finish(), "");
    }

    // ── U26: tag giữa câu ────────────────────────────────────────────────────

    #[test]
    fn tag_giua_cau_bi_loai_khoi_tts() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(
            filter.push("Chào bạn. [happy] Vui quá!"),
            "Chào bạn.  Vui quá!"
        );
    }

    #[test]
    fn animation_id_hop_le_va_khong_ro_deu_khong_bi_doc_len() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(
            filter.push("[anim:201]Xin chào[anim:999999] bạn."),
            "Xin chào bạn."
        );
    }

    #[test]
    fn nhieu_tag_giua_cau_deu_bi_loai() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(
            filter.push("Xong rồi[nod], nhưng hỏng[sad] mất."),
            "Xong rồi, nhưng hỏng mất."
        );
    }

    #[test]
    fn tag_la_giua_cau_duoc_giu_lai_khac_voi_o_tien_to() {
        let mut filter = AvatarSpeechFilter::default();

        // Ở tiền tố, `[dance]` bị nuốt (xem nhieu_tag_va_tag_la_o_dau_cau_deu_bi_loai).
        // Ở giữa câu thì KHÔNG — nếu nuốt, mọi ngoặc vuông hợp lệ đều biến mất.
        assert_eq!(
            filter.push("Nhạc nền [dance mix] rất hay."),
            "Nhạc nền [dance mix] rất hay."
        );
    }

    #[test]
    fn tag_giua_cau_bi_cat_doi_giua_hai_chunk_van_ghep_dung() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(filter.push("Chào bạn. [wa"), "Chào bạn. ");
        assert_eq!(filter.push("ve] Hẹn gặp lại."), " Hẹn gặp lại.");
    }

    #[test]
    fn ngoac_khong_the_thanh_tag_khong_lam_nghen_luong() {
        let mut filter = AvatarSpeechFilter::default();

        // `2 + 2` không thể lớn lên thành tag nào ⇒ nhả ngay, TTS không phải đợi
        // dấu `]` ở chunk sau. Đây là khác biệt so với `[wa` ở test trên.
        assert_eq!(filter.push("Kết quả [2 + 2"), "Kết quả [2 + 2");
        assert_eq!(filter.push("] là 4."), "] là 4.");
    }

    #[test]
    fn tag_dang_do_o_cuoi_luong_bi_bo_chu_khong_doc_len() {
        let mut filter = AvatarSpeechFilter::default();

        assert_eq!(filter.push("Chào bạn. [ha"), "Chào bạn. ");
        assert_eq!(filter.finish(), "");
    }

    /// Bảng ca kiểm dùng CHUNG với `liva-ui/tests/utils/avatarControlTags.test.ts`.
    /// Hai bản cài đặt phải cho ra cùng một văn bản còn lại; lệch là TTS đọc lên
    /// một tag mà UI đã nuốt, hoặc ngược lại.
    #[test]
    fn bang_ca_kiem_chung_voi_ban_typescript() {
        let cases: [(&str, &str); 8] = [
            ("[happy] Xin chào.", "Xin chào."),
            ("[happy] [jump] [dance] Tuyệt quá!", "Tuyệt quá!"),
            ("Kết quả [2 + 2] là 4.", "Kết quả [2 + 2] là 4."),
            ("Chào bạn. [happy] Vui quá!", "Chào bạn.  Vui quá!"),
            (
                "Xong rồi[nod], nhưng hỏng[sad] mất.",
                "Xong rồi, nhưng hỏng mất.",
            ),
            (
                "Nhạc nền [dance mix] rất hay.",
                "Nhạc nền [dance mix] rất hay.",
            ),
            (
                "[come_closer]Lại đây[step_back] rồi lùi.",
                "Lại đây rồi lùi.",
            ),
            ("Không có tag nào cả.", "Không có tag nào cả."),
        ];

        for (input, expected) in cases {
            let mut filter = AvatarSpeechFilter::default();
            let got = format!("{}{}", filter.push(input), filter.finish());
            assert_eq!(got, expected, "đầu vào: {input:?}");
        }
    }

    #[test]
    fn danh_sach_tag_khong_rong_va_khong_trung() {
        let mut sorted = AVATAR_CONTROL_TAGS;
        sorted.sort_unstable();
        let mut deduped = sorted.to_vec();
        deduped.dedup();
        assert_eq!(
            deduped.len(),
            AVATAR_CONTROL_TAGS.len(),
            "có tag trùng nhau"
        );
        assert!(AVATAR_CONTROL_TAGS.iter().all(|tag| !tag.is_empty()));
    }
}
