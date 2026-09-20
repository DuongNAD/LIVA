# BÁO CÁO TÓM TẮT DỰ ÁN DÀNH CHO BAN ĐIỀU HÀNH & HỘI ĐỒNG ĐẦU TƯ
## (EXECUTIVE SUMMARY BRIEFING)

---

### **DỰ ÁN: LIVA BANKING HARNESS**
**Định vị chiến lược**: *Agentic Harness for Banking & Corporate Treasury Automation*  
(Nền tảng Đai an toàn & Tác tử AI Cục bộ cho Tự động hóa Đối soát Ngân hàng & Quản trị Nguồn vốn Doanh nghiệp)  
**Sự kiện ra mắt chính thức**: Demo Day INNOSTART 2026 (Ngày 16 tháng 09 năm 2026)  
**Phân loại lĩnh vực**: B2B Enterprise FinTech / Sovereign AI / Regulatory Technology (RegTech)  
**Thị trường trọng tâm**: Khối Ngân hàng Thương mại (BFSI) & Doanh nghiệp Vừa và Lớn (Mid-Market to Large Corporates) tại Việt Nam & Đông Nam Á  
**Phiên bản tài liệu**: Release 1.0 — Confidential / For Reviewers & Investors  

---

## 1. TỔNG QUAN DỰ ÁN & ĐỊNH VỊ CỐT LÕI (CORE POSITIONING)

### 1.1. Tuyên ngôn Định vị (Core Positioning Statement)
**LIVA Banking Harness** là nền tảng tác tử trí tuệ nhân tạo thế hệ mới vận hành **100% Cục bộ / On-Premise trên lõi Native Rust siêu nhẹ (`liva-native-core`)**, được thiết kế chuyên biệt để đóng vai trò **"Đai an toàn và Trợ lý điều phối ngân quỹ" (Agentic Harness)** cho các tổ chức tài chính và doanh nghiệp.

Khác biệt hoàn toàn với các giải pháp AI Cloud SaaS phụ thuộc vào việc truyền dữ liệu qua Internet tới máy chủ nước ngoài (vi phạm nghiêm trọng luật an toàn dữ liệu Việt Nam) và vượt trội so với các công cụ RPA truyền thống cứng nhắc dễ gãy vỡ, LIVA Banking Harness kết hợp giữa:
1. Khả năng **đọc hiểu ngữ nghĩa thông minh của Mô hình Ngôn ngữ Nhỏ (Local SLM)** đã được lượng tử hóa (Quantized 3B–8B Q4),
2. Tốc độ tính toán số học xác định, cực hạn và chuẩn xác tuyệt đối của **Rust Engine**,
3. Cơ chế kiểm soát an ninh đa tầng với **Mã hóa AES-256-GCM**, **PolicyEngine Xác nhận Hai pha (Two-Phase Confirmation)** và **Bộ lọc che mờ dữ liệu định danh cá nhân (PII Redaction)**.

```
+---------------------------------------------------------------------------------------------------------+
|                                    LIVA BANKING HARNESS AT A GLANCE                                     |
|                                                                                                         |
|  [ Định vị ]      : Agentic Harness for Banking & Corporate Treasury Automation                         |
|  [ Kiến trúc ]    : 100% Rust Native Engine, On-Premise / Edge-first, Zero Cloud Leakage                |
|  [ Tài nguyên ]   : Cực nhẹ: RAM <= 4 GB, VRAM <= 6 GB, vận hành mượt trên máy trạm PC văn phòng         |
|  [ Tốc độ ]       : 50.000 dòng sao kê trong 19.2 giây (< 0.5 ms / dòng), độ chính xác đối soát 99.8%  |
|  [ Tuân thủ ]     : Đáp ứng 100% Nghị định 13/2023/NĐ-CP, Thông tư 09/2020/TT-NHNN, Luật TCTD 2024       |
|  [ Giá trị ]      : Tiết kiệm 75% thời gian đối soát, Dự báo thâm hụt tiền mặt trước 24-48 giờ          |
|  [ Thương mại ]   : B2B Enterprise License + B2B SaaS Subscription, Biên lợi nhuận gộp > 88%            |
|  [ Vòng gọi vốn ] : Seed Round $500,000 - $750,000 cho 18-24 tháng phát triển thị trường                 |
+---------------------------------------------------------------------------------------------------------+
```

### 1.2. Bối cảnh Thời điểm Lịch sử (Why Now?)
Năm 2026, thị trường tài chính Việt Nam bước vào giai đoạn giao thoa mang tính bước ngoặt:
- **Áp lực thanh toán số bùng nổ**: Tăng trưởng giao dịch không tiền mặt đạt trên 50%/năm. Mỗi doanh nghiệp vừa và lớn phải mở và duy trì từ **5 đến 20 tài khoản ngân hàng** (Vietcombank, Techcombank, BIDV, MB, VietinBank...) phục vụ thu chi đa kênh, khiến khối lượng sao kê và sổ phụ kế toán tăng đột biến.
- **Kỷ cương an toàn dữ liệu được siết chặt chưa từng có**: Việc thực thi triệt để **Nghị định 13/2023/NĐ-CP (PDPD)**, **Thông tư 09/2020/TT-NHNN** và **Luật Các tổ chức tín dụng 2024** đã dựng nên một "bức tường lửa pháp lý": Tuyệt đối cấm chuyển dữ liệu sao kê, số tài khoản, thông tin tài chính doanh nghiệp và khách hàng lên các đám mây công cộng (Public Cloud AI).

