---
title: "Master roadmap LIVA → JARVIS"
updated: 2026-08-25
commit: f35961cf
stale-ok: ac693be8
status: living
owns:
  - master-roadmap-jarvis
covers:
  - docs/_data/capabilities.json
  - docs/00-san-pham/tam-nhin-jarvis.md
  - docs/01-kien-truc/cognitive-runtime.md
  - docs/03-he-thong-con/agent-tools.md
  - docs/03-he-thong-con/memory.md
  - docs/03-he-thong-con/persistence.md
  - docs/03-he-thong-con/voice.md
  - docs/03-he-thong-con/wake-word.md
  - docs/03-danh-gia/05-nang-cap-toan-dien.md
  - docs/03-danh-gia/06-nhan-tin-ra-ngoai.md
  - docs/03-danh-gia/07-wake-word-viec-con-lai.md
  - docs/05-chat-luong/action-policy.md
  - docs/05-chat-luong/threat-model.md
  - docs/05-chat-luong/voice-slo.md
  - docs/05-chat-luong/wake-benchmark.md
---
# Master roadmap LIVA → JARVIS

[⬆ Mục lục](../README.md) · [Tầm nhìn](../00-san-pham/tam-nhin-jarvis.md) · [Ma trận năng lực](../_generated/ma-tran-nang-luc.md)

## 1. Vai trò của tài liệu này

Đây là nguồn chuẩn duy nhất cho **việc còn phải làm và thứ tự thực hiện**. Các roadmap
cũ vẫn được giữ để bảo toàn bằng chứng và lịch sử, nhưng không được thêm hạng mục mới.

Trạng thái capability nằm trong `docs/_data/capabilities.json`; bảng Markdown được sinh
tự động. Tài liệu này sở hữu milestone, dependency và acceptance gate.

## 2. Nguyên tắc ưu tiên

Thứ tự cố định:

1. security và data integrity;
2. beta reliability;
3. action policy và audit;
4. latency và resource coexistence;
5. cognitive capability;
6. embodiment và autonomous behavior.

Không đưa agent swarm hoặc self-modifying code lên trước policy, memory provenance và
OS-level isolation.

## 3. Lộ trình tổng thể

| Giai đoạn | Thời gian solo full-time | Kết quả |
|---|---:|---|
| **GĐ0 — Nguồn sự thật và beta** | tuần 1–4 | Tài liệu, quyền desktop, installer và đường dữ liệu ổn định |
| **GĐ1 — Cognitive Runtime v1** | tuần 5–10 | Tool/action đi qua một policy và audit contract |
| **GĐ2 — Bộ nhớ nhận thức** | tuần 11–18 | Fact/relationship bền, provenance, conflict và quyền quên |
| **GĐ3 — Chủ động an toàn** | tuần 19–26 | ContextBroker opt-in, rule engine và notification budget |
| **GĐ4 — Hành động ngoài máy** | tuần 27–36 | Matter/messaging/cross-device có identity và xác nhận |
| **GĐ5 — Cá nhân hóa và hiện thân** | tuần 37–52 | Wake cá nhân, voice clone khả thi, lip-sync và expert routing |
| **GĐ6 — Tự tiến hóa có kiểm soát** | sau v1 | Isolation, review, rollback trước khi nối CodeAgent thật |

## 4. GĐ0 — Nguồn sự thật và beta

### Kết quả

Một bản release có thể giao cho beta tester, và người tiếp theo đọc tài liệu biết đúng
thứ gì đã chạy.

### Công việc

