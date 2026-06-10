# BÁO CÁO NGHIÊN CỨU TỐI ƯU HÓA BỘ NHỚ RAM / VRAM HỆ THỐNG LIVA
*LIVA RAM/VRAM System Memory Optimization Report*

> DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

---

## TÓM TẮT DỰ ÁN (EXECUTIVE SUMMARY)
Hệ thống LIVA là một nền tảng trợ lý thông minh đa tác vụ, kết hợp các mô hình ngôn ngữ lớn (LLM) nội cục (Local LLM) với giao diện tương tác 3D WebGL Avatar thời gian thực. Báo cáo nghiên cứu này phân tích kiến trúc quản lý tài nguyên bộ nhớ kép (RAM và VRAM) trong dự án LIVA. Chúng tôi tập trung vào việc làm rõ các cơ chế tráo đổi nóng tuần tự (Sequential Hot-Swapping), cướp quyền ưu tiên bộ nhớ (Preemptive Memory Management), nén ngữ cảnh (Context Compression) ở gateway, tối ưu hóa vòng lặp kết xuất đồ họa (WebGL Rendering Loop) tại UI, phân bổ lưu trữ SQLite và các chiến dịch tối ưu hóa đệm (Caching).

Đồng thời, báo cáo cung cấp bản thiết kế hệ thống giám sát tài nguyên tự động (`TelemetryProfiler`) cùng các cấu hình thực tế cho ba phân cấp phần cứng khác nhau nhằm đảm bảo tính ổn định tối đa của hệ thống LIVA.

---

## 1. PHÂN TÍCH TÀI NGUYÊN VRAM (VRAM FOOTPRINT ANALYSIS)

### 1.1. Giao thức Tráo đổi Tuần tự (Sequential Hot-Swap Flow)
Trong các hệ thống phần cứng cá nhân, tài nguyên VRAM thường bị giới hạn. LIVA giải quyết vấn đề này bằng cách thiết lập ràng buộc: **Chỉ cho phép tối đa một mô hình LLM lớn hoạt động trên VRAM tại một thời điểm**.

Quy trình tráo đổi mô hình được điều phối bởi lớp `ModelOrchestrator` (`liva-gateway/src/core/ModelOrchestrator.ts`). Hệ thống định nghĩa hai mô hình chính:
1. **Router Model (Gemma 4 E4B)**: Chiếm dụng ~1.5 GB VRAM, đóng vai trò định tuyến ý định, lựa chọn công cụ (Tool selection) và phân tích cảm xúc nhanh.
2. **Expert Model (Gemma 4 12B QAT 4-bit)**: Chiếm dụng ~6.7 GB VRAM (bao gồm ~6.0 GB cho weights lượng hóa và ~0.7 GB cho KV cache ở ngữ cảnh tiêu chuẩn), đảm nhiệm các tác vụ suy luận sâu, xử lý logic phức tạp.

#### Quy trình tráo đổi nóng (Hot-Swap Lifecycle):
1. **Yêu cầu đến (Incoming Request)**: Khi nhận được yêu cầu cần mô hình Expert, `ModelOrchestrator` sẽ kiểm tra mô hình hiện tại đang nạp trên VRAM.
2. **Giải phóng mô hình cũ (Unload)**: Nếu mô hình hiện tại là Router, orchestrator sẽ gửi tín hiệu kết thúc tiến trình tới instance `llama-server` tương ứng của Router, giải phóng toàn bộ vùng nhớ VRAM mà nó đang giữ thông qua lệnh hủy tiến trình hệ thống.
3. **Nạp mô hình mới (Load & Warm-up)**: Tiến trình `llama-server` dành cho mô hình Expert được khởi chạy với các cấu hình định sẵn. Hệ thống sẽ đợi cổng HTTP của server này sẵn sàng (Warm-up).
4. **Cơ chế trì hoãn hạ nhiệt (Cooldown Timer - EXPERT_COOLDOWN_MS)**:
   Để tránh hiện tượng tráo đổi liên tục (thrashing) gây suy giảm nghiêm trọng hiệu năng do thời gian khởi động mô hình lớn, `ModelOrchestrator` áp dụng một bộ hẹn giờ duy trì `EXPERT_COOLDOWN_MS` có giá trị mặc định là `90,000 ms` (1.5 phút).
   - Sau khi mô hình Expert hoàn thành xử lý một yêu cầu, nó sẽ không bị dỡ bỏ ngay lập tức mà đi vào trạng thái chờ (Cooldown).
   - Nếu có yêu cầu mới liên quan đến Expert trong vòng 60 giây, bộ đếm thời gian sẽ được làm mới.
   - Khi bộ hẹn giờ kết thúc mà không có yêu cầu Expert nào mới, hệ thống tự động dỡ bỏ Expert và nạp lại Router để trả lại VRAM trống cho hệ điều hành và giao diện WebGL.

---

### 1.2. Mô hình Cướp quyền và Hạ cấp VRAM (Preemption & Graduated Degradation Model)
Để đảm bảo hệ thống không bị đổ vỡ do lỗi thiếu bộ nhớ GPU (CUDA Out-of-Memory - OOM), LIVA triển khai mô hình quản lý VRAM chủ động qua bộ khóa ưu tiên `PreemptiveVramMutex` (`liva-gateway/src/core/PreemptiveVramMutex.ts`) kết hợp ước lượng dung lượng tại `VramCostEstimator.ts`.

#### Kịch bản Hạ cấp 3 bước (3-Step Graduated Degradation):
Khi phát hiện bộ nhớ VRAM khả dụng tụt xuống dưới ngưỡng an toàn, hệ thống thực hiện hạ cấp tuần tự theo các mức:

1. **Mức 1: Eco Mode (Chế độ Tiết kiệm)**
   - Kích hoạt cờ hệ thống `LIVA_ECO_MODE`.
   - Giảm tần suất cập nhật của avatar xuống còn 5 FPS (chu kỳ kiểm tra 200ms).
   - Rút ngắn độ dài cửa sổ ngữ cảnh LLM để giảm kích thước KV cache.
2. **Mức 2: Freeze Mode (Chế độ Đóng băng)**
   - Thiết lập `LIVA_AVATAR_DEMOTE_LEVEL = 'freeze'`.
   - Vòng lặp kết xuất của UI (`use3DModel.ts`) sẽ bỏ qua hoàn toàn việc dựng hình (0 FPS), giải phóng băng thông GPU và đóng băng trạng thái của mô hình 3D trên màn hình.
