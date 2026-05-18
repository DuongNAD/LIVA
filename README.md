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

2. **Tầng L0.5 (Context Buffer - Bộ đệm Ngữ cảnh):**
   - **Chức năng:** Cầu nối trung gian giữa bộ đệm tạm thời và trí nhớ ngắn hạn.
   - **Cơ chế:** Giữ lại các thông tin mấu chốt của các tác vụ hoặc Tool Calls vừa mới hoàn thành (ví dụ: kết quả tìm kiếm web, dữ liệu phân tích hệ thống). Giúp AI duy trì luồng suy nghĩ (Chain-of-Thought) ngay lập tức mà không cần đẩy lại toàn bộ dữ liệu thô vào lịch sử trò chuyện chính.

3. **Tầng L1 (Session Memory - Trí nhớ Ngắn hạn):**
   - **Chức năng:** Lưu trữ ngữ cảnh của cuộc hội thoại hiện tại.
   - **Cơ chế:** Giữ lại khoảng 10-20 lượt trao đổi gần nhất. Khi bộ nhớ L1 đầy hoặc khi phiên làm việc kết thúc, LIVA sẽ kích hoạt một tiến trình nền ngầm (Reflection Daemon) để chắt lọc các ý chính, rút ra bài học và đẩy chúng xuống tầng L2. Giúp duy trì Context Window luôn ở mức lý tưởng và siêu tốc.

4. **Tầng L2 (Semantic Vector Memory - Trí nhớ Ngữ nghĩa Dài hạn):**
   - **Chức năng:** Trí nhớ vĩnh viễn chứa những "Sự thật" (Facts), sở thích cá nhân của người dùng, và kiến thức hệ thống đã học được.
   - **Cơ chế:** Mọi dữ liệu được mã hóa thành các mảng Vector đa chiều (Embeddings) và lưu vào các file SQLite. Khi người dùng hỏi một vấn đề từng nhắc đến ở quá khứ, thuật toán định tuyến (Semantic Router) sẽ thực hiện quét độ tương đồng (Similarity Search) để bốc chính xác mảnh ký ức đó từ L2 lên, ghép vào ngữ cảnh hiện tại với độ trễ chỉ tính bằng mili-giây.

5. **Tầng L3 (Consolidation Archive - Nén & Lưu trữ Cấu trúc):**
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

## 🧰 Kho Kỹ năng Tích hợp (MCP Skills Ecosystem)
LIVA được trang bị một hệ sinh thái khổng lồ với hơn **78+ kỹ năng** hoạt động dưới chuẩn **Model Context Protocol (MCP)**. Hệ thống này biến AI từ một chatbot trò chuyện thông thường thành một **Siêu Trợ lý Thực thi (Agentic AI)** có khả năng thao tác trực tiếp với thế giới thực. Các kỹ năng được phân chia thành các cụm module chuyên sâu:

### 1. 💻 Quản trị Hệ điều hành & Tệp tin (OS & File System)
LIVA có khả năng kiểm soát sâu vào hệ điều hành Windows/Linux nội bộ của bạn:
- **Thao tác tệp nâng cao:** `read_file`, `write_file`, `list_dir`, `grep_search` (tìm kiếm chuỗi RegEx tốc độ cao trong hàng vạn dòng code).
- **Trình chỉnh sửa mã (Code Editor):** Hỗ trợ `replace_file_content` và `multi_replace_file_content` để sửa mã nguồn thông minh mà không cần ghi lại toàn bộ file.
- **Thực thi lệnh System:** `ExecuteCommand` cho phép chạy bất kỳ lệnh Terminal/PowerShell nào (như `npm install`, `python script.py`).
- **Quản lý Cửa sổ & Tài nguyên:** Giám sát mức tiêu thụ RAM/CPU, tự động đóng các tiến trình bị treo.
- 🛡️ **Bảo mật HITL (Human-in-the-Loop):** Mọi thao tác xóa tệp hoặc chạy lệnh nguy hiểm đều tự động bị chặn lại, yêu cầu người dùng gõ xác nhận `y/yes` trên terminal.