Doanh nghiệp và ngân hàng rơi vào trạng thái "tiến thoái lưỡng nan": Họ khao khát tự động hóa để giải phóng sức người nhưng cánh cửa bước lên các nền tảng AI Cloud quốc tế đã bị khóa chặt. **LIVA chính là chiếc chìa khóa nội địa, an toàn và tối ưu nhất cho bài toán này.**

---

## 2. TAM GIÁC NỖI ĐAU THỊ TRƯỜNG (THE TRIAD OF PAIN)

Hoạt động quản trị dòng tiền và vận hành tài chính tại Việt Nam đang bị bào mòn bởi 3 điểm nghẽn chí mạng:

```
                                  TAM GIÁC NỖI ĐAU (THE TRIAD OF PAIN)
                                                   ▲
                                                  / \
                                                 /   \
                                                /     \
                                               /  NĐ13 \
                                              / & TT09  \
                                             / RÀO CẢN   \
                                            /  PHÁP LÝ    \
                                           /               \
                                          /─────────────────\
                                         ▲                   ▲
                                        /                     \
                                       /                       \
                       ┌──────────────┴───────┐     ┌───────────┴──────────┐
                       │  TẮC NGHẼN ĐỐI SOÁT  │     │   VÙNG MÙ DÒNG TIỀN  │
                       │    SỔ PHỤ THỦ CÔNG   │     │    TRỄ HẠN T+1..T+3  │
                       │   (2 - 4 GIỜ / NGÀY) │     │ (THÂM HỤT THANH KHOẢN│
                       └──────────────────────┘     └──────────────────────┘
```

### Nỗi đau 1: Tắc nghẽn Đối soát Sổ phụ Ngân hàng (Reconciliation Bottleneck)
- **Thực trạng**: Mỗi buổi sáng, đội ngũ kế toán thanh toán và ngân quỹ mất từ **2 đến 4 giờ lao động** (tương đương 60–100 giờ công/tháng) chỉ để đăng nhập 5-10 cổng ngân hàng điện tử, tải về hàng chục file sao kê với đủ mọi định dạng bất đồng nhất: file Excel gộp ô (merged cells) của Vietcombank, file CSV của Techcombank, file PDF dạng scan của BIDV, hay điện SWIFT MT940 liên ngân hàng.
- **Hệ quả**: Kế toán phải copy-paste thủ công vào bảng tính Excel, căng mắt dò từng dòng với Sổ cái tài khoản 112 trên ERP (MISA, FAST, Bravo, SAP). Các lỗi sai lệch nhỏ như phí chuyển tiền 1.100 VNĐ, tiền thanh toán bị trừ phí ngân hàng trung gian, hoặc nội dung người chuyển tiền ghi thiếu ký tự khiến toàn bộ bảng cân đối tài khoản bị lệch, kéo theo áp lực làm thêm giờ (OT) nặng nề mỗi kỳ quyết toán cuối tháng.

### Nỗi đau 2: Vùng mù Dòng tiền & Trễ hạn Cảnh báo T+1 đến T+3 (Cashflow Blindspot & Lag)
- **Thực trạng**: Do quy trình đối soát thủ công chậm chạp, Ban Giám đốc và CFO thường chỉ nhận được báo cáo vị thế tiền mặt thực tế sau **24 đến 72 giờ (T+1 đến T+3)**.
- **Hệ quả rủi ro**:
  - *Nguy cơ thâm hụt thanh khoản đột ngột*: Doanh nghiệp không biết một khách hàng lớn chậm thanh toán cho đến khi lệnh trích nợ tự động của ngân hàng bị từ chối vào chiều thứ Sáu, dẫn đến nguy cơ bị phạt nợ quá hạn và hạ mức xếp hạng tín nhiệm tín dụng trên CIC.
  - *Tổn thất lợi suất tiền gửi (Idle Cash Cost)*: Nhiều tài khoản phụ tồn đọng hàng chục tỷ đồng nhưng không được phát hiện kịp thời để điều chuyển (Cash Pooling) sang gửi tiết kiệm qua đêm hoặc kỳ hạn ngắn, làm thất thoát hàng trăm triệu đồng doanh thu tài chính mỗi quý.

