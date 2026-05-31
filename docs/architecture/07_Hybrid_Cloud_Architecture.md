# Kế Hoạch Triển Khai Kiến Trúc Tổng Thể Cho Dự Án Trợ Lý LIVA

## 1. Tổng Quan Kiến Trúc Hệ Thống Đám Mây Lai (Hybrid Cloud Architecture Overview)
Sự bùng nổ của các mô hình ngôn ngữ lớn (LLM) và các tác nhân AI tự trị (autonomous agents) đã định hình lại hoàn toàn các tiêu chuẩn về cơ sở hạ tầng công nghệ. Quá trình phát triển và triển khai dự án trợ lý AI Liva — với cốt lõi là khả năng tự nâng cấp và thực thi mã nguồn độc lập — đòi hỏi một bản thiết kế hệ thống vượt ra ngoài các mô hình máy chủ - máy khách (client-server) truyền thống. 

Việc phân tách kiến trúc thành ba thực thể vật lý riêng biệt bao gồm: **một máy chủ tính toán cục bộ** (máy case hoặc Mac host), **một máy chủ trung gian** (VPS/Cloud host) hoạt động như cổng giao tiếp, và **các thiết bị đầu cuối đa nền tảng** (Mobile app, Desktop app) tạo ra một mô hình đám mây lai (hybrid cloud) tinh vi.

Mô hình này mang lại lợi thế tuyệt đối về mặt tối ưu hóa chi phí đầu tư phần cứng và bảo vệ quyền riêng tư dữ liệu ở mức cao nhất, bởi vì toàn bộ quá trình suy luận (inference) tính toán nặng nề và việc lưu trữ dữ liệu nhạy cảm đều được thực hiện nội bộ ngay tại máy chủ cá nhân của người dùng. Tuy nhiên, sự phân tán này cũng đồng thời đưa ra những thách thức kỹ thuật vô cùng phức tạp. Hệ thống cần sự kết hợp chặt chẽ của các giao thức mạng riêng ảo, kiến trúc cân bằng tải, và công nghệ vi máy ảo (MicroVM) để đảm bảo an ninh tuyệt đối trước nguy cơ tiêm nhiễm (prompt injection).

---

## 2. Nền Tảng Lưu Trữ Khâu Trung Gian: Cổng Giao Tiếp Và Vượt Tường Lửa (Edge Gateway)
Phân hệ đầu tiên cần giải quyết trong kiến trúc phân tán của Liva là thiết lập một điểm kết nối công cộng an toàn. Các thiết bị đầu cuối tuyệt đối không được phép kết nối trực tiếp vào mạng nội bộ (nhằm chống port scanning và DDoS). Thay vào đó, hệ thống sử dụng VPS làm cổng trung gian.

### 2.1. Thách Thức Về Mạng Diện Rộng Và Định Tuyến CGNAT
Tại Việt Nam, các ISP như Viettel, FPT, VNPT áp dụng CGNAT khiến việc port forwarding qua Router trở nên vô nghĩa. Ba hướng giải quyết bao gồm:
1. **Cloudflare Tunnels**: Tốt nhưng có rủi ro vi phạm TOS (phát luồng dữ liệu LLM non-HTTP) và phá vỡ mã hóa đầu cuối do giải mã tại máy chủ biên.
2. **SSH Reverse Tunneling**: Dễ đứt kết nối, hiệu năng không cao.
3. **Mạng Riêng Ảo Dạng Lưới (Tailscale / WireGuard)**: **Sự lựa chọn hoàn hảo nhất.** Sử dụng kỹ thuật NAT traversal đâm thủng tường lửa, kết nối trực tiếp (P2P) máy cục bộ và VPS với mã hóa cấp độ gói tin và độ trễ chỉ 1-3ms.

### 2.2. Chiến Lược Lựa Chọn Hạ Tầng Máy Chủ VPS
Cấu hình VPS chỉ cần ở mức 2 vCPU và 2GB-4GB RAM (chủ yếu chuyển tiếp mạng). Yếu tố quyết định là **Độ Trễ Mạng (Network Latency)**.

