---
title: "Kiểm thử và CI"
updated: 2026-07-31
commit: 3688b5f
status: living
owns:
  - bang-test
  - bang-binary-verify
  - ci-pipeline
covers:
  - Cargo.toml
  - Cargo.lock
  - package.json
  - package-lock.json
  - eslint.config.js
  - .github/workflows/test.yml
  - .github/workflows/release.yml
  - liva-native-core/Cargo.toml
  - liva-native-core/src/*
  - liva-native-core/tests/*
  - liva-desktop/src-tauri/src/*
  - liva-desktop/src-tauri/tests/*
  - liva-ui/src/*
  - liva-ui/tests/*
  - liva-ui/vitest.config.ts
  - scripts/e2e-gateway-ci.mjs
  - scripts/check-installer-config.mjs
  - scripts/check-installer-config.test.mjs
  - scripts/docs-check.mjs
---
# Kiểm thử và CI

[⬆ Mục lục](../README.md) · [◀ Triển khai và runtime](03-trien-khai-va-runtime.md) ·
[Cài đặt cho người dùng ▶](05-cai-dat-cho-nguoi-dung.md)

Đây là contract vận hành hiện hành. Khảo sát chi tiết ngày 22/07/2026 đã được
lưu ở [99-luu-tru](../99-luu-tru/khao-sat-kiem-thu-va-ci-2026-07-22.md);
không dùng các số đếm và nhận định thiếu coverage trong bản đó làm trạng thái
hiện tại.

## 1. Các lớp kiểm thử

| Lớp | Lệnh chuẩn | Phạm vi | Gate CI |
|---|---|---|---|
| Rust core | `cargo test -p liva-native-core` | unit + integration: crypto boot, backup/restore, authorization, deletion, outbox persistence, MCP sandbox, WebSocket transport, voice runtime | Có |
| Tauri policy | `cargo test -p liva-desktop` | capability JSON và quyền cửa sổ `widget`/`dashboard`/`setup` | Có |
| UI unit + coverage | `npm run test:coverage -w liva-ui` | component/composable/utils trong jsdom; áp threshold từ `vitest.config.ts` | Có |
| UI type/build | `npm run build -w liva-ui` | `vue-tsc -b` + Vite production build | Có |
| Client builds | `npm run build -w liva-desktop` và `npm run build -w liva-mobile-client` | adapter desktop/mobile và type contract | Có |
| Gateway socket thật | `npm run e2e:gateway` | spawn binary, handshake WebSocket thật, response lỗi/thành công và cleanup | Có |
| Installer/setup | `npm run test:installer` và `npm run check:installer` | manifest SHA-256/revision, bundle paths, NSIS/WebView2/capability setup | Test trong verification; config gate ở release |
| Docs/Vault | `npm run docs:check`; typecheck/test workspace Vault | frontmatter, link, ownership, citations, generated views, knowledge tooling | Có |
| Experimental | `cargo check --all-targets --features experimental` | chống mục nát cho passive/evolution/swarm chưa ship | Compile-only |

Không đóng đinh tổng số test trong tài liệu. Con số thay đổi theo mỗi slice và
không phải một API; bằng chứng hợp lệ là output mới của chính các lệnh trên.

## 2. Pipeline `.github/workflows/test.yml`

Trình tự gate hiện hành:

1. checkout full history để stale-doc checker đối chiếu được commit;
2. docs structure/citations;
3. `npm ci`;
4. AI DevKit lint bản ghim `0.47.0`;
5. `npm audit --audit-level=high`;
6. `cargo fmt --all -- --check`;
7. cài `cargo-audit` ghim `0.22.2`, rồi `cargo audit`;
8. TypeScript typecheck, ESLint `--max-warnings 0`;
9. UI coverage;
10. build ba web client/workspace;
11. kiểm Vault;
12. Rust core tests;
13. gateway E2E qua socket thật;
14. Tauri tests/check;
15. compile-check experimental;
16. Clippy `--all-targets -- -D warnings`.

Các gate supply-chain chạy trên **toàn workspace**, gồm cả dev tooling vì chính
tooling xử lý file/diff không tin cậy trong CI. Không dùng `npm audit
--omit=dev` để che advisory.

## 3. Coverage UI

CI phải gọi `test:coverage`, không gọi `vitest run` trần. Threshold toàn cục
trong `liva-ui/vitest.config.ts` là chốt chống thụt lùi; các hotspot có ngưỡng
per-file riêng. Khi một file dưới ngưỡng:

1. đọc report mới;
2. thêm test hành vi ở nhánh chưa phủ;
3. không exclude file hoặc hạ threshold nếu không có quyết định kiến trúc được
   ghi lại;
4. chạy lại coverage toàn UI.

Các hotspot đang được theo dõi trực tiếp: `WidgetApp.vue`, `useGateway.ts` và
`VisionView.vue`.

## 4. Security/data-loss acceptance tests

Những contract không được thay bằng smoke test:

| Contract | Bằng chứng tự động |
|---|---|
| Khoá sai không làm mất dữ liệu | `crypto_boot_e2e.rs`, backup/restore tests |
| Transcript/checkpoint/outbox không lưu plaintext | raw SQLite/file scan trong integration tests |
| Bản nháp nhắn tin sống qua restart và chỉ lấy một lần | `messaging_outbox_persistence.rs` + unit tests outbox |
| `DeleteConversation` dry-run không mutate, execute đúng scope | `conversation_delete.rs` |
| Caller không đủ quyền không gọi được lệnh nhạy cảm | `command_authorization.rs` |
| MCP không thoát Vault | `mcp_vault_sandbox_escape.rs` |
| Khung WebSocket dữ liệu không tin cậy fail-closed | `websocket_transport.rs` + unit tests `webrtc/frame.rs` |
| Cấu hình cửa sổ setup không được nâng quyền nhầm | `capability_policy.rs` |

## 5. Release workflow

`.github/workflows/release.yml` dựng release/bundle Windows khi tag `v*`, chạy
tay và theo lịch tuần. Release chỉ được coi là sẵn sàng khi:

- test workflow xanh;
- installer config/test xanh;
- build release + NSIS thành công;
- chạy clean-machine smoke test;
- có quyết định ký số. Hiện ký số vẫn là gate phát hành còn mở, không được mô
  tả là đã giải quyết.

## 6. Lệnh verification cục bộ

```powershell
npm ci
npm run devkit:lint
npm audit --audit-level=high
cargo audit
cargo fmt --all -- --check
cargo test -p liva-native-core
cargo test -p liva-desktop
cargo clippy --all-targets -- -D warnings
npm run test:coverage -w liva-ui
npm run build -w liva-ui
npm run test:installer
npm run docs:check
```

Chỉ báo hoàn tất bằng output mới của các lệnh liên quan tới slice. Không suy ra
“đã chạy được” từ build thành công, test cũ hoặc trạng thái CI của commit khác.

## Liên quan

- [Mô hình AI và tài nguyên](02-mo-hinh-ai-va-tai-nguyen.md) — fixture/model và
  điều kiện build.
- [Triển khai và runtime](03-trien-khai-va-runtime.md) — preflight và cách khởi
  động đúng profile.
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — backlog
  chỉ được đóng sau khi các gate ở đây xanh.
- [Threat model](../05-chat-luong/threat-model.md) — bề mặt cần security test.