### 2. 🤖 Lập trình Tự trị & Kỹ sư Phần mềm (AI Software Engineer)
Được truyền cảm hứng từ *The AI Scientist*, LIVA có thể tự đóng vai trò là một Senior Developer:
- **Tự động hóa Git & GitNexus:** Tự động sử dụng `gitnexus_impact` và `gitnexus_query` để đánh giá rủi ro (Blast Radius) trước khi sửa bất kỳ hàm nào trong dự án, đảm bảo không làm "hỏng" code cũ.
- **Tự sửa lỗi (Self-Correction Loop):** Tự chạy Test/Lint, đọc log lỗi, tự suy luận nguyên nhân (Reflection) và viết lại đoạn code bị hỏng cho đến khi chương trình chạy được.
- **Lập kế hoạch & Báo cáo:** Sử dụng `PlanWriter` để phân rã một tính năng lớn thành các task nhỏ, và `ReportWriter` để viết tài liệu Markdown tổng kết sau khi code xong.

### 3. 💬 Tự động hóa Mạng xã hội & Giao tiếp (RPA Communication)
Hệ thống RPA (Robotic Process Automation) giúp LIVA thay bạn làm các công việc chân tay nhàm chán:
- **Zalo RPA (Độc quyền):** `SendZaloMessage`, `ZaloPolling` giúp tự động đọc tin nhắn chưa xem, phân loại khách hàng, và tự động phản hồi theo kịch bản có sẵn.
- **Facebook Messenger:** Bóc tách thông tin liên hệ, tự động trả lời bình luận/tin nhắn.
- **Email Management:** Kết nối trực tiếp qua IMAP/SMTP để đọc hòm thư (`ReadEmail`), tóm tắt hàng chục email dài thành một đoạn văn ngắn, phân loại thư rác và tự động soạn thảo thư phản hồi chuyên nghiệp (`SendEmail`).

### 4. 📊 Hệ sinh thái Google Workspace & Văn phòng
LIVA kết nối mượt mà với các công cụ làm việc nhóm:
- **Google Sheets:** Tự động đọc (`read_sheet`), cập nhật số liệu (`update_sheet`), phân tích dữ liệu bán hàng, tính toán tài chính và định dạng bảng tính.
- **Google Docs:** Tự động chèn văn bản, soạn thảo hợp đồng, tóm tắt các tài liệu dài.
- **Google Drive:** Tìm kiếm, quản lý và tải lên/tải xuống tài liệu từ Cloud.
- **Phân tích Data Cục bộ:** Công cụ `pdf_parser` và `csv_analyzer` để trích xuất dữ liệu từ các file báo cáo nội bộ.

### 5. 🌐 Khai thác Internet & Trình duyệt Ẩn (Web Mining)
Khi kiến thức cục bộ là chưa đủ, LIVA sẽ vươn ra Internet:
- **Web Search Thời gian thực:** Truy vấn thông tin, cập nhật tin tức nóng hổi, giá vàng, chứng khoán bằng công cụ Search Engine.
- **Trình duyệt Tự trị (Headless Browser):** Sử dụng các công cụ `read_browser_page`, `click_element`, `type_text` để mở một trình duyệt tàng hình. LIVA có thể tự click nút, điền form, cào dữ liệu (Scraping) trên các trang web phức tạp không hỗ trợ API.
- **Ngoại cảnh:** Truy vấn `Weather_API` và `Time_API` để lấy thông tin môi trường xung quanh phục vụ cho hội thoại.

---

## 🛠 Hướng dẫn Cài đặt & Sử dụng (Step-by-Step Guide)

Để khởi chạy LIVA một cách hoàn hảo trên máy tính cá nhân, hãy làm theo hướng dẫn sau:

### Bước 1: Chuẩn bị Môi trường (Prerequisites)
- **Node.js**: Phiên bản 22.x trở lên (hỗ trợ chuẩn ESM).
- **Python**: Phiên bản 3.10 hoặc 3.11 (đảm bảo đã tick chọn "Add Python to PATH" khi cài đặt).
- **Trình duyệt**: Cài đặt Google Chrome (để phục vụ hệ thống điều khiển RPA).
- **Phần cứng**: Tối thiểu 16GB RAM. 
- **GPU**: NVIDIA (Có hỗ trợ CUDA) với **VRAM tối thiểu 8GB (Khuyến nghị 12GB)** để chạy AI Engine trơn tru nhất.
- **Mô hình Trí tuệ (Dual-Model Architecture)**: Dự án sử dụng kiến trúc phân luồng hai mô hình AI (chuẩn `.gguf`) để tối ưu hóa cả tốc độ phản hồi lẫn độ sâu suy luận:
  - **Model Router (Điều hướng & Logic nhanh):** Khuyên dùng `Gemma 4 E4B`.
  - **Model Heavy (Nhận thức sâu & Giao tiếp):** Khuyên dùng `Gemma 26B`.

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