3. **Mức 3: Preempt Mode (Chế độ Thu hồi Triệt để)**
   - Hệ thống thực hiện cướp quyền ưu tiên (Preemption). Các tiến trình LLM có mức ưu tiên thấp hơn (ví dụ: tác vụ Consolidation chạy ngầm hoặc tác vụ nghiên cứu phụ) sẽ bị chấm dứt cưỡng bức.
   - Dỡ bỏ mô hình Expert ra khỏi VRAM ngay lập tức bất kể thời gian Cooldown chưa hết, chuyển toàn bộ luồng xử lý về mô hình Router gọn nhẹ.

#### Cơ chế Ngắt mạch (Circuit Breaker):
Nếu sau khi thực hiện cả 3 bước hạ cấp mà dung lượng VRAM vẫn vượt ngưỡng giới hạn tối đa hoặc hệ thống phát hiện lỗi phân bổ bộ nhớ liên tục, phương thức `triggerCircuitBreaker` trong `PreemptiveVramMutex.ts` sẽ được kích hoạt. Lỗi ngắt mạch này sẽ tạm thời dừng tất cả các yêu cầu suy luận LLM mới, hiển thị thông báo lỗi hệ thống tạm thời bận để ngăn chặn sự cố sập driver đồ họa hoặc sập nhân hệ thống (kernel panic).

---

### 1.3. Cấu hình Tham số llama-server (llama-server Parameter Tuning)
Việc tối ưu hóa tham số dòng lệnh khởi chạy `llama-server` đóng vai trò quyết định đến lượng VRAM tĩnh và động mà mô hình chiếm giữ.

*   `--n-gpu-layers` (`n_gpu_layers`):
    - Đối với Router (Gemma 4 E4B): Thiết lập offload 100% các lớp (layers) lên GPU nhằm đạt tốc độ phản hồi nhanh nhất.
    - Đối với Expert (Gemma 4 12B QAT 4-bit): Số lượng lớp offload được tính toán động dựa trên lượng VRAM trống khả dụng. Trên các card đồ họa từ 12GB VRAM trở lên (Tier 2), mô hình có thể được offload hoàn toàn (100%) lên GPU. Trên các cấu hình thấp hơn (ví dụ: 6GB/8GB VRAM), hệ thống sẽ offload một phần các lớp lên GPU, số còn lại đẩy về CPU để tránh lỗi Out-Of-Memory (OOM).
*   `--ctx-size` (`n_ctx`): Giới hạn kích thước ngữ cảnh tối đa. Ví dụ: Router giới hạn ở `4096` tokens và Expert giới hạn ở `8192` tokens để kiểm soát sự phình to của KV Cache.
*   `--flash-attn` (`flash_attn`):
    Kích hoạt thuật toán Flash Attention. Đây là cấu hình bắt buộc để giảm độ phức tạp bộ nhớ của KV Cache từ $O(N^2)$ xuống $O(N)$ (tuyến tính với độ dài chuỗi đầu vào), giúp tiết kiệm tới 40% VRAM động khi xử lý các chuỗi hội thoại dài.

---

### 1.4. Quản lý KV Cache và Nén Ngữ cảnh (KV Cache & Context Compression)
KV Cache đóng vai trò lưu trữ các khóa (keys) và giá trị (values) của các token đã xử lý trước đó để tăng tốc độ sinh từ tiếp theo. Tuy nhiên, nó tiêu tốn VRAM rất lớn. LIVA giải quyết vấn đề này qua lớp xử lý `TokenCompressionService.ts` và bộ xây dựng prompt `PromptBuilder.ts`.

#### Quy trình Nén Ngữ cảnh 4 Giai đoạn (4-Stage Token Compression Pipeline):
Trước khi đưa lịch sử hội thoại vào LLM, dữ liệu được nén qua 4 bước nhằm giảm thiểu số lượng token đầu vào:

```
[Lịch sử Hội thoại gốc]
          │
          ▼
┌──────────────────┐
│ 1. Structural    │ ──► Loại bỏ cấu trúc Markdown, HTML, XML tags dư thừa.
│    Strip         │
└──────────────────┘
          │
          ▼
┌──────────────────┐
│ 2. JSON/XML      │ ──► Rút gọn khoảng trắng, định dạng thu gọn trong chuỗi dữ liệu.
│    Condense      │
└──────────────────┘
          │
          ▼
┌──────────────────┐
│ 3. Sentence      │ ──► Loại bỏ các câu trùng lặp ngữ nghĩa hoặc có mức độ tương
│    Deduplication │     đồng ngữ nghĩa quá cao.
└──────────────────┘
          │
          ▼
┌──────────────────┐
│ 4. Budget        │ ──► Áp dụng bộ ước lượng Token chủ động để cắt tỉa văn bản
│    Enforcement   │     đảm bảo vừa vặn với dung lượng quy định (Budget).
└──────────────────┘
          │
          ▼
[Dữ liệu nén đưa vào LLM]
```

1.  **Giai đoạn 1: Structural Strip (Tước bỏ Cấu trúc)**: Loại bỏ các thẻ định dạng văn bản thừa, Markdown không cần thiết, HTML/XML tags rườm rà nhưng vẫn giữ lại nội dung cốt lõi của thông điệp.
2.  **Giai đoạn 2: JSON/XML Condense (Nén cấu trúc dữ liệu)**: Chuyển đổi các chuỗi dữ liệu cấu trúc phức tạp về dạng rút gọn nhất, giảm khoảng trắng và dấu xuống dòng.
3.  **Giai đoạn 3: Sentence Deduplication (Loại bỏ câu trùng lặp)**: Quét qua văn bản và loại bỏ các câu có nội dung lặp lại về mặt ngữ nghĩa thông qua so khớp ký tự hoặc ngữ nghĩa cơ bản.
4.  **Giai đoạn 4: Budget Enforcement (Thực thi ngân sách token)**: Sử dụng phương thức `estimateTokens` dựa trên heuristic độ dài chuỗi để cắt tỉa (truncation) văn bản từ phía xa nhất của lịch sử hội thoại nhằm đảm bảo tổng số lượng token nằm trong giới hạn cho phép.