| ID | Trạng thái | Việc | Phụ thuộc | Acceptance |
|---|---|---|---|---|
| D0.1 | xong | Capability registry + matrix sinh tự động | — | `npm run docs:capabilities:check` |
| D0.2 | xong | Tạo product vision, cognitive architecture và master roadmap | D0.1 | `npm run docs:check` |
| D0.3 | xong | Di trú mục lục sang cấu trúc mới, giữ link cũ | D0.2 | không link gãy |
| D0.4 | xong | Phân loại toàn bộ docs: keep/split/generated/frozen/merge | D0.2 | `npm run docs:inventory:check` |
| D0.5 | đang làm — voice+wake, agent+tools, memory, security+data, vision+proactive+governor, desktop+frontend xong | Giảm citation mơ hồ theo từng subsystem | D0.4 | `npm run docs:cite` giữ trần 207 và tiếp tục giảm. **Đo 25/08/2026: 187 trích dẫn mơ hồ / trần 207** — còn 20 slot, nên mỗi tài liệu mới thêm toạ độ `lib.rs`/`main.rs`/`engine.rs` trần trụi là ăn dần vào phần đệm đó |
| D0.6 | xong 2026-07-30 | Bật SQLite foreign keys và kiểm orphan/cascade | persistence D0 | FK/integrity/cascade tests xanh |
| D0.7 | xong cho local beta 2026-07-31 | Backup/restore và retention/delete propagation | D0.6 | restore drill + raw DB/WAL xanh; DeleteSubject local, Settings execute và retention opt-in bounded-batch đã có; non-local fail-closed |
| A31-01 | xong 2026-07-31 | Làm sạch Rust/npm supply chain và khóa dependency dễ tổn thương | — | `cargo audit` + full/runtime `npm audit` đều exit 0; test/build các workspace xanh |
| A31-02 | xong 2026-07-31 | Mã hóa transcript/checkpoint, khép CSP và diễn tập backup cùng key recovery | S0.2–S0.4 | ADR-001; raw DB/WAL/backup không có canary; restore key-ID fail-closed |
| A31-03 | xong 2026-07-31 | Đưa full npm audit, `cargo fmt --check` và `cargo audit` vào CI bắt buộc | A31-01 | local gate và workflow YAML gate cùng xanh. **Cập nhật 08/08/2026 (`f35961cf`): `cargo audit` đã bị thay bằng `cargo deny check -W unmaintained -W unsound advisories licenses sources` — cùng vai trò advisory, thêm license + source compliance. Nghiệm thu gốc giữ nguyên vì nó mô tả việc đã làm ngày 31/07** |
| A31-04 | milestone 1 xong 2026-07-31 | Tách command/deletion/test khỏi god-file | — | không file >2.000; `handle_command` 140 dòng; file >1.000 giảm 14→13; lib 505 pass + 1 ignored, bin 30 pass |
| A31-05 | mở — delivery risk | Chia working tree thành review slices | A31-04 | user tự commit theo lát; agent không commit/push theo git boundary |
| A31-07 | xong 2026-07-31 | Đối chiếu và quy hoạch tài liệu living; tách các cụm canonical | D0.4 | `npm run docs:check` sạch, inventory đủ 96 tài liệu, citation mơ hồ còn 207 |
| A31-06 | xong 2026-07-31 | Nâng coverage ba hotspot UI và khóa ngưỡng per-file | — | Widget 70,27%, useGateway 50,13%, VisionView 100% line; 285 test xanh |
| A31-08 | xong 2026-07-31 | Pin AI DevKit và thêm compatibility pointers không nhân đôi docs | D0.4 | `devkit:lint` + memory search + npm audit xanh; gate nằm trong CI |
| S0.1 | xong 2026-07-31 | Tách capability Tauri theo widget/dashboard/setup | D0.2 | 5 negative/static permission tests xanh |
| S0.2 | xong 2026-07-30 | Nối credential UI vào Stronghold write-only, cấm secret trong config | threat model S0 | adapter/UI/core tests + production build |
| S0.3 | xong 2026-07-31 | Identity/authorization cho command plane | S0.2 | Tauri label/capability cấp session ticket WS 256-bit TTL 30s single-use; mặc định WS remote; principal query nhận 403; replay/expired/non-loopback negative tests xanh |
| S0.4 | xong 2026-07-31 | Ghim model/vec0 dưới canonical trust root + manifest | S0.3 | traversal/junction/tamper/hash tests xanh; không còn cwd/bare DLL |
| M0.1 | xong 2026-07-31 | Persist messaging outbox xuống SQLite | migration DB | ciphertext sống qua pool restart, sai khóa fail-closed, consume một lần |
| O0.1 | xong 2026-07-31 | Preflight **luồng setup/installer** và lỗi boot có hướng khắc phục — *không phải* màn Hệ thống trong Dashboard, xem ghi chú dưới bảng | installer/preflight | fixture thiếu model blocking; setup/capability/installer + boot hint tests xanh |
| V0.1 | mở | Đo voice SLO trên vỏ Tauri release | model thật | log p50/p95 và barge-in |
| W0.1 | xong 2026-07-31 | Chỉ dùng “Hey Liva” kèm lệnh trong UX beta | — | regression gate khóa README/widget/trang thử mic; UI không hứa câu gọi đứng riêng |
| W0.2 | artifact tổng hợp xong, corpus thật mở | Nghiệm thu “Hey Liva” v2 trên giọng thật | 20+ positive + 1 giờ negative | v2 conv-attention/hash pin đạt 91,82% recall, 0,0773 FPPH trên 25,88h tổng hợp; còn gate mic mục tiêu |
| I0.1 | mở | Chạy Telegram E2E với tài khoản test | token test | send + receive + fail-closed allow-list |

