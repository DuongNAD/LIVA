# Kiến Trúc Remake Dự Án LIVA: Đề Xuất Nâng Cấp & Hiện Đại Hóa Toàn Diện
# LIVA Next-Gen Architecture Remake Proposal (LIVA-REMAKE-2026)

**Tài liệu:** Đề xuất Kiến trúc Remake Toàn diện Dự án LIVA  
**Mã dự án:** LIVA-REMAKE-2026  
**Ngày lập:** 2026-09-19  
**Mục tiêu:** Giải quyết triệt để các điểm nghẽn về đồng quy, lưu trữ, CI/CD, IPC và kết xuất giao diện; tái cấu trúc hệ thống thành kiến trúc micro-crates hiệu năng cao, hướng actor, chuẩn hóa bộ nhớ đệm và tối ưu hóa tài nguyên phần cứng máy trạm.

---

## 1. Tóm Tắt Điều Hành (Executive Summary)

Dự án LIVA (Local Intelligent Virtual Assistant) đã hoàn tất quá trình hợp nhất logic nghiệp vụ từ hệ sinh thái Node.js/Python sang lõi Rust thuần (`liva-native-core`), kết hợp khung ứng dụng máy tính Tauri v2 (`liva-desktop`) và giao diện Vue 3 / Three.js (`liva-ui`). Sự chuyển đổi này mang lại nền tảng vững chắc về an toàn bộ nhớ và hiệu năng xử lý cục bộ, bảo đảm khả năng vận hành ngoại tuyến 100% trên các máy tính Windows 10/11 x64 với giới hạn tài nguyên khắt khe:
- **RAM hệ thống:** $\le 4.0\text{ GB}$ (ngưỡng hoạt động tối ưu $\sim 3.0\text{ GB}$).
- **VRAM GPU:** $\le 5.1\text{ GB}$.

Tuy nhiên, qua khảo sát và điều tra pháp y chuyên sâu (forensic exploration) từ 3 hướng độc lập:
1. **Kiến trúc mã nguồn & mô hình đồng quy** (`teamwork_preview_explorer_arch_codebase`)
2. **Hệ thống cơ sở dữ liệu & lưu trữ** (`teamwork_preview_explorer_arch_database`)
3. **Hệ thống build & quy trình CI/CD** (`teamwork_preview_explorer_arch_cicd`)

Hệ thống hiện tại đang bộc lộ những **điểm nghẽn kiến trúc chí tử**:
- Khóa toàn cục `tokio::sync::Mutex<LlamaRouterManager>` gây hiện tượng nghẽn đầu hàng (head-of-line blocking), chặn đứng mọi luồng kiểm tra sức khỏe và truy vấn đồng thời trong suốt thời gian sinh token kéo dài 3–15 giây.
- Cấu hình SQLite phóng đại bộ đệm trang (`cache_size = -64000` tiêu tốn tới 312.5 MB RAM chỉ cho cache SQLite), kèm theo việc quét toàn bảng và giải mã AES-256-GCM toàn bộ cơ sở tri thức facts trên từng lượt hội thoại của tính năng Active Recall.
- Thư mục build `target/` phình trướng kỷ lục **94.34 GB** (chứa 9.772 artifacts trong `target/debug/deps`) do 106 mục tiêu thực thi liên kết riêng rẽ (78 integration tests + 26 binary probes) với các thư viện C++ nặng nề (`llama.cpp`, `ort`).
- Trùng lặp hạ tầng giao tiếp: Chạy song song cả Tauri Native IPC lẫn TCP WebSocket server nội bộ, làm phát sinh hơn 2.100 dòng mã boilerplate điều vận thủ công trong `websocket.rs`.
- Giao diện người dùng bị nghẽn luồng: Vòng lặp render 60 FPS Three.js, động học xương phụ (spring bones) và xử lý âm thanh worklet chạy chung trên luồng chính JavaScript của Vue 3, dẫn đến sụt khung hình khi có tương tác thoại.

Tài liệu này đề xuất **Kiến trúc Remake LIVA Thế Hệ Mới (LIVA Next-Gen Architecture)** với các trụ cột:
1. **Phân rã không gian làm việc Multi-Crate Topology** (`crates/liva-core-types`, `crates/liva-storage`, `crates/liva-llm`, `crates/liva-voice`, `crates/liva-tools`).
2. **Mô hình Đồng quy Actor-Based Concurrency** với hàng đợi ưu tiên 3 mức (High, Normal, Low) và kênh phi khóa (MPSC/Broadcast).
3. **Động cơ Lưu trữ Tối ưu Hóa (`liva-storage`)** với pragmas chuẩn hóa (tiết kiệm 85% RAM cache), gom cụm giao dịch micro-batching (5ms), và chỉ mục tiền tố Trie trong bộ nhớ cho Active Recall.
4. **Hạ tầng CI/CD DAG 4 Nhánh Song Song** cùng bộ thu gọn test harness và tối ưu hóa phát hành (`[profile.release]` LTO Thin, symbol stripping, giảm kích thước cài đặt từ 291 MB xuống dưới 85 MB).
5. **Giao tiếp Hợp nhất 100% Tauri v2 Channels**, khai tử hoàn toàn TCP WebSocket nội bộ.

---

## 2. Phân Tích Pháp Y Chi Tiết: Các Điểm Nghẽn Trọng Yếu Trong Kiến Trúc Cũ

### 2.1 Điểm nghẽn 1: Xung Đột Khóa Toàn Cục LLM & Nghẽn Đầu Hàng (Head-of-Line Blocking)

#### 1. Vị trí mã nguồn cụ thể:
- `liva-native-core/src/lib.rs`: Dòng 75 (định nghĩa `AppState.llm`), dòng 586–616 (`handle_chat_completion_scoped`).
- `liva-native-core/src/commands/llm.rs`: Dòng 82 (`llm:health_check`), dòng 109 (`llm:context_info`), dòng 136 (`llm:cancel`).
- `liva-native-core/src/llm/engine.rs`: Khối xử lý sinh token và tráo đổi mô hình.

#### 2. Nguyên nhân gốc rễ & Cơ chế tác động:
Trong `liva-native-core/src/lib.rs`, cấu trúc `AppState` lưu trữ `LlamaRouterManager` thông qua một khóa độc quyền:
```rust
// liva-native-core/src/lib.rs:75
pub struct AppState {
    ...
    pub llm: tokio::sync::Mutex<LlamaRouterManager>,
    ...
}
```
Khi người dùng kích hoạt một lượt sinh văn bản qua `handle_chat_completion_scoped`, mã nguồn thực thi:
```rust
// liva-native-core/src/lib.rs:588-592
let completion_res = tokio::task::spawn_blocking(move || {
    let messages = inference_messages;
    let mut llm_manager = state_clone.llm.blocking_lock();
    // Tự động tráo đổi router <-> expert model theo độ khó
    let _ = llm_manager.maybe_auto_swap_blocking(do_kho);
    ...
```
`state_clone.llm.blocking_lock()` chiếm giữ độc quyền toàn bộ động cơ LLM trong suốt toàn bộ chu kỳ sinh văn bản (kéo dài từ 3.000ms đến hơn 15.000ms đối với các câu trả lời dài hoặc suy luận chuỗi tư duy CoT). Đặc biệt, nếu độ khó của câu lệnh kích hoạt `maybe_auto_swap_blocking`, việc hủy nạp mô hình router và nạp mô hình expert dung lượng 4GB–7GB từ ổ cứng SSD diễn ra **ngay trong khi đang nắm giữ khóa độc quyền này**.