#### Cơ chế Trượt Ngữ cảnh (Context Sliding Strategy - PromptBuilder.ts):
`PromptBuilder.ts` triển khai phân chia độ ưu tiên của dữ liệu đưa vào ngữ cảnh:
-   **Độ ưu tiên 1 (System Prompt & Tools)**: Luôn được giữ nguyên vẹn (chiếm khoảng 2500 tokens).
-   **Độ ưu tiên 2 (RAG Chunks - Hệ thống tìm kiếm tài liệu)**: Được chèn vào dưới dạng `[Recalled Context]` và được xếp thứ tự lại bằng thuật toán `longContextReorder` để đặt các thông tin quan trọng nhất ở đầu và cuối ngữ cảnh (tránh hiện tượng "lost in the middle").
-   **Độ ưu tiên 3 (Lịch sử hội thoại - Chat History)**: Sẽ bị cắt tỉa đầu tiên khi tổng dung lượng vượt quá giới hạn (chỉ giữ lại lượt hội thoại gần nhất để bảo vệ luồng tương tác tức thời).

---

### 1.5. Cơ chế Tối ưu VRAM Kết xuất Đồ họa 3D (WebGL Render Loop & Avatar Lifecycle)
Phần giao diện người dùng của LIVA sử dụng Three.js để hiển thị mô hình nhân vật 3D VRM/FBX thông qua composable `use3DModel.ts` (`liva-ui/src/composables/use3DModel.ts`). Để tránh rò rỉ bộ nhớ VRAM trên trình duyệt khi tráo đổi mô hình nhân vật hoặc đóng ứng dụng, các cơ chế dọn dẹp sâu đã được xây dựng.

#### Quy trình Giải phóng Tài nguyên Sâu (Deep Disposal Lifecycle):
Khi thay đổi hoặc dỡ bỏ mô hình 3D, phương thức `deepDispose` sẽ duyệt đệ quy qua toàn bộ đồ thị thực thể (scene graph):
-   **Geometry (Hình học)**: Gọi `object.geometry.dispose()` để giải phóng bộ đệm đỉnh (vertex buffers) trên GPU.
-   **Material (Chất liệu) & Textures (Kết cấu)**: Duyệt qua tất cả các thuộc tính của chất liệu. Nếu thuộc tính đó là một texture (kết cấu ảnh), hệ thống sẽ gọi `texture.dispose()` để giải phóng dung lượng bộ nhớ kết cấu trên GPU trước khi gọi `material.dispose()`.
-   **Skeleton (Bộ xương)**: Gọi `object.skeleton.dispose()` để giải phóng ma trận chuyển động của các khớp xương khớp.
-   **Renderer (Bộ dựng hình WebGL)**: Gọi `renderer.dispose()` và `renderer.forceContextLoss()`. Lệnh `forceContextLoss()` đặc biệt quan trọng vì nó buộc trình duyệt giải phóng ngay lập tức toàn bộ tài nguyên ngữ cảnh WebGL liên kết với GPU mà không cần đợi trình thu gom rác của trình duyệt kích hoạt.

#### Điều chỉnh Tần suất Kết xuất (Render Loop Throttling):
Tần suất dựng hình (FPS) của vòng lặp WebGL được điều tiết tự động dựa trên trạng thái hoạt động và khả năng hiển thị của cửa sổ ứng dụng:
-   **Chế độ thông thường (Active Window)**: Hoạt động ở tốc độ quét màn hình tối đa (thường là 60 FPS) để đảm bảo chuyển động mượt mà.
-   **Chế độ chạy ngầm (Hidden/Background Window)**: Khi nhận sự kiện `visibilitychange` báo hiệu tab trình duyệt bị ẩn, hệ thống hạ tốc độ kết xuất xuống **15 FPS** (chu kỳ kiểm tra 66ms).
-   **Chế độ Tiết kiệm VRAM (LIVA_ECO_MODE)**: Tốc độ kết xuất được hạ xuống **5 FPS** (chu kỳ kiểm tra 200ms).
-   **Chế độ Đóng băng/Thu hồi (Freeze/Preempted)**: Tốc độ kết xuất hạ xuống **0 FPS** (ngừng gọi `requestAnimationFrame`), loại bỏ hoàn toàn việc sử dụng GPU của WebGL.

#### Xử lý Chuyển động Mượt mà (Spring-Damped LookAt Damping):
Để chuyển động xoay đầu và mắt của nhân vật theo dõi người dùng không bị giật cục, hệ thống áp dụng bộ lọc giảm chấn lò xo (Spring-damped interpolation):
$$\theta_{\text{current}} = \theta_{\text{current}} + (\theta_{\text{target}} - \theta_{\text{current}}) \cdot f_{\text{spring}}$$
Trong đó hệ số kéo lò xo là $f_{\text{spring}} = 1 - 0.001^{\Delta t}$. Cơ chế này giúp triệt tiêu hiện tượng di chuyển giật cục của xương mắt/đầu nhân vật khi camera gửi dữ liệu yaw/pitch không liên tục.

#### Đồng bộ hóa Khẩu hình dựa trên Tần số Âm thanh (Audio-Driven Lip-Sync):
Mouth expressions của nhân vật 3D được đồng bộ hóa với giọng nói đầu ra thông qua bộ phân tích tần số thời gian thực (`AnalyserNode` với `fftSize = 256`). Năng lượng âm thanh được chia làm 5 băng tần chính:
1.  **Băng tần 0 (bins 0-3)**: Âm trầm hạ âm (Sub-bass), điều khiển khẩu hình âm **'aa'** (mở miệng rộng).
2.  **Băng tần 1 (bins 4-8)**: Âm trung trầm (Low-mid), điều khiển khẩu hình âm **'oh'** (môi tròn).
3.  **Băng tần 2 (bins 9-16)**: Âm trung (Mid), điều khiển khẩu hình âm **'ee'** (môi dẹt ngang).
4.  **Băng tần 3 (bins 17-32)**: Âm trung cao (Upper-mid), điều khiển khẩu hình âm **'ih'** (hơi mở nhẹ).
5.  **Băng tần 4 (bins 33-64)**: Âm cao (High), điều khiển khẩu hình âm **'ou'** (môi thu nhỏ).

Quy trình tính toán sử dụng giá trị căn trung bình bình phương (Root Mean Square - RMS) của các bin tần số:
$$x_{\text{RMS}} = \sqrt{\frac{1}{N} \sum_{i=0}^{N-1} x_i^2}$$
Hệ thống áp dụng một vùng chết `RMS_DEAD_ZONE` có giá trị `0.05` để lọc bỏ các tạp âm nhỏ trong môi trường tĩnh lặng trước khi áp dụng hệ số nội suy làm mượt `RMS_SMOOTH_FACTOR = 0.3` giúp khẩu hình chuyển động tự nhiên, không bị rung giật.

