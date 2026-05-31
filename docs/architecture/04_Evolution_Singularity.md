# 04. Chu Kỳ Tự Tiến Hoá (Evolution Singularity Pipeline)

**Phiên bản: v29 Enterprise-Ready Cognitive OS**

Singularity Pipeline là khả năng độc đáo nhất của hệ thống LIVA: Cho phép AI có khả năng lập trình, viết mã, vá lỗi, tự sinh ra mã nguồn cho chính nó để tiến hoá thành phiên bản ưu việt hơn. Ở v26, toàn bộ quá trình này được cấu trúc hóa dưới dạng DAG (Directed Acyclic Graph) để tránh vòng lặp chết (Fork-Bomb).

## 1. Mạch Điều Phối Lõi (EvolutionPipeline)

Thay vì một vòng lặp `while(true)` vô hạn có rủi ro ăn sạch tài nguyên máy chủ, `EvolutionPipeline` sử dụng mô hình DAG:
1. **Lập Kế Hoạch (Planning)**: AI phân tích và đưa ra kế hoạch sửa mã dựa trên các Axiom (Chân lý quá khứ). Áp dụng Deduplication để không lặp lại lỗi sai.
2. **Coding**: Gọi `AIScientist` tạo mã nguồn.
3. **Phẫu Thuật (AST Surgery)**: Tích hợp mã thông qua Abstract Syntax Tree thay vì chuỗi Regex.
4. **Cô Lập (Sandboxing)**: Build và Test trong môi trường an toàn.
5. **Rollback / Commit**: Chấp nhận mã mới hoặc đảo ngược an toàn tuyệt đối.

## 2. Bác Sĩ Cú Pháp (ASTCodeSurgeon)

Trong các hệ thống Agentic cũ, AI thường dùng chuỗi Text Regex hoặc `sed` để Replace mã nguồn. Hậu quả là hỏng file do không đúng ngữ cảnh (Context Mismatch).

- LIVA giải quyết vấn đề bằng **ASTCodeSurgeon**. Sử dụng bộ công cụ `ts-morph`, nó chuyển đổi file Typescript thành cấu trúc Cây cú pháp trừu tượng (Abstract Syntax Tree).
- Phẫu thuật AST đảm bảo tính chính xác 100%. Các class, methods, variables được thêm/sửa/xoá an toàn tuyệt đối. Mọi thao tác đều thực thi tại một Background Worker Thread để không khóa cứng Event Loop chính.

## 3. Lò Thử Nghiệm Tốc Độ Ánh Sáng (MicroVMDaemon)

Một cỗ máy tự tiến hoá cần biên dịch và test hàng nghìn dòng code mỗi giờ. Các giải pháp truyền thống như Docker hay WSL2 mất 2-4GB RAM (vmmem) và vài giây để khởi động.

- **MicroVMDaemon**: LIVA thay thế toàn bộ lớp ảo hoá cồng kềnh bằng `isolated-vm` (chạy script V8) hoặc `WASI` (WebAssembly System Interface).
- Boot time mất chưa tới `<1ms` và tốn `<15MB` RAM. Cho phép AI chạy thử nghiệm các đoạn mã rủi ro mà hệ điều hành bên ngoài hoàn toàn miễn nhiễm. 

## 4. Bàn Tay Thép Phục Hồi (RollbackManager Physical Snapshots)

Phục hồi hệ thống khi AI tự viết code hỏng là một quá trình cực kỳ nhạy cảm.

- **Kiến trúc Cũ (Nguy hiểm)**: Dùng `git checkout -- src/` hoặc `git clean -fd src/`. Hệ luỵ là nó sẽ phá hủy vĩnh viễn các file cá nhân (Uncommitted Work) mà lập trình viên con người đang làm dở dang.
- **Kiến trúc v26 (Physical Snapshots)**: `RollbackManager` sử dụng cơ chế Snapshot Vật lý. Trước khi AI đụng vào code, hệ thống dùng `fs.promises.cp` tạo một bản copy tĩnh dạng `.src.rollback.bak`. Nếu quá trình tự build thất bại, hệ thống chỉ đè bản `.bak` này lại. Mã nguồn dang dở của con người luôn được bảo vệ nguyên vẹn.

## 5. Mạng Lưới Nhận Thức Kép (GitNexus Dual System)

Để LIVA hiểu kiến trúc dự án 6000+ dòng code của chính nó, hệ thống ứng dụng GitNexus được thiết kế chống tràn VRAM:
- `GitNexusIndexer`: Tiến trình chạy ẩn bên dưới (Daemon), sử dụng luồng phụ để liên tục phân tích và lập chỉ mục Cây tương tác quan hệ giữa các File mã nguồn.
- `GitNexusQuery`: Khi `AIScientist` cần hiểu "Cách hoạt động của CoreKernel", nó không cần đọc hết 50 file mã nguồn, chỉ cần một RAG Truy vấn Semantic là lấy chính xác 3 file chứa Dependency liên quan. Tiết kiệm 95% LLM Tokens.
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