#### 3. Hệ quả & Bán kính ảnh hưởng:
- **Đóng băng giao diện người dùng:** Bất kỳ thao tác kiểm tra trạng thái (`llm:health_check`) định kỳ từ Dashboard, truy vấn thông tin ngữ cảnh (`llm:context_info`), hay tạo embedding phụ trợ từ background worker đều bị phong tỏa hoàn toàn.
- **Không thể hủy yêu cầu tức thời:** Khi người dùng gửi lệnh ngắt (`voice:interrupt` hoặc `llm:cancel`), luồng xử lý không thể lấy được khóa để can thiệp cờ hủy cho đến khi vòng lặp C++ của llama.cpp tự chạm tới điểm dừng.
- **Sụp đổ tính phản hồi đa cửa sổ:** Ứng dụng widget và dashboard chạy đồng thời sẽ bị treo cứng nếu một bên đang yêu cầu LLM xử lý văn bản.

---

### 2.2 Điểm nghẽn 2: Phình Trướng Bộ Đệm SQLite, Giao Dịch Đơn Lẻ & Quét Toàn Bảng Active Recall

#### 1. Vị trí mã nguồn cụ thể:
- `liva-native-core/src/db.rs`: Dòng 55–56 (`configure_connection`).
- `liva-native-core/src/db_actor.rs`: Dòng 424–444 (vòng lặp tiếp nhận ghi `rx.blocking_recv()`).
- `liva-native-core/src/active_recall.rs`: Dòng 248–278 (`get_candidate_facts`).
- `liva-native-core/src/db/csr_graph.rs`: Dòng 357, 416–445 (tái biên dịch toàn bộ đồ thị CSR).
- `liva-native-core/src/db.rs`: Dòng 1794–1808 (lọc subquery vector `sqlite-vec`).

#### 2. Nguyên nhân gốc rễ & Cơ chế tác động:
1. **Lãng phí bộ nhớ RAM nghiêm trọng:**
   Trong `src/db.rs:55-56`:
   ```rust
   PRAGMA cache_size = -64000;
   PRAGMA page_size = 32768;
   ```
   Tham số `-64000` yêu cầu SQLite phân bổ 64.000 KiB ($62.5\text{ MiB}$) bộ đệm trang trên **mỗi kết nối kết nối**. Với 1 kết nối ghi chuyên dụng và 4 kết nối đọc trong pool `r2d2`, hệ thống tiêu tốn:
   $$(1 + 4) \times 62.5\text{ MiB} = 312.5\text{ MiB RAM}$$
   chỉ dành riêng cho bộ đệm SQLite. Khi pool đọc được mở rộng lên 8 kết nối, lượng RAM bị khóa vượt quá **562.5 MiB**, trực tiếp tước đoạt tài nguyên của các mô hình âm thanh ONNX và LLM. Kích thước trang 32 KB tạo độ phóng đại ghi (write amplification) từ 40x đến 160x với các bản ghi nhỏ 200–800 bytes.
2. **Giao dịch ghi bị phân mảnh (Unbatched Writes):**
   Trong `src/db_actor.rs:424-444`, `DbActor` nhận từng `DbWriteCommand` từ kênh MPSC. Mặc dù có vòng lặp vét `try_recv()`, hàm `process_write_command` mở và commit một transaction riêng biệt cho từng lệnh đơn lẻ. Khi có 50 thao tác cập nhật (ví dụ import lịch sử hay đồng bộ tri thức), SQLite thực thi 50 lệnh `COMMIT` liên tiếp, ép ghi dữ liệu xuống đĩa 50 lần.
3. **Thảm họa CPU từ Active Recall:**
   Trong `src/active_recall.rs:248-272`, trên **mỗi lượt hội thoại** trước khi gửi prompt vào LLM:
   ```rust
   let mut stmt = reader.prepare("SELECT key, value, memory_strength, ... FROM facts")?;
   for row in rows.flatten() {
       let fr = crypto.read_fact(&enc_val);
       let plain_val = fr.into_value();
       candidate_facts.push((key, plain_val, ...));
   }
   ```
   Hệ thống thực hiện quét toàn bộ bảng `facts` không qua chỉ mục từ khóa, sau đó thực thi phép dẫn xuất khóa HKDF-SHA256 và giải mã AES-256-GCM cho **toàn bộ các fact có trong database**. Với 2.000 facts của người dùng, CPU phải thực hiện 2.000 lần giải mã mã hóa đối xứng trước mỗi câu trả lời, gây độ trễ trừng phạt 50ms – 250ms trên luồng tương tác thoại.
4. **Tái biên dịch đồ thị CSR dưới khóa ghi:**
   Mỗi khi chèn một cạnh tri thức L3 vào đồ thị HippoRAG, `src/db.rs:416` chiếm giữ `RwLock::write` của `CsrGraph` và gọi `compile_csr()`. Hàm này duyệt lại toàn bộ các đỉnh và cạnh, chuẩn hóa xác suất ngẫu nhiên và cấp phát mới toàn bộ 4 mảng vector phẳng (`row_ptr`, `col_indices`, `weights`, `edge_relations`), làm ngưng trệ thuật toán lan truyền Personalized PageRank (PPR).

---

### 2.3 Điểm nghẽn 3: CI/CD Đơn Khối, 106 Binaries Độc Lập & Thư Mục Target Phình Trướng 94 GB

#### 1. Vị trí mã nguồn cụ thể:
- `Cargo.toml`: Dòng 1–38 (thiếu cấu hình profile release, thiếu workspace inheritance).
- `liva-native-core/tests/*.rs`: 78 file kiểm thử tích hợp độc lập.
- `liva-native-core/src/bin/*.rs`: 26 file binary probe và benchmark độc lập.
- `.github/workflows/test.yml`: Dòng 1–210 (25 bước kiểm thử tuần tự trên Windows runner).

#### 2. Nguyên nhân gốc rễ & Cơ chế tác động:
1. **Phình trướng đĩa cứng 94.34 GB:**
   Thư mục `target/` đạt dung lượng kỷ lục:
   - `target/debug/deps`: **46.89 GB** với **9.772 file** `.rlib`, `.exe`, `.pdb`.
   - `target/debug/incremental`: **28.65 GB** bộ nhớ đệm biên dịch gia tăng.
   - `target/release`: **10.96 GB**.
   Tổng dung lượng lên tới **94.34 GB**, gây nghẽn nghiêm trọng I/O ổ đĩa trên Windows Defender và hệ thống tệp NTFS.
2. **Nghẽn cổ chai Linking từ 106 mục tiêu thực thi:**
   Cargo quản lý mỗi file trong `tests/*.rs` (78 files) và mỗi file trong `src/bin/*.rs` (26 files) như một crate thực thi độc lập. Mỗi binary này đều phải liên kết tĩnh hoặc động với các thư viện C++ đồ sộ: `llama-cpp-sys-2`, `ort-sys`, `rusqlite`, `rodio`, `windows-sys`. Trình liên kết mặc định `link.exe` của MSVC phải thực hiện 106 lần linking riêng biệt, khiến lệnh `cargo test` hay `cargo check --tests` mất nhiều phút.
3. **Pipeline CI đơn luồng, lãng phí chi phí:**
   File `.github/workflows/test.yml` dồn ép toàn bộ 25 bước chạy tuần tự trên một máy ảo duy nhất `windows-latest`:
   - Phải cài đặt `choco install llvm` và biên dịch `cargo-deny` từ mã nguồn (tiêu tốn 4–6 phút vô ích trên mỗi lần kích hoạt commit).
   - Chạy ESLint, Vitest, TypeScript checking và documentation check trên Windows thay vì chạy song song trên `ubuntu-latest` (nhanh hơn gấp 3 lần và tiết kiệm 50% chi phí).
   - Cache toàn bộ thư mục `target/` khổng lồ, thường xuyên vượt ngưỡng giới hạn 10 GB per-repo của GitHub Actions dẫn tới vỡ cache (cache eviction).