---

## 2. PHÂN TÍCH TÀI NGUYÊN RAM (RAM MEMORY USAGE ANALYSIS)

### 2.1. Phân bổ Bộ nhớ Gateway Node.js (Node.js Gateway Footprint)
Cổng kết nối LIVA Gateway hoạt động trên nền tảng Node.js. Các luồng dữ liệu thô từ API, các đối tượng biểu diễn ngữ cảnh hội thoại, và các tài nguyên MCP (Model Context Protocol) được quản lý chặt chẽ để tối ưu hóa Garbage Collection (GC) của công cụ V8:
-   Sử dụng truyền phát dữ liệu (streaming) thay vì đọc toàn bộ file vào bộ nhớ.
-   Hạn chế tối đa việc tạo các biến tạm thời trong các vòng lặp xử lý hội thoại tần suất cao nhằm giảm áp lực dọn rác của V8, tránh hiện tượng dừng hệ thống do GC thực hiện tác vụ dọn dẹp quá lâu (GC pauses).

---

### 2.2. Chiến lược Quản lý Bộ nhớ Đệm (Caching Strategy)
Để giảm thiểu tối đa việc truy xuất ổ đĩa vật lý (Disk I/O) và tính toán lại các vector nhúng (embedding), LIVA áp dụng một hệ thống bộ nhớ đệm nhiều tầng sử dụng thư viện `lru-cache`:

| Tên Cache | Vị trí định nghĩa | Dung lượng (Max) | Thời gian sống (TTL) | Mục đích & Đặc thù |
| :--- | :--- | :--- | :--- | :--- |
| `memCache` | `MemoryManager.ts` | 50 tin nhắn | Vĩnh viễn (trong phiên) | Lưu lịch sử chat tức thời trên RAM. Vượt quá 50 sẽ tự động cắt tỉa còn 30, chuyển phần dư sang `ReflectionDaemon` để đồng hóa ngầm. |
| `hybridCache` | `MemoryManager.ts` | 50 đối tượng | 5 phút | Bộ đệm kết quả tìm kiếm ngữ nghĩa (RAG). Bỏ qua tìm kiếm vector nếu câu hỏi trùng lặp trong thời gian ngắn (L0.5 Cache). |
| `profileCache` | `MemoryManager.ts` | 1 đối tượng | 1 phút | Lưu thông tin cấu hình người dùng (`user_profile.json`). Áp dụng cơ chế **Stale-While-Revalidate (SWR)**: trả về dữ liệu cũ ngay lập tức (0ms) và kích hoạt đọc đĩa cập nhật ngầm. |
| `descEmbeddingCache` | `SkillRegistry.ts` | 100 thực thể | 1 giờ | Bộ đệm chứa các vector nhúng của mô tả công cụ MCP, ngăn việc tính lại embedding của 33 công cụ trên mỗi lượt nhập của người dùng. |
| `#promptCache` | `PromptBuilder.ts` | 100 thực thể | 5 phút | Lưu trữ các prompt đã được đóng gói và ký số (sealed prompts) để tránh việc ghép chuỗi và tính toán lại tham số hệ thống liên tục. |
| `SemanticCache` | `SemanticCache.ts` | 500 thực thể | 24 giờ | Bộ đệm ngữ nghĩa cho các câu lệnh ngắn gọn của người dùng. Áp dụng so khớp chính xác (Exact match) kết hợp so khớp mờ (Fuzzy Levenshtein similarity $\ge 0.95$). |

---

### 2.3. Tối ưu hóa Hiệu năng SQLite và sqlite-vec (SQLite Database Performance Tuning)
LIVA sử dụng SQLite làm công cụ lưu trữ dữ liệu có cấu trúc chính thông qua API đồng bộ mới `node:sqlite` và thư viện tìm kiếm vector `sqlite-vec`. Các tham số hệ thống được tinh chỉnh để đạt hiệu năng tối đa:

*   `PRAGMA journal_mode = WAL`: Kích hoạt chế độ ghi nhật ký trước (Write-Ahead Logging). Chế độ này tách biệt tiến trình đọc và ghi, cho phép nhiều luồng đọc hoạt động đồng thời ngay cả khi có luồng đang ghi dữ liệu vào cơ sở dữ liệu.
*   `PRAGMA synchronous = NORMAL`: Giảm mức độ đồng bộ hóa đĩa vật lý từ chế độ FULL xuống NORMAL. Trong chế độ WAL, điều này hoàn toàn an toàn và giúp tăng tốc độ ghi đĩa đáng kể do giảm thiểu số lần gọi lệnh đồng bộ hóa đĩa hệ thống.
*   `PRAGMA busy_timeout = 5000`: Đặt thời gian chờ giải phóng khóa là 5 giây. Ngăn chặn lỗi đổ vỡ `SQLITE_BUSY` khi có xung đột tài nguyên ghi giữa luồng chính và luồng phụ.
*   `PRAGMA wal_autocheckpoint = 500`: Đặt ngưỡng checkpoint WAL nhỏ (500 trang). Giúp file `-wal` không bị phình to quá mức, đảm bảo thời gian phục hồi nhanh khi khởi động lạnh hệ thống.
*   `PRAGMA cache_size = -8192`: Phân bổ bộ nhớ đệm trang SQLite lên tới 8MB (giá trị âm biểu thị Kilobytes, tương ứng $8192 \times 1024$ bytes). Giúp tăng tỷ lệ truy cập dữ liệu trực tiếp trên RAM, giảm thiểu đọc ghi đĩa.
*   `PRAGMA page_size = 32768`: Sử dụng kích thước trang lưu trữ lớn 32KB để tối ưu hóa việc phân bổ không gian đĩa cho các bản ghi chứa dữ liệu nhúng vector.
*   `PRAGMA mmap_size = 268435456`: Cấu hình 256MB bộ nhớ ánh xạ trực tiếp (Memory-Mapped I/O). Việc ánh xạ toàn bộ tệp cơ sở dữ liệu (thường < 50MB) vào không gian địa chỉ ảo giúp loại bỏ hoàn toàn chi phí hệ điều hành gọi các hàm hệ thống `read` và `write`.

