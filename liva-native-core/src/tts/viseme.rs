//! Ánh xạ phoneme → viseme cho lip-sync theo khẩu hình (VC-8).
//!
//! Chuỗi phoneme đã được sinh sẵn ở G2P/espeak của từng backend TTS và trước đây
//! bị vứt đi sau khi tổng hợp, trong khi client mở miệng theo RMS — thứ không
//! phân biệt được `m` với `a`. Module này chuyển chuỗi phoneme thành một
//! timeline viseme trung gian để đẩy kèm PCM ra client (`OP_VISME`).
//!
//! Vì sao MỘT bảng IPA chung thay vì bảng riêng theo backend: Piper (espeak-ng),
//! VieNeu (sea-g2p) và Kokoro (g2p) đều phát ra các ký hiệu IPA; ký tự
//! nào không nhận diện được rơi về [`Viseme::Nil`] (miệng đóng) — an toàn hơn
//! đoán sai, và cơ chế fallback đổi backend giữa lượt (`synthesis_plan`) không
//! làm vỡ bảng. Bảng hỗ trợ mở rộng toàn bộ các nguyên âm IPA và âm khép môi.

/// Tập viseme trung gian — tên trùng preset biểu cảm VRM chuẩn; [`Viseme::Nil`]
/// nghĩa là miệng đóng/không phát biểu cảm nào (âm môi m/b/p/f/v, khoảng lặng,
/// ký tự lạ).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Viseme {
    Aa,
    Ee,
    Ih,
    Oh,
    Ou,
    Nil,
}

impl Viseme {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Aa => "aa",
            Self::Ee => "ee",
            Self::Ih => "ih",
            Self::Oh => "oh",
            Self::Ou => "ou",
            Self::Nil => "nil",
        }
    }

    /// Phân rã một ký tự phoneme (IPA hoặc ASCII gần IPA) thành viseme.
    pub(crate) fn from_phoneme(ph: char) -> Self {
        match ph {
            // Nguyên âm mở/há rộng (bao gồm cả ʌ và ɒ).
            'a' | 'ɑ' | 'æ' | 'ɐ' | 'ä' | 'ą' | 'ã' | 'ʌ' | 'ɒ' => Self::Aa,
            // Nguyên âm trước cao — môi dàn rộng.
            'i' | 'ɪ' | 'y' | 'ɨ' | 'j' => Self::Ee,
            // Nguyên âm trước trung / trung tâm (bao gồm cả ɜ và ɚ).
            'e' | 'ɛ' | 'ə' | 'ɜ' | 'ɚ' => Self::Ih,
            // Nguyên âm sau trung/tròn mở.
            'o' | 'ɔ' | 'ø' => Self::Oh,
            // Nguyên âm sau cao tròn môi (bao gồm cả ʉ và ɯ).
            'u' | 'ʊ' | 'ư' | 'w' | 'ʉ' | 'ɯ' => Self::Ou,
            // Âm môi & răng môi phải khép miệng — chính là chỗ RMS không phân biệt được.
            'm' | 'b' | 'p' | 'f' | 'v' | 'ɱ' | 'ʋ' | 'β' => Self::Nil,
            // Mọi âm khác (xát, tắc, hơi, dấu cách…) → miệng về trung tính.
            _ => Self::Nil,
        }
    }
}

/// Helper kiểm tra ký tự biến âm / dấu nhấn / dấu kéo dài không mang thời lượng phoneme độc lập.
fn is_ipa_modifier(c: char) -> bool {
    matches!(
        c,
        'ˈ' | 'ˌ' | 'ː' | 'ˑ' | '̆' | '͡' | '͜' | 'ʰ' | 'ʲ' | 'ʷ' | 'ˤ' | '˞' | '̃'
    )
}

/// Một mốc khẩu hình: từ `t_ms` (kể từ mẫu PCM đầu tiên của mẩu) miệng giữ
/// `viseme` cho tới mốc kế tiếp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisemeCue {
    pub viseme: Viseme,
    pub t_ms: u64,
}

/// Dựng timeline từ chuỗi phoneme của một mẩu audio.
///
/// Chia ĐỀU `duration_ms` cho mỗi phoneme rồi gộp các ký tự liên tiếp cùng
/// viseme (giữ mốc sớm nhất): VITS/codec không nhả ma trận căn chỉnh per-
/// phoneme, phân bố đều là xấp xỉ chuẩn của các bộ lip-sync nhẹ; mục tiêu của
/// VC-8 là phân biệt nhóm môi với nguyên âm mở, không phải nhảy khẩu hình đúng
/// từng ms. Kết quả luôn bắt đầu ở t=0 và tăng ngặt.
pub(crate) fn build_viseme_timeline(phonemes: &str, duration_ms: u64) -> Vec<VisemeCue> {
    let phones: Vec<char> = phonemes
        .chars()
        .filter(|c| !c.is_whitespace() && !is_ipa_modifier(*c))
        .collect();
    if phones.is_empty() || duration_ms == 0 {
        return Vec::new();
    }
    let n = phones.len() as u64;
    let mut cues: Vec<VisemeCue> = Vec::new();
    for (i, &ph) in phones.iter().enumerate() {
        let viseme = Viseme::from_phoneme(ph);
        let t_ms = i as u64 * duration_ms / n;
        if cues.last().is_none_or(|last| last.viseme != viseme) {
            cues.push(VisemeCue { viseme, t_ms });
        }
    }
    cues
}

