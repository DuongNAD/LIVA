---
title: "Kiến trúc Bộ nhớ Đa tầng và Truy xuất Hỗn hợp (Hybrid RAG)"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-storage/src/lib.rs
  - liva-native-core/src/db/csr_graph.rs
  - liva-native-core/src/crypto.rs
  - liva-native-core/src/db.rs
---

# Kiến trúc Bộ nhớ Đa tầng và Truy xuất Hỗn hợp (Hybrid RAG)

## 1. Phân tầng Bộ nhớ 4 Cấp (L0 - L3)

Hệ thống bộ nhớ của LIVA được thiết kế theo 4 tầng phân cấp tối ưu hóa tài nguyên phần cứng, cân bằng giữa tốc độ phản hồi tức thì và chiều sâu ngữ cảnh:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        HỆ THỐNG BỘ NHỚ ĐA TẦNG                         │
├────────────────────────────────────────────────────────────────────────┤
│ TẦNG L0: ACTIVE RECALL RADIX TRIE (RAM - < 0.1 ms)                     │
│   - Cây tiền tố FactTrie lưu trữ khóa thông tin và thách thức ôn tập   │
│   - Đối khớp cửa sổ trượt trên chuỗi người dùng, chi phí 0 token LLM   │
├────────────────────────────────────────────────────────────────────────┤
│ TẦNG L1: PERSISTENT SQLITE WAL POOL (NVMe / RAM - 0.5 - 2 ms)          │
│   - Kết nối tách biệt: 1 writer duy nhất qua DbActor + 4 readers       │
│   - Calibrated PRAGMAs: cache_size -2000 (2MB), mmap_size 256MB        │
│   - Quản lý sự kiện, hội thoại, trạng thái và sổ cái kiểm toán         │
├────────────────────────────────────────────────────────────────────────┤
│ TẦNG L2: SCOPED VECTOR ANN & FTS5 (Tĩnh C FFI - 3 - 8 ms)              │
│   - Bảng ảo vector biên dịch tĩnh sqlite-vec (int8/float 384 chiều)    │
│   - Động cơ EmbeddingEngine phi khóa dùng ONNX Runtime CPU (&self)     │
│   - FTS5 Full-Text Search kết hợp xếp hạng RRF (Reciprocal Rank Fusion)│
├────────────────────────────────────────────────────────────────────────┤
│ TẦNG L3: HIPPORAG KNOWLEDGE GRAPH (RAM CSR - ~1.12 ms)                 │
│   - Đồ thị tri thức nén Compressed Sparse Row (CsrGraph)               │
│   - Cán đọc phi khóa ArcSwap<CsrGraph> cho thuật toán PPR 3 vòng SpMV  │
│   - Cập nhật đồ thị đồng bộ in-place thông qua DbActor                 │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Chi tiết từng Tầng Bộ nhớ

### 2.1. Tầng L0 — In-Memory Radix Trie (`FactTrie` & `ActiveRecallManager`)
- Được cài đặt trực tiếp trong `crates/liva-storage/src/lib.rs`.
- Cấu trúc dữ liệu `FactTrie`: Mỗi nút biểu diễn một ký tự chuẩn hóa chữ thường. Lưu trữ danh sách `fact_keys` tại các điểm kết thúc từ.
- Thuật toán `search(text)`: Quét trượt trên chuỗi hội thoại của người dùng mà không cần gọi mô hình ngôn ngữ lớn. Khi phát hiện từ khóa khớp với tri thức đã lưu (ví dụ: ngày sinh, sở thích, thông số kỹ thuật đã ghi nhớ), hệ thống có thể lập tức trả lời hoặc đưa ra câu hỏi củng cố (Spaced Repetition Challenge).

### 2.2. Tầng L1 — SQLite WAL và Bộ điều phối `DbActor`
- **Bộ thông số PRAGMA hiệu chỉnh**:
  - `page_size = 4096`: Khớp kích thước khối phân trang của hệ điều hành Windows x64.
  - `cache_size = -2000`: Giới hạn bộ nhớ đệm RAM ở mức ~2 MB (thay thế mức mặc định cũ gây phình bộ nhớ tới 312 MB).
  - `mmap_size = 268435456`: Ánh xạ trực tiếp 256 MB tệp cơ sở dữ liệu vào không gian địa chỉ ảo để tăng tốc độ đọc.
  - `journal_mode = WAL` & `synchronous = NORMAL`: Đảm bảo an toàn giao dịch mà không làm chậm ổ cứng SSD.
  - `journal_size_limit = 67108864`: Giới hạn tệp WAL tối đa 64 MB, tự động checkpoint khi vượt 1000 trang.