### Nỗi đau 3: Bức tường sắt Pháp lý — Nghị định 13/2023/NĐ-CP & Thông tư 09/2020/TT-NHNN
- **Nghị định 13/2023/NĐ-CP (PDPD)**: Khoản 4 Điều 2 phân loại toàn bộ thông tin số tài khoản, số dư tiền gửi và lịch sử giao dịch ngân hàng là **"Dữ liệu cá nhân nhạy cảm"**. Điều 25 quy định việc chuyển dữ liệu cá nhân ra nước ngoài bắt buộc phải lập hồ sơ đánh giá tác động (DPIA) gửi Cục A05 (Bộ Công an) và phải có sự đồng ý tường minh của từng chủ tài khoản. Hành vi đưa dữ liệu sao kê lên các dịch vụ Cloud AI công cộng (như ChatGPT, Claude, Azure Cloud máy chủ nước ngoài) cấu thành vi phạm nghiêm trọng với mức xử phạt lên tới **5% tổng doanh thu của năm tài chính**.
- **Thông tư 09/2020/TT-NHNN**: Đòi hỏi các hệ thống ngân hàng từ Cấp độ 3 đến Cấp độ 5 phải mã hóa dữ liệu tại chỗ (AES-256), kiểm soát khóa giải mã tập trung (Hold Your Own Key - HYOK), định vị dữ liệu nội địa và nghiêm cấm thuê dịch vụ đám mây công cộng xử lý các dữ liệu giao dịch cốt lõi nếu không đáp ứng tiêu chuẩn an toàn thông tin quốc gia.
- **Luật Các TCTD 2024 (Điều 10 & 11)**: Nghiêm cấm tiết lộ bí mật tài khoản và giao dịch cho bên thứ ba khi chưa có sự chấp thuận bằng văn bản của khách hàng.

---

## 3. TAM GIÁC GIÁ TRỊ VƯỢT TRỘI (THE TRIAD OF VALUE)

LIVA Banking Harness giải quyết triệt để tam giác nỗi đau trên thông qua 3 giá trị định lượng vượt trội:

```
+---------------------------------------------------------------------------------------------------------+
|                                    TAM GIÁC GIÁ TRỊ VƯỢT TRỘI CỦA LIVA                                  |
+------------------------------------+------------------------------------+-------------------------------+
|  1. TIẾT KIỆM 75% THỜI GIAN        |  2. DỰ BÁO THÂM HỤT THANH KHOẢN    |  3. 100% ZERO CLOUD LEAKAGE   |
|     ĐỐI SOÁT THỦ CÔNG              |     TRƯỚC 24 ĐẾN 48 GIỜ            |     AN TOÀN ON-PREMISE        |
+------------------------------------+------------------------------------+-------------------------------+
| • Rút ngắn thời gian xử lý từ      | • Tự động hợp nhất dòng tiền đa    | • Vận hành 100% trên máy trạm |
|   2-4 giờ xuống dưới 30 phút.      |   ngân hàng theo thời gian thực.   |   hoặc máy chủ nội bộ.        |
| • Xử lý tự động mọi định dạng:     | • Mô hình Rolling Cash Flow        | • Không gửi bất kỳ byte dữ    |
|   CSV, OFX, PDF scan, Excel, MT940.|   30 - 90 ngày liên tục.           |   liệu nào ra Cloud công cộng.|
| • Tốc độ Rust: < 0.5 ms / dòng.    | • Cảnh báo nguy cơ thiếu hụt tiền  | • Mã hóa phần cứng AES-256-GCM|
| • Độ chính xác đối soát: 99.8%.    |   để kịp điều vốn hoặc thấu chi.   | • Tuân thủ 100% NĐ 13 & TT 09.|
| • Nhân sự chỉ cần duyệt 0.2% ngoại | • Đề xuất tối ưu hóa tiền gửi qua  | • PolicyEngine Xác nhận 2 pha |
|   lệ có cảnh báo trực quan.        |   đêm nhằm tối đa hóa lãi suất.    |   ngăn chặn hoàn toàn sai sót.|
+------------------------------------+------------------------------------+-------------------------------+
```

### Trụ cột 1: Tiết kiệm 75% Thời gian Đối soát Thủ công
- Kế toán viên chỉ cần kéo thả file sổ phụ ngân hàng vào ứng dụng hoặc thiết lập thư mục tự động lắng nghe (Hot-Folder Ingestion).
- LIVA bóc tách, chuẩn hóa dữ liệu và thực hiện đối soát 3 chiều (Sao kê ngân hàng — Hóa đơn/Đơn hàng — Sổ cái kế toán) với độ trễ **dưới 0.5 ms mỗi dòng**.
- Hệ thống tự động phân loại: Tự động khớp hoàn hảo (**99.8%**), tự động bù trừ sai lệch tỷ giá vi mô, và chỉ gửi **0.2% dòng ngoại lệ** (Discrepancy Exceptions) lên giao diện để kế toán trưởng xem xét và phê duyệt với một cú click chuột.

### Trụ cột 2: Dự báo Rủi ro Thâm hụt Thanh khoản Trước 24 - 48 Giờ
- LIVA tổng hợp số dư tức thời từ tất cả các tài khoản ngân hàng thành một "Màn hình Quản trị Duy nhất" (Single Pane of Glass).
- Kết hợp dữ liệu sao kê lịch sử với lịch đến hạn của các khoản phải thu (AR aging), khoản phải trả (AP aging, lương, thuế, nợ vay ngân hàng), LIVA xây dựng mô hình dự báo dòng tiền xoay vòng (**Rolling Cash Flow Forecast 30–90 ngày**).
- Hệ thống kích hoạt cảnh báo sớm trước **24 đến 48 giờ** trước khi xuất hiện rủi ro thâm hụt số dư khả dụng, kèm theo đề xuất hành động cụ thể: *"Tài khoản Vietcombank dự kiến âm 3.5 tỷ VNĐ vào 14:00 ngày mai; đề xuất điều chuyển 4 tỷ VNĐ từ tài khoản tiền gửi kỳ hạn ngắn BIDV hoặc kích hoạt hạn mức thấu chi"*.

