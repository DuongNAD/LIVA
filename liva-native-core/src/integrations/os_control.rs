//! Điều khiển hệ điều hành: âm lượng và phát nhạc (U19).
//!
//! Vì sao có module này: vòng tool-calling (G1) đã chạy, nhưng danh mục tool
//! gần như rỗng — hai thao tác vault, một smart-home chưa có phần cứng. Cơ chế
//! không có nội dung thì không ai thấy nó tồn tại. "Đang bận tay, nói *nhỏ nhạc
//! lại* → nó làm ngay" chứng minh trợ lý **chạm được vào máy**, không chỉ nói.
//!
//! ## Vì sao `SendInput` chứ không Core Audio / WMI
//!
//! Phím đa phương tiện của Windows (`VK_VOLUME_*`, `VK_MEDIA_*`) được **shell xử
//! lý toàn hệ thống**: âm lượng đổi đúng như bấm phím trên bàn phím, và
//! play/pause tới đúng ứng dụng đang giữ phiên phát. Đường này cần đúng feature
//! `Win32_UI_Input_KeyboardAndMouse` mà `windows-sys` **đã bật sẵn** trong
//! `Cargo.toml` — tức **không thêm một dependency nào**.
//!
//! Core Audio (`IAudioEndpointVolume`) đặt được mức tuyệt đối, nhưng kéo theo
//! COM + hai feature nữa, để đổi lấy thứ mà người dùng nói năng tự nhiên hiếm
//! khi cần ("giảm âm lượng" phổ biến hơn "đặt âm lượng 37 %").
//!
//! ## Độ sáng màn hình — CỐ TÌNH chưa làm
//!
//! Không có phím ảo chuẩn cho độ sáng. Hai đường còn lại đều hỏng ở đúng chỗ
//! quan trọng nhất: `SetMonitorBrightness` (Dxva2) cần DDC/CI nên **trượt trên
//! phần lớn màn laptop**, còn WMI `WmiMonitorBrightnessMethods` kéo theo cả
//! tầng COM. Một tool "điều khiển độ sáng" trượt im lặng trên chính máy beta
//! tester còn tệ hơn không có tool — đó đúng là "thành công giả" mà dự án vừa
//! gỡ khỏi `smart_home`. Khi làm, phải kèm dò năng lực thật và báo lỗi thẳng.
//!
//! ## Hoàn tác được
//!
//! Cả hai tool đều đảo ngược bằng đúng một lệnh ngược lại (tăng ↔ giảm, mute là
//! công tắc, play/pause là công tắc). Đó là điều kiện để chúng được phép **tự
//! chạy** — xem `NATIVE_AUTOEXEC` trong `llm/tool_calling.rs`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Số lần bấm phím tối đa cho một lệnh âm lượng.
///
/// Mỗi lần bấm đổi khoảng 2 %, nên 10 lần ≈ 20 % — đủ để một câu ("giảm âm
/// lượng nhiều vào") có tác dụng nghe được, mà vẫn không thể tắt tiếng hẳn
/// bằng một lệnh. Trần này là hàng rào an toàn, không phải tuỳ biến.
const MAX_STEPS: u8 = 10;
const DEFAULT_STEPS: u8 = 4;

// Mã phím ảo của Windows. Ghi thẳng giá trị thay vì mượn hằng số của
// `windows-sys` để module này vẫn biên dịch được trên nền không phải Windows
// (nhánh `cfg` bên dưới trả lỗi trung thực thay vì không build được).
#[cfg(windows)]
const VK_VOLUME_MUTE: u16 = 0xAD;
#[cfg(windows)]
const VK_VOLUME_DOWN: u16 = 0xAE;
#[cfg(windows)]
const VK_VOLUME_UP: u16 = 0xAF;
#[cfg(windows)]
const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
#[cfg(windows)]
const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
#[cfg(windows)]
const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;

