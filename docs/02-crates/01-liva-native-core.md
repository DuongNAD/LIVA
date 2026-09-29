---
title: "Tài liệu Kỹ thuật Crate: liva-native-core"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - liva-native-core/Cargo.toml
  - liva-native-core/src/lib.rs
  - liva-native-core/src/boot.rs
---

# Tài liệu Kỹ thuật Crate: liva-native-core

## 1. Tổng quan và Vai trò Kiến trúc

`liva-native-core` là crate trung tâm đóng vai trò **Orchestration Facade** của toàn bộ hệ thống LIVA. Crate này quản lý trạng thái chia sẻ toàn cục (`AppState`), khởi tạo chu trình khởi động an toàn (`boot.rs`), định tuyến các lệnh nghiệp vụ (`handle_command_as`), và tích hợp các phân hệ trí tuệ nhân tạo (thoại full-duplex, thị giác máy tính, điều phối tác tử và máy chủ MCP).

Mọi kết nối từ ứng dụng máy tính (`liva-desktop`) hoặc các công cụ dòng lệnh (`liva-tools`) đều tương tác trực tiếp với crate này thông qua API thư viện hoặc hàm thực thi nhị phân.

---

## 2. Cấu trúc Trạng thái Toàn cục (`AppState`)

Trạng thái toàn cục được quản lý tập trung và chia sẻ an toàn giữa các luồng:

```rust
pub struct AppState {
    pub db: DatabasePool,
    pub crypto: EncryptionEngine,
    pub stt: tokio::sync::Mutex<SttManager>,
    pub tts: tokio::sync::Mutex<Option<TtsManager>>,
    pub tts_player: TtsAudioPlayer,
    pub llm: liva_llm::LlmActorHandle,
    pub vad: tokio::sync::Mutex<Option<webrtc::vad::VadEngine>>,
    pub denoiser: tokio::sync::Mutex<Option<webrtc::denoise::GtcrnDenoiser>>,
    pub turn_shadow: tokio::sync::Mutex<Option<webrtc::turn_shadow::SmartTurnClassifier>>,
    pub aec: tokio::sync::Mutex<Option<webrtc::aec::SelfEchoCanceller>>,
    pub mcp_server: Arc<mcp::server::NativeMcpServer>,
    pub vision: tokio::sync::Mutex<VisionManager>,
    pub embedder: Option<Arc<llm::embedder::EmbeddingEngine>>,
    pub active_recall: Arc<active_recall::ActiveRecallManager>,
    pub cua: Arc<liva_cua::CuaEngine>,
}
```

### Các thành phần chính trong `AppState`:
- **`db` (`DatabasePool`)**: Quản lý nhóm kết nối SQLite WAL (1 writer thông qua `DbActorHandle` và 4 readers), đồng thời duy trì ảnh chụp đồ thị tri thức trong RAM `csr_graph: Arc<ArcSwap<CsrGraph>>`.
- **`crypto` (`EncryptionEngine`)**: Động cơ mã hóa đối xứng AES-256-GCM v2, bảo vệ dữ liệu nhạy cảm trước khi ghi xuống đĩa cứng.
- **`llm` (`liva_llm::LlmActorHandle`)**: Kênh giao tiếp với Actor suy luận ngôn ngữ, điều phối hàng đợi ưu tiên và hỗ trợ hủy tác vụ sớm.
- **`stt` / `tts` / `tts_player`**: Động cơ nhận dạng giọng nói tiếng Việt/Anh và phát âm thanh tổng hợp đa luồng.
- **`vad` / `denoiser` / `turn_shadow` / `aec`**: Chuỗi xử lý tín hiệu âm thanh kỹ thuật số (DSP) phục vụ thoại hai chiều full-duplex.
- **`vision` (`VisionManager`)**: Quản lý mô hình Qwen3-VL-2B và tác vụ chụp màn hình, được giám sát bởi `VisualGovernor`.
- **`active_recall`**: Cây tiền tố Radix Trie đánh chặn hội thoại để truy hồi tri thức tức thời.
- **`cua` (`Arc<liva_cua::CuaEngine>`)**: Động cơ tự động hóa giao diện desktop Win32.