---

### 2.4. Cơ chế Bảo vệ I/O và Đệm Ghi (I/O Shielding & Write Buffering)

#### Chống Ghi Đè Đồng Thời (WAL Atomic Write & safeRename):
Để ngăn ngừa mất dữ liệu hoặc hỏng file cấu hình khi xảy ra sự cố mất điện đột ngột hoặc xung đột tiến trình, LIVA áp dụng quy tắc ghi đè nguyên tử (Atomic Write thông qua cơ chế ghi file tạm `.tmp` và đổi tên):
1.  Dữ liệu mới được ghi vào một tệp tạm thời `.tmp` trên cùng một phân vùng ổ đĩa.
2.  Sau khi ghi thành công, hệ thống gọi hàm `safeRename` (`liva-gateway/src/utils/FileUtils.ts`) để thực hiện đổi tên tệp tạm đè lên tệp gốc.
3.  Hàm `safeRename` tích hợp cơ chế tự phục hồi lỗi khóa file trên hệ điều hành (chống lại sự can thiệp tạm thời của các phần mềm như Windows Defender hoặc tiến trình quét hệ thống) bằng cách thử lại tối đa 3 lần với thuật toán giãn cách lũy thừa (Exponential Backoff):
    $$\text{delay} = \text{base\_delay} \cdot 2^{\text{attempt} - 1}$$
    Với $\text{base\_delay} = 50\text{ms}$.

#### Bộ đệm Tương tác (Fact Touch Buffer & Debounced Writes):
Các thao tác truy cập dữ liệu kiến thức (Facts) trong bộ nhớ L3 sẽ kích hoạt cập nhật độ bền nhớ (memory strength) và thời gian truy cập gần nhất (spaced repetition). Thay vì thực thi câu lệnh SQL `UPDATE` trực tiếp trên đĩa cho mỗi lượt truy vấn (gây suy giảm tuổi thọ SSD và nghẽn I/O), LIVA triển khai bộ đệm ghi ngầm:
-   **Fact Touch Buffer**: Lưu trữ các sự kiện truy vấn fact vào bộ nhớ tạm thời trên RAM (`#factTouchBuffer`).
-   Bộ đếm thời gian `FACT_TOUCH_FLUSH_MS = 60,000 ms` (1 phút) sẽ thực hiện gom nhóm toàn bộ các sự kiện truy cập và thực thi cập nhật đồng loạt dưới dạng một khối Batch duy nhất qua luồng phụ.
-   Khi hệ thống tắt (`close()`), bộ đệm này sẽ được ép buộc xả (flush) hoàn toàn trước khi đóng kết nối SQLite để tránh thất thoát dữ liệu.

---

### 2.5. Vòng đời Luồng Phụ Cơ sở Dữ liệu (Database Worker Thread Lifecycle)
Để đảm bảo luồng chính (Main Thread) của Node.js luôn đạt trạng thái phản hồi tức thời dưới 16ms (đáp ứng giao diện người dùng mượt mà), LIVA cô lập toàn bộ các tác vụ truy vấn SQL đồng bộ và các phép toán so sánh vector nhúng sang một luồng phụ chuyên dụng (`liva-gateway/src/workers/DatabaseWorker.ts`) thông qua lớp cầu nối `DatabaseWorkerBridge.ts`.

#### Kiến trúc Cầu nối và Đồng bộ hóa:
-   `DatabaseWorkerBridge` khởi tạo một thực thể `Worker` của Node.js chạy mã nguồn `DatabaseWorker`.
-   Các câu lệnh SQL được gửi qua kênh truyền tin nhắn dưới dạng các Job kèm theo một định danh duy nhất (UUID) và mã định danh vết `traceId`.
-   Luồng chính nhận về một `Promise` và sẽ giải phóng Event Loop để tiếp tục xử lý các yêu cầu khác. Khi luồng phụ hoàn thành, nó gửi tin nhắn phản hồi để giải quyết (resolve) Promise tương ứng.

#### Bộ giám sát trạng thái Luồng phụ (Watchdog & Deadlock Recovery):
-   Một bộ giám sát chạy chu kỳ `WATCHDOG_PING_MS = 10,000 ms` (10 giây) sẽ gửi tin nhắn `ping` tới luồng phụ.
-   Nếu luồng phụ không phản hồi (Pong) trong vòng `25,000 ms` (25 giây), hệ thống xác định luồng phụ đã rơi vào trạng thái nghẽn chết (Deadlock - ví dụ do tính toán khoảng cách vector quá tải hoặc file DB bị khóa cứng).
-   `DatabaseWorkerBridge` sẽ lập tức tiêu diệt (terminate) tiến trình phụ bị nghẽn và khởi tạo lại một luồng phụ mới.
-   Để tránh vòng lặp khởi động lại vô hạn khi file cơ sở dữ liệu bị hỏng vật lý, hệ thống giới hạn tối đa `MAX_RECOVERY_ATTEMPTS = 3` lần thử lại.

#### Cơ chế Tự phục hồi Cơ sở dữ liệu (Self-Healing Recovery):
Khi luồng phụ khởi chạy hoặc phục hồi, nó sẽ thực hiện kiểm tra tính toàn vẹn:
1.  Thực thi lệnh kiểm tra `PRAGMA integrity_check`.
2.  Nếu phát hiện tệp dữ liệu chính bị lỗi cấu trúc (corruption):
    - Đóng kết nối lỗi.
    - Sao chép tệp sao lưu dự phòng `<dbPath>.bak` (được tạo trước đó qua câu lệnh an toàn `VACUUM INTO` trong các chu kỳ Consolidation thành công) ghi đè lên tệp chính.
    - Xóa bỏ các tệp tin phụ `-shm` và `-wal` đi kèm để loại bỏ xung đột đồng bộ hóa của WAL.
    - Mở lại tệp cơ sở dữ liệu đã khôi phục và tiếp tục vận hành.

---