// Alias = từ vựng model 2B thật sự sinh ra, đo bằng `os_control_probe`
// (26/07/2026, Qwen3-VL-2B). Schema chỉ quảng cáo tên chuẩn nên prompt không
// phồng thêm token nào; alias chỉ nới phía NHẬN. Cùng cách `ControlSmartHomeArgs`
// giữ `command` làm alias của `action`.
//
// Vì sao đáng làm thay vì bắt model nói đúng: người ta nói "pause", "louder" —
// đó là tiếng Anh tự nhiên. Bắt một model 2B nhớ rằng phải gõ `play_pause` là
// đặt cược vào thứ nó kém nhất, trong khi nới phía nhận không tốn gì.
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VolumeAction {
    #[serde(alias = "louder", alias = "increase", alias = "raise", alias = "volume_up")]
    Up,
    #[serde(
        alias = "quieter",
        alias = "decrease",
        alias = "lower",
        alias = "volume_down"
    )]
    Down,
    // `unmute` cũng về đây: đây là CÔNG TẮC, nên "bật tiếng lại" và "tắt tiếng"
    // là cùng một thao tác vật lý.
    #[serde(alias = "unmute", alias = "toggle_mute", alias = "silence")]
    Mute,
}

/// `steps` khoan dung: số, chuỗi số, hoặc **bất kỳ thứ gì khác → bỏ qua**.
///
/// Đo được 26/07/2026 (Qwen3-VL-2B): model không biết điền gì cho một trường
/// tuỳ chọn nên nó điền chuỗi giữ chỗ — `{"action":"up","steps":"any"}`. Với
/// `Option<u8>` thẳng, cả lời gọi hỏng và người dùng nhận "Validation error"
/// cho một câu mà **ý định hoàn toàn rõ ràng** ("to lên chút đi").
///
/// Nguyên tắc: một trường phụ hỏng không được phép giết một lệnh có ý định rõ.
/// `action` thì vẫn nghiêm — sai từ vựng ở đó là sai ý định, phải báo lỗi.
fn steps_khoan_dung<'de, D>(d: D) -> Result<Option<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_u64().and_then(|x| u8::try_from(x).ok()),
        Value::String(s) => s.trim().parse::<u8>().ok(),
        _ => None,
    })
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeArgs {
    pub action: VolumeAction,
    /// Số nấc (1–10, mặc định 4). Bỏ qua khi `action` là `mute`.
    #[serde(default, deserialize_with = "steps_khoan_dung")]
    pub steps: Option<u8>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaAction {
    // "pause" và "play" là thứ model sinh ra nhiều nhất — và cả hai đều đúng ý,
    // vì phím media của Windows vốn là một công tắc duy nhất.
    #[serde(
        alias = "pause",
        alias = "play",
        alias = "resume",
        alias = "stop",
        alias = "toggle"
    )]
    PlayPause,
    #[serde(alias = "skip", alias = "forward", alias = "next_track")]
    Next,
    #[serde(alias = "back", alias = "prev", alias = "previous_track")]
    Previous,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaArgs {
    pub action: MediaAction,
}

/// Tách phân giải tham số khỏi việc bấm phím.
///
/// Chia đôi như vậy để **test được mà không đổi âm lượng máy đang chạy test** —
/// một test gọi thẳng `control_volume` sẽ thật sự vặn loa của người chạy nó.
fn parse_volume(raw: Value) -> Result<(VolumeAction, u8), String> {
    let args: VolumeArgs =
        serde_json::from_value(raw).map_err(|e| format!("Validation error: {}", e))?;
    let steps = match args.action {
        VolumeAction::Mute => 1,
        _ => {
            let requested = args.steps.unwrap_or(DEFAULT_STEPS);
            if requested == 0 {
                return Err("'steps' phải từ 1 đến 10".to_string());
            }
            requested.min(MAX_STEPS)
        }
    };
    Ok((args.action, steps))
}

fn parse_media(raw: Value) -> Result<MediaAction, String> {
    let args: MediaArgs =
        serde_json::from_value(raw).map_err(|e| format!("Validation error: {}", e))?;
    Ok(args.action)
}