- **Hà Nội / TP.HCM**: Trễ cực thấp (< 20ms), loại bỏ rủi ro đứt cáp biển gây lag. Phù hợp nhất nếu 100% người dùng ở Việt Nam.
- **Singapore**: Trễ 40-60ms, hạ tầng toàn cầu ổn định nhưng dễ chịu ảnh hưởng bởi cáp biển đứt.
👉 Dựa trên phân tích, VPS tại Hà Nội / HCM là tối ưu nhất.

### 2.3. Thiết Lập Cổng API, Xác Thực và Ủy Quyền Ngược (Reverse Proxy)
- **Cổng API (Nginx / Envoy)**: Thiết lập tại VPS (lắng nghe port 80/443). SSL/TLS được giải mã tại đây trước khi Nginx định tuyến luồng dữ liệu xuống địa chỉ IP Tailscale của LIVA Local. Lối kiến trúc này giúp môi trường vận hành LIVA phía sau tàng hình hoàn toàn khỏi Internet.
- **Bảo mật (JWT & OAuth 2.1)**: Liva App sẽ xác thực để nhận Token JWT (tuổi thọ 15-60 phút). Tại VPS, Nginx dùng `auth_jwt` xác minh chữ ký; nếu token hợp lệ, luồng dữ liệu mới được đi vào đường hầm VPN về máy trạm.

---

## 3. Trung Tâm Xử Lý Cục Bộ: Kiến Trúc Động Cơ Suy Luận Và Quản Lý Vòng Đời AI
Lõi xử lý vật lý tập trung tại máy Local (Mac/Case), nạp LLM và xử lý toán học ma trận.

### 3.1. Lựa Chọn Động Cơ Suy Luận (Native Engine vs llama-server)
- **Native Engine (gRPC)**: Động cơ suy luận tốc độ cao, giao tiếp trực tiếp qua gRPC (port 8100), hoàn toàn bỏ qua overhead của HTTP. Hỗ trợ Hot-Swap các mô hình `.gguf` qua `mmap` trực tiếp trên VRAM để tránh VRAM Thrashing.
- **llama-server (C++)**: Động cơ HTTP tương thích chuẩn OpenAI. Dễ dàng triển khai đa nền tảng nhưng chịu giới hạn overhead của HTTP REST.
👉 _Định hướng v29:_ Kiến trúc Sequential Hot-Swap yêu cầu Native Engine (gRPC) để tối ưu độ trễ đổi model từ Router (4B) sang Expert (26B) chỉ trong 5-15s trên cùng một GPU.

### 3.2. Triển Khai Blue-Green Deployment
Tính năng tự nâng cấp khiến LIVA thường xuyên tải lại model khổng lồ. Để Zero-Downtime:
- Duy trì **Môi trường Blue** (hoạt động) và **Môi trường Green** (khởi tạo và làm nóng).
- Định tuyến lưu lượng 5% qua Green để Canary Testing. Khi các node đoạt Health Checks 100%, nginx sẽ đảo traffic sang Green hoàn toàn theo mạch liền lạc.

### 3.3. Logic Phục Hồi Tự Động (Rollback) & Trích Xuất Bối Cảnh
LLM dễ bị "fail silently" (ảo giác, rò rỉ token). Nếu vLLM báo tỷ lệ lỗi vuợt chuẩn thông qua Prometheus Trigger, hệ thống lập tức Auto-Rollback về luồng Blue.
**Bắt Buộc:** Phải trích xuất nhật ký lỗi từ luồng Green trước khi sụp đổ để nhúng vào trí nhớ Tiến hóa của LIVA (LearningLog) nhằm ngăn lặp lại sai sót ở chu trình học sau.

---

## 4. Ranh Giới An Ninh Thực Thi: Kiến Trúc Hộp Cát Kín (Isolated Sandboxing)
Liva có thể sinh ra và chạy mã độc do ảo giác hoặc do mọc "cửa sau" từ tiêm nhiễm Prompt.

### 4.1. Hạn chế của Docker Container
Container chia sẻ chung Host Kernel Linux. Liva chạy mã độc có nguy cơ "Docker Breakout", phá nát máy chủ cá nhân.