4. **Bộ cài đặt cồng kềnh chưa tối ưu:**
   File cài đặt Windows `LIVA_1.0.0_x64-setup.exe` nặng tới **291.2 MB** do thiếu cấu hình `[profile.release]` tại root `Cargo.toml`: không bật Link-Time Optimization (LTO), `codegen-units` giữ nguyên mặc định 16, không bật `strip = "symbols"`, và `panic` dùng kiểu unwind.

---

### 2.4 Điểm nghẽn 4: Trùng Lặp Hạ Tầng Giao Tiếp (Dual IPC Architecture Redundancy)

#### 1. Vị trí mã nguồn cụ thể:
- `liva-desktop/src-tauri/src/lib.rs`: Dòng 572–600, dòng 883–896.
- `liva-native-core/src/websocket.rs`: Dòng 1–2146 (toàn bộ 2.146 dòng mã).
- `liva-ui/src/composables/useGateway.ts`: Dòng 543–575.
- `liva-ui/src/composables/useWidgetTransport.ts`: Dòng 69–250.

#### 2. Nguyên nhân gốc rễ & Cơ chế tác động:
Ứng dụng LIVA trên máy tính là một tiến trình đơn khối: giao diện người dùng Webview và lõi xử lý Rust chạy trên cùng một máy cục bộ, được kết nối trực tiếp thông qua cầu nối native IPC của Tauri v2 (`invoke`, `Channel`).
Tuy nhiên, hệ thống vẫn duy trì một máy chủ TCP WebSocket hoàn chỉnh (`tokio-tungstenite`) lắng nghe trên cổng mạng cục bộ (`ws://127.0.0.1:port`). 
File `websocket.rs` dài tới **2.146 dòng**, trong đó hơn 600 dòng mã (từ dòng 1163 đến 1750) chỉ làm nhiệm vụ:
- Nhận chuỗi JSON qua kết nối TCP loopback.
- Bóc tách trường `"event"` và `"payload"`.
- Chuyển tiếp vào hàm `handle_command_as(...)` của AppState.
- Đóng gói kết quả trả về vào phong bì JSON `{ "event": "...", "payload": ... }` rồi phát lại qua mạng.

#### 3. Hệ quả & Bán kính ảnh hưởng:
- **Nguy cơ xung đột mạng:** Gây lỗi chiếm dụng cổng (port binding collision), kích hoạt cảnh báo Windows Firewall không đáng có cho người dùng cuối.
- **Tổn thất hiệu năng gấp đôi (Double Serialization):** Dữ liệu âm thanh thô (PCM) và visemes chuyển động miệng phải trải qua hai lần đóng gói và sao chép bộ nhớ (buffer copy) qua socket stack thay vì truyền trực tiếp qua con trỏ bộ nhớ dùng chung trong tiến trình.
- **Phân mảnh ranh giới lỗi:** Các lỗi rớt mạng loopback khiến UI bị mất kết nối giả tạo dù lõi Rust native bên dưới vẫn đang hoạt động bình thường.

---

### 2.5 Điểm nghẽn 5: Giao Diện Phình Trướng "God Components" & Tranh Chấp Luồng Render WebGL

#### 1. Vị trí mã nguồn cụ thể:
- `liva-ui/src/WidgetApp.vue`: 2.105 dòng mã (83.6 KB).
- `liva-ui/src/composables/use3DModel.ts`: 1.914 dòng mã (74.9 KB).
- `liva-desktop/src-tauri/src/lib.rs`: Dòng 784–872 (vòng lặp 30ms sleep thăm dò chuột).

#### 2. Nguyên nhân gốc rễ & Cơ chế tác động:
1. **Quá tải luồng chính trình duyệt:**
   Hai file `WidgetApp.vue` và `use3DModel.ts` dồn ép toàn bộ các trách nhiệm nặng nề nhất vào một luồng đơn JavaScript:
   - Vòng lặp kết xuất Three.js 60 FPS, tính toán biến dạng xương và động học lò xo (spring bones).
   - Động học chuyển động người gia số (additive procedural kinematics: nhịp thở sin, xoay bù cột sống, chuyển động mắt saccades).
   - Thu âm audio worklet, băm nhỏ PCM và nội suy phoneme lip-sync theo thời gian thực.
   - Cập nhật DOM phản ứng (reactive DOM) của Vue 3 cho bóng thoại, thanh nhập liệu và bảng công cụ.
   Khi luồng LLM phát token về dồn dập hoặc Vue thực hiện cập nhật cây DOM phức tạp, luồng chính JS bị nghẽn (JavaScript execution lag), khiến Three.js bị trễ khung hình, avatar 3D bị giật cục rõ rệt.
2. **Thăm dò chuột cưỡng bức 33 Hz:**
   Trong `liva-desktop/src-tauri/src/lib.rs:784`, một luồng hệ điều hành chạy vòng lặp vô tận với `sleep(Duration::from_millis(30))` liên tục gọi `cursor_position()` để tính khoảng cách Euclidean cho tính năng xuyên chuột (Ghost Mode) thay vì sử dụng hook sự kiện cấp thấp Windows (`WH_MOUSE_LL`). Điều này gây lãng phí chu kỳ CPU liên tục ngay cả khi máy tính đang ở trạng thái rảnh.

---

## 3. Thiết Kế Kiến Trúc Remake Thế Hệ Mới (Next-Gen Remake Architecture)

Kiến trúc Remake LIVA được thiết kế xoay quanh 5 nguyên lý cốt lõi:
1. **Module hóa cực đại (Strict Decoupling):** Chia tách mã nguồn thành các crate độc lập, có ranh giới trách nhiệm rõ ràng.
2. **Xử lý phi nghẽn luồng (Non-Blocking Concurrency):** Thay thế khóa toàn cục bằng mô hình tác tử (Actor Pattern) và các kênh truyền MPSC/Broadcast.
3. **Tiết kiệm tài nguyên tuyệt đối (Resource Frugality):** Chuẩn hóa cấu hình cơ sở dữ liệu và bộ nhớ đệm, bảo đảm mức tiêu thụ RAM luôn nằm trong ngưỡng an toàn $< 3.0\text{ GB}$.
4. **Hiện đại hóa quy trình biên dịch & phân phối:** Tự động hóa CI/CD song song dạng DAG, thu gọn linking targets và tối ưu hóa nhị phân release.
5. **Giao tiếp trong tiến trình thuần nhất:** Sử dụng hoàn toàn cơ chế truyền phát nhị phân của Tauri v2.

