# LIVA - Enterprise Desktop AI Assistant

LIVA là hệ thống Trợ lý ảo Desktop sở hữu năng lực tự động suy luận sâu, quản trị đa Agent và tự chủ vận hành (RPA) thông qua kiến trúc hệ thống Cục bộ (Local LLM) linh hoạt bậc nhất. LIVA được thiết kế với tư duy kỹ thuật thực dụng (Pragmatic Engineering), kết hợp giữa sức mạnh AI mã nguồn mở và khả năng tinh chỉnh sát sườn cấu hình phần cứng.

---

## 🌟 Chức năng Vùng Lõi Tiên Tiến (Core Architecture State-Of-The-Art)

Dự án đã trải qua các đợt đại tu "đập đi xây lại" nhằm đạt độ bền bỉ Zero-latency và an toàn cấp độ Server:

- 🏎️ **Adaptive Engine Selection (Single Expert Model):** Hệ thống triển khai kiến trúc 100% VRAM cho một mô hình chuyên gia duy nhất. `ModelOrchestrator` kết hợp với `llama-server` (C++) tự động nhận diện cấu hình phần cứng (RAM/VRAM/CPU) lúc khởi động để chọn chế độ hoạt động tối ưu (Local GGUF, Cloud API, hoặc Hybrid).
- 🎯 **Semantic Router & SQLite-Vec (C-Extension):** Loại bỏ hoàn toàn sự cồng kềnh của cơ sở dữ liệu vector bên ngoài. LIVA gom toàn bộ trí nhớ (L1/L2) vào 1 file `node:sqlite` duy nhất. Hệ thống sử dụng C-Extension `sqlite-vec` và `FTS5` để truy vấn vector siêu tốc, tự động lọc và chỉ dâng Top-5 MCP Tools khớp nhất lên System Prompt, chống phình ngốn Token tuyệt đối.
- 🛡️ **Bảo Mật HITL chống Prompt Injection:** Kỹ năng sinh tử `execute_command` được trang bị **Whitelist (Danh Sách Trắng)** cực kỳ khắt khe chặn đứng mọi kỹ thuật Obfuscation (Làm rối mã bằng dấu `^` hoặc Base64). Trước khi chạm đến lõi OS, hệ thống **Human-in-the-loop (HITL)** bắt buộc chặn đứng Terminal bằng biến `readline` đòi hỏi người quản trị (Admin) gõ `y/yes` để phê duyệt. Kháng độc và Hijack máy hoàn toàn!
- ♻️ **Self-Correction Guardrails (Hàng rào Tự Động Định Tuyến):** LIVA không bao giờ văng rớt khi kỹ năng đứt gãy. Khi một API Tool bị Crash bẩn, LIVA bọc nó lại thành Message thân thiện ném về System, ép Agent tự soi Lỗi (Reflection) và thử cách khác. LIVA cũng tích hợp **Deterministic Hash-Set** ngăn cấm LLM điền lại tham số sai ở Vòng Lặp Tử Thần (Doom Loop) và ấn định cảnh hạ cánh mềm (Graceful Exit) ở chu kỳ suy diễn số 5.

## 🛠️ Trạm Kỹ Năng Thông Minh (In-Process MCP Host)

LIVA trang bị rương kỹ năng khổng lồ (78+ Skills), được phân loại thành 5 phân viện chính và **hoạt động dưới chuẩn MCP (Model Context Protocol)** nội bộ (In-process MCP Host). Cơ chế Auto-Discovery cho phép nạp động kỹ năng vào RAM với độ trễ 0ms mà không tốn thêm bộ nhớ sinh tiến trình:

1. **⚙️ Hệ Điều Hành & Quản Trị Hệ Thống (Mức độ bảo mật cao)**
   - `ExecuteCommand`: Chạy shell/cmd (Có bọc HITL chốt chặn).
   - `ListDirectory`, `ReadLocalFile`, `WriteLocalFile`, `DeleteLocalFile`: Thanh tra và Thao túng File System Local.
   - `GetSystemInfo`: Đọc thông số RAM/Storage/CPU.
   - `GitSyncProject`: Đồng bộ hóa mã nguồn tự động với Github.

2. **🧠 AI Scientist & Lập Trình Tự Trị**
   - `AIScientist`: Vòng lặp lập trình tự trị (Autonomous coding loop).
   - `PlanWriter`, `ReportWriter`: Sinh báo cáo, phác thảo thiết kế thông minh chuẩn kỹ nghệ.
   - `ResearchIdeation`: Đào sâu logic, phản biện đa chiều cho các luận điểm kỹ thuật.

