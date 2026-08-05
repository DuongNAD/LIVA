/// Ý định mà node `router` suy ra từ câu của người dùng.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Hỏi về nội dung màn hình → nhánh vision.
    Vision,
    /// Điều khiển thiết bị → nhánh tool_exec.
    SmartHome {
        device: &'static str,
        action: &'static str,
    },
    /// Điều khiển chính máy này: âm lượng / phát nhạc (U19) → `mcp_tool_exec`.
    ///
    /// Vì sao đi đường nhanh thay vì để LLM chọn: đo trên Qwen3-VL-2B
    /// (26/07/2026) cho thấy model trượt đúng những câu **đa nghĩa** —
    /// *"bật nhạc lên"* rơi sang chỉnh âm lượng, *"chuyển bài khác"* chọn đúng
    /// tool nhưng sai hướng. Đó là trần của model 2B, không sửa được bằng cách
    /// viết lại prompt. Bảng từ khoá thì không đa nghĩa, không tốn token nào,
    /// và cho cùng một kết quả mọi lần.
    OsControl {
        /// `control_volume` hoặc `control_media`.
        tool: &'static str,
        action: &'static str,
    },
    /// Nhắn tin cho người trong danh bạ → nhánh `message_draft`.
    ///
    /// Mang `String` chứ không `&'static str` như hai nhánh trên: tên người và
    /// nội dung tin lấy ra từ chính câu nói, không thể là hằng.
    ///
    /// `body` được phép RỖNG — "nhắn cho Hiến đi" là câu hợp lệ, chỉ là chưa nói
    /// nội dung. Nhánh thi hành sẽ hỏi lại thay vì gửi một tin trống.
    SendMessage {
        recipient: String,
        body: String,
        platform: Option<String>,
    },
    /// Còn lại → trả lời bằng LLM.
    Chat,
}

