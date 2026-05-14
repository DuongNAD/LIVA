# Kỷ Nguyên Tiến Hóa Vĩ Mô: LIVA Autonomous Singularity V5
> Hệ thống Tự Tái Tạo, Tự Nâng Cấp và Tự Vá Lỗi Lõi (Khái niệm: Macro-Evolution) của J.A.R.V.I.S 3.0

Tài liệu này đặc tả chi tiết luồng kiến trúc, các mảnh ghép kĩ thuật và quy trình tuần tự đằng sau Kỹ năng `liva_ai_scientist` (Ghost Coder). Đây là khả năng cho phép Agent tự đào sâu vào bản thân, tự sửa code của chính nó trong một Không Gian Hộp Cát (Sandbox), chạy kiểm thử an toàn, và cập nhật hệ thống mà không cần người dùng nhúng tay vào.

---

## 1. CÁC THÀNH PHẦN KIẾN TRÚC (CORE COMPONENTS)

### 1.1 Vòng Lặp Bất Tử (Infinity Daemon) - `auto_singularity.ts`
Đây là một background watcher luôn lắng nghe trạng thái của Hệ thống. Khi nhận được lệnh "Tự Nâng Cấp", script này sẽ điều hướng toàn bộ tài nguyên, tạo ra các chu kỳ tiến hóa vô tận (Evolution Cycles) và ghi log tiến trình toàn hệ thống.

### 1.2 Động Cơ Xử Lý Trung Tâm (Adaptive Engine Selection)
Kiến trúc LIVA hiện tại vận hành hoàn toàn trên một mô hình duy nhất (Single Expert Model) nhằm tối đa hóa 100% VRAM (Loại bỏ kiến trúc Dual-Port cũ gây OOM). Quá trình suy luận nặng của lập trình viên ma (Coder) được xử lý thông qua Engine mạnh nhất cấu hình hiện tại (Local GGUF qua `llama-server` hoặc Cloud API).

### 1.3 Môi trường Sinh tồn Không Gian Vi Máy Ảo - `MicroVMDaemon.ts`
Để tránh việc AI viết code sai làm chết (Crash) cả Core hệ thống hiện tại, mô-đun này tạo ra một vòng kìm tỏa cực kỳ an toàn (dựa trên WASI/isolated-vm):
- Khởi động cực nhanh <1ms và tốn <15MB RAM, thay thế hoàn toàn công nghệ Docker/WSL2 cũ kĩ.
- Tạo một `shadow_workspace` cô lập với File System.
- Ngắt mạng (Air-gap) khi chạy bài kiểm định để ngăn chặn việc tải mã độc.
- Cho phép snapshot vật lý (`.src.rollback.bak`) để làm tiền đề cho Quản lý phục hồi (`RollbackManager`).

### 1.4 Hạt Nhân Xử Lý Phẫu Thuật AST - `ASTCodeSurgeon.ts`
Thao tác thay đổi source code được thực hiện độc quyền qua công cụ phẫu thuật AST (`ts-morph`) thay vì dùng RegEx. Áp dụng quy tắc Atomic Write để ghi file an toàn, ngăn file bị lỗi (corrupt) khi đang lưu.

---

## 2. QUY TRÌNH LUỒNG KHÉP KÍN (EXECUTION FLOW)

Luồng tiến hóa được chia làm **6 Pha Sinh Tồn (Survival Phases)** chạy theo cấu trúc State Machine ngầm:

### Pha 1: Kích hoạt & Phân Lập (Isolation)
1. Tiến trình ngầm ra lệnh đình chỉ dịch vụ Messaging. Các tin nhắn của người dùng bị giam vào *Zalo Pending Queue*.
2. Kỹ năng `liva_ai_scientist` được kích hoạt. Không Gian Vi Máy Ảo (`MicroVMDaemon`) được Spin-up. Mã nguồn LIVA được nhân bản chuyển vào `shadow_workspace`.
3. `RollbackManager` lưu bản snapshot `.src.rollback.bak`.

### Pha 2: Thẩm tra Mục Tiêu (Objective Analysis & Ideation)
1. Hệ thống truyền tham số cấu trúc tổng thể và File Code gốc nguyên vẹn cho mô hình thông qua API.
2. Coder phân tích file cấu trúc, tìm ra nút thắt. Nêu ra `pros` (Điểm mạnh), `cons` (Rủi ro rò rỉ bộ nhớ), `feasibilityScore` (Điểm khả thi).

