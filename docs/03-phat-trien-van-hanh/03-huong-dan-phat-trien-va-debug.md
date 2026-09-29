---
title: "Hướng dẫn Môi trường Phát triển và Debug"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - .cargo/config.toml
  - package.json
---

# Hướng dẫn Môi trường Phát triển và Debug

## 1. Yêu cầu Tiên quyết (Prerequisites)

Để xây dựng và phát triển LIVA trên máy trạm Windows, cần cài đặt các công cụ sau:

- **Hệ điều hành**: Windows 10/11 64-bit (Build 19045 trở lên).
- **Bộ công cụ Rust**: Phiên bản Rust `1.85+` (cài đặt qua `rustup-init.exe`), bổ sung thành phần `rustfmt` và `clippy`.
- **Node.js**: Phiên bản Node.js `v20 LTS` trở lên kèm `npm 10+`.
- **Visual Studio 2022 C++ Build Tools**:
  - Gói tải "Desktop development with C++".
  - Bộ biên dịch MSVC v143 (x64/x86).
  - Windows 10/11 SDK.
  - LLVM / Clang (cho linker `lld-link` được cấu hình trong `.cargo/config.toml`).
- **Phần cứng GPU (Tùy chọn)**: Card đồ họa NVIDIA tối thiểu 6GB VRAM, cài đặt Driver NVIDIA bản 550+ và CUDA Toolkit 12.x.

---

## 2. Quy trình Cài đặt và Khởi động Môi trường

Thực hiện các bước thiết lập theo thứ tự:

```powershell
# 1. Cài đặt các gói phụ thuộc JavaScript/TypeScript
npm install

# 2. Tải về các mô hình ONNX cơ sở (VAD, GTCRN, Embedder)
npm run setup:models

# 3. Kiểm tra tính sẵn sàng của môi trường và phần cứng
npm run doctor

# 4. Biên dịch giao diện người dùng
npm run build:ui
```

---

## 3. Nguyên tắc Biên dịch Tuần tự và Giới hạn Tài nguyên

Để bảo vệ sự ổn định của máy tính phát triển và ngăn ngừa hiện tượng cạn RAM hoặc phình tệp PDB debug:
1. **Biên dịch tuần tự (Sequential Execution Only)**: Tuyệt đối không chạy đồng thời nhiều lệnh `cargo check`, `cargo build` hoặc `cargo test` cùng lúc trong các terminal khác nhau.
2. **Khống chế tài nguyên bằng cờ `-j 2`**: Luôn thêm cờ `-j 2` khi thực hiện các lệnh cargo để giới hạn tối đa 2 luồng biên dịch:
   ```powershell
   cargo check --workspace -j 2
   cargo test --workspace -j 2 -- --test-threads 2
   ```
3. **Cấu hình Linker nhanh trong `.cargo/config.toml`**: Dự án đã cấu hình cờ `-C link-arg=-fuse-ld=lld` trên nền tảng MSVC để rút ngắn thời gian liên kết thư viện từ hàng phút xuống còn vài giây.

---

## 4. Kỹ thuật Debug và Ghi vết Hệ thống (Tracing & Logging)

LIVA sử dụng thư viện `tracing` và `tracing-subscriber` của Rust để ghi nhật ký có cấu trúc:

### 4.1. Bật Log Chi tiết qua Biến Môi trường
Trước khi khởi chạy tiến trình, đặt mức chi tiết mong muốn qua PowerShell:
```powershell
# Bật log cấp độ DEBUG cho toàn bộ các crate nội bộ LIVA
$env:RUST_LOG = "info,liva_native_core=debug,liva_storage=debug,liva_llm=debug,liva_cua=debug"

# Khởi chạy native core độc lập để xem luồng log trực tiếp
cargo run -p liva-native-core
```

### 4.2. Debug Giao diện Tauri và IPC Channels
- Khi chạy ứng dụng ở chế độ phát triển:
  ```powershell
  cd liva-desktop
  npx tauri dev
  ```
- Nhấn phím `F12` hoặc nhấp chuột phải vào cửa sổ ứng dụng chọn **Inspect Element** để mở bộ công cụ DevTools của Microsoft Edge WebView2.
- Tab **Console** sẽ hiển thị chi tiết các sự kiện streaming `ipc-stream:*`, trạng thái kết nối `TauriAdapter` và dữ liệu viseme 3D avatar.