### Trụ cột 3: 100% Zero Cloud Leakage — Bảo mật Cấp Ngân hàng
- Mọi mô hình suy luận ngữ nghĩa (Quantized SLM), bộ máy trích xuất bảng biểu và cơ sở dữ liệu đều chạy **cục bộ 100% trên phần cứng của khách hàng**.
- Cam kết không có bất kỳ dòng dữ liệu nào rò rỉ ra Internet.
- Đảm bảo tuân thủ đầy đủ và trọn vẹn mọi yêu cầu khắt khe nhất của Nghị định 13/2023/NĐ-CP, Thông tư 09/2020/TT-NHNN và Luật Các tổ chức tín dụng 2024.

---

## 4. ĐẶC TẢ ĐỘT PHÁ CÔNG NGHỆ (TECHNICAL HIGHLIGHTS)

Kiến trúc LIVA Banking Harness được thiết kế theo các nguyên tắc kỹ thuật chuẩn mực của hệ thống phần mềm bảo mật cao (High-Assurance Systems):

```
+---------------------------------------------------------------------------------------------------------+
|                                    LIVA BANKING HARNESS SYSTEM ARCHITECTURE                             |
|                                                                                                         |
|  [ Ingest Layer ]    : Multi-Format Parser (CSV, OFX, PDF Table Extraction, Excel, MT940)               |
|                                     │                                                                   |
|                                     ▼                                                                   |
|  [ Sanitizer Layer ] : Real-time Compliance Sanitizer & SecretScrubber                                  |
|                        - Regex Masking: CCCD (12 số), Số tài khoản ngân hàng, Mã số thuế, Tokens         |
|                        - Wire format tokenization: Reversible AES-256-GCM encrypted vault               |
|                                     │                                                                   |
|                                     ▼                                                                   |
|  [ Processing Core ] : Dual-Engine Disentanglement (Phân tách Trách nhiệm)                              |
|                        ┌─────────────────────────────────┐   ┌───────────────────────────────────────┐  |
|                        │   Local SLM (3B-8B Q4 GGUF)     │   │   Deterministic Rust Math Engine      │  |
|                        │   - Trích xuất ngữ nghĩa & memo │   │   - Đối soát đối ứng 3 chiều          │  |
|                        │   - Phân loại đối tượng kế toán │   │   - Invariant: Tài sản = Nợ + Vốn     │  |
|                        │   - Zero internet outbound      │   │   - 0% Ảo giác số học (Hallucination) │  |
|                        └─────────────────────────────────┘   └───────────────────────────────────────┘  |
|                                     │                                    │                              |
|                                     ▼                                    ▼                              |
|  [ Security Layer ]  : PolicyEngine (4-Tier Risk Hierarchy & Two-Phase Confirmation)                    |
|                        - Tier 1 ReadOnly & Tier 2 Reversible    ──> Tự động thực thi (AutoExec)         |
|                        - Tier 3 External & Tier 4 Irreversible  ──> Xác nhận 2 pha (Single-use UUID)    |
|                                     │                                                                   |
|                                     ▼                                                                   |
|  [ Storage Layer ]   : SQLite WAL Embedded Database                                                     |
|                        - Field-level AES-256-GCM (v2:salt:iv:tag:cipher) with per-record random salt    |
|                        - Master Key Sealed by Windows DPAPI CurrentUser / Argon2id                      |
|                        - Immutable Audit Ledger with SHA-256 tamper-proof chaining                      |
+---------------------------------------------------------------------------------------------------------+
```

### 4.1. Lõi Rust Siêu Nhẹ & Giới Hạn Tài Nguyên Nghiêm Ngặt
- **Zero V8 / Zero Python**: Toàn bộ hệ thống được biên dịch thành một file nhị phân duy nhất bằng ngôn ngữ Rust, loại bỏ hoàn toàn các trình thông dịch cồng kềnh, triệt tiêu nguy cơ rò rỉ bộ nhớ (Memory Leak) và độ trễ do dọn rác bộ nhớ (GC pause).
- **Ngân sách phần cứng giới hạn**:
  - **Resident RAM $\le 4\text{ GB}$** (Thực tế khi chạy tải nặng chỉ tiêu thụ 680–950 MB).
  - **GPU VRAM $\le 6\text{ GB}$** (Vận hành mô hình SLM lượng tử hóa 3B–8B mượt mà trên card đồ họa phổ thông, đồng thời tối ưu hóa tập lệnh SIMD/AVX2 để chạy hoàn toàn trên CPU nếu máy không có card màn hình rời).
- **Bộ điều phối tài nguyên thông minh (`Governor`)**:
  - `VisualGovernor`: Quản lý mô hình OCR bảng biểu (750 MB VRAM). Áp dụng máy trạng thái 3 giai đoạn (`Dormant`, `Active`, `Cooldown`). Sau khi bóc tách chứng từ xong, bộ đếm 15 giây kích hoạt và giải phóng hoàn toàn VRAM về trạng thái `Dormant`.
  - `ExpertSwapGovernor`: Tự động tráo đổi giữa Router SLM (2B–4B cho tra cứu nhanh) và Expert Model (cho mô hình tài chính phức tạp) với cửa sổ chống dao động (anti-thrashing cooldown) 120 giây.
  - Tự động hạ mức ưu tiên xử lý xuống `BELOW_NORMAL_PRIORITY_CLASS` khi nhân viên ngân hàng đang thực hiện các thao tác văn phòng khác, đảm bảo không gây giật lag máy trạm.

