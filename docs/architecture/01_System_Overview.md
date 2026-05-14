# 01. Tổng quan Hệ thống (System Overview)

> Phiên bản: v20 (2026-05-11) — LIVA-UHM v2

## 1. Kiến trúc Cốt lõi (Core Architecture)

Dự án LIVA là một trợ lý ảo đa đặc vụ (multi-agent AI desktop assistant) vận hành theo kiến trúc **Hybrid Intelligence** (local AI + cloud fallback). Kiến trúc tổng thể được chia thành 4 phân hệ chính:

1. **liva-ui (Frontend)**: 
   - Xây dựng bằng Tauri v2 (Rust) và Vue 3.
   - Ứng dụng OS WebView Native siêu nhẹ, giới hạn RAM dưới 50MB.
   - Giao tiếp với Gateway qua WebSocket.
   
2. **openclaw-gateway (Bộ não Đặc vụ - Agent Brain)**:
   - Xây dựng bằng Node.js/TypeScript.
   - Quản lý Máy trạng thái hữu hạn (FSM) qua `AgentLoop`.
   - Kết nối với cơ sở dữ liệu SQLite duy nhất (Consolidated Brain).
   - Đóng vai trò Remote Control Hub (Telegram, CDP Bridge).

3. **llama-server (LLM Engine)**:
   - Viết bằng C++ Native (dựa trên llama.cpp).
   - Kiến trúc **Single Expert Model**: Dành trọn 100% VRAM cho một model duy nhất, loại bỏ hoàn toàn việc nạp song song (Dual-Port) để tránh tràn VRAM.
   - Cung cấp API tương thích OpenAI (`/v1/embeddings`, `/v1/chat/completions`) hỗ trợ CUDA/Vulkan GPU offload.

4. **TTS System (Hệ thống Giọng nói)**:
   - Xây dựng bằng Python.
   - Kiến trúc Hybrid: Sử dụng Edge-TTS (ưu tiên hiệu năng cao qua mạng) hoặc Kokoro-JS (dự phòng, 100% offline).

---

## 2. Chuỗi Khởi động (Startup Sequence)

Quy trình khởi động khi chạy `npm run desktop` hoặc qua Tauri Sidecar:
1. `openclaw-gateway` chạy lệnh `tsx src/Gateway.ts` để kích hoạt `AutoGPUSetup` (nhận diện phần cứng).
2. `ModelOrchestrator` trong gateway tiến hành spawn tiến trình `llama-server.exe` trên port 8000.
3. Khởi chạy `voice_engine.py` (nếu dùng hệ thống giọng nói).
4. `liva-ui` khởi động Desktop app và kết nối WebSocket về port tự động cấp phát của gateway.

---

## 3. Các Luồng Dữ Liệu Chính (Data Flows)

### 3.1. Luồng Tin nhắn Người dùng
\`\`\`text
User Input (Tauri WebSocket)
  → UIController.ts (Gateway)
  → AgentLoop.ts (FSM: IDLE → THINKING)
  → SemanticRouter.route() (Phân loại ý định <100ms bằng vector cosine)
  → PromptBuilder.ts (Lắp ráp prompt dựa trên intent, tiêm context từ Memory)
  → ModelOrchestrator.ts (Gọi Single Expert port 8000)
  → LLM sinh phản hồi và/hoặc các lệnh gọi công cụ (Tool Calls)
  → SkillRegistry.ts (Thực thi công cụ)
  → ZMAS_Guard.ts (Lọc và kiểm duyệt kết quả)
  → ReflectionDaemon.queueTurn() (Trích xuất Φ/Ψ nền, emit 'NEW_TURN' qua MemoryEventBus)
  → AgentLoop.ts (FSM: REFLECTING → IDLE)
  → UIController.ts (Phát WebSocket về Tauri UI)
\`\`\`

### 3.2. Quản lý Tài nguyên (Garbage Collection)
Gateway có một quá trình đóng băng tiến trình (`CoreKernel.shutdown()`) cực kỳ nghiêm ngặt nhằm tránh việc rò rỉ tài nguyên, đặc biệt là lỗi **VRAM Zombie**:
1. Bước 1: `killLlamaServer()` ngay lập tức để giải phóng 100% VRAM.
2. Bước 2: Chấm dứt các `worker_threads`.
3. Bước 3: `memory.dispose()` — theo thứ tự nghiêm ngặt:
   - `flushFactTouches()` (RAM buffer → SQLite)
   - `reflectionDaemon.flushPending()` + `dispose()` (xả Φ/Ψ pending)
   - `consolidationCron.dispose()` (unsubscribe EventBus + clear timers)
   - `quantStore.dispose()` (GC + tensor cache)
   - `structuredMemory.close()` (SQLite WAL flush)
4. Giải phóng tài nguyên phụ: SensoryManager, EmbeddingService, EmailManager, VoiceSpeaker.