```
                           ┌────────────────────────────────────────────────────────┐
                           │                 LIVA DESKTOP SHELL                     │
                           │                    (liva-desktop)                      │
                           │     Tauri v2 Application · System Tray · Windows       │
                           └───────────────────────────┬────────────────────────────┘
                                                       │ Native Tauri v2 IPC
                                                       │ (Channels & Binary Streams)
                                                       ▼
┌───────────────────────────────────────────────────────────────────────────────────────────────────┐
│                             LIVA NATIVE ORCHESTRATION FACADE                                      │
│                                    (liva-native-core)                                             │
│                 handle_command · Lifecycle Coordinator · AppState Context                         │
├──────────────────────┬───────────────────────┬─────────────────────┬──────────────────────────────┤
│                      │                       │                     │                              │
│       crates/        │        crates/        │       crates/       │           crates/            │
│   liva-core-types    │      liva-storage     │      liva-llm       │          liva-voice          │
│                      │                       │                     │                              │
│  - Pure Domain Types │  - SQLite Pool (WAL)  │  - LlmActor Worker  │  - Full-Duplex WebRTC        │
│  - Errors & Results  │  - Micro-Batched      │  - Priority Queue   │  - Silero VAD / Sonora AEC3  │
│  - Permissions/Auth  │    Single-Writer      │    (High/Norm/Low)  │  - GTCRN Denoiser            │
│  - IPC Message       │  - Trie Active Recall │  - Broadcast Stream │  - Parakeet STT (ONNX)       │
│    Contracts         │  - Double-Buffered    │  - SHA-256 Trust    │  - VieNeu/Piper TTS          │
│  - Zero Heavy Deps   │    CsrGraph (HippoRAG)│    Artifact Cache   │  - Phoneme / Viseme Stream   │
│                      │  - Calibrated Pragmas │  - Dynamic Prompt   │  - Audio Ring Buffers        │
│                      │  - Scoped Vector ANN  │    Budgeting        │                              │
└──────────────────────┴───────────────────────┴─────────────────────┴──────────────────────────────┘
                                                       ▲
                                                       │ Command Dispatch & Diagnostics
                               ┌───────────────────────┴───────────────────────┐
                               │                 crates/liva-tools             │
                               │        Consolidated CLI & Diagnostic Suite    │
                               │   (Replaces 26 separate probe/test binaries)  │
                               └───────────────────────────────────────────────┘
```

---

### 3.1 Cấu Trúc Phân Rã Không Gian Làm Việc (Multi-Crate Workspace Topology)

Không gian làm việc được tái cấu trúc thành một Cargo Monorepo hiện đại với cây thư mục sau:

```
LIVA/
├── Cargo.toml                         # Root Workspace: workspace.dependencies, release profiles, lints
├── .cargo/
│   └── config.toml                    # lld-link fast linker, msvc optimization flags
├── .github/
│   └── workflows/
│       └── test.yml                   # Modern 4-Job Parallel DAG CI/CD Pipeline
├── crates/
│   ├── liva-core-types/               # Crate nền tảng: pure domain structs, error enums, permission models
│   │   ├── Cargo.toml                 # Zero heavy external dependencies (compile time < 2s)
│   │   └── src/                       # lib.rs, events.rs, errors.rs, ipc_contracts.rs
│   ├── liva-storage/                  # Động cơ lưu trữ & cơ sở tri thức
│   │   ├── Cargo.toml                 # rusqlite, r2d2, sqlite-vec, aes-gcm, hkdf
│   │   └── src/                       # db_actor.rs (micro-batching), trie_recall.rs, csr_graph.rs, pragmas.rs
│   ├── liva-llm/                      # Động cơ suy luận ngôn ngữ & Actor Worker
│   │   ├── Cargo.toml                 # llama-cpp-2, tokio channels, tokenizers
│   │   └── src/                       # actor.rs, router.rs, prompt_budget.rs, trust_cache.rs
│   ├── liva-voice/                    # Hệ thống thoại song công toàn phần (Full-duplex)
│   │   ├── Cargo.toml                 # cpal, sonora (AEC3), ort, rodio, rubato
│   │   └── src/                       # vad.rs, aec.rs, denoise.rs, stt.rs, tts.rs, viseme.rs
│   └── liva-tools/                    # Bộ công cụ kiểm thử, benchmark & chẩn đoán hợp nhất
│       ├── Cargo.toml                 # clap v4, console reporting
│       └── src/                       # main.rs (subcommands: probe-db, probe-vad, bench-llm, inspect-wal)
├── liva-native-core/                  # Facade dàn xếp chính, giữ tương thích ngược API
│   ├── Cargo.toml                     # Phụ thuộc vào các crates/*
│   ├── src/lib.rs                     # AppState, handle_command, bootloader
│   └── tests/
│       └── harness.rs                 # 1 Unified Integration Test Harness (Submodules thay thế 78 test binaries)
├── liva-desktop/                      # Vỏ ứng dụng Tauri v2
│   ├── src-tauri/
│   │   └── src/lib.rs                 # Tauri native commands, unified channels, mouse hook
│   └── tauri.conf.json
└── liva-ui/                           # Giao diện Vue 3 tách module
    ├── src/
    │   ├── App.vue                    # Root shell
    │   ├── components/
    │   │   ├── avatar/                # AvatarCanvas.vue (OffscreenCanvas Web Worker)
    │   │   ├── dialogue/              # DialogueBubble.vue, TypingIndicator.vue
    │   │   └── widgets/               # ToolCard.vue, HitlConfirmDialog.vue
    │   └── workers/
    │       └── avatarRenderer.worker.ts # Three.js render loop & spring bones hoàn toàn độc lập
```

---

### 3.2 Động Cơ Lưu Trữ Tối Ưu Hóa (`liva-storage`)

#### 1. Chuẩn hóa Pragmas & Cố định kích thước trang:
Thiết lập lại toàn bộ các tham số SQLite trong `liva-storage/src/pragmas.rs`:
- `PRAGMA page_size = 4096;`: Khớp hoàn hảo với kích thước phân trang bộ nhớ 4KB của kiến trúc CPU x86_64 và NTFS cluster, triệt tiêu 100% độ phóng đại ghi (write amplification).
- `PRAGMA cache_size = -2000;`: Giới hạn bộ nhớ đệm ở mức $2.000\text{ KiB} \approx 1.95\text{ MiB}$ cho mỗi kết nối đọc, và `-4000` ($3.9\text{ MiB}$) cho kết nối ghi.
  $$\text{Tổng RAM bộ đệm} = 3.9\text{ MiB} + (4 \times 1.95\text{ MiB}) \approx 11.7\text{ MiB}$$
  **Tiết kiệm ngay lập tức hơn 300.0 MB RAM quý giá cho hệ điều hành.**
- `PRAGMA wal_autocheckpoint = 1000;`: Giới hạn kích thước file WAL ở mức $1.000 \times 4\text{ KB} = 4.0\text{ MB}$, ngăn ngừa triệt để hiện tượng file `-wal` phình to nhiều chục MB.
- `PRAGMA synchronous = NORMAL;`: Đảm bảo an toàn cơ sở dữ liệu khi mất điện đột ngột trong khi đạt thông lượng ghi đĩa tối ưu.

#### 2. Gom cụm giao dịch tự động (Micro-Batched Writer Actor):
Thay thế cơ chế ghi từng dòng đơn lẻ bằng bộ gom cụm micro-batching trong `DbActor`:
- Duy trì một kết nối ghi độc quyền duy nhất (`rusqlite::Connection`), loại bỏ hoàn toàn chi phí checkout và ping connection từ pool `r2d2`.
- Cơ chế gom cụm linh hoạt theo 2 điều kiện kích hoạt:
  1. **Ngưỡng dung lượng:** Gom đủ **50 câu lệnh ghi**.
  2. **Ngưỡng thời gian:** Độ trễ chờ tối đa **5 milliseconds**.
- Toàn bộ 50 câu lệnh được thực thi bên trong một khối duy nhất `BEGIN IMMEDIATE ... COMMIT`.
- Thông lượng ghi tăng từ 25–40 transactions/giây lên tới **1.500–3.000 operations/giây**, triệt tiêu tình trạng nghẽn I/O đĩa.

