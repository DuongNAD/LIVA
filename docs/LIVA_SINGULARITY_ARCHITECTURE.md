# Kỷ Nguyên Tiến Hóa Vĩ Mô: LIVA Autonomous Singularity V5
> Hệ thống Tự Tái Tạo, Tự Nâng Cấp và Tự Vá Lỗi Lõi (Khái niệm: Macro-Evolution) của J.A.R.V.I.S 3.0

Tài liệu này đặc tả chi tiết luồng kiến trúc, các mảnh ghép kĩ thuật và quy trình tuần tự đằng sau Kỹ năng `liva_ai_scientist` (Ghost Coder). Đây là khả năng cho phép Agent tự đào sâu vào bản thân, tự sửa code của chính nó trong một Không Gian Hộp Cát (Sandbox), chạy kiểm thử an toàn, và cập nhật hệ thống mà không cần người dùng nhúng tay vào.

---

## 1. CÁC THÀNH PHẦN KIẾN TRÚC (CORE COMPONENTS)

### 1.1 Vòng Lặp Bất Tử (Infinity Daemon) - `auto_singularity.ts`
Đây là một background watcher luôn lắng nghe trạng thái của Hệ thống. Khi nhận được lệnh "Tự Nâng Cấp", script này sẽ điều hướng toàn bộ tài nguyên, tạo ra các chu kỳ tiến hóa vô tận (Evolution Cycles) và ghi log tiến trình toàn hệ thống.

### 1.2 Động Cơ Luân Phiên VRAM (VRAM Hot-Swap Mechanism)
Do phần cứng bị giới hạn (Consumer GPU), LIVA không thể giữ cả 2 bộ não chạy song song. 
- **Router (4B)**: Thường xuyên túc trực ở Cổng 8000 để chat Zalo hoặc UI tốc độ cao.
- **Expert/Coder (26B)**: Chuyên gia được triệu hồi chỉ khi cần lập trình sâu (Cổng 8001 / 8002).
Khi chu kỳ diễn ra, `DualPortController` (trong `AgentLoop.ts`) đình chỉ Engine Router, "rút" model 4B khỏi VRAM và đẩy Node 26B lên để thực thi quá trình tư duy (Thinking) cho code, và sau đó trả lại thiết lập khi hoàn thành.

### 1.3 Môi trường Sinh tồn (Air-Gapped Docker Sandbox) - `DockerSandbox.ts`
Để tránh việc AI viết code sai làm chết (Crash) cả Core hệ thống hiện tại, mô-đun này tạo ra một vòng kìm tỏa:
- Tạo một `shadow_workspace` bên trong Container Ubuntu.
- Giả lập Internet mở / đóng (Air-gap) theo từng Phase cụ thể (Ví dụ: Cho bật Internet khi cài thư viện NPM bằng lệnh, và Ngắt mạng khi thực thi Tester).
- Cho phép snapshot Git (`git commit`) trước mỗi nhịp tạo tiền đề quay xe (Rollback).

### 1.4 Hạt Nhân Xử Lý (The Ghost Coder) - `AIScientist.ts`
Nơi chắp nối giữa Phép nhúng AI Prompt, Lệnh System, Thuật toán Parsing JSON. Nơi diễn ra thuật toán bóc tách Context và bảo vệ tọa độ Code (Anti-Hallucination Guard).

---

## 2. QUY TRÌNH LUỒNG KHÉP KÍN (EXECUTION FLOW)

Luồng tiến hóa được chia làm **6 Pha Sinh Tồn (Survival Phases)** chạy theo cấu trúc State Machine ngầm:

### Pha 1: Kích hoạt & Phân Lập (Hot-Swap & Isolation)
1. Tiến trình ngầm ra lệnh đình chỉ dịch vụ Messaging. Các tin nhắn của người dùng bị giam vào *Zalo Pending Queue*.
2. VRAM được hút cạn (Garbage Collection của Python). Cổng 8002 của LLM Expert 26B thức tỉnh hoàn tất.
3. Kỹ năng `liva_ai_scientist` được kích hoạt. Không Gian Docker được mồi (Spin-up). Mã nguồn LIVA được nhân bản (clone) chuyển vào `/app/shadow_workspace`.

### Pha 2: Thẩm tra Mục Tiêu (Objective Analysis & Ideation)
1. Hệ thống truyền tham số cấu trúc tổng thể và File Code gốc nguyên vẹn (Kèm 1-indexed line numbers) cho mô hình thông qua API.
2. AI phân tích file cấu trúc, tìm ra nút thắt. Nêu ra `pros` (Điểm mạnh), `cons` (Rủi ro rò rỉ bộ nhớ), `feasibilityScore` (Điểm khả thi).