3. **💬 Giao Tiếp & Mạng Xã Hội (API-First, Zero-Trust)**
   - `SendZaloBot`, `ZaloPolling`: Tương tác qua Zalo Official Account Bot API (Loại bỏ hoàn toàn trình duyệt Headless để bảo vệ quyền riêng tư).
   - `ReadEmails`, `ReadRecentEmails`: Truy cập và tóm tắt Inbox (IMAP/Google).

4. **☁️ Tương tác Google Workspace**
   - `ReadGoogleSheet`, `WriteGoogleSheet`: Phân tích và nạp dữ liệu đa chiều.
   - `SearchGoogleDrive`, `CreateGoogleDoc`, `AppendGoogleDoc`: Thao tác soạn thảo văn bản trực tiếp với Google ecosystem.

5. **🌐 Khai Thác Mạng Chuyên Sâu**
   - `WebSearch`: Khai thác Internet và chắt lọc Content thời gian thực (API-based).
   - `GetWeather`: Khai thác Sensor thời tiết siêu cục bộ.

---

## 📂 Tổ Chức Modules Kiến Trúc

Dự án được chia thành 3 modules chính siêu chặt chẽ (Single Responsibility Principle - SRP):

1. **`liva-ui`** `(Vue 3 / Tauri v2 / Rust)`: 
   - Ứng dụng Desktop cho phép khởi chạy nhân vật hiển thị trong suốt (tàng hình nền) trên góc Desktop (Chiếm <50MB RAM).
   - Web Workers: Chịu trách nhiệm hiển thị giao diện, đồ họa 3D Animation và xử lý âm thanh đồng bộ môi Live2D tách biệt khỏi Main Thread.

2. **`openclaw-gateway`** `(Node.js / TypeScript)`:
   - Hệ thống Não bộ Trung gian (Gateway) với 3 nhánh: `ModelOrchestrator.ts` lo chạy mô hình, `PromptBuilder.ts` lo bối cảnh RAG, `AgentLoop.ts` chịu trách nhiệm vòng Quyết định.
   - Quản trị bộ nhớ `StructuredMemory` (sqlite-vec) và điều phối kỹ năng qua `LocalMCPServer`.

3. **`liva-ai-engine`** `(C++ / Python)`: 
   - Cốt lõi xử lý AI do `llama-server` (C++) đảm nhiệm để tối đa hóa hiệu suất Local Inference. Python (`edge-tts`) được sử dụng chuyên biệt cho hạ tầng Voice/TTS với cơ chế Circuit Breaker chống Event Loop blocking.

---

## ⚙️ Yêu cầu & Hướng dẫn Cài Đặt

### Yêu Cầu Cấu Hình
- **Node.js** (Khuyến nghị bản v22 trở lên, chuẩn ESM)
- **Python** (Bản v3.10 trở lên)
- **RAM**: Tối thiểu 16GB.
- Cấu hình Key mạng và Đường dẫn Folder Model nằm hoàn toàn ở `.env` của `openclaw-gateway`.

### Tiến Hành
1. **Cài đặt các dependencies**:
   - `liva-ui` & `openclaw-gateway`: chạy `npm install` 
   - `liva-ai-engine`: Cài đặt môi trường cho `llama-server` và `edge-tts`.

2. **Khởi chạy Hệ thống tự động (Automate Pipeline)**:
   - Ở thư mục gốc (`e:\Project\LIVA`), chạy `npm run desktop` (Tauri Sidecar).
   - Hệ thống tự động thiết lập phần cứng (AutoGPUSetup) và chọn AI_PROVIDER tối ưu.

---

## 🌿 Quản Lý Đa Nhánh (Git Versioning)

Dự án duy trì đồng thời 2 luồng phát triển song song nhưng có cấu trúc Git khác nhau:
- Nhánh **`main`**: Toàn bộ tinh hoa mã nguồn + Kho dữ liệu lưu trữ / Logs nặng đầy đủ.
- Nhánh **`lite`**: Một bản rẽ phái nhẹ tựa lông hồng. Giúp đồng bộ dễ dàng trên Cloud IDE hoặc máy tính phụ mà không lo tràn băng thông.

> LIVA không ngừng được phát triển trên kiến trúc tối tân và sự giám sát thực tiễn của các Admin/Software Engineers chuyên sâu! Cảm ơn bạn đã sử dụng.
