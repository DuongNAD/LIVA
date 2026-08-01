# Lưu trữ — artifact thời Node.js (25/06/2026)

Hai file này được chuyển vào đây ngày 02/08/2026 theo [U2](../../docs/03-danh-gia/05-nang-cap-toan-dien.md).

**Vì sao xác định là di sản:** `desktop-client.exe` chứa chuỗi **Electron** và
**Node**, và **không** chứa `liva-native-core` — tức nó là bản trước khi lõi Rust
được nhúng vào vỏ Tauri. `desktop-client-setup.exe` (2,5 MB) không có dấu vết
nào, là stub NSIS cùng ngày, di sản theo liên đới.

**CHUYỂN chứ không XOÁ, có lý do:** cả hai đều nằm trong `.gitignore`, nên xoá là
**mất vĩnh viễn** — git không khôi phục được. Hoàn tác bằng cách chuyển ngược ra
`release/`.

**Artifact hiện hành** không nằm ở đây: `target/release/bundle/nsis/LIVA_25.0.0_x64-setup.exe`
(224,4 MB, dựng 31/07/2026, **bản CPU** — xem cảnh báo về vision ở §U2).

`release/liva-mobile.apk` là thứ khác, không thuộc phạm vi U2, để nguyên.
