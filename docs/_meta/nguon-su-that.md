---
title: "Sổ đăng ký nguồn sự thật"
updated: 2026-08-07
commit: bd11c84
status: index
owns:
  - so-do-nguon-su-that
covers:
  - docs/_data/capabilities.json
  - docs/_data/document-inventory.json
  - docs/_generated/kiem-ke-tai-lieu.md
  - docs/00-san-pham/tam-nhin-jarvis.md
  - docs/01-kien-truc/cognitive-runtime.md
  - docs/01-kien-truc/inventory-he-thong.md
  - docs/03-he-thong-con/agent-tools.md
  - docs/03-he-thong-con/memory.md
  - docs/03-he-thong-con/persistence.md
  - docs/03-he-thong-con/voice.md
  - docs/03-he-thong-con/wake-word.md
  - docs/05-chat-luong/action-policy.md
  - docs/05-chat-luong/threat-model.md
  - docs/05-chat-luong/voice-slo.md
  - docs/05-chat-luong/wake-benchmark.md
  - docs/06-ke-hoach/roadmap.md
  - docs/07-dong-gop/quy-hoach-tai-lieu.md
---
# Sổ đăng ký nguồn sự thật (Source-of-Truth Registry)

[⬆ Mục lục](../README.md) · [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md)

---

## 1. Vì sao cần sổ này

Bộ tài liệu LIVA có nhiều tài liệu sống mô tả cùng một hệ thống. Trước khi có sổ này, **cùng một bảng bị chép ở nhiều file**: bảng biến môi trường xuất hiện cả trong tài liệu cấu hình lẫn tài liệu thoại lẫn tài liệu bảo mật; catalog `handle_command` bị trích lại một phần trong bốn bản vẽ khác nhau; ngưỡng governor có mặt ở cả bản vẽ thị giác lẫn tài liệu tài nguyên.

Hậu quả rất cụ thể: **code đổi một chỗ, tài liệu phải sửa năm chỗ — và luôn sót.** Bản chép sót trở thành thông tin sai, người đọc tin bản sai vì nó nằm ngay chỗ họ đang đọc, không ai biết bản nào mới hơn.

Sổ này giải quyết bằng một nguyên tắc duy nhất:

> **Một sự thật chỉ được viết đầy đủ ở ĐÚNG MỘT nơi.**
> Mọi tài liệu khác chỉ được giữ 1–3 dòng tóm tắt cho mạch đọc, kèm dòng
> `> 📌 Nguồn đầy đủ: [<tên tài liệu>](<đường dẫn>)` trỏ về chủ sở hữu.

Mỗi tài liệu khai báo phần sự thật mình sở hữu trong khoá `owns:` của YAML front-matter. Sổ này là **bảng tra ngược**: từ tên sự thật → tìm ra tài liệu chủ.

Sổ đi kèm hai cơ chế khác trong `_meta/`:

| Cơ chế | Trả lời câu hỏi | Ở đâu |
|---|---|---|
| `owns:` trong front-matter | "Tài liệu này sở hữu sự thật nào?" | Đầu mỗi file `.md` |
| `covers:` trong front-matter | "Code đổi thì tài liệu nào lỗi thời?" | Đầu mỗi file `.md` + [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md) |
| **Sổ này** | "Sự thật X được viết đầy đủ ở đâu?" | File này |

---

## 2. Bảng đăng ký

79 khoá sự thật, thu thập từ `owns:` của toàn bộ tài liệu sống. Cột "Ai tham chiếu" liệt kê các tài liệu hiện có dòng `📌 Nguồn đầy đủ` trỏ về chủ sở hữu — đó chính là danh sách nơi cần rà lại khi sự thật thay đổi.

### 2.0 Nguồn chuẩn v2 — sản phẩm, kiến trúc đích và roadmap