/// Bấm một phím ảo `presses` lần (nhấn + nhả mỗi lần).
///
/// Trả `Err` khi Windows nhận ít sự kiện hơn số đã gửi — thường là do một tiến
/// trình quyền cao hơn đang chặn (UIPI). Báo thẳng thay vì coi như xong: đây
/// đúng chỗ mà "thành công giả" hay lọt vào.
#[cfg(windows)]
fn press_key(vk: u16, presses: u8) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    };

    let make = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    for _ in 0..presses {
        let events = [make(0), make(KEYEVENTF_KEYUP)];
        // SAFETY: `events` sống hết lời gọi, `cbsize` đúng bằng kích thước một
        // phần tử như API yêu cầu.
        let sent = unsafe {
            SendInput(
                events.len() as u32,
                events.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if sent as usize != events.len() {
            return Err(format!(
                "Windows chỉ nhận {sent}/{} sự kiện phím — có thể một cửa sổ quyền cao hơn \
                 đang chặn (UIPI). Không có gì được thay đổi.",
                events.len()
            ));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn press_key(_vk: u16, _presses: u8) -> Result<(), String> {
    Err("Điều khiển âm lượng/phát nhạc hiện chỉ cài đặt cho Windows.".to_string())
}

pub fn control_volume(raw_args: Value) -> Result<String, String> {
    let (action, steps) = parse_volume(raw_args)?;

    #[cfg(windows)]
    let vk = match action {
        VolumeAction::Up => VK_VOLUME_UP,
        VolumeAction::Down => VK_VOLUME_DOWN,
        VolumeAction::Mute => VK_VOLUME_MUTE,
    };
    #[cfg(not(windows))]
    let vk = 0u16;

    press_key(vk, steps)?;
    tracing::info!("[OsControl] âm lượng: {:?} × {}", action, steps);

    Ok(match action {
        VolumeAction::Up => format!("Đã tăng âm lượng {steps} nấc."),
        VolumeAction::Down => format!("Đã giảm âm lượng {steps} nấc."),
        // Mute là CÔNG TẮC: không đọc được trạng thái trước đó nếu không dựng
        // Core Audio, nên nói đúng thứ đã làm ("bật/tắt tiếng") thay vì khẳng
        // định kết quả ("đã tắt tiếng") mà có thể ngược.
        VolumeAction::Mute => "Đã gạt công tắc tắt/bật tiếng.".to_string(),
    })
}

pub fn control_media(raw_args: Value) -> Result<String, String> {
    let action = parse_media(raw_args)?;

    #[cfg(windows)]
    let vk = match action {
        MediaAction::PlayPause => VK_MEDIA_PLAY_PAUSE,
        MediaAction::Next => VK_MEDIA_NEXT_TRACK,
        MediaAction::Previous => VK_MEDIA_PREV_TRACK,
    };
    #[cfg(not(windows))]
    let vk = 0u16;

    press_key(vk, 1)?;
    tracing::info!("[OsControl] media: {:?}", action);

    Ok(match action {
        // Cũng là công tắc — xem ghi chú ở `Mute`.
        MediaAction::PlayPause => {
            "Đã gạt công tắc phát/tạm dừng cho ứng dụng đang phát.".to_string()
        }
        MediaAction::Next => "Đã chuyển sang bài kế tiếp.".to_string(),
        MediaAction::Previous => "Đã quay lại bài trước.".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Mọi test dưới đây CỐ TÌNH chỉ chạm tầng phân giải tham số. Gọi
    // `control_volume` trong test sẽ thật sự vặn loa của máy đang chạy `cargo
    // test` — kể cả trên CI.

    #[test]
    fn steps_mac_dinh_khi_khong_noi_ro() {
        let (action, steps) = parse_volume(json!({"action": "down"})).unwrap();
        assert_eq!(action, VolumeAction::Down);
        assert_eq!(steps, DEFAULT_STEPS);
    }

    #[test]
    fn steps_bi_kep_o_tran_an_toan() {
        let (_, steps) = parse_volume(json!({"action": "up", "steps": 250})).unwrap();
        assert_eq!(
            steps, MAX_STEPS,
            "một lệnh không được phép vặn âm lượng đi quá xa"
        );
    }

    #[test]
    fn steps_bang_khong_bi_tu_choi() {
        // 0 nấc là lệnh vô nghĩa: hoặc model hiểu sai, hoặc người dùng nói sai.
        // Báo lỗi còn hơn im lặng không làm gì rồi trả "đã giảm âm lượng".
        let loi = parse_volume(json!({"action": "down", "steps": 0})).unwrap_err();
        assert!(loi.contains("1 đến 10"), "nhận được: {loi}");
    }

    /// Hồi quy của ca đã đo: `{"action":"up","steps":"any"}`.
    #[test]
    fn steps_rac_roi_ve_mac_dinh_thay_vi_giet_lenh() {
        let (action, steps) = parse_volume(json!({"action": "up", "steps": "any"}))
            .expect("một trường phụ hỏng KHÔNG được làm hỏng lệnh có ý định rõ");
        assert_eq!(action, VolumeAction::Up);
        assert_eq!(steps, DEFAULT_STEPS);

        // Chuỗi chứa số thì vẫn đọc được — model hay bọc số trong dấu nháy.
        let (_, steps) = parse_volume(json!({"action": "down", "steps": "3"})).unwrap();
        assert_eq!(steps, 3);
    }

    /// Alias là hợp đồng: từ vựng model thật sự sinh phải về đúng hành động.
    #[test]
    fn alias_ve_dung_hanh_dong_chuan() {
        assert_eq!(
            parse_volume(json!({"action": "louder"})).unwrap().0,
            VolumeAction::Up
        );
        assert_eq!(
            parse_media(json!({"action": "pause"})).unwrap(),
            MediaAction::PlayPause
        );
        assert_eq!(
            parse_media(json!({"action": "play"})).unwrap(),
            MediaAction::PlayPause
        );
        assert_eq!(
            parse_media(json!({"action": "skip"})).unwrap(),
            MediaAction::Next
        );
    }

    #[test]
    fn mute_bo_qua_steps() {
        let (action, steps) = parse_volume(json!({"action": "mute", "steps": 9})).unwrap();
        assert_eq!(action, VolumeAction::Mute);
        assert_eq!(steps, 1, "mute là công tắc, không có khái niệm nấc");
    }

    /// `action` vẫn NGHIÊM, kể cả sau khi nới alias.
    ///
    /// Ranh giới cố ý: `steps` hỏng thì bỏ qua (ý định vẫn rõ), còn `action`
    /// hỏng thì **phải** báo lỗi — sai ở đó là sai ý định, và đoán bừa sẽ làm
    /// một việc người dùng không yêu cầu.
    ///
    /// Dùng từ ngoài cả danh sách alias: `"louder"` và `"stop"` từng nằm trong
    /// test này nhưng nay là alias hợp lệ (đo thấy model sinh chúng), nên chúng
    /// không còn chứng minh được điều gì.
    #[test]
    fn tu_vung_that_su_sai_bi_tu_choi() {
        assert!(parse_volume(json!({"action": "brighter"})).is_err());
        assert!(parse_volume(json!({"action": "maximum"})).is_err());
        assert!(parse_media(json!({"action": "shuffle"})).is_err());
        assert!(parse_media(json!({"action": "seek"})).is_err());
    }

    #[test]
    fn truong_la_bi_tu_choi() {
        let loi = parse_volume(json!({"action": "up", "level": 50})).unwrap_err();
        assert!(loi.contains("unknown field"), "nhận được: {loi}");
    }

    #[test]
    fn media_phan_giai_du_ba_hanh_dong() {
        assert_eq!(
            parse_media(json!({"action": "play_pause"})).unwrap(),
            MediaAction::PlayPause
        );
        assert_eq!(
            parse_media(json!({"action": "next"})).unwrap(),
            MediaAction::Next
        );
        assert_eq!(
            parse_media(json!({"action": "previous"})).unwrap(),
            MediaAction::Previous
        );
    }
}
