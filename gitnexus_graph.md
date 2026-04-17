# Bộ dữ liệu Kiến trúc GitNexus Hệ thống LIVA (Macro-Evolution)

> **Mục đích**: File này chứa báo cáo tổng quát về Cây Kiến Trúc (Knowledge Graph) của dự án LIVA đã được tự động bóc tách. Cấu trúc này dùng để đẩy lên AI phân tích luồng thực thi, vòng lặp tự sửa lỗi, và quy hoạch thiết kế tương lai.

## 1. Tổng quan Trạng thái Index (GitNexus Status)
- **Repository**: `E:\Project\LIVA`
- **Mã Hash Cuối (Current Commit)**: `89c31e4`
- **Thống Kê Sinh Tồn Bề Mặt (Size Metrics)**:
  - **Số File (Tệp tin)**: 1,138
  - **Nodes (Nút đồ thị - Function/Class/Files...)**: 3,690
  - **Edges (Cạnh đồ thị - Lời gọi hàm phụ thuộc)**: 7,693
  - **Clusters (Cụm tính năng được trích xuất)**: 265 cộng đồng chức năng
  - **Processes (Chuỗi thực thi luồng logic)**: 203 luồng thực thi (Execution Flows)

## 2. Thống kê Hình thể Thuật toán (Node Graph Density)
Báo cáo truy vấn truy xuất từ `npx gitnexus cypher "MATCH (n) RETURN labels(n), count(n)"`:

| Phân Loại Khái Niệm Đồ Thị (Graph Node Types) | Tỷ Trọng Số Lượng | 
| --- | --- |
| Folder (Thư mục Logic) | 293 |
| Component (File Mảng) | 1,138 |
| Section (Phân khu) | 32 |
| Class (Lớp Đối tượng cốt lõi) | 141 |
| Interface (Giao thức Data) | 43 |
| Property (Thuộc tính) | 389 |
| Function (Hàm Chức Năng Độc Lập) | 832 |
| Method (Phương thức của Lớp)  | 420 |
| Process (Quy trình nghiệp vụ ráp nối) | 203 |
| Community (Nhóm Module liên kết chặt chẽ) | 199 |
| CodeEmbedding (Bộ nhớ nhúng Vectơ) | 2,574 |

## 3. Bản Đồ Mạch Cốt Lõi (Critical Execution Flows)
Dựa vào chỉ mục phân tách của LIVA, hệ thống duy trì các luồng sau:
1. **[Core Orchestrator Loop]** (`src/core/AgentLoop.ts` - 171 Nodes phụ thuộc): Khởi tạo *Context-Lane Dispatching*, phân luồng `dispatch()` và dọn rác bằng cơ chế `TTL-Sentinel`. Liên kết ngầm với OpenAI API của Não 26B và 4B.
2. **[Singularity Daemon]** (`src/auto_singularity.ts` - 48 Edges): Vòng lặp Vĩnh Cửu (Infinity Loop), chịu trách nhiệm Hot-Swap Cổng VRAM 8000 và 8001 giữa Router/Expert. Lắng nghe `Line-Shift Error` để báo hiệu thoái lui.
3. **[AI Scientist Skill]** (`src/skills/AIScientist.ts` - 82 Edges): Kiến trúc Persistent Sandbox, thao tác Git Node qua nhánh Song Song (Parallel branch), trích xuất *Sliding Windows*, và thực thi `git diff HEAD`.
4. **[Temporal Token Memory]** (`src/memory/TurboQuantStore.ts` - 21 Nodes): Khâu rễ lưu bộ nhớ dài hạn, cấu trúc bằng Hash/Vector O(1).
5. **[Skill Interceptor]** (`src/SkillRegistry.ts`): Trạm Gác (Auth Gate) cho phép AI đăng ký kĩ năng gọi RPA ra ngoài UI.

> *Dữ liệu đã được nén và update thành công thông qua lệnh `npx gitnexus analyze --force`*.