| Khoá `owns` | Tài liệu sở hữu | Mô tả ngắn | Ai tham chiếu |
|---|---|---|---|
| `tam-nhin-jarvis` | [Tầm nhìn LIVA](../00-san-pham/tam-nhin-jarvis.md) | Định nghĩa “kiểu JARVIS” thành các năng lực có thể xây và nghiệm thu | README, roadmap |
| `nguyen-tac-san-pham-jarvis` | [Tầm nhìn LIVA](../00-san-pham/tam-nhin-jarvis.md) | Local-first, planner không thi hành, proactive có consent, phản xạ tách suy luận | cognitive runtime |
| `ma-tran-nang-luc-jarvis` | [Ma trận năng lực](../_generated/ma-tran-nang-luc.md) | Bảng trạng thái sinh từ registry JSON; không sửa tay | README, vision, roadmap |
| `inventory-capability-module` | [Inventory hệ thống](../01-kien-truc/inventory-he-thong.md) | Bản đồ capability tới module, entry point, test và khoảng trống as-built | README, roadmap |
| `inventory-disposition-tai-lieu` | [Kiểm kê tài liệu](../_generated/kiem-ke-tai-lieu.md) | Disposition, target và inbound-link count sinh từ registry | README, quy hoạch tài liệu |
| `chuoi-xu-ly-thoai` | [Voice runtime](../03-he-thong-con/voice.md) | Chuỗi mic → VAD → STT → agent/LLM → TTS → loa và cancellation epoch | README, voice SLO |
| `bang-backend-tts` | [Voice runtime](../03-he-thong-con/voice.md) | VieNeu/Piper/Kokoro, điều kiện dùng và fallback có cancellation | voice SLO |
| `bang-engine-stt` | [Voice runtime](../03-he-thong-con/voice.md) | Nemotron streaming và Parakeet whole-utterance | wake architecture |
| `bang-nguong-vad-aec-denoise` | [Voice SLO](../05-chat-luong/voice-slo.md) | Ngưỡng runtime VAD/AEC/denoise và nguồn cấu hình hiện hành | voice runtime |
| `voice-slo` | [Voice SLO](../05-chat-luong/voice-slo.md) | Chỉ số, benchmark matrix và gate nâng voice lên working | roadmap |
| `wake-word-viec-con-lai` | [Wake architecture](../03-he-thong-con/wake-word.md) | Đường wake hiện hành, modes và giới hạn “Hey Liva” | roadmap |
| `wake-benchmark` | [Wake benchmark](../05-chat-luong/wake-benchmark.md) | Baseline, corpus và recall/FPPH gate cho classifier cá nhân | wake architecture |
| `kien-truc-dich-cognitive-runtime` | [Cognitive Runtime](../01-kien-truc/cognitive-runtime.md) | Kiến trúc đích perception → context → policy → tool → observation → memory | roadmap |
| `phan-cap-rui-ro-hanh-dong` | [Cognitive Runtime](../01-kien-truc/cognitive-runtime.md) | Bốn risk tier và chính sách mặc định cho hành động | roadmap |
| `master-roadmap-jarvis` | [Master roadmap](../06-ke-hoach/roadmap.md) | Nguồn duy nhất cho milestone, dependency và acceptance gate còn mở | README |
| `quy-hoach-tai-lieu-v2` | [Quy hoạch tài liệu](../07-dong-gop/quy-hoach-tai-lieu.md) | Cấu trúc đích, mapping, chiến lược di trú và docs gates | README, roadmap |

### 2.1 Bản vẽ kỹ thuật — `01-ban-ve/`

