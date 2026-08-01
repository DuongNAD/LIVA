//! Tool thời tiết — nguồn duy nhất đi ra Internet trong catalog tool.
//!
//! # Vì sao một tool ĐÁM MÂY lại nằm trong một sản phẩm offline-first
//!
//! LIVA chạy hoàn toàn cục bộ, và mọi thứ đi ra Internet là **ngoại lệ cần lý
//! do**, không phải mặc định. Lý do ở đây: thời tiết là dữ liệu *thay đổi theo
//! giờ và theo vị trí* — không model cục bộ nào biết được, và không cách nào
//! đóng gói sẵn. Đây đúng loại việc mà một tool tồn tại để làm.
//!
//! Ba ràng buộc bù lại cho ngoại lệ đó, cả ba đều bắt buộc:
//!
//! 1. **Không khoá API, không đăng ký, không định danh người dùng.** Dùng
//!    Open-Meteo — gửi đi đúng một toạ độ, không kèm gì khác. Một API cần khoá
//!    sẽ buộc người dùng tạo tài khoản, và khoá đó thành một bí mật nữa phải
//!    quản; một API miễn phí đổi lấy telemetry thì còn tệ hơn.
//! 2. **Hỏng thì phải NÓI RÕ là mất mạng**, không được im lặng hay treo. Người
//!    dùng LIVA có thể đang offline hoàn toàn — đó là kịch bản BÌNH THƯỜNG của
//!    sản phẩm này, không phải sự cố.
//! 3. **Hạn chờ cứng.** Một tool treo sẽ kéo theo cả lượt chat, và bài học
//!    `smart_home` trong dự án này là fail-closed nhanh hơn fail-silent chậm.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Hạn chờ mỗi lượt gọi mạng. Ngắn có chủ đích: nếu máy đang offline thì
/// `reqwest` thường lỗi ngay, còn mạng chậm tới mức quá ngưỡng này thì việc
/// đúng là trả lời "không lấy được" chứ không bắt người dùng chờ.
const HAN_CHO: Duration = Duration::from_secs(6);

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherArgs {
    /// Tên địa điểm, ví dụ "Hà Nội", "Đà Nẵng", "Tokyo". Không dấu cũng được.
    pub location: String,
}

/// Mã thời tiết WMO → mô tả tiếng Việt.
///
/// Chỉ gom nhóm, không dịch đủ 100 mã: người dùng hỏi "hôm nay thế nào" cần
/// biết *có mưa không, nóng hay lạnh*, không cần phân biệt "mưa phùn nhẹ" với
/// "mưa phùn vừa". Mã lạ rơi về một câu trung tính thay vì để trống.
fn mo_ta_wmo(ma: i64) -> &'static str {
    match ma {
        0 => "trời quang",
        1..=3 => "có mây",
        45 | 48 => "sương mù",
        51..=57 => "mưa phùn",
        61..=67 => "mưa",
        71..=77 => "tuyết",
        80..=82 => "mưa rào",
        85 | 86 => "mưa tuyết",
        95..=99 => "dông",
        _ => "không rõ dạng thời tiết",
    }
}

fn loi_mang(ngu_canh: &str, e: impl std::fmt::Display) -> String {
    // Nói thẳng "cần Internet" trong thông điệp: người dùng LIVA hoàn toàn có
    // thể đang offline có chủ đích, và họ cần biết đây là giới hạn chứ không
    // phải LIVA hỏng.
    format!(
        "Không lấy được thời tiết ({ngu_canh}) — tool này cần Internet, mọi phần \
         khác của LIVA vẫn chạy offline bình thường. Chi tiết: {e}"
    )
}

/// Tra toạ độ từ tên địa điểm.
async fn tim_toa_do(client: &reqwest::Client, ten: &str) -> Result<(f64, f64, String), String> {
    let url = "https://geocoding-api.open-meteo.com/v1/search";
    let res = client
        .get(url)
        .query(&[("name", ten), ("count", "1"), ("language", "vi")])
        .timeout(HAN_CHO)
        .send()
        .await
        .map_err(|e| loi_mang("tra toạ độ", e))?;
    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| loi_mang("đọc kết quả toạ độ", e))?;
    let hit = body["results"]
        .get(0)
        .ok_or_else(|| format!("Không tìm thấy địa điểm '{ten}'."))?;
    let lat = hit["latitude"]
        .as_f64()
        .ok_or_else(|| "Kết quả toạ độ thiếu latitude".to_string())?;
    let lon = hit["longitude"]
        .as_f64()
        .ok_or_else(|| "Kết quả toạ độ thiếu longitude".to_string())?;
    // Tên chuẩn hoá do API trả về, kèm quốc gia — để người dùng phát hiện ngay
    // khi nó tra nhầm thành phố trùng tên.
    let nhan = match hit["country"].as_str() {
        Some(qg) if !qg.is_empty() => format!("{}, {qg}", hit["name"].as_str().unwrap_or(ten)),
        _ => hit["name"].as_str().unwrap_or(ten).to_string(),
    };
    Ok((lat, lon, nhan))
}