---

## 3. Quy trình Khởi động Hệ thống (`boot.rs`)

Quá trình khởi động `build_app_state()` diễn ra tuần tự theo các bước kiểm soát nghiêm ngặt:
1. **Kiểm tra đường dẫn và tệp cấu hình (`paths.rs`)**: Xác định thư mục dữ liệu ứng dụng (`%APPDATA%/LIVA` hoặc đường dẫn phát triển cục bộ).
2. **Khởi tạo Kho khóa (Stronghold Vault)**: Nạp hoặc tạo mới khóa mã hóa chủ (Master Key) thông qua Argon2id.
3. **Mở kết nối Cơ sở dữ liệu và Áp dụng Migration**: Kích hoạt `DatabasePool`, áp dụng 20 bảng cơ sở dữ liệu và nạp đồ thị tri thức L3 vào `CsrGraph`.
4. **Nạp Cây tiền tố Radix Trie**: Đọc các khóa sự kiện và tri thức chủ động vào bộ nhớ đệm L0 `FactTrie`.
5. **Khởi động Actor LlmActor**: Tạo kênh MPSC và luồng nền cho suy luận ngôn ngữ.
6. **Kiểm tra tính sẵn sàng của mô hình AI**: Khởi tạo động cơ nhúng vector ONNX, mô hình VAD Silero và nhận diện giọng nói STT.

---

## 4. Định tuyến Lệnh Nghiệp vụ (`handle_command_as`)

Hàm `handle_command_as` tiếp nhận lệnh từ tầng IPC hoặc CLI, xác thực quyền hạn và chuyển tiếp tới các module chuyên trách:
- `vision:*`: Chụp màn hình, trích xuất văn bản OCR, phân tích vùng quan tâm ROI (`commands::vision`).
- `voice:*`: Đăng ký kênh âm thanh, truyền khối microphone, thăm dò từ khóa kích hoạt wake-word (`commands::voice`).
- `cua:*`: Thực thi thao tác click chuột, nhấn phím ảo, di chuyển cửa sổ ứng dụng desktop (`commands::cua`).
- `chat:completion`: Chu trình hội thoại tích hợp trí nhớ, kiểm tra Active Recall và streaming token (`commands::llm`).
- `memory:*` / `db:*`: Thao tác tra cứu, bổ sung tri thức và quản lý sự kiện cá nhân.

---

## 5. Đường ống Âm thanh Thoại Hai chiều (WebRTC Audio Pipeline)

Đường ống thoại vận hành ở tần số 16kHz đơn kênh (mono) với độ trễ cực thấp:
1. **Khử vọng âm (Sonora AEC3)**: Loại bỏ âm thanh do chính trợ lý phát ra loa khỏi tín hiệu thu từ microphone.
2. **Khử nhiễu nền (GTCRN Denoiser)**: Lọc tiếng ồn môi trường xung quanh, tăng cường độ rõ của giọng nói tiếng Việt.
3. **Phát hiện tiếng nói (Silero VAD)**: Nhận diện sự xuất hiện của giọng nói trong cửa sổ 200ms.
4. **Cổng ngắt lượt thích ứng (Smart Turn v3.2)**: Phân loại trạng thái ngập ngừng hay dứt câu để quyết định thời điểm trợ lý phản hồi ($T_{\text{gate}} \le 225\text{ ms}$).
5. **Nhận dạng giọng nói (Parakeet STT)**: Chuyển đổi âm thanh thành văn bản thời gian thực.
6. **Tổng hợp âm thanh (VieNeu / Piper TTS)**: Sinh sóng âm và xuất kèm luồng viseme đồng bộ khẩu hình 3D avatar.