| Khoá `owns` | Tài liệu sở hữu | Mô tả ngắn | Ai tham chiếu |
|---|---|---|---|
| `bang-chi-so-du-an` | [00 — Tổng quan hệ thống](../01-ban-ve/00-tong-quan-he-thong.md) | Số liệu tổng: LOC, số file, số module, số symbol/quan hệ GitNexus | 08 |
| `ban-do-workspace` | [00 — Tổng quan hệ thống](../01-ban-ve/00-tong-quan-he-thong.md) | Cây thư mục repo, vai trò từng crate/package, thứ tự đọc | 08 |
| `hai-profile-chay` | [01 — Kiến trúc tổng thể](../01-ban-ve/01-kien-truc-tong-the.md) | Vỏ Tauri nhúng core vs binary standalone, bảng so sánh năng lực từng profile | 00, 02, 02-vh/01, 02-vh/03, 03-đg/01, 03-đg/02 |
| `so-do-kien-truc-tong-the` | [01 — Kiến trúc tổng thể](../01-ban-ve/01-kien-truc-tong-the.md) | Sơ đồ khối toàn hệ + chiều dữ liệu giữa UI ↔ core ↔ vệ tinh | 00, 02-vh/01, 02-vh/03, 03-đg/01 |
| `catalog-lenh-handle-command` | [02 — Giao thức IPC và WebSocket](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) | Catalog lệnh theo miền; số lệnh là snapshot, nguồn thật nằm trong `commands/*` + dispatcher | 01, 04, 08, 09, 03-đg/02 |
| `khung-nhi-phan-9-byte` | [02 — Giao thức IPC và WebSocket](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) | Header `VoiceFrame` 9 byte, giới hạn 1 MiB, cách đóng/mở khung | 01, 04, 08, 09, 03-đg/02, 03-đg/03 |
| `bang-opcode` | [02 — Giao thức IPC và WebSocket](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) | 5 opcode nhị phân và ý nghĩa từng mã | 01, 08, 09, 03-đg/02, 03-đg/03 |
| `cau-hinh-llm` | [04 — Hệ LLM và prompt](../01-ban-ve/04-he-llm-va-prompt.md) | Router/expert, `n_ctx`, sampling, GPU layer, nguồn đường dẫn model | 02, 03-đg/02 |
| `persona-va-chong-injection` | [04 — Hệ LLM và prompt](../01-ban-ve/04-he-llm-va-prompt.md) | Nội dung persona, cách dựng prompt, lớp chặn prompt-injection | 06 |
| `may-trang-thai-agent` | [Agent và tool runtime](../03-he-thong-con/agent-tools.md) | Entry point, trạng thái và vòng đời agent voice hiện hành | 00, 04 |
| `state-graph-agent` | [Agent và tool runtime](../03-he-thong-con/agent-tools.md) | StateGraph sáu node, reflex lane và luật rẽ nhánh | 04, 09 |
| `tool-catalog-va-executor` | [Agent và tool runtime](../03-he-thong-con/agent-tools.md) | Catalog native/external MCP, selection và executor | action policy |
| `skill-runtime-cuc-bo` | [Agent và tool runtime](../03-he-thong-con/agent-tools.md) | Skill store, version, ranking, signals và ranh giới chưa auto-exec | README |
| `memory-runtime-as-built` | [Memory runtime](../03-he-thong-con/memory.md) | Checkpoint, conversational RAG, facts, projection worker và các tầng chưa có writer | README, roadmap |
| `memory-scope-va-lineage` | [Memory runtime](../03-he-thong-con/memory.md) | Owner boundary, conversation/audience scope và lineage event → vector | agent runtime |
| `memory-projection-consumer` | [Memory runtime](../03-he-thong-con/memory.md) | Batch, checkpoint, retry/DLQ và nghĩa chính xác của trạng thái consolidated | roadmap |
| `memory-upgrade-plan` | [Memory runtime](../03-he-thong-con/memory.md) | Lộ trình M0–M5 và acceptance gates từ truthful UI tới semantic/multimodal memory | master roadmap |
| `erd-sqlite` | [Persistence runtime](../03-he-thong-con/persistence.md) | ERD logic của 20 bảng SQLite hiện hành | memory, đánh giá cũ |
| `bang-schema-du-lieu` | [Persistence runtime](../03-he-thong-con/persistence.md) | 20 bảng, miền dữ liệu, production writer và phạm vi mã hóa | memory, roadmap |
| `persistence-runtime-as-built` | [Persistence runtime](../03-he-thong-con/persistence.md) | Data root, pool, PRAGMA, migration và durability hiện hành | README |
| `data-lifecycle-upgrade-plan` | [Persistence runtime](../03-he-thong-con/persistence.md) | Lộ trình D0–D4 cho integrity, backup/restore, retention và degraded mode | master roadmap |
| `action-policy-as-built` | [Action policy](../05-chat-luong/action-policy.md) | ExecPolicy, messaging confirmation, consent và gap contract thống nhất | roadmap |
| `so-do-ma-hoa` | [Threat model](../05-chat-luong/threat-model.md) | DB device key, fact encryption và Stronghold key hierarchy | persistence |
| `security-trust-boundaries` | [Threat model](../05-chat-luong/threat-model.md) | Principal, WebSocket/Tauri/MCP boundary và giả định local-user | README |
| `security-data-at-rest` | [Threat model](../05-chat-luong/threat-model.md) | Ma trận bảo vệ facts, transcript, checkpoint, config và backup | persistence |
| `security-upgrade-plan` | [Threat model](../05-chat-luong/threat-model.md) | Lộ trình S0–S5 từ credential path tới identity, artifact trust và audit | master roadmap |
| `nguong-governor` | [06 — Thị giác, passive và governor](../01-ban-ve/06-thi-giac-passive-va-governor.md) | Ngưỡng tải GPU/CPU, mức throttle, cách đo tải thật | 04, 08, 09, 02-vh/02, 03-đg/03 |
| `canh-bao-passive-keylogger` | [06 — Thị giác, passive và governor](../01-ban-ve/06-thi-giac-passive-va-governor.md) | Hook bàn phím/màn hình: phạm vi thu thập, rủi ro riêng tư, điều kiện bật | 03-đg/03 |
| `bang-man-hinh-dashboard` | [Frontend runtime](../03-he-thong-con/frontend.md) | Danh sách màn hình dashboard, component tương ứng, trạng thái hoàn thiện | 01 |
| `bang-tauri-command` | [Desktop Tauri](../03-he-thong-con/desktop-tauri.md) | Lệnh `invoke` của Tauri: tên, capability và handler Rust | 01, 02 |
| `cau-hinh-cua-so` | [Desktop Tauri](../03-he-thong-con/desktop-tauri.md) | `tauri.conf.json`: kích thước, ghost mode, always-on-top, transparent | 02 |
| `bang-tich-hop-ngoai` | [09 — Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) | Telegram, MCP client/server, smart home, Google API: trạng thái thật từng cái | 01, 05 |
| `bang-module-va-loc` | [10 — Phụ thuộc module và tra cứu](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) | LOC từng module, người gọi, mức độ nối dây | 01, 03, 02-vh/04 |
| `so-do-phu-thuoc-module` | [10 — Phụ thuộc module và tra cứu](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) | Sơ đồ phụ thuộc giữa các module Rust, điểm nghẽn | 03, 04 |
| `tra-cuu-file` | [10 — Phụ thuộc module và tra cứu](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) | Bảng "sửa X thì mở file nào" + nguyên tắc an toàn khi sửa | 04, 06 |

