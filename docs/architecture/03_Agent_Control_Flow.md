# 03. Luồng Kiểm Soát Tác Vụ Đặc Vụ (Agent Control Flow)

**Phiên bản: v26 Enterprise-Ready Cognitive OS**

Luồng kiểm soát tác vụ của LIVA được thiết kế xoay quanh cốt lõi là `AgentLoop`, hoạt động như một cỗ máy trạng thái (State Machine) quản lý vòng đời của trí tuệ nhân tạo. Đặc biệt ở phiên bản v26, kiến trúc đa đặc vụ (Multi-agent) được thắt chặt bằng giao thức LACP cho phép phối hợp an toàn tuyệt đối.

## 1. Cỗ Máy Trạng Thái AgentLoop (State Machine)

`AgentLoop` vận hành qua 4 giai đoạn vòng lặp chính:
1. **IDLE**: Chờ đợi tác vụ từ `CoreKernel` (gõ phím, giọng nói, Telegram...).
2. **THINKING**: Tiền xử lý, nạp Memory (L0/L1/L2), xây dựng Prompt bằng `PromptBuilder`, phát âm thanh đệm (Latency Masking).
3. **ACTING**: Stream dữ liệu từ LLM (Local hoặc Cloud). Nếu có gọi Tools (phân tích qua `ToolCallExtractor` XML format), chuyển sang thực thi Skill.
4. **REFLECTING**: Phân tích kết quả sau khi gọi Tool. Tự động sửa lỗi thông qua `ZMASGuard.autoRemediation()`. Đóng gói kết quả lưu vào Memory.

## 2. Giao Thức LACP (LLM Agent Communication Protocol)

Trong tương lai đa đặc vụ (Ví dụ: Đặc vụ Lên lịch làm việc với Đặc vụ Ngân hàng), hệ thống sử dụng **LACPProtocol**:
- **Bản chất**: LACP cung cấp tầng Giao dịch (Transactional Layer) ứng dụng cơ chế **2-Phase Commit (2PC)**.
- **Bảo Mật Kép**: Mọi giao tiếp giữa các Đặc vụ đều được bọc trong vỏ bọc `LACPTxEnvelope` và ký điện tử thuật toán JWS (JSON Web Signature) kết hợp cùng AES-256-GCM HMAC từ `EncryptionEngine`.
- **Chống Zombie Transaction**: Sử dụng `lru-cache` có TTL chặt chẽ thay vì `Map` thông thường để chặn đứng việc rò rỉ bộ nhớ từ các Giao dịch chưa bao giờ hoàn thành (Zombie Transactions).

## 3. Skill Circuit Breaker & Whitelist (Rào chắn Kỹ năng)

LIVA hỗ trợ hơn 78+ Skills. Việc phòng chống rủi ro lỗi domino (Cascading Failure) là bắt buộc.
- **SkillCircuitBreaker**: Là một cầu dao chủ động. Khi một Skill (ví dụ: cào dữ liệu Shopee) gọi API thất bại quá 3 lần liên tiếp, Circuit Breaker chuyển sang trạng thái OPEN. Ngay lập tức, `PromptBuilder` sẽ loại bỏ mô tả của Skill đó khỏi System Prompt. LLM sẽ "mù tạm thời" với Skill đó, tránh việc Agent liên tục Hallucination gọi lại một hàm đã chết.
- **SkillWhitelist**: Cơ chế phân quyền cứng. Ngay cả khi Agent muốn gọi lệnh `ExecuteCommand`, nó phải qua cửa kiểm tra Token Authority từ `CoreKernel`.

## 4. Quản Lý GPU Bằng Preemptive Vram Mutex

- Hệ thống thực thi tác vụ AI không dùng các khóa FIFO (First-In-First-Out) cổ điển mà dùng **Preemptive Mutex** (`AbortController`).
- Tại sao? Các tác vụ nền (Background Consolidation) không được phép giành quyền của người dùng. Nếu người dùng cất giọng "Hey LIVA", mọi tác vụ chạy nền trên GPU sẽ ngay lập tức bị huỷ (Abort) để trả lại 100% VRAM phục vụ tốc độ trả lời (Voice Full-Duplex 0ms Latency).

## 5. Two-Stage Barge-in (Gián Đoạn Hội Thoại Đa Tầng)

Trải nghiệm hội thoại tự nhiên yêu cầu LIVA phải biết lúc nào cần ngừng nói (Barge-in):
- **Giai đoạn 1 (Audio Ducking)**: Module `VADWorkerBridge` (chạy Silero ONNX ở luồng phụ độc lập) phát hiện `speech_start`. Hệ thống KHÔNG ngắt AgentLoop ngay lập tức (tránh trường hợp tiếng ho, tằng hắng gây False Positive) mà chỉ hạ âm lượng TTS xuống 20%.
- **Giai đoạn 2 (Semantic Classification)**: STT từ Whisper trả về văn bản. `BackchannelDetector` phân loại văn bản đó là Backchannel (Từ đệm như "ừm", "ok") hay là câu nói thật. 
  - Nếu là Backchannel: Trả volume về 100%, LLM tiếp tục trả lời bình thường.
  - Nếu là lời nói chặn thật sự: Kích hoạt ngắt `agentLoop.bargeIn()`.

## 6. Che Giấu Độ Trễ (Latency Masking)

LLM có thể tốn từ 1-3 giây để xuất ra token đầu tiên (TTFT) khi thực hiện tác vụ nặng (Suy luận sâu, Gọi hệ thống).
- `AgentLoop` tự động bắt luồng tín hiệu và phát ra một đoạn âm thanh đệm (Filler Audio) ngắn ngẫu nhiên bằng tiếng Việt (Ví dụ: "Dạ vâng...", "Sếp đợi em một tí...").
- Kỹ xảo này che giấu toàn bộ quá trình chờ đợi API/LLM, tạo cảm giác LIVA phản hồi ngay lập tức sau 0ms.
