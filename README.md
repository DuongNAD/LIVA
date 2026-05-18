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

## 🧩 Hệ thống Trí nhớ Đa tầng (Multi-tier Memory System)
Một trong những điểm làm nên sự khác biệt cốt lõi và đáng tự hào nhất của LIVA chính là kiến trúc **Trí nhớ Mô phỏng Não bộ**. Thay vì nhồi nhét toàn bộ lịch sử trò chuyện vào Prompt (gây tốn Token, giật lag và làm AI "lú lẫn"), LIVA chia trí nhớ thành 4 tầng riêng biệt và quản lý bằng cơ sở dữ liệu Vector `SQLite-Vec` siêu nhẹ:

1. **Tầng L0 (Working RAM - Trí nhớ Làm việc):** 
   - **Chức năng:** Hoạt động giống như bộ nhớ đệm (buffer) của não người.
   - **Cơ chế:** Lưu trữ các biến số tạm thời, trạng thái giao diện UI đang mở, các câu lệnh đang thực thi dở dang. Dữ liệu tầng này hoàn toàn "vô hình" với Prompt và sẽ bị xóa sạch (Flush) ngay khi tác vụ kết thúc để tiết kiệm tài nguyên.

2. **Tầng L1 (Session Memory - Trí nhớ Ngắn hạn):**
   - **Chức năng:** Lưu trữ ngữ cảnh của cuộc hội thoại hiện tại.
   - **Cơ chế:** Giữ lại khoảng 10-20 lượt trao đổi gần nhất. Khi bộ nhớ L1 đầy hoặc khi phiên làm việc kết thúc, LIVA sẽ kích hoạt một tiến trình nền ngầm (Reflection Daemon) để chắt lọc các ý chính, rút ra bài học và đẩy chúng xuống tầng L2. Giúp duy trì Context Window luôn ở mức lý tưởng và siêu tốc.

3. **Tầng L2 (Semantic Vector Memory - Trí nhớ Ngữ nghĩa Dài hạn):**
   - **Chức năng:** Trí nhớ vĩnh viễn chứa những "Sự thật" (Facts), sở thích cá nhân của người dùng, và kiến thức hệ thống đã học được.
   - **Cơ chế:** Mọi dữ liệu được mã hóa thành các mảng Vector đa chiều (Embeddings) và lưu vào các file SQLite. Khi người dùng hỏi một vấn đề từng nhắc đến ở quá khứ, thuật toán định tuyến (Semantic Router) sẽ thực hiện quét độ tương đồng (Similarity Search) để bốc chính xác mảnh ký ức đó từ L2 lên, ghép vào ngữ cảnh hiện tại với độ trễ chỉ tính bằng mili-giây.

4. **Tầng L3 (Consolidation Archive - Nén & Lưu trữ Cấu trúc):**
   - **Chức năng:** Nén và hình thành nhận thức, củng cố tri thức.
   - **Cơ chế:** Thường chạy ngầm vào ban đêm (Nightly Cron) hoặc khi hệ thống rảnh rỗi. Máy học sẽ đọc lại toàn bộ L2, kết nối các mảnh thông tin rời rạc, nhận diện các thói quen của người dùng và lưu trữ lại dưới dạng Đồ thị Tri thức (Knowledge Graph) bảo mật.

---

## 🚀 Tổng quan Tính năng & Kiến trúc Monorepo
Dự án được thiết kế chặt chẽ theo nguyên tắc **Single Responsibility Principle (SRP)** và chia thành 4 module chính:

### 1. `liva-gateway` (Node.js / TypeScript)
- Đóng vai trò là "Bộ não trung tâm" điều phối toàn bộ các tiến trình. Quản lý Vòng lặp Quyết định (`AgentLoop`) và quản trị bộ nhớ (`StructuredMemory`).
- Sở hữu hệ thống Kỹ năng đồ sộ với hơn **78+ kỹ năng** theo chuẩn **MCP (Model Context Protocol)**, cho phép AI thao tác từ việc tra cứu Internet, gửi Email, thao tác hệ điều hành (RPA) đến việc tự động lập trình.
- **AI Tự Sửa Lỗi (Self-Correction):** Khi một công cụ (Tool) bị lỗi, hệ thống ngầm tự động phân tích mã lỗi, suy luận nguyên nhân và tìm hướng giải quyết khác mà không bị "treo".