#### 3. Chỉ mục Tiền tố Trie trong RAM cho Active Recall:
Xóa bỏ hoàn toàn việc quét toàn bảng và giải mã AES-256-GCM hàng loạt trong `active_recall.rs`:
- Khi khởi động ứng dụng, `ActiveRecallManager` nạp danh sách các khóa và siêu dữ liệu (plaintext keywords) vào một cây tiền tố **Radix Trie** trong RAM (tiêu tốn $< 150\text{ KB}$ bộ nhớ).
- Khi người dùng nói một câu, hệ thống quét câu nói qua Trie trong thời gian $< 0.5\text{ ms}$ để tìm các khóa khớp.
- **Chỉ giải mã AES-256-GCM đối với những facts thực sự khớp từ khóa** (thường từ 1 đến 3 bản ghi thay vì 2.000 bản ghi). Độ trễ xử lý giảm từ 150ms xuống **dưới 2ms**.

#### 4. Đồ thị CSR HippoRAG Phi Khóa với Cơ Chế Đổi Đệm Kép (`ArcSwap`):
Khắc phục tình trạng xung đột khóa ghi `RwLock::write` khi cập nhật tri thức L3:
- Lưu trữ đồ thị dưới dạng `arc_swap::ArcSwap<CsrGraph>`.
- Các luồng đọc thuật toán lan truyền Personalized PageRank chỉ cần tải con trỏ nguyên tử `graph_swap.load()`, thực thi SpMV hoàn toàn phi khóa (Lock-Free Read).
- Khi có luồng ghi tri thức L3 mới, đồ thị mới được xây dựng ngầm trên một bản sao (shadow buffer). Sau khi hoàn tất `compile_csr()`, con trỏ được tráo đổi nguyên tử chỉ trong vài nano giây thông qua `graph_swap.store(Arc::new(new_graph))`. Thuật toán RAG không bao giờ bị rơi vào trạng thái rỗng do cờ `dirty`.

#### 5. Sửa lỗi truy vấn lọc vector trong `sqlite-vec`:
Khắc phục tình trạng lọc subquery làm mất kết quả tương đồng:
- Thay vì sử dụng heuristic `v.k = top_k * 3` và mệnh đề `IN (SELECT id FROM vectors_meta WHERE ...)`, chuyển sang cấu trúc bảng phụ kết hợp chỉ mục: tách biệt vector index theo domain hoặc sử dụng bảng ảo vector kết hợp metadata partition, đảm bảo tỷ lệ thu hồi chính xác (Recall = 100%).

---

### 3.3 Mô Hình Đồng Quy Hướng Tác Tử Cho LLM (`liva-llm`)

Thay thế khóa toàn cục `tokio::sync::Mutex<LlamaRouterManager>` bằng mô hình **Actor Pattern**:

```
                       ┌────────────────────────────────────────────────────────┐
                       │                   Tokio Async Tasks                    │
                       │           (Tauri Commands, Voice, Web, Widgets)        │
                       └───────────┬────────────────┬───────────────────────────┘
                                   │                │
             High Priority         │                │ Normal Priority
             (Health, Cancel)      │                │ (User Dialogue Turns)
                                   ▼                ▼
                       ┌────────────────────────────────────────────────────────┐
                       │          LlmActor Bounded Priority MPSC Queue          │
                       │             (Capacity: 128 Requests)                   │
                       └───────────────────────────┬────────────────────────────┘
                                                   │
                                                   │ Sequential Worker Dispatch
                                                   ▼
                       ┌────────────────────────────────────────────────────────┐
                       │            Dedicated OS Worker Thread                  │
                       │           "liva-llama-inference-worker"                │
                       │                                                        │
                       │   ┌────────────────────────────────────────────────┐   │
                       │   │           LlamaRouterManager Engine            │   │
                       │   │  - Non-blocking token generator loop           │   │
                       │   │  - Atomic cancellation flag checking           │   │
                       │   │  - mmap weight retention & zero-copy tensors   │   │
                       │   └────────────────────────┬───────────────────────┘   │
                       └────────────────────────────┼───────────────────────────┘
                                                    │
                                                    │ Broadcast Chunk Stream
                                                    ▼
                       ┌────────────────────────────────────────────────────────┐
                       │              tokio::sync::broadcast<TokenChunk>        │
                       │     (Subscribed by Desktop UI, Voice TTS, Logs)        │
                       └────────────────────────────────────────────────────────┘
```

#### 1. Phân luồng hàng đợi ưu tiên 3 cấp:
`LlmActor` giao tiếp qua kênh `mpsc::channel` có hỗ trợ phân cấp ưu tiên:
- **Ưu tiên 1 (`Priority::High`):** Các lệnh kiểm tra sức khỏe (`HealthCheck`), truy vấn thông số ngữ cảnh, và cờ hủy dòng suy luận (`CancelInference`). Các lệnh này được xử lý ngay lập tức giữa các chu kỳ token mà không bị xếp hàng sau các prompt sinh văn bản.
- **Ưu tiên 2 (`Priority::Normal`):** Các lượt tương tác thoại và chat trực tiếp từ người dùng. Các token sinh ra được phát qua kênh `tokio::sync::broadcast` cho phép UI hiển thị văn bản đồng thời TTS tổng hợp giọng nói song song.
- **Ưu tiên 3 (`Priority::Low`):** Các tác vụ nền: tạo vector nhúng (embedding), gom cụm bộ nhớ dài hạn, tóm tắt nhật ký.

#### 2. Tách biệt hoàn toàn động cơ Embedding:
Chuyển đổi hoàn toàn `AppState.embedder` sang phiên bản phi khóa chia sẻ `Arc<EmbeddingEngine>` sử dụng khả năng thực thi luồng an toàn đa luồng của ONNX Runtime (`ort::Session::run(&self)`). Tác vụ embedding chạy trên các luồng worker độc lập, hoàn toàn không phụ thuộc và không bị chặn bởi động cơ sinh văn bản llama.cpp.

#### 3. Bộ nhớ đệm tin cậy mô hình & Tránh băm SHA-256 lặp lại:
- Khi tải hoặc cài đặt mô hình, mã băm SHA-256 được tính toán một lần duy nhất và lưu vào bảng `artifact_trust` trong SQLite cùng với thông tin kích thước file và thời gian sửa đổi cuối cùng (`mtime`).
- Khi khởi động lại hoặc nạp lại mô hình sau thời gian rảnh, hệ thống chỉ kiểm tra nhanh `file_size` và `mtime` ($< 1\text{ ms}$) thay vì đọc toàn bộ file 4GB–7GB từ SSD để băm SHA-256. Hiện tượng đứng hình 3–6 giây khi thức giấc được xóa bỏ hoàn toàn.

---

### 3.4 Hiện Đại Hóa Hệ Thống Build & Quy Trình CI/CD

#### 1. Cấu hình Tối ưu Hóa Release tại Gốc (`Cargo.toml`):
Khai báo khối tối ưu hóa phát hành toàn cục nhằm cắt giảm tối đa kích thước nhị phân và tăng tốc độ thực thi:
```toml
# Cargo.toml
[profile.release]
opt-level = 3
lto = "thin"              # Bật tối ưu hóa liên kết ThinLTO liên crate
codegen-units = 1         # Tối đa hóa cơ hội inline code của LLVM
panic = "abort"           # Cắt bỏ toàn bộ bảng unwind metadata
strip = "symbols"         # Lược bỏ toàn bộ debug symbols khỏi binary
debug = false
incremental = false
```
Cấu hình này kết hợp với việc nén tài nguyên giúp giảm kích thước gói cài đặt `LIVA_1.0.0_x64-setup.exe` từ **291.2 MB xuống dưới 85.0 MB**.