### 2.2 Vận hành — `02-van-hanh/`

| Khoá `owns` | Tài liệu sở hữu | Mô tả ngắn | Ai tham chiếu |
|---|---|---|---|
| `bang-bien-moi-truong` | [01 — Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) | Toàn bộ biến `LIVA_*` theo nhóm A–F: nơi đọc, mặc định, điều kiện bật | 02, 03, 04, 06, 07, 08, 09, 02-vh/02, 02-vh/03, 03-đg/02, 03-đg/03 |
| `lech-env-example-vs-code` | [01 — Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) | Danh mục chỗ lệch `.env.example` ↔ code: dòng số, biến, hướng lệch | 06, 08, 09, 02-vh/02, 03-đg/03 |
| `bang-model` | [02 — Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) | Model thật trên đĩa: đường dẫn, kích thước, định dạng, có/không tồn tại | 01, 02, 04, 08, 02-vh/01, 02-vh/03 |
| `bang-tai-nguyen-ram-vram` | [02 — Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) | RAM/VRAM từng model, tổng chi phí khi chạy đủ profile | 02, 08, 02-vh/03 |
| `dieu-kien-tien-quyet-build` | [02 — Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) | CMake/LLVM/`LIBCLANG_PATH`, feature `cuda`/`vulkan`/`openblas`, espeak-ng/ffmpeg | 02-vh/04, 03-đg/02 |
| `bang-tien-trinh` | [03 — Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md) | Tiến trình nào chạy, lệnh khởi động, cổng mở, phụ thuộc, có bắt buộc không | 08 |
| `cach-chay-dung` | [03 — Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md) | Trình tự chạy đúng để có đủ hai profile + xử lý sự cố thường gặp | 01, 02, 08, 02-vh/01, 02-vh/04 |
| `bang-test` | [04 — Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) | File test nào tồn tại, cái nào chạy trong CI, subsystem nào không có test | 03, 08, 02-vh/01, 02-vh/02, 03-đg/02, 03-đg/03 |
| `bang-binary-verify` | [04 — Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) | 17 binary `verify_*`/`*_stress`/`*_bench`: dùng làm gì, chạy bằng lệnh nào | 06, 02-vh/02, 03-đg/03 |
| `ci-pipeline` | [04 — Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) | Workflow CI từng bước, những gì CI **không** gate, pre-commit hook + 3 cách bypass | 02-vh/01, 03-đg/02 |
| `cai-dat-windows` | [05 — Cài đặt cho người dùng](../02-van-hanh/05-cai-dat-cho-nguoi-dung.md) | Luồng cài đặt Windows, preflight và tải model cho bản beta | README |
| `go-va-nang-cap` | [05 — Cài đặt cho người dùng](../02-van-hanh/05-cai-dat-cho-nguoi-dung.md) | Cách gỡ, nâng cấp và giữ dữ liệu người dùng | — |
| `khac-phuc-su-co-nguoi-dung` | [05 — Cài đặt cho người dùng](../02-van-hanh/05-cai-dat-cho-nguoi-dung.md) | Chẩn đoán lỗi phổ biến theo triệu chứng và bước phục hồi | — |

