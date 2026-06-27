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

## Follow-up — 2026-06-25T04:43:55Z

# Teamwork Project Prompt — Final

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Design and implement a mobile client application and network connection configuration for LIVA, specifically optimized for an Android device (Samsung S24+). The client should connect to the existing central Rust WebSocket server.

Working directory: E:\Project\LIVA\mobile_client
Integrity mode: benchmark

## Requirements

### R1. Flexible Mobile Tech Stack
The agent team must design and build the mobile client using the technology stack they deem most appropriate (e.g., Native Kotlin/Compose, React Native, Flutter, or PWA) for a Samsung S24+.

### R2. Network Topology & Connection
The agent team must propose and implement a reliable network configuration that allows the mobile phone to securely connect to the case machine's WebSocket server (whether via local LAN discovery or a secure remote tunnel).

### R3. Standardized UI/UX
The user interface should follow modern design standards, providing a functional and responsive experience tailored for mobile screens.

## Acceptance Criteria

### Design & Architecture
- [ ] A mobile architecture design document (`MOBILE_CLIENT_DESIGN.md`) is provided, detailing the chosen tech stack, UI layout, and network topology.

### Implementation & Verification
- [ ] The mobile client source code is provided and can be built/compiled without errors.
- [ ] Any required network configuration scripts (e.g., tunneling setup or local IP binding configs) are provided.
- [ ] An automated test or verification script demonstrates that the chosen mobile framework can successfully establish a WebSocket handshake with the `liva-native-core` server.


## Follow-up — 2026-06-25T12:00:36Z

# Teamwork Project Prompt — Draft

Extract the underlying voice cloning model from OmniVoice-Studio, convert it to ONNX format, and implement a native Rust inference pipeline using the `ort` crate for the LIVA project.

Working directory: E:\Project\LIVA\teamwork_projects\omnivoice_poc
Integrity mode: benchmark

## Requirements

### R1. Model Extraction & Export
Analyze the OmniVoice-Studio codebase to identify the core zero-shot TTS model. Write a Python script that exports this model (or its sub-components) into standard ONNX format. The script should be reproducible.

### R2. Rust Native Inference CLI
Create a standalone Rust application using the `ort` crate that loads the exported ONNX model(s). It should accept a reference audio file (for voice cloning) and a text string as input, and output the synthesized speech as a `.wav` file. 

## Acceptance Criteria

### Programmatic Verification
- [ ] A Python script `export_onnx.py` exists, runs without crashing, and successfully produces `.onnx` files.
- [ ] A Rust Cargo project exists and successfully compiles with `cargo build`.
- [ ] Running the Rust application with a test text string and a 3-second reference audio file successfully outputs a playable `.wav` file without runtime errors.
- [ ] The generated `.wav` file's audio duration roughly matches the expected length of the spoken text.

## Follow-up — 2026-06-27T01:56:42Z

Kiểm thử toàn diện và chi tiết dự án LIVA để phát hiện các lỗi (bugs), vấn đề về hiệu suất, bảo mật hoặc kiến trúc.

Working directory: e:\Project\LIVA

## Requirements

### R1. Frontend & E2E Testing
Sử dụng `browser` subagent để truy cập, tương tác và kiểm thử thực tế các luồng chức năng trên giao diện web của LIVA. Cần kiểm tra kỹ các lỗi hiển thị, lỗi logic ở client-side và UX.

### R2. Backend & API Testing
Thực thi các test suite hiện có của dự án (nếu có). Viết và chạy thêm các test script mới cho các API endpoint hoặc logic backend quan trọng nếu phát hiện thiếu coverage. Báo cáo các lỗi runtime, lỗ hổng bảo mật hoặc hiệu năng.

### R3. Code Review & Kiến trúc
Đọc mã nguồn và sử dụng các công cụ (như GitNexus) để phân tích tĩnh. Đối chiếu mã nguồn với các quy tắc trong `AGENTS.md` và Obsidian Vault (đặc biệt chú ý đến tiến độ di chuyển sang Rust `liva-native-core`).

## Acceptance Criteria

### Verification & Báo cáo
- [ ] Tương tác trình duyệt: Thực sự gọi `browser` subagent và cung cấp kết quả màn hình/ghi nhận lỗi UI.
- [ ] Test execution: Có log chạy test (có sẵn hoặc tự viết mới) được đính kèm hoặc trích dẫn.
- [ ] Architectural Audit: Có phần đánh giá sự tuân thủ kiến trúc của codebase so với các yêu cầu trong hệ thống `AGENTS.md`.
- [ ] Báo cáo tổng hợp: Phải tạo ra một file artifact `liva_test_report.md` chi tiết liệt kê mọi lỗi, lỗ hổng, hoặc điểm yếu kiến trúc đã tìm thấy.



