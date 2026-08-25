---
title: "Quy hoạch và di trú tài liệu LIVA v2"
updated: 2026-08-25
commit: ac693be8
stale-ok: 4ae8bfb6
status: living
owns:
  - quy-hoach-tai-lieu-v2
covers:
  - docs/README.md
  - docs/_meta/nguon-su-that.md
  - docs/_meta/huong-dan-bao-tri.md
  - scripts/docs-check.mjs
  - scripts/docs-citations.mjs
  - scripts/docs-capabilities.mjs
  - scripts/docs-inventory.mjs
  - docs/_data/capabilities.json
  - docs/_data/document-inventory.json
---
# Quy hoạch và di trú tài liệu LIVA v2

[⬆ Mục lục](../README.md) · [Sổ nguồn sự thật](../_meta/nguon-su-that.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

## 1. Mục tiêu

Tạo một kiến trúc thông tin trong đó:

- một sự thật có một chủ;
- trạng thái capability có một nguồn máy đọc được;
- as-built và target architecture không bị trộn;
- roadmap đang chạy tách khỏi báo cáo lịch sử;
- bảng có thể sinh từ code/data thì không viết tay;
- tài liệu cũ vẫn mở được trong suốt quá trình di trú.

## 2. Cấu trúc đích

```text
docs/
├── README.md
├── 00-san-pham/
├── 01-kien-truc/
│   └── adr/
├── 02-hop-dong/
├── 03-he-thong-con/
├── 04-van-hanh/
├── 05-chat-luong/
├── 06-ke-hoach/
│   ├── epics/
│   └── decisions/
├── 07-dong-gop/
├── _data/
├── _generated/
└── 99-luu-tru/
```

## 3. Nguồn chuẩn theo loại thông tin

| Loại | Chủ sở hữu |
|---|---|
| Tầm nhìn và nguyên tắc sản phẩm | `00-san-pham/tam-nhin-jarvis.md` |
| Trạng thái capability | `_data/capabilities.json` |
| Ma trận đọc cho người | `_generated/ma-tran-nang-luc.md` |
| Kiến trúc đích | `01-kien-truc/cognitive-runtime.md` |
| Việc còn làm và thứ tự | `06-ke-hoach/roadmap.md` |
| As-built subsystem | `03-he-thong-con/<subsystem>.md` |
| SLO/risk/threat model | `05-chat-luong/` |
| Agent rules | `AGENTS.md` + Vault `Rules/` |
| Agent workflow | `.agents/skills/` |
| Lịch sử | `99-luu-tru/` |

## 4. Chiến lược di trú

### Đợt A — Dựng xương sống

- [x] Capability registry.
- [x] Generator và drift check.
- [x] Product vision.
- [x] Cognitive target architecture.
- [x] Master roadmap.
- [x] Mục lục chuyển tiếp.

### Đợt B — Inventory và gắn disposition

Mỗi tài liệu active nhận một trong năm nhãn:

- `KEEP` — giữ nguyên vị trí trong giai đoạn chuyển tiếp;
- `SPLIT` — quá nhiều concern, tách theo subsystem;
- `GENERATE` — dữ liệu phải sinh từ code/registry;
- `FREEZE` — snapshot có giá trị lịch sử;
- `MERGE` — nội dung trùng, nhập vào canonical owner.

Không di chuyển file trước khi có mapping nguồn → đích và danh sách link trỏ tới nó.

- [x] Kiểm kê capability → module/entry point/test.
- [x] Gắn disposition cho toàn bộ `docs/**/*.md`, gồm cả archive.
- [x] Ghi target cho mọi mục `SPLIT/MERGE`.
- [x] Sinh inbound-link count và gate phát hiện file chưa phân loại.

Nguồn máy đọc được là `docs/_data/document-inventory.json`; báo cáo đọc cho người là
[`docs/_generated/kiem-ke-tai-lieu.md`](../_generated/kiem-ke-tai-lieu.md).

### Đợt C — Di trú theo subsystem

Thứ tự để giảm rủi ro:

1. [x] voice + wake;
2. [x] agent + tools;
3. [x] memory;
4. [x] security + data;
5. [ ] vision + proactive;
6. [ ] integrations;
7. [ ] frontend/Tauri;
8. [ ] operations.

Một subsystem chỉ hoàn tất khi tài liệu mới có `owns`, `covers`, link từ mục lục và
tài liệu cũ đã chuyển thành redirect/tóm tắt hoặc `frozen`.

Voice + wake hoàn tất đợt di trú đầu với bốn nguồn chuẩn: `voice.md`, `voice-slo.md`,
`wake-word.md` và `wake-benchmark.md`. Citation checker đọc disposition từ inventory,
bỏ qua đúng các snapshot `FREEZE` đã xác nhận.

Agent + tools hoàn tất với `agent-tools.md` và `action-policy.md`; tài liệu tổng hợp agent,
memory và evolution cũ trở thành snapshot. Trần citation living-doc sau hai cụm là 359.

Memory hoàn tất với `memory.md` và bản `memory.html` để quét nhanh. Tài liệu mới tách checkpoint,
conversational RAG, facts và projection worker khỏi thiết kế semantic L0–L3 chưa triển khai. Trần
citation living-doc sau ba cụm vẫn là 359 vì tài liệu mới dùng symbol anchor.

Security + data hoàn tất bằng `persistence.md` và `threat-model.md`; snapshot 07 được đóng băng.
Nguồn mới phân biệt rõ durability/schema với trust boundary/crypto và sửa sự thật 15 bảng thành
schema v5 có 20 bảng. Cụm kế tiếp là vision + proactive.

### Đợt D — Tự động hóa và siết gate

- command catalogue sinh từ command ownership;
- DB schema sinh từ migration/schema source;
- model table sinh từ `models-manifest.json`;
- capability matrix đã sinh từ registry;
- giảm citation mơ hồ về 0 trong living docs;
- strict stale cho các thư mục canonical mới.

## 5. Quy tắc không phá link

1. Không move hàng loạt trong một commit.
2. File cũ được giữ một chu kỳ release với banner `superseded_by`.
3. Sửa link inbound trước khi archive.
4. Không dùng find-and-replace để đổi symbol hoặc citation.
5. Không cập nhật `commit:` nếu chưa đọc diff của toàn bộ `covers`.
6. Báo cáo lịch sử giữ `status: frozen`, không “sửa cho khớp code mới”.

## 6. Frontmatter v2

Các trường hiện tại vẫn là bắt buộc:

```yaml
title:
updated:
commit:
status: living | frozen | index
owns:
covers:
```

Các trường v2 sẽ được thêm sau khi `docs-check` hiểu chúng:

```yaml
kind: product | architecture | contract | subsystem | operations | quality | roadmap
owner:
canonical_for:
supersedes:
review_interval_days:
verification:
```

Không thêm trường mới vào hàng chục file trước khi checker có validation; nếu không,
metadata sẽ trở thành trang trí.

## 7. Gate chấp nhận

| Gate | Lệnh |
|---|---|
| Registry hợp lệ, evidence tồn tại | `npm run docs:capabilities:check` |
| Mọi tài liệu có disposition, báo cáo không drift | `npm run docs:inventory:check` |
| Frontmatter · liên kết tương đối · **neo `#anchor` nội bộ** (từ `98efc55`) · `owns` · `covers` · mermaid | `npm run docs:check` |
| Citation report, không vượt trần living-doc hiện hành | `npm run docs:cite` |
| Vault schema và wiki links | `npm run validate --workspace teamwork_projects/obsidian_llm_wiki` |
| Skill parity/governance | `npm run skills:audit` |
| Rà stale bằng AI | Gemini worker một doc/lượt → JSON verdict → Pro/human review; contract ở `docs/_meta/huong-dan-bao-tri.md` §7.5 |

Mục tiêu cuối:

- 0 broken link;
- 0 duplicate owner;
- 0 generated drift;
- 0 unchecked citation trong living docs;
- 0 active roadmap ngoài master roadmap;
- 1 canonical owner cho mỗi capability và subsystem.

## 8. Phạm vi đợt đầu

Đợt đầu **không**:

- xóa hoặc move tài liệu cũ;
- viết lại báo cáo frozen;
- sửa code runtime;
- khôi phục kiến trúc Node/Python;
- tự động đổi citation theo số dòng.

Đợt đầu tạo hạ tầng để các thay đổi trên có thể làm từng phần và kiểm chứng được.

AI không được tự bump SHA. `commit:` chỉ đi cùng patch nội dung; `stale-ok:` chỉ đi cùng danh sách
`coversReviewed` và bằng chứng đã đọc `git diff BASE..HEAD -- <covers>`. File security/architecture
hoặc diff rộng phải qua reviewer Pro, không giao chung với nhóm Flash hàng loạt.