### 2.3 Đánh giá — `03-danh-gia/`

| Khoá `owns` | Tài liệu sở hữu | Mô tả ngắn | Ai tham chiếu |
|---|---|---|---|
| `bang-doi-chieu-tuyen-bo` | [01 — Đối chiếu tuyên bố vs thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) | Từng tuyên bố sản phẩm đặt cạnh bằng chứng `file:dòng` | 00, 02, 03-đg/03 |
| `kiem-chung-offline` | [01 — Đối chiếu tuyên bố vs thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) | Kiểm chứng "chạy hoàn toàn offline": chỗ nào thật, chỗ nào còn gọi mạng | 03-đg/03 |
| `bang-rui-ro-xep-hang` | [02 — Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) | Rủi ro CRITICAL/HIGH/MEDIUM/LOW, mã định danh C\*/H\*/M\*/L\* | 00, 03, 04, 05, 06, 07, 08, 09, 10, 02-vh/04 |
| `bang-code-mo-coi` | [02 — Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) | Module chết, hàm `pub` 0 caller, 22 sự kiện UI mồ côi, 14 lệnh core không client gọi | 02, 03, 05, 07, 08, 09, 10, 02-vh/04 |
| `lo-trinh-5-giai-doan` | [03 — Lộ trình sửa lỗi và nâng cấp](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) | 5 giai đoạn hành động và thứ tự ưu tiên | 00, 05, 06, 07, 03-đg/02 |
| `huong-dan-sua-F1-F5` | [03 — Lộ trình sửa lỗi và nâng cấp](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) | Hướng dẫn sửa chi tiết 5 việc ưu tiên cao nhất | 04, 05, 03-đg/02 |
| `de-xuat-openspace-g0-g4` | [04 — Đề xuất OpenSpace](../03-danh-gia/04-de-xuat-tich-hop-openspace.md) | Phương án tích hợp G0–G4 tại thời điểm đánh giá | — |
| `phan-ra-openspace-lay-va-tu-choi` | [04 — Đề xuất OpenSpace](../03-danh-gia/04-de-xuat-tich-hop-openspace.md) | Phần OpenSpace nên lấy, trì hoãn hoặc từ chối | — |
| `duong-co-so-do-luong` | [05 — Nâng cấp toàn diện](../03-danh-gia/05-nang-cap-toan-dien.md) | 8 cổng kiểm đo thật ngày 26/07/2026 kèm lệnh tái lập; mốc phát hiện hồi quy | 02-vh/04 |
| `backlog-nang-cap-U1-U15` | [05 — Nâng cấp toàn diện](../03-danh-gia/05-nang-cap-toan-dien.md) | 16 mục nâng cấp chất lượng, nhóm A–E (U1–U15 + U21 thêm 01/08/2026), mỗi mục có điều kiện nghiệm thu đo được. Khoá giữ nguyên tên cũ vì đổi tên khoá không thêm thông tin mà làm hỏng tra ngược | 03-đg/03 |
| `goi-trinh-dien-U16-U20` | [05 — Nâng cấp toàn diện](../03-danh-gia/05-nang-cap-toan-dien.md) | Nhóm F — 5 mục biến năng lực đã có thành khoảnh khắc demo được, kèm nguyên tắc "giới hạn bởi kỹ thuật, không bởi IQ model" | — |
| `nhan-tin-ra-ngoai-telegram-messenger` | [06 — Nhắn tin ra ngoài](../03-danh-gia/06-nhan-tin-ra-ngoai.md) | Hiện trạng Messenger/Telegram, outbox và việc nghiệm thu còn lại | roadmap |

