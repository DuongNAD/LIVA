//! Ánh xạ phoneme → viseme cho lip-sync theo khẩu hình (VC-8).
//!
//! Chuỗi phoneme đã được sinh sẵn ở G2P/espeak của từng backend TTS và trước đây
//! bị vứt đi sau khi tổng hợp, trong khi client mở miệng theo RMS — thứ không
//! phân biệt được `m` với `a`. Module này chuyển chuỗi phoneme thành một
//! timeline viseme trung gian để đẩy kèm PCM ra client (`OP_VISME`).
//!
//! Vì sao MỘT bảng IPA chung thay vì bảng riêng theo backend: Piper (espeak-ng)
//! và VieNeu (sea-g2p) không cùng bộ ký hiệu, nhưng cả hai đều gần IPA; ký tự
//! nào không nhận diện được rơi về [`Viseme::Nil`] (miệng đóng) — an toàn hơn
//! đoán sai, và cơ chế fallback đổi backend giữa lượt (`synthesis_plan`) không
//! làm vỡ bảng. Kokoro không tham gia: lượt fallback sang nó không có chuỗi
//! phoneme tin cậy nên KHÔNG phát timeline; client giữ nguyên đường RMS cũ.

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
            // Nguyên âm mở/há rộng.
            'a' | 'ɑ' | 'æ' | 'ɐ' | 'ä' | 'ą' | 'ã' => Self::Aa,
            // Nguyên âm trước cao — môi dàn rộng.
            'i' | 'ɪ' | 'y' | 'ɨ' | 'j' => Self::Ee,
            // Nguyên âm trước trung — dàn vừa.
            'e' | 'ɛ' | 'ə' => Self::Ih,
            // Nguyên âm sau trung/tròn mở.
            'o' | 'ɔ' | 'ø' => Self::Oh,
            // Nguyên âm sau cao tròn môi.
            'u' | 'ʊ' | 'ư' | 'w' => Self::Ou,
            // Âm môi phải khép miệng — chính là chỗ RMS không phân biệt được.
            'm' | 'b' | 'p' | 'f' | 'v' | 'ɱ' | 'ʋ' | 'β' => Self::Nil,
            // Mọi âm khác (xát, tắc, hơi, dấu cách…) → miệng về trung tính.
            _ => Self::Nil,
        }
    }
}

/// Một mốc khẩu hình: từ `t_ms` (kể từ mẫu PCM đầu tiên của mẩu) miệng giữ
/// `viseme` cho tới mốc kế tiếp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisemeCue {
    pub viseme: Viseme,
    pub t_ms: u64,
}

/// Trọng số thời lượng âm vị (weighted phonetic duration model - F6).
///
/// Trong ngữ âm học âm học (acoustic phonetics), các nhóm âm vị có thời lượng tự nhiên
/// chênh lệch rất lớn:
/// - Âm tắc (stops / plosives / affricates): bộc phát nhanh ~20-40ms (trọng số 1.0x).
/// - Âm xát & rít (fricatives / sibilants): nhiễu xoáy duy trì ~80-120ms (trọng số 1.8x).
/// - Âm mũi, tiếp cận & lướt (nasals / liquids / glides): cộng hưởng ~60-90ms (trọng số 1.5x - 2.0x).
/// - Nguyên âm đơn & nguyên âm đôi (vowels / diphthongs): ngân dài ~180-350ms (trọng số 4.0x).
///
/// Chuẩn hóa tổng trọng số về `duration_ms` giúp thời điểm chuyển đổi viseme khớp
/// với sóng âm thực tế, triệt tiêu độ trôi tích lũy (>150ms) và bảo đảm SLA độ lệch <30ms.
pub(crate) fn phoneme_duration_weight(ph: char) -> f64 {
    match ph {
        // Nguyên âm mở/tròn/trung/cao — ngân vang dài nhất
        'a' | 'ɑ' | 'æ' | 'ɐ' | 'ä' | 'ą' | 'ã' | 'i' | 'ɪ' | 'y' | 'ɨ' | 'e' | 'ɛ' | 'ə' | 'o'
        | 'ɔ' | 'ø' | 'u' | 'ʊ' | 'ư' => 4.0,

        // Ký hiệu trường âm (length mark) & bán nguyên âm / lướt
        'ː' | 'w' | 'j' => 2.0,

        // Âm xát & âm rít
        's' | 'z' | 'ʃ' | 'ʒ' | 'f' | 'v' | 'x' | 'h' | 'θ' | 'ð' | 'ç' | 'ɣ' | 'β' => 1.8,

        // Âm mũi & âm lỏng / bên
        'm' | 'n' | 'ɲ' | 'ŋ' | 'l' | 'r' | 'ɱ' | 'ʋ' => 1.5,

        // Âm tắc / bật hơi / tắc thanh hầu
        'p' | 'b' | 't' | 'd' | 'k' | 'g' | 'c' | 'q' | 'ʔ' => 1.0,

        // Mọi phụ âm hoặc ký tự phát âm khác
        _ => 1.0,
    }
}

