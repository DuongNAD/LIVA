---
title: "Mục lục Tổng quan Hệ thống Tài liệu Kỹ thuật LIVA"
updated: 2026-09-29
commit: 25e4229
status: index
covers:
  - Cargo.toml
---

# Mục lục Tổng quan Hệ thống Tài liệu Kỹ thuật LIVA

Chào mừng bạn đến với hệ thống tài liệu kỹ thuật chính thức của dự án **LIVA (Local Intelligent Virtual Assistant)**.

Hệ thống tài liệu được tổ chức theo mô hình **4 Tầng Chuẩn hóa (4-Tier Documentation Structure)** bằng tiếng Việt kỹ thuật chuẩn, phản ánh chính xác 100% hiện trạng mã nguồn thực tế của kiến trúc Rust Native Multi-Crate Workspace và vỏ ứng dụng máy tính Tauri v2.

---

## 1. Sơ đồ Phân tầng Hệ thống Tài liệu

```
docs/
├── README.md                                          # Trang điều hướng tổng thể (Bạn đang xem)
├── 01-kien-truc/                                      # TẦNG 1: KIẾN TRÚC TỔNG THỂ & NGUYÊN LÝ
│   ├── 01-tong-quan-va-tam-nhin.md                    # Triết lý Local-First, ranh giới an toàn, 7 crate workspace
│   ├── 02-nguyen-ly-cognitive-actor.md                # Mô hình Native Actor, chu trình nhận thức, Smart Turn v3.2
│   ├── 03-kien-truc-bo-nho-da-tang.md                 # Bộ nhớ L0-L3: Radix Trie, SQLite WAL, sqlite-vec, HippoRAG
│   └── 04-giao-thuc-ipc-tauri-v2.md                   # Kênh Tauri v2 IPC nội trình, zero open ports, Specta types
├── 02-crates/                                         # TẦNG 2: TÀI LIỆU KỸ THUẬT CHI TIẾT 7 CRATE
│   ├── 01-liva-native-core.md                         # Orchestration Facade, AppState, WebRTC audio, Qwen3-VL
│   ├── 02-liva-storage.md                             # SQLite WAL pool, DbActor micro-batching, static C-FFI
│   ├── 03-liva-llm.md                                 # Non-blocking LlmActor priority queue, token budgeting
│   ├── 04-liva-core-types.md                          # Domain types, errors, permissions, JSON schemas
│   ├── 05-liva-cua.md                                 # Computer-Use Agent Win32 automation, DPI, kill switch
│   ├── 06-liva-tools.md                               # Hợp nhất CLI suite: probe db/audio, bench, doctor
│   └── 07-liva-desktop-src-tauri.md                   # Vỏ Tauri v2 đa cửa sổ, Ghost Mode WH_MOUSE_LL, IPC commands
├── 03-phat-trien-van-hanh/                            # TẦNG 3: HƯỚNG DẪN PHÁT TRIỂN, KIỂM THỬ & VẬN HÀNH
│   ├── 01-cau-hinh-va-bien-moi-truong.md              # Toàn bộ biến môi trường PowerShell ($env:LIVA_*), phần cứng
│   ├── 02-mo-hinh-ai-va-tai-nguyen.md                 # Quản lý weights GGUF & ONNX, setup:models, SHA-256 trust
│   ├── 03-huong-dan-phat-trien-va-debug.md            # Môi trường Win x64, VS2022 C++, build tuần tự, tracing log
│   ├── 04-kiem-thu-va-ci.md                           # Bộ kiểm thử hợp nhất, cờ -j 2 & test-threads 2, CI 4-job DAG
│   └── 05-sao-luu-va-xu-ly-su-co.md                   # Online SQLite backup/restore, troubleshooting runbook
└── 99-luu-tru/                                        # TẦNG 4: KHO LƯU TRỮ LỊCH SỬ & KẾ HOẠCH ĐÃ HOÀN THÀNH
    ├── README.md                                      # Cảnh báo cách ly và mục lục các tài liệu cũ
    ├── bao-cao-lich-su/                               # Các báo cáo audit, khảo sát SOTA, benchmark từ 2026-07 đến 2026-09
    ├── ke-hoach-da-hoan-thanh/                        # Kế hoạch di trú Rust, Avatar 3D, U1-U33, hoàn thành M1-M4
    ├── kien-truc-nodejs-v29/                          # Toàn bộ 16 tài liệu kiến trúc thời Node.js / Python cũ
    ├── ban-ve-va-khao-sat-2026-07/                    # Các bản vẽ chi tiết trước khi phân tách multi-crate
    ├── thiet-ke-goc/                                  # Thiết kế và yêu cầu ban đầu
    ├── prompts-cu/                                    # Các prompt sinh tài liệu lịch sử
    ├── registries-2026-07/                            # Dữ liệu kiểm kê tài liệu cũ
    └── meta-cu/                                       # Metadata công cụ cũ
```