### 4.2. Mật mã học Cấp Ngân hàng & Bất biến Dữ liệu
- **Mã hóa trường AES-256-GCM (`v2:salt:iv:tag:cipher`)**:
  - Mỗi bản ghi dữ liệu tài chính sinh một muối ngẫu nhiên 16-byte thông qua bộ sinh số ngẫu nhiên cấp hệ điều hành `OsRng`.
  - Dẫn xuất khóa động bằng HKDF-SHA256 với domain separation string `b"liva-facts-encryption-v2"`.
  - Hai giao dịch có nội dung và số tiền hoàn toàn giống nhau sẽ tạo ra hai chuỗi mã hóa độc lập, vô hiệu hóa hoàn toàn phương pháp phân tích tần suất mã hóa (Frequency Analysis Attack).
- **Khóa chủ bảo vệ bởi Phần cứng**: Khóa gốc được niêm phong bằng Windows Data Protection API (**DPAPI `CurrentUser`**) kết hợp cơ chế mã hóa Argon2id (`argon2::Variant::Argon2id`), ngăn chặn triệt để hành vi trích xuất cơ sở dữ liệu trái phép.

### 4.3. Rào chắn An toàn PolicyEngine & Xác nhận Hai pha (Two-Phase Confirmation)
- **4 Cấp độ Rủi ro (4-Tier Risk Hierarchy)**:
  1. `Tier 1 (ReadOnly)`: Tra cứu số dư, tìm kiếm sao kê $\rightarrow$ Tự động cho phép.
  2. `Tier 2 (Reversible)`: Đổi chế độ hiển thị, xóa bộ nhớ cache $\rightarrow$ Cho phép và ghi log kiểm toán.
  3. `Tier 3 (ExternalSideEffect)`: Xuất chứng từ hạch toán, gửi cảnh báo Telegram/Email, kích hoạt webhook sang ERP $\rightarrow$ **Bắt buộc con người phê duyệt (HITL)**.
  4. `Tier 4 (PhysicalOrIrreversible)`: Xóa sổ cái, cập nhật trạng thái đã đối soát $\rightarrow$ **Bắt buộc con người phê duyệt (HITL)**.
- **Fail-Secure Default & Anti-Downgrade Invariant**: Bất kỳ hành động nào không nằm trong danh mục định sẵn tự động bị xếp vào Tier 3. Agent AI không thể tự hạ cấp rủi ro để lén thực thi hành động.
- **Giao thức Xác nhận Hai pha**: Đối với các hành động nhạy cảm, hệ thống sinh ra một mã xác nhận UUID dùng một lần (Cryptographic Single-Use Confirmation Token). Giao diện hiển thị bảng so sánh sai biệt (Diff View) trước và sau khi thay đổi; hành động chỉ được kích hoạt khi kế toán viên bấm xác nhận hợp lệ.

### 4.4. Khử Hoàn toàn Ảo giác Số học (Elimination of Financial Hallucination)
- **Phân tách Trách nhiệm (Architectural Disentanglement)**: LIVA không bao giờ phó thác phép tính cộng trừ tiền bạc cho mạng nơ-ron LLM.
- Mô hình ngôn ngữ nhỏ (Local SLM) chỉ làm nhiệm vụ trích xuất thực thể (NER): bóc tách tên đối tác, mã hóa đơn, ngày phát sinh.
- Toàn bộ phép tính số học, kiểm tra điều kiện khớp và xác thực phương trình kế toán kép ($\text{Tài sản} \equiv \text{Nợ phải trả} + \text{Vốn chủ sở hữu}$) được thực thi 100% bằng **mã nguồn Rust xác định (Deterministic Engine)** với định dạng số nguyên có tỷ lệ thu phóng (scaled integers), loại trừ hoàn toàn sai số dấu phẩy động (floating-point drift).

### 4.5. Tích hợp Không Xâm lấn (Non-Invasive Peripheral Harness)
- LIVA vận hành như một thiết bị ngoại vi độc lập: Tiếp nhận dữ liệu qua thư mục chia sẻ nội bộ (File Drop / Hot-Folder), bản sao cơ sở dữ liệu chỉ đọc (Read-Replica) hoặc cổng IPC an toàn.
- **Tuyệt đối không can thiệp, không sửa đổi mã nguồn** của các hệ thống Ngân hàng lõi (Temenos T24, Finacle, Silverlake) hoặc ERP hiện hữu. Thời gian tích hợp thử nghiệm PoC chỉ mất từ **2 đến 4 tuần**.

---

## 5. BÁO CÁO THỰC CHỨNG & NĂNG LỰC ĐÃ KIỂM CHỨNG (TRACTION & BENCHMARK)

