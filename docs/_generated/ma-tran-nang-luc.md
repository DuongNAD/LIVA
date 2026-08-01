---
title: "Ma trận năng lực LIVA → JARVIS"
updated: 2026-07-31
commit: 3688b5f
status: index
owns:
  - ma-tran-nang-luc-jarvis
covers:
  - docs/_data/capabilities.json
  - scripts/docs-capabilities.mjs
---
# Ma trận năng lực LIVA → JARVIS

[⬆ Mục lục](../README.md) · [Tầm nhìn](../00-san-pham/tam-nhin-jarvis.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

> File này được sinh từ [`docs/_data/capabilities.json`](../_data/capabilities.json).
> Không sửa tay. Chạy `npm run docs:capabilities` để sinh lại hoặc
> `npm run docs:capabilities:check` để kiểm tra drift.

## Tóm tắt

| Trạng thái | Số năng lực |
|---|---:|
| [OK] | 3 |
| [MỘT PHẦN] | 11 |
| [THỬ NGHIỆM] | 3 |
| [THIẾU] | 1 |
| [BỊ CHẶN] | 1 |
| **Tổng** | **19** |

## Danh sách

| ID | Năng lực | Trạng thái | Ưu tiên | Đích | Hiện trạng | Bằng chứng | Mốc tiếp theo |
|---|---|---|---|---|---|---|---|
| `runtime.native-core` | Lõi Rust thống nhất | [OK] | P0 | GĐ0 | Tauri và binary standalone dùng chung build_app_state, AppState và danh sách dịch vụ nền. | `liva-native-core/src/boot.rs`<br>`liva-native-core/src/lib.rs`<br>`liva-desktop/src-tauri/src/lib.rs` | Giữ một composition root và loại bỏ mọi tài liệu còn mô tả hai runtime có năng lực khác nhau. |
| `experience.voice-conversation` | Hội thoại giọng nói song công | [MỘT PHẦN] | P0 | GĐ0 | STT, TTS, VAD, denoise, AEC và barge-in có đường chạy cục bộ; chưa có SLO đầu-cuối khóa trong CI. | `liva-native-core/src/webrtc/pipeline.rs`<br>`liva-native-core/src/commands/voice.rs`<br>`liva-ui/src/composables/useVoicePipeline.ts`<br>`liva-native-core/tests/voice_runtime_components.rs` | Đo speech-end → first-audio và thời gian dừng khi barge-in trên vỏ Tauri release. |
| `experience.wake-word` | Wake word cá nhân | [MỘT PHẦN] | P1 | GĐ5 | Đường wake chạy thật và chỉ có một câu gọi “Hey Liva”. Artifact v2 đã được tích hợp, pin SHA-256 và vượt gate tổng hợp 25,88h tại threshold 0,58 (recall 91,82%, FPPH 0,0773); UX beta vẫn yêu cầu kèm lệnh cho tới khi vượt gate giọng thật trên mic mục tiêu. | `liva-native-core/src/wake.rs`<br>`liva-native-core/src/wake_model.rs`<br>`liva-native-core/src/bin/wakeword_benchmark.rs`<br>`scripts/e2e-wake-probe.mjs`<br>`tools/wakeword/hey_liva_prod.yaml`<br>`docs/03-he-thong-con/wake-word.md`<br>`docs/05-chat-luong/wake-benchmark.md` | Thu 20+ positive và 1 giờ negative trên mic mục tiêu, benchmark artifact v2 và xác nhận recall >=90%, FPPH <=1 trước khi bật UX câu gọi đứng riêng. |
| `perception.screen-vision` | Thị giác màn hình cục bộ | [MỘT PHẦN] | P1 | GĐ1 | Windows Graphics Capture, region diff và Qwen3-VL chạy thật; CUDA đạt tốc độ hội thoại, CPU không đạt. | `liva-native-core/src/vision/mod.rs`<br>`liva-native-core/src/vision/capture.rs`<br>`liva-native-core/src/commands/vision.rs`<br>`liva-native-core/src/bin/screen_vision_bench.rs` | Chọn vùng theo ngữ cảnh, thêm SLO CUDA và hiển thị rõ khi hệ thống rơi về CPU. |
| `memory.cross-session-recall` | Nhớ và truy hồi qua nhiều phiên | [OK] | P1 | GĐ0 | Typed chat, voice và Telegram/API dùng hybrid vector + FTS5 recall, lưu trong SQLite qua restart. | `liva-native-core/src/agent/graph.rs`<br>`liva-native-core/src/db.rs`<br>`liva-native-core/src/llm/embedder.rs`<br>`docs/03-he-thong-con/memory.md`<br>`scripts/e2e-memory.mjs` | Giữ regression E2E đa phiên và bổ sung benchmark retrieval theo owner/conversation scope. |
| `memory.semantic-consolidation` | Semantic consolidation và L3 graph | [MỘT PHẦN] | P1 | GĐ2 | Projection worker, checkpoint và DLQ đã có; chưa trích fact/quan hệ bền vào turn_layer_nodes và l3_nodes. | `liva-native-core/src/memory_consolidation.rs`<br>`liva-native-core/src/db.rs`<br>`docs/03-he-thong-con/memory.md`<br>`teamwork_projects/obsidian_llm_wiki/vault/Knowledge/memory_architecture.md` | Viết semantic consolidator có provenance, confidence, conflict queue và delete propagation. |
| `agent.tool-runtime` | Tool runtime do LLM dẫn | [MỘT PHẦN] | P0 | GĐ1 | Keyword fast path và LLM tool-calling loop đã có; LLM path vẫn opt-in do chi phí và chưa nối retrieval threshold vào chat thật. | `liva-native-core/src/llm/tool_calling.rs`<br>`liva-native-core/src/agent/graph.rs`<br>`liva-native-core/src/bin/tool_calling_probe.rs`<br>`docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md` | Chỉ gọi LLM khi retrieval vượt ngưỡng và khóa corpus song ngữ cho accuracy/latency. |
| `agent.action-policy` | Policy, consent và audit cho hành động | [MỘT PHẦN] | P0 | GĐ1 | Có consent store, messaging confirmation và ExecPolicy cục bộ nhưng chưa có hợp đồng ActionProposal/PolicyDecision thống nhất. | `liva-native-core/src/consent.rs`<br>`liva-native-core/src/llm/tool_calling.rs`<br>`liva-native-core/src/messaging/outbox.rs`<br>`liva-native-core/src/commands/consent.rs` | Chuẩn hóa risk tier, action id, idempotency, confirmation, observation và audit record. |
| `action.os-control` | Điều khiển âm lượng và media | [OK] | P2 | GĐ1 | Volume và media virtual-key tools chạy trên Windows, có giới hạn thao tác và lỗi trung thực ngoài Windows. | `liva-native-core/src/integrations/os_control.rs`<br>`liva-native-core/src/mcp/server.rs`<br>`liva-native-core/src/bin/os_control_probe.rs` | Đưa vào action audit chung và đo tỷ lệ thực thi thành công trên corpus nói tự nhiên. |
| `action.messaging` | Nhắn tin có xác nhận | [MỘT PHẦN] | P0 | GĐ0 | Danh bạ, draft confirmation và Messenger đã chạy thật; outbox SQLite mã hóa sống qua restart, phân loại expiry/missing/locked và tiêu thụ một lần; Telegram chưa nghiệm thu với token thật. | `liva-native-core/src/messaging`<br>`liva-native-core/src/integrations/messenger.rs`<br>`liva-native-core/src/commands/messaging.rs`<br>`docs/03-danh-gia/06-nhan-tin-ra-ngoai.md` | Chạy Telegram E2E thật và đưa messaging vào action audit/idempotency contract chung. |
| `action.smart-home` | Smart home cục bộ | [THIẾU] | P2 | GĐ4 | Command placeholder chỉ báo không có adapter; không thực hiện hardware I/O. | `liva-native-core/src/integrations/smart_home.rs`<br>`liva-native-core/src/integrations/mod.rs` | Thiết kế device registry, Matter/Home Assistant adapter, read-after-write verification và physical-action consent. |
| `proactive.context-broker` | Chủ động theo ngữ cảnh | [THỬ NGHIỆM] | P1 | GĐ3 | Passive keyboard/mouse hook tồn tại sau feature experimental nhưng chưa có call site và không được phép bật mặc định. | `liva-native-core/src/passive`<br>`liva-native-core/Cargo.toml`<br>`liva-native-core/src/consent.rs` | Thay raw keylogging bằng ContextBroker opt-in, chỉ báo trực quan, retention và kill switch. |
| `personalization.voice-clone` | Clone giọng người dùng | [BỊ CHẶN] | P3 | GĐ5 | VieNeu preset chạy thật nhưng thiếu MOSS encoder và speaker encoder tương thích để clone từ WAV. | `liva-native-core/src/tts/vieneu/mod.rs`<br>`liva-native-core/src/tts/style_vector.rs`<br>`liva-voice/src/voice_pipeline.py` | Xác định đúng hai model tương thích, kiểm bằng tai và chỉ sau đó thiết kế onboarding ghi âm. |
| `devices.cross-device` | Companion đa thiết bị | [THỬ NGHIỆM] | P3 | GĐ4 | Có mobile Capacitor PoC và WebSocket transport nhưng chưa có pairing, identity hoặc encrypted sync. | `mobile_client`<br>`liva-native-core/src/websocket.rs`<br>`packages/liva-common/src/types/websocket.ts` | Thiết kế pairing, device identity, scoped capability và đồng bộ event/memory được mã hóa. |
| `evolution.self-correction` | Tự sửa mã có kiểm soát | [THỬ NGHIỆM] | P4 | GĐ6 | Sandbox loop và CodeAgent mock có test, không được nối với LLM thật và nằm ngoài build mặc định. | `liva-native-core/src/evolution`<br>`liva-native-core/tests/self_correction_stress.rs`<br>`liva-native-core/Cargo.toml` | Hoãn tới khi có OS isolation, review gate, patch provenance và rollback end-to-end. |
| `distribution.windows-beta` | Phân phối Windows cho beta | [MỘT PHẦN] | P0 | GĐ0 | Có installer, model manifest, preflight CLI, setup UI tự mở khi thiếu model bắt buộc, resume + SHA-256 download, boot dialog có remediation và release workflow; chưa có signing và clean-machine matrix định kỳ. | `liva-desktop/src-tauri/tauri.conf.json`<br>`data/models-manifest.json`<br>`scripts/check-installer-config.mjs`<br>`docs/02-van-hanh/05-cai-dat-cho-nguoi-dung.md` | Chạy clean-machine acceptance định kỳ và quyết định code signing. |
| `data.persistence-integrity` | Dữ liệu bền và có thể phục hồi | [MỘT PHẦN] | P0 | GĐ0 | SQLite schema v7 có một writer/bốn reader, WAL, foreign keys, migration transaction và backup/restore gắn key-ID. DeleteConversation và DeleteSubject local có dry-run/audit/transaction + raw DB/WAL gate; retention conversation local opt-in theo last activity, batch tối đa 25 và retry từ remaining state. | `liva-native-core/src/db.rs`<br>`liva-native-core/src/boot.rs`<br>`liva-native-core/src/db/deletion.rs`<br>`liva-native-core/src/memory_retention.rs`<br>`liva-native-core/src/lib.rs`<br>`docs/03-he-thong-con/persistence.md` | Thêm scheduler/quota backup, release restore matrix và owner columns cho projection lịch sử trước khi mở DeleteSubject non-local; harden audit cho DeleteFact. |
| `security.desktop-boundaries` | Ranh giới quyền desktop | [MỘT PHẦN] | P0 | GĐ0 | Command plane có principal allow-list fail-closed; WebSocket mặc định là remote, query tự khai principal bị từ chối 403, còn widget/dashboard chỉ nhận principal đặc quyền qua session ticket 256-bit do Tauri capability tương ứng cấp, TTL 30 giây và dùng một lần; non-loopback bắt buộc Bearer; credential UI dùng Stronghold; widget/dashboard/setup có capability riêng; CSP không còn unsafe-inline; GGUF/mmproj/vec0 canonicalize và kiểm SHA-256 từ manifest nhúng trước khi nạp; transcript/checkpoint dùng field encryption, conversation dense-only; backup manifest key-ID fail-closed. | `liva-native-core/src/authorization.rs`<br>`liva-native-core/src/artifact_trust.rs`<br>`liva-native-core/src/websocket.rs`<br>`liva-native-core/tests/command_authorization.rs`<br>`liva-native-core/tests/artifact_trust.rs`<br>`liva-native-core/tests/websocket_transport.rs`<br>`liva-ui/tests/composables/useGateway.test.ts`<br>`liva-desktop/src-tauri/capabilities/widget.json`<br>`liva-desktop/src-tauri/capabilities/dashboard.json`<br>`liva-desktop/src-tauri/capabilities/setup.json`<br>`liva-desktop/src-tauri/tests/capability_policy.rs`<br>`liva-desktop/src-tauri/tauri.conf.json`<br>`scripts/check-installer-config.test.mjs`<br>`liva-native-core/tests/crypto_boot_e2e.rs`<br>`liva-native-core/tests/sqlite_backup_restore.rs`<br>`docs/01-kien-truc/adr-001-ma-hoa-du-lieu-ca-nhan-beta.md`<br>`docs/05-chat-luong/threat-model.md` | Bổ sung audit correlation/redaction cho các session WebSocket đặc quyền và mở rộng policy at-rest cho metadata cá nhân hậu beta. |
| `governance.documentation` | Quản trị tài liệu và nguồn sự thật | [MỘT PHẦN] | P0 | GĐ0 | Có frontmatter, owns/covers, stale gate và citation checker; roadmap bị phân tán và còn hàng trăm citation mơ hồ. | `scripts/docs-check.mjs`<br>`scripts/docs-citations.mjs`<br>`docs/_meta/nguon-su-that.md`<br>`docs/_meta/huong-dan-bao-tri.md` | Dùng registry này làm nguồn status duy nhất, sinh ma trận tự động và di trú roadmap cũ vào master roadmap. |

## Quy ước cập nhật

1. Sửa trạng thái trong `docs/_data/capabilities.json`, không sửa bảng này.
2. Mọi trạng thái `working` phải có bằng chứng đường sản phẩm và acceptance test.
3. Khi capability đổi trạng thái, cập nhật cùng lúc master roadmap và tài liệu subsystem canonical.
4. `experimental` không được quảng cáo như hành vi sản phẩm mặc định.
5. `blocked` phải ghi dependency hoặc quyết định sản phẩm đang thiếu.