---

## 2. Bảng Tra cứu Tài liệu theo Vai trò Kỹ thuật

| Vai trò Kỹ thuật | Tài liệu Trọng tâm Cần đọc |
|---|---|
| **Kiến trúc sư Hệ thống (System Architect)** | [01. Tổng quan & Tầm nhìn](01-kien-truc/01-tong-quan-va-tam-nhin.md) <br> [02. Nguyên lý Cognitive Actor](01-kien-truc/02-nguyen-ly-cognitive-actor.md) <br> [03. Bộ nhớ Đa tầng & Hybrid RAG](01-kien-truc/03-kien-truc-bo-nho-da-tang.md) |
| **Kỹ sư Backend & Rust Developer** | [Crate: liva-native-core](02-crates/01-liva-native-core.md) <br> [Crate: liva-storage](02-crates/02-liva-storage.md) <br> [Crate: liva-llm](02-crates/03-liva-llm.md) <br> [Crate: liva-core-types](02-crates/04-liva-core-types.md) |
| **Kỹ sư Tự động hóa & Desktop (CUA / Tauri)** | [Crate: liva-cua](02-crates/05-liva-cua.md) <br> [Crate: liva-desktop (src-tauri)](02-crates/07-liva-desktop-src-tauri.md) <br> [Giao thức Tauri v2 IPC](01-kien-truc/04-giao-thuc-ipc-tauri-v2.md) |
| **Kỹ sư Đảm bảo Chất lượng & DevOps (QA/CI)** | [Crate: liva-tools](02-crates/06-liva-tools.md) <br> [Hướng dẫn Phát triển & Debug](03-phat-trien-van-hanh/03-huong-dan-phat-trien-va-debug.md) <br> [Hạ tầng Kiểm thử & Đường ống CI](03-phat-trien-van-hanh/04-kiem-thu-va-ci.md) |
| **Chuyên viên Vận hành Hệ thống** | [Cấu hình & Biến Môi trường](03-phat-trien-van-hanh/01-cau-hinh-va-bien-moi-truong.md) <br> [Quản lý Mô hình AI](03-phat-trien-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) <br> [Sao lưu & Xử lý Sự cố](03-phat-trien-van-hanh/05-sao-luu-va-xu-ly-su-co.md) |

---

## 3. Nguồn Sự thật Duy nhất (Single Source of Truth)

Ngoài hệ thống tài liệu Markdown kỹ thuật trong thư mục `docs/`, các hướng dẫn chi tiết về tiêu chuẩn viết mã, quy tắc an toàn git và kho tri thức ngữ nghĩa của các Agent được đồng bộ tại:
- **Obsidian Vault**: `teamwork_projects/obsidian_llm_wiki/vault`
- **Tài liệu tham khảo lịch sử**: Toàn bộ các kế hoạch đã hoàn thành và tài liệu thời kỳ kiến trúc cũ được cô lập tại [docs/99-luu-tru/README.md](99-luu-tru/README.md).
