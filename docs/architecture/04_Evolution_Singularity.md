# 04. Tiến hóa Tự động & Dị điểm (Evolution & Singularity Pipeline)

> Phiên bản: v20 (2026-05-11) — LIVA-UHM v2

## 1. Mục tiêu (Objective)

Hệ thống tiến hóa (Singularity Pipeline) là khả năng đặc biệt của LIVA, cho phép AI tự động bảo trì, viết lại, tối ưu hóa hoặc sửa lỗi chính bộ mã nguồn của nó (Self-Reflection & Self-Evolution) mà không làm sập hệ thống.

---

## 2. Các Thành Phần Chính

### 2.1. Evolution Pipeline (Đạo diễn vòng lặp tiến hóa)
`EvolutionPipeline` quản lý toàn bộ quy trình dưới dạng Cây đồ thị có hướng (DAG). Quá trình bắt đầu từ khi phát hiện lỗi hệ thống, hoặc được trigger định kỳ để tối ưu mã. 
Nó điều phối `ASTMutator` (thay đổi mã) và `RollbackManager` (khôi phục an toàn).

### 2.2. AST Code Surgeon (Phẫu thuật AST)
Để tránh các lỗi do Regex Find & Replace (ví dụ sinh ra các dấu ngoặc nhọn `{}` thừa hoặc xóa nhầm code), LIVA sử dụng:
- **`ts-morph`**: Trình phân tích Cú pháp trừu tượng (Abstract Syntax Tree - AST) của TypeScript.
- **Path Jail**: Hệ thống chặn AI không được phép chỉnh sửa các file lõi cấu hình, chỉ được sửa trong phạm vi cho phép (thường là logic bên trong một class hay hàm cụ thể).
- **Atomic Write**: Mọi thao tác sửa file đều ghi ra file `.tmp` trước khi đổi tên đè lên file gốc, đảm bảo không bị corrupt nếu mất điện hoặc sập tiến trình.

### 2.3. MicroVMDaemon (Hộp Cát Kiểm thử)
Trước đây LIVA dùng Docker/WSL2 để kiểm thử code AI tự sinh ra, nhưng nó chiếm quá nhiều RAM (2-4GB). Trong kiến trúc P4, LIVA chuyển sang:
- **`isolated-vm` / WASI**: Một máy ảo siêu nhẹ trong process Node.js.
- **Boot time**: < 1ms.
- **RAM**: Dưới 15MB.
- **Mục đích**: Chạy thử hàm AI vừa tạo xem có crash không, trước khi commit thay thế.

### 2.4. Rollback Manager (Khôi phục mã nguồn)
- **Cơ chế BlueGreenRouter V8**: Sử dụng Physical Snapshot. Trước khi AI thực hiện chỉnh sửa, toàn bộ module đó được sao lưu vật lý (`.src.rollback.bak` sử dụng `fs.promises.cp`).
- Nếu quá trình compile (`tsc`) thất bại, LIVA sẽ lập tức khôi phục (rollback) thay vì dùng `git checkout` (vì `git checkout -- src/` sẽ tàn phá tất cả các file working tree đang không liên quan).

### 2.5. GitNexus Indexer (Code Graph)
Chạy ngầm (Daemon). Phân tích cấu trúc hàm, biến, class trong dự án để AI hiểu được nếu đổi tên một biến ở File A thì File B sẽ bị ảnh hưởng thế nào (Blast Radius). Hỗ trợ đắc lực cho RAG về mã nguồn.