#### 2. Kế thừa Thuộc tính Workspace (`[workspace.dependencies]` & `[workspace.lints]`):
Toàn bộ phiên bản thư viện dùng chung (`tokio`, `serde`, `rusqlite`, `tracing`, `windows-sys`, `ort`, `thiserror`) được chuẩn hóa tập trung tại root `Cargo.toml`. Các crate con chỉ cần khai báo `tokio.workspace = true`, ngăn ngừa 100% tình trạng trùng lặp phiên bản crate phát hiện qua `cargo tree -d`.

#### 3. Bộ Thu Gọn Test Harness & Bộ Công Cụ Chẩn Đoán Duy Nhất:
- **Thu gọn 78 Integration Tests thành 1 Harness duy nhất:** Chuyển 78 file rời rạc trong `tests/*.rs` thành các submodules bên trong `tests/harness.rs`:
  ```rust
  // liva-native-core/tests/harness.rs
  mod memory_tests;
  mod voice_tests;
  mod agent_tests;
  mod security_tests;
  ```
  Số lượng binary cần link giảm từ 78 xuống còn **1 binary duy nhất**, giải phóng hơn 40 GB rác build trong `target/debug/deps` và giảm thời gian link cục bộ từ hàng phút xuống còn dưới 5 giây.
- **Thu gọn 26 Binary Probes thành CLI `liva-tools`:** Gom toàn bộ các file probe tại `src/bin/*.rs` vào một binary duy nhất `crates/liva-tools`, sử dụng `clap` định tuyến subcommand (`liva-tools probe memory`, `liva-tools bench voice`). Số lượng binary trong crate chính giảm về 0.

#### 4. Đường Ống CI/CD Song Song Dạng DAG 4 Nhánh:
Tái cấu trúc file `.github/workflows/test.yml` từ 25 bước tuần tự thành mô hình DAG phân tán:

```
                                      ┌────────────────────────┐
                                      │     GitHub Trigger     │
                                      │    (Push / PR Main)    │
                                      └───────────┬────────────┘
                                                  │
                 ┌────────────────────────────────┼────────────────────────────────┐
                 │                                │                                │
                 ▼                                ▼                                ▼
   ┌───────────────────────────┐    ┌───────────────────────────┐    ┌───────────────────────────┐
   │       Job 1: Lints        │    │    Job 2: Web & UI        │    │   Job 3: Rust Quality     │
   │      (ubuntu-latest)      │    │    (ubuntu-latest)        │    │     (ubuntu-latest)       │
   │  - Markdown / Citations   │    │  - npm ci (Root hoisted)  │    │  - cargo check (core)     │
   │  - ShellCheck / YAML      │    │  - vue-tsc --noEmit       │    │  - cargo clippy (-D warn) │
   │  - License Compliance     │    │  - Vitest Unit Tests      │    │  - Pre-built cargo-deny   │
   │  - Thời gian: ~45 giây    │    │  - UI Artifact Build      │    │  - Thời gian: ~70 giây    │
   │                           │    │  - Thời gian: ~60 giây    │    │                           │
   └─────────────┬─────────────┘    └─────────────┬─────────────┘    └─────────────┬─────────────┘
                 │                                │                                │
                 └────────────────────────────────┼────────────────────────────────┘
                                                  │ All Fast Checks Passed
                                                  ▼
                                    ┌───────────────────────────┐
                                    │ Job 4: Native & Packaging │
                                    │      (windows-latest)     │
                                    │  - sccache acceleration   │
                                    │  - lld-link fast linker   │
                                    │  - cargo test (harness)   │
                                    │  - Tauri v2 Bundle (NSIS) │
                                    │  - Thời gian: ~5.5 phút   │
                                    └───────────────────────────┘
```

- **Sử dụng pre-built binary:** Thay thế `cargo install cargo-deny` bằng GitHub Action chính chủ `EmbarkStudios/cargo-deny-action@v2` (chạy tức thì trong 3 giây, tiết kiệm 4 phút biên dịch mã nguồn).
- **Phục hồi bộ nhớ đệm an toàn:** Chỉ cache thư mục `~/.cargo/registry`, `~/.cargo/git` và bộ nhớ đệm `sccache`. Tuyệt đối không cache toàn bộ thư mục `target/` để không bao giờ chạm trần 10 GB cache của GitHub.

---

### 3.5 Giao Tiếp Hợp Nhất 100% Tauri v2 Channels

- Khai tử hoàn toàn máy chủ TCP WebSocket (`websocket.rs` và `tokio-tungstenite`), loại bỏ 2.146 dòng mã điều vận trùng lặp và xóa bỏ hoàn toàn rủi ro xung đột cổng mạng cục bộ.
- Chuẩn hóa giao tiếp bằng **Tauri v2 IPC Channels**:
  - Dòng token văn bản LLM phát trực tiếp vào `tauri::ipc::Channel<String>`.
  - Gói dữ liệu ngữ âm chuyển động môi (visemes) và các mẩu âm thanh PCM phát qua kênh nhị phân với độ trễ chuyển giao $< 0.1\text{ ms}$.
- Sử dụng công cụ sinh kiểu tự động `specta` / `tauri-specta` để tự động xuất khẩu các cấu trúc Rust từ `crates/liva-core-types` sang các file định nghĩa kiểu TypeScript (`bindings.ts`), đảm bảo tính đồng bộ 100% về kiểu dữ liệu tại thời điểm biên dịch (Compile-time Type Safety).

---

### 3.6 Tách Biệt Kết Xuất Giao Diện 3D Avatar (OffscreenCanvas Web Worker)

- Phân rã file "God Component" `WidgetApp.vue` (2.105 dòng) thành các thành phần nguyên tử:
  - `AvatarViewport.vue` ($< 200$ dòng): Chỉ giữ thẻ `<canvas>` và quản lý vòng đời Worker.
  - `DialogueBubble.vue` ($< 250$ dòng): Hiển thị phản hồi văn bản và markdown streaming.
  - `ActionOverlay.vue` ($< 180$ dòng): Thẻ phê duyệt quyền con người (HITL) và đồng hồ trạng thái.
- Đưa toàn bộ vòng lặp kết xuất Three.js và thuật toán vật lý lò xo (spring bones) ra khỏi luồng chính bằng `canvas.transferControlToOffscreen()` chạy trong `avatarRenderer.worker.ts`. Luồng chính của trình duyệt hoàn toàn tự do để xử lý sự kiện DOM và streaming dữ liệu mà không bao giờ làm rớt khung hình avatar (khóa cứng 60 FPS).
- Thay thế vòng lặp thăm dò chuột 30ms sleep trong tiến trình native bằng việc bắt hook chuột cấp thấp của Windows (`WH_MOUSE_LL`) dạng sự kiện (event-driven), giảm mức sử dụng CPU lúc rảnh rỗi từ 3.2% xuống gần 0.0%.

---

## 4. Bảng Đối Soát Toàn Diện: Kiến Trúc Cũ vs. Kiến Trúc Remake

Dưới đây là bảng so sánh chi tiết ưu và nhược điểm giữa Kiến trúc Hiện tại và Kiến trúc Remake Đề xuất trên tất cả các khía cạnh kỹ thuật:

### Bảng 1: So sánh theo Chiều Kỹ thuật & Kiến trúc Hệ thống