### Pha 3: Tái Cấu Trúc (AST Patch Generation)
1. Mô hình Coder ói ra định dạng JSON tĩnh chữa mã dưới dạng các cấu trúc thay đổi hàm/biến.
2. **Luật Anti-Structural Hallucination:** Bất kỳ thao tác RegEx cắt ghép chuỗi nào cũng bị chặn. Hệ thống dựa trên AST Node ID để xác định phạm vi thay thế mã thông qua `ASTCodeSurgeon`.

### Pha 4: Khâu Vá và Kiểm Định (Merge & Verification)
1. Phẫu thuật AST tiêm `new_code` vào trong Tree Nodes. 
2. File được đẩy ngược đè lên trong `MicroVMDaemon`.
3. Chạy lệnh cập nhật NPM nếu cần.
4. Run Global Type Checker qua hệ sinh thái bóng (`npx tsc --noEmit`).
5. Kích Hoạt Automation Unit Test (Chạy Vitest nếu có).

### Pha 5: Phản hồi Sinh tồn (Feedback Loop)
Nếu Phase 4 **THẤT BẠI** (Có lỗi TypeScript do AI code hỏng hoặc gọi hàm không tồn tại):
1. Parser thu thập Error `stderr` tạc vào Stack Trace.
2. Trích xuất **Trọng Tâm Cú Pháp (Holistic AST Context)**: Bóc tách AST Tree Context xung quanh dòng bị lỗi để cho AI thấy tận mắt mã nguồn.
3. **GitNexus Guard**: NodeJs bí mật sử dụng MCP Tool `GitNexus` truy vấn Semantic RAG để tìm xem hàm đó có nấp ở đâu không.
4. AI bị đánh bật quay lại **Pha 3** tối đa 3 Vòng.

### Pha 6: Hoàn Tất, Hợp Nhất & Ghi Nhớ (Checkpoint & Distillation)
1. Nếu Code pass toàn bộ rào cản Hộp Cát: Bắn tín hiệu SUCCESS.
2. Cập nhật mã sinh tồn lên Lõi Thực tại.
3. Đóng đinh nhật ký Tiến Hóa vào **StructuredMemory (SQLite-Vec Distillation)** để khắc phục bệnh Mù Trí Nhớ Tiến hóa (Amnesia).
4. Triệt tiêu MicroVMDaemon. Đồng loạt xả các tin nhắn kẹt trong Queue lên Engine. Trả Hệ Thống về trạng thái hoạt động bình thường!

---

## 3. CÁC HÀNG RÀO AN NINH CỐT LÕI (DEFENSE MECHANISMS)

Để đảm bảo hệ thống không bị AI lập trình phá hoại bằng những cú "ngáo" (hallucination), 4 hệ thống Guardrails đã được chọc thẳng vào Core:

- **1. Circuit Breaker (Anti Doom-loop)**
  Các task bị khống chế tối đa `MAX_ITERATIONS = 5` và Runtime là `300,000ms`. Quá giờ, Break-Chain cắt lìa Queue để ngăn nhồi CPU và Memory RAM rác. Tự động `process.exit()` khỏi Hộp Cát.
- **2. Structural Hallucination Guard (Mới Áp Dụng)**
  Quy định bắt buộc dùng `ASTCodeSurgeon`. Không được thay thế mã ngu ngốc theo chuỗi/dòng. Tránh vỡ file `try/catch`.
- **3. MicroVM Air-Gap Guardrail**
  Kiểm soát tính hợp lệ của cấu trúc lệnh. AI chỉ được phép chạy ở mode Isolation (WASI). Internet bị Block trong bài phân tích TSC Check. Khóa truy cập thư mục mẹ, bảo mật khóa AES Zero Trust.
- **4. Strict JSON Re-healing**
  Cố gắng tự hàn gắn JSON bị gãy khúc (nút mạng sập / timeout / stream dở dang) bằng thư viện `jsonrepair`. Thọc tay vào Regex cướp đúng mảng đối tượng bất chấp mô hình có nói năng lan man bên ngoài hay không.