### 2.6. Ảnh hưởng của Kiến trúc Bộ nhớ Thống nhất (Unified Memory - Apple Silicon)
Trên các hệ máy Mac sử dụng chip Apple Silicon (M1/M2/M3/M4), CPU và GPU chia sẻ chung một không gian bộ nhớ vật lý thống nhất (Unified Memory).
-   **Đặc điểm**: Băng thông truyền dẫn cực lớn, không mất thời gian sao chép dữ liệu qua bus PCIe giữa bộ nhớ RAM của hệ thống và bộ nhớ VRAM của card đồ họa.
-   **Nguy cơ nghẽn ảo (Virtual Memory Swap Thrashing)**:
    Khi tổng dung lượng bộ nhớ của ứng dụng LIVA (bao gồm RAM của Gateway Node.js, VRAM của hai mô hình LLM nạp qua llama-server, tài nguyên dựng hình WebGL của trình duyệt) vượt quá dung lượng bộ nhớ vật lý (ví dụ: máy Mac cấu hình cơ bản 8GB hoặc 16GB), macOS sẽ kích hoạt cơ chế swap dữ liệu từ bộ nhớ xuống ổ cứng SSD.
    Do tốc độ đọc ghi của SSD (dù là NVMe tốc độ cao) vẫn chậm hơn hàng chục lần so với băng thông của Unified Memory bus, hiện tượng tráo đổi liên tục này sẽ làm tốc độ sinh từ của LLM giảm mạnh (từ 30 token/giây xuống dưới 1 token/giây) và làm giật lag giao diện WebGL Avatar.
-   **Giải pháp của LIVA**: Việc thực thi nghiêm ngặt cơ chế tráo đổi nóng tuần tự (Hot-Swapping) để chỉ giữ một mô hình LLM trên bộ nhớ và hạ cấp WebGL xuống 0 FPS ở chế độ Freeze là bắt buộc để đảm bảo tổng lượng sử dụng RAM/VRAM vật lý luôn nằm dưới ngưỡng giới hạn vật lý của máy Mac.

---

## 3. THIẾT KẾ CÔNG CỤ ĐO LƯỜNG VÀ ĐÁNH GIÁ (PROFILING TOOL DESIGN)

Dưới đây là mã nguồn TypeScript hoàn chỉnh và có khả năng biên dịch dành cho lớp `TelemetryProfiler`. Công cụ này thực hiện nhiệm vụ đo đạc tài nguyên hệ thống theo thời gian thực, ước lượng bộ nhớ đồ họa dựa trên các mô hình đang nạp và liên kết các thông số này với trạng thái hoạt động của vòng lặp tác vụ Agent (Agent Loop States).

```typescript
import * as os from 'node:os';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';

// Định nghĩa các trạng thái hoạt động của Agent Loop
export type AgentLoopState = 'IDLE' | 'THINKING' | 'SWAPPING' | 'CONSOLIDATION' | 'RENDERING';

export interface MemorySnapshot {
    timestamp: number;
    state: AgentLoopState;
    ram: {
        rss: number;          // Resident Set Size (Bộ nhớ thực tế Node.js chiếm dụng)
        heapTotal: number;    // Tổng dung lượng Heap được cấp phát
        heapUsed: number;     // Dung lượng Heap thực tế đang sử dụng
        external: number;     // Bộ nhớ C++ nằm ngoài Heap quản lý bởi V8
        systemTotal: number;  // Tổng RAM vật lý của hệ thống
        systemFree: number;   // RAM hệ thống còn trống
    };
    vram: {
        estimatedTotal: number; // Ước lượng tổng lượng VRAM ứng dụng chiếm dụng (MB)
        routerFootprint: number;// VRAM tĩnh ước tính cho mô hình Router (MB)
        expertFootprint: number;// VRAM tĩnh ước tính cho mô hình Expert (MB)
        avatarFootprint: number;// VRAM tĩnh ước tính cho WebGL Avatar (MB)
        limit: number;          // Giới hạn an toàn VRAM được cấu hình (MB)
    };
}

export interface ProfilerConfig {
    vramLimitMB: number;
    ramWarningThresholdBytes: number;
    logDir: string;
    sampleIntervalMS: number;
}

export class TelemetryProfiler {
    private config: ProfilerConfig;
    private logFilePath: string;
    private activeState: AgentLoopState = 'IDLE';
    private intervalId: NodeJS.Timeout | null = null;
    private memoryHistory: MemorySnapshot[] = [];
    private isExpertLoaded: boolean = false;
    private isRouterLoaded: boolean = false;

    // Các hằng số footprint tĩnh từ VramCostEstimator
    private readonly ROUTER_VRAM_MB = 1536; // ~1.5 GB
    private readonly EXPERT_VRAM_MB = 6860;  // ~6.7 GB (Gemma 4 12B QAT 4-bit)
    private readonly AVATAR_VRAM_MB = 800;   // ~800 MB cho WebGL textures & buffers

    constructor(config: Partial<ProfilerConfig> = {}) {
        this.config = {
            vramLimitMB: config.vramLimitMB ?? 12288, // 12GB mặc định (phù hợp với cấu hình Tier 2)
            ramWarningThresholdBytes: config.ramWarningThresholdBytes ?? (os.totalmem() * 0.85), // 85% tổng RAM
            logDir: config.logDir ?? path.join(process.cwd(), 'logs', 'telemetry'),
            sampleIntervalMS: config.sampleIntervalMS ?? 5000 // 5 giây quét một lần
        };
        this.logFilePath = path.join(this.config.logDir, `memory_profile_${Date.now()}.jsonl`);
    }

    /**
     * Cập nhật trạng thái hiện tại của Agent Loop
     */
    public setAgentState(state: AgentLoopState): void {
        this.activeState = state;
    }

    /**
     * Báo hiệu trạng thái nạp mô hình vào VRAM từ ModelOrchestrator
     */
    public setModelLoadingStatus(model: 'router' | 'expert', loaded: boolean): void {
        if (model === 'router') {
            this.isRouterLoaded = loaded;
        } else if (model === 'expert') {
            this.isExpertLoaded = loaded;
        }
    }

    /**
     * Khởi động bộ theo dõi ngầm
     */
    public async start(): Promise<void> {
        await fs.mkdir(this.config.logDir, { recursive: true });
        if (this.intervalId) return;

        this.intervalId = setInterval(async () => {
            try {
                const snapshot = await this.takeSnapshot();
                this.memoryHistory.push(snapshot);
                await this.writeSnapshotToDisk(snapshot);
                this.evaluateThresholds(snapshot);
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                console.error(`[TelemetryProfiler] Ghi nhận snapshot thất bại: ${msg}`);
            }
        }, this.config.sampleIntervalMS);

        if (this.intervalId.unref) {
            this.intervalId.unref(); // Tránh nghẽn tiến trình khi tắt ứng dụng
        }
    }

    /**
     * Dừng bộ theo dõi
     */
    public stop(): void {
        if (this.intervalId) {
            clearInterval(this.intervalId);
            this.intervalId = null;
        }
    }

    /**
     * Trích xuất thông tin bộ nhớ tại thời điểm gọi
     */
    public async takeSnapshot(): Promise<MemorySnapshot> {
        const memoryUsage = process.memoryUsage();
        
        let estimatedVram = 0;
        let routerFootprint = 0;
        let expertFootprint = 0;
        let avatarFootprint = 0;

        if (this.isRouterLoaded) {
            routerFootprint = this.ROUTER_VRAM_MB;
            estimatedVram += this.ROUTER_VRAM_MB;
        }
        if (this.isExpertLoaded) {
            expertFootprint = this.EXPERT_VRAM_MB;
            estimatedVram += this.EXPERT_VRAM_MB;
        }
        if (this.activeState === 'RENDERING') {
            avatarFootprint = this.AVATAR_VRAM_MB;
            estimatedVram += this.AVATAR_VRAM_MB;
        }

        return {
            timestamp: Date.now(),
            state: this.activeState,
            ram: {
                rss: memoryUsage.rss,
                heapTotal: memoryUsage.heapTotal,
                heapUsed: memoryUsage.heapUsed,
                external: memoryUsage.external,
                systemTotal: os.totalmem(),
                systemFree: os.freemem()
            },
            vram: {
                estimatedTotal: estimatedVram,
                routerFootprint,
                expertFootprint,
                avatarFootprint,
                limit: this.config.vramLimitMB
            }
        };
    }

    /**
     * Đánh giá các ngưỡng cảnh báo tài nguyên
     */
    private evaluateThresholds(snapshot: MemorySnapshot): void {
        // Cảnh báo vượt ngưỡng RAM Node.js
        if (snapshot.ram.rss > this.config.ramWarningThresholdBytes) {
            const usagePercent = ((snapshot.ram.rss / snapshot.ram.systemTotal) * 100).toFixed(1);
            console.warn(
                `[TelemetryProfiler] ⚠️ CẢNH BÁO BỘ NHỚ RAM: Bộ nhớ RSS Node.js (${(snapshot.ram.rss / 1024 / 1024).toFixed(1)} MB) ` +
                `vượt ngưỡng an toàn. Chiếm dụng hệ thống: ${usagePercent}%. Trạng thái Agent: ${snapshot.state}`
            );
        }

        // Cảnh báo quá tải VRAM ước tính
        if (snapshot.vram.estimatedTotal > snapshot.vram.limit) {
            console.warn(
                `[TelemetryProfiler] ⚠️ CẢNH BÁO VRAM: Lượng VRAM ước tính (${snapshot.vram.estimatedTotal} MB) ` +
                `vượt quá giới hạn cấu hình (${snapshot.vram.limit} MB). Nguy cơ gây lỗi OOM GPU.`
            );
        }
    }

    /**
     * Ghi thông tin snapshot xuống ổ đĩa định dạng JSON Lines (.jsonl)
     */
    private async writeSnapshotToDisk(snapshot: MemorySnapshot): Promise<void> {
        const line = JSON.stringify(snapshot) + '\n';
        await fs.appendFile(this.logFilePath, line, 'utf-8');
    }

    /**
     * Xuất báo cáo tổng kết lịch sử đo đạc
     */
    public getHistoryReport(): { totalSamples: number; stateStats: Record<string, number>; maxRamBytes: number; maxVramMB: number } {
        let maxRam = 0;
        let maxVram = 0;
        const stateCounts: Record<string, number> = {};

        for (const snap of this.memoryHistory) {
            if (snap.ram.rss > maxRam) maxRam = snap.ram.rss;
            if (snap.vram.estimatedTotal > maxVram) maxVram = snap.vram.estimatedTotal;
            stateCounts[snap.state] = (stateCounts[snap.state] || 0) + 1;
        }

        return {
            totalSamples: this.memoryHistory.length,
            stateStats: stateCounts,
            maxRamBytes: maxRam,
            maxVramMB: maxVram
        };
    }
}
```

