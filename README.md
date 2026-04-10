# LIVA - Enterprise Desktop AI Assistant

LIVA là hệ thống Trợ lý ảo Desktop sở hữu năng lực tự động suy luận sâu, quản trị đa Agent và tự chủ vận hành (RPA) thông qua kiến trúc hệ thống Cục bộ (Local LLM) 3 phân lớp linh hoạt bậc nhất. LIVA được thiết kế với tư duy kỹ thuật thực dụng (Pragmatic Engineering), kết hợp giữa sức mạnh AI mã nguồn mở và khả năng tinh chỉnh sát sườn cấu hình phần cứng.

---

## 🌟 Chức năng Vùng Lõi Tiên Tiến (Core Architecture State-Of-The-Art)

Dự án đã trải qua các đợt đại tu "đập đi xây lại" nhằm đạt độ bền bỉ Zero-latency và an toàn cấp độ Server:

- 🏎️ **Dual-Port Handoff (Chuyển giao 0s):** LIVA sử dụng song song mô hình Router Model (gemma-4-E4B) túc trực vĩnh viễn ở System RAM (Port 8000) nhằm tiếp đón tốc độ cao. Khi nhiệm vụ phức tạp, LIVA **chỉ bốc Expert Model (26B)** ép toàn phần lên VRAM ở một Port 8001 riêng biệt. Sau khi não Chuyên gia chạy xong sẽ tự động "Rút điện" VRAM để tiết kiệm tài nguyên.
- 🎯 **Semantic Tool Router (RAG Lightweight):** Với hơn 30+ kỹ năng dễ gây ngộp (Context Bloat), Engine Node.js của LIVA triển khai bộ đếm Lexical/Jaccard Similarity kết hợp `search_keywords` độc quyền. Thuật toán tự động cắt lọc bớt hàng rào, **chỉ dâng Top-5 Tools khớp nhất lên System Prompt**, chống phình ngốn Token mà không cần dùng Vector Database phức tạp.
- 🛡️ **Bảo Mật HITL chống Prompt Injection:** Kỹ năng sinh tử `execute_command` được trang bị **Whitelist (Danh Sách Trắng)** cực kỳ khắt khe chặn đứng mọi kỹ thuật Obfuscation (Làm rối mã bằng dấu `^` hoặc Base64). Trước khi chạm đến lõi OS, hệ thống **Human-in-the-loop (HITL)** bắt buộc chặn đứng Terminal bằng biến `readline` đòi hỏi người quản trị (Admin) gõ `y/yes` để phê duyệt. Kháng độc và Hijack máy hoàn toàn!
- ♻️ **Self-Correction Guardrails (Hàng rào Tự Động Định Tuyến):** LIVA không bao giờ văng rớt khi kỹ năng đứt gãy. Khi một API Tool bị Crash bẩn, LIVA bọc nó lại thành Message thân thiện ném về System, ép Agent tự soi Lỗi (Reflection) và thử cách khác. LIVA cũng tích hợp **Deterministic Hash-Set** ngăn cấm LLM điền lại tham số sai ở Vòng Lặp Tử Thần (Doom Loop) và ấn định cảnh hạ cánh mềm (Graceful Exit) ở chu kỳ suy diễn số 5.

## 🛠️ Trạm Kỹ Năng Thông Minh (Dynamic Plugin Skills)

LIVA trang bị rương kỹ năng khổng lồ (26+ Skills), được phân loại thành 5 phân viện chính và **nạp động vào RAM qua cơ chế Auto-Discovery** (chỉ việc thả file `.ts` vào thư mục, không cần sửa Code Lõi):

1. **⚙️ Hệ Điều Hành & Quản Trị Hệ Thống (Mức độ bảo mật cao)**
   - `ExecuteCommand`: Chạy shell/cmd (Có bọc HITL chốt chặn).
   - `ListDirectory`, `ReadLocalFile`, `WriteLocalFile`, `DeleteLocalFile`: Thanh tra và Thao túng File System Local.
   - `GetSystemInfo`: Đọc thông số RAM/Storage/CPU.
   - `GitSyncProject`: Đồng bộ hóa mã nguồn tự động với Github.

2. **🧠 AI Scientist & Lập Trình Tự Trị**
   - `AIScientist`: Vòng lặp lập trình tự trị (Autonomous coding loop).
   - `PlanWriter`, `ReportWriter`: Sinh báo cáo, phác thảo thiết kế thông minh chuẩn kỹ nghệ.
   - `ResearchIdeation`: Đào sâu logic, phản biện đa chiều cho các luận điểm kỹ thuật.

