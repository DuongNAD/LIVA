# Original User Request

## Initial Request — 2026-06-27T11:16:08+07:00

Triển khai Giai đoạn 4 (mảnh ghép cuối cùng) của hệ thống LIVA: Self-Evolving Codebase. Xây dựng module Self-Correction Loop cho phép AI tự động chạy Unit Tests, đọc log lỗi, và vá mã nguồn (giới hạn tối đa 3 lần thử) để tự động hóa hoàn toàn quy trình phát triển mà không cần con người.

Working directory: e:\Project\LIVA\liva-native-core
Integrity mode: benchmark

## Requirements

### R1. Test Execution Sandbox
Viết một module (ví dụ `src/evolution/sandbox.rs`) có khả năng lập lịch khởi chạy lệnh `cargo test` dưới dạng tiến trình con (subprocess), sau đó bắt luồng kết quả đầu ra (stdout/stderr) để phân tích log một cách an toàn.

### R2. Self-Correction Loop
Xây dựng một vòng lặp logic (Self-Correction Loop). Nếu tiến trình test báo lỗi (Fail), module này phải trích xuất được dòng bị lỗi, gọi nội bộ một Mock Agent để sinh ra đoạn code sửa sai, và ghi đè lại file mã nguồn tương ứng. Quá trình này chỉ được lặp lại tối đa 3 lần (Max Retries = 3) để tránh kẹt vô tận.

### R3. Viết Unit Tests
Cung cấp Unit Tests đầy đủ chứng minh vòng lặp hoạt động.

## Acceptance Criteria

### Xác minh Kỹ thuật (Programmatic Verification)
- [ ] Lệnh `cargo build` trong thư mục `liva-native-core` biên dịch thành công không có lỗi.
- [ ] Viết một Unit Test đặc biệt: tạo ra một file `.rs` tạm thời chứa mã nguồn cố tình bị lỗi cú pháp. Truyền file này vào Self-Correction Loop và thiết lập Mock Agent luôn trả về đoạn code đã được sửa đúng. Vòng lặp phải phát hiện lỗi, chạy Mock Agent, sửa file, và trả về kết quả PASS (mô phỏng thành công).
- [ ] Lệnh `cargo test` chạy thành công toàn bộ module Self-Evolving này.