### 2.4 Siêu dữ liệu — `_meta/`

| Khoá `owns` | Tài liệu sở hữu | Mô tả ngắn | Ai tham chiếu |
|---|---|---|---|
| `so-do-nguon-su-that` | [Sổ đăng ký nguồn sự thật](nguon-su-that.md) *(chính file này)* | Bảng tra ngược "sự thật → tài liệu chủ" + quy tắc thêm/chuyển chủ | (chưa có tài liệu nào trỏ tới — nên thêm liên kết từ README) |
| `luoc-do-front-matter` | [Hướng dẫn bảo trì](huong-dan-bao-tri.md) | Schema frontmatter, ý nghĩa và quy tắc cập nhật | mọi tài liệu living/index |
| `quy-trinh-bao-tri-tai-lieu` | [Hướng dẫn bảo trì](huong-dan-bao-tri.md) | Quy trình kiểm tra stale, owns/covers, link và citation | contributor workflow |

**Ghi chú ký hiệu:** trong cột "Ai tham chiếu", số trần (`00`–`10`) là tài liệu trong `01-ban-ve/`; `02-vh/NN` là `02-van-hanh/`; `03-đg/NN` là `03-danh-gia/`.

**Hai tài liệu không sở hữu sự thật nào** (`owns: []`), có chủ đích:

- [README.md](../README.md) — chỉ điều hướng, mọi số liệu trong đó là trích dẫn.
- [03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md](../03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) — `status: frozen`, là bản chụp khảo sát lịch sử. **Không cập nhật file này**; nội dung lỗi thời trong đó là bình thường và có chủ đích.

---

## 3. Quy tắc khi thêm một sự thật mới

Khi bạn viết một bảng/sơ đồ/số liệu mới mà chưa tài liệu nào có:

1. **Chọn đúng tài liệu chủ.** Sự thật thuộc về tài liệu mà người đọc sẽ tìm nó *đầu tiên*, không phải tài liệu bạn tình cờ đang mở. Nếu phân vân giữa hai tài liệu, chọn cái mô tả **cơ chế** (bản vẽ) thay vì cái mô tả **hệ quả** (đánh giá).
2. **Đặt khoá kebab-case, tiếng Việt không dấu**, mô tả *nội dung* chứ không mô tả *vị trí*:
   - ✅ `bang-nguong-vad-aec-denoise`, `so-do-ma-hoa`, `cach-chay-dung`
   - ❌ `bang-3`, `muc-5-2`, `phan-thoai` (số mục sẽ đổi, tên tài liệu sẽ đổi)
