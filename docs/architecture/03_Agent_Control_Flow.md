# 03. Luồng Kiểm soát Đặc vụ (Agent Control Flow)

> Phiên bản: v20 (2026-05-11) — LIVA-UHM v2

## 1. Vòng lặp Trung tâm (AgentLoop)

`AgentLoop` là hạt nhân điều phối Máy trạng thái hữu hạn (Finite State Machine - FSM) của toàn bộ hệ thống openclaw-gateway. 
Các trạng thái chính bao gồm:
1. **IDLE**: Chờ yêu cầu mới từ người dùng.
2. **THINKING**: Tiền xử lý yêu cầu, gọi SemanticRouter để phân loại intent.
3. **ACTING**: Lắp ráp prompt và gọi LLM (`ModelOrchestrator`), sau đó thực thi các công cụ qua `SkillRegistry`.
4. **REFLECTING**: Xử lý tri thức sau mỗi lượt. `ReflectionDaemon` trích xuất Φ/Ψ và emit `'NEW_TURN'` qua `MemoryEventBus` — `ConsolidationCron` lắng nghe và tích lũy passive signals.

---

## 2. Semantic Router (Bộ định tuyến Ngữ nghĩa)
Thay vì sử dụng các cấu trúc `if/else` cứng nhắc, hệ thống dùng **SemanticRouter**.
- **Chức năng**: Khi người dùng gửi một chuỗi văn bản, `SemanticRouter` sẽ nhanh chóng tính toán Vector Cosine (<100ms thông qua SQLite-vec).
- **Kết quả**: Phân loại request thành các kịch bản khác nhau (chitchat, system_command, factual_recall, deep_reasoning).
- **Hiệu năng**: Các câu lệnh hệ thống hay chào hỏi sẽ được bỏ qua bước nhúng RAG phức tạp (Skip RAG), giúp AI phản hồi cực kỳ nhanh.

---

## 3. Prompt Builder (Lắp ráp Ngữ cảnh)
Đây là module đóng vai trò "người làm bếp" chuẩn bị món ăn cho LLM:
1. Lấy thông tin cá nhân lõi từ `PersonalKnowledge` (L3).
2. Tùy theo Intent mà `SemanticRouter` trả về, kéo thêm thông tin ngữ cảnh từ VectorMemory (L2) hoặc Working Buffer (L0).
3. Đưa danh sách các Tool có thể sử dụng (qua `SkillRegistry`).
4. Nhúng các tri thức chống lỗi từ `HeraCompass` để AI "nhớ" bài học quá khứ (In-context learning).
5. Trả ra chuẩn văn bản (bao gồm schema JSON/XML cần thiết) cho Model Orchestrator.

---

## 4. Model Orchestrator (Điều phối viên Mô hình)
- Là lớp vỏ bao bọc, quản lý giao tiếp với C++ `llama-server`.
- **Cơ chế Single Expert**: Khác với phiên bản cũ tải nhiều Model cùng lúc, P4 Architecture dồn 100% VRAM GPU cho 1 Model duy nhất.
- Hỗ trợ cơ chế Streaming token trả về theo thời gian thực (được đẩy xuống WebSocket UI thông qua `UIController`).
- Đóng vai trò làm cổng nhận dạng Anomaly Detection (ping mỗi 15s để xem LLM server có treo VRAM hay không). Nếu server treo 3 lần, kích hoạt `killLlamaServer()` và tự động nạp lại.

---

## 5. Thực thi Công cụ (SkillRegistry)
Khi LLM trả về một "Tool Call", gateway sẽ:
1. Trích xuất cú pháp XML/JSON một cách an toàn thông qua `JsonExtractor.ts`.
2. Kiểm tra `SkillRegistry` xem có công cụ đó không (hỗ trợ Domain-driven architecture: web, devops, personal, v.v.).
3. Đi qua cổng Human-in-the-loop (`HITLGuard`) nếu hành động đó mang tính phá hủy (vd: Delete File).
4. Thực thi và trả lại kết quả thô về cho LIVA FSM để LLM tiếp tục trả lời user.
