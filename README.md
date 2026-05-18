<div align="center">
  <img src="https://img.icons8.com/?size=100&id=113063&format=png" alt="LIVA Logo" width="150" height="150" />

  # LIVA Cognitive OS 🧠
  *Hệ điều hành Nhận thức Thu nhỏ - Trợ lý AI Siêu việt*

  [![GitHub stars](https://img.shields.io/github/stars/DuongNAD/LIVA?style=social)](https://github.com/DuongNAD/LIVA/stargazers)
  [![GitHub forks](https://img.shields.io/github/forks/DuongNAD/LIVA?style=social)](https://github.com/DuongNAD/LIVA/network/members)
  [![License](https://img.shields.io/badge/License-Custom_All_Rights_Reserved-red.svg)](LICENSE)

</div>

## 👨‍💻 Giới thiệu Tác giả
Xin chào! Mình là **Nguyễn Anh Dương**, hiện đang là sinh viên trường **Đại học FPT Hà Nội**. 
Dự án **LIVA Cognitive OS** là tâm huyết của mình nhằm tạo ra một hệ điều hành nhận thức (Cognitive OS) thực thụ, tích hợp sâu vào hệ thống máy tính với khả năng suy luận, tương tác thời gian thực và quản lý bộ nhớ thông minh.

---

## 🚀 Tổng quan Hệ thống (System Overview)
LIVA không chỉ là một chatbot, mà là một **Hệ điều hành Nhận thức (Cognitive OS)** chạy độc lập dưới dạng Sidecar trên máy tính cá nhân. Với độ trễ (latency) cực thấp và khả năng tích hợp linh hoạt, LIVA có thể:
- **Nghe & Nói Thời gian thực:** Pipeline Whisper (STT) + Kokoro (TTS) tốc độ cao.
- **Trí nhớ Định tuyến (Semantic Memory):** Tự động truy xuất L0/L1/L2/L3 Memory thông qua SQLite-Vec cực kỳ nhẹ, tối ưu Token.
- **Tương tác OS (RPA):** Điều khiển Zalo, Messenger, File System và các ứng dụng Desktop một cách tự động với cơ chế Lazy-Init.
- **Giao diện Nổi (Overlay UI):** Widget Desktop Tauri v2 siêu nhẹ, nền trong suốt với Dashboard quản lý 2D chuyên nghiệp.

## 📁 Kiến trúc Monorepo Hiện đại
Dự án được cấu trúc theo dạng Monorepo với nguyên tắc Single Responsibility Principle (SRP):

1. **`liva-gateway`** *(Node.js / TypeScript)*:
   - Trái tim điều phối của LIVA. Quản lý `ModelOrchestrator`, quy trình suy luận (AgentLoop), và quản trị bộ nhớ `StructuredMemory`.
   - Chứa hơn 78+ kỹ năng theo chuẩn MCP (Model Context Protocol).

2. **`liva-ai-engine`** *(Python / C++)*:
   - Động cơ cốt lõi (Native Engine). Sử dụng `llama.cpp` (C++) để tối đa hóa hiệu suất inference cục bộ với VRAM.
   - Cơ chế khóa luồng (Mutex Lock) song song: Sinh text (Chat) và Nhúng vector (Embedding) chạy độc lập, triệt tiêu hoàn toàn độ trễ (Zero-latency).

3. **`liva-desktop`** *(Tauri v2 / Rust / Vue 3)*:
   - Ứng dụng Desktop nhẹ, tương tác với hệ điều hành ở mức thấp (OS-level).
   - Bao gồm Widget trong suốt (Ghost Mode) và Memory Dashboard hiển thị dữ liệu trực quan theo thời gian thực qua WebSocket.

4. **`packages/liva-common`**:
   - Chứa các kiểu dữ liệu dùng chung (Types, Schemas) giữa Frontend và Backend.

## 🛠 Hướng dẫn Cài đặt & Khởi chạy

### Yêu cầu hệ thống
- **Node.js** v22+
- **Python** 3.10+
- **Hệ điều hành:** Windows 10/11
- **RAM:** Tối thiểu 16GB.
- **GPU:** NVIDIA (Có hỗ trợ CUDA) để chạy Native AI Engine hiệu quả.

### Bắt đầu nhanh
Toàn bộ dự án đã được tích hợp thành một kịch bản tự động hoàn chỉnh. Tại thư mục gốc của dự án, bạn chỉ cần mở Terminal/PowerShell và gõ:

```powershell
.\start.ps1
```
Hệ thống sẽ tự động:
1. Giải phóng các cổng mạng đang bị chiếm dụng.
2. Khởi tạo Whisper STT, AI Native Engine, và Kokoro Voice Engine.
3. Kích hoạt LIVA Gateway.
4. Bật giao diện LIVA Tauri Desktop.

## 🛡️ Bản quyền & Giấy phép (License)
Dự án này thuộc bản quyền sở hữu trí tuệ của **Nguyễn Anh Dương** và được bảo vệ bởi **Giấy phép Cá nhân & Nội bộ (Personal & Internal Use License)**.
- Bạn **ĐƯỢC PHÉP** sử dụng, học hỏi, nâng cấp và sửa đổi cho mục đích cá nhân.
- Bạn **TUYỆT ĐỐI KHÔNG ĐƯỢC PHÉP** đăng tải lại, sao chép để chia sẻ công khai, bán, hay cung cấp dưới dạng dịch vụ (SaaS).

Vui lòng đọc chi tiết tại file [`LICENSE`](LICENSE).