/// Tách "nhắn cho X bảo Y" thành `(X, Y)`.
///
/// ## Vì sao không để LLM làm
///
/// Cùng lý do với `OsControl`: model 2B trượt đúng những câu đa nghĩa, và ở đây
/// cái giá của việc trượt cao hơn nhiều — không phải bật nhầm đèn mà là gửi
/// nhầm chữ cho người khác. Bảng từ khoá không đa nghĩa, không tốn token, và ra
/// cùng kết quả mọi lần. Phần *diễn đạt lại cho tự nhiên* mới là việc của LLM,
/// và nó nằm sau bước xác nhận.
///
/// ## Quy tắc
///
/// 1. **Cò:** (`nhắn`|`gửi`) [`tin`] [`nhắn`] `cho`. So khớp trên dạng đã bỏ dấu
///    nên "nhan cho" từ STT vẫn ăn.
/// 2. **Mốc nội dung:** từ đầu tiên trong {`bảo`, `rằng`, `là`, `nói`, `hỏi`} hoặc dấu
///    hai chấm. Trước mốc là tên, sau mốc là nội dung.
/// 3. **Bỏ đại từ mở đầu nội dung:** "bảo **nó** ngủ đi" → "ngủ đi".
///
/// Không có mốc thì toàn bộ phần sau cò là tên, nội dung rỗng.
pub(super) fn tach_nhan_tin(text: &str) -> Option<(String, String, Option<String>)> {
    let goc: Vec<&str> = text.split_whitespace().collect();
    if goc.is_empty() {
        return None;
    }
    // Dạng bỏ dấu của từng token, giữ nguyên chỉ số để cắt lại trên bản gốc.
    let gap: Vec<String> = goc
        .iter()
        .map(|t| crate::wake::normalize_for_match(t))
        .collect();

    // ── 1. Tìm cò ────────────────────────────────────────────────────────────
    let mut sau_co = None;
    for i in 0..gap.len() {
        if gap[i] != "nhan" && gap[i] != "gui" {
            continue;
        }
        let mut j = i + 1;
        // Nuốt "tin", "nhắn" ở giữa: "gửi tin nhắn cho", "nhắn tin cho".
        while j < gap.len() && (gap[j] == "tin" || gap[j] == "nhan") {
            j += 1;
        }
        if j < gap.len() && gap[j] == "cho" {
            sau_co = Some(j + 1);
            break;
        }
    }
    let bat_dau = sau_co?;
    if bat_dau >= goc.len() {
        return None; // "nhắn cho" rồi hết câu — không có người nhận
    }

    // ── 2. Tìm mốc nội dung ──────────────────────────────────────────────────
    //
    // Mốc so khớp theo dấu HAY không tuỳ câu, và đây không phải cầu kỳ vô cớ —
    // nó là bản vá cho một lỗi đo được: "nhắn cho Người **Lạ** Hoắc bảo alo".
    // Bỏ dấu thì `lạ` và `là` cùng ra `la`, nên tên bị cắt còn "Người" và nội
    // dung thành "Hoắc bảo alo". Cùng bẫy đó rình mọi tên có `La/Lá/Lã`, và
    // `Bảo` là tên người rất phổ biến.
    //
    // Quy tắc: câu CÓ dấu thì đòi mốc đúng dấu (`là`, `bảo`, `rằng`, `nói`);
    // câu KHÔNG dấu nào — tức STT trả về trần — mới chấp nhận mốc không dấu.
    // Người gõ có dấu thì gõ có dấu cả câu; người đọc cho STT thì mất dấu cả
    // câu. Trường hợp lẫn lộn hiếm, và nếu trượt thì thẻ xác nhận đỡ.
    const MOC_CO_DAU: [&str; 5] = ["bảo", "rằng", "là", "nói", "hỏi"];
    const MOC_KHONG_DAU: [&str; 5] = ["bao", "rang", "la", "noi", "hoi"];
    let cau_co_dau = goc.iter().any(|t| {
        t.chars().any(|c| {
            crate::wake::normalize_for_match(&c.to_string()) != c.to_lowercase().to_string()
        })
    });

    let mut moc = None;
    for k in bat_dau..goc.len() {
        // Dấu hai chấm dính cuối token: "Hiến: ngủ đi".
        if goc[k].ends_with(':') {
            moc = Some((k, true));
            break;
        }
        // Token đầu ngay sau "cho" LUÔN thuộc về tên: người nhận không thể
        // rỗng. Không có dòng này thì "nhắn cho **Bảo** rằng mai đi học" ra tên
        // rỗng rồi trả None — tức mất trắng câu, tệ hơn cả tách sai.
        if k == bat_dau {
            continue;
        }
        let la_moc = if cau_co_dau {
            let thuong = goc[k].to_lowercase();
            MOC_CO_DAU.contains(&thuong.trim_matches(|c: char| !c.is_alphanumeric()))
        } else {
            MOC_KHONG_DAU.contains(&gap[k].as_str())
        };
        if la_moc {
            moc = Some((k, false));
            break;
        }
    }

    let (het_ten, dau_noi_dung) = match moc {
        Some((k, dinh_hai_cham)) => {
            if dinh_hai_cham {
                (k + 1, k + 1) // token có dấu ':' vẫn thuộc về tên
            } else {
                (k, k + 1)
            }
        }
        None => (goc.len(), goc.len()),
    };

    // Nền tảng là hậu tố tùy chọn của tên: "Minh Hiền bằng Messenger".
    // Chỉ cắt khi cả cụm nằm sát mốc nội dung để không nuốt các tên có từ
    // "bằng"/"qua" ở giữa.
    let mut het_nguoi_nhan = het_ten;
    let mut platform = None;
    if het_ten >= bat_dau + 3 {
        for k in (bat_dau + 1)..(het_ten - 1) {
            let la_tu_noi = matches!(gap[k].as_str(), "bang" | "qua" | "tren");
            if !la_tu_noi || k + 2 != het_ten {
                continue;
            }
            platform = match gap[k + 1].as_str() {
                "messenger" | "messager" | "facebook" => Some("messenger".to_string()),
                "telegram" => Some("telegram".to_string()),
                _ => None,
            };
            if platform.is_some() {
                het_nguoi_nhan = k;
            }
            break;
        }
    }

    let ten = goc[bat_dau..het_nguoi_nhan]
        .join(" ")
        .trim_end_matches(':')
        .trim()
        .to_string();
    if ten.is_empty() {
        return None;
    }

    // ── 3. Bỏ đại từ mở đầu nội dung ─────────────────────────────────────────
    let mut i = dau_noi_dung;
    if i < gap.len() {
        if gap[i] == "no" {
            i += 1;
        } else if matches!(
            gap[i].as_str(),
            "anh" | "chi" | "em" | "cau" | "ban" | "ong" | "ba"
        ) && i + 1 < gap.len()
            && matches!(gap[i + 1].as_str(), "ay" | "ta")
        {
            i += 2;
        }
    }
    let noi_dung = goc.get(i..).unwrap_or(&[]).join(" ").trim().to_string();

    Some((ten, noi_dung, platform))
}

