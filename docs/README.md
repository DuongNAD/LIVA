---
title: "Mục lục điều hướng bộ tài liệu LIVA"
updated: 2026-07-22
commit: 91bbdfa
status: living
owns: []
covers:
  - data/liva-config.json
  - liva-desktop/src-tauri/tauri.conf.json
  - liva-native-core/src/*
  - liva-native-core/src/agent/memory.rs
  - liva-native-core/src/vision/capture.rs
  - liva-native-core/src/webrtc/vad.rs
  - liva-ui/src/composables/useGateway.ts
  - liva-ui/src/composables/useVoicePipeline.ts
---
# Tài liệu LIVA

LIVA là một trợ lý AI chạy **cục bộ trên máy người dùng**: lõi Rust `liva-native-core` (LLM, STT, TTS, agent, WebSocket gateway cổng 8002) được vỏ Tauri v2 `liva-desktop` nhúng in-process, còn giao diện là ứng dụng Vue 3 `liva-ui`. Toàn bộ suy luận mặc định chạy offline bằng model cục bộ — `llama.cpp` cho LLM/vision, ONNX Runtime cho STT/TTS/VAD/denoise — dữ liệu nằm trong SQLite cục bộ. Bộ tài liệu này mô tả **code thật đang tồn tại tại commit `5d69c3c`**, không mô tả kế hoạch hay ý định.

> **Đọc trước khi tin bất cứ điều gì:** LIVA có **hai profile chạy không tương đương** (vỏ Tauri nhúng core vs binary `liva-native-core.exe` standalone). Rất nhiều câu hỏi kiểu "tại sao VAD không hoạt động", "tại sao `ws://127.0.0.1:8002/ws` bị từ chối", "tại sao bot Telegram im lặng" đều quy về việc bạn đang chạy profile nào. Chi tiết ở [§0 của Kiến trúc tổng thể](01-ban-ve/01-kien-truc-tong-the.md).

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
2. [01-ban-ve/02-giao-thuc-ipc-va-websocket.md](01-ban-ve/02-giao-thuc-ipc-va-websocket.md) — hợp đồng giao thức: `AppState`, 25 bước khởi động, bảng 42 lệnh `handle_command`.
3. Bản vẽ của đúng khu vực bạn đụng tới: [thoại](01-ban-ve/03-duong-ong-thoai.md) · [LLM & prompt](01-ban-ve/04-he-llm-va-prompt.md) · [agent & bộ nhớ](01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) · [thị giác & governor](01-ban-ve/06-thi-giac-passive-va-governor.md) · [dữ liệu & bảo mật](01-ban-ve/07-tang-du-lieu-va-bao-mat.md) · [frontend & Tauri](01-ban-ve/08-frontend-va-vo-tauri.md) · [tích hợp ngoài](01-ban-ve/09-tich-hop-ngoai.md).
4. [02-van-hanh/04-kiem-thu-va-ci.md](02-van-hanh/04-kiem-thu-va-ci.md) — test nào có thật, 17 binary kiểm chứng, pre-commit hook và cách bypass.

### 3. Người đánh giá dự án (giám khảo, reviewer, người quyết định đầu tư)

1. [03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — từng tuyên bố đặt cạnh bằng chứng `file:dòng`.
2. [03-danh-gia/02-no-ky-thuat-va-rui-ro.md](03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — rủi ro xếp hạng CRITICAL/HIGH/MEDIUM/LOW + danh sách code mồ côi.
3. [03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md](03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) — 5 giai đoạn hành động, 5 việc ưu tiên cao nhất có hướng dẫn sửa chi tiết.
4. [01-ban-ve/00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) — để có bối cảnh kỹ thuật cho các kết luận trên.

---

## 01-ban-ve/ — Bản vẽ kỹ thuật hệ thống

Mô tả **hệ thống được lắp ráp thế nào**. Đây là phần dày nhất và là nguồn tham chiếu chính khi đọc/sửa code.

| Tài liệu | Nội dung | Sơ đồ |
|---|---|---|
| [00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) | Cửa vào bộ tài liệu: LIVA là gì, tầm nhìn gốc & ba trụ cột, đánh giá hiện trạng thẳng thắn, bảng chỉ số dự án, bản đồ workspace và cây thư mục, hướng dẫn đọc tiếp | 3 mermaid |
| [01-kien-truc-tong-the.md](01-ban-ve/01-kien-truc-tong-the.md) | **Hai profile chạy** (Tauri nhúng core vs binary standalone) và vì sao chúng không tương đương; sơ đồ kiến trúc tổng thể; diễn giải từng khối; bảng thành phần — công nghệ — tiến trình — trạng thái; ba lát cắt kiến trúc đáng chú ý | 1 mermaid |
| [02-giao-thuc-ipc-va-websocket.md](01-ban-ve/02-giao-thuc-ipc-va-websocket.md) | **Hợp đồng giao thức** cho mọi client: `AppState`, vòng đời khởi động 25 bước, WebSocket server + stdio IPC, khung nhị phân `VoiceFrame`, hai giao thức text trên cùng socket, **bảng 42 lệnh `handle_command`**, khung streaming, lệnh UI gửi mà core không có handler, đối chiếu thiết kế gốc vs as-built, checklist viết client | 5 mermaid |
| [03-duong-ong-thoai.md](01-ban-ve/03-duong-ong-thoai.md) | Toàn chuỗi mic → AEC → GTCRN denoise → Silero VAD → Smart Turn shadow / wake gate → STT → agent+LLM → TTS → loa; hai hệ wake word song song; barge-in bốn lớp; bảng timing từ hằng số trong code; bảng env đường ống thoại | 3 mermaid |
| [04-he-llm-va-prompt.md](01-ban-ve/04-he-llm-va-prompt.md) | Kiến trúc engine LLM, `swap_model`/hot-swap, đường đa phương thức Qwen3-VL, prefix-cache + sliding window, sampler, embedding, persona và ba lớp chống prompt-injection, ba đường streaming token; **giới hạn cốt lõi: một engine / một context / một Mutex dùng chung**; router vs expert **[THIẾU]** | 5 mermaid |
| [05-agent-bo-nho-va-tien-hoa.md](01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) | Hai tầng máy trạng thái, `StateGraph` + `build_pipeline_graph`, router phân loại ý định, `memory.rs` như checkpointer, swarm dispatcher và `evolution/` (đều mồ côi), tool/skill calling, ranh giới nối dây, tóm tắt rủi ro tầng agent | 5 mermaid |
| [06-thi-giac-passive-va-governor.md](01-ban-ve/06-thi-giac-passive-va-governor.md) | Chụp màn hình `vision/capture.rs`, hai thuật toán diff độc lập, đường nối ảnh → Qwen3-VL, module `passive/` (hook bàn phím/chuột), `governor.rs` (fullscreen **và** tải CPU thật) và ảnh hưởng lên LLM/TTS/vision, bảng tra cứu nhanh file | 7 mermaid |
| [07-tang-du-lieu-va-bao-mat.md](01-ban-ve/07-tang-du-lieu-va-bao-mat.md) | ERD SQLite 15 bảng (9/15 không có câu lệnh ghi nào), pool + PRAGMA + WAL, `crypto.rs` AES-256-GCM với ba vấn đề, ba két bí mật đều không sống, cấu trúc `data/`, `prng.rs`, `.gitignore`/`.aiexclude`, rủi ro bảo mật quan sát được | 1 mermaid (ERD) |
| [08-frontend-va-vo-tauri.md](01-ban-ve/08-frontend-va-vo-tauri.md) | Ba entry Vite (chỉ hai được build), `useGateway.ts` dual transport, `useVoicePipeline.ts` (ScriptProcessorNode chứ không AudioWorklet), playback loa, adapter `platform/`, Ghost Mode click-through, avatar VRM/Three.js vs model FBX thật, bảng đầy đủ màn hình Dashboard, i18n/logger/safeFetch, tám lệnh Tauri, `tauri.conf.json` + CSP + capabilities, cách nhúng core in-process | 4 mermaid |
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

---

## 03-danh-gia/ — Đánh giá, rủi ro và lộ trình

Mô tả **hệ thống đang ở đâu so với những gì nó tuyên bố**, và phải làm gì tiếp.

| Tài liệu | Nội dung | Sơ đồ |
|---|---|---|
| [00-bao-cao-khao-sat-goc-2026-07.md](03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) | **Báo cáo khảo sát gốc** — nguồn mà toàn bộ bộ tài liệu này được biên tập ra. Khảo sát 18 khu vực mã nguồn, 4 vòng phản biện chéo, mọi khẳng định kèm trích dẫn `file:dòng`. Giữ nguyên để đối chiếu khi nghi ngờ một chi tiết đã bị biên tập sai | 6 mermaid |
| [01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) | Đặt từng tuyên bố trong `README.md`, `data/liva-config.json`, `.env.example` cạnh bằng chứng đọc được từ code: bảng đối chiếu đầy đủ, ba claim sai nghiêm trọng nhất, kiểm chứng tuyên bố "100% offline", claim đúng nên giữ nguyên, chỗ README đang **dưới-báo cáo**, câu chữ thay thế đề xuất | 3 mermaid |
| [02-no-ky-thuat-va-rui-ro.md](03-danh-gia/02-no-ky-thuat-va-rui-ro.md) | Kiểm kê rủi ro xếp hạng CRITICAL → HIGH → MEDIUM → LOW, mỗi mục kèm `file:dòng` tự kiểm chứng được; danh sách **code mồ côi** (có code, 0 call-site); đối chiếu với `tech-debt-ledger.json`; ba việc nên làm trước khi phát hành cho beta tester | 2 mermaid |
| [03-lo-trinh-sua-loi-va-nang-cap.md](03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) | **Tài liệu hành động**: nguyên tắc ưu tiên, bản đồ 5 giai đoạn (trước beta → khớp tuyên bố → nối dây thứ có sẵn → dọn dẹp → ba trụ cột), hướng dẫn sửa chi tiết cho 5 việc ưu tiên cao nhất kèm code đề xuất, bảng tổng hợp ưu tiên | 3 mermaid |

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
3. **Cập nhật khối header** ở đầu file: đổi `**Cập nhật:**` sang ngày mới và `**Trạng thái:**` sang commit hash mới. Nếu nguồn không còn là đợt khảo sát 31 agent thì sửa luôn dòng `**Nguồn:**` cho trung thực.
4. **Đổi nhãn khi trạng thái nối dây đổi.** Bật một tính năng opt-in thành mặc định thì nâng **[MỘT PHẦN]** → **[OK]**; xoá code mồ côi thì gỡ hẳn mục **[THIẾU]** tương ứng khỏi cả `03-danh-gia/02-no-ky-thuat-va-rui-ro.md`.
5. **Kiểm lại toạ độ `file:dòng`.** Sau khi sửa code, số dòng trong tài liệu thường lệch. Tối thiểu phải kiểm những trích dẫn nằm trong file vừa sửa.
6. **Sửa mục lục này** nếu thêm/xoá/đổi tên file: cập nhật bảng tương ứng **và** chuỗi đọc trong mục "Bắt đầu từ đâu".
7. **Cập nhật mục `## Liên quan`** ở cuối mỗi file bị ảnh hưởng — liên kết chéo phải luôn dùng **đường dẫn tương đối** so với vị trí file đó.
8. **Không sửa file trong `99-luu-tru/`.** Chúng là ảnh chụp lịch sử. Nếu một tài liệu hiện hành trở nên lỗi thời, hãy chuyển nó vào lưu trữ và ghi lý do trong [99-luu-tru/README.md](99-luu-tru/README.md).
9. **Không commit tự động.** Theo quy ước dự án (`AGENTS.md`), chỉ chạy `git commit`/`git push` khi người dùng yêu cầu rõ ràng.

---

## Liên quan

- [01-ban-ve/00-tong-quan-he-thong.md](01-ban-ve/00-tong-quan-he-thong.md) — cửa vào nội dung kỹ thuật
- [01-ban-ve/01-kien-truc-tong-the.md](01-ban-ve/01-kien-truc-tong-the.md) — kiến trúc và hai profile chạy
- [01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md](01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) — bản đồ tìm đường trong mã nguồn
- [02-van-hanh/03-trien-khai-va-runtime.md](02-van-hanh/03-trien-khai-va-runtime.md) — cách chạy đúng
- [03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md](03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — tuyên bố vs thực tế
- [03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md](03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) — việc cần làm tiếp
- [99-luu-tru/README.md](99-luu-tru/README.md) — cảnh báo tài liệu lỗi thời
