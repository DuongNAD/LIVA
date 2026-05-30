# 06. Tối Ưu UX & Phần Cứng (Ambient Cognitive OS)

**Phiên bản: v29 Enterprise-Ready Cognitive OS**

Mục tiêu cốt lõi của LIVA từ phiên bản 24 đến 29 không chỉ là thông minh mà phải **vô hình** (Ambient) và **cực kỳ tiết kiệm tài nguyên** khi chạy nền trên Desktop cá nhân.

Kiến trúc tối ưu UX và phần cứng của LIVA xoay quanh 5 trụ cột (5 Pillars):

## Pillar 1: Preemptive VRAM Yielding (VRAMGuard)
- Vấn đề: `llama-server` (Mô hình Local) khi hoạt động liên tục sẽ giam lỏng (lock) toàn bộ 8GB-24GB VRAM của card đồ hoạ. Người dùng không thể chơi Game AAA hoặc làm việc đồ hoạ 3D.
- **Giải pháp**: Lớp dịch vụ `AppWatcherService` giám sát danh sách các ứng dụng nặng (Game/Render) thông qua tiến trình OS. Khi phát hiện các ứng dụng này khởi chạy:
  - Hàm `CoreKernel.yieldVRAM()` kích hoạt, ngắt cứng tiến trình mô hình cục bộ. VRAM lập tức xả 100%.
  - LIVA tự động cấu hình lại `ModelOrchestrator`, đẩy toàn bộ giao tiếp AI tương lai lên Cloud API (Gemini/Groq).
  - Khi ứng dụng nặng kết thúc, `CoreKernel.reclaimVRAM()` tái kích hoạt và warm-up mô hình Local, đảm bảo trải nghiệm AI tiếp diễn bình thường.

## Pillar 2: Semantic Action Cache L0.5
- Vấn đề: Để bật/tắt một cái đèn thông minh hoặc hỏi ngày giờ, việc đi qua AgentLoop -> LLM Prompting -> XML Regex parsing là quá chậm và dư thừa.
- **Giải pháp**: `SemanticRouter` duy trì một bộ nhớ L0.5 riêng trong SQLite cache.
  - Các lệnh thường xuyên lặp lại được biến đổi thành Vector Embedding.
  - Khi có truy vấn mới, nếu khoảng cách Cosine Similarity > 0.95 với lịch sử lệnh: Router trực tiếp gọi thẳng hàm `SkillRegistry.execute()` trong chưa tới 5ms. Hoàn toàn Bypass quá trình phân tích bởi LLM. Xoá bỏ hoàn toàn gánh nặng VRAM.

## Pillar 3: On-Demand Zero-Trust Vision
- Vấn đề: Việc quay phim hay stream màn hình 24/7 để phân tích ngữ cảnh (Screen Awareness) tốn lượng lớn VRAM, CPU và đặc biệt vi phạm quyền riêng tư cực đoan.
- **Giải pháp On-Demand**: 
  - Tính năng AI Nhãn quan chỉ kích hoạt khi người dùng nói/gõ các từ khóa nhạy ngữ cảnh (Deictic words) như "cái này", "phần text này", "ở hình trên".
  - Khi đó, lệnh được chuyển sang Rust Tauri Host để chụp duy nhất 1 khung hình màn hình ở định dạng WebP cực nhẹ.
  - Dữ liệu đi qua một lớp thuật toán phân tích cục bộ để bôi mờ (Redact) các thông tin nhạy cảm (như Input mật khẩu, Thẻ tín dụng) trước khi gửi khung hình đã mã hoá qua mạng tới Cloud Vision API.

## Pillar 4: Wake-Word Edge Offloading (LivaWakeWorker)
- Vấn đề: Mở Microphone 24/7 gửi Audio lên Server liên tục sẽ gây tràn RAM, nghẽn đường truyền và tốn CPU xử lý VAD (Voice Activity Detection).
- **Giải pháp WebAssembly Edge**:
  - Đóng gói mô hình học sâu `hey_liva.onnx` (<5KB) vào trình duyệt Vue 3 thông qua chuẩn `onnxruntime-web` WASM.
  - Vòng lặp thu nhận âm thanh qua Micro được thực thi thẳng trên trình duyệt người dùng với mức tiêu thụ CPU 0-1%. 
  - Hệ thống Backend hoàn toàn KHÔNG NHẬN ĐƯỢC CHÚT DỮ LIỆU AUDIO NÀO trừ khi tiến trình Edge WASM Frontend nhận diện thành công chữ "Hey Liva" và mở khoá WebSocket. 
  - Phương pháp này loại trừ thư viện bên thứ 3 có phí (Picovoice) và đảm bảo chuẩn Full-Duplex.

## Pillar 5: Sequential Hot-Swap (v29)
- Vấn đề: Để đạt được khả năng reasoning sâu (Deep Reasoning), hệ thống cần một model lớn (Expert 26B). Tuy nhiên, tải cùng lúc Router 4B và Expert 26B lên một card đồ hoạ 12GB VRAM sẽ gây lỗi OOM (Out Of Memory).
- **Giải pháp Hot-Swap Đơn Kênh**:
  - `ModelOrchestrator` đảm bảo tại một thời điểm chỉ có ĐÚNG MỘT mô hình (Router hoặc Expert) được tải lên VRAM.
  - Tận dụng `mmap` (Memory-Mapped Files) của GGUF kết hợp ổ cứng SSD NVMe để nạp mô hình vào VRAM chỉ trong 5-15 giây.
  - Khi luồng giao tiếp yêu cầu logic phức tạp, Router tự động `unload()`, gọi Expert load lên giải quyết.
  - **Expert Cooldown TTL**: Thay vì trả lại Router ngay lập tức gây VRAM Thrashing, Expert được giữ lại 120-180 giây (`EXPERT_COOLDOWN_MS`) để xử lý các câu hỏi follow-up. Khi hết TTL, tự động swap lại Router.
