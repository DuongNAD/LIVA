---
title: "Kiểm kê và disposition tài liệu LIVA"
updated: 2026-11-01
commit: eadb605b
status: index
owns:
  - inventory-disposition-tai-lieu
covers:
  - docs/_data/document-inventory.json
  - scripts/docs-inventory.mjs
---
# Kiểm kê và disposition tài liệu LIVA

[⬆ Mục lục](../README.md) · [Quy hoạch tài liệu](../07-dong-gop/quy-hoach-tai-lieu.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

> File này được sinh từ [`docs/_data/document-inventory.json`](../_data/document-inventory.json)
> và link Markdown trong `docs/`. Không sửa tay.

## Tóm tắt

| Disposition | Số tài liệu |
|---|---:|
| KEEP | 17 |
| SPLIT | 0 |
| GENERATE | 2 |
| FREEZE | 112 |
| MERGE | 0 |
| **Tổng** | **131** |

## Quy ước

| Nhãn | Quyết định |
|---|---|
| KEEP | Giữ vai trò hiện tại trong giai đoạn chuyển tiếp |
| SPLIT | Tách theo subsystem/contract trước khi để lại redirect |
| GENERATE | Dữ liệu phải sinh tự động, không duy trì bảng bằng tay |
| FREEZE | Đóng băng như bằng chứng lịch sử |
| MERGE | Nhập phần còn giá trị vào canonical owner rồi để lại redirect |

## Danh sách đầy đủ

| File | Disposition | Đợt | Link vào | Đích | Lý do |
|---|---|---|---:|---|---|
| `docs/01-kien-truc/01-tong-quan-va-tam-nhin.md` | **KEEP** | A | 1 | — | Tài liệu kiến trúc hệ thống LIVA Native Core và nguyên lý hoạt động. |
| `docs/01-kien-truc/02-nguyen-ly-cognitive-actor.md` | **KEEP** | A | 1 | — | Tài liệu kiến trúc hệ thống LIVA Native Core và nguyên lý hoạt động. |
| `docs/01-kien-truc/03-kien-truc-bo-nho-da-tang.md` | **KEEP** | A | 1 | — | Tài liệu kiến trúc hệ thống LIVA Native Core và nguyên lý hoạt động. |
| `docs/01-kien-truc/04-giao-thuc-ipc-tauri-v2.md` | **KEEP** | A | 1 | — | Tài liệu kiến trúc hệ thống LIVA Native Core và nguyên lý hoạt động. |
| `docs/02-crates/01-liva-native-core.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/02-crates/02-liva-storage.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/02-crates/03-liva-llm.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/02-crates/04-liva-core-types.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/02-crates/05-liva-cua.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/02-crates/06-liva-tools.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/02-crates/07-liva-desktop-src-tauri.md` | **KEEP** | A | 1 | — | Tài liệu kỹ thuật crate thuộc Rust Workspace 7 crate của LIVA. |
| `docs/03-phat-trien-van-hanh/01-cau-hinh-va-bien-moi-truong.md` | **KEEP** | A | 1 | — | Hướng dẫn phát triển, cấu hình môi trường, kiểm thử CI và vận hành LIVA. |
| `docs/03-phat-trien-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md` | **KEEP** | A | 1 | — | Hướng dẫn phát triển, cấu hình môi trường, kiểm thử CI và vận hành LIVA. |
| `docs/03-phat-trien-van-hanh/03-huong-dan-phat-trien-va-debug.md` | **KEEP** | A | 1 | — | Hướng dẫn phát triển, cấu hình môi trường, kiểm thử CI và vận hành LIVA. |
| `docs/03-phat-trien-van-hanh/04-kiem-thu-va-ci.md` | **KEEP** | A | 1 | — | Hướng dẫn phát triển, cấu hình môi trường, kiểm thử CI và vận hành LIVA. |
| `docs/03-phat-trien-van-hanh/05-sao-luu-va-xu-ly-su-co.md` | **KEEP** | A | 1 | — | Hướng dẫn phát triển, cấu hình môi trường, kiểm thử CI và vận hành LIVA. |
| `docs/99-luu-tru/ai-devkit-pointers/design/README.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ai-devkit-pointers/implementation/README.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ai-devkit-pointers/planning/README.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ai-devkit-pointers/requirements/README.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ai-devkit-pointers/testing/README.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/00-tong-quan-he-thong.md` | **FREEZE** | C | 2 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/01-cau-hinh-va-bien-moi-truong.md` | **FREEZE** | C | 3 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/01-kien-truc-tong-the.md` | **FREEZE** | C | 9 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/02-giao-thuc-ipc-va-websocket.md` | **FREEZE** | C | 10 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/02-mo-hinh-ai-va-tai-nguyen.md` | **FREEZE** | C | 4 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/03-duong-ong-thoai.md` | **FREEZE** | C | 5 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/03-trien-khai-va-runtime.md` | **FREEZE** | C | 4 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/04-he-llm-va-prompt.md` | **FREEZE** | C | 6 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/04-kiem-thu-va-ci.md` | **FREEZE** | C | 3 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/05-agent-bo-nho-va-tien-hoa.md` | **FREEZE** | C | 2 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/05-cai-dat-cho-nguoi-dung.md` | **FREEZE** | C | 2 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/06-backup-restore-sqlite.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/06-thi-giac-passive-va-governor.md` | **FREEZE** | C | 5 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/07-tang-du-lieu-va-bao-mat.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/08-frontend-va-vo-tauri.md` | **FREEZE** | C | 4 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/09-tich-hop-ngoai.md` | **FREEZE** | C | 5 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/10-phu-thuoc-module-va-tra-cuu.md` | **FREEZE** | C | 8 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/adr-001-ma-hoa-du-lieu-ca-nhan-beta.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/agent-tools.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/cognitive-runtime.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/context-broker.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/desktop-tauri.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/frontend.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/inventory-he-thong.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/KNOWLEDGE_BASE.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/memory.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/NEW_feature_template.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/persistence.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/release-v1.0.0-smoke-test.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/tam-nhin-jarvis.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/vision.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/voice.md` | **FREEZE** | C | 2 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ban-ve-va-khao-sat-2026-07/wake-word.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/00-bao-cao-khao-sat-goc-2026-07.md` | **FREEZE** | C | 3 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/01-doi-chieu-tuyen-bo-vs-thuc-te.md` | **FREEZE** | C | 4 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/02-no-ky-thuat-va-rui-ro.md` | **FREEZE** | C | 5 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/03-lo-trinh-sua-loi-va-nang-cap.md` | **FREEZE** | C | 4 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/04-de-xuat-tich-hop-openspace.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/05-nang-cap-toan-dien.md` | **FREEZE** | C | 4 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/06-nhan-tin-ra-ngoai.md` | **FREEZE** | C | 2 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/07-wake-word-viec-con-lai.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/08-tien-do-nang-cap-va-giai-quyet-no-ky-thuat.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/action-policy.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/architecture_remake_proposal.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/architecture-review/architecture-review-report-2026-05-31-20-12.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/architecture-review/architecture-review-report-2026-05-31.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/BAO_CAO_AUDIT_TOAN_DIEN_VA_TOI_UU_HE_THONG_2026_09.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/beta-thuc-dia.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_4TIER_MEMORY_ARCHITECTURE_AUDIT_AND_BLUEPRINT.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_Acceptance_Report_2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_ARCHITECTURAL_EVALUATION_AND_OPTIMIZATION_REPORT.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_Architecture_Audit_2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_COMPREHENSIVE_SYSTEM_AUDIT_REPORT_2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_OSS_Research_2026-07.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_SOTA_RESEARCH_BENCHMARKING_AND_STRATEGIC_ROADMAP_2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_SYSTEM_AUDIT_AND_ROADMAP_2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/liva_test_report.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_UPGRADE_RESEARCH_AND_BLUEPRINT_2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/resource-governor.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/spring-cleaning/spring-cleaning-report-2026-05-31.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/TEST_INFRA.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/TEST_READY.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/threat-model.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/vision-prompt-budget-acceptance.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/voice-slo.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/wake-benchmark.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/bao-cao-lich-su/wer-fleurs-vi.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/implementation_plan.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/KE-HOACH-AVATAR-3D.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/KE-HOACH-XU-LY-18-08-2026.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/LIVA_NATIVE_MIGRATION_PLAN.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/parakeet_vi_integration_plan.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/roadmap-2026-08.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/VAN-DE-CAN-XU-LY.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/khao-sat-kiem-thu-va-ci-2026-07-22.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/01_System_Overview.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/02_Memory_Subsystem.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/03_Agent_Control_Flow.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/04_Evolution_Singularity.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/05_Security_Guardrails.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/06_Hardware_UX_Optimization.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/07_Hybrid_Cloud_Architecture.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/AI_CONTEXT.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/Architectural_Teardown_Proposal.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/codebase_architecture.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/personality_architecture_report.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/PROJECT.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/skills_development_guide.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/STARTUP_GUIDE.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/streaming_optimization.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/TEST_READY.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/meta-cu/ban-do-code-tai-lieu.md` | **FREEZE** | C | 2 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/meta-cu/huong-dan-bao-tri.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/meta-cu/nguon-su-that.md` | **FREEZE** | C | 1 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/_meta/optimize-architecture-review.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/_meta/optimize-code-review.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/_meta/optimize-readme.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/_meta/optimize-spring-cleaning.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/architecture-review.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/code-review-prompt.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/openspace-g0-mcp-client-prompt.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/readme-generation-prompt.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/prompts-cu/spring-cleaning-prompt.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/quy-hoach-tai-lieu-2026-07.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/README.md` | **FREEZE** | C | 50 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/registries-2026-07/kiem-ke-tai-lieu.md` | **GENERATE** | B | 0 | — | Báo cáo kiểm kê tài liệu được sinh tự động từ scripts/docs-inventory.mjs. |
| `docs/99-luu-tru/registries-2026-07/ma-tran-nang-luc.md` | **GENERATE** | A | 0 | — | Ma trận năng lực được sinh tự động từ capabilities.json. |
| `docs/99-luu-tru/thiet-ke-goc/LIVA_CLIENT_SERVER_DESIGN.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/99-luu-tru/thiet-ke-goc/ORIGINAL_REQUEST.md` | **FREEZE** | C | 0 | — | Ảnh chụp lịch sử và tài liệu đã di trú vào kho lưu trữ 99-luu-tru. |
| `docs/README.md` | **KEEP** | A | 2 | — | Mục lục tổng quan và điểm vào chính của bộ tài liệu LIVA. |

## File chưa được phép di chuyển ngay

Các file sau có disposition `SPLIT/MERGE` và đang có link Markdown trỏ vào. Phải cập nhật
inbound link trước, sau đó giữ redirect ít nhất một chu kỳ release.

- Không có.

## Gate

- Mọi `docs/**/*.md` phải có đúng một disposition.
- `SPLIT` và `MERGE` phải có ít nhất một target.
- Registry không được trỏ tới file nguồn đã biến mất.
- Chạy `npm run docs:inventory:check` để phát hiện thiếu file hoặc generated drift.