### Pha 3: Tái Cấu Trúc (AST Patch Generation)
1. Mô hình Coder 26B ói ra định dạng JSON tĩnh chữa mã dưới dạng Array Patch:
   - Các Block Patch được quy định ép khuôn gồm `start_line`, `end_line` và `new_code`.
   - **Luật Anti-Structural Hallucination:** Nghiêm cấm mô hình chia tách một hàm ra thành 50 patch, ở các dòng liền nhau. Nếu mô hình làm thế, NodeJS Parser sẽ chặn đứng tức thì để tránh lệch vị trí tọa độ gốc.

### Pha 4: Khâu Vá và Kiểm Định (Merge & Verification)
1. Các Json Patches được parse, thuật toán NodeJS phân tách mảnh chuỗi (String Splice) và tiêm `new_code` vào trong các khoảng Array Lines. 
2. File được đẩy ngược đè lên trong Container Hộp Cát.
3. Chạy `npm install` (Nếu AI yêu cầu thêm gói bên ngoài mới).
4. Run Global Type Checker qua hệ sinh thái bóng (`npx tsc --noEmit`).
5. Kích Hoạt Automation Unit Test (Chạy Vitest nếu có).

### Pha 5: Phản hồi Sinh tồn (Feedback Loop)
Nếu Phase 4 **THẤT BẠI** (Có lỗi TypeScript do AI code hỏng hoặc gọi hàm không tồn tại):
1. Parser thu thập Error `stderr` tạc vào Stack Trace.
2. Trích xuất **Trọng Tâm Cú Pháp (Holistic AST Context)**: Tính toán số dòng báo lỗi (Ví dụ: báo lỗi dòng 357), bóc tách lấy 15 dòng bên trên và 15 dòng bên dưới của File Hiện Tại, để cho AI *thấy tận mắt* code của nó sai chỗ nào.
3. **ZMAS Guard (Symbol Hunt)**: Nếu lỗi là `Cannot find name 'X'`, NodeJs bí mật chạy `grep` toàn thư mục dự án xem hàm đó có nấp ở đâu không. Nếu thấy, mớm đường dẫn Import. Nếu không thấy, Tuyên án chửi AI "Mày đang ảo giác bịa ra hàm!".
4. AI bị đánh bật quay lại **Pha 3** tối đa 3 Vòng, phải viết lại khối JSON kèm cả Patch cũ lẫn Fix mới.

### Pha 6: Hoàn Tất, Hợp Nhất & Ghi Nhớ (Checkpoint & Lance Distillation)
1. Nếu Code pass toàn bộ rào cản Hộp Cát: Docker bắn tín hiệu SUCCESS.
2. Cập nhật mã sinh tồn lên Lõi Thực tại (Pull ra ngoài Local File System của người dùng theo đường dẫn gốc).
3. Đóng đinh nhật ký Tiến Hóa vào **Bộ Chưng cất LanceDB** (Axiom Distillation) để khắc phục bệnh Mù Trí Nhớ Tiến hóa (Amnesia). Qua đó các vòng nạp Coder đằng sau sẽ biết hệ thống từng áp dụng Pattern này.
4. Triệt tiêu Container. Tắt AI 26B, Mở lại Cổng AI Rounter 4B (Port 8000). Đồng loạt xả các tin nhắn Zalo kẹt trong Queue lên Coder. Trả Hệ Thống về trạng thái hoạt động bình thường!

---

## 3. CÁC HÀNG RÀO AN NINH CỐT LÕI (DEFENSE MECHANISMS)

Để đảm bảo hệ thống không bị AI lập trình phá hoại bằng những cú "ngáo" (hallucination), 4 hệ thống Guardrails đã được chọc thẳng vào Core:

- **1. Circuit Breaker (Anti Doom-loop)**
  Các task bị khống chế tối đa `MAX_ITERATIONS = 5` và Runtime là `300,000ms`. Quá giờ, Break-Chain cắt lìa Queue để ngăn nhồi CPU và Memory RAM rác. Tự động `process.exit()` khỏi Hộp Cát.
- **2. Structural Hallucination Guard (Mới Áp Dụng)**
  Phát hiện tỷ lệ chia nhỏ Patch. Nếu tổng số lượng Patch > 4 mà có quá 50% là các điểm nối 1 dòng, Coder sẽ ném ngoại lệ huỷ áp dụng thay thế để không dội lỗi đè lấn lên các function sau khối lập trình.
- **3. ZMAS Guardrail (Code Semantic Analyzer)**
  Kiểm soát tính hợp lệ của cấu trúc lệnh `shell_commands`. AI chỉ được phép chạy NPM ở mode Isolation. Internet bị Block bằng `sandbox.disconnectNetwork()` trong bài phân tích TSC Check. Đóng mạch mạng (Zero Trust).
- **4. Strict JSON Re-healing**
  Cố gắng tự hàn gắn JSON bị gãy khúc (nút mạng sập / timeout / stream dở dang) bằng thư viện `jsonrepair`. Thọc tay vào Regex ````json {...} ```` để cướp đúng mảng đối tượng bất chấp mô hình có nói năng lan man bên ngoài hay không.
