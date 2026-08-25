---
title: "Mục lục điều hướng bộ tài liệu LIVA"
updated: 2026-08-25
commit: f35961cf
status: index
owns: []
covers:
  - docs/_data/capabilities.json
  - docs/_data/document-inventory.json
  - docs/_generated/kiem-ke-tai-lieu.md
  - docs/00-san-pham/tam-nhin-jarvis.md
  - docs/01-kien-truc/cognitive-runtime.md
  - docs/01-kien-truc/inventory-he-thong.md
  - docs/03-he-thong-con/agent-tools.md
  - docs/03-he-thong-con/context-broker.md
  - docs/03-he-thong-con/desktop-tauri.md
  - docs/03-he-thong-con/frontend.md
  - docs/03-he-thong-con/memory.md
  - docs/03-he-thong-con/persistence.md
  - docs/03-he-thong-con/vision.md
  - docs/03-he-thong-con/voice.md
  - docs/03-he-thong-con/wake-word.md
  - docs/05-chat-luong/action-policy.md
  - docs/05-chat-luong/resource-governor.md
  - docs/05-chat-luong/threat-model.md
  - docs/05-chat-luong/voice-slo.md
  - docs/05-chat-luong/wake-benchmark.md
  - docs/06-ke-hoach/roadmap.md
  - docs/07-dong-gop/quy-hoach-tai-lieu.md
---
# Tài liệu LIVA

LIVA là một trợ lý AI chạy **cục bộ trên máy người dùng**: lõi Rust `liva-native-core` (LLM, STT, TTS, agent, WebSocket gateway cổng 8002) được vỏ Tauri v2 `liva-desktop` nhúng in-process, còn giao diện là ứng dụng Vue 3 `liva-ui`. Suy luận mặc định chạy offline bằng model cục bộ — `llama.cpp` cho LLM/vision, ONNX Runtime cho STT/TTS/VAD/denoise — dữ liệu nằm trong SQLite cục bộ. Ngoại lệ mạng đều opt-in hoặc theo tool: weather/geolocation và HTTP tương thích OpenAI tắt mặc định. Tài liệu as-built phải bám code/test; tài liệu target và roadmap phải tự ghi rõ rằng chúng mô tả đích đến.

Từ 30/07/2026, bộ tài liệu được di trú dần sang kiến trúc v2. Tài liệu **as-built**
vẫn nằm trong `01-ban-ve/`, `02-van-hanh/` và `03-danh-gia/`; tầm nhìn, kiến trúc
đích và roadmap mới có nguồn chuẩn riêng. Khi một tài liệu cũ và ma trận năng lực
mâu thuẫn về trạng thái hiện tại, đọc bằng chứng code/test trong ma trận và dùng code
làm trọng tài.

## Nguồn chuẩn mới

1. [Tầm nhìn LIVA — trợ lý kiểu JARVIS](00-san-pham/tam-nhin-jarvis.md) — định nghĩa sản phẩm và nguyên tắc.
2. [Ma trận năng lực LIVA → JARVIS](_generated/ma-tran-nang-luc.md) — trạng thái sinh từ registry, không sửa tay.
3. [Inventory capability → module](01-kien-truc/inventory-he-thong.md) — entry point, luồng, test và khoảng trống as-built.
4. [Kiến trúc đích Cognitive Runtime](01-kien-truc/cognitive-runtime.md) — vòng perception → policy → action → memory.
5. [Master roadmap](06-ke-hoach/roadmap.md) — nguồn duy nhất cho việc còn làm và thứ tự.
5b. [Việc còn lại trên nhánh `mac-v2`](06-ke-hoach/viec-con-lai-mac-v2.md) — punch list MV-1…MV-13 phạm vi nhánh: CI chưa từng chạy, hai cổng không bao giờ xanh được, 3 GB rác chưa gitignore.
5c. [Việc cần làm — rà soát 25/08/2026](06-ke-hoach/viec-can-lam-2026-08-25.md) — VC-1…VC-7: ba cổng kiểm đang đỏ (advisory `h2`, test `preflight` phụ thuộc đĩa, ba test vault trên macOS) và bốn mục thoại real-time, mở đầu bằng việc đo lượt thoại vốn chưa từng có.
6. [Kiểm kê disposition tài liệu](_generated/kiem-ke-tai-lieu.md) — toàn bộ file được gắn KEEP/SPLIT/GENERATE/FREEZE/MERGE.
7. [Quy hoạch tài liệu v2](07-dong-gop/quy-hoach-tai-lieu.md) — mapping và gate di trú.
8. [Voice runtime](03-he-thong-con/voice.md) + [Voice SLO](05-chat-luong/voice-slo.md) — as-built và cổng chất lượng thoại.
9. [Wake architecture](03-he-thong-con/wake-word.md) + [Wake benchmark](05-chat-luong/wake-benchmark.md) — đường wake hiện hành và cổng nghiệm thu.
10. [Agent và tool runtime](03-he-thong-con/agent-tools.md) + [Action policy](05-chat-luong/action-policy.md) — StateGraph, selector, executor và ranh giới side effect.
11. [Memory runtime](03-he-thong-con/memory.md) ([HTML quét nhanh](03-he-thong-con/memory.html)) — checkpoint, RAG, facts, projection worker và lộ trình semantic memory.
12. [Persistence runtime](03-he-thong-con/persistence.md) + [Threat model](05-chat-luong/threat-model.md) — data root, 20 bảng, migration, backup/restore, trust boundary, mã hóa và kế hoạch hardening.
13. [Vision runtime](03-he-thong-con/vision.md) + [Context broker](03-he-thong-con/context-broker.md) + [Resource governor](05-chat-luong/resource-governor.md) — perception theo yêu cầu, ranh giới proactive và chính sách sống chung với workload nặng.
14. [Frontend runtime](03-he-thong-con/frontend.md) + [Desktop Tauri](03-he-thong-con/desktop-tauri.md) — hai entry Vue production, transport, cửa sổ, capability và IPC native.