/// Tách câu thành các "từ" theo ranh giới ký tự chữ-số Unicode.
///
/// Dùng `is_alphanumeric` chứ không phải `is_ascii_alphanumeric` để giữ nguyên
/// chữ tiếng Việt có dấu — `đèn`, `bật`, `tắt` phải là một token trọn vẹn.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Câu có chứa cụm từ (dãy token liên tiếp) này không?
fn has_phrase(tokens: &[String], phrase: &[&str]) -> bool {
    if phrase.is_empty() || tokens.len() < phrase.len() {
        return false;
    }
    tokens
        .windows(phrase.len())
        .any(|w| w.iter().zip(phrase).all(|(a, b)| a == b))
}

/// Câu có chứa **nguyên** từ này không (không phải chuỗi con).
fn has_word(tokens: &[String], word: &str) -> bool {
    tokens.iter().any(|t| t == word)
}

/// Suy ý định từ câu của người dùng.
///
/// # Vì sao không dùng `contains()`
///
/// Bản trước khớp chuỗi con nên sai cả hai chiều:
/// - **Dương tính giả:** `contains("ac")` khớp "b**ac**k", "pl**ac**e";
///   `contains("on")` khớp "m**on**ey", "c**on**versation";
///   `contains("off")` khớp "c**off**ee", "**off**ice".
///   "back on track" từng bị hiểu thành lệnh bật điều hoà.
/// - **Âm tính giả:** không có một từ khoá tiếng Việt nào, nên "bật đèn giúp
///   mình" không khớp gì cả — đúng thứ người dùng Việt sẽ nói đầu tiên.
///
/// Giờ khớp theo **token trọn vẹn** và có cả từ khoá tiếng Việt. Đây vẫn là
/// định tuyến theo từ khoá, chưa phải tool-calling có schema do LLM sinh —
/// bước đó nằm ở lộ trình.
pub fn route_intent(text: &str) -> Intent {
    let tokens = tokenize(text);

    // Nhắn tin đứng TRƯỚC tất cả, kể cả vision. Vì nội dung tin nhắn là câu của
    // NGƯỜI KHÁC, và nó có thể chứa bất kỳ từ khoá nào của các nhánh dưới:
    // "nhắn cho Nam bật nhạc lên" mà rơi vào OsControl thì LIVA bật nhạc của
    // chính máy này thay vì nhắn — sai thầm lặng, người dùng tưởng đã nhắn.
    // Đặt đầu tiên là cách duy nhất để phần thân tin nhắn không bị nhánh khác
    // cướp. Đổi lại, cái giá phải trả là câu "chụp màn hình gửi cho Nam" sẽ
    // thành nhắn tin — chấp nhận được, vì bản nháp hiện ra để người dùng huỷ.
    if let Some((recipient, body, platform)) = tach_nhan_tin(text) {
        return Intent::SendMessage {
            recipient,
            body,
            platform,
        };
    }

    // Vision ưu tiên cao nhất: hỏi về màn hình thì không thể là lệnh thiết bị.
    if has_phrase(&tokens, &["màn", "hình"])
        || has_word(&tokens, "screen")
        || has_word(&tokens, "screenshot")
        || has_phrase(&tokens, &["trên", "màn"])
    {
        return Intent::Vision;
    }

    // ── Điều khiển máy: âm lượng / phát nhạc (U19) ─────────────────────────
    //
    // CỐ TÌNH chỉ nhận từ vựng TIẾNG VIỆT. Đường nhanh này tồn tại đúng vì
    // tiếng Việt là chỗ model 2B yếu nhất; tiếng Anh nó xử lý tốt nên nhường
    // cho LLM. Thêm danh từ tiếng Anh vào đây là tự rước lại bẫy
    // `"let's get back on track"` — `track` + `back` sẽ thành "quay lại bài
    // trước", đúng loại dương tính giả mà `khong_con_duong_tinh_gia` canh.
    //
    // Đặt TRƯỚC nhánh smart-home nhưng đòi một danh từ âm thanh/nhạc, nên nó
    // không thể cướp `"bật đèn"` / `"tắt quạt"`.
    let danh_tu_am_thanh = has_word(&tokens, "tiếng")
        || has_phrase(&tokens, &["âm", "lượng"])
        || has_word(&tokens, "loa");
    let danh_tu_nhac =
        has_word(&tokens, "nhạc") || has_word(&tokens, "bài") || has_word(&tokens, "hát");

    if danh_tu_am_thanh || danh_tu_nhac {
        // ĐỘ TO thắng ĐANG-PHÁT-GÌ: `"nhỏ nhạc lại"` có cả "nhạc" lẫn "nhỏ",
        // và ý người nói là âm lượng. Cùng ranh giới đã ghi trong mô tả tool.
        let am_luong =
            if has_word(&tokens, "to") || has_word(&tokens, "lớn") || has_word(&tokens, "tăng") {
                Some("up")
            } else if has_word(&tokens, "nhỏ")
                || has_word(&tokens, "bé")
                || has_word(&tokens, "giảm")
                || has_word(&tokens, "khẽ")
            {
                Some("down")
            } else if has_word(&tokens, "tắt") && danh_tu_am_thanh {
                // "tắt tiếng" = mute. "tắt nhạc" thì KHÁC — đó là dừng phát, nên
                // nhánh này đòi đúng danh từ âm thanh.
                Some("mute")
            } else {
                None
            };
        if let Some(action) = am_luong {
            return Intent::OsControl {
                tool: "control_volume",
                action,
            };
        }

        if danh_tu_nhac {
            let media = if has_word(&tokens, "trước")
                || has_phrase(&tokens, &["quay", "lại"])
                || has_word(&tokens, "lùi")
            {
                Some("previous")
            } else if has_word(&tokens, "khác")
                || has_word(&tokens, "kế")
                || has_word(&tokens, "chuyển")
                || has_phrase(&tokens, &["tiếp", "theo"])
            {
                Some("next")
            } else if has_word(&tokens, "dừng")
                || has_word(&tokens, "phát")
                || has_word(&tokens, "bật")
                || has_word(&tokens, "mở")
                || has_word(&tokens, "tắt")
            {
                Some("play_pause")
            } else {
                None
            };
            if let Some(action) = media {
                return Intent::OsControl {
                    tool: "control_media",
                    action,
                };
            }
        }
    }

    let device =
        if has_word(&tokens, "light") || has_word(&tokens, "lamp") || has_word(&tokens, "đèn") {
            Some("light")
        } else if has_word(&tokens, "ac")
            || has_phrase(&tokens, &["điều", "hoà"])
            || has_phrase(&tokens, &["điều", "hòa"])
            || has_phrase(&tokens, &["máy", "lạnh"])
        {
            Some("ac")
        } else if has_word(&tokens, "fan") || has_word(&tokens, "quạt") {
            Some("fan")
        } else {
            None
        };

    let action = if has_word(&tokens, "on") || has_word(&tokens, "bật") || has_word(&tokens, "mở")
    {
        Some("on")
    } else if has_word(&tokens, "off") || has_word(&tokens, "tắt") || has_word(&tokens, "đóng")
    {
        Some("off")
    } else {
        None
    };

    match (device, action) {
        (Some(device), Some(action)) => Intent::SmartHome { device, action },
        _ => Intent::Chat,
    }
}