/// Lấy thời tiết hiện tại. Trả về một câu tiếng Việt đọc lên được — đầu ra này
/// đi thẳng vào lượt nói của LIVA nên không được là JSON thô.
pub async fn get_weather(arguments: serde_json::Value) -> Result<String, String> {
    let args: WeatherArgs = serde_json::from_value(arguments)
        .map_err(|e| format!("Tham số get_weather không hợp lệ: {e}"))?;
    let ten = args.location.trim();
    if ten.is_empty() {
        return Err("Cần cho biết địa điểm, ví dụ: thời tiết Hà Nội.".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(HAN_CHO)
        .build()
        .map_err(|e| format!("Không dựng được HTTP client: {e}"))?;

    let (lat, lon, nhan) = tim_toa_do(&client, ten).await?;

    let res = client
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            (
                "current",
                "temperature_2m,relative_humidity_2m,weather_code".to_string(),
            ),
            ("timezone", "auto".to_string()),
        ])
        .timeout(HAN_CHO)
        .send()
        .await
        .map_err(|e| loi_mang("gọi API thời tiết", e))?;
    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| loi_mang("đọc kết quả thời tiết", e))?;

    let cur = &body["current"];
    let nhiet = cur["temperature_2m"]
        .as_f64()
        .ok_or_else(|| "Kết quả thiếu nhiệt độ".to_string())?;
    let am = cur["relative_humidity_2m"].as_f64();
    let ma = cur["weather_code"].as_i64().unwrap_or(-1);

    let mut cau = format!("{nhan}: {:.0}°C, {}", nhiet, mo_ta_wmo(ma));
    if let Some(am) = am {
        cau.push_str(&format!(", độ ẩm {:.0}%", am));
    }
    cau.push('.');
    Ok(cau)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mo_ta_wmo_gom_nhom_dung_va_khong_de_trong_ma_la() {
        assert_eq!(mo_ta_wmo(0), "trời quang");
        assert_eq!(mo_ta_wmo(2), "có mây");
        assert_eq!(mo_ta_wmo(63), "mưa");
        assert_eq!(mo_ta_wmo(95), "dông");
        // Mã ngoài bảng KHÔNG được trả chuỗi rỗng: một câu trống đi thẳng vào
        // lượt nói của LIVA sẽ thành "Hà Nội: 30°C, ." — vô nghĩa mà không lỗi.
        assert_eq!(mo_ta_wmo(12345), "không rõ dạng thời tiết");
        assert_eq!(mo_ta_wmo(-1), "không rõ dạng thời tiết");
    }

    #[tokio::test]
    async fn dia_diem_rong_bi_tu_choi_truoc_khi_cham_mang() {
        // Quan trọng: phải trượt ở tầng tham số, KHÔNG được đi ra Internet rồi
        // mới hỏng — nếu không, test này sẽ nhấp nháy theo tình trạng mạng của
        // máy chạy, đúng lớp lỗi đã cắn ba lần trong dự án.
        let err = get_weather(serde_json::json!({ "location": "   " }))
            .await
            .unwrap_err();
        assert!(err.contains("Cần cho biết địa điểm"), "được: {err}");
    }

    #[tokio::test]
    async fn tham_so_sai_kieu_bi_tu_choi_ro_rang() {
        let err = get_weather(serde_json::json!({ "location": 42 }))
            .await
            .unwrap_err();
        assert!(err.contains("không hợp lệ"), "được: {err}");
    }

    /// Kiểm THẬT qua Internet. `#[ignore]` có chủ đích — chạy tay bằng
    /// `cargo test -- --ignored weather_that`.
    ///
    /// **Vì sao KHÔNG để nó chạy trong CI:** ngày 01/08/2026, bước UI Tests đỏ
    /// vì UnoCSS đi lấy font từ `fonts.googleapis.com` và hết hạn chờ — một
    /// cổng phụ thuộc mạng biến sự cố ngoài thành "mã hỏng". Bốn test trên đã
    /// phủ toàn bộ phần logic; phần còn lại là API của người khác, không phải
    /// thứ CI nên khẳng định.
    #[tokio::test]
    #[ignore = "chạm Internet — chạy tay, không để CI phụ thuộc mạng"]
    async fn weather_that_lay_duoc_nhiet_do_that() {
        let s = get_weather(serde_json::json!({ "location": "Hà Nội" }))
            .await
            .expect("phải lấy được thời tiết Hà Nội");
        assert!(s.contains("°C"), "được: {s}");
        assert!(s.contains("Hà Nội") || s.contains("Hanoi"), "được: {s}");
        println!("  → {s}");
    }

    #[tokio::test]
    async fn truong_la_bi_tu_choi_deny_unknown_fields() {
        let err = get_weather(serde_json::json!({ "location": "Hà Nội", "api_key": "x" }))
            .await
            .unwrap_err();
        assert!(err.contains("không hợp lệ"), "được: {err}");
    }
}