Registry máy đọc được: [`capabilities.json`](_data/capabilities.json) và
[`document-inventory.json`](_data/document-inventory.json).

> **Trạng thái chuyển tiếp:** chưa di chuyển hàng loạt tài liệu cũ. Việc này giữ mọi
> liên kết hiện có hoạt động trong khi từng subsystem được tách sang cấu trúc mới.

> **Cảnh báo "hai profile chạy không tương đương" đã được GỠ (26/07/2026).** Bản trước dặn người đọc rằng VAD/WS 8002/Telegram chỉ sống ở binary standalone chứ không ở vỏ Tauri. Đối chiếu lại mã nguồn: **hai trong ba khẳng định đó đã sai từ trước**, và phần còn lại đã đóng — từ `boot.rs`, hai vỏ dựng cùng một `AppState` và bật cùng một danh sách dịch vụ nền. Khác biệt còn lại chỉ là **đường IPC** (stdin/stdout ở gateway vs `invoke` ở vỏ Tauri). `npm run dev` cho bạn đủ tính năng. ⚠ Đổi lại: **đừng chạy đồng thời hai vỏ** — cả hai đều bind `:8002`. Chi tiết và đối chiếu từng khẳng định cũ ở [§0 của Kiến trúc tổng thể](01-ban-ve/01-kien-truc-tong-the.md).

---

## Bắt đầu từ đâu

Ba lối vào theo vai trò. Mỗi lối là một chuỗi đọc **theo đúng thứ tự**.

### 1. Người mới tìm hiểu LIVA — "hệ thống này là cái gì?"

