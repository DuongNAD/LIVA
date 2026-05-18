<div align="center">

  # LIVA - The AI Assistant 🧠
  *Trợ lý Cá nhân Đa năng (Jarvis) - Nền tảng hướng tới Hệ điều hành Nhận thức*

  [![GitHub stars](https://img.shields.io/github/stars/DuongNAD/LIVA?style=social)](https://github.com/DuongNAD/LIVA/stargazers)
  [![GitHub forks](https://img.shields.io/github/forks/DuongNAD/LIVA?style=social)](https://github.com/DuongNAD/LIVA/network/members)
  [![License](https://img.shields.io/badge/License-Custom_All_Rights_Reserved-red.svg)](LICENSE)

</div>

## 👨‍💻 Giới thiệu Tác giả
Xin chào! Mình là **Nguyễn Anh Dương**, hiện đang là sinh viên trường **Đại học FPT Hà Nội**. 
Dự án **LIVA** hiện tại là một Trợ lý AI cá nhân (lấy cảm hứng từ Jarvis trong Iron Man). Đây là tâm huyết và cũng là những bước đi đầu tiên của mình trên hành trình nghiên cứu, xây dựng một **Hệ điều hành Nhận thức (Cognitive OS)** thực thụ trong tương lai.

Vì dự án có quy mô khá lớn và được xây dựng bởi một cá nhân, nên chắc chắn sẽ còn nhiều thiếu sót. Mình rất mong nhận được sự hỗ trợ, góp ý và **đóng góp mã nguồn (Pull Requests)** từ cộng đồng để cùng nhau tối ưu, nâng cấp và phát triển dự án này ngày càng hoàn thiện hơn!

---

## 🚀 Tổng quan Hệ thống (System Overview)
LIVA không chỉ là một chatbot thông thường, mà là một **Trợ lý Cá nhân Đa năng** chạy độc lập dưới dạng Sidecar trên máy tính cá nhân. Với độ trễ (latency) cực thấp và khả năng tích hợp linh hoạt, hệ thống có thể:

- **Nghe & Nói Thời gian thực:** Tích hợp quy trình xử lý giọng nói tốc độ cao sử dụng Whisper (STT) để nghe và Kokoro (TTS) để phát âm thanh tự nhiên.
- **Trí nhớ Định tuyến (Semantic Memory):** Tự động phân cấp và truy xuất Trí nhớ L0 (RAM), L1 (Ngắn hạn), L2 (Dài hạn), L3 (Lịch sử) thông qua cơ sở dữ liệu `SQLite-Vec`. Cực kỳ nhẹ, truy vấn siêu tốc và tối ưu hóa số lượng Token (Context Window).
- **Tương tác Cấp Hệ điều hành (RPA & OS Control):** Tự động hóa các tác vụ trên Zalo, Messenger, quản lý File System, và điều khiển các cửa sổ ứng dụng Windows thông qua cơ chế Lazy-Init (khởi tạo trễ để tiết kiệm tài nguyên).
- **Giao diện Nổi (Overlay UI):** Ứng dụng Desktop sử dụng Tauri v2 siêu nhẹ, hiển thị tàng hình (Ghost Mode) trên màn hình máy tính với Bảng điều khiển Trí nhớ (Memory Dashboard) 2D chuyên nghiệp, cập nhật theo thời gian thực.
- **AI Tự Sửa Lỗi (Self-Correction):** Tích hợp vòng lặp tự đánh giá. Khi một công cụ (Tool) bị lỗi, AI tự động phân tích mã lỗi, suy luận nguyên nhân và tìm hướng giải quyết khác mà không bị "treo" hệ thống.

---

## 📁 Kiến trúc Monorepo Hiện đại
Dự án được thiết kế chặt chẽ theo nguyên tắc Single Responsibility Principle (SRP) và chia thành 4 module chính:

### 1. `liva-gateway` (Node.js / TypeScript)
- Đóng vai trò là "Bộ não trung tâm" điều phối toàn bộ các tiến trình.
- Quản lý `ModelOrchestrator`, vòng lặp Quyết định (`AgentLoop`), và quản trị bộ nhớ (`StructuredMemory`).
- Sở hữu hệ thống Kỹ năng đồ sộ với hơn **78+ kỹ năng** theo chuẩn **MCP (Model Context Protocol)**, cho phép AI thao tác từ việc tra cứu Internet, phân tích Data, gửi Email, đến việc tự động lập trình.

### 2. `liva-ai-engine` (Python / C++)
- "Động cơ cốt lõi" (Native AI Engine) được tối ưu hóa để chạy trực tiếp trên máy cá nhân.
- Sử dụng `llama.cpp` (C++) để tối đa hóa hiệu năng suy luận (Inference) bằng VRAM của GPU.
- **Đột phá về hiệu năng:** Tách biệt hoàn toàn cơ chế khóa luồng (Mutex Lock) giữa việc Sinh văn bản (Chat Generation) và Nhúng dữ liệu (Vector Embedding). Nhờ vậy, AI có thể vừa trò chuyện vừa ghi nhớ vào SQLite cùng lúc, triệt tiêu hoàn toàn độ trễ 6-8 giây (Zero-latency) khi mới khởi động.

### 3. `liva-desktop` (Tauri v2 / Rust / Vue 3)
- Ứng dụng Desktop nhẹ, tương tác với hệ điều hành ở mức thấp (OS-level).
- Cung cấp Widget tương tác, hỗ trợ "Ghost Mode" (cho phép người dùng click xuyên qua cửa sổ AI) và giao diện Quản lý bộ nhớ thời gian thực thông qua WebSocket.

### 4. `packages/liva-common`
- Gói thư viện chia sẻ chung (Shared Library), chứa định nghĩa các Kiểu dữ liệu (Types, Interfaces) và Schema kết nối giữa Frontend và Backend, giúp mã nguồn luôn đồng bộ và hạn chế lỗi.

---

## 🛠 Hướng dẫn Cài đặt & Khởi chạy

### Yêu cầu hệ thống
- **Node.js**: Phiên bản 22 trở lên (chuẩn ESM).
- **Python**: Phiên bản 3.10 trở lên.
- **Hệ điều hành**: Windows 10/11.
- **RAM**: Tối thiểu 16GB.
- **GPU**: NVIDIA (Có hỗ trợ CUDA) với **VRAM tối thiểu 8GB (Khuyến nghị 12GB)** để chạy Native AI Engine hiệu quả.

### Bắt đầu nhanh
Toàn bộ dự án đã được tự động hóa bằng script tích hợp. Tại thư mục gốc của dự án, bạn chỉ cần mở Terminal hoặc PowerShell và gõ:

```powershell
.\start.ps1
```

**Quá trình khởi động sẽ tự động thực hiện:**
1. Kiểm tra và giải phóng các cổng mạng (Port) bị chiếm dụng.
2. Khởi tạo Whisper STT, C++ Native AI Engine và Kokoro Voice Engine.
3. Kích hoạt trung tâm điều phối LIVA Gateway.
4. Bật giao diện người dùng LIVA Tauri Desktop.

*(Lưu ý: Trong lần chạy đầu tiên, hệ thống có thể cần thời gian tải các module và weights của AI Model, vui lòng đảm bảo kết nối mạng ổn định).*

---

## 🤝 Lời kêu gọi Đóng góp (Contributing)
Để biến **LIVA** từ một Trợ lý cá nhân trở thành một **Cognitive OS** hoàn chỉnh là một chặng đường dài. Mình rất hoan nghênh và trân trọng mọi sự hỗ trợ từ cộng đồng lập trình viên:

- **Báo lỗi (Issues):** Nếu bạn gặp bug trong quá trình cài đặt hay sử dụng, hãy mở Issue.
- **Tối ưu hóa (Optimization):** Cải thiện hiệu suất Rust (Tauri), tinh chỉnh Prompt, hoặc tối ưu tốc độ cho `llama.cpp`.
- **Phát triển Tính năng (Pull Requests):** Viết thêm các MCP Skills mới, tích hợp các công cụ mới hoặc nâng cấp giao diện Dashboard.

Mặc dù dự án có các giới hạn về mặt thương mại (xem phần Bản quyền bên dưới), nhưng bạn hoàn toàn có thể tự do Fork, vọc vạch, tối ưu và gửi Pull Request về kho lưu trữ gốc này để cùng nhau xây dựng LIVA mạnh mẽ hơn!

---

## 🛡️ Bản quyền & Giấy phép (License)
Dự án này thuộc bản quyền sở hữu trí tuệ của **Nguyễn Anh Dương** và được bảo vệ bởi **Giấy phép Cá nhân & Nội bộ (Personal & Internal Use License)**.
- Bạn **ĐƯỢC PHÉP** tải về, sử dụng, học hỏi, nâng cấp và sửa đổi cho mục đích cá nhân.
- Bạn **TUYỆT ĐỐI KHÔNG ĐƯỢC PHÉP** đăng tải lại, sao chép để chia sẻ công khai như một dự án mới, cấm thương mại hóa, bán, hay cung cấp dưới dạng dịch vụ (SaaS).

Chi tiết cụ thể vui lòng đọc tại file [`LICENSE`](LICENSE).
