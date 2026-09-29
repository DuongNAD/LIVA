---
title: "Tài liệu Kỹ thuật Crate: liva-storage"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-storage/Cargo.toml
  - crates/liva-storage/build.rs
  - crates/liva-storage/src/lib.rs
  - crates/liva-storage/src/pragmas.rs
  - crates/liva-storage/src/sqlite_vec.rs
---

# Tài liệu Kỹ thuật Crate: liva-storage

## 1. Tổng quan

`crates/liva-storage` là crate chuyên trách tầng lưu trữ dữ liệu bền vững và bộ nhớ đệm hiệu năng cao của LIVA. Crate cung cấp:
- Nhóm kết nối SQLite WAL đã hiệu chỉnh thông số phần cứng (Calibrated Pragmas).
- Tiến trình xử lý vi lô `DbActor` tuần tự hóa các thao tác ghi, loại bỏ triệt để lỗi tranh chấp khóa `SQLITE_BUSY`.
- Cây tiền tố Radix Trie trong bộ nhớ RAM (`FactTrie` và `ActiveRecallManager`) phục vụ tra cứu chủ động dưới $0.1\text{ ms}$.
- Bảng ảo vector biên dịch tĩnh C-FFI `sqlite-vec`, loại bỏ sự phụ thuộc vào các tệp `.dll` ngoại lai.

---

## 2. Biên dịch Tĩnh C-FFI `sqlite-vec` (`build.rs` & `sqlite_vec.rs`)

Thay vì nạp thư viện động `vec0.dll` lúc chạy ứng dụng (dễ gây lỗi tương thích phiên bản và yêu cầu quyền hệ thống phức tạp), `liva-storage` biên dịch trực tiếp mã nguồn C của `sqlite-vec`:

### Kịch bản biên dịch `build.rs`:
- Sử dụng crate `cc` để biên dịch tệp `c/sqlite-vec.c`.
- Thiết lập định nghĩa tiền xử lý: `SQLITE_CORE = 1` và `SQLITE_VEC_STATIC = 1`.
- Cấu hình cờ tối ưu phần cứng trên bộ biên dịch MSVC x64:
  - `/O2`: Tối ưu hóa tốc độ tối đa.
  - `/fp:fast`: Tối ưu hóa phép tính dấu phẩy động cho tính toán khoảng cách vector.
  - `/arch:AVX2`: Kích hoạt tập lệnh vector SIMD AVX2.
  - `/utf-8`: Đảm bảo xử lý chuỗi UTF-8 chuẩn xác.

### Đăng ký FFI an toàn (`sqlite_vec.rs`):
```rust
unsafe extern "C" {
    pub fn sqlite3_vec_init(
        db: *mut rusqlite::ffi::sqlite3,
        pzErrMsg: *mut *mut std::ffi::c_char,
        pApi: *const rusqlite::ffi::sqlite3_api_routines,
    ) -> std::ffi::c_int;
}

pub fn register_sqlite_vec() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    });
}
```
Nhờ cơ chế `auto_extension`, mọi kết nối mới mở từ `rusqlite` đều tự động có sẵn mô-đun bảng ảo vector `vec0`.

---

## 3. Cấu hình Hiệu chuẩn PRAGMA SQLite (`pragmas.rs`)

Mỗi kết nối cơ sở dữ liệu khi mở đều được áp dụng bộ thông số tối ưu:

| PRAGMA | Giá trị | Mục đích kỹ thuật |
|---|---|---|
| `foreign_keys` | `ON` | Ràng buộc toàn vẹn khóa ngoại trên toàn bộ 20 bảng schema. |
| `busy_timeout` | `5000` | Chờ tối đa 5000ms nếu bảng đang bị khóa trước khi trả lỗi. |
| `cache_size` | `-2000` | Giới hạn bộ đệm trang khoảng ~2 MB RAM (thay mức mặc định 312 MB). |
| `page_size` | `4096` | Khớp khối phân trang 4KB của Windows NTFS/NVMe. |
| `mmap_size` | `268435456` | Ánh xạ trực tiếp 256 MB tệp cơ sở dữ liệu vào bộ nhớ ảo. |
| `temp_store` | `MEMORY` | Bảng tạm và sắp xếp trung gian thực hiện hoàn toàn trong RAM. |
| `journal_mode` | `WAL` | Kích hoạt Write-Ahead Logging, cho phép đọc ghi đồng thời. |
| `synchronous` | `NORMAL` | Giảm tải việc gọi `fsync` liên tục xuống đĩa mà vẫn đảm bảo tính toàn vẹn WAL. |
| `journal_size_limit` | `67108864` | Khống chế tệp WAL không vượt quá 64 MB. |
| `wal_autocheckpoint` | `1000` | Tự động chuyển trang WAL về tệp chính khi đạt 1000 trang. |

---

## 4. Mô hình Vi lô Ghi Dữ liệu `DbActor`

Để xử lý tải đồng quy cao từ hàng chục tác vụ async mà không gây tắc nghẽn `SQLITE_BUSY`:
- **Cơ chế**: Toàn bộ thao tác ghi (`DbWriteCommand::Execute`, `Flush`, `CheckpointWal`) được gửi tới một kênh Tokio bounded MPSC (kích thước đệm 1024).
- **Vòng lặp sự kiện (Event Loop)**: Được ghim vào một luồng hệ điều hành độc lập. Vòng lặp gom các lệnh ghi theo cơ chế vi lô: tối đa 50 câu lệnh hoặc tối đa 5ms chờ đợi.
- **Thực thi giao dịch**:
  ```sql
  BEGIN IMMEDIATE;
  -- Thực thi toàn bộ lệnh trong lô
  COMMIT;
  ```
  Nếu có bất kỳ lỗi nào xảy ra trong lô, giao dịch tự động `ROLLBACK;` và báo lỗi về cho người gọi thông qua `oneshot::Sender`. Năng lực ghi đạt từ 1,500 đến 3,000 thao tác/giây.

---

## 5. Cây Tiền tố Radix Trie (`FactTrie` & `ActiveRecallManager`)

Nằm trong `crates/liva-storage/src/lib.rs`:
- **`FactTrie`**: Cấu trúc dữ liệu cây tiền tố trong bộ nhớ RAM:
  - `insert(key, fact_key)`: Chuẩn hóa chữ thường và chèn khóa thực thể.
  - `search(text)`: Sử dụng cửa sổ trượt quét qua từng chuỗi con của câu người dùng nói, tìm kiếm các khóa tri thức liên quan với thời gian $< 0.1\text{ ms}$.
  - `search_prefix(prefix)`: Duyệt nhánh cây để tự động hoàn thành từ khóa hoặc gợi ý tri thức.
- **`ActiveRecallManager`**:
  - Đóng gói `FactTrie` và danh sách thách thức đang chờ duyệt (`pending_challenges`).
  - Hàm `try_intercept_turn`: Đánh chặn lượt hội thoại trước khi chuyển vào LLM. Nếu người dùng nhắc đến một sự kiện quan trọng, hệ thống có thể lập tức đưa ra phản hồi hoặc câu đố củng cố mà không tiêu tốn token của mô hình ngôn ngữ lớn.
