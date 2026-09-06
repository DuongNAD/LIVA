---
title: "ADR-001 — Mã hóa dữ liệu cá nhân mức beta"
updated: 2026-07-31
commit: 3688b5f
stale-ok: a0153135
status: living
owns:
  - adr-ma-hoa-du-lieu-ca-nhan-beta
covers:
  - liva-native-core/src/crypto.rs
  - liva-native-core/src/db.rs
  - liva-native-core/src/lib.rs
  - liva-native-core/src/agent/memory.rs
  - liva-native-core/src/persistence_backup.rs
  - liva-native-core/tests/crypto_boot_e2e.rs
  - liva-native-core/tests/sqlite_backup_restore.rs
---
# ADR-001 — Mã hóa dữ liệu cá nhân mức beta

[⬆ Mục lục](../README.md) · [Threat model](../05-chat-luong/threat-model.md) ·
[Persistence](../03-he-thong-con/persistence.md) ·
[Runbook backup/restore](../02-van-hanh/06-backup-restore-sqlite.md)

## Trạng thái

**Accepted và đã triển khai ngày 2026-07-31.**

Phạm vi beta là plaintext hội thoại có thể đọc trực tiếp và checkpoint agent. Metadata phục vụ
định tuyến, vector embedding, contacts/tasks/events không tự động được xem là đã mã hóa bởi ADR này.

## Bối cảnh

Trước ADR, `facts.value` dùng AES-256-GCM nhưng:

- `agent_checkpoints.state_json` giữ prompt, message và tool context ở plaintext;
- `vectors_meta.content` của `conversation_turn` và projection FTS giữ transcript plaintext;
- UPDATE/delete trong WAL có thể để lại byte plaintext cũ;
- backup không cho biết recovery key nào tương thích.

Mã hóa mù toàn bộ SQLite bằng field crypto sẽ làm FTS5 không tìm được. Chuyển ngay sang SQLCipher
thay đổi native dependency, packaging, migration toàn file và backup contract sát ngày beta.

## Quyết định

### 1. Dùng field encryption hiện có cho hai payload nhạy cảm

- `agent_checkpoints.state_json` luôn được mã hóa bằng `EncryptionEngine` trước khi ghi.
- `vectors_meta.content` luôn được mã hóa khi `type='conversation_turn'`.
- Format là AES-256-GCM v2 với HKDF-SHA256, salt và IV ngẫu nhiên từng bản ghi.
- Đường đọc dùng trạng thái fail-closed: sai/mất khóa không trả ciphertext vào prompt hoặc UI.

### 2. Conversation recall dùng dense-only

- `conversation_turn` không được ghi vào `vectors_fts`.
- Mọi FTS row legacy của loại này bị xóa lúc boot.
- `vec_idx` và metadata scope vẫn tồn tại; dense search chọn ứng viên rồi mới giải mã content.
- Các loại memory không chứa transcript vẫn giữ hybrid dense + FTS như trước.

Đánh đổi được chấp thuận: recall hội thoại mất lexical-only match nhưng không giữ thêm một bản
plaintext. Dense recall và lineage/domain/category vẫn hoạt động.

### 3. Migration boot có rescue key và dọn remnant

`resolve_and_rekey` xử lý cả checkpoint và conversation:

1. bản plaintext được mã hóa bằng live key;
2. ciphertext khóa mặc định hoặc `LIVA_ENCRYPTION_KEY_OLD` được giải rồi mã hóa lại;
3. ciphertext không khóa nào mở được được giữ nguyên và báo `locked`, không mã hóa chồng;
4. UPDATE so khớp giá trị gốc để tránh lost update;
5. trên DB đĩa, `secure_delete`, WAL checkpoint `TRUNCATE` và `VACUUM` loại byte plaintext cũ.

Migration idempotent: ciphertext v2 đã mở được bằng live key không bị đổi salt vô ích.

### 4. Backup ràng buộc với recovery key

- `EncryptionEngine::key_id()` tạo fingerprint SHA-256 có domain separation, lấy 16 byte
  (32 ký tự hex). Đây là identifier, không phải secret.
- Manifest version 2 chứa `key_id`, không chứa device/recovery key.
- Restore phải nhận expected key-ID và từ chối mismatch trước khi tạo/swap target.
- Recovery key vẫn được lưu ở kênh khác với backup.

Fingerprint của passphrase yếu có thể hỗ trợ dò từ điển; production key phải là device key ngẫu
nhiên hoặc secret có entropy cao, không phải mật khẩu người dùng dễ đoán.

## Phương án không chọn

| Phương án | Lý do chưa chọn ở beta |
|---|---|
| SQLCipher toàn DB | Thay native/runtime packaging và cần migration/restore matrix lớn hơn lát beta |
| Chỉ dựa BitLocker/OS filesystem | Không phải policy do ứng dụng cưỡng chế; backup rời máy mất bảo vệ |
| Mã hóa mọi `vectors_meta.content` | Phá FTS cho cả loại memory không chứa transcript |
| Giữ transcript FTS plaintext | Không đạt mục tiêu raw DB/WAL không chứa canary |

SQLCipher vẫn là ứng viên hậu beta nếu cần bảo vệ cả metadata, contacts/tasks/events và vector.

## Hệ quả và vận hành

- Bản phát hành cũ không hiểu conversation/checkpoint ciphertext mới; downgrade trực tiếp không
  được hỗ trợ. Trước nâng cấp lớn phải giữ backup + manifest + recovery key tương thích.
- Mất recovery key có thể làm payload thành `locked`; code giữ ciphertext nguyên để cứu khi có
  đúng key, nhưng không thể tự khôi phục khóa.
- Backup/restore là maintenance API offline; scheduler/quota backup, UI và release restore
  matrix vẫn thuộc backlog persistence. Retention conversation opt-in đã được tách thành worker
  riêng và không thay đổi phạm vi security gate mã hóa này.

## Bằng chứng acceptance

```powershell
cargo test -p liva-native-core checkpoint --lib
cargo test -p liva-native-core conversation_turn_ --lib
cargo test -p liva-native-core --test crypto_boot_e2e
cargo test -p liva-native-core --test sqlite_backup_restore
```

Các test khóa:

- raw checkpoint/vector/FTS không chứa canary;
- dense recall giải mã đúng;
- migration plaintext/old-key idempotent và không làm mất locked ciphertext;
- DB/WAL sau purge không còn byte canary;
- restore sai key-ID không chạm target; đúng recovery key đọc lại được payload.