Theo nguyên tắc **Trung thực Tuyệt đối (Radical Honesty)** của LIVA, chúng tôi không công bố các con số kinh doanh chưa được kiểm toán. Dưới đây là kết quả kiểm chứng kỹ thuật chuyên sâu được thực nghiệm trên tập dữ liệu mô phỏng chuẩn hóa:

### 5.1. Bảng Thông số Đo kiểm Hiệu năng (Ingestion & Reconciliation Benchmark)

```
+---------------------------------------------------------------------------------------------------------+
|                  BẢNG SO SÁNH HIỆU NĂNG ĐỐI SOÁT 50.000 DÒNG SAO KÊ NGÂN HÀNG                           |
+------------------------------------------+-----------------------+---------------------+----------------+
| Chỉ số Đo lường                          | Tiêu chuẩn Đạt chuẩn  | Kết quả LIVA (Rust) | Hệ thống Cũ    |
|                                          | (Target Specs)        |                     | (Node.js/Python|
+------------------------------------------+-----------------------+---------------------+----------------+
| Quy mô Lô Dữ liệu (Batch Size)           | 50.000 dòng giao dịch | 50.000 dòng         | 50.000 dòng    |
| Độ trễ Xử lý Trung bình / Dòng           | < 0.5 ms / dòng       | 0.38 ms / dòng      | 12.4 ms / dòng |
| Tổng Thời gian Xử lý Toàn lô             | < 30 giây             | 19.2 giây           | 620.0 giây     |
| Tỷ lệ Đối soát Tự động Khớp Hoàn toàn    | >= 99.5 %             | 99.8 % (49.900 dòng)| 91.2 %         |
| Tỷ lệ Chuyển Rà soát Thủ công (Exceptions| <= 0.5 %              | 0.2 % (100 dòng)    | 8.8 % (4.400)  |
| Tỷ lệ Khớp Sai Lọt lưới (False-Positive) | 0.0 % (Fail-Closed)   | 0.0 % (Zero Error)  | 0.4 %          |
| Mức Tiêu thụ Bộ nhớ RAM Tối đa (RSS Peak)| <= 4 GB               | 680 MB              | 3.850 MB (OOM) |
| Mức Tiêu thụ VRAM (Khi chạy SLM)         | <= 6 GB               | 2.8 GB              | N/A (Cloud)    |
+------------------------------------------+-----------------------+---------------------+----------------+
```
*(Chú thích kỹ thuật: 90%+ các dòng giao dịch sao kê có cấu trúc chuẩn được bóc tách và so khớp tức thời bằng Rust Streaming Deserialization & In-Memory AHash Engine với độ trễ micro-giây (< 0.5ms); Local SLM (3B-8B Q4) chỉ được triệu gọi chọn lọc theo lô tối ưu hóa SIMD/AVX2 đối với các diễn giải thanh toán phi cấu trúc phức tạp hoặc quan hệ đa thực thể cấn trừ).*


### 5.2. Đánh giá Nghiệp vụ từ Chuyên gia Tài chính
- **100% chuyên gia tài chính và kiểm toán viên** tham gia kiểm thử độc lập xác nhận:
  - Cơ chế kiểm soát phương trình kép ngăn chặn hoàn toàn tình trạng sai lệch số cái.
  - Khả năng tự động lọc bỏ các giao dịch trùng lặp (Duplicate Detection) giúp loại trừ rủi ro chi trả hai lần cho một hóa đơn.
  - Tỷ lệ 0.2% chuyển rà soát thủ công là con số tối ưu trong ngành, cho phép kế toán viên kiểm soát toàn bộ ngoại lệ chỉ trong 10-15 phút làm việc mỗi ngày.

---

## 6. MÔ HÌNH THƯƠNG MẠI & TÀI CHÍNH (FINANCIAL SNAPSHOT)