3. **Khai vào `owns:`** của tài liệu chủ, giữ thứ tự xuất hiện trong bài.
4. **Ghi một dòng vào bảng §2 của sổ này**, đúng mục con `01-ban-ve` / `02-van-hanh` / `03-danh-gia` / `_meta`.
5. **Cập nhật `covers:`** của tài liệu chủ nếu sự thật mới bám vào file mã nguồn chưa có trong danh sách — nếu không, cơ chế phát hiện lỗi thời sẽ bỏ qua nó.
6. **Mọi nơi khác chỉ được trỏ tới**: giữ tối đa 3 dòng tóm tắt rồi thêm
   `> 📌 Nguồn đầy đủ: [<tên tài liệu>](<đường dẫn tương đối>)`.
   Đường dẫn tính từ thư mục của file đang sửa: cùng thư mục → `0X-ten.md`; khác thư mục → `../02-van-hanh/0X-ten.md`.

---

## 4. Quy tắc khi một sự thật đổi chủ

Chuyển nội dung sang tài liệu khác là việc hợp lệ (thường vì tài liệu cũ phình to, hoặc vì sự thật đó hoá ra thuộc lĩnh vực khác). Làm **đủ 6 bước, trong một lần commit** — làm nửa vời sẽ sinh ra hai bản chép, đúng thứ sổ này sinh ra để chống:

1. **Chuyển nguyên khối nội dung** sang tài liệu mới. Không viết lại từ đầu — viết lại là cơ hội cho sai lệch chen vào.
2. **Xoá khoá khỏi `owns:` của chủ cũ**, thêm vào `owns:` của chủ mới. Giữ nguyên tên khoá nếu nội dung không đổi bản chất — đổi tên khoá là một thay đổi riêng, đừng gộp.
3. **Ở chủ cũ, thay khối vừa chuyển bằng 1–3 dòng tóm tắt** + dòng `📌 Nguồn đầy đủ` trỏ sang chủ mới. Không xoá trắng: người đọc chủ cũ vẫn cần mạch.
4. **Sửa mọi dòng `📌 Nguồn đầy đủ` đang trỏ về chủ cũ.** Tìm bằng:
   ```powershell
   Select-String -Path "docs\**\*.md" -Pattern "Nguồn đầy đủ" | Select-String "<ten-file-chu-cu>"
   ```
5. **Chuyển các mục `covers:` liên quan** từ chủ cũ sang chủ mới (chỉ những file mã nguồn mà chủ cũ không còn mô tả nữa).
6. **Cập nhật dòng tương ứng trong bảng §2**: cột "Tài liệu sở hữu" và cột "Ai tham chiếu".

Nếu một sự thật bị **xoá hẳn** (code không còn), xoá khoá khỏi `owns:`, xoá dòng khỏi §2, và chuyển các dòng `📌` trỏ tới nó thành mô tả trực tiếp hoặc xoá — không để liên kết chết.

---

## 5. Sự thật chưa có chủ

Các bảng/số liệu dưới đây **đang được viết ở ≥2 nơi nhưng chưa nằm trong bất kỳ `owns:` nào**. Chưa xử lý — liệt kê để giải quyết ở đợt rà soát sau. Với mỗi mục đã có gợi ý chủ sở hữu (cột "Nên thuộc về").