---

## 4. PHƯƠNG ÁN CẤU HÌNH PHẦN CỨNG TIÊU CHUẨN (HARDWARE CONFIGURATION PROFILES)

Để đảm bảo hệ thống LIVA hoạt động ổn định trên nhiều thiết bị phần cứng khác nhau của người dùng, chúng tôi đề xuất 3 nhóm cấu hình chuẩn dựa trên dung lượng bộ nhớ khả dụng:

### CẤU HÌNH PHÂN CẤP (TIER PROFILES):

```
┌────────────────────────────────────────────────────────────────────────┐
│                        TIER PROFILES MATRIX                            │
├──────────────────┬──────────────────────────┬──────────────────────────┤
│    Tier 1        │        Tier 2            │        Tier 3            │
│  (8GB RAM /      │     (16GB RAM /          │     (24GB+ RAM /         │
│   6GB VRAM)      │      12GB VRAM)          │      16GB+ VRAM)         │
├──────────────────┼──────────────────────────┼──────────────────────────┤
│ • Không sử dụng  │ • Tráo đổi nóng tuần tự  │ • Duy trì cả hai mô hình │
│   Expert model   │   (60s Cooldown)         │   (Không cần Hot-Swap)   │
│ • Hoạt động 100% │ • Router: 100% GPU       │ • Router: 100% GPU       │
│   trên Router    │ • Expert: Phân bổ động   │ • Expert: 100% GPU       │
│ • Avatar:        │ • Avatar:                │ • Avatar:                │
│   Eco Mode (5FPS)│   Chạy tự động (60 FPS)  │   Tối đa chất lượng      │
│ • Context:       │ • Context:               │ • Context:               │
│   2048 tokens    │   4096 / 8192 tokens     │   16384 tokens           │
└──────────────────┴──────────────────────────┴──────────────────────────┘
```

### 4.1. Tier 1: Thiết bị Cơ bản / Di động (Low-spec: 8GB System RAM / 6GB VRAM)
*   **Chiến lược mô hình (Model Strategy)**: **Không sử dụng mô hình Expert 12B**. Chỉ cho phép nạp mô hình Router 4B. Mọi yêu cầu suy luận phức tạp đều được dịch chuyển về các cấu hình tinh gọn của mô hình Router để tránh gây tràn bộ nhớ GPU 6GB VRAM.
*   **Cấu hình llama-server**:
    - `--n-gpu-layers`: 100% các lớp của Router đưa vào GPU.
    - `--ctx-size`: Rút ngắn xuống còn `2048` tokens để tiết kiệm tối đa KV Cache.
    - `--flash-attn`: Bắt buộc kích hoạt.
