---
title: "Hạ tầng Kiểm thử Tự động và Đường ống CI"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - .github/workflows/test.yml
  - liva-native-core/tests/harness.rs
---

# Hạ tầng Kiểm thử Tự động và Đường ống CI

## 1. Chiến lược và Kiến trúc Kiểm thử

LIVA áp dụng chiến lược kiểm thử tự động đa cấp độ nhằm đảm bảo tính ổn định và tính đúng đắn của toàn bộ 7 crate mà không gây quá tải tài nguyên máy phát triển:

- **Kiểm thử Đơn vị (Unit Tests)**: Viết kèm trong các tệp mã nguồn tương ứng (`#[cfg(test)]`), kiểm tra logic nội bộ của từng module (ví dụ: giải mã lệnh ngắt lượt trong `turn_taking.rs`, hàng đợi ưu tiên trong `actor.rs`, cây tiền tố `FactTrie`).
- **Khung Kiểm thử Hợp nhất (Consolidated Single Test Harness)**: Thay vì tạo hàng chục tệp `tests/*.rs` riêng biệt (mỗi tệp khiến Rust compiler sinh ra một tệp thực thi `.exe` kiểm thử độc lập, làm phình thư mục `target/` lên tới hàng trăm GB tệp PDB debug), LIVA gom các bài kiểm thử tích hợp vào một điểm vào duy nhất:
  - `liva-native-core/tests/harness.rs`
  - `crates/liva-cua/tests/harness.rs`
- **Rào chắn Tài nguyên**:
  - Luôn truyền cờ biên dịch `-j 2`.
  - Luôn truyền cờ chạy kiểm thử `-- --test-threads 2`.
  - Tiền kiểm tra bộ nhớ RAM khả dụng $\ge 4\text{ GB}$ trước khi khởi chạy bộ kiểm thử toàn diện.

---

## 2. Các Lệnh Kiểm thử Chuẩn

| Mục tiêu kiểm tra | Lệnh thực thi trên PowerShell |
|---|---|
| **Kiểm tra biên dịch toàn Workspace** | `cargo check --workspace -j 2` |
| **Chạy toàn bộ bài test Workspace** | `cargo test --workspace -j 2 -- --test-threads 2` |
| **Kiểm thử tích hợp Native Core** | `cargo test -p liva-native-core -j 2 -- --test-threads 2` |
| **Kiểm thử bộ nhớ Storage & sqlite-vec**| `cargo test -p liva-storage -j 2 -- --test-threads 2` |
| **Kiểm thử Actor Concurrency LLM** | `cargo test -p liva-llm -j 2 -- --test-threads 2` |
| **Kiểm thử tự động hóa Desktop CUA** | `cargo test -p liva-cua -j 2 -- --test-threads 2` |
| **Kiểm tra định dạng mã nguồn** | `cargo fmt --all -- --check` |
| **Kiểm tra tĩnh Clippy** | `cargo clippy --all-targets -- -D warnings` |
| **Kiểm toán giấy phép và lỗ hổng** | `cargo deny check -W unmaintained -W unsound advisories licenses sources` |
| **Kiểm tra tính toàn vẹn tài liệu** | `node scripts/docs-check.mjs --strict-stale=docs/01-kien-truc,docs/02-crates` |
| **Kiểm tra trích dẫn mã nguồn** | `node scripts/docs-citations.mjs` |

---

## 3. Quy trình CI 4 Cổng Độc lập (GitHub Actions DAG)

Đường ống tích hợp liên tục (CI) được định nghĩa trong `.github/workflows/test.yml` vận hành theo mô hình đồ thị có hướng không chu trình (DAG) gồm 4 công việc:

```
┌─────────────────────────────────────────────────────────────┐
│                 GITHUB ACTIONS CI WORKFLOW                  │
├─────────────────────────────────────────────────────────────┤
│ Job 1: gate_hygiene (Vệ sinh mã nguồn & Kiểm toán tài liệu) │
│   - actionlint kiểm tra tệp cấu hình workflow               │
│   - cargo fmt kiểm tra chuẩn định dạng mã Rust              │
│   - docs-check kiểm tra frontmatter YAML & liên kết tài liệu│
│   - docs-citations kiểm tra các trích dẫn ký hiệu mã nguồn  │
├─────────────────────────────────────────────────────────────┤
│ Job 2: gate_cargo_deny (Kiểm toán bảo mật & Giấy phép)      │
│   - Kiểm tra tương thích giấy phép nguồn mở (MIT / Apache)  │
│   - Rà soát lỗ hổng bảo mật đã công bố của các thư viện phụ │
├─────────────────────────────────────────────────────────────┤
│ Job 3: gate_check (Biên dịch tĩnh & Phân tích tĩnh)         │
│   - cargo check --workspace -j 2                            │
│   - cargo clippy --all-targets -- -D warnings               │
├─────────────────────────────────────────────────────────────┤
│ Job 4: gate_test (Thực thi kiểm thử chức năng & tích hợp)   │
│   - Chạy toàn bộ test suites trên môi trường Windows Server │
│   - Thu thập báo cáo kiểm thử và xác nhận 100% PASS         │
└─────────────────────────────────────────────────────────────┘
```

Mỗi nhánh commit khi đẩy lên GitHub đều phải vượt qua toàn bộ 4 cổng trên trước khi được phép nhập vào nhánh chính `master`.
