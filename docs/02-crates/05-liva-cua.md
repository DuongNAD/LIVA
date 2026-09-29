---
title: "Tài liệu Kỹ thuật Crate: liva-cua"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-cua/Cargo.toml
  - crates/liva-cua/src/lib.rs
---

# Tài liệu Kỹ thuật Crate: liva-cua

## 1. Tổng quan về Động cơ Tự động hóa Desktop CUA

`crates/liva-cua` là động cơ **Computer-Use Agent (CUA)** bản địa chạy trên nền tảng Windows, cung cấp năng lực điều khiển chuột, bàn phím và tương tác với các ứng dụng máy tính một cách tự động, chính xác và có kiểm soát an toàn nghiêm ngặt.

### Đặc điểm thiết kế:
- **Native Win32 Integration**: Sử dụng trực tiếp các giao diện cấp thấp của Windows thông qua crate `windows-sys` (SendInput, GetCursorPos, WindowFromPoint, DWM composition), mang lại độ trễ thao tác dưới $5\text{ ms}$.
- **Nhận biết DPI Đa màn hình (Per-Monitor DPI-Aware)**: Tự động chuyển đổi tọa độ điểm ảnh vật lý và tọa độ ảo trên các màn hình có tỷ lệ thu phóng (DPI Scaling 100%, 125%, 150%, 200%) khác nhau.
- **Rào chắn An toàn Đa tầng (Multi-Layer Safety Guards)**: Tích hợp công tắc ngắt khẩn cấp phần cứng (`KillSwitchController`) và sổ cái kiểm toán bất biến ghi nhận mọi thao tác vào cơ sở dữ liệu SQLite.

---

## 2. Kiến trúc và Các Phân hệ Cốt lõi

Cấu trúc đối tượng `CuaEngine` kết hợp 4 phân hệ chính:

```
┌─────────────────────────────────────────────────────────────┐
│                          CuaEngine                          │
├──────────────────────────────┬──────────────────────────────┤
│ SecurityGovernor             │ KillSwitchController         │
│ - Kiểm tra UIPI              │ - Luồng OS riêng biệt        │
│ - Ngăn ngừa thao tác cấm     │ - Phím ngắt tức thì VK_ESCAPE│
├──────────────────────────────┼──────────────────────────────┤
│ CuaRouter                    │ CuaAuditRecorder             │
│ - Định tuyến DirectWin32     │ - Đồng bộ qua DbActor        │
│ - Dự phòng OCR / VLM         │ - Ghi nhận sổ cái kiểm toán  │
└──────────────────────────────┴──────────────────────────────┘
```

### 2.1. Công tắc Ngắt Khẩn cấp (`kill_switch.rs`)
- Để đảm bảo người dùng luôn nắm quyền kiểm soát tuyệt đối và không bị ứng dụng chiếm quyền điều khiển trái ý muốn, một luồng OS độc lập liên tục thăm dò trạng thái phím **`VK_ESCAPE`** (hoặc tổ hợp phím nóng an toàn).
- Ngay khi phát hiện người dùng nhấn phím thoát, `KillSwitchController` ngay lập tức:
  1. Hủy bỏ tác vụ CUA đang chạy.
  2. Phát tín hiệu nhả toàn bộ các phím chuột và bàn phím ảo đang bị giữ (`SendInput` với cờ `KEYEVENTF_KEYUP` và `MOUSEEVENTF_LEFTUP`).
  3. Đưa hệ thống về trạng thái an toàn.

### 2.2. Kiểm soát Đặc quyền và Trạng thái Cửa sổ (`guards.rs`)
- **Cách ly Đặc quyền Giao diện (UIPI - User Interface Privilege Isolation)**: Ngăn chặn tác tử AI gửi tín hiệu chuột/phím vào các cửa sổ có đặc quyền cao hơn (ví dụ: các cửa sổ Administrator hoặc màn hình bảo mật UAC của Windows) để tránh leo thang đặc quyền.
- **Bảo vệ Cửa sổ Hệ thống**: Chặn hoàn toàn việc tự động tương tác vào các tiến trình nhạy cảm như `taskmgr.exe`, các cài đặt bảo mật Windows Defender hoặc trình quản trị chứng chỉ số.

### 2.3. Định tuyến Tác vụ Ba Cấp (`router.rs`)
Khi nhận một mục tiêu giao diện người dùng (ví dụ: "Nhấn nút Lưu trong Notepad"):
1. **Tier 1 (DirectWin32)**: Sử dụng Win32 UI Automation để tìm trực tiếp phần tử điều khiển (Control Handle - `HWND`). Thực hiện thao tác nhanh chóng và chính xác $100\%$.
2. **Tier 2 (OcrFallback)**: Nếu phần tử không hỗ trợ Win32 UI Automation, hệ thống chụp ảnh màn hình vùng quan tâm và dùng OCR để xác định tọa độ nhãn chữ.
3. **Tier 3 (VlmFallback)**: Sử dụng mô hình thị giác Qwen3-VL-2B để định vị tọa độ hộp bao (Bounding Box) trên các giao diện phức tạp hoặc canvas đồ họa.

### 2.4. Sổ cái Kiểm toán Thao tác (`audit.rs`)
Mọi hành động nhấp chuột, gõ văn bản, kéo thả đều được đóng gói thành bản ghi `CuaActionRecord` và đẩy vào `DbActorHandle` của `crates/liva-storage` để ghi nhận vào bảng `action_audit_ledger`, phục vụ việc truy vết và đánh giá an toàn sau phiên làm việc.

---

## 3. Quy chuẩn Kiểm thử

Crate sở hữu bộ kiểm thử tự động toàn diện được gom trong `tests/harness.rs` (gồm 17 bộ kiểm thử độc lập) mô phỏng các trường hợp: biến đổi tọa độ DPI, kích hoạt công tắc ngắt khẩn cấp, ngăn chặn thao tác UIPI và xác minh tính toàn vẹn của sổ cái kiểm toán.
