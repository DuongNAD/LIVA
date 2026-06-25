# Original User Request

## Initial Request — 2026-06-25T10:06:42+07:00

# Teamwork Project Prompt — Draft

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Hoàn thiện toàn bộ dự án nâng cấp `liva-native-core` (bao gồm tích hợp Local LLM Llama.cpp, tối ưu hóa code) và xuất báo cáo tổng kết dạng HTML Premium trực quan.

Working directory: E:\Project\LIVA\liva-native-core
Integrity mode: benchmark

## Requirements

### R1. Tích hợp Llama.cpp Local LLM
Cài đặt và nhúng thư viện `llama-cpp-2` vào pipeline để LIVA có thể xử lý ngôn ngữ tự nhiên ngay trên máy (Local) thông qua StateGraph, thay thế hoàn toàn AI Engine bằng Python cũ.

### R2. Tối ưu hoá & Dọn dẹp Code
Thực hiện dọn dẹp các mã thừa (Dead code/unused warnings) xuất hiện trong lúc `cargo check`. Tối ưu hoá hiệu năng xử lý luồng (Thread pool) của Tokio.

### R3. Báo cáo HTML Premium (Final Report)
Tạo một file báo cáo tĩnh tại `static/report.html`. Báo cáo này phải tổng hợp toàn bộ kiến trúc lõi vừa nâng cấp (Native MCP, State-Graph, WebRTC, Local LLM). Yêu cầu thiết kế UI/UX xuất sắc (Sử dụng Glassmorphism, Dark mode, Animation mượt mà và sơ đồ kiến trúc động/tương tác).

## Acceptance Criteria

### Đánh giá tự động & Trực quan (Programmatic Verification)
- [ ] Lệnh `cargo check` và `cargo test` chạy qua toàn bộ project và không còn xuất hiện bất kỳ cảnh báo (0 warnings) hay lỗi nào.
- [ ] File `static/report.html` phải tồn tại, hiển thị hoàn hảo trên trình duyệt với đầy đủ hiệu ứng CSS cao cấp như yêu cầu, không cần phụ thuộc framework bên ngoài.
- [ ] Code có khả năng tự động tải và chạy mô hình ngôn ngữ Llama (GGUF) thông qua FFI bindings một cách trơn tru.