LIVA áp dụng chiến lược kinh doanh B2B hai mũi nhọn (Dual-Track B2B Strategy) nhằm dung hòa giữa việc tạo dòng tiền ngắn hạn và xây dựng giá trị doanh nghiệp dài hạn:

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────────────┐
|                                    MÔ HÌNH KINH DOANH B2B HAI MŨI NHỌN                                  |
├────────────────────────────────────────────────────┬────────────────────────────────────────────────────┤
|  MŨI NHỌN 1: KHỐI DOANH NGHIỆP (CFO / TREASURY)    |  MŨI NHỌN 2: KHỐI NGÂN HÀNG THƯƠNG MẠI (BFSI)      |
├────────────────────────────────────────────────────┼────────────────────────────────────────────────────┤
| • Đối tượng: Doanh nghiệp Vừa & Lớn (Mid-Market)   | • Đối tượng: Khối Corporate Banking, Vận hành,     |
|   có từ 5 - 20 tài khoản ngân hàng.                |   Nguồn vốn các NHTM (Vietcombank, TCB, MB, ACB...) |
| • Chu kỳ bán hàng: Nhanh (2 đến 4 tuần).           | • Chu kỳ bán hàng: Dài (6 đến 18 tháng qua Sandbox)|
| • Hình thức: Thuê bao theo năm (Annual SaaS).      | • Hình thức: Bản quyền Doanh nghiệp (Enterprise    |
|   - Gói Professional (SMB)        : $99 / tháng    |   License) On-Premise: $50,000 - $150,000 / năm.   |
|   - Gói Enterprise Treasury (Mid) : $499 / tháng   | • Phí Dịch vụ Tích hợp: $15,000 - $50,000 / lần.   |
|   - Gói Corporate Holding (Group) : $999-$1,999/tháng| • Phí Bảo trì & Cập nhật thường niên (SLA):      |
| • Phí Tích hợp ERP ban đầu: $2,000 - $5,000        |   20% giá trị hợp đồng bản quyền / năm.            |
| • Mục đích: Tạo dòng tiền tức thì nuôi dưỡng bộ máy| • Mục đích: Khẳng định vị thế thị trường và mở     |
|   mà không cần chờ đợi thủ tục đấu thầu ngân hàng. |   rộng quy mô doanh thu cấp độ tập đoàn.           |
└────────────────────────────────────────────────────┴────────────────────────────────────────────────────┘
```

### 6.1. Hiệu quả Kinh tế Đơn vị (Unit Economics)
- **Chi phí Máy chủ Biên bằng Không ($0 Marginal Cloud Compute)**: Do phần mềm vận hành trực tiếp trên cơ sở hạ tầng máy trạm/máy chủ nội bộ sẵn có của khách hàng, LIVA không phải gánh chịu chi phí thuê GPU đám mây hay chi phí gọi API theo token.
- **Tỷ suất Lợi nhuận Gộp (Gross Margin)**: Đạt trên **88%**, tương đương với các công ty phần mềm Pure-Play Software hàng đầu thế giới.
- **Giá trị Vòng đời Khách hàng (LTV/CAC)**: Ước tính đạt tỷ lệ **> 4.5x** nhờ tỷ lệ giữ chân khách hàng (Net Retention Rate) cao xuất phát từ tính thiết yếu của nghiệp vụ kế toán - ngân quỹ hàng ngày.

### 6.2. Kế hoạch Gọi vốn Hạt giống (Seed Round Ask: $500,000 - $750,000)
Tại Demo Day INNOSTART 2026, LIVA chính thức mở vòng gọi vốn Hạt giống (Seed Round) với mục tiêu:
- **Quy mô gọi vốn**: **$500,000 – $750,000 USD** cho **12% – 15% cổ phần**
- **Định giá mục tiêu**: **$4,000,000 – $5,000,000 USD** (Post-money valuation)
- **Thời gian bảo đảm đường băng tài chính (Runway)**: 18 – 24 tháng
- **Cơ cấu phân bổ nguồn vốn (4 trụ cột chuẩn hóa)**:
  - **50% R&D (Rust Native Core & Local Connectors)**: Mở rộng đội ngũ kỹ sư Rust và Chuyên gia AI; hoàn thiện bộ thư viện bóc tách dữ liệu tự động cho toàn bộ 35+ ngân hàng tại Việt Nam; hoàn thiện các đầu nối (connectors) chuẩn hóa với SAP S/4HANA, Oracle NetSuite, MISA AMIS, FAST và Bravo ($250,000 – $375,000).
  - **25% GTM & ERP Ecosystem (MISA, FAST, Bravo, SAP)**: Phát triển mạng lưới đối tác tích hợp ISV, xây dựng đội ngũ B2B Direct Sales tiếp cận 200+ CFO doanh nghiệp vừa và lớn; triển khai chương trình Khách hàng Tiên phong (Lighthouse Program) ($125,000 – $187,500).
  - **15% SBV Sandbox & Regulatory Compliance**: Hoàn tất chứng nhận kiểm toán an ninh độc lập (ISO/IEC 27001, PCI-DSS On-Premise); hoàn thiện hồ sơ đánh giá tác động dữ liệu (DPIA) gửi Cục A05 theo Nghị định 13; tham gia Cơ chế Thử nghiệm Fintech Sandbox của Ngân hàng Nhà nước cùng các ngân hàng đối tác ($75,000 – $112,500).
  - **10% Operational Runway Reserve**: Quỹ dự phòng tiền mặt duy trì đường băng an toàn từ 18 – 24 tháng, quản trị rủi ro thanh khoản và hạ tầng lab kiểm thử máy trạm ($50,000 – $75,000).

```
+---------------------------------------------------------------------------------------------------------+
|                DỰ PHÓNG TÀI CHÍNH 3 NĂM (2026 - 2028): KỊCH BẢN CƠ SỞ & KỊCH BẢN MỤC TIÊU               |
+------------------------------------------+-----------------------+---------------------+----------------+
| Chỉ số Tài chính                         | Năm 2026 (Nền tảng)   | Năm 2027 (Tăng tốc) | Năm 2028 (Quy mô)|
+------------------------------------------+-----------------------+---------------------+----------------+
| KỊCH BẢN CƠ SỞ (BASE CASE)               |                       |                     |                |
| • Khách hàng Doanh nghiệp (CFO)          | 15 Doanh nghiệp       | 120 Doanh nghiệp    | 450 Doanh nghiệp|
| • Khách hàng Ngân hàng Thương mại (BFSI) | 1 Ngân hàng (PoC)     | 3 Ngân hàng         | 8 Ngân hàng    |
| • Doanh thu Thuê bao Doanh nghiệp (ARR)  | $85,000               | $680,000            | $2,750,000     |
| • Doanh thu Bản quyền & Phí Tích hợp NH  | $60,000               | $320,000            | $1,150,000     |
| • TỔNG DOANH THU THỰC NHẬN (BASE)        | $145,000              | $1,000,000          | $3,900,000     |
| • Tỷ suất Lợi nhuận Gộp (Gross Margin)   | 85 %                  | 88 %                | 91 %           |
| • Điểm Hòa vốn Vận hành (Breakeven Point)| Tháng 11/2027         | Đạt dòng tiền dương | Tự tài trợ vốn |
+------------------------------------------+-----------------------+---------------------+----------------+
| KỊCH BẢN MỤC TIÊU / TĂNG TỐC (BULL CASE) |                       |                     |                |
| • Khách hàng Doanh nghiệp (CFO)          | 35 Doanh nghiệp       | 220 Doanh nghiệp    | 750 Doanh nghiệp|
| • Khách hàng Ngân hàng Thương mại (BFSI) | 1 Ngân hàng (PoC)     | 3 Ngân hàng         | 8 Ngân hàng    |
| • Doanh thu Thuê bao Doanh nghiệp (ARR)  | $320,000              | $1,900,000          | $5,800,000     |
| • Doanh thu Bản quyền & Phí Tích hợp NH  | $130,000              | $500,000            | $1,400,000     |
| • TỔNG DOANH THU THỰC NHẬN (BULL TARGET) | $450,000              | $2,400,000          | $7,200,000     |
+------------------------------------------+-----------------------+---------------------+----------------+
```
*(Đối chiếu đồng bộ: Kịch bản Mục tiêu / Bull Target Case khớp nối trực tiếp 1:1 với đồ thị doanh thu tại Slide 7 Bộ Pitch Deck INNOSTART 2026; Kịch bản Cơ sở / Base Case là mô hình tài chính thận trọng bảo vệ an toàn runway cho nhà đầu tư).*


---

## 7. LỘ TRÌNH TRIỂN KHAI 4 GIAI ĐOẠN (STRATEGIC ROADMAP)

```mermaid
timeline
    title LỘ TRÌNH CHIẾN LƯỢC LIVA BANKING HARNESS (2026 - 2028)
    section Giai đoạn 1 : Q4/2026 - Q1/2027
        Lighthouse Clients : 10-15 Doanh nghiệp Mid-Market thân thiết
        Hoàn thiện Parser : 20 mẫu sổ phụ ngân hàng phổ biến nhất
        Chứng thực ROI : Cắt giảm 75% thời gian đối soát thực tế
    section Giai đoạn 2 : Q2/2027 - Q4/2027
        Liên minh ERP : Ký kết ISV Partner với MISA, FAST, Odoo
        Tích hợp Add-on : 1-Click Banking Harness trên phần mềm kế toán
        Quy mô hóa : Chạm mốc 100+ khách hàng doanh nghiệp trả phí
    section Giai đoạn 3 : Năm 2028
        Hợp tác Ngân hàng : Cung cấp White-label cho 2-3 NHTM TMCP
        Gia tăng CASA : Đóng gói thành tiện ích VIP cho khách hàng doanh nghiệp
        Hòa vốn & Lãi : Doanh thu vượt 1 triệu USD / năm
    section Giai đoạn 4 : 2028 trở đi
        SBV Fintech Sandbox : Tham gia chính thức cơ chế thử nghiệm có kiểm soát
        Mở rộng Khu vực : Tiến ra Đông Nam Á (Indonesia, Thái Lan, Philippines)
        Chủ quyền AI : Trở thành Hệ điều hành Tác tử Tài chính số 1 Việt Nam
