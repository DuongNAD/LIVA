---
title: "Runbook backup và restore SQLite"
updated: 2026-07-31
commit: 3688b5f
stale-ok: bd11c84
status: living
covers:
  - liva-native-core/src/persistence_backup.rs
  - liva-native-core/tests/sqlite_backup_restore.rs
---
# Runbook backup và restore SQLite

[⬆ Mục lục](../README.md) · [Persistence](../03-he-thong-con/persistence.md) ·
[Threat model](../05-chat-luong/threat-model.md)

## Contract

- Backup dùng SQLite Online Backup API; không copy trực tiếp file database đang chạy WAL.
- Mỗi backup có manifest v2 chứa SHA-256, kích thước, schema version, thời điểm và key-ID.
- Restore từ chối sai key-ID trước khi chạm target, rồi xác minh checksum, `quick_check` và
  `foreign_key_check`.
- Restore giữ database cũ cùng WAL/SHM dưới dạng `.rollback`.
- Manifest và backup không chứa device key/recovery key. Muốn phục hồi facts mã hóa phải bảo toàn
  key tương ứng riêng biệt.

## Tạo backup online

Gọi `persistence_backup::backup_database(&pool, destination, &crypto.key_id())` khi runtime đang
hoạt động.
Destination và manifest không được tồn tại trước; API fail-closed thay vì ghi đè.

Sau khi hoàn tất, giữ hai file cùng nhau:

```text
liva-2026-07-30.backup.db
liva-2026-07-30.backup.db.manifest.json
```

Không coi backup thành công nếu thiếu manifest. Sao chép cả hai artifact sang nơi lưu trữ có kiểm
soát truy cập; giữ recovery key ở kênh khác.

## Restore offline

1. Dừng hoàn toàn LIVA/Tauri và xác nhận không còn process giữ database.
2. Sao chép backup và manifest về cùng thư mục cục bộ.
3. Nạp recovery key dự kiến và gọi
   `persistence_backup::restore_database(backup, target, &crypto.key_id())`.
4. Ghi lại `rollback_path` trả về; không xóa cho tới khi smoke test đạt.
5. Khởi động LIVA, kiểm tra boot, recall fixture, skills và contacts.
6. Chỉ xóa rollback theo retention policy sau khi người vận hành xác nhận.

Nếu key-ID, checksum, schema, SQLite integrity hoặc FK sai, restore kết thúc trước khi target bị
đổi. Nếu atomic swap thất bại, code đưa target cũ trở lại. Key-ID chỉ là fingerprint; nó không thể
thay recovery key và không được dùng như secret.

## Diễn tập bắt buộc trước release

```powershell
cargo test --manifest-path liva-native-core/Cargo.toml --test sqlite_backup_restore
```

Gate hiện tại khóa:

- online backup lấy đúng trạng thái tại thời điểm snapshot;
- restore trả dữ liệu snapshot;
- bản database trước restore còn đọc được ở rollback;
- sửa một byte/append dữ liệu vào backup làm checksum fail và target không đổi;
- sai recovery key bị từ chối trước khi chạm target;
- đúng recovery key phục hồi và giải mã được canary; file backup không chứa canary plaintext.

## Khoảng trống còn lại

- Chưa có scheduler/quota/nút UI cho **backup**. Retention conversation là worker riêng,
  opt-in qua `LIVA_MEMORY_RETENTION_*`, không thay thế backup retention.
- Chưa tự động backup trước migration.
- Chưa có release matrix phục hồi artifact từ mọi schema/version được hỗ trợ.
- Restore là API maintenance offline, chưa phải command plane từ xa.
