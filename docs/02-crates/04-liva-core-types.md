---
title: "Tài liệu Kỹ thuật Crate: liva-core-types"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-core-types/Cargo.toml
  - crates/liva-core-types/src/lib.rs
---

# Tài liệu Kỹ thuật Crate: liva-core-types

## 1. Tổng quan và Nguyên tắc Thiết kế

`crates/liva-core-types` là crate nền tảng chứa các định nghĩa kiểu dữ liệu thuần túy (Domain Models), cấu trúc hợp đồng IPC, và phân loại quyết định ngắt lượt của LIVA.

### Nguyên tắc cốt lõi:
- **Zero Heavy Dependencies**: Crate không phụ thuộc vào các thư viện tính toán máy học nặng (như `ort`, `llama-cpp-2`), thư viện cơ sở dữ liệu (`rusqlite`) hay API hệ điều hành đồ họa. Nhờ đó, thời gian biên dịch của crate chỉ tính bằng mili-giây.
- **Single Source of Truth cho IPC**: Mọi cấu trúc dữ liệu dùng để giao tiếp giữa Rust và giao diện người dùng WebView2 đều được định nghĩa tại đây và trang trí với thuộc tính hỗ trợ sinh kiểu dữ liệu TypeScript.
- **Tương thích Lược đồ Công cụ (Tool Schema Contracts)**: Hỗ trợ sinh lược đồ JSON Schema chuẩn thông qua `schemars` phục vụ việc gọi công cụ (Tool Calling / Function Calling) của các mô hình ngôn ngữ lớn.

---

## 2. Các Kiểu Dữ liệu Điển hình

### 2.1. Quyết định Ngắt lượt Thích ứng (`AdaptiveTurnDecision`)
Được sử dụng bởi máy trạng thái thoại full-duplex (`turn_taking.rs`) và tầng giao tiếp IPC:
```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AdaptiveTurnDecision {
    /// Xác suất dứt câu p > 0.92. Ngắt lượt ngay sau 200ms khoảng lặng chuẩn.
    ImmediateCutoff { probability: f32 },
    /// Xác suất 0.50 <= p <= 0.92. Người dùng đang ngập ngừng; nới rộng cửa sổ chờ tới 450ms.
    HesitationWait { probability: f32 },
    /// Xác suất p < 0.50. Câu nói chưa kết thúc; tiếp tục thu âm.
    Incomplete { probability: f32 },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TurnVerdict {
    pub probability: f32,
    pub complete: bool,
}
```

### 2.2. Lược đồ Tham số Kỹ năng (`WeatherArgs`)
Minh họa chuẩn cấu trúc tham số đầu vào cho kỹ năng thời tiết với tài liệu hóa lược đồ JSON tự động:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct WeatherArgs {
    #[schemars(description = "Tên thành phố hoặc vị trí cần xem thời tiết")]
    pub location: String,
    #[schemars(description = "Số ngày dự báo tương lai (mặc định là 1)")]
    pub days: Option<u32>,
}
```

---

## 3. Vai trò Hợp đồng trong Toàn bộ Workspace

Nhờ vị trí độc lập ở đáy cây phụ thuộc:
- `liva-native-core` sử dụng `liva-core-types` để định nghĩa trạng thái phiên hội thoại và phân loại tín hiệu thoại.
- `liva-desktop/src-tauri` sử dụng để tuần tự hóa các sự kiện phát ra cho giao diện frontend.
- `liva-tools` sử dụng để định dạng kết quả đo kiểm tra chuẩn hóa trên màn hình dòng lệnh.