### 4.2. Khung Ảo Hóa Lớp Phần Cứng - Firecracker MicroVMs
Là công nghệ do AWS phát minh, cấp CPU/Bộ nhớ/Linux Kernel hoàn toàn tách biệt. Firecracker Snapshot cho phép MicroVM khởi động thần tốc dưới `150ms`. An toàn tuyệt đối trước mọi kỹ thuật nhúng mã sâu ở hạt nhân.

### 4.3. Đánh Chặn Bí Mật (MITM Proxy Secret Injection)
Ngay trong MicroVM, Liva không được biết biến môi trường thực (API Key, Database Root). Khi Liva gọi ra API bên ngoài, Firewall layer 4 (`nftables`) ép luồng ra ngoài phải đi ngang qua Transparent proxy trên Máy Chủ vật lý. Proxy đánh chặn TLS, hoán đổi thẻ giữ chỗ (Placeholder) bằng Mật mã Auth đích thực rối mới truyền tiếp đường đi. Cơ chế này loại bỏ 100% rò rỉ Key ngay cả khi LIVA phản bội gửi Key ra ngoài cho Hacker.

---

## 5. Hệ Sinh Thái Ứng Dụng Đầu Cuối: Phát Triển Đa Nền Tảng
### 5.1. Ứng Dụng Di Động (Mobile App) với Flutter
Xóa sổ React Native JavaScript Bridge. Động cơ `Impeller` của Flutter render pixel thông qua Vulkan/Metal, duy trì ổn định 60-120fps cho giao diện Hội Thoại, phù hợp với App cá nhân cần mượt và nhất quán.

### 5.2. Ứng Dụng Desktop với Tauri 2.0
Loại bỏ khung Electron khổng lồ. Tauri 2.0 (nhân Rust Backend + Native WebView cho Frontend) tạo ra Desktop App LIVA dao động chỉ `3MB - 10MB`. Hầu như không chiếm RAM của hệ thống, cởi trói bộ nhớ rảnh rỗi cho VRAM của Mac kéo trạm LLM ngầm ở nhà.

### 5.3. Định Tuyến Ngữ Cảnh Qua Giao Thức (MCP)
Không bắn hết dự liệu cá nhân lên LLM. Ứng dụng Desktop/Mobile sẽ tích hợp thư viện **Model Context Protocol (MCP)** làm local server. Khi LIVA cần biết file nào đang mở trên App, thông qua WebRPC, nó sẽ truy vấn riêng rẽ và App sẽ gửi đoạn text giới hạn đủ phục vụ câu hỏi, bảo vệ dữ liệu cực độ.

---

## 6. Lộ Trình Triển Khai (Roadmap)
- **Giai đoạn 1: Lõi Máy Chủ Cục Bộ:** Triển khai Native Engine (gRPC), môi trường vi máy ảo Firecracker Snapshot và chiến lược Sequential Hot-Swap (Router/Expert).
- **Giai đoạn 2: Trạm Trung Gian & Đường Hầm Nền Tảng:** Cấu hình VPS HN/HCM, triển khai Tailscale p2p đâm CGNAT. Dựng Nginx chặn JWT. 
- **Giai đoạn 3: Ranh Giới An Ninh Tư Động Hóa:** Áp dụng Matchlock Proxy đổi token giữ chỗ trên tường lửa Máy Chủ khi microVM giao tiếp. 
- **Giai đoạn 4: Ứng Dụng Khách MCP:** Đóng gói bản Mobile Flutter & Desktop Tauri siêu nhẹ, đấu nối tín hiệu STT (Voice). 

## 7. Kết Luận
Bản thiết kế này là tiêu chuẩn kiến trúc tối thượng cho kỷ nguyên điện toán biên và trợ lý AI phi tập trung hiện nay. Kết hợp Tailscale P2P, Firecracker MicroVM, Native Engine Hot-Swap cùng LIVA Auto-Singularity. Hệ thống củng cố sức mạnh nội tại vượt quá ranh giới một phần mềm tiện ích, vươn lên thành một Trạm Trợ Lý Độc Lập Bất Khả Xâm Phạm.