### 2. `liva-ai-engine` (Python / C++)
- "Động cơ cốt lõi" (Native AI Engine) được tối ưu hóa để chạy trực tiếp trên máy tính cá nhân. Sử dụng `llama.cpp` (C++) để tối đa hóa hiệu năng suy luận (Inference) bằng VRAM của GPU.
- Hỗ trợ **Nghe & Nói Thời gian thực** thông qua Whisper (STT) và Kokoro (TTS).
- **Đột phá về hiệu năng:** Tách biệt hoàn toàn cơ chế khóa luồng (Mutex Lock) giữa việc Sinh văn bản (Chat Generation) và Nhúng dữ liệu (Vector Embedding). Nhờ vậy, AI có thể vừa trò chuyện vừa ghi nhớ vào SQLite cùng lúc, triệt tiêu hoàn toàn độ trễ 6-8 giây (Zero-latency).

### 3. `liva-desktop` (Tauri v2 / Rust / Vue 3)
- Ứng dụng Desktop siêu nhẹ, cung cấp Bảng điều khiển Trí nhớ (Memory Dashboard) 2D hiển thị theo thời gian thực (Real-time).
- Cung cấp Widget tương tác, hỗ trợ "Ghost Mode" (cho phép người dùng click xuyên qua cửa sổ AI mà không ảnh hưởng công việc).

### 4. `packages/liva-common`
- Gói thư viện chia sẻ chung (Shared Library), chứa định nghĩa các Kiểu dữ liệu (Types, Interfaces) đồng bộ giữa Frontend và Backend.

---

## 🛠 Hướng dẫn Cài đặt & Sử dụng (Step-by-Step Guide)

Để khởi chạy LIVA một cách hoàn hảo trên máy tính cá nhân, hãy làm theo hướng dẫn sau:

### Bước 1: Chuẩn bị Môi trường (Prerequisites)
- **Node.js**: Phiên bản 22.x trở lên (hỗ trợ chuẩn ESM).
- **Python**: Phiên bản 3.10 hoặc 3.11 (đảm bảo đã tick chọn "Add Python to PATH" khi cài đặt).
- **Trình duyệt**: Cài đặt Google Chrome (để phục vụ hệ thống điều khiển RPA).
- **Phần cứng**: Tối thiểu 16GB RAM. 
- **GPU**: NVIDIA (Có hỗ trợ CUDA) với **VRAM tối thiểu 8GB (Khuyến nghị 12GB)** để chạy AI Engine trơn tru nhất.
- **Model Đề xuất (GGUF)**: Để tối ưu hóa tốc độ và khả năng nhận thức, hệ thống khuyến nghị sử dụng các Model Local sau (chuẩn định dạng `.gguf`):
  - Dành cho VRAM 8GB: `Gemma-4-E4B-Instruct`
  - Dành cho VRAM 12GB+: `Gemma-26B-Instruct`

### Bước 2: Tải Dự án và Cài đặt
Mở Terminal / PowerShell và chạy các lệnh sau:

```bash
# 1. Clone repository về máy
git clone https://github.com/DuongNAD/LIVA.git
cd LIVA

# 2. Cài đặt các gói thư viện Node.js cho toàn bộ Monorepo
npm install
```

### Bước 3: Cấu hình Môi trường (Environment Variables)
Hệ thống cần các API Keys hoặc thông tin Model để suy luận:
1. Mở thư mục `liva-gateway/`.
2. Sao chép file `.env.example` thành `.env`.
3. Điền các cấu hình quan trọng (Ví dụ: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, hoặc cấu hình Model Local, đường dẫn trình duyệt).