### Quy trình đóng góp (How to contribute)
Nếu bạn muốn đề xuất nâng cấp hoặc sửa đổi mã nguồn, vui lòng thực hiện theo các bước chuẩn của mã nguồn mở:
1. **Fork** dự án này về tài khoản Github của bạn.
2. Tạo một nhánh mới (Branch) cho tính năng của bạn: `git checkout -b feature/TenTinhNang`
3. Commit các thay đổi của bạn: `git commit -m 'feat: Thêm tính năng XYZ'`
4. Push lên nhánh vừa tạo: `git push origin feature/TenTinhNang`
5. Mở một **Pull Request (PR)** trên kho lưu trữ gốc của LIVA. Mình sẽ xem xét, thảo luận và gộp (merge) code của bạn vào dự án chính!

*(Mặc dù dự án có một số quy định để tránh bị sao chép thương mại hóa sai mục đích, nhưng bạn hoàn toàn có thể tự do đóng góp mã nguồn về kho lưu trữ gốc này để cùng nhau xây dựng LIVA mạnh mẽ hơn!)*

---

## 🛡️ Bản quyền & Giấy phép (License)
Dự án này thuộc bản quyền sở hữu trí tuệ của **Nguyễn Anh Dương** và được bảo vệ bởi **Giấy phép Cá nhân & Nội bộ (Personal & Internal Use License)**.
- Bạn **ĐƯỢC PHÉP** tải về, sử dụng, học hỏi, nâng cấp và sửa đổi cho mục đích cá nhân.
- Bạn **TUYỆT ĐỐI KHÔNG ĐƯỢC PHÉP** đăng tải lại, sao chép để chia sẻ công khai như một dự án mới, cấm thương mại hóa, bán, hay cung cấp dưới dạng dịch vụ (SaaS).

Chi tiết cụ thể vui lòng đọc tại file [`LICENSE`](LICENSE).

---

## 🙏 Lời cảm ơn (Acknowledgments)
Dự án LIVA được xây dựng dựa trên sự kế thừa và đứng trên vai những người khổng lồ. Xin gửi lời cảm ơn sâu sắc tới các cộng đồng mã nguồn mở, các tác giả bài báo khoa học và các dự án tuyệt vời đã cung cấp công nghệ nền tảng hoặc mã nguồn (snippets) truyền cảm hứng cho LIVA, điển hình như:

**Các Nghiên cứu Khoa học & Bài báo (Research Papers):**
- Lấy cảm hứng mạnh mẽ từ báo cáo nghiên cứu *"The AI Scientist: Towards Fully Automated Open-Ended Scientific Discovery"*, giúp định hình và xây dựng vòng lặp Lập trình Tự trị (AI Scientist) cho dự án.
- Các nghiên cứu chuyên sâu về **Cognitive Architecture** (Kiến trúc Nhận thức), **Self-Reflection** (Tự đánh giá/sửa lỗi) và **Semantic Memory**, tạo tiền đề cho hệ thống Trí nhớ đa tầng L0-L3.

**Công nghệ Nền tảng (Open-Source Core):**
- Cộng đồng **llama.cpp** vì một AI Engine siêu tốc độ, tận dụng tối đa phần cứng cục bộ.
- Đội ngũ **Tauri** và **Vue 3** cho nền tảng giao diện Desktop siêu nhẹ.
- Mã nguồn **SQLite-Vec** hỗ trợ hệ thống truy vấn Vector cục bộ.
- Các mô hình AI mã nguồn mở từ **Google (Gemma)**, **Qwen**, **Meta**.
- Và vô số các thư viện mã nguồn mở nhỏ lẻ khác đã góp phần tạo nên hệ sinh thái khổng lồ của LIVA ngày hôm nay.
