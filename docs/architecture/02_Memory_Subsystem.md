# 02. Phân hệ Lưu trữ Ký ức — LIVA-UHM v3 (Unified Hierarchical Memory)

> Phiên bản: v21 (2026-05-11) — AgeMem + DLQ 3-Strike + PRAGMA Tuning + Snapshot Backup

## 1. Kiến trúc Não bộ Hợp nhất (Consolidated Brain)

Toàn bộ não bộ của AI được tập trung vào **một tệp SQLite duy nhất** (`node:sqlite` DatabaseSync) với khả năng tìm kiếm vector thông qua C-Extension `sqlite-vec` và Full-text search thông qua `FTS5`.

Kiến trúc này được phân cấp thành **4 tầng** (L0 → L3), vận hành theo nguyên tắc **zero LLM extra calls** cho metadata và **zero circular dependency** giữa các module.

```text
L0: TurboQuantStore (RAM)     — Working memory, quantized KV cache (in-process)
L1: StructuredMemory (SQLite) — Event bricks (Φ Factual + Ψ Relational) + KV facts
L2: VectorMemory (sqlite-vec) — Consolidated narratives + H-MEM Positional Index (source_event_ids → L1)
L3: PersonalKnowledge (KV)    — Insights người dùng + Ebbinghaus Forgetting Curve
```

---

## 2. Các Tầng Ký Ức (Memory Layers)

### L0: TurboQuantStore (Working Memory)
- **Vị trí**: RAM (In-process).
- **Chức năng**: Lưu trữ ngữ cảnh làm việc tức thời của Agent.
- **Công nghệ**: Bounded FIFO cache, lưu trữ tin nhắn hiện tại và các token lượng tử hóa. Bị cắt xén (Evicted) liên tục để chống tràn RAM.

### L1: StructuredMemory (Event Bricks & KV Facts)
- **Vị trí**: Tệp SQLite (`StructuredMemory.sqlite`).
- **Chức năng**: Ghi nhận các đoạn đối thoại thô vào bảng `turn_layer_nodes` dưới dạng Event Bricks (Φ Factual + Ψ Relational). Đồng thời lưu trữ Key-Value Facts.
- **Cơ chế**: WAL Mode (`PRAGMA journal_mode = WAL`) + Debounced Writes.
- **[UHM] Ebbinghaus Forgetting Curve**:
  - Cột `memory_strength` (REAL DEFAULT 1.0) và `last_accessed_at` (INTEGER) trên bảng `facts`.
  - Tính toán decay bằng `Math.exp()` trong V8 (SQLite KHÔNG có `EXP()`).
  - RAM-buffered touch tracking: `touchFact()` gom batch trong RAM, flush atomic khi shutdown.
  - Facts với `strength < 0.2` bị loại khỏi System Prompt. Facts dưới `0.1` bị archive (xóa).
  - `setFact()` conflict resolution: `MAX(old_strength, 0.8)` — ngăn decay-on-overwrite.
  - **[G11]** `applyMemoryDecay()` là **async** với chunking 500 dòng + `setImmediate` yield để tránh block Event Loop.

### L2: VectorMemory & Reconsolidation (Semantic Context)
- **Vị trí**: Cùng tệp SQLite (Extension `sqlite-vec`).
- **Chức năng**: Lưu trữ tri thức đúc kết (Narratives), tìm kiếm vector (Cosine similarity) cho RAG.
- **[UHM] H-MEM Positional Index**:
  - Cột `source_event_ids` (TEXT DEFAULT '[]') trên bảng `vectors_meta` — trỏ ngược về L1 events (max 50 IDs).
  - `searchWithDrilldown(vec, k)`: Trả kết quả vector + sourceEventIds cho L1 drill-down.
  - `collectDrilldownEventIds(vec, k)`: Tập hợp deduplicated event IDs qua `json_each()` SQL — bypass giới hạn 999 biến của SQLite.
  - **[G5]** JSON validation bằng Zod: `EventIdsSchema = z.array(z.string()).max(50)` + `safeExtractJSON()` — ngăn LLM garbage crash `json_each`.
- **Thành phần tham gia**:
  - `ReflectionDaemon`: Trích xuất Φ/Ψ bất đồng bộ, debounce 12s micro-batch.
  - `DualChannelSegmenter`: Cắt văn bản song song bằng GPU.
  - `ReconsolidationEngine`: Đánh giá lại vector khi có thông tin mới.

### L3: PersonalKnowledge & HeraCompass (Long-term Strategy)
- **Chức năng**:
  - `PersonalKnowledgeExtractor`: Tự động tìm sở thích, thói quen cốt lõi → lưu KV, nhúng System Prompt.
  - `HeraCompass`: Lưu log sửa lỗi từ Skill. Tự sinh Rule tránh lỗi lần sau, có Utility Score (auto-GC nếu < -2).

---

## 3. Quá trình Đồng hóa (ConsolidationCron)

`ConsolidationCron` đóng vai trò "giấc ngủ" của AI — hợp nhất L1→L2+L3.

### Trigger Modes
| Trigger | Điều kiện | Mô tả |
|---------|-----------|-------|
| **Idle** | 30 phút không tương tác | Check mỗi 5 phút, fire khi idle ≥ 30 min |
| **Passive Signal** | `topicShiftCount >= 3` HOẶC `unconsolidatedCount >= 20` | 15s debounce, zero LLM calls |
| **Cold-start** | `getUnconsolidatedCount() >= 10` lúc boot | Dọn orphaned events từ session trước |
| **Manual** | `consolidateNow()` | Debug/skill trigger |

