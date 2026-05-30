# 01. Tổng Quan Hệ Thống LIVA (System Overview)

**Phiên bản: v29 Enterprise-Ready Cognitive OS**

Tài liệu này cung cấp cái nhìn toàn cảnh về kiến trúc hệ thống của LIVA (Liva Intelligent Virtual Assistant), một trợ lý ảo đa đặc vụ (multi-agent) hoạt động trên Desktop (Windows & macOS). LIVA sử dụng triết lý **Hybrid Intelligence**, kết hợp linh hoạt giữa khả năng suy luận cục bộ (Local GPU) và sức mạnh đám mây (Cloud API).

## 1. Triết Lý Thiết Kế (Design Philosophy)

- **Trí Tuệ Lai (Hybrid Intelligence)**: LIVA không bị giới hạn trong việc chỉ chạy Local hoặc chỉ dùng Cloud. Mô-đun `ModelOrchestrator` tự động quyết định môi trường thực thi dựa trên phần cứng khả dụng (VRAM, RAM) và độ phức tạp của tác vụ (Routing bằng L0.5 Semantic Action Cache).
- **Tối Ưu Phần Cứng (Zero-Leak & Zero-VRAM Overhead)**: Toàn bộ hệ thống quản lý bộ nhớ và tiện ích được thiết kế tách biệt khỏi GPU. Ví dụ, `EmbeddingWorker` dùng mô hình CPU ONNX `onnxruntime-node` để tiết kiệm 100% VRAM cho LLM Core. Hệ thống cũng có VRAMGuard tự động giải phóng mô hình khi người dùng mở các phần mềm nặng (Gaming/Render).
- **Giao Diện Trong Suốt (Ghost Mode)**: Frontend Vue 3 sử dụng Tauri v2 (Rust Host) thay vì Electron. Ứng dụng chạy mượt mà dưới dạng widget Desktop trong suốt, không chiếm dụng tài nguyên OS, hỗ trợ click-through.
- **Micro-Services In-Process**: Thay vì triển khai qua Docker (gây tốn 2-4GB vmmem), mọi sandbox tiến hoá, plugin MCP, và background worker đều chạy dưới dạng Node.js `worker_threads` hoặc WASI `isolated-vm` in-process.

## 2. Năm Trụ Cột Tối Ưu Hardware & UX (Ambient Cognitive OS)

Trong phiên bản v24-v29, kiến trúc hệ thống đã được thiết kế lại xoay quanh 5 trụ cột cốt lõi:

### Trụ cột 1: Preemptive VRAM Yielding (VRAMGuard)
- LIVA hoạt động ngầm thông qua `AppWatcherService`. Khi phát hiện người dùng khởi chạy các ứng dụng được đưa vào danh sách trắng (Whitelist) cần nhiều tài nguyên như game AAA hoặc phần mềm đồ hoạ (Blender/Premiere), hàm `CoreKernel.yieldVRAM()` sẽ được kích hoạt.
- Hệ thống tự động tắt tiến trình `llama-server` (giải phóng 100% VRAM) và định tuyến toàn bộ tác vụ suy luận sang Cloud API (Gemini/Groq).
- Khi ứng dụng nặng đóng lại, hàm `CoreKernel.reclaimVRAM()` được gọi để warm-up lại mô hình Local. Trải nghiệm người dùng hoàn toàn không bị gián đoạn.

### Trụ cột 2: Semantic Action Cache L0.5
- Được tích hợp vào `SemanticRouter`, lớp L0.5 sử dụng SQLite để cache các cặp `[vector_truy_vấn] -> [tên_công_cụ, tham_số]`.
- Mọi yêu cầu đơn giản có độ tương đồng Cosine > 0.95 với lịch sử sẽ đi thẳng từ Router đến `SkillRegistry` (< 5ms), bỏ qua hoàn toàn bước suy luận bằng LLM.

### Trụ cột 3: Wake-Word Edge Offloading (LivaWakeWorker)
- Đưa tính năng phát hiện từ khóa đánh thức ("Hey Liva") xuống Frontend. Sử dụng mô hình `hey_liva.onnx` biên dịch qua WebAssembly chạy ngay trên Vue 3.
- Micro của UI luôn bật (Always-On) để đảm bảo Full-Duplex, nhưng hoàn toàn KHÔNG gửi dữ liệu Audio Base64 qua WebSocket nếu chưa kích hoạt wake-word. Zero CPU/GPU usage ở Backend.