### Bước 4: Khởi chạy Hệ thống
Quay lại thư mục gốc của dự án (`LIVA/`), mở PowerShell bằng **Quyền Quản trị viên (Run as Administrator)** (để ứng dụng có quyền quản lý giao diện OS) và gõ lệnh khởi động cực kỳ đơn giản:

```powershell
.\start.ps1
```

**Quá trình khởi chạy sẽ diễn ra tự động hoàn toàn:**
1. Kịch bản tự tạo môi trường ảo Python (`venv`) và tự cài `requirements.txt`.
2. Tự động kiểm tra và giải phóng các cổng mạng (Port 8082, 8100, 5173).
3. Khởi tạo Whisper STT, C++ Native AI Engine và Kokoro Voice Engine.
4. Bật giao diện người dùng LIVA Tauri Desktop trên màn hình máy tính.

*(Lưu ý: Trong lần chạy đầu tiên, hệ thống có thể cần thời gian tải các module và weights của AI Model, vui lòng đảm bảo kết nối mạng ổn định).*

### Bước 5: Hướng dẫn Sử dụng Thực tế
- **Tương tác Cơ bản:** Sau khi giao diện nổi (Overlay) hiện lên, bạn có thể click vào thanh chat để nhập lệnh text hoặc dùng Micro để gọi hội thoại.
- **Theo dõi Trí nhớ (Memory Dashboard):** Mở giao diện Dashboard trên UI để quan sát trực tiếp luồng dữ liệu đang chảy giữa tầng L1 và L2. Bạn có thể thấy rõ AI đang suy nghĩ gì, lưu gì và đang dùng Công cụ (Tool) nào ở hậu cảnh.
- **Chế độ Tàng hình (Ghost Mode):** Giao diện của LIVA được thiết kế hiển thị xuyên thấu. Bạn có thể tương tác với các ứng dụng khác ngay dưới LIVA mà không bị cản trở.

---

## 🤝 Lời kêu gọi Đóng góp (Contributing)
Để biến **LIVA** từ một Trợ lý cá nhân trở thành một **Cognitive OS** hoàn chỉnh là một chặng đường dài. Mình rất hoan nghênh và trân trọng mọi sự hỗ trợ từ cộng đồng lập trình viên:

- **Báo lỗi (Issues):** Nếu bạn gặp bug trong quá trình cài đặt hay sử dụng, hãy mở Issue.
- **Tối ưu hóa (Optimization):** Rất cần các cao thủ cải thiện hiệu suất Rust (Tauri), tinh chỉnh System Prompt, hoặc tối ưu tốc độ và quản lý bộ nhớ cho `llama.cpp`.
- **Phát triển Tính năng (Pull Requests):** Viết thêm các MCP Skills mới (như điều khiển Smarthome, kết nối API mới), hoặc nâng cấp giao diện Dashboard 2D.

Mặc dù dự án có một số quy định để tránh bị sao chép thương mại hóa sai mục đích (xem phần Bản quyền bên dưới), nhưng bạn hoàn toàn có thể tự do Fork, vọc vạch, tối ưu và **gửi Pull Request** về kho lưu trữ gốc này để chúng ta cùng nhau xây dựng LIVA mạnh mẽ hơn!

---

## 🛡️ Bản quyền & Giấy phép (License)
Dự án này thuộc bản quyền sở hữu trí tuệ của **Nguyễn Anh Dương** và được bảo vệ bởi **Giấy phép Cá nhân & Nội bộ (Personal & Internal Use License)**.
- Bạn **ĐƯỢC PHÉP** tải về, sử dụng, học hỏi, nâng cấp và sửa đổi cho mục đích cá nhân.
- Bạn **TUYỆT ĐỐI KHÔNG ĐƯỢC PHÉP** đăng tải lại, sao chép để chia sẻ công khai như một dự án mới, cấm thương mại hóa, bán, hay cung cấp dưới dạng dịch vụ (SaaS).

Chi tiết cụ thể vui lòng đọc tại file [`LICENSE`](LICENSE).