/// Công tắc VC-8: chỉ khi `LIVA_LIPSYNC=phoneme` mới phát timeline viseme kèm
/// PCM. Mặc định `rms` — hành vi cũ giữ nguyên cho tới khi bật tường minh.
pub(crate) fn lipsync_enabled_from(raw: Option<&str>) -> bool {
    raw.is_some_and(|v| v.eq_ignore_ascii_case("phoneme"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn am_mbp_khep_mieng_nguyen_am_mo_ha_rong() {
        assert_eq!(Viseme::from_phoneme('m'), Viseme::Nil);
        assert_eq!(Viseme::from_phoneme('b'), Viseme::Nil);
        assert_eq!(Viseme::from_phoneme('p'), Viseme::Nil);
        assert_eq!(Viseme::from_phoneme('a'), Viseme::Aa);
        assert_eq!(Viseme::from_phoneme('ɑ'), Viseme::Aa);
        assert_eq!(Viseme::from_phoneme('i'), Viseme::Ee);
        assert_eq!(Viseme::from_phoneme('o'), Viseme::Oh);
        assert_eq!(Viseme::from_phoneme('u'), Viseme::Ou);
        // Âm không môi cũng về trung tính, nhưng khác hẳn việc há rộng.
        assert_eq!(Viseme::from_phoneme('z'), Viseme::Nil);
        assert_ne!(Viseme::from_phoneme('z'), Viseme::Aa);
    }

    #[test]
    fn test_extended_ipa_vowels_mapping() {
        assert_eq!(Viseme::from_phoneme('ʌ'), Viseme::Aa);
        assert_eq!(Viseme::from_phoneme('ɒ'), Viseme::Aa);
        assert_eq!(Viseme::from_phoneme('ɜ'), Viseme::Ih);
        assert_eq!(Viseme::from_phoneme('ɚ'), Viseme::Ih);
        assert_eq!(Viseme::from_phoneme('ʉ'), Viseme::Ou);
        assert_eq!(Viseme::from_phoneme('ɯ'), Viseme::Ou);
    }

    #[test]
    fn test_bilabials_and_labiodentals_all_nil() {
        let bilabials = ['m', 'b', 'p', 'f', 'v', 'ɱ', 'ʋ', 'β'];
        for ph in bilabials {
            assert_eq!(
                Viseme::from_phoneme(ph),
                Viseme::Nil,
                "Phoneme '{}' phai duoc map ve Nil de khong ho mieng",
                ph
            );
        }
    }

    #[test]
    fn timeline_chia_deu_va_gop_trung_cung_viseme() {
        // "ma" 400 ms → Nil@0, Aa@200.
        let cues = build_viseme_timeline("ma", 400);
        assert_eq!(
            cues,
            vec![
                VisemeCue {
                    viseme: Viseme::Nil,
                    t_ms: 0
                },
                VisemeCue {
                    viseme: Viseme::Aa,
                    t_ms: 200
                }
            ]
        );

        // Gộp trùng: "mmma" 400 ms → Nil@0 (gộp 3 m), Aa@300.
        let cues = build_viseme_timeline("mmma", 400);
        assert_eq!(
            cues,
            vec![
                VisemeCue {
                    viseme: Viseme::Nil,
                    t_ms: 0
                },
                VisemeCue {
                    viseme: Viseme::Aa,
                    t_ms: 300
                }
            ]
        );
    }

    #[test]
    fn timeline_loc_sach_cac_modifier_va_stress_marks() {
        // "həlˈoʊ" with duration 400ms:
        // Phones after modifier filtering: ['h', 'ə', 'l', 'o', 'ʊ'] (5 phones)
        // h -> Nil@0
        // ə -> Ih@80
        // l -> Nil@160
        // o -> Oh@240
        // ʊ -> Ou@320
        let cues = build_viseme_timeline("həlˈoʊ", 400);
        assert_eq!(
            cues,
            vec![
                VisemeCue {
                    viseme: Viseme::Nil,
                    t_ms: 0
                },
                VisemeCue {
                    viseme: Viseme::Ih,
                    t_ms: 80
                },
                VisemeCue {
                    viseme: Viseme::Nil,
                    t_ms: 160
                },
                VisemeCue {
                    viseme: Viseme::Oh,
                    t_ms: 240
                },
                VisemeCue {
                    viseme: Viseme::Ou,
                    t_ms: 320
                },
            ]
        );

        // String with only modifiers returns empty timeline
        assert!(build_viseme_timeline("ˈˌːˑ̆", 500).is_empty());
    }

    #[test]
    fn timeline_rong_khi_khong_co_phoneme_hoac_do_dai_0() {
        assert!(build_viseme_timeline("", 400).is_empty());
        assert!(build_viseme_timeline("   ", 400).is_empty());
        assert!(build_viseme_timeline("ma", 0).is_empty());
    }

    #[test]
    fn cong_tac_chi_bat_dung_gia_tri_phoneme() {
        assert!(lipsync_enabled_from(Some("phoneme")));
        assert!(lipsync_enabled_from(Some("PHONEME"))); // không phân biệt hoa thường
        assert!(!lipsync_enabled_from(Some("rms")));
        assert!(!lipsync_enabled_from(Some("junk")));
        assert!(!lipsync_enabled_from(None)); // mặc định: hành vi cũ
    }
}