| Tiêu chí Đánh giá | Kiến trúc Hiện tại (Old Architecture) | Kiến trúc Remake Đề xuất (Remake Architecture) | Ưu điểm Cũ vs. Mới | Nhược điểm Cũ vs. Mới |
|---|---|---|---|---|
| **Cấu trúc Không gian Crate** | Monolith: Toàn bộ mã nguồn backend nằm trong 1 crate khổng lồ `liva-native-core` kèm 26 binary probes. | Multi-Crate Workspace: `crates/` phân tách rõ ràng (`core-types`, `storage`, `llm`, `voice`, `tools`). | Cũ: Dễ tìm code trong 1 crate duy nhất.<br>Mới: Ranh giới rõ ràng, biên dịch độc lập, tái sử dụng tối đa. | Cũ: Compile time cực lâu, blast radius lan rộng.<br>Mới: Cần quản lý cấu hình workspace dependencies. |
| **Đồng quy Động cơ LLM** | Khóa toàn cục `tokio::sync::Mutex<LlamaRouterManager>` bao bọc toàn bộ chu kỳ sinh văn bản 3–15s. | Mô hình Tác tử (`LlmActor`) với hàng đợi ưu tiên 3 cấp (High/Normal/Low) và luồng worker chuyên trách. | Cũ: Viết code gọi hàm đơn giản, ít boilerplate.<br>Mới: Không nghẽn đầu hàng, health check tức thì, hủy luồng phản hồi ngay lập tức. | Cũ: Treo toàn bộ hệ thống khi LLM đang sinh chữ.<br>Mới: Cần kiến trúc hướng thông điệp (MPSC channels). |
| **Động cơ Nhúng Vector (Embedding)** | Chạy chung khóa hoặc phụ thuộc vào điều phối của router LLM. | Tách biệt hoàn toàn qua `Arc<EmbeddingEngine>` phi khóa đa luồng (ORT session thread-safe). | Cũ: Ít thành phần quản lý.<br>Mới: Tạo embedding song song 100% với việc sinh text, không cạnh tranh khóa. | Cũ: Rủi ro tranh chấp tài nguyên.<br>Mới: Tiêu tốn thêm một lượng nhỏ RAM cho ORT session. |
| **Bộ đệm Trang SQLite (Page Cache)** | `cache_size = -64000` (62.5 MiB/conn) kết hợp `page_size = 32768`. Tốn 312.5 MB RAM chỉ cho cache. | `cache_size = -2000` (~1.95 MiB/conn), `page_size = 4096`. Tổng cache SQLite chỉ tốn ~11.7 MB RAM. | Cũ: Cache lớn có thể giữ được nhiều trang nếu DB to.<br>Mới: Tiết kiệm ngay hơn 300 MB RAM cho hệ điều hành và LLM. | Cũ: Phá vỡ ngân sách RAM 3.0GB của máy trạm.<br>Mới: Cần chạy lệnh `VACUUM` một lần để áp dụng page_size mới. |
| **Giao dịch Cơ sở Dữ liệu (Writes)** | Unbatched: Mỗi thao tác ghi mở và commit 1 transaction riêng lẻ (thực thi 50 commits cho 50 rows). | Micro-Batching: Tự động gom cụm tối đa 50 operations hoặc mỗi 5ms thành 1 khối commit duy nhất. | Cũ: Ghi tức thời từng dòng đơn lẻ.<br>Mới: Tăng thông lượng ghi từ 30 lên 2.500 ops/s, giảm 95% số lần flush đĩa. | Cũ: Nghẽn I/O SSD nghiêm trọng khi có tải dồn.<br>Mới: Dữ liệu ghi có thể trễ tối đa 5ms (trong ngưỡng an toàn). |
| **Quét Tri Thức Active Recall** | Quét toàn bảng `facts`, giải mã HKDF/AES-256-GCM hàng ngàn facts trên mỗi lượt hội thoại. | Chỉ mục Tiền tố Radix Trie trong RAM, chỉ giải mã đúng 1–3 facts khớp từ khóa trong câu nói. | Cũ: Không cần bảo trì cấu trúc dữ liệu phụ trong RAM.<br>Mới: Độ trễ trừng phạt giảm từ 150ms xuống <2ms, triệt tiêu xung đột CPU. | Cũ: CPU bị nghẽn và nóng máy liên tục.<br>Mới: Tốn thêm khoảng ~150 KB RAM để lưu cây từ khóa Trie. |
| **Bộ Nhớ Đồ Thị HippoRAG** | Tái biên dịch toàn bộ đồ thị CSR dưới khóa ghi `RwLock::write`. Đồ thị rỗng nếu cờ `dirty = true`. | Đổi đệm kép nguyên tử `ArcSwap<CsrGraph>`. Thuật toán RAG đọc hoàn toàn Lock-Free. | Cũ: Cấu trúc dữ liệu đơn giản.<br>Mới: Đọc đồ thị không bao giờ bị chặn, không bao giờ mất tri thức khi đang học mới. | Cũ: Xung đột khóa đọc/ghi làm đơ pipeline RAG.<br>Mới: Tốn thêm bộ nhớ tạm trong lúc xây dựng shadow buffer. |
| **Hạ tầng Giao tiếp Desktop IPC** | Trùng lặp kép: Chạy song song cả Tauri Native IPC lẫn TCP WebSocket server nội bộ (`websocket.rs` 2.146 dòng). | Hợp nhất 100% Tauri v2 IPC Channels (truyền nhị phân âm thanh và streaming text). | Cũ: Cho phép kết nối thử nghiệm từ trình duyệt ngoài qua ws://.<br>Mới: Xóa bỏ 2.100 dòng code thừa, triệt tiêu xung đột port và double serialization. | Cũ: Lãng phí tài nguyên, dễ lỗi rớt mạng loopback.<br>Mới: Mọi giao tiếp phụ thuộc vào môi trường Tauri. |
| **Kết xuất Giao diện 3D Avatar** | Three.js, spring bones, audio worklet và Vue DOM reactive chạy chung trên luồng JS chính. | Đưa toàn bộ Three.js và tính toán vật lý xương vào OffscreenCanvas Web Worker. | Cũ: Tất cả logic viết chung trong 1 component dễ truy cập biến.<br>Mới: Avatar khóa cứng 60 FPS mượt mà, không giật hình khi LLM stream text. | Cũ: Sụt giảm FPS nghiêm trọng khi chat thoại.<br>Mới: Giao tiếp giữa UI và Worker cần qua postMessage. |
| **Quy trình Build & Pipeline CI/CD** | 1 Job Windows duy nhất chạy 25 bước tuần tự; 106 binaries độc lập; `target/` phình 94 GB. | 4-Job Parallel DAG; 1 Test Harness duy nhất; pre-built tooling; `target/` sạch gọn. | Cũ: Cấu hình CI đơn giản trong 1 file phẳng.<br>Mới: Tốc độ CI từ >60 phút giảm còn 5.5 phút; giải phóng 45 GB ổ cứng cho lập trình viên. | Cũ: Tốn chi phí máy ảo đắt đỏ, cache vỡ liên tục.<br>Mới: Cần cấu hình ma trận job phụ thuộc (needs: [lints, web]). |
| **Kích thước Gói Cài đặt Release** | `LIVA_1.0.0_x64-setup.exe` nặng 291.2 MB do thiếu LTO và chưa strip debug symbols. | Gói cài đặt $< 85.0\text{ MB}$ nhờ kích hoạt ThinLTO, codegen-units=1, panic=abort và strip symbols. | Cũ: Không mất thời gian tối ưu hóa khi build release.<br>Mới: Tải về cực nhanh, tiết kiệm băng thông mạng và dung lượng lưu trữ máy trạm. | Cũ: Kích thước phân phối quá nặng nề.<br>Mới: Thời gian build bản release cuối cùng sẽ lâu hơn do ThinLTO. |