> **Hai thứ tên "preflight", đừng gộp.** **O0.1** (xong 31/07) là preflight của **luồng setup/installer** — chặn khi thiếu model, gợi ý khắc phục lúc boot. Màn **Hệ thống trong Dashboard** hiển thị trạng thái tài nguyên là việc khác, thuộc mục [U3](../03-danh-gia/05-nang-cap-toan-dien.md) và chỉ đóng ngày **07/08/2026** (`SystemView.vue` + `useGateway.ts`); trước đó bề mặt duy nhất là cờ CLI `--preflight`. Đọc O0.1 thành "Dashboard đã có màn preflight từ 31/07" là sai — chính chỗ này từng gây nhầm.

Mốc D0.5: voice+wake, agent+tools, memory, security+data, vision+proactive+governor và
desktop+frontend đã có nguồn chuẩn; các khảo sát cũ tương ứng được đóng băng. Kế hoạch kỹ thuật
D0–D4 nằm tại
[Persistence runtime](../03-he-thong-con/persistence.md), S0–S5 nằm tại
[Threat model](../05-chat-luong/threat-model.md). Cụm tài liệu kế tiếp là contracts +
integrations.

### Gate ra

- Không capability desktop nào rộng hơn cửa sổ cần dùng.
- Outbox, memory và config không mất do restart hoặc đổi working directory.
- Installer clean-machine và model recovery có bằng chứng.
- Tài liệu living không chứa một lỗi đã biết là sai.
- Test, clippy, gateway E2E, docs, Vault và skill audit xanh.

## 5. GĐ1 — Cognitive Runtime v1

### Công việc

1. Định nghĩa `PerceptionEvent`, `ActionProposal`, `PolicyDecision`,
   `ToolObservation` và `MemoryCandidate`.
2. Chuẩn hóa risk tier trong tool catalogue.
3. Bọc OS, MCP và messaging bằng một executor contract.
4. Thêm action id, idempotency, timeout, cancellation và audit.
5. Nối retrieval threshold vào đường chat thật.
6. Khóa corpus tool selection tiếng Việt/Anh.
7. Chỉ bật LLM tool-calling mặc định khi chat thông thường không trả thêm lượt LLM.

### Gate ra

- ≥95% tool selection trên corpus versioned.
- 100% external/physical side effect tuân policy.
- Retry không tạo side effect trùng.
- Audit không chứa secret.
- Reflex lane vẫn hoạt động khi LLM/tool-calling tắt.

## 6. GĐ2 — Bộ nhớ nhận thức

Kiến trúc as-built, khoảng trống và thứ tự M0–M5 nằm tại
[Memory runtime](../03-he-thong-con/memory.md); mục này chỉ sở hữu milestone cấp chương trình.

### Công việc

1. Semantic consolidator trên event đã finalized.
2. Fact/entity/relation có provenance, confidence và effective time.
3. Conflict queue thay vì ghi đè im lặng.
4. L3 graph writer.
5. Memory UI: xem nguồn, sửa, khóa, quên, export.
6. Delete propagation cho vector, metadata và relation.
7. Background worker chịu preemption khi voice/session hoạt động.