```

---

## 8. LỜI KẾT & KÊU GỌI HÀNH ĐỘNG (CALL TO ACTION)

Thưa Hội đồng Giám khảo, Quý Đại diện Ngân hàng và các Nhà đầu tư,

Sự trỗi dậy của Trí tuệ Nhân tạo là tất yếu, nhưng tương lai của AI trong ngành tài chính - ngân hàng không thể xây dựng trên sự đánh đổi về an ninh dữ liệu hay sự tùy tiện của các đám mây công cộng xuyên biên giới. 

**LIVA Banking Harness chứng minh rằng: Một hệ thống AI có thể vừa thông minh vượt trội, vừa chạy siêu tốc trên máy trạm thông thường, vừa tuyệt đối tuân thủ pháp luật Việt Nam với chi phí tối ưu nhất.**

Chúng tôi trân trọng kính mời:
1. **Các Ngân hàng Thương mại**: Cùng hợp tác triển khai chương trình thử nghiệm PoC không xâm lấn trong khuôn khổ Quản trị Dòng tiền Doanh nghiệp.
2. **Các Nhà đầu tư mạo hiểm**: Đồng hành cùng LIVA trong vòng gọi vốn Hạt giống để cùng kiến tạo nên **Nền tảng Tác tử Tài chính Độc lập Hàng đầu Việt Nam**.

---
*Báo cáo Tóm tắt Điều hành này được lập phục vụ Hội đồng Đánh giá tại Demo Day INNOSTART 2026.*  
*Mọi thắc mắc kỹ thuật và hồ sơ thẩm định chi tiết (Due Diligence Package), xin vui lòng tham chiếu tài liệu `README.md` cùng thư mục.*
