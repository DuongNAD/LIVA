# 04. Chu Kỳ Tự Tiến Hoá (Evolution Singularity Pipeline)

**Phiên bản: v26 Enterprise-Ready Cognitive OS**

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