| # | Sự thật đang trôi | Đang xuất hiện ở | Nên thuộc về | Ghi chú |
|---|---|---|---|---|
| 1 | **Vòng đời khởi động 26 bước** của composition root dùng chung | `01-ban-ve/02` §3 (bản đầy đủ), README (nhắc tên), `01-ban-ve/01` (nhắc thứ tự khởi tạo) | `01-ban-ve/02` — khoá đề xuất `vong-doi-khoi-dong` | Bản đầy đủ đã ở đúng chỗ; tránh nhúng số bước vào tên khoá vì mỗi bước mới sẽ buộc rename owner |
| 2 | **Cấu trúc `AppState`** (các trường, ai giữ, ai khoá) | `01-ban-ve/02` (bản đầy đủ) + **16 tài liệu khác nhắc tên trường** | `01-ban-ve/02` — khoá đề xuất `cau-truc-appstate` | Đây là ký hiệu bị nhắc lại nhiều nhất toàn bộ tài liệu; thêm một trường mới hiện không có cách nào biết phải sửa ở đâu |
| 3 | **Bảng cổng mạng** (8002 / 5173 / 8765 / cổng Tauri) | `02-van-hanh/03` §2.1 (bảng đầy đủ), `01-ban-ve/01` (liệt kê lại trong sơ đồ + bảng profile), `01-ban-ve/00`, `01-ban-ve/09`, `03-danh-gia/01`, `03-danh-gia/02` | `02-van-hanh/03` — khoá đề xuất `bang-cong-mang` | Rủi ro cao: đổi `LIVA_SERVER_PORT` mặc định sẽ phải sửa ≥5 file |
| 4 | **Bảng 10 nhóm `.gitignore` + tình trạng `.aiexclude`** | `02-van-hanh/01` §6.1 (bảng đầy đủ), `01-ban-ve/07` §8 (tóm tắt phần Secrets), `02-van-hanh/02` (nhắc nhóm weights), `03-danh-gia/02` | `02-van-hanh/01` — khoá đề xuất `bang-gitignore-aiexclude` | ⚠️ Kèm một liên kết sai cần sửa: `01-ban-ve/07:768` ghi *"Cấu hình và biến môi trường §8"* nhưng bảng thật nằm ở **§6.1** |
| 5 | **Bảng khoá `data/liva-config.json` — có reader hay không** | `02-van-hanh/01` §4.3 (bảng đầy đủ), `01-ban-ve/07` (trích), `01-ban-ve/04` (trích khối `ai`), `01-ban-ve/08` | `02-van-hanh/01` — khoá đề xuất `bang-khoa-liva-config` | Đây là nguồn thật của đường dẫn model LLM (không phải env), nên rất hay bị trích lại |
| 6 | **Pre-commit hook + 3 cách bypass** (`SKIP_AI_HOOK=1`, `--no-verify`, lint-staged chỉ lint `*.ts`) | `02-van-hanh/04` §5.2–5.3 (bản đầy đủ), `02-van-hanh/01`, `01-ban-ve/10`, `01-ban-ve/09`, `03-danh-gia/02` | `02-van-hanh/04` — nằm trong `ci-pipeline`, chỉ cần ghi rõ phạm vi khoá | Có thể không cần khoá mới: mở rộng mô tả của `ci-pipeline` trong §2.2 để tuyên bố rõ nó bao gồm cả hook |
| 7 | **Ranh giới `verify_*` binary vs `cargo test`** (cái nào chạy trong CI) | `02-van-hanh/04` (`bang-test` + `bang-binary-verify`) — nhưng lằn ranh "cái nào CI chạy" bị nhắc lại ở `03-danh-gia/02` M2 và `01-ban-ve/03` | `02-van-hanh/04` — thuộc `ci-pipeline` | Không phải bảng mới, mà là một *kết luận* bị chép; nên rút gọn ở hai nơi kia thành 1 dòng + `📌` |

**Cách xử lý đề xuất** (khi có thời gian, làm từ trên xuống): với mục 1–2 chỉ cần thêm khoá vào `owns:` và ghi vào §2 — nội dung đã ở đúng chỗ. Mục 3–5 cần thêm cả việc rút gọn bản chép ở các tài liệu vệ tinh theo quy tắc §3.6. Mục 6–7 chỉ cần sửa mô tả khoá sẵn có, không sinh khoá mới.

---

## 6. Xem thêm

- [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md) — chiều ngược lại: file mã nguồn nào ảnh hưởng tài liệu nào (dựng từ `covers:`).
- [Mục lục bộ tài liệu](../README.md) — ba lối đọc theo vai trò.
