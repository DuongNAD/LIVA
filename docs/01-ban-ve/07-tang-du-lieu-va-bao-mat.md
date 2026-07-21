---
title: "Tầng dữ liệu và bảo mật"
updated: 2026-07-21
commit: cc1c0cc
status: living
owns:
  - erd-sqlite
  - bang-15-bang-du-lieu
  - so-do-ma-hoa
covers:
  - data/*
  - liva-desktop/src-tauri/src/lib.rs
  - liva-native-core/src/*
  - liva-native-core/src/agent/memory.rs
  - liva-native-core/src/bin/verify_duplex.rs
  - liva-native-core/src/llm/embed.rs
  - liva-native-core/src/webrtc/pipeline.rs
  - liva-native-core/tests/verify_commands.rs
  - liva-ui/src/components/dashboard/*
  - liva-ui/src/platform/MockWebAdapter.ts
  - liva-ui/src/platform/TauriAdapter.ts
  - scripts/legacy/migration_stronghold.cjs
---
# Tầng dữ liệu và bảo mật

[⬆ Mục lục](../README.md) · [◀ Thị giác, passive và governor](06-thi-giac-passive-va-governor.md) · [Frontend và vỏ Tauri ▶](08-frontend-va-vo-tauri.md)

---

Tài liệu này mô tả toàn bộ tầng lưu trữ của LIVA: cơ sở dữ liệu SQLite (15 bảng), lớp mã hoá `EncryptionEngine` (AES-256-GCM), các két bí mật (Stronghold + Argon2id, `liva_vault.json`), và cấu trúc các file JSON trong `data/`.

Ba con số cần nhớ trước khi đọc chi tiết:

| Con số | Ý nghĩa | Kiểm chứng |
|---|---|---|
| **15** | Số bảng được `init_schemas` tạo ra | `db.rs:188-354`, đếm `CREATE TABLE`/`CREATE VIRTUAL TABLE` |
| **9/15** | Số bảng **không có một câu lệnh ghi nào** trong toàn bộ `src/*.rs` | grep `INSERT INTO|INSERT OR|UPDATE |DELETE FROM` toàn repo |
| **1** | Số cột duy nhất trong toàn DB được mã hoá (`facts.value`) | `db.rs:454`, `db.rs:514`, `lib.rs:876` |

Ký hiệu trạng thái dùng xuyên suốt: **[OK]** đang chạy thật · **[MỘT PHẦN]** có code nhưng tắt/opt-in/chưa nối dây · **[THIẾU]** chưa có/stub.

---

## 1. Sơ đồ ERD

Sơ đồ dưới đây gồm cả 15 bảng SQLite lẫn các file JSON trong `data/` (đặt tên hậu tố `_json`) để thấy toàn cảnh tầng lưu trữ. Quan hệ nét đứt (`||..o{`) là **liên kết logic**, không có khoá ngoại thật.

```mermaid
erDiagram
    facts {
        TEXT key PK
        TEXT value "AES-256-GCM ciphertext iv:tag:ct"
        TEXT createdAt
        TEXT updatedAt
        INTEGER ttlDays "khong co code quet TTL"
        TEXT source
        TEXT category
        REAL importance "default 0.5"
        REAL confidenceScore "default 1.0"
        TEXT sourceTurnId FK "logic -> turn_layer_nodes.turnId"
        REAL memory_strength
        INTEGER last_accessed_at
        INTEGER access_count
    }

    turn_layer_nodes {
        TEXT turnId PK
        INTEGER temporal_anchor "IX idx_turns_temporal"
        TEXT userMsg "plaintext"
        TEXT aiReply "plaintext"
        TEXT createdAt
        TEXT agentId "default liva_core"
    }

    events {
        TEXT eventId PK
        INTEGER timestamp
        TEXT phi_facts
        TEXT phi_entities
        TEXT psi_sentiment
        TEXT psi_intent
        TEXT psi_relational
        TEXT rawUserMsg "plaintext"
        TEXT rawAiReply "plaintext"
        INTEGER consolidated
        TEXT domain "default General"
        TEXT category "default Uncategorized"
        TEXT trace_keywords
        INTEGER last_accessed_at
        TEXT consolidation_status "default pending"
        INTEGER retry_count
        TEXT agentId "default liva_core"
    }

    vectors_meta {
        INTEGER id PK "AUTOINCREMENT = rowid"
        TEXT vec_id UK
        TEXT type "IX (type,domain,category)"
        TEXT content
        TEXT domain
        TEXT category
        TEXT trace_keywords "JSON array"
        TEXT file_target
        INTEGER created_at "epoch ms, IX"
        INTEGER last_accessed_at
        REAL decay_weight
        INTEGER access_count
        TEXT source_event_ids "JSON array, cap 50 -> events.eventId"
    }

    vec_idx {
        INTEGER rowid PK "= vectors_meta.id"
        INT8_384 embedding "vec0, int8 quantized, 384 chieu"
    }

    vectors_fts {
        INTEGER rowid PK "= vectors_meta.id"
        TEXT content "fts5 unicode61 remove_diacritics 0"
    }

    l3_nodes {
        TEXT id PK
        TEXT label
        TEXT properties "JSON, default {}"
    }

    l3_edges {
        TEXT source PK_FK "-> l3_nodes.id"
        TEXT target PK_FK "-> l3_nodes.id"
        TEXT relation PK
        REAL weight
        INTEGER obsolete
    }

    agent_checkpoints {
        TEXT thread_id PK "= session_id cua WebRTCActor"
        TEXT state_json "AgentState serialize, plaintext"
    }

    tasks {
        TEXT id PK
        TEXT title
        TEXT description
        TEXT status "pending/..."
        TEXT priority "default medium"
        TEXT result
        INTEGER created_at
        INTEGER updated_at
    }

    personality_state {
        TEXT agentId PK
        REAL valence
        REAL arousal
        REAL friendliness
        REAL verbosity
        REAL assertiveness
        INTEGER updatedAt
    }

    daily_briefings {
        TEXT id PK
        INTEGER created_at
        TEXT topics
        TEXT content
        INTEGER is_read
        TEXT source "default tavily"
        INTEGER expires_at
    }

    consolidation_checkpoints {
        TEXT session_id PK
        INTEGER last_step
        TEXT state_data "JSON"
        INTEGER created_at
        INTEGER updated_at
    }

    dlq_consolidation {
        INTEGER id PK
        TEXT session_id
        TEXT failed_step
        TEXT error_msg
        INTEGER retry_count
        TEXT status
        INTEGER created_at
    }

    vector_dlq {
        INTEGER id PK
        TEXT delete_filter
        TEXT status
        INTEGER retry_count
    }

    liva_config_json {
        JSON avatar "engineMode, live2dModel, vrmModel, activeModel"
        JSON ai "provider, localModelsDir, routerModel, mmprojModel, expertModel"
        JSON ui "widgetPosition, dashboardTheme, avatarMode"
        JSON system "proactive*, digest*"
        JSON voice "enabled, provider, activeProfile, language, sampleRate"
    }

    user_profile_json {
        TEXT name
        INTEGER birthYear
        TEXT nationality
        TEXT language
        TEXT hobbies
        TEXT preferences
        TEXT profession
        TEXT location
    }

    liva_vault_json {
        TEXT EMAIL_HOST "ciphertext iv:tag:ct"
        TEXT EMAIL_USER "ciphertext"
        TEXT EMAIL_PASS "ciphertext"
        TEXT TAVILY_API_KEY "ciphertext"
        TEXT TELEGRAM_BOT_TOKEN "ciphertext"
        TEXT ZALO_OA_ACCESS_TOKEN "ciphertext"
        TEXT ZALO_APP_ID "ciphertext"
        TEXT ZALO_APP_SECRET "ciphertext"
        TEXT GOOGLE_CLIENT_SECRET "ciphertext"
    }

    credentials_json {
        JSON installed "client_id, client_secret, auth_uri, token_uri - PLAINTEXT"
    }

    token_json {
        TEXT access_token "PLAINTEXT"
        TEXT refresh_token "PLAINTEXT"
        TEXT scope
        TEXT token_type
        INTEGER expiry_date
    }

    models_config_json {
        JSON llm "provider, model"
        JSON stt "provider, language"
        JSON tts "provider, voice"
    }

    skill_whitelist_json {
        BOOLEAN enabled "theo ten skill"
        INTEGER lastToggled "epoch ms"
    }

    vectors_meta ||--|| vec_idx : "id = rowid (1:1, upsert cung transaction)"
    vectors_meta ||--|| vectors_fts : "id = rowid (1:1, dong bo thu cong)"
    l3_nodes ||--o{ l3_edges : "FK source (khai bao, PRAGMA foreign_keys OFF)"
    l3_nodes ||--o{ l3_edges : "FK target (khai bao, khong thuc thi)"
    turn_layer_nodes ||..o{ facts : "turnId -> sourceTurnId (logic, khong FK)"
    events ||..o{ vectors_meta : "eventId -> source_event_ids JSON (logic)"
    turn_layer_nodes ||..o{ events : "cung luot noi (logic, khong khoa)"
    personality_state ||..o{ events : "agentId (logic)"
    personality_state ||..o{ turn_layer_nodes : "agentId (logic)"
    consolidation_checkpoints ||..o{ dlq_consolidation : "session_id (logic)"
    liva_config_json ||..|| models_config_json : "cung mo ta model, trung lap - models.config.json khong co reader"
    credentials_json ||..|| token_json : "OAuth Google: client -> token (khong co reader trong Rust)"
```

Sơ đồ ASCII tương đương (từ báo cáo gốc, giữ lại vì thể hiện rõ hơn phần quan hệ 1:1 của bộ ba vector):

```
                       ┌──────────────────────────┐
                       │ facts                    │  (value = AES-GCM ciphertext)
                       │ PK key                   │  ── không FK với ai
                       └──────────────────────────┘

  ┌───────────────────┐        (liên kết LOGIC, không FK)
  │ turn_layer_nodes  │  turnId ◄╌╌╌╌╌╌╌╌╌╌╌╌ facts.sourceTurnId
  │ PK turnId         │
  │ IX temporal_anchor│
  └───────────────────┘
           ╎ (không FK)
           ▼
  ┌───────────────────┐        eventId ╌╌╌► vectors_meta.source_event_ids  (JSON array, cap 50)
  │ events            │
  │ PK eventId        │
  │ IX (partial) x2   │
  └───────────────────┘

  ┌────────────────────────┐   1 : 1   ┌──────────────────────────────┐
  │ vectors_meta           │ ────────► │ vec_idx  (vec0, int8[384])   │
  │ PK id  (AUTOINCREMENT) │  id=rowid │ rowid, embedding             │
  │ UQ vec_id              │           └──────────────────────────────┘
  │ IX (type,domain,cat)   │   1 : 1   ┌──────────────────────────────┐
  │ IX created_at          │ ────────► │ vectors_fts (fts5)           │
  └────────────────────────┘  id=rowid │ rowid, content               │
                                       └──────────────────────────────┘

  ┌───────────┐  FK source   ┌───────────┐
  │ l3_edges  │─────────────►│ l3_nodes  │      (FK khai báo, nhưng
  │ PK(source,│  FK target   │ PK id     │       PRAGMA foreign_keys OFF)
  │  target,  │─────────────►│           │
  │  relation)│              └───────────┘
  └───────────┘

  Bảng độc lập, không quan hệ:
    agent_checkpoints(thread_id)   ← LIVE (webrtc pipeline)
    tasks(id)                      ← LIVE (IPC CRUD)
    daily_briefings(id)            ← không dùng
    personality_state(agentId)     ← không dùng
    consolidation_checkpoints(session_id), dlq_consolidation(id), vector_dlq(id) ← không dùng
```

---

## 2. Bảng dữ liệu — ai ghi, ai đọc

### 2.1 Bảng tổng hợp

| Bảng / File | Cột chính | Mục đích | Ai ghi | Ai đọc | Trạng thái |
|---|---|---|---|---|---|
| `facts` | `key` PK, `value` (ciphertext), `importance`, `memory_strength`, `sourceTurnId` | Bộ nhớ khoá–giá trị; **cột duy nhất trong toàn DB được mã hoá** | `db::set_fact` (`db.rs:467`) qua `memory:set_fact` (`lib.rs:991`) | `db::get_fact` (`db.rs:501`); `get_memory_data` (`lib.rs:871`); `db.rs:962` | **[MỘT PHẦN]** — UI không gọi |
| `turn_layer_nodes` | `turnId` PK, `temporal_anchor` IX, `userMsg`, `aiReply` | L0 lịch sử lượt nói, **plaintext** | **Không có writer** | `get_memory_data` (`lib.rs:854`); `telegram.rs:145` | **[THIẾU]** |
| `events` | `eventId` PK, `phi_*`, `psi_*`, `rawUserMsg`, `rawAiReply`, `consolidation_status` | Log Φ/Ψ + hàng đợi consolidation, **plaintext** | **Không có writer** (2 index partial `pending` chờ pipeline chưa tồn tại) | `get_memory_data` (`lib.rs:894`) | **[THIẾU]** |
| `vectors_meta` | `id` PK/rowid, `vec_id` UQ, `type`, `content`, `decay_weight`, `source_event_ids` | Metadata RAG lai | `db::upsert_vector` (`db.rs:536`) qua `memory:upsert_vector` | 3 hàm search | **[MỘT PHẦN]** — UI 0 call |
| `vec_idx` | `rowid`, `embedding int8[384]` | Chỉ mục KNN `sqlite-vec` | `upsert_vector` (DELETE + INSERT `vec_quantize_int8`) | `search_similar_vectors` / hybrid | **[MỘT PHẦN]** |
| `vectors_fts` | `rowid`, `content` | FTS5 sparse, `remove_diacritics 0` giữ dấu tiếng Việt | `upsert_vector` (`INSERT OR REPLACE`, đồng bộ thủ công) | `search_fts_vectors` | **[MỘT PHẦN]** |
| `agent_checkpoints` | `thread_id` PK, `state_json` | Checkpoint `AgentState`; **plaintext dù chứa nguyên văn hội thoại** | `save_checkpoint` (`pipeline.rs:282`) | `load_checkpoint` (`pipeline.rs:251`) — **luôn `None`** | **[MỘT PHẦN]** hỏng ngữ nghĩa |
| `tasks` | `id` PK, `title`, `status`, `priority`, `result` | Quản lý công việc | INSERT `lib.rs:616`, UPDATE `:696`, DELETE `:638` | SELECT `:563/667/730` | **[OK]** — bảng duy nhất CRUD đầy đủ |
| `l3_nodes` / `l3_edges` | graph L3 | Knowledge graph | **Không ai** | **Không ai** | **[THIẾU]** |
| `personality_state` | `valence`, `arousal`, `friendliness`, `verbosity`, `assertiveness` | Trạng thái tính cách (mô hình PAD) | **Không ai** | **Không ai** | **[THIẾU]** |
| `daily_briefings` | `topics`, `content`, `expires_at` | Bản tin ngày (`source` mặc định `tavily`) | **Không ai** | **Không ai** | **[THIẾU]** |
| `consolidation_checkpoints` | `session_id`, `last_step`, `state_data` | Điểm dừng consolidation | **Không ai** | **Không ai** | **[THIẾU]** |
| `dlq_consolidation` | `failed_step`, `error_msg` | DLQ consolidation | **Không ai** | **Không ai** | **[THIẾU]** |
| `vector_dlq` | `delete_filter`, `status` | DLQ xoá vector | **Không ai** | **Không ai** | **[THIẾU]** |
| `data/liva-config.json` | `avatar`, `ai`, `ui`, `system`, `voice` | **SSOT cấu hình runtime** | `update_config` — `merge_json` + `fs::write` (`lib.rs:404-415`) | `read_config_file()` (`lib.rs:58-73`), `AvatarGallery.vue` | **[OK]** |
| `data/user_profile.json` | `name`, `birthYear`, `nationality`, `language`, `hobbies`… | Hồ sơ cá nhân hoá prompt | **Không có writer** (sửa tay) | `get_user_profile` (`lib.rs:534`) | **[MỘT PHẦN]** — PII plaintext |
| `data/liva_vault.json` | 9 khoá bí mật `iv:tag:ct` | Két bí mật cũ | **Không dòng Rust nào ghi** | **Không dòng Rust nào đọc** | **[THIẾU]** file chết |
| `data/credentials.json` | `installed.client_id`, `client_secret` | OAuth client Google Desktop | tải tay | **Không có reader** | **[THIẾU]** — ⚠️ secret plaintext |
| `data/token.json` | `access_token`, `refresh_token`, `scope` (Drive+Docs+Sheets) | Token OAuth | luồng OAuth Python đã xoá | **Không có reader** | **[THIẾU]** — ⚠️ refresh_token plaintext |
| `data/models.config.json` | `llm.model`, `stt`, `tts` | Config kiểu cũ | **Không ai** | **Không ai** | **[THIẾU]** |
| `data/skill_whitelist.json` | `<skill>.enabled` | Cổng kiểm soát kỹ năng | **Không ai** | **Không ai** | **[THIẾU]** — whitelist không được thực thi |

### 2.2 Kiểm đếm chính xác: bảng nào thực sự có writer

Đây là kết luận quan trọng nhất của cả chương, nên được kiểm chứng lại bằng grep trực tiếp trên `liva-native-core/src` (mọi lệnh ghi SQL trong toàn bộ `*.rs`):

| # | Câu lệnh ghi tìm thấy | Vị trí | Bảng đích |
|---|---|---|---|
| 1 | `INSERT OR REPLACE INTO agent_checkpoints (thread_id, state_json)` | `agent/memory.rs:24` | `agent_checkpoints` |
| 2 | `INSERT INTO facts (...) ON CONFLICT(key) DO UPDATE SET ...` | `db.rs:467-469` | `facts` |
| 3 | `INSERT OR IGNORE INTO vectors_meta (...)` | `db.rs:566` | `vectors_meta` |
| 4 | `UPDATE vectors_meta SET ...` | `db.rs:591` | `vectors_meta` |
| 5 | `DELETE FROM vec_idx WHERE rowid = ?` | `db.rs:607` | `vec_idx` |
| 6 | `INSERT INTO vec_idx (rowid, embedding) VALUES (?, vec_quantize_int8(?, 'unit'))` | `db.rs:613` | `vec_idx` |
| 7 | `INSERT OR REPLACE INTO vectors_fts (rowid, content)` | `db.rs:619` | `vectors_fts` |
| 8 | `INSERT INTO tasks (...)` | `lib.rs:616` | `tasks` |
| 9 | `DELETE FROM tasks WHERE id = ?1` | `lib.rs:638` | `tasks` |
| 10 | `UPDATE tasks SET ... WHERE id = ?7` | `lib.rs:696` | `tasks` |

⇒ Chỉ **6/15 bảng** có bất kỳ đường ghi nào: `facts`, `vectors_meta`, `vec_idx`, `vectors_fts`, `agent_checkpoints`, `tasks`.

⇒ **9/15 bảng hoàn toàn không có writer** trong toàn bộ mã nguồn Rust:
`events`, `turn_layer_nodes`, `l3_nodes`, `l3_edges`, `personality_state`, `daily_briefings`, `consolidation_checkpoints`, `dlq_consolidation`, `vector_dlq`.

Nếu tính chặt hơn theo tiêu chí **"được vòng hội thoại đang chạy thật sự ghi vào"** thì con số còn khắc nghiệt hơn: chỉ **2/15 bảng** (`agent_checkpoints` qua `webrtc/pipeline.rs:282`, và `tasks` qua IPC CRUD) thực sự nhận dữ liệu khi người dùng nói chuyện với LIVA. Bốn bảng còn lại (`facts`, `vectors_meta`, `vec_idx`, `vectors_fts`) chỉ có writer **nằm sau IPC mà UI không bao giờ gọi** ⇒ **13/15 bảng trống rỗng khi chạy thật**.

> **Cảnh báo diễn giải:** con số "12/15 bảng không có writer" từng lưu hành trong bản nháp là **không khớp code**. Số đúng là **9/15 không có writer ở bất kỳ đâu**, hoặc **13/15 không được ghi trong luồng hội thoại thật**. Tài liệu này dùng hai con số sau.

### 2.3 Hệ quả: bộ nhớ dài hạn chưa nối dây

Grep toàn bộ `liva-native-core/src`: **không có một câu `INSERT INTO events`, `INSERT INTO turn_layer_nodes`, `INSERT INTO l3_nodes/l3_edges` nào**. Các bảng này chỉ được **SELECT** trong `get_memory_data` (`lib.rs:844-978`) và `telegram.rs:145`.

Bốn IPC ghi/đọc memory tồn tại nhưng **không có caller nào trong `liva-ui/src`** (đã grep `set_fact|upsert_vector|search_hybrid|get_memory_data` — chỉ `get_memory_data` được UI gọi, tại `MemoryViewer.vue:32`, `useGateway.ts:178/287/322`):

- `"memory:set_fact"` (`lib.rs:980`) → `db::set_fact`
- `"memory:get_fact"` (`lib.rs:1000`)
- `"memory:search_hybrid"` (`lib.rs:1024`) — **nhận `query_vector` từ payload**, không tự embed
- `"memory:upsert_vector"` (`lib.rs:1084`)

⇒ **RAG / bộ nhớ dài hạn hiện là hạ tầng có sẵn nhưng không được vòng hội thoại gọi.** Cái duy nhất thực sự persist trong hội thoại là `agent_checkpoints` qua `SqliteCheckpointer` (`agent/memory.rs:5`) được `webrtc/pipeline.rs:247` khởi tạo mỗi lượt — lưu **toàn bộ `AgentState` dạng JSON plaintext**, key = `session_id`.

Đồng thời `load_checkpoint` (`pipeline.rs:251`) trên thực tế **luôn trả `None`** vì `session_id` sinh mới mỗi phiên ⇒ checkpoint ghi ra nhưng không bao giờ đọc lại được ⇒ trạng thái **[MỘT PHẦN] hỏng ngữ nghĩa**.

---

## 3. SQLite — pool, PRAGMA, WAL

**File:** `E:\Project\LIVA\liva-native-core\src\db.rs` (1185 dòng)

### 3.1 Engine & pool

SQLite qua `rusqlite` + `r2d2` + `r2d2_sqlite`, có wrapper riêng để chèn PRAGMA ngay lúc `connect()`:

```rust
// db.rs:15
pub struct CustomSqliteManager { inner: Arc<SqliteConnectionManager>, read_only: bool }
impl r2d2::ManageConnection for CustomSqliteManager { type Connection = Connection; type Error = rusqlite::Error; }

// db.rs:131
pub struct DatabasePool { pub writer: Pool<CustomSqliteManager>, pub readers: Pool<CustomSqliteManager> }
impl DatabasePool {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>>   // db.rs:137
    pub fn new_in_memory() -> Result<Self, Box<dyn std::error::Error>>                // db.rs:159
}
```

- **Tách reader/writer:** `writer` `max_size(1)` mở `READ_WRITE|CREATE` (`db.rs:143`); `readers` `max_size(4)` mở **`SQLITE_OPEN_READ_ONLY`** (`db.rs:141,148`) — mô hình single-writer/multi-reader chuẩn WAL.
- **PRAGMA chung mọi kết nối** (`db.rs:30-37`): `busy_timeout=5000`, `cache_size=-8192` (8 MiB), `page_size=32768`, `mmap_size=268435456` (256 MiB).
- **WAL** (`db.rs:42-48`): chỉ áp cho connection ghi — `journal_mode=WAL; synchronous=NORMAL; wal_autocheckpoint=500`. Reader chỉ set `synchronous=NORMAL` (`db.rs:40`).
- **In-memory:** URI `file:memdb_{rand}?mode=memory&cache=shared` (`db.rs:162`) để reader/writer dùng chung DB. Lưu ý reader pool ở chế độ in-memory lại mở `READ_WRITE` (`db.rs:169`) — khác hẳn chế độ file.
- Test khẳng định WAL bật thật: `test_database_pooling_and_wal` (`db.rs:904-932`); stress 100 reader / 10 writer đồng thời: `test_sqlite_wal_concurrency_stress` (`db.rs:1071`).

### 3.2 `sqlite-vec` là dependency CỨNG để boot

```rust
pub fn load_sqlite_vec(conn: &Connection) -> Result<(), rusqlite::Error>  // db.rs:63
```

Luồng nạp:

1. Thử `SELECT vec_version()` trước — nếu extension đã có sẵn thì thôi.
2. Chưa có → `load_extension_enable()` (`db.rs:73`) rồi dò **7 đường dẫn ứng viên** (`db.rs:91-98`): `node_modules/sqlite-vec-windows-x64/vec0.dll`, `../node_modules/…`, `../../node_modules/…`, `vec0.dll`, `vec0`.
3. Load fail → **chỉ `eprintln!("Warning: …")`** (`db.rs:27`) rồi đi tiếp.

Nhưng `init_schemas` ngay sau đó chạy `CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[384])` (`db.rs:348`) → lỗi → **panic cả process** qua `.expect("Failed to initialize DatabasePool")` (`main.rs:74`).

⇒ **DLL `vec0` là điều kiện cần để boot**, dù code trông như optional. `vec_idx` cũng là bảng ảo duy nhất phải tạo có điều kiện (kiểm tra `sqlite_master` trước) vì `vec0` không hỗ trợ `IF NOT EXISTS`.

### 3.3 Không có hệ thống migration

- Không `PRAGMA user_version`, không bảng `schema_migrations`, **không một câu `ALTER TABLE` nào trong toàn bộ `src/`** (đã grep).
- Chỉ có `CREATE TABLE IF NOT EXISTS` chạy lại mỗi lần `DatabasePool::new()`.
- ⇒ DB cũ trên máy beta tester (ví dụ `data/global/structured_memory.sqlite` 1,25 MB từ 11/06, hoặc `data/agents/liva_core/structured_memory.sqlite` + `-wal` 2 MB) **không bao giờ được nâng cấp cột**. Thêm cột mới trong tương lai = silent breakage.
- `PRAGMA foreign_keys` **không bao giờ được bật** ⇒ FK của `l3_edges` chỉ là trang trí.
- `PRAGMA page_size=32768` đặt **sau khi** DB đã tồn tại ⇒ vô hiệu với DB cũ (chỉ có tác dụng trước lần ghi đầu tiên hoặc sau `VACUUM`).

### 3.4 Chi tiết schema từng bảng

**`facts`** (`db.rs:190`) — bộ nhớ khoá–giá trị, **cột `value` được mã hoá**

| Cột | Kiểu | Ghi chú |
|---|---|---|
| `key` | TEXT | PRIMARY KEY |
| `value` | TEXT NOT NULL | ciphertext `iv:tag:ct` |
| `createdAt` / `updatedAt` | TEXT NOT NULL | chuỗi ISO (không phải epoch) |
| `ttlDays` | INTEGER | nullable — **không có code nào quét TTL** |
| `source` | TEXT NOT NULL | |
| `category` | TEXT | |
| `importance` | REAL DEFAULT 0.5 | |
| `confidenceScore` | REAL DEFAULT 1.0 | |
| `sourceTurnId` | TEXT | liên kết logic → `turn_layer_nodes.turnId` |
| `memory_strength` | REAL DEFAULT 1.0 | |
| `last_accessed_at` | INTEGER DEFAULT 0 | |
| `access_count` | INTEGER DEFAULT 0 | |

Index: **không có** ngoài PK.

**`agent_checkpoints`** (`db.rs:206`): `thread_id TEXT PK`, `state_json TEXT NOT NULL`.

**`events`** (`db.rs:211`) — log hội thoại Φ/Ψ: `eventId TEXT PK`, `timestamp INTEGER NOT NULL`, `phi_facts`, `phi_entities`, `psi_sentiment`, `psi_intent`, `psi_relational`, `rawUserMsg`, `rawAiReply`, `consolidated INTEGER DEFAULT 0`, `domain TEXT DEFAULT 'General'`, `category TEXT DEFAULT 'Uncategorized'`, `trace_keywords`, `last_accessed_at INTEGER DEFAULT 0`, `consolidation_status TEXT DEFAULT 'pending'`, `retry_count INTEGER DEFAULT 0`, `agentId TEXT DEFAULT 'liva_core'`.
Index partial: `idx_events_pending ON events(eventId) WHERE consolidation_status='pending'` (`db.rs:231`); `idx_events_consolidated_ts ON events(consolidated, timestamp) WHERE consolidation_status='pending'` (`db.rs:232`).
⚠️ `rawUserMsg` / `rawAiReply` lưu **plaintext**, không qua `EncryptionEngine`.

**`vector_dlq`** (`db.rs:234`): `id INTEGER PK AUTOINCREMENT`, `delete_filter TEXT NOT NULL`, `status TEXT DEFAULT 'pending'`, `retry_count INTEGER DEFAULT 0` — không có code đọc/ghi.

**`turn_layer_nodes`** (`db.rs:241`) — L0 lịch sử lượt nói: `turnId TEXT PK`, `temporal_anchor INTEGER NOT NULL`, `userMsg`, `aiReply`, `createdAt TEXT NOT NULL`, `agentId TEXT DEFAULT 'liva_core'`. Index `idx_turns_temporal(temporal_anchor)` (`db.rs:249`). Plaintext.

**`daily_briefings`** (`db.rs:251`): `id TEXT PK`, `created_at INTEGER NOT NULL`, `topics TEXT NOT NULL`, `content TEXT NOT NULL`, `is_read INTEGER DEFAULT 0`, `source TEXT DEFAULT 'tavily'`, `expires_at INTEGER NOT NULL` — không reader/writer.

**`tasks`** (`db.rs:261`): `id TEXT PK`, `title TEXT NOT NULL`, `description TEXT DEFAULT ''`, `status TEXT DEFAULT 'pending'`, `priority TEXT DEFAULT 'medium'`, `result TEXT DEFAULT ''`, `created_at INTEGER NOT NULL`, `updated_at INTEGER NOT NULL` — **bảng duy nhất có CRUD đầy đủ**.

**`consolidation_checkpoints`** (`db.rs:272`): `session_id TEXT PK`, `last_step INTEGER DEFAULT 0`, `state_data TEXT DEFAULT '{}'`, `created_at`, `updated_at` — không ai dùng.

**`dlq_consolidation`** (`db.rs:280`): `id INTEGER PK AUTOINCREMENT`, `session_id TEXT NOT NULL`, `failed_step TEXT NOT NULL`, `error_msg`, `retry_count INTEGER DEFAULT 0`, `status TEXT DEFAULT 'pending'`, `created_at` — không ai dùng.

**`personality_state`** (`db.rs:290`): `agentId TEXT PK`, `valence REAL DEFAULT 0.5`, `arousal REAL DEFAULT 0.5`, `friendliness REAL DEFAULT 0.8`, `verbosity REAL DEFAULT 0.6`, `assertiveness REAL DEFAULT 0.5`, `updatedAt INTEGER NOT NULL` — không ai dùng.

**`vectors_meta`** (`db.rs:300`): `id INTEGER PK AUTOINCREMENT`, `vec_id TEXT UNIQUE NOT NULL`, `type TEXT NOT NULL`, `content TEXT NOT NULL`, `domain TEXT DEFAULT 'General'`, `category TEXT DEFAULT 'Uncategorized'`, `trace_keywords TEXT DEFAULT '[]'` (JSON), `file_target TEXT`, `created_at INTEGER NOT NULL` (epoch ms), `last_accessed_at INTEGER DEFAULT 0`, `decay_weight REAL DEFAULT 1.0`, `access_count INTEGER DEFAULT 0`, `source_event_ids TEXT DEFAULT '[]'` (JSON, cap 50 phần tử — `db.rs:555`).
Index: `idx_vectors_meta_type_domain_category(type, domain, category)` (`db.rs:315`), `idx_vectors_meta_created_at(created_at)` (`db.rs:316`).

**`vectors_fts`** (`db.rs:318`) — `CREATE VIRTUAL TABLE … USING fts5(content, tokenize="unicode61 remove_diacritics 0")`. `remove_diacritics 0` ⇒ **giữ nguyên dấu tiếng Việt**. Là bảng FTS5 độc lập (không `content=`), đồng bộ thủ công bằng `INSERT OR REPLACE` trong `upsert_vector` (`db.rs:619`), `rowid` = `vectors_meta.id`.

**`vec_idx`** (`db.rs:346-351`) — `CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[384])`.

**`l3_nodes`** (`db.rs:323`): `id TEXT PK`, `label TEXT NOT NULL`, `properties TEXT DEFAULT '{}'`.
**`l3_edges`** (`db.rs:329`): `source`, `target`, `relation`, `weight REAL DEFAULT 1.0`, `obsolete INTEGER DEFAULT 0`, `PRIMARY KEY(source,target,relation)`, FK → `l3_nodes(id)` cả hai — knowledge graph L3 **hoàn toàn không có code đọc/ghi**.

### 3.5 Bộ nhớ vector: embedding, upsert, ba hàm truy vấn

```rust
// db.rs:536
pub fn upsert_vector(conn: &Connection, vec_id: &str, r#type: &str, content: &str,
    vector: &[f32], domain: Option<&str>, category: Option<&str>,
    trace_keywords: Option<&[String]>, file_target: Option<&str>,
    source_event_ids: Option<&[String]>) -> Result<(), rusqlite::Error>
```

Luồng ghi 5 bước (`db.rs:564-621`): `INSERT OR IGNORE` vào `vectors_meta` → lấy `id` → nếu đã tồn tại thì `UPDATE` + `DELETE FROM vec_idx WHERE rowid=?` → `INSERT INTO vec_idx (rowid, embedding) VALUES (?, vec_quantize_int8(?, 'unit'))` (f32 → bytes qua `bytemuck::cast_slice`) → `INSERT OR REPLACE INTO vectors_fts`.

Sinh embedding: `pub fn get_embedding(model: &LlamaModel, context: &mut LlamaContext, text: &str) -> Result<Vec<f32>, String>` (`llm/embed.rs:5`) — dùng chính llama.cpp engine, mean-pooling qua `embeddings_seq_ith(0)` với fallback `embeddings_ith(last)`, rồi **L2-normalize** (`embed.rs:39-46`). Phơi ra IPC `"llm:embed"` (`lib.rs:1282`).

Ba hàm truy vấn:

```rust
pub fn search_similar_vectors(conn, query_vector: &[f32], top_k: usize, filter: &MetadataFilter)
    -> Result<Vec<VectorSearchResult>, rusqlite::Error>          // db.rs:626
pub fn search_fts_vectors(conn, query_text: &str, top_k: usize, filter: &MetadataFilter)
    -> Result<Vec<FtsSearchResult>, rusqlite::Error>             // db.rs:720
pub fn search_hybrid_vectors(conn, query_text: &str, query_vector: &[f32], top_k: usize,
    filter: &MetadataFilter, dense_weight: f64, sparse_weight: f64)
    -> Result<Vec<VectorSearchResult>, rusqlite::Error>          // db.rs:839
```

- **Dense** (`db.rs:641-649`): KNN `WHERE v.embedding MATCH vec_quantize_int8(?, 'unit') AND v.k = ?` + subquery lọc metadata; có filter thì fetch `top_k*3`.
  Điểm số (`db.rs:678-680`): `dist_f32 = distance/120.0; similarity = max(0, 1 - dist_f32²/2); score = similarity * decay_weight`. Comment tại chỗ nói rõ đây là **port bit-for-bit từ bản JS cũ**.
- **Sparse** (`db.rs:711-718`): `prepare_fts_query` bọc mỗi token thành `"token"*` rồi nối `AND`; có nhánh fallback chạy lại với raw query nếu FTS5 parse lỗi (`db.rs:782-830`).
- **Hybrid** = **RRF (Reciprocal Rank Fusion)** với `K = 60.0` (`db.rs:854`): `score = weight * 1/(K + rank)`, cộng dồn khi trùng `vec_id`, sort giảm dần, truncate `top_k`. Bản ghi chỉ khớp FTS được gán `distance = 999.0` làm sentinel (`db.rs:878`).

`MetadataFilter { type, domain, category, created_after, created_before }` (`db.rs:377`) → `build_metadata_conditions` (`db.rs:415`) sinh WHERE **có tham số hoá đầy đủ** (không nối chuỗi giá trị) ⇒ **không có SQL injection ở đây**.

---

## 4. `crypto.rs` — AES-256-GCM với ba vấn đề

**File:** `E:\Project\LIVA\liva-native-core\src\crypto.rs` (133 dòng, chỉ 89 dòng code)

```rust
type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;   // crypto.rs:8 — nonce 16 byte (128-bit)
pub struct EncryptionEngine { key: [u8; 32] }            // crypto.rs:10
impl EncryptionEngine {
    pub fn new(key_str: &str) -> Self                            // :15
    pub fn encrypt(&self, text: &str) -> Result<String, String>  // :23
    pub fn decrypt(&self, text: &str) -> String                  // :50 — TRẢ String, KHÔNG Result
}
```

### 4.1 Thuật toán và định dạng lưu

- **Nonce/IV 16 byte (128-bit), không phải 96-bit tiêu chuẩn NIST.** Hợp lệ về mật mã (GCM với nonce ≠ 96-bit phải chạy GHASH để dẫn xuất J₀), nhưng chỉ tương thích với thư viện chấp nhận nonce tuỳ ý — chọn vậy để khớp bản Node cũ dùng `createCipheriv('aes-256-gcm', key, iv16)`.
- Auth tag 16 byte, được **tách khỏi đuôi ciphertext** (`crypto.rs:39-41`) rồi ghép lại khi giải mã (`crypto.rs:81-82`).
- **Không dùng AAD.**
- Định dạng lưu: `format!("{}:{}:{}", iv_hex, tag_hex, ciphertext_hex)` (`crypto.rs:47`) — 3 phần hex ngăn bởi `:`. Test khẳng định mỗi phần IV/tag dài 32 ký tự hex (`crypto.rs:106-107`). Đây **đúng định dạng thấy trong `data/liva_vault.json`**.
- Nonce sinh bằng `rand::rngs::OsRng.fill_bytes` (`crypto.rs:24-25`) — CSPRNG của OS, 16 byte ngẫu nhiên mỗi lần `encrypt()`. **Không dùng `Mulberry32`** — điểm này đúng.

### 4.2 Vấn đề 1 — không có KDF

```rust
// crypto.rs:15-21
let mut key = [0u8; 32];
let bytes = key_str.as_bytes();
let len = bytes.len().min(32);
key[..len].copy_from_slice(&bytes[..len]);   // raw ASCII, zero-pad
```

Lấy **thẳng byte ASCII** của `LIVA_ENCRYPTION_KEY`, cắt ở 32, **zero-pad phần thiếu**. Không PBKDF2/Argon2/HKDF, không salt.

Passphrase `"liva"` → key = `6c 69 76 61` + **28 byte `0x00`**, entropy thực tế ~32 bit. Khoá kiểu passphrase ngắn ⇒ entropy rất thấp *và* phần đuôi khoá là hằng số đã biết công khai.

### 4.3 Vấn đề 2 — khoá mặc định yếu, công khai

`LIVA_ENCRYPTION_KEY` được đọc ở **cả hai điểm vào** với cùng fallback `"00000000000000000000000000000000"` — `main.rs:62-63` và `liva-desktop/src-tauri/src/lib.rs:270-271` — rồi nhét vào `AppState` qua `EncryptionEngine::new(&encryption_key)` (`main.rs:242`, tauri `lib.rs:357`). Các bin/test (`verify_integrations.rs:14/33`, `verify_duplex.rs:73`, `tests/verify_commands.rs:11`, `db.rs:937/1081`) dùng thẳng khoá zero cố định.

> 📌 Nguồn đầy đủ (bảng biến môi trường, mọi giá trị mặc định, điểm đọc): [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md)

```rust
// main.rs:62-63
let encryption_key = std::env::var("LIVA_ENCRYPTION_KEY")
    .unwrap_or_else(|_| "00000000000000000000000000000000".to_string());
```

Chuỗi `"0"` × 32 là **ký tự ASCII `'0'` = `0x30`**, nên khoá thực tế = byte `0x30` lặp 32 lần — một khoá hằng số nằm công khai trong mã nguồn. **Không panic, không log cảnh báo, không từ chối boot** ⇒ rất dễ chạy production với khoá này mà không ai biết. Khi đó toàn bộ `facts.value` coi như plaintext.

### 4.4 Vấn đề 3 — `decrypt()` fail-open

`decrypt` trả `String` chứ không `Result`. **Mọi** lỗi — không đủ 3 phần, hex sai, độ dài IV/tag sai, **xác thực GCM thất bại**, UTF-8 hỏng — đều `return text.to_string()`, tức trả nguyên chuỗi đầu vào (`crypto.rs:53,58,62,66,70,75,86`).

Hai test **cố ý khẳng định hành vi này**: `test_decrypt_plain_fallback` (`crypto.rs:114`) và `test_decrypt_corrupted_fallback` (`crypto.rs:124`).

Thiết kế nhằm đọc được dữ liệu legacy chưa mã hoá, nhưng hệ quả là **mất hoàn toàn tính chất *authenticated* của AES-GCM ở tầng ứng dụng**: không phân biệt được "chưa mã hoá", "khoá sai" và "bị giả mạo". UI sẽ hiển thị chuỗi ciphertext hex như thể đó là giá trị fact, và chuỗi đó có thể đi thẳng vào prompt LLM.

### 4.5 Phạm vi mã hoá — chỉ 3 chỗ

1. `db::set_fact` — mã hoá `fact.value` (`db.rs:454`)
2. `db::get_fact` — giải mã (`db.rs:514`)
3. `get_memory_data` — giải mã `facts.value` để trả UI (`lib.rs:876`)

Nghĩa là **plaintext trong SQLite**: `events.rawUserMsg` / `rawAiReply`, `turn_layer_nodes.userMsg` / `aiReply`, `vectors_meta.content` (còn được **nhân bản thêm một lần** vào `vectors_fts`), `agent_checkpoints.state_json` (chứa cả system prompt lẫn toàn bộ lịch sử tin nhắn), `tasks.*`. File `-wal` 2 MB cũng plaintext.

### 4.6 `LIVA_VAULT_PATH` **không phải** két bí mật

```rust
// main.rs:166-168  (giống hệt tauri lib.rs:345-347)
let vault_path = std::env::var("LIVA_VAULT_PATH")
    .unwrap_or_else(|_| "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\vault".to_string());
let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(&vault_path));
```

Đây là **thư mục Obsidian markdown** làm knowledge base cho MCP server (`read_markdown`, `search_vault`), xác nhận bởi `.env.example:140-142` ("─── 6. Obsidian Vault (MCP knowledge base) ───") và `docs/KNOWLEDGE_BASE.md:9`. **Không liên quan gì tới `EncryptionEngine`** — đây là điểm dễ hiểu nhầm nhất khi đọc tên biến.

---

## 5. Ba két bí mật, cả ba đều không sống

### 5.1 `data/liva_vault.json` — **[THIẾU]** file chết

Chứa 9 secret ở đúng định dạng `iv:tag:ct` của `EncryptionEngine`:

```
{ "<ENV_NAME>": "<iv_hex32>:<tag_hex32>:<ciphertext_hex>" }
ENV_NAME ∈ { EMAIL_HOST, EMAIL_USER, EMAIL_PASS, TAVILY_API_KEY,
             ZALO_OA_ACCESS_TOKEN, TELEGRAM_BOT_TOKEN, ZALO_APP_ID,
             ZALO_APP_SECRET, GOOGLE_CLIENT_SECRET }
```

Grep toàn repo: **không một dòng Rust/TS nào đọc file này**. Chỉ `scripts/legacy/migration_stronghold.cjs` (đã đánh dấu legacy) và tài liệu mô tả cơ chế Node cũ (`docs/architecture/05_Security_Guardrails.md:10`).

Đây là bằng chứng cơ chế vault cũ (Node) đã chạy thật; bản Rust **không port lại** phần `loadVaultIntoEnv()`.

### 5.2 Tauri Stronghold + Argon2id — **[MỘT PHẦN]** có nhưng không nối dây

Toàn bộ nằm trong `liva-desktop/src-tauri/src/lib.rs`:

```rust
// lib.rs:123-129
fn get_stronghold_credentials() -> (String, Vec<u8>) {
    let password = std::env::var("LIVA_STRONGHOLD_PASSWORD")
        .unwrap_or_else(|_| "LIVA_DEFAULT_SECURE_PASSWORD".to_string());
    let salt_str = std::env::var("LIVA_STRONGHOLD_SALT")
        .unwrap_or_else(|_| "LIVA_STRONGHOLD_PERSISTENT_SALT_KEY".to_string());
    (password, salt_str.into_bytes())
}
```

| Hàm | Vị trí | Vai trò |
|---|---|---|
| `get_stronghold_credentials()` | `lib.rs:123-129` | Đọc `LIVA_STRONGHOLD_PASSWORD` / `LIVA_STRONGHOLD_SALT`, **fallback hardcode** (lặp lại ở `lib.rs:384`) — chi tiết hai biến này ở [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) |
| `get_vault_key(app)` | `lib.rs:131` | **Argon2id**, `hash_length = 32`, cache trong `StrongholdKey` mutex |
| `read_vault_key(app, key)` | `lib.rs:152` | Đọc snapshot `{app_local_data_dir}/liva_vault.app`, client `"liva_client"` |
| `write_vault_key(app, key, value)` | `lib.rs:189` | Ghi vào cùng snapshot qua `client.store()` |

Đã đăng ký trong `invoke_handler` (`lib.rs:570-571`) và có wrapper `TauriAdapter.readVaultKey` / `writeVaultKey` (`liva-ui/src/platform/TauriAdapter.ts:40,50`). **Nhưng grep `VaultKey` bên ngoài thư mục `platform/` cho 0 kết quả** ⇒ không component/composable nào gọi. Chỉ test `PlatformAdapter.test.ts:141/155` dùng tới.

`MockWebAdapter.ts:31,35` lưu thẳng vào `localStorage` key `liva_vault_${key}` (chỉ dùng khi chạy web mock).

⚠️ Điểm yếu cốt lõi: Argon2id là KDF tốt, nhưng **áp lên một password đã biết công khai với salt cố định trên mọi máy** thì không bảo vệ được gì. Argon2id chỉ có giá trị khi `LIVA_STRONGHOLD_PASSWORD` được đặt bằng bí mật thật.

### 5.3 `ApiManagementView.vue` — **[THIẾU]** màn hình hỏng hoàn toàn

`ApiManagementView.vue` đọc `payload?.vault` từ IPC `get_env_config` (dòng 89, 136) và ghi lại `.env` **plaintext** qua `setEnvField` (dòng 149-168, gồm `AI_API_KEY`, `TAVILY_API_KEY`, `TELEGRAM_BOT_TOKEN`, `ZALO_APP_SECRET`…).

Nhưng **`get_env_config` / `save_env_config` không tồn tại trong bất kỳ file `.rs` nào** (grep toàn repo, 0 kết quả) ⇒ màn hình quản lý API key này **hỏng/chết** với core hiện tại.

---

## 6. Cấu trúc các file trong `data/`

Chỉ liệt kê tên khoá và kiểu; giá trị nhạy cảm đã che.

### 6.1 `data/liva-config.json` — **[OK]** SSOT cấu hình runtime

Đọc bởi `config_file_path()` / `read_config_file()` (`lib.rs:58,66,76`); UI ghi qua deep-merge `merge_json` (`lib.rs:102`, `update_config` tại `lib.rs:404-415`). **Được track trong git.**

Năm nhóm khoá cấp cao: `avatar` (engine/model 3D), `ai` (provider, `localModelsDir`, `routerModel`, `mmprojModel`, `expertModel`, tham số sampling), `ui` (theme, vị trí widget, model đang chọn), `system` (proactive + digest), `voice` (provider, profile, ngôn ngữ, sample rate).

> 📌 Nguồn đầy đủ (bảng từng khoá kèm cột "có reader hay không"): [Cấu hình và biến môi trường §4](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md)

⚠️ Quan sát bảo mật riêng của tầng dữ liệu: `ai.cloudApiKey` là **chỗ chứa API key nằm trong file git-tracked** (hiện đang rỗng). Giá trị thực tế `ai.routerModel` = `Qwen3-VL-2B-Instruct-Q4_K_M.gguf`.

### 6.2 `data/models.config.json` — **[THIẾU]** không reader

Track trong git.

```
llm: { provider:str, model:str(gguf) }
stt: { provider:str, language:str }
tts: { provider:str, voice:str }
```

Nội dung lệch hẳn với `liva-config.json` (`gemma-4-26B` vs `Qwen3-VL-2B`) ⇒ file legacy bị bỏ lại.

### 6.3 `data/user_profile.json` — **[MỘT PHẦN]** PII plaintext

Đọc tại `lib.rs:534` (`"get_user_profile"`). **Gitignored** (`.gitignore:26`). Không có writer — sửa tay.

```
name:str, birthYear:int, nationality:str, language:str,
hobbies:str, preferences:str, age:int, profession:str, location:str
```

⚠️ Nếu file thiếu, `lib.rs:542-552` **hardcode nguyên PII của người dùng vào binary** làm giá trị mặc định. `birthYear` và `age` mâu thuẫn nhau ở cả file lẫn phần hardcode.

### 6.4 `data/skill_whitelist.json` — **[THIẾU]** whitelist không được thực thi

Track trong git, không reader trong Rust.

```
{ "<skill_name>": { enabled:bool, lastToggled:int(epoch_ms) } }
skill_name ∈ { privacy_dashboard, system_audit, send_zalo_rpa, read_emails }
```

⚠️ Bảng cho phép/cấm kỹ năng — **kể cả `send_zalo_rpa` và `read_emails`** — không được bất kỳ đoạn code nào kiểm tra ⇒ whitelist không có hiệu lực thực thi ở runtime.

### 6.5 `data/credentials.json` — **[THIẾU]** ⚠️ secret plaintext

Không reader. **Gitignored** (`.gitignore:21`), `git log --all` xác nhận **chưa từng commit**.

```
installed: {
  client_id:str,                      // Google OAuth desktop client — PLAINTEXT
  project_id:str,
  auth_uri:str(url), token_uri:str(url), auth_provider_x509_cert_url:str(url),
  client_secret:str,                  // ⛔ SECRET, PLAINTEXT
  redirect_uris:[str]
}
```

### 6.6 `data/token.json` — **[THIẾU]** ⚠️ refresh_token plaintext

Không reader. Gitignored (`.gitignore:22`), chưa từng commit.

```
access_token:str,               // ⛔ SECRET, PLAINTEXT
refresh_token:str,              // ⛔ SECRET, PLAINTEXT (long-lived)
scope:str,                      // Google Drive + Docs + Sheets — quyền ghi toàn bộ
token_type:str("Bearer"),
refresh_token_expires_in:int(s),
expiry_date:int(epoch_ms)
```

### 6.7 Thư mục con trong `data/`

| Đường dẫn | Vai trò |
|---|---|
| `data/agents/liva_core/structured_memory.sqlite` (+`-wal` 2 MB, `-shm`) | **DB mặc định đang dùng** (`main.rs:62`, tauri `lib.rs:269`). Cùng thư mục có `rpa_audit_log.jsonl` |
| `data/agents/__diag_structured_memory__/`, `data/agents/stress_benchmark_agent/`, `test-ws/`, `benchmark_agent_liva_brutal/` | DB rác từ test/benchmark còn sót |
| `data/global/structured_memory.sqlite` (1,25 MB, 11/06) | DB legacy, không path nào trỏ tới |

---

## 7. `prng.rs` — Mulberry32, vì sao tự viết — **[THIẾU]** code chết

**File:** `E:\Project\LIVA\liva-native-core\src\prng.rs` (70 dòng, 31 dòng code, mở đầu bằng `#![allow(dead_code)]`)

```rust
pub struct Mulberry32 { seed: u32 }
impl Mulberry32 {
    pub fn new(seed_str: &str) -> Self        // prng.rs:8   — hash chuỗi qua encode_utf16()
    pub fn from_seed_u32(seed: u32) -> Self   // prng.rs:18
    pub fn next_f64(&mut self) -> f64         // prng.rs:22
}
```

**Thuật toán:** Mulberry32 — PRNG 32-bit state, chu kỳ 2³², **không phải mật mã**.

- Hash seed từ chuỗi (`prng.rs:9-14`): khởi `h = 0xdead_beef`, duyệt **`encode_utf16()`** (chứ không phải UTF-8 bytes), `h = (h ^ val).wrapping_mul(2654435761)` (hằng Knuth), kết `h ^ (h >> 16)`.
- Bước sinh (`prng.rs:23-29`): `seed += 0x6D2B79F5`; `t = (t ^ t>>15) * (t|1)`; `t ^= t + (t ^ t>>7)*(t|61)`; trả `(t ^ t>>14) as f64 / 4294967296.0`.

**Vì sao tự viết** (đọc từ chính test, không suy đoán): `test_mulberry32_matches_js_bit_for_bit` (`prng.rs:38`) và `test_mulberry32_matches_js_with_emoji` (`prng.rs:55`) so **bit-for-bit với output tham chiếu của Node.js** (`0.3707022285088897`, `0.7425355203449726`, sai số `< 1e-15`). Việc dùng `encode_utf16` cũng chính vì phải tái tạo `String.charCodeAt()` của JS.

⇒ Mục đích: **port xác định-tính (deterministic) của bản gateway JS cũ**, để cùng seed cho cùng kết quả giữa hai runtime — cùng động cơ với comment "matching JS" ở `db.rs:676-678`.

**Trạng thái: code chết.** Grep `Mulberry32|prng::` toàn bộ `*.rs`: chỉ có khai báo `pub mod prng;` (`lib.rs:5`) và test nội bộ. Không nơi nào dùng để sinh ID, jitter, sampling hay nonce (nonce dùng `OsRng` — đúng).

> 📌 Nguồn đầy đủ (danh mục toàn bộ code mồ côi của repo, `prng.rs` là một mục trong đó): [Nợ kỹ thuật và rủi ro §5](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md)

---

## 8. `.gitignore` / `.aiexclude`

Phần liên quan trực tiếp tới tầng dữ liệu: nhóm **Secrets** của `.gitignore` (`:16-18` cho `**/.env`, `:135-137` cho `data/liva_vault.json`, `data/user_profile.json`, `credentials.json`, `token.json`, `*.pem`, `*.key`) phủ đúng 4 file bí mật/PII mô tả ở §6. `.aiexclude` **không** đồng bộ với danh sách này.

> 📌 Nguồn đầy đủ (bảng 10 nhóm `.gitignore`, bẫy pattern không neo thư mục, tình trạng `.aiexclude`): [Cấu hình và biến môi trường §8](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md)

**Điểm tốt kiểm chứng được (kiểm chứng riêng của tài liệu này):** `git log --all -- data/{credentials,token,liva_vault,user_profile}.json` trả về **rỗng** — 4 file chứa secret/PII **chưa từng vào lịch sử git**.

---

## 9. Rủi ro bảo mật quan sát được

Chỉ nêu quan sát, không kèm khuyến nghị hành động cụ thể — quyết định thuộc về chủ dự án.

### 9.0 Những rủi ro đã được xếp hạng ở nơi khác

Bốn quan sát của chương này đã được đưa vào bảng rủi ro xếp hạng toàn dự án, nên ở đây **không nhắc lại**, chỉ nêu tên để đọc mạch lạc: khoá mã hoá mặc định công khai + không KDF + `decrypt()` fail-open (gộp thành một mục CRITICAL); Stronghold hardcode password/salt; WebSocket `/ws` không xác thực nên mọi lệnh đọc bộ nhớ đều mở; và việc không có hệ thống migration DB kèm `PRAGMA foreign_keys` không bao giờ bật. Phân tích kỹ thuật của từng thứ nằm ở §3–§5 phía trên; phần **xếp hạng, mức độ và đề xuất xử lý** nằm ở tài liệu rủi ro.

> 📌 Nguồn đầy đủ: [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · lộ trình sửa: [Lộ trình sửa lỗi và nâng cấp](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md)

### 9.1 Nghiêm trọng — quan sát riêng của tầng dữ liệu

| # | Rủi ro | Vị trí |
|---|---|---|
| 1 | **Google OAuth refresh token + client_secret plaintext trên đĩa**, scope Drive + Docs + Sheets đầy đủ. Tin tốt: đã gitignore và `git log --all` xác nhận chưa từng vào lịch sử git. Tin xấu: bất kỳ process nào chạy dưới user Windows đều đọc được, không có DPAPI/Stronghold bảo vệ, refresh token không có TTL ngắn | `data/token.json`, `data/credentials.json` |
| 5 | **Phạm vi mã hoá quá hẹp**: chỉ `facts.value`. Nội dung hội thoại thật, nội dung vector (nhân bản thêm vào `vectors_fts`), và `agent_checkpoints.state_json` (chứa system prompt + toàn bộ lịch sử tin nhắn) đều plaintext. File `-wal` 2 MB cũng plaintext | `db.rs:454/514`, `lib.rs:876` |

### 9.2 Trung bình

| # | Rủi ro | Vị trí |
|---|---|---|
| 7 | **`skill_whitelist.json` không được thực thi**: không code nào đọc. Kỹ năng nhạy cảm (`read_emails`, `send_zalo_rpa`) không có cổng kiểm soát runtime | `data/skill_whitelist.json` |
| 9 | **DLL hijacking tiềm năng**: `load_extension_enable()` rồi dò `vec0.dll` theo **đường dẫn tương đối với CWD** (`node_modules/…`, `../node_modules/…`, và cuối cùng `"vec0"` để SQLite tự resolve). Nếu process chạy từ thư mục kẻ tấn công ghi được ⇒ nạp code tuỳ ý vào process | `db.rs:73`, `db.rs:91-98` |

### 9.3 Thấp / vệ sinh dữ liệu

| # | Rủi ro | Vị trí |
|---|---|---|
| 11 | **Không có đường xoá bộ nhớ**: không hàm nào xoá `facts`/`vectors_meta`. `ttlDays` (facts) và `expires_at` (daily_briefings) khai báo nhưng **không job nào quét** ⇒ dữ liệu cá nhân giữ vĩnh viễn, trái hàm ý của cột TTL. `vector_dlq.delete_filter` cũng chưa có consumer | `db.rs:190/251/234` |
| 12 | **Rò rỉ qua bản sao**: `upsert_vector` ghi `content` vào cả `vectors_meta` và `vectors_fts`; nếu sau này xoá `vectors_meta` thủ công, bản `vectors_fts`/`vec_idx` sẽ mồ côi (không trigger/cascade) | `db.rs:566-619` |
| 13 | `vec_idx` cố định **384 chiều** trong khi `llm::get_embedding` trả `n_embd` của model đang nạp. Điều chắc chắn đọc được: **không có kiểm tra chiều nào** trong `upsert_vector` ⇒ ghi vector sai chiều fail ở tầng SQLite, không có thông báo rõ ràng | `db.rs:348`, `db.rs:536` |
| 14 | `data/global/`, `data/agents/__diag_structured_memory__/`, `stress_benchmark_agent/`, `test-ws/` chứa DB cũ/benchmark có thể còn dữ liệu hội thoại thật, không được dọn | `data/` |

---

## 10. Bản đồ nhanh trạng thái

| Thành phần | Trạng thái |
|---|---|
| `db.rs` — SQLite pool + schema + hybrid search | **[OK]** chạy thật, nối dây (`main.rs:74`, tauri `lib.rs:268`) |
| `crypto.rs` — `EncryptionEngine` AES-256-GCM | **[MỘT PHẦN]** chạy thật, nhưng chỉ dùng cho **duy nhất cột `facts.value`** |
| `prng.rs` — `Mulberry32` | **[THIẾU]** code chết — không nơi nào gọi ngoài test của chính nó |
| Vault Stronghold (Tauri) + Argon2id | **[MỘT PHẦN]** command đăng ký nhưng UI không invoke |
| `data/liva_vault.json` | **[THIẾU]** chết — không một dòng Rust nào đọc |
| `data/credentials.json`, `data/token.json`, `data/models.config.json`, `data/skill_whitelist.json` | **[THIẾU]** không có reader trong code hiện hành |
| Ghi bộ nhớ dài hạn (`events`, `turn_layer_nodes`, `vectors_meta`) | **[THIẾU]/[MỘT PHẦN]** chỉ đọc, không có đường ghi từ vòng hội thoại |
| `tasks` CRUD | **[OK]** bảng duy nhất có đủ INSERT/SELECT/UPDATE/DELETE |
| `data/liva-config.json` | **[OK]** SSOT cấu hình, đọc-ghi thật |

---

## Liên quan

**Đọc tiếp theo mạch:** [◀ Thị giác, passive và governor](06-thi-giac-passive-va-governor.md) · [Frontend và vỏ Tauri ▶](08-frontend-va-vo-tauri.md) · [⬆ Mục lục](../README.md)

**Tài liệu này dựa vào (nguồn sự thật ở nơi khác):**
- [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) — bảng biến môi trường (`LIVA_ENCRYPTION_KEY`, `LIVA_DB_PATH`, `LIVA_STRONGHOLD_*`, `LIVA_VAULT_PATH`), bảng khoá `liva-config.json` kèm cột có/không reader, và bảng `.gitignore`/`.aiexclude`.
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — bảng rủi ro xếp hạng (C/H/M/L) và danh mục code mồ côi mà `prng.rs`, `l3_*`, `personality_state` nằm trong đó.
- [Lộ trình sửa lỗi và nâng cấp](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) — thứ tự xử lý các lỗ hổng mã hoá/bộ nhớ nêu ở §9.
- [Giao thức IPC và WebSocket](02-giao-thuc-ipc-va-websocket.md) — định nghĩa đầy đủ các lệnh `memory:*`, `get_memory_data`, `get_user_profile` mà chương này chỉ nói tới ở mặt lưu trữ.
- [Báo cáo khảo sát gốc 2026-07](../03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) — dữ liệu khảo sát gốc mà sơ đồ ERD và phần kiểm đếm writer được dựng lên từ đó.

**Tài liệu khác dựa vào tài liệu này:**
- [Hệ agent, bộ nhớ và tiến hoá](05-agent-bo-nho-va-tien-hoa.md) — lấy schema `agent_checkpoints`, sự thật "checkpoint ghi nhưng không bao giờ đọc lại", và tình trạng bộ nhớ dài hạn chưa nối dây.
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — lấy con số 9/15 bảng không có writer và phân tích `EncryptionEngine` làm bằng chứng cho các mục CRITICAL/HIGH.
- [Đối chiếu tuyên bố vs thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — lấy bằng chứng "dữ liệu nằm cục bộ, mã hoá tới đâu" để chấm các tuyên bố offline/riêng tư.
- [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) — trỏ ngược về đây cho ERD và ý nghĩa từng bảng SQLite.

**Khi sửa code sau đây thì phải cập nhật tài liệu này:**
- `liva-native-core/src/db.rs` — mọi thay đổi schema, PRAGMA, pool, hàm search sẽ làm lệch §1 (ERD), §2 (bảng dữ liệu) và §3.
- `liva-native-core/src/crypto.rs` — đổi thuật toán/định dạng `iv:tag:ct`, KDF hay hành vi fail-open sẽ làm lệch toàn bộ §4 và §9.
- `liva-desktop/src-tauri/src/lib.rs` — vault Stronghold + Argon2id, khoá mã hoá, đường dẫn DB: §4.3, §5.2.
- `liva-native-core/src/agent/memory.rs` — `SqliteCheckpointer` ghi `agent_checkpoints`: §2.3, §4.5.
- `liva-native-core/src/webrtc/pipeline.rs` — `save_checkpoint`/`load_checkpoint`: kết luận "luôn trả `None`" ở §2.3.
- `liva-native-core/src/llm/embed.rs` — chiều embedding và L2-normalize: §3.5 và rủi ro #13 (`vec_idx` cố định 384 chiều).
- `data/*` (`liva-config.json`, `user_profile.json`, `skill_whitelist.json`, `credentials.json`, `token.json`) — cấu trúc khoá và trạng thái reader/writer ở §6.
- `liva-ui/src/platform/TauriAdapter.ts`, `liva-ui/src/platform/MockWebAdapter.ts` — wrapper `readVaultKey`/`writeVaultKey`: kết luận "UI không invoke" ở §5.2.