- **Tiến trình ghi vi lô `DbActor`**:
  - Mọi thao tác chèn/cập nhật dữ liệu từ các luồng async Tokio được đẩy vào kênh MPSC bounded (dung lượng 1024).
  - Luồng OS chuyên biệt gộp tối đa 50 câu lệnh hoặc đợi tối đa 5ms, sau đó mở giao dịch `BEGIN IMMEDIATE;` để ghi một lần. Nhờ vậy, năng lực ghi đạt từ 1,500 đến 3,000 thao tác/giây mà không hề xuất hiện lỗi `SQLITE_BUSY`.

### 2.3. Tầng L2 — Bảng ảo Vector `sqlite-vec` Biên dịch Tĩnh
- Không sử dụng tệp nhị phân nạp động (`vec0.dll`) vốn gây rủi ro tương thích và phụ thuộc môi trường cài đặt bên ngoài.
- `crates/liva-storage/build.rs` biên dịch trực tiếp mã nguồn C `sqlite-vec.c` bằng crate `cc` với cờ phần cứng `/O2`, `/arch:AVX2`, `/fp:fast` trên MSVC x64.
- Đăng ký hàm FFI `sqlite3_vec_init` trực tiếp vào kết nối SQLite thông qua `register_sqlite_vec()` được bảo vệ bởi `std::sync::Once`.
- Bảng ảo `vec_idx USING vec0(embedding int8[384])` hỗ trợ tìm kiếm khoảng cách Cosine hoặc L2 trên vector 384 chiều sinh bởi mô hình `multilingual-e5-small`.
- Kết hợp với `vectors_fts` (FTS5) thông qua công thức Reciprocal Rank Fusion (RRF) để đạt độ chính xác cao nhất:
  $$\text{Score}(d) = \frac{1}{60 + \text{Rank}_{\text{vec}}(d)} + \frac{1}{60 + \text{Rank}_{\text{fts}}(d)}$$

### 2.4. Tầng L3 — Đồ thị Tri thức HippoRAG (`CsrGraph`)
- Nằm trong `liva-native-core/src/db/csr_graph.rs`.
- Biểu diễn đồ thị quan hệ giữa các thực thể dưới định dạng ma trận nén CSR (Compressed Sparse Row: mảng `row_ptr`, `col_indices`, `weights`).
- Cơ chế đọc phi khóa: Biến đồ thị được bọc trong `Arc<ArcSwap<CsrGraph>>`. Luồng truy vấn RAG chỉ cần gọi `.load()` để lấy một con trỏ bất biến đọc đồ thị mà không hề tranh chấp khóa với bất kỳ luồng nào.
- Thuật toán Personalized PageRank (PPR): Thực hiện 3 vòng nhân ma trận thưa với vector (SpMV) trên CPU để lan truyền trọng số liên kết tri thức, hoàn tất trong $\sim 1.12\text{ ms}$.

---

## 3. Mã hóa Dữ liệu Cá nhân (AES-256-GCM v2)

Nhằm bảo vệ tối đa dữ liệu nhạy cảm của người dùng khi lưu trữ cục bộ:
- Mọi nội dung hội thoại, ghi chú cá nhân và khóa bí mật được mã hóa bằng thuật toán đối xứng **AES-256-GCM**.
- Khóa chính được sinh từ mật khẩu người dùng thông qua hàm băm dẫn xuất khóa **Argon2id** (memory cost 64MB, iterations 3) và được bảo vệ an toàn trong kho khóa cấp hệ điều hành (Tauri Stronghold Vault).
- Mỗi bản ghi sử dụng một vector khởi tạo (IV / Nonce) 96-bit ngẫu nhiên duy nhất, đi kèm thẻ xác thực (Auth Tag) 128-bit chống giả mạo dữ liệu.

---

## 4. Chu trình Suy giảm Trí nhớ theo Đường cong Lãng quên Ebbinghaus

Để ngăn chặn cơ sở dữ liệu phình to theo thời gian và giữ cho ngữ cảnh hội thoại luôn sắc bén:
- Mỗi nút tri thức lưu trữ trường `last_accessed_at` và `recall_count`.
- Trọng số truy xuất của một sự kiện được điều chỉnh theo hàm mũ suy giảm Ebbinghaus:
  $$S(t) = S_0 \cdot e^{-\frac{t}{\tau \cdot (1 + \ln(1 + R))}}$$
  Trong đó $t$ là thời gian trôi qua kể từ lần nhắc gần nhất, $R$ là số lần người dùng tái khẳng định thông tin đó, và $\tau$ là hệ số bền vững ngữ cảnh.
- Định kỳ, tiến trình bảo trì nền sẽ dọn dẹp các liên kết đồ thị có trọng số thấp hơn ngưỡng quy định.