3. **💬 Tự Động Hóa Mạng Xã Hội (RPA)**
   - `SendZaloBot`: Tương tác API Zalo OA.
   - `SendZaloRPA`, `SendMessengerRPA`: Thao túng DOM Browser giả lập thao tác gõ/gửi tin nhắn thực tế.
   - `ReadEmails`, `ReadRecentEmails`: Truy cập và tóm tắt Inbox (IMAP/Google).

4. **☁️ Tương tác Google Workspace**
   - `ReadGoogleSheet`, `WriteGoogleSheet`: Phân tích và nạp dữ liệu đa chiều.
   - `SearchGoogleDrive`, `CreateGoogleDoc`, `AppendGoogleDoc`: Thao tác soạn thảo văn bản trực tiếp với Google ecosystem.

5. **🌐 Khai Thác Mạng Chuyên Sâu**
   - `WebSearch`, `WebBrowser`: Khai thác Internet, mảng bám HTML và chắt lọc Content thời gian thực.
   - `GetWeather`: Khai thác Sensor thời tiết siêu cục bộ.

---

## 📂 Tổ Chức Modules Kiến Trúc

Dự án được chia thành 3 modules chính siêu chặt chẽ (Single Responsibility Principle - SRP):

1. **`liva-ui`** `(Vue 3 / Electron)`: 
   - Ứng dụng Desktop cho phép khởi chạy nhân vật hiển thị trong suốt (tàng hình nền) trên góc Desktop.
   - Chịu trách nhiệm hiển thị giao diện, đồ họa 3D Animation và thu thập tương tác với người dùng ở tiền tuyến.

2. **`openclaw-gateway`** `(Node.js / TypeScript)`:
   - Hệ thống Não bộ Trung gian (Gateway) với 3 nhánh: `ModelOrchestrator.ts` lo chạy mô hình, `PromptBuilder.ts` lo bối cảnh RAG và Ký ức, `AgentLoop.ts` chuyên chịu trách nhiệm vòng Quyết định (Decision Loop).
   - Giao thiệp chặt chẽ với các nền tảng Zalo, Google Workspace, hệ thống báo cáo Nội bộ qua bộ kỹ năng Auto-Discovery (Dynamic Plugin Loader).

3. **`liva-ai-engine`** `(Python)`: 
   - Động cơ AI Native xử lý Inference, tự động khởi động các Llama Server cục bộ tùy theo cấu hình `.env` chỉ thị mà không có Hardcode dính trên code.

---

## ⚙️ Yêu cầu & Hướng dẫn Cài Đặt

### Yêu Cầu Cấu Hình
- **Node.js** (Khuyến nghị bản v18 trở lên)
- **Python** (Bản v3.10 trở lên)
- **RAM**: Tối thiểu 32GB (Khuyến nghị 48GB trở lên để chạy cơ cấu ngâm RAM Router Model)
- Cấu hình Key mạng và Đường dẫn Folder Model nằm hoàn toàn ở `.env` của `openclaw-gateway`.

### Tiến Hành
1. **Cài đặt các dependencies**:
   - `liva-ui` & `openclaw-gateway`: chạy `npm install` 
   - `liva-ai-engine`: `python -m venv venv` => Cài đặt `pip install -r requirements.txt`. (Có sẵn các scripts `fix_cuda.ps1`, `install_cuda.ps1`).

2. **Khởi chạy Hệ thống tự động (Automate Pipeline)**:
   - Ở thư mục gốc (`e:\Project\LIVA`), nhấn đúp vào file `start_all.bat`.
   - Tiến trình Ghost Orchestrator sẽ thiết lập hệ sinh thái Backend và bật hiển thị UI Front-end hoàn chỉnh.

---

## 🌿 Quản Lý Đa Nhánh (Git Versioning)

Dự án duy trì đồng thời 2 luồng phát triển song song nhưng có cấu trúc Git khác nhau:
- Nhánh **`main`**: Toàn bộ tinh hoa mã nguồn + Kho dữ liệu lưu trữ / Logs nặng đầy đủ.
- Nhánh **`lite`**: Một bản rẽ phái nhẹ tựa lông hồng, cắt đứt các File tài nguyên Cache rác (Puppeteer Profiles, Database Vectors). Giúp đồng bộ dễ dàng trên Cloud IDE hoặc máy tính phụ mà không lo tràn băng thông.

> LIVA không ngừng được phát triển trên kiến trúc tối tân và sự giám sát thực tiễn của các Admin/Software Engineers chuyên sâu! Cảm ơn bạn đã sử dụng.
