---
title: "Báo cáo phát hành v1.0.0"
updated: 2026-08-04
commit: 596e8b6
status: living
owns:
  - bao-cao-phat-hanh-v1
covers:
  - CHANGELOG.md
  - liva-desktop/src-tauri/tauri.conf.json
  - scripts/check-installer-config.mjs
---

# Báo cáo phát hành v1.0.0

Ngày kiểm tra: 2026-08-03

## Artifact cục bộ

- Lệnh dựng: `npm run installer:windows`
- Kết quả: thành công
- Bundle: `target/release/bundle/nsis/LIVA_1.0.0_x64-setup.exe`
- Dung lượng: 241.162.341 byte (229,99 MiB)
- SHA-256: `8381FFCB0CE8A7385DD871FCD5C58C3A0E8D9A45666EB4A1A491B47ED8968086`

Artifact hiện tại là bản CPU mặc định; model được tải riêng khi sử dụng nên kích thước không bao gồm CUDA/cuBLAS hoặc toàn bộ model.

## Smoke test home trống trên máy phát triển

Ứng dụng được chạy từ working directory riêng với `LIVA_HOME` không chứa model:

- Lần chạy hoàn toàn đầu tiên hiển thị hộp thoại `LIVA — SAO LƯU khoá mã hoá` để giao recovery key cho người dùng.
- Sau khi device key đã được tạo, lần chạy kế tiếp chỉ thấy `LIVA Widget` ở giây 2 và 5.
- Ở giây 10, các cửa sổ nhìn thấy là `LIVA Widget` và `LIVA — Chuẩn bị lần đầu`; Dashboard không xuất hiện.

Luồng này được giữ bằng test policy Rust và test cấu hình installer: model bắt buộc còn thiếu thì chọn Setup, còn Dashboard phải khởi tạo với `visible: false`. Kết quả trên máy phát triển không thay thế nghiệm thu cài đặt trên Windows sạch.

## Nghiệm thu còn lại trên Windows sạch

- [ ] Commit thay đổi phát hành và tạo tag `v1.0.0`.
- [ ] Xác nhận workflow `release.yml` chạy xanh và tải artifact NSIS từ GitHub Release.
- [ ] Cài trên Windows 11 không có Rust, LLVM, CUDA hoặc Node.js.
- [ ] Ghi dung lượng tải thực tế và thời gian cài đặt.
- [ ] Xác nhận ứng dụng khởi động được.
- [ ] Chụp màn hình đầu tiên khi máy chưa có model; ghi rõ CTA và thông báo lỗi nếu có.

Không đánh dấu V1 hoàn tất trước khi sáu mục trên có bằng chứng.