### VRAM Guard (Dual Layer)
```text
🚨 ConsolidationCron CHỈ kích hoạt khi ĐỒNG THỜI thỏa:
  1. this.isRunning === false     (không đang consolidate)
  2. agentLoopStateGetter() === 'IDLE'  (LLM không đang stream/think)

→ Ngăn OOM khi LLM chiếm 100% VRAM mà embedding API cũng chạy song song.
```

### Pipeline
1. Nhóm events thành sessions (gap 30 phút).
2. LLM tổng hợp narrative summary + personal insights per session.
3. Embed narrative → L2 vectors (`AXIOM`, `ANCHOR`).
4. Extract insights → L3 KV facts.
5. GC consolidated events > 7 ngày.
6. Dynamic Taxonomy Auto-Expansion.
7. WAL Checkpoint PASSIVE.
8. DLQ retry.
9. **[UHM] Ebbinghaus Memory Decay** (async, chunked).

---

## 4. Hệ thống Tín hiệu Thụ động (Passive Signal Architecture)

LIVA-UHM v2 sử dụng kiến trúc **Passive Signals** thay vì Sentiment Analysis để trigger consolidation sớm.

### Tại sao không dùng Sentiment?
- Phân tích cảm xúc đòi hỏi thêm LLM call → tốn VRAM, token, làm chậm chat.
- Vi phạm nguyên tắc "zero extra LLM calls" của SemanticRouter.

### MemoryEventBus (Node.js EventEmitter)
```text
ReflectionDaemon                    ConsolidationCron
  │                                     │
  ├── insertEvent(brick)                │
  ├── memoryEvents.emit('NEW_TURN') ──→ memoryEvents.on('NEW_TURN')
  │                                     ├── recordActivity('NEW_TURN')
  │                                     └── scheduleAffectiveCheck() [15s debounce]
  │
DualChannelSegmenter
  ├── memoryEvents.emit('TOPIC_SHIFT') → memoryEvents.on('TOPIC_SHIFT')
                                        ├── recordActivity('TOPIC_SHIFT')
                                        └── topicShiftCount++
```

**Lợi ích**: Zero import coupling, zero circular dependency, zero LLM calls.

---

## 5. Luật Thép UHM (Guardrails)

| ID | Luật | Lý do |
|---|---|---|
| G1 | Không dùng hàm toán SQLite | `Math.exp()` chỉ trong V8 — SQLite không có `EXP()` |
| G2 | RAM-buffered fact touches | Tránh write amplification trên hot paths |
| G3 | `json_each()` cho variable binding | Bypass giới hạn 999 parameter của SQLite |
| G4 | Cap `sourceEventIds` tối đa 50 | Tránh overflow VRAM/RAM trên vector meta |
| G5 | Zod-validated `source_event_ids` | `EventIdsSchema.safeParse()` — ngăn LLM garbage crash `json_each` |
| G6 | 15s affective debounce | Ngăn event loop flooding |
| G7 | EventBus decoupling | `MemoryEventBus` — zero import coupling ReflectionDaemon ↔ ConsolidationCron |
| G8 | Atomic transactions | BEGIN/COMMIT/ROLLBACK cho batch writes |
| G9 | Dual VRAM guard | `isRunning` + `agentLoopStateGetter() === 'IDLE'` |
| G10 | Shutdown flush guarantee | `flushFactTouches()` → `flushPending()` → `db.close()` |
| G11 | Chunked decay + `setImmediate` yield | 500-row chunks ngăn block Event Loop > 10ms |
| G12 | `VACUUM INTO` cho backup | KHÔNG BAO GIờ dùng `fs.promises.cp` trên SQLite đang chạy — WAL corruption |
| G13 | DLQ 3-Strike | Events fail Zod 3 lần → `consolidation_status='dlq'`, loại khỏi retry |
| G14 | AgeMem namespace isolation | Chỉ các category whitelisted được truy cập qua ManageMemory skill |

---

## 6. Quản lý Ký ức Chủ động (AgeMem — Agentic Memory)

LIVA-UHM v3 trao quyền CRUD trực tiếp cho Agent thông qua skill `ManageMemory`:

| Action | Mô tả | HITL |
|--------|-------|------|
| `add` | Thêm fact mới vào L1 KV | Không |
| `update` | Cập nhật fact, reset strength=1.0 | Không |
| `delete` | Xóa fact | **Bắt buộc** |
| `search` | Tìm kiếm fact theo key/value | Không |

**Guardrails**: Namespace isolation (5 categories), rate limit (5 ops/turn), audit trail (`source='agent_explicit'`).

---

## 7. DLQ 3-Strike & Snapshot Backup

### DLQ Schema
- `events.consolidation_status`: `'pending'` (mới) | `'consolidated'` (xong) | `'dlq'` (fail 3 lần)
- `events.retry_count`: 0→3 (tăng mỗi lần Zod fail)
- **Backward Compat**: Dữ liệu cũ DEFAULT `'consolidated'` — KHÔNG bị Thundering Herd
- Partial Index: `idx_events_pending` chỉ scan events `'pending'`

### Snapshot Backup
- Sử dụng `VACUUM INTO` (SQLite native) — atomic freeze + WAL merge
- Chạy sau mỗi consolidation thành công
- Pattern: tmp → rename (Rule 4.3)
- **Tuyệt đối KHÔNG dùng `fs.promises.cp`** trên SQLite WAL đang mở

### PRAGMA Tuning
- `cache_size = -8192` (8MB page cache)
- `wal_autocheckpoint = 500` (nhỏ hơn → cold-start nhanh)
- `mmap_size = 256MB` (chỉ Unix — Windows NTFS bị hard lock)