*   **Tham số Kết xuất Đồ họa (Avatar Settings)**:
    - Bắt buộc chạy ở chế độ tiết kiệm năng lượng (`LIVA_ECO_MODE = true`), giới hạn khung hình ở mức **5 FPS**.
    - Sử dụng các mô hình nhân vật có kích thước kết cấu ảnh (texture size) nhỏ (giới hạn dưới 1024x1024px).
*   **Tham số Hệ thống CSDL (SQLite Parameters)**:
    - `PRAGMA cache_size = -2048` (Giới hạn bộ đệm trang chỉ ở mức 2MB để tiết kiệm RAM).
    - `PRAGMA mmap_size = 33554432` (Ánh xạ bộ nhớ giảm xuống còn 32MB).

### 4.2. Tier 2: Thiết bị Tiêu chuẩn (Mid-spec: 16GB System RAM / 12GB VRAM)
*   **Chiến lược mô hình (Model Strategy)**: **Kích hoạt tráo đổi nóng tuần tự (Sequential Hot-Swapping)** điều phối bởi `ModelOrchestrator`.
    - Duy trì Cooldown timer `EXPERT_COOLDOWN_MS = 90,000` (1.5 phút) để giảm thiểu tần suất nạp mô hình lớn.
*   **Cấu hình llama-server**:
    - Đối với Router: `--n-gpu-layers` = tối đa (100% GPU). Kích thước ngữ cảnh `4096`.
    - Đối với Expert: Với GPU 12GB VRAM, mô hình Expert 12B QAT 4-bit (~6.7 GB bao gồm cả cache) hoàn toàn có thể chạy offload 100% lên GPU (`--n-gpu-layers` = tối đa). Kích thước ngữ cảnh giới hạn ở mức `8192` tokens. Cơ chế Hot-Swapping vẫn cần được bật để tránh quá tải tổng thể khi WebGL và hệ điều hành cùng chia sẻ VRAM.
    - `--flash-attn`: Bắt buộc kích hoạt.
*   **Tham số Kết xuất Đồ họa (Avatar Settings)**:
    - Cho phép chạy ở tốc độ quét màn hình tối đa (60 FPS) khi cửa sổ hoạt động tích cực.
    - Tự động hạ xuống **15 FPS** khi tab chạy ẩn và kích hoạt chế độ **Freeze (0 FPS)** ngay lập tức khi phát hiện VRAM của Expert cần thu hồi.
*   **Tham số Hệ thống CSDL (SQLite Parameters)**:
    - `PRAGMA cache_size = -8192` (Bộ đệm trang 8MB).
    - `PRAGMA mmap_size = 268435456` (Ánh xạ bộ nhớ 256MB).

### 4.3. Tier 3: Trạm làm việc Hiệu năng cao (High-spec: 24GB+ System RAM / 16GB+ VRAM)
*   **Chiến lược mô hình (Model Strategy)**: **Không cần Hot-Swap hoặc đặt thời gian Cooldown rất dài (5+ phút)**.
    - Với dung lượng VRAM lớn (ví dụ: Apple Studio 64GB hoặc GPU Nvidia RTX 4090/A6000), hệ thống cho phép giữ cả hai mô hình Router và Expert trên VRAM đồng thời mà không xảy ra xung đột tranh chấp tài nguyên.
*   **Cấu hình llama-server**:
    - `--n-gpu-layers`: Thiết lập giá trị cao nhất cho cả hai mô hình (nạp 100% lên GPU).
    - `--ctx-size`: Đẩy cao giới hạn lên tới `16384` tokens cho Expert để xử lý các tài liệu nghiên cứu siêu dài.
*   **Tham số Kết xuất Đồ họa (Avatar Settings)**:
    - Chạy kết xuất chất lượng cao nhất không giới hạn FPS.
    - Sử dụng các bộ vân bề mặt nhân vật (textures) độ phân giải cao và các thuật toán đổ bóng nâng cao.
*   **Tham số Hệ thống CSDL (SQLite Parameters)**:
    - `PRAGMA cache_size = -32768` (Tăng bộ đệm trang lên tới 32MB).
    - `PRAGMA mmap_size = 1073741824` (Tăng kích thước mmap lên tới 1GB để đọc cơ sở dữ liệu nhanh như truy xuất biến RAM cục bộ).

---

## KẾT LUẬN VÀ LỘ TRÌNH ĐỀ XUẤT (CONCLUSION & ROADMAP)
Qua các phân tích trên, kiến trúc phân cấp bộ nhớ của LIVA đã tận dụng tốt tính năng của SQLite và Three.js để duy trì trải nghiệm người dùng ổn định trên cấu hình chuẩn. Tuy nhiên, việc liên tục dịch chuyển giữa các mô hình LLM vẫn tồn tại độ trễ nhất định. 

Để tối ưu hóa sâu hơn trong tương lai, nhóm phát triển LIVA đề xuất lộ trình cải tiến gồm 3 bước:
1.  **Giai đoạn 1**: Ổn định và chuẩn hóa hệ thống đo lường tự động `TelemetryProfiler` vào nhân lõi hệ thống gateway nhằm ghi nhận hành vi bộ nhớ của người dùng thực tế.
2.  **Giai đoạn 2**: Thử nghiệm và tích hợp các phiên bản lượng hóa tối ưu hơn cho mô hình Expert (ví dụ: định dạng GGUF kiểu IQ4_NL hoặc Q3_K_L) để giảm dung lượng tĩnh từ ~6.0 GB xuống còn ~4.8 GB mà không làm giảm đáng kể khả năng suy luận logic.
3.  **Giai đoạn 3**: Nghiên cứu xây dựng cơ chế chia sẻ không gian VRAM động trực tiếp giữa WebGL Renderer của trình duyệt và tiến trình `llama-server` (thông qua WebGPU và bộ đệm chia sẻ vùng nhớ IPC) nhằm triệt tiêu hoàn toàn sự lãng phí tài nguyên khi chuyển đổi trạng thái kết xuất đồ họa.