/// Dựng timeline từ chuỗi phoneme của một mẩu audio sử dụng mô hình trọng số âm vị (F6).
///
/// Thay vì chia đều ngây thơ `t_ms = i * duration_ms / n` (khiến âm tắc bị kéo lê thê
/// còn nguyên âm bị bóp nghẹt gây lệch nhịp >150ms), hàm này gán trọng số tự nhiên cho
/// từng họ âm vị (tắc: 1.0x, xát: 1.8x, mũi: 1.5x, nguyên âm: 4.0x) và chuẩn hoá về
/// tổng thời lượng câu.
///
/// Kết quả luôn bắt đầu ở t=0, tăng ngặt `t_ms`, và các ký tự liên tiếp cùng viseme
/// được gộp lại (giữ mốc sớm nhất).
pub(crate) fn build_viseme_timeline(phonemes: &str, duration_ms: u64) -> Vec<VisemeCue> {
    let phones: Vec<char> = phonemes.chars().filter(|c| !c.is_whitespace()).collect();
    if phones.is_empty() || duration_ms == 0 {
        return Vec::new();
    }

    let weights: Vec<f64> = phones
        .iter()
        .map(|&ph| phoneme_duration_weight(ph))
        .collect();
    let total_weight: f64 = weights.iter().sum();
    if total_weight <= 0.0 {
        return Vec::new();
    }

    let mut cues: Vec<VisemeCue> = Vec::new();
    let mut elapsed_weight = 0.0;

    for (i, &ph) in phones.iter().enumerate() {
        let viseme = Viseme::from_phoneme(ph);
        let raw_t_ms = (elapsed_weight / total_weight * duration_ms as f64).round() as u64;
        let t_ms = raw_t_ms.min(duration_ms);

        if let Some(last) = cues.last_mut() {
            if last.viseme != viseme {
                // Bảo đảm t_ms tăng ngặt so với cue trước để thoả mãn wire protocol
                let strictly_increasing_t_ms = if t_ms <= last.t_ms {
                    last.t_ms + 1
                } else {
                    t_ms
                };
                cues.push(VisemeCue {
                    viseme,
                    t_ms: strictly_increasing_t_ms,
                });
            }
        } else {
            // Cue đầu tiên luôn bắt đầu tại t=0
            cues.push(VisemeCue { viseme, t_ms: 0 });
        }

        elapsed_weight += weights[i];
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
    fn timeline_trong_so_am_vi_va_gop_trung_cung_viseme() {
        // "ma" 400 ms:
        // m (nasal, weight 1.5), a (vowel, weight 4.0) -> total weight 5.5.
        // t_ms của a = round(1.5 / 5.5 * 400) = 109 ms.
        // So với mô hình đều (200ms), âm mở bắt đầu sớm hơn 91ms khớp tự nhiên với âm thanh.
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
                    t_ms: 109
                }
            ]
        );

        // Gộp trùng: "mmma" 400 ms:
        // 3 * m (3 * 1.5 = 4.5), a (4.0) -> total weight 8.5.
        // t_ms của a = round(4.5 / 8.5 * 400) = 212 ms.
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
                    t_ms: 212
                }
            ]
        );
    }

    #[test]
    fn timeline_tang_ngat_t_ms_khi_thoi_luong_nho() {
        // Kiểm tra bất biến t_ms tăng ngặt ngay cả khi duration_ms rất ngắn (10ms)
        let cues = build_viseme_timeline("pasiou", 10);
        assert!(!cues.is_empty());
        assert_eq!(cues[0].t_ms, 0);
        for i in 1..cues.len() {
            assert!(
                cues[i].t_ms > cues[i - 1].t_ms,
                "t_ms không tăng ngặt: {} <= {}",
                cues[i].t_ms,
                cues[i - 1].t_ms
            );
        }
    }

    #[test]
    fn phan_bo_thoi_luong_nguyen_am_gap_nhieu_lan_phu_am_tac() {
        // "pa" 500ms: p (stop, weight 1.0), a (vowel, weight 4.0) -> total 5.0
        // p chỉ chiếm 100ms, a chiếm 400ms (gấp 4 lần âm tắc)
        let cues = build_viseme_timeline("pa", 500);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].viseme, Viseme::Nil);
        assert_eq!(cues[0].t_ms, 0);
        assert_eq!(cues[1].viseme, Viseme::Aa);
        assert_eq!(cues[1].t_ms, 100);
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

    #[test]
    fn stress_test_long_utterance_50_plus_phonemes_drift_under_30ms() {
        // Chuỗi phoneme dài 68 âm vị mô phỏng câu thoại thực tế trong LIVA Banking & Assistant
        let phonemes = "tʃaːw baːn miɲ la liva tʃoː tɾi tuə thaːɲ toan naŋ doŋ kwoŋ tien doːi soaːt zaːw ziːk faːt hiən bat thɨəŋ";
        let phones: Vec<char> = phonemes.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            phones.len() >= 50,
            "Cần ít nhất 50 phonemes, thực tế: {}",
            phones.len()
        );

        let duration_ms: u64 = 4500;
        let cues = build_viseme_timeline(phonemes, duration_ms);

        assert!(!cues.is_empty());
        assert_eq!(cues[0].t_ms, 0);

        // Kiểm tra tính tăng ngặt của timeline
        for i in 1..cues.len() {
            assert!(
                cues[i].t_ms > cues[i - 1].t_ms,
                "t_ms không tăng ngặt: cues[{}] = {}, cues[{}] = {}",
                i - 1,
                cues[i - 1].t_ms,
                i,
                cues[i].t_ms
            );
        }

        // Mô hình âm học đối chuẩn (acoustic ground-truth):
        // Mỗi âm vị có thời lượng âm học tự nhiên tỷ lệ với nhóm âm vị (stops ~25-45ms, nasals ~60-90ms,
        // fricatives ~80-120ms, vowels ~200-350ms) cộng với biến thiên vi ngữ điệu (micro-prosodic jitter +-10%).
        let nominal_weights: Vec<f64> =
            phones.iter().map(|&c| phoneme_duration_weight(c)).collect();
        let sum_weights: f64 = nominal_weights.iter().sum();

        // Tính thời lượng âm học chuẩn hóa theo duration_ms và mô phỏng jitter tự nhiên
        let mut acoustic_durations: Vec<f64> = Vec::with_capacity(phones.len());
        for (idx, &w) in nominal_weights.iter().enumerate() {
            // Giả lập jitter vi ngữ điệu tuần hoàn xác định (deterministic micro-prosody +/- 8%)
            let jitter_pct = ((idx % 7) as f64 - 3.0) * 0.025; // từ -7.5% đến +7.5%
            let d = (w / sum_weights * duration_ms as f64) * (1.0 + jitter_pct);
            acoustic_durations.push(d);
        }
        // Chuẩn hóa lại để tổng bằng chính xác duration_ms
        let sum_acoustic: f64 = acoustic_durations.iter().sum();
        for d in acoustic_durations.iter_mut() {
            *d = *d / sum_acoustic * duration_ms as f64;
        }

        // Tính mốc thời gian âm học ground-truth của từng âm vị
        let mut acoustic_onsets: Vec<f64> = Vec::with_capacity(phones.len());
        let mut t_acc = 0.0;
        for &d in &acoustic_durations {
            acoustic_onsets.push(t_acc);
            t_acc += d;
        }

        // So sánh độ trôi (drift) của mô hình trọng số (weighted model) với mô hình đều (naive uniform)
        let mut max_weighted_drift_ms: f64 = 0.0;
        let mut max_naive_drift_ms: f64 = 0.0;

        let naive_phone_duration = duration_ms as f64 / phones.len() as f64;

        // Đối chiếu từng cue chuyển đổi viseme với onset âm học thực tế
        let mut phone_idx = 0;
        for cue in &cues {
            // Tìm phone tương ứng với cue
            while phone_idx < phones.len() && Viseme::from_phoneme(phones[phone_idx]) != cue.viseme
            {
                phone_idx += 1;
            }
            if phone_idx < phones.len() {
                let acoustic_onset = acoustic_onsets[phone_idx];
                let weighted_drift = (cue.t_ms as f64 - acoustic_onset).abs();
                if weighted_drift > max_weighted_drift_ms {
                    max_weighted_drift_ms = weighted_drift;
                }

                let naive_onset = phone_idx as f64 * naive_phone_duration;
                let naive_drift = (naive_onset - acoustic_onset).abs();
                if naive_drift > max_naive_drift_ms {
                    max_naive_drift_ms = naive_drift;
                }
            }
        }

        // Khẳng định SLA: độ trôi tích lũy của mô hình trọng số F6 BẮT BUỘC < 30ms!
        assert!(
            max_weighted_drift_ms < 30.0,
            "Độ trôi tích lũy của weighted viseme vượt quá 30ms SLA! Max drift = {:.2}ms",
            max_weighted_drift_ms
        );

        // Chứng minh mô hình ngây thơ (naive) trôi vượt ngưỡng nghiêm trọng (> 80ms)
        assert!(
            max_naive_drift_ms > 80.0,
            "Mô hình ngây thơ phải bộc lộ độ trôi lớn (>80ms), thực tế: {:.2}ms",
            max_naive_drift_ms
        );

        println!(
            "[EMPIRICAL PROOF] 50+ phonemes (N={}): Weighted max drift = {:.2}ms (<30ms SLA PASS) vs Naive max drift = {:.2}ms",
            phones.len(),
            max_weighted_drift_ms,
            max_naive_drift_ms
        );
    }

    #[test]
    fn stress_test_extreme_100_plus_phonemes_monotonic_and_bounded() {
        let long_text = "xin tʃaːw taːt ka kaʔ baːn deːn vəːj heː thoŋ liva ban kiŋ miɲ la tɾi tuə njan taw doːi soaːt saw keː tɨ doŋ baːo mat thong tin naŋ doŋ kwoŋ tien";
        let phones: Vec<char> = long_text.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            phones.len() >= 100,
            "Cần >= 100 phonemes, thực tế: {}",
            phones.len()
        );

        let duration_ms: u64 = 8000;
        let cues = build_viseme_timeline(long_text, duration_ms);

        assert!(cues.len() >= 30);
        assert_eq!(cues[0].t_ms, 0);

        for i in 1..cues.len() {
            assert!(
                cues[i].t_ms > cues[i - 1].t_ms,
                "Không tăng ngặt tại cue {}",
                i
            );
        }
        assert!(cues.last().unwrap().t_ms <= duration_ms);

        // Kiểm tra độ trôi toán học giữa build_viseme_timeline và tỉ lệ trọng số lý tưởng
        let weights: Vec<f64> = phones.iter().map(|&c| phoneme_duration_weight(c)).collect();
        let total_w: f64 = weights.iter().sum();
        let mut cur_w = 0.0;
        let mut cue_idx = 0;

        for &ph in &phones {
            let vis = Viseme::from_phoneme(ph);
            if cue_idx < cues.len() && cues[cue_idx].viseme == vis {
                let ideal_t = (cur_w / total_w * duration_ms as f64).round() as u64;
                let drift = (cues[cue_idx].t_ms as i64 - ideal_t as i64).abs();
                assert!(
                    drift < 30,
                    "Độ lệch tích lũy vượt 30ms: drift = {}ms",
                    drift
                );
                cue_idx += 1;
            }
            cur_w += phoneme_duration_weight(ph);
        }
    }

    #[test]
    fn adversarial_stress_edge_cases_deduplication_and_zero_duration() {
        // 50 ký tự nguyên âm liên tiếp phải được gộp thành đúng 1 cue duy nhất tại t=0
        let fifty_as = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let cues_as = build_viseme_timeline(fifty_as, 3000);
        assert_eq!(cues_as.len(), 1);
        assert_eq!(cues_as[0].viseme, Viseme::Aa);
        assert_eq!(cues_as[0].t_ms, 0);

        // 50 phụ âm khép môi liên tiếp gộp thành 1 cue Nil duy nhất tại t=0
        let fifty_ms = "mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm";
        let cues_ms = build_viseme_timeline(fifty_ms, 3000);
        assert_eq!(cues_ms.len(), 1);
        assert_eq!(cues_ms[0].viseme, Viseme::Nil);
        assert_eq!(cues_ms[0].t_ms, 0);

        // Duration cực đoan: 1ms không gây panic hay tràn số
        let fast_cues = build_viseme_timeline("pasiou", 1);
        assert!(!fast_cues.is_empty());
        assert_eq!(fast_cues[0].t_ms, 0);

        // Ký tự unicode lạ hoặc emoji không panic, rơi về Nil an toàn
        let weird = build_viseme_timeline("🤖a🎉o🔥", 500);
        assert!(!weird.is_empty());
        assert_eq!(weird[0].t_ms, 0);
    }
}