---

### Bảng 2: Định lượng Số liệu Đo lường Hiệu năng (Quantitative Benchmark Comparison)

| Chỉ số Hiệu năng (Metric) | Kiến trúc Cũ (Baseline Đo đạc) | Kiến trúc Remake (Mục tiêu Thiết kế) | Mức độ Cải thiện |
|---|---|---|---|
| **Kích thước thư mục `target/` cục bộ** | **94.34 GB** (chứa 9.772 artifacts) | **< 12.0 GB** | **Giảm 87.3% dung lượng đĩa** |
| **Số lượng Executable Binaries cần link** | **106 binaries** (78 tests + 26 bins + 2) | **2 binaries** (1 core harness + 1 tools CLI) | **Giảm 98.1% số mục tiêu link** |
| **Thời gian Link cục bộ khi sửa code (Touch)** | 20 – 60 giây (với MSVC `link.exe`) | **< 3 giây** (với `lld-link` & 1 harness) | **Nhanh hơn 10x – 20x** |
| **Tổng thời gian chạy Pipeline CI/CD** | > 60 phút (tuần tự trên Windows) | **~5.5 phút** (DAG song song 4 nhánh) | **Rút ngắn 91% thời gian chờ CI** |
| **Dung lượng RAM chiếm dụng bởi SQLite Cache** | **312.5 MB** (5 connections × 62.5 MB) | **~11.7 MB** (định mức calibrated) | **Tiết kiệm 300.8 MB RAM (96.2%)** |
| **Độ trễ xử lý Active Recall mỗi lượt chat** | 50 – 250 ms (quét toàn bảng + AES decrypt) | **< 2 ms** (quét tiền tố Trie trong RAM) | **Nhanh hơn 50x – 100x** |
| **Thông lượng ghi Cơ sở dữ liệu (Transactions/s)**| 20 – 50 tx/giây (unbatched disk sync) | **1.500 – 3.000 ops/giây** (5ms micro-batch) | **Tăng thông lượng gấp 50 lần** |
| **Độ trễ Kiểm tra Sức khỏe khi LLM đang sinh** | Bị nghẽn 3.000 – 15.000 ms (Mutex locked) | **< 5 ms** (Ưu tiên cao qua Actor channel) | **Triệt tiêu 100% hiện tượng đơ UI** |
| **Tốc độ khung hình Avatar 3D khi hội thoại** | Sụt giảm xuống 20 – 35 FPS | **Khóa cứng ổn định 60 FPS** | **Trải nghiệm hình ảnh mượt mà** |
| **Dung lượng File Cài đặt Windows Release** | **291.2 MB** | **< 85.0 MB** | **Giảm 70.8% kích thước tải về** |

---

## 5. Lộ Trình Triển Khai & Kế Hoạch Hiện Thực Hóa (Implementation Roadmap)

Để đảm bảo quá trình chuyển đổi diễn ra an toàn, không gián đoạn và tuân thủ tuyệt đối quy tắc **Zero-Regression**, kế hoạch Remake được tổ chức thành 4 cột mốc (Milestones) tuần tự:

### Cột mốc 1: Hạ Tầng Không Gian Làm Việc & Hiện Đại Hóa CI/CD (M1)
1. Cấu hình root `Cargo.toml`: thiết lập `[workspace.dependencies]`, `[workspace.lints]`, và cấu hình tối ưu hóa `[profile.release]`.
2. Thiết lập `.cargo/config.toml` kích hoạt trình liên kết nhanh `lld-link` trên Windows.
3. Gom 78 integration tests thành 1 harness duy nhất `liva-native-core/tests/harness.rs`.
4. Gom 26 binary probes thành CLI `crates/liva-tools`.
5. Tái cấu trúc file `.github/workflows/test.yml` thành DAG 4 nhánh song song, kích hoạt pre-compiled `cargo-deny` và cache an toàn.
6. Vá các lỗ hổng bảo mật `npm audit` thông qua cập nhật `package.json` overrides.

### Cột mốc 2: Hiện Đại Hóa Động Cơ Lưu Trữ & Cơ Sở Dữ Liệu (M2)
1. Cập nhật `liva-storage/src/pragmas.rs`: áp dụng `page_size = 4096`, `cache_size = -2000`, `wal_autocheckpoint = 1000`.
2. Triển khai bộ gom cụm micro-batching trong `DbActor` (ngưỡng 50 commands / 5ms commit).
3. Hiện thực hóa cây tiền tố Radix Trie cho `ActiveRecallManager`, loại bỏ vòng lặp quét giải mã toàn bảng.
4. Bổ sung các chỉ mục thứ cấp còn thiếu (`facts(sourceTurnId)`, `events(timestamp DESC)`).
5. Thay thế `RwLock` của đồ thị CSR bằng con trỏ nguyên tử `ArcSwap<CsrGraph>`.

### Cột mốc 3: Đồng Quy Tác Tử LLM & Hợp Nhất Giao Tiếp (M3)
1. Xây dựng `LlmActor` với hàng đợi ưu tiên và kênh MPSC phi khóa, bãi bỏ `tokio::sync::Mutex<LlamaRouterManager>`.
2. Lưu trữ mã băm SHA-256 tin cậy của mô hình trong SQLite để khởi động mô hình tức thì sau thời gian rảnh rỗi.
3. Xóa bỏ hoàn toàn máy chủ WebSocket `websocket.rs`, chuyển toàn bộ luồng truyền âm thanh/token sang Tauri v2 IPC Channels.
4. Tách biệt `WidgetApp.vue` thành các component nhỏ và chuyển Three.js vào OffscreenCanvas Web Worker.

### Cột mốc 4: Kiểm Thử Xác Minh Toàn Diện & Đóng Gói Phát Hành (M4)
1. Chạy toàn bộ bộ kiểm thử tự động tuần tự với tài nguyên giới hạn: `cargo test -j 2 -- --test-threads 2`.
2. Kiểm tra biên dịch sạch không cảnh báo: `cargo clippy -j 2 -- -D warnings`.
3. Kiểm tra kiểm định kiểu giao diện: `npm run typecheck` và `npm run test:coverage`.
4. Đóng gói bộ cài đặt Windows qua Tauri CLI và xác thực kích thước file $< 85\text{ MB}$.
5. Cập nhật tài liệu hướng dẫn vận hành kiến trúc mới.

---

## 6. Kết Luận

Kiến trúc Remake LIVA Thế Hệ Mới (LIVA Next-Gen Architecture) là giải pháp toàn diện và triệt để nhất cho các vấn đề tồn đọng của hệ thống. Bằng cách loại bỏ các nút thắt cổ chai về khóa toàn cục, chuẩn hóa bộ nhớ đệm cơ sở dữ liệu, phân rã không gian làm việc đa crate và hiện đại hóa quy trình build/CI, dự án LIVA sẽ đạt được sự cân bằng hoàn hảo:
- **Tối ưu hóa hiệu năng máy trạm:** Vận hành trơn tru trong giới hạn 3.0 GB RAM và 5.1 GB VRAM.
- **Trải nghiệm người dùng vượt trội:** Phản hồi thoại $< 200\text{ ms}$, avatar 3D 60 FPS không giật lag.
- **Năng suất phát triển phần mềm tăng vọt:** Chu kỳ build/test nhanh hơn 10 lần, thời gian kiểm thử CI giảm 90%, mã nguồn sáng sủa, an toàn và sẵn sàng cho môi trường sản xuất quy mô lớn.