### Gate ra

- Ba sự thật ở ba phiên được nối đúng ở phiên thứ tư.
- Xung đột không tự động phá fact cũ.
- Xóa memory loại bỏ mọi projection dẫn xuất.
- Không tăng TTFT hoặc contention DB trên đường chat.

## 7. GĐ3 — Chủ động an toàn

### Công việc

1. Thay raw passive hook bằng ContextBroker có sensor permission.
2. Presence indicator và kill switch.
3. Quiet hours, cooldown, notification budget.
4. Deterministic rule engine trước proactive LLM.
5. “Why now?” cho mọi gợi ý.
6. Vision memory opt-in, không lưu screenshot mặc định.

### Gate ra

- Không sensor event trước consent.
- Tắt quyền dừng thu thập ngay.
- Mọi suggestion có trigger giải thích được.
- Idle CPU/RAM/GPU nằm trong SLO.

## 8. GĐ4 — Hành động ngoài máy

Thứ tự adapter:

1. Matter bridge cục bộ;
2. Home Assistant cho thiết bị cũ;
3. email/calendar qua API chính thức;
4. messaging profile adapters;
5. mobile pairing;
6. encrypted cross-device sync.

Gate: physical action có confirmation/rule, read-after-write, identity, scope và trạng
thái `unknown` khi không đọc lại được.

## 9. GĐ5 — Cá nhân hóa và hiện thân

- Train wake classifier bằng giọng người dùng và khóa recall/FPPH.
- Tìm đúng MOSS encoder + speaker encoder tương thích VieNeu.
- Chỉ thiết kế onboarding clone voice sau khi pipeline model được kiểm bằng tai.
- Lip-sync từ audio.
- Automatic router↔expert chỉ khi đo được một tập ca 2B sai và expert đúng.
- Avatar phản ánh listening/thinking/confirming/acting, không chỉ animation trang trí.

## 10. GĐ6 — Tự tiến hóa

Chỉ bắt đầu khi có:

- process/OS sandbox thật;
- repo/worktree isolation;
- patch provenance;
- human review;
- deterministic rollback;
- network/file scopes;
- giới hạn thời gian và chi phí;
- acceptance test chứng minh test đỏ → patch → test xanh.

`evolution/` tiếp tục ở feature experimental cho tới khi đủ toàn bộ điều kiện trên.

## 11. 90 ngày đầu

| Tuần | Trọng tâm | Đầu ra |
|---|---|---|
| 1–2 | Registry, master docs, inventory, source ownership | bộ xương tài liệu v2 |
| 3–4 | Tauri permissions, outbox persistence, preflight, clean-machine | beta baseline |
| 5–6 | Typed action contract, policy, audit | executor v1 |
| 7–8 | Retrieval threshold, tool corpus, cancellation/idempotency | tool runtime mặc định có điều kiện |
| 9–12 | Semantic consolidator, provenance, memory UI | cognitive memory v1 |

## 12. Ứng viên nâng cấp từ điểm tin R&D 2026-08-07

Các mục dưới đây là **ứng viên có gate**, chưa phải cam kết triển khai. Kiến trúc chuẩn vẫn là
Unified Native Engine bằng Rust; không đưa Python/PydanticAI trở lại đường backend và không thêm
shared-memory framework trước khi benchmark chứng minh JSON/copy là nút thắt.