### Trụ cột 4: On-Demand Zero-Trust Vision
- Thay vì truyền phát liên tục màn hình Desktop, tính năng Vision chỉ được kích hoạt nếu `SemanticRouter` phát hiện các từ khoá chỉ định (deictic keywords) như "cái này", "đoạn code trên màn hình".
- Tauri WebView sau đó gửi lệnh chụp ảnh một khung hình (1 frame) nén WebP. Tính năng xử lý cục bộ làm mờ các mật khẩu, thẻ tín dụng trước khi gọi Cloud Vision. An toàn 100%.

### Trụ cột 5: Sequential Hot-Swap (v29)
- **Single Model on VRAM**: Tránh lỗi OOM trên GPU bằng cách chỉ tải 1 model vào VRAM tại một thời điểm.
- Hỗ trợ đổi model nhanh (Hot-Swap) từ model Router nhỏ (4B) sang model Expert lớn (26B) thông qua NativeIPCClient và cơ chế Memory-Mapped (`mmap`).
- **Expert Cooldown TTL**: Model Expert được giữ lại trong VRAM 120-180 giây sau tác vụ cuối cùng để chờ các câu hỏi tiếp theo (latency masking), sau đó tự động swap ngược lại Router để giải phóng tài nguyên.

## 3. Các Thành Phần Chính của Gateway

Toàn bộ Backend được triển khai bằng Node.js v22+ (ESM Strict TypeScript), chia thành 6 khu vực độc lập:

1. **Core Kernel (`src/core`)**: Não bộ điều phối vòng đời của Agent. Quản lý trạng thái (`AgentLoop`), luồng Stream (`StreamSanitizer`, `ToolCallExtractor`), và Giao thức giao tiếp đa đặc vụ (`LACPProtocol`).
2. **LIVA-UHM Memory (`src/memory`)**: Hệ thống bộ nhớ 4 tầng lưu trong một file `node:sqlite` duy nhất. Hoạt động bất đồng bộ với các Daemon (`DualChannelSegmenter`, `ReconsolidationEngine`). (Xem chi tiết tại 02_Memory_Subsystem.md).
3. **Security Guardrails (`src/security`)**: Cổng an ninh `ZMASGuard`, `EncryptionEngine` (AES-256-GCM), và cơ chế xác thực chặn tiêm prompt (Sanitize Sensory Data).
4. **Skills / Plugins (`src/skills`)**: 93+ MCP Tools được phân cụm rành mạch (Agentic, DevOps, System, Web). Hoạt động dưới hệ thống Circuit Breaker chống sập toàn hệ thống nếu API bên thứ 3 lỗi.
5. **Evolution & Singularity (`src/evolution`)**: Khả năng "Tự cải thiện mã nguồn". Sử dụng `ASTCodeSurgeon` để phẫu thuật AST, an toàn với `MicroVMDaemon` và Rollback Physical Snapshot.
6. **Peripheral Services (`src/services`)**: Các tiện ích ngoại vi kết nối Zalo, Telegram, Xử lý âm thanh (Whisper STT, Edge-TTS/Kokoro).

## 4. Giao Tiếp Kép (Dual Communication)

- **UI ↔ Gateway**: Sử dụng WebSocket tại cổng `8082`. Tauri App giao tiếp bằng chuỗi JSON và trao đổi Binary PCM Audio.
- **Gateway ↔ Engine**: Giao thức gRPC (Dữ liệu lớn tốc độ cao tới Native Engine) và HTTP REST (Tương thích OpenAI `/v1/chat/completions` cho Cloud API/llama-server).

## 5. Kết Luận
Bằng việc loại bỏ những thư viện rác tốn tài nguyên (Electron, Docker, LanceDB, Puppeteer) thay bằng các công cụ Low-level gọn nhẹ (Tauri, WASI, sqlite-vec, Playwright-core) và cơ chế Hot-Swap tối ưu bộ nhớ, LIVA v29 đạt được hiệu năng ngang ngửa các công cụ doanh nghiệp lớn trên đám mây, nhưng vẫn hoạt động mượt mà trên Desktop cá nhân.