1. [01-ban-ve/00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) — LIVA là gì, ba trụ cột, hiện trạng thẳng thắn, bản đồ thư mục.
2. [01-ban-ve/01-kien-truc-tong-the.md](01-ban-ve/01-kien-truc-tong-the.md) — sơ đồ kiến trúc, hai profile chạy, bảng thành phần → công nghệ → tiến trình.
3. [02-van-hanh/03-trien-khai-va-runtime.md](02-van-hanh/03-trien-khai-va-runtime.md) — chạy thử cho đúng, tiến trình nào mở cổng nào.
4. [03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — cái gì thật sự chạy, cái gì mới là code.

### 2. Lập trình viên chuẩn bị sửa code

1. [01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md](01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) — "sửa X thì mở file nào", bảng module + LOC + người gọi, nguyên tắc an toàn.
2. [01-ban-ve/02-giao-thuc-ipc-va-websocket.md](01-ban-ve/02-giao-thuc-ipc-va-websocket.md) — hợp đồng giao thức: `AppState`, vòng đời khởi động và catalog lệnh `handle_command` theo miền.
3. Bản vẽ của đúng khu vực bạn đụng tới: [voice runtime](03-he-thong-con/voice.md) · [wake word](03-he-thong-con/wake-word.md) · [agent & tools](03-he-thong-con/agent-tools.md) · [memory](03-he-thong-con/memory.md) · [persistence](03-he-thong-con/persistence.md) · [vision](03-he-thong-con/vision.md) · [context broker](03-he-thong-con/context-broker.md) · [resource governor](05-chat-luong/resource-governor.md) · [frontend](03-he-thong-con/frontend.md) · [desktop Tauri](03-he-thong-con/desktop-tauri.md) · [action policy](05-chat-luong/action-policy.md) · [threat model](05-chat-luong/threat-model.md) · [LLM & prompt](01-ban-ve/04-he-llm-va-prompt.md) · [tích hợp ngoài](01-ban-ve/09-tich-hop-ngoai.md).
4. [02-van-hanh/04-kiem-thu-va-ci.md](02-van-hanh/04-kiem-thu-va-ci.md) — test nào có thật, 17 binary kiểm chứng, pre-commit hook và cách bypass.

### 3. Người đánh giá dự án (giám khảo, reviewer, người quyết định đầu tư)

1. [03-danh-gia/05-nang-cap-toan-dien.md §1](03-danh-gia/05-nang-cap-toan-dien.md) — **đường cơ sở đã đo**: 14 cổng kiểm với số thật và lệnh tái lập. Đọc mục này trước, vì nó là phần duy nhất có thể tự kiểm chứng trong một buổi.
2. [03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — từng tuyên bố đặt cạnh bằng chứng `file:dòng`.
3. [03-danh-gia/02-no-ky-thuat-va-rui-ro.md](03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — rủi ro xếp hạng CRITICAL/HIGH/MEDIUM/LOW + danh sách code mồ côi.
4. [03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md](03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) — 5 giai đoạn hành động, 5 việc ưu tiên cao nhất có hướng dẫn sửa chi tiết.
5. [01-ban-ve/00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) — để có bối cảnh kỹ thuật cho các kết luận trên.

---

## 01-ban-ve/ — Bản vẽ kỹ thuật hệ thống

Mô tả **hệ thống được lắp ráp thế nào**. Đây là phần dày nhất và là nguồn tham chiếu chính khi đọc/sửa code.

| Tài liệu | Nội dung | Sơ đồ |
|---|---|---|
| [00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) | Cửa vào bộ tài liệu: LIVA là gì, tầm nhìn gốc & ba trụ cột, đánh giá hiện trạng thẳng thắn, bảng chỉ số dự án, bản đồ workspace và cây thư mục, hướng dẫn đọc tiếp | 3 mermaid |
| [01-kien-truc-tong-the.md](01-ban-ve/01-kien-truc-tong-the.md) | **Hai profile chạy** (Tauri nhúng core vs binary standalone) và vì sao chúng không tương đương; sơ đồ kiến trúc tổng thể; diễn giải từng khối; bảng thành phần — công nghệ — tiến trình — trạng thái; ba lát cắt kiến trúc đáng chú ý | 1 mermaid |
| [02-giao-thuc-ipc-va-websocket.md](01-ban-ve/02-giao-thuc-ipc-va-websocket.md) | **Hợp đồng giao thức** cho mọi client: `AppState`, vòng đời khởi động, WebSocket server + stdio IPC, khung nhị phân `VoiceFrame`, hai giao thức text trên cùng socket, **catalog lệnh `handle_command` theo miền**, khung streaming, đối chiếu thiết kế gốc vs as-built, checklist viết client | 5 mermaid |
| [03-duong-ong-thoai.md](01-ban-ve/03-duong-ong-thoai.md) | **Snapshot frozen** của khảo sát thoại cũ; nguồn chuẩn hiện hành là [Voice runtime](03-he-thong-con/voice.md), [Voice SLO](05-chat-luong/voice-slo.md), [Wake architecture](03-he-thong-con/wake-word.md) và [Wake benchmark](05-chat-luong/wake-benchmark.md) | 3 mermaid |
| [04-he-llm-va-prompt.md](01-ban-ve/04-he-llm-va-prompt.md) | Kiến trúc engine LLM, `swap_model`/hot-swap, đường đa phương thức Qwen3-VL, prefix-cache + sliding window, sampler, embedding, persona và ba lớp chống prompt-injection, ba đường streaming token; **giới hạn cốt lõi: một engine / một context / một Mutex dùng chung**; router vs expert **[THIẾU]** | 5 mermaid |
| [05-agent-bo-nho-va-tien-hoa.md](01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) | **Snapshot frozen** trước khi tách subsystem; nguồn chuẩn hiện hành là [Agent và tool runtime](03-he-thong-con/agent-tools.md), [Action policy](05-chat-luong/action-policy.md) và [Memory runtime](03-he-thong-con/memory.md); evolution chỉ còn là lịch sử/experimental | 5 mermaid |
| [06-thi-giac-passive-va-governor.md](01-ban-ve/06-thi-giac-passive-va-governor.md) | **Snapshot frozen** trước khi tách subsystem; nguồn chuẩn hiện hành là [Vision runtime](03-he-thong-con/vision.md), [Context broker](03-he-thong-con/context-broker.md) và [Resource governor](05-chat-luong/resource-governor.md) | 7 mermaid |
| [07-tang-du-lieu-va-bao-mat.md](01-ban-ve/07-tang-du-lieu-va-bao-mat.md) | **Snapshot frozen** trước schema v5 và crypto/Stronghold hiện hành; nguồn chuẩn là [Persistence runtime](03-he-thong-con/persistence.md) và [Threat model](05-chat-luong/threat-model.md) | 1 mermaid (ERD lịch sử) |
| [08-frontend-va-vo-tauri.md](01-ban-ve/08-frontend-va-vo-tauri.md) | **Snapshot frozen** trước khi tách subsystem; nguồn chuẩn hiện hành là [Frontend runtime](03-he-thong-con/frontend.md) và [Desktop Tauri](03-he-thong-con/desktop-tauri.md) | 4 mermaid |
| [09-tich-hop-ngoai.md](01-ban-ve/09-tich-hop-ngoai.md) | MCP (hai bản song song, bản Rust mồ côi), bot Telegram (chạy được nhưng vòng lặp không khép kín), smart home **[THIẾU]**, dịch vụ Python `liva-voice` cổng 8765, `mobile_client/` Capacitor, `obsidian_llm_wiki`; bảng tổng hợp danh sách mồ côi cần hành động | 4 mermaid |
| [10-phu-thuoc-module-va-tra-cuu.md](01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) | **Bản đồ tìm đường trong mã nguồn**: sơ đồ phụ thuộc module Rust, bảng module (LOC · trách nhiệm · phụ thuộc · người gọi), bảng tra cứu nhanh file quan trọng, sáu thành phần mồ côi, tra cứu theo tình huống "tôi cần sửa X thì mở file nào", nguyên tắc an toàn khi sửa | 1 mermaid |

---

## 02-van-hanh/ — Cấu hình, model, chạy và kiểm thử

Mô tả **cách làm cho hệ thống chạy được trên một máy thật**.

| Tài liệu | Nội dung | Sơ đồ |
|---|---|---|
| [01-cau-hinh-va-bien-moi-truong.md](02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) | Phát hiện gốc: **không có cơ chế nào nạp `.env` vào tiến trình Rust**; bảng đầy đủ biến môi trường (mặc định + `file:dòng` + tác dụng); chỗ lệch giữa `.env.example` và code; khoá trong `data/liva-config.json` không có reader; bảng model có thật trên đĩa hay không; feature flags; điều kiện tiên quyết; checklist vận hành | 3 mermaid |
| [02-mo-hinh-ai-va-tai-nguyen.md](02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) | Nguồn sự thật của đường dẫn model, bảng model trong `models/`, LLM GGUF ngoài repo, ánh xạ model → module → thiết bị (CPU/GPU), bảng tài nguyên RAM/VRAM, điều kiện build, ba feature flag `cuda`/`vulkan`/`openblas` thật sự làm gì, checklist trước khi chạy trên máy mới | 1 mermaid |
| [03-trien-khai-va-runtime.md](02-van-hanh/03-trien-khai-va-runtime.md) | Sơ đồ triển khai; bảng tiến trình · cổng · phụ thuộc; bảng bộ nhớ model; **cách chạy đúng** để có đủ cả hai profile (`npm run dev` không khởi động binary lõi); sự cố thường gặp khi khởi động; đóng gói bản build | 1 mermaid |
| [04-kiem-thu-va-ci.md](02-van-hanh/04-kiem-thu-va-ci.md) | Bản đồ bề mặt kiểm thử; bảng test Rust (cái nào thật sự chạy trong CI); **bảng 17 binary kiểm chứng trong `src/bin/`**; CI pipeline làm và không làm gì; pre-commit hook + ba cách bypass; khoảng trống độ phủ; script/asset mồ côi; công thức chạy nhanh | 4 mermaid |
| [05-cai-dat-cho-nguoi-dung.md](02-van-hanh/05-cai-dat-cho-nguoi-dung.md) | Hướng dẫn cho **người dùng cuối** trên Windows: yêu cầu máy, cài đặt, lần chạy đầu tải model, dữ liệu nằm ở đâu, nâng cấp/gỡ, khắc phục sự cố, và mục 8 nói thẳng bản cài này **không** làm gì | — |
| [06-backup-restore-sqlite.md](02-van-hanh/06-backup-restore-sqlite.md) | Runbook online backup, manifest SHA-256, restore offline, rollback và release drill | — |
| [07-macos-dev.md](02-van-hanh/07-macos-dev.md) | Đường phát triển trên **macOS** (nhánh `mac-v2`): `scripts/start_all.sh`, Metal bật tự động không cần feature flag, số đo CPU thật cho Governor, hash `vec0` theo nền trong trust manifest, và khác biệt đã biết so với bản Windows | — |
| [release-v1.0.0-smoke-test.md](02-van-hanh/release-v1.0.0-smoke-test.md) | **Snapshot frozen** — báo cáo dựng installer v1.0.0: artifact cục bộ, smoke test home trống trên máy phát triển, phần nghiệm thu còn lại trên Windows sạch | — |

---

## 03-danh-gia/ — Đánh giá, rủi ro và lộ trình

Mô tả **hệ thống đang ở đâu so với những gì nó tuyên bố**, và phải làm gì tiếp.

| Tài liệu | Nội dung | Sơ đồ |
|---|---|---|
| [00-bao-cao-khao-sat-goc-2026-07.md](03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) | **Báo cáo khảo sát gốc** — nguồn mà toàn bộ bộ tài liệu này được biên tập ra. Khảo sát 18 khu vực mã nguồn, 4 vòng phản biện chéo, mọi khẳng định kèm trích dẫn `file:dòng`. Giữ nguyên để đối chiếu khi nghi ngờ một chi tiết đã bị biên tập sai | 6 mermaid |
| [01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) | Đặt từng tuyên bố trong `README.md`, `data/liva-config.json`, `.env.example` cạnh bằng chứng đọc được từ code: bảng đối chiếu đầy đủ, ba claim sai nghiêm trọng nhất, kiểm chứng tuyên bố "100% offline", claim đúng nên giữ nguyên, chỗ README đang **dưới-báo cáo**, câu chữ thay thế đề xuất | 3 mermaid |
| [02-no-ky-thuat-va-rui-ro.md](03-danh-gia/02-no-ky-thuat-va-rui-ro.md) | Kiểm kê rủi ro xếp hạng CRITICAL → HIGH → MEDIUM → LOW, mỗi mục kèm `file:dòng` tự kiểm chứng được; danh sách **code mồ côi** (có code, 0 call-site); đối chiếu với `tech-debt-ledger.json`; ba việc nên làm trước khi phát hành cho beta tester | 2 mermaid |
| [03-lo-trinh-sua-loi-va-nang-cap.md](03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) | **Tài liệu hành động**: nguyên tắc ưu tiên, bản đồ 5 giai đoạn (trước beta → khớp tuyên bố → nối dây thứ có sẵn → dọn dẹp → ba trụ cột), hướng dẫn sửa chi tiết cho 5 việc ưu tiên cao nhất kèm code đề xuất, bảng tổng hợp ưu tiên | 3 mermaid |
| [05-nang-cap-toan-dien.md](03-danh-gia/05-nang-cap-toan-dien.md) | **Backlog thi hành cho phiên làm việc sau**: giao thức 5 bước để nhận việc, **đường cơ sở đã đo** (14 cổng kiểm kèm lệnh tái lập — dùng để phát hiện hồi quy), các mục nâng cấp **U1–U33** chia 6 nhóm A–F, **mỗi mục có điều kiện nghiệm thu đo được**, và mục §9 "cái KHÔNG nên làm" để chặn công việc vô ích. Nhóm F là **gói trình diễn** — không thêm năng lực, chỉ biến năng lực đã có thành một khoảnh khắc demo được. ⚠️ **Đọc theo bảng §2, đừng đọc theo số**: dải U16–U20 bị U21+ chiếm chỗ trước nên số hiệu không còn phản ánh nhóm | — |

| [06-nhan-tin-ra-ngoai.md](03-danh-gia/06-nhan-tin-ra-ngoai.md) | **Nhắn tin ra ngoài (Telegram + Messenger + web khác)**: hiện trạng từng mảnh kèm bằng chứng, 5 việc còn lại theo thứ tự, và — phần đáng đọc nhất — **các bẫy đã trả giá**: `Input.dispatchKeyEvent` bị Chrome vứt khi cửa sổ không phải foreground, danh bạ biến mất theo thư mục chạy, bản nháp chết khi lõi restart, cùng ba lỗi ĐO từng dẫn tới kết luận sai | — |

| [07-wake-word-viec-con-lai.md](03-danh-gia/07-wake-word-viec-con-lai.md) | **Snapshot frozen** của đợt vá ngày 27/07/2026; nguồn chuẩn hiện hành là [Wake architecture](03-he-thong-con/wake-word.md) và [Wake benchmark](05-chat-luong/wake-benchmark.md) | — |

Hai tài liệu hành động này **không trùng nhau**: `03-…` theo dõi **sửa lỗi** GĐ0–GĐ4; `05-…` theo dõi **nâng cấp chất lượng** sau khi lớp bug chặn phát hành đã đóng, và là nơi ghi lại số đo thật để so sánh về sau. Nhóm F trong `05-…` trả lời một câu hỏi khác hẳn hai nhóm còn lại — không phải "cái gì đang hỏng" mà "làm sao để người ngoài nhìn vào thấy ấn tượng" — nên nó xếp sau cùng và cần A/B/C làm nguyên liệu.

Thư mục [`03-danh-gia/bao-cao/`](03-danh-gia/bao-cao/) hiện **rỗng** — dành cho báo cáo đánh giá phát sinh về sau (kết quả chạy prompt review, biên bản beta test).

---

## 04-quy-trinh/ — Prompt quy trình, template và knowledge base

Không mô tả code; đây là **công cụ làm việc** — các prompt hệ thống dùng để chạy review tự động và mẫu tài liệu.

| Tài liệu | Nội dung |
|---|---|
| [KNOWLEDGE_BASE.md](04-quy-trinh/KNOWLEDGE_BASE.md) | Chỉ đường tới nguồn sự thật duy nhất của Knowledge / Rules / Skills / Templates: vault Obsidian `teamwork_projects/obsidian_llm_wiki/vault/`. Bản sao cũ dưới `docs/Knowledge|Rules|Skills|Templates/` đã bị xoá — **đừng tạo lại**, sửa trong vault |
| [NEW_feature_template.md](04-quy-trinh/NEW_feature_template.md) | Mẫu prompt đặt hàng tính năng full-stack mới (vai Lead Full-Stack Architect, bắt buộc trinh sát code trước khi lập kế hoạch) |
| [prompts/architecture-review.md](04-quy-trinh/prompts/architecture-review.md) | Prompt hệ thống cho agent kiểm toán **kiến trúc** LIVA (Rust core + Vue 3 + Tauri): thu thập dữ liệu sống, kiểm tra SQLite/WAL/`sqlite-vec`, xuất Architectural Review Report. *Sinh tự động — không sửa trực tiếp* |
| [prompts/code-review-prompt.md](04-quy-trinh/prompts/code-review-prompt.md) | Prompt hệ thống cho agent **rà soát code**: kiểm tra ranh giới tech stack theo luật ESLint (`no-console`, cấm `fetch` gốc, cấm `fs.*Sync`) và luật trong `AGENTS.md`/vault; xuất Vibe Coding Compliance Report. *Sinh tự động* |
| [prompts/readme-generation-prompt.md](04-quy-trinh/prompts/readme-generation-prompt.md) | Prompt hệ thống sinh lại `README.md` gốc của dự án. **Cảnh báo:** nội dung prompt này vẫn nói tới kiến trúc 5 phần cũ (`liva-gateway`, `liva-ai-engine`, `liva-dataset`) — đã không còn tồn tại trong repo; cần cập nhật trước khi dùng lại |
| [prompts/spring-cleaning-prompt.md](04-quy-trinh/prompts/spring-cleaning-prompt.md) | Prompt hệ thống cho agent **dọn dẹp**: tìm import/symbol chết, file mồ côi, dependency thừa; có safeguard cấm đụng `.skills/`. *Sinh tự động* |
| [prompts/_meta/optimize-architecture-review.md](04-quy-trinh/prompts/_meta/optimize-architecture-review.md) | Meta-prompt sinh ra `prompts/architecture-review.md` |
| [prompts/_meta/optimize-code-review.md](04-quy-trinh/prompts/_meta/optimize-code-review.md) | Meta-prompt sinh ra `prompts/code-review-prompt.md` |
| [prompts/_meta/optimize-readme.md](04-quy-trinh/prompts/_meta/optimize-readme.md) | Meta-prompt sinh ra `prompts/readme-generation-prompt.md` |
| [prompts/_meta/optimize-spring-cleaning.md](04-quy-trinh/prompts/_meta/optimize-spring-cleaning.md) | Meta-prompt sinh ra `prompts/spring-cleaning-prompt.md` |

> **Quy tắc sửa prompt:** bốn file trong `prompts/` đều tự khai "This is an automatically generated system prompt. Do not edit directly." Muốn đổi hành vi thì sửa file `_meta/optimize-*.md` tương ứng rồi sinh lại.

---

## 99-luu-tru/ — Lưu trữ, KHÔNG dùng làm tham chiếu

> ⚠️ **CẢNH BÁO.** Mọi tài liệu trong `99-luu-tru/` **không mô tả code hiện tại**. Phần lớn mô tả một hệ thống Node.js/TypeScript **đã bị xoá khỏi repo** (`liva-gateway`, `openclaw-gateway`, `liva-ai-engine`, WebSocket cổng **8082**). Đọc chúng như **tư liệu lịch sử** — đừng bao giờ trích dẫn chúng để trả lời câu hỏi "LIVA hiện hoạt động thế nào".

Đọc cảnh báo đầy đủ và bản đồ từng thư mục con tại [99-luu-tru/README.md](99-luu-tru/README.md), bao gồm: `kien-truc-nodejs-v29/` (kiến trúc Node.js cũ), `bao-cao-lich-su/` (báo cáo audit/acceptance/nghiên cứu OSS + mẫu âm thanh VieNeu PoC), `ke-hoach-da-hoan-thanh/` (kế hoạch migration & Parakeet đã xong), `thiet-ke-goc/` (thiết kế client-server và yêu cầu ban đầu).

Ảnh chụp giao diện dùng trong tài liệu nằm ở [`assets/`](assets/).

---

## Quy ước tài liệu

### Nhãn trạng thái

Ba nhãn này dùng **thống nhất trong toàn bộ bộ tài liệu**. Chúng nói về **mức độ nối dây**, không nói về chất lượng code.

| Nhãn | Ý nghĩa | Cách kiểm chứng |
|---|---|---|
| **[OK]** | Đang chạy thật trên đường chạy chính, đã nối dây đầu-cuối | Có call-site trong đường đi mặc định, không cần bật env |
| **[MỘT PHẦN]** | Có code chạy được, nhưng **tắt mặc định / chỉ bật opt-in bằng env / chỉ sống ở một trong hai profile chạy / mới nối dây một nửa** | Đọc điều kiện bật trong code — thường là một `std::env::var(...)` hoặc một nhánh `if` |
| **[THIẾU]** | Chưa có, là **stub trả literal**, hoặc là **code mồ côi** (0 call-site trong `src/`) | Grep tên symbol trong `src/` — không có nơi nào gọi |

Khi một mục mang nhãn **[MỘT PHẦN]** hoặc **[THIẾU]**, tài liệu luôn nêu rõ **thiếu chính xác cái gì** để người sửa biết phải nối dây ở đâu.

### Trích dẫn `file:dòng`

Mọi khẳng định về hành vi hệ thống đều kèm toạ độ dạng `` `db.rs:188-354` `` hoặc `` `main.rs:42` ``. Đường dẫn được rút gọn tương đối theo module (ví dụ `webrtc/vad.rs` = `liva-native-core/src/webrtc/vad.rs`). Mục đích là **bất kỳ ai cũng mở đúng chỗ và tự kiểm chứng lại được** — nếu một dòng đã dịch chuyển do sửa code, hãy tìm theo tên symbol chứ đừng tin số dòng tuyệt đối.

### Không bịa số liệu

Nguyên tắc cứng khi biên soạn và khi cập nhật:

- **Số nào không có nguồn thì không viết.** Mọi con số (LOC, số bảng SQLite, số lệnh `handle_command`, timing, ngưỡng) đều phải đếm được lại từ code hoặc đọc được từ hằng số trong code.
- **Tách bạch "đã kiểm chứng" và "tiềm năng".** Benchmark chưa chạy thì ghi rõ là ước tính, kèm cách tính. Không quy đổi ý định thành thành tựu.
- **Không suy diễn từ tài liệu cũ.** Nếu `99-luu-tru/` nói một đằng và code nói một nẻo, code thắng.
- **Ghi rõ chỗ nghi ngờ** thay vì làm tròn cho đẹp: "chưa xác minh", "không tìm thấy call-site", "cần đo lại" đều là câu trả lời hợp lệ.

---

## Cách cập nhật tài liệu này khi code đổi

1. **Xác định phạm vi ảnh hưởng trước.** Chạy `impact({target: "symbolName", direction: "upstream"})` và `detect_changes()` (GitNexus MCP) để biết thay đổi chạm vào những module nào — từ đó suy ra file tài liệu nào phải sửa. Bảng ánh xạ module → tài liệu nằm ở [01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md](01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md).
2. **Sửa bản vẽ trước, đánh giá sau.** Thứ tự: `01-ban-ve/` → `02-van-hanh/` → `03-danh-gia/`. Bản đánh giá tham chiếu các bản vẽ, nên sửa ngược lại sẽ sinh mâu thuẫn.
3. **Cập nhật front-matter đúng nghĩa:** có sửa nội dung thì đổi `updated:` + `commit:`; chỉ sau khi đọc hết diff và không cần sửa mới đặt `stale-ok:`. Không đổi `updated` cho sửa chính tả/metadata không làm nội dung kỹ thuật đổi.

   ⚠️ **Với `03-danh-gia/`, lỗi thời là LỖI chặn CI** (`docs-check.mjs --strict-stale=docs/03-danh-gia`, từ 26/07/2026). Khi bước này đỏ, chọn đúng một trong hai — chúng **không** thay thế nhau: sửa nội dung rồi bump `commit:`, **hoặc** nếu đọc diff thấy không cần sửa gì thì đặt `stale-ok: <sha>`. Bump `commit:` khi bạn không sửa gì là khẳng định một việc đối chiếu chưa xảy ra. 📌 Nguồn đầy đủ: [Hướng dẫn bảo trì](_meta/huong-dan-bao-tri.md)
4. **Đổi nhãn khi trạng thái nối dây đổi.** Bật một tính năng opt-in thành mặc định thì nâng **[MỘT PHẦN]** → **[OK]**; xoá code mồ côi thì gỡ hẳn mục **[THIẾU]** tương ứng khỏi cả `03-danh-gia/02-no-ky-thuat-va-rui-ro.md`.
5. **Kiểm lại toạ độ `file:dòng`.** Sau khi sửa code, số dòng trong tài liệu thường lệch. Tối thiểu phải kiểm những trích dẫn nằm trong file vừa sửa.
6. **Sửa mục lục này** nếu thêm/xoá/đổi tên file: cập nhật bảng tương ứng **và** chuỗi đọc trong mục "Bắt đầu từ đâu".
7. **Cập nhật mục `## Liên quan`** ở cuối mỗi file bị ảnh hưởng — liên kết chéo phải luôn dùng **đường dẫn tương đối** so với vị trí file đó.
8. **Không sửa file trong `99-luu-tru/`.** Chúng là ảnh chụp lịch sử. Nếu một tài liệu hiện hành trở nên lỗi thời, hãy chuyển nó vào lưu trữ và ghi lý do trong [99-luu-tru/README.md](99-luu-tru/README.md).
9. **Không commit tự động.** Theo quy ước dự án (`AGENTS.md`), chỉ chạy `git commit`/`git push` khi người dùng yêu cầu rõ ràng.

Rà theo lô bằng Gemini 3.6 Flash/3.1 Pro dùng workflow + schema fail-closed ở
[Hướng dẫn bảo trì §7.5](_meta/huong-dan-bao-tri.md#75-rà-stale-theo-lô-bằng-gemini-36-flash--31-pro).

---

## Liên quan

- [01-ban-ve/00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) — cửa vào nội dung kỹ thuật
- [01-ban-ve/01-kien-truc-tong-the.md](01-ban-ve/01-kien-truc-tong-the.md) — kiến trúc và hai profile chạy
- [01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md](01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) — bản đồ tìm đường trong mã nguồn
- [02-van-hanh/03-trien-khai-va-runtime.md](02-van-hanh/03-trien-khai-va-runtime.md) — cách chạy đúng
- [03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — tuyên bố vs thực tế
- [06-ke-hoach/roadmap.md](06-ke-hoach/roadmap.md) — master roadmap duy nhất; các lộ trình trong `03-danh-gia/` là hồ sơ lịch sử/đối chiếu
- [03-danh-gia/05-nang-cap-toan-dien.md](03-danh-gia/05-nang-cap-toan-dien.md) — backlog nâng cấp U1–U33 + đường cơ sở đo được + gói trình diễn (nhóm F)
- [99-luu-tru/README.md](99-luu-tru/README.md) — cảnh báo tài liệu lỗi thời