| ID | Ưu tiên | Ứng viên | Phạm vi đúng | Dependency | Acceptance trước khi nhận vào milestone |
|---|---:|---|---|---|---|
| RD-01 | P1 | Structured tool decision native Rust | Thêm constrained output nếu backend hỗ trợ; nếu không, giữ parser/JSON Schema hiện tại và cho phép tối đa một lượt repair khi parse/schema lỗi | action policy, corpus tool selection | output sai không bao giờ thực thi; tối đa 1 retry; ≥95% chọn đúng trên corpus versioned; đo p50/p95 TTFT; không có Python runtime mới |
| RD-02 | P1 | Session-aware LLM scheduling | Đo lock hiện tại, prefix-cache, context/sequence isolation và batching cho 2–4 phiên đồng thời; không chia sẻ raw KV pointer giữa agent | benchmark TTFT/throughput, VRAM governor | có baseline một phiên và nhiều phiên; không lẫn context; không OOM; chỉ nhận phương án cải thiện p95/throughput có ý nghĩa trên máy mục tiêu |
| RD-03 | P2 | Binary data plane cho payload lớn | Giữ typed JSON cho control plane; chỉ thử packed binary/borrowed buffer cho ảnh, audio hoặc tensor lớn qua Tauri/WebSocket | S0.3 command identity, benchmark payload | benchmark chứng minh serialization/copy là hotspot; schema có version, kích thước trần, backpressure và fuzz/negative tests; không đổi đường text nhỏ sang Arrow |
| RD-04 | P2 | Cầu nối LIVA ↔ Anima Engine | Typed/authenticated control messages; binary snapshot/telemetry riêng; shared-memory ring chỉ là bước sau nếu binary transport chưa đạt SLO | contract hai repo, cancellation, auth | không chia sẻ pointer; reconnect/failure fail-closed; producer không chặn simulation; benchmark end-to-end có p95 và wire bytes |
| RD-05 | P3 | Hunyuan3D như job tạo asset | Tool/job tùy chọn, chạy tách khỏi chat; unload model LIVA trước khi chiếm GPU; output qua kiểm tra mesh/license/provenance | VRAM governor, artifact trust | thử bản nhỏ trên GPU mục tiêu; không OOM; job hủy được; asset GLB đọc lại được; license model/input/output được ghi nhận |

### Không đưa vào roadmap thực thi

- **ZCAO**: chưa tìm thấy paper/repository/DOI tương ứng với tiêu đề trong bản tin. Apache Arrow là
  công nghệ thật nhưng không tự cung cấp orchestration hay chia sẻ KV-cache VRAM an toàn. Chỉ đánh
  giá Arrow cho telemetry dạng bảng sau khi có benchmark.
- **PydanticAI framework**: có giá trị tham khảo về typed output, validation và observability, nhưng
  tích hợp trực tiếp sẽ tái tạo Python boundary đã được loại bỏ. Chỉ port pattern sang Rust.
- **IBM Quantum real-time decoding và Classiq Quantum Engineering Agents**: theo dõi nghiên cứu,
  không có workload sản phẩm hoặc bằng chứng lợi thế cho LIVA hiện tại.

Nguồn upstream đã kiểm ngày 2026-08-07:
[Apache Arrow](https://github.com/apache/arrow),
[PydanticAI](https://github.com/pydantic/pydantic-ai),
[Hunyuan3D-2](https://github.com/Tencent-Hunyuan/Hunyuan3D-2),
[IBM Relay-BP trên FPGA](https://www.ibm.com/quantum/blog/qdc-2025) và
[Classiq Quantum Engineering Agents](https://www.classiq.io/blog).

## 13. Roadmap cũ và trạng thái

| Tài liệu | Trạng thái chuyển tiếp |
|---|---|
| `03-lo-trinh-sua-loi-va-nang-cap.md` | lịch sử các bản vá F/P; không thêm mục mới |
| `05-nang-cap-toan-dien.md` | bằng chứng U1–U20; tách việc còn mở vào master roadmap |
| `06-nhan-tin-ra-ngoai.md` | nguồn kỹ thuật messaging cho tới khi subsystem doc mới hoàn tất |
| `07-wake-word-viec-con-lai.md` | nguồn số đo wake cho tới khi subsystem doc mới hoàn tất |
| `teamwork_projects/liva_upgrade_plan/upgrade_plan.md` | superseded; sẽ đưa vào archive |

> Việc di chuyển/xóa chỉ thực hiện theo
> [quy hoạch tài liệu](../07-dong-gop/quy-hoach-tai-lieu.md), không làm hàng loạt
> bằng tìm-thay-thế.
