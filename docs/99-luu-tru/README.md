# Kho Lưu trữ Lịch sử và Kế hoạch Đã hoàn thành (`docs/99-luu-tru/`)

> ⚠️ **CẢNH BÁO QUAN TRỌNG DÀNH CHO CÁC NHÀ PHÁT TRIỂN VÀ AI AGENT**:  
> Toàn bộ các tệp tin trong thư mục `docs/99-luu-tru/` là **ảnh chụp lịch sử (Historical Snapshots)** và **kế hoạch kỹ thuật đã hoàn thành**.  
> **TUYỆT ĐỐI KHÔNG** sử dụng các hướng dẫn trong thư mục này để vận hành hay sửa đổi mã nguồn hiện tại của dự án LIVA. Dự án hiện đã di trú $100\%$ sang kiến trúc Native Rust Multi-Crate Workspace kết hợp Tauri v2. Mọi tàn dư liên quan đến máy chủ Node.js gateway, Python FastAPI, hay máy chủ loopback WebSocket cổng 8002 đã bị xóa bỏ hoàn toàn.

---

## 1. Mục đích của Kho Lưu trữ

Dự án LIVA trải qua nhiều giai đoạn tiến hóa công nghệ:
1. **Giai đoạn Node.js / Python (v29 trở về trước)**: Sử dụng kiến trúc microservices phân tán qua cổng mạng loopback.
2. **Giai đoạn Rust Monolith (Tháng 07/2026)**: Hợp nhất toàn bộ logic vào thư viện `liva-native-core` duy nhất.
3. **Giai đoạn Multi-Crate Workspace & SOTA 2026 (Tháng 09/2026 đến nay)**: Tái cấu trúc thành Rust Workspace 7 crate độc lập, tích hợp bộ nhớ đa tầng (Radix Trie + SQLite WAL + sqlite-vec C-FFI tĩnh + HippoRAG PPR), động cơ Actor LLM và CUA Win32.

Kho lưu trữ `docs/99-luu-tru/` được duy trì để lưu giữ bối cảnh ra quyết định kỹ thuật (ADR), các kết quả đo kiểm quá khứ và nhật ký khảo sát nghiên cứu mà không làm loãng hệ thống tài liệu sống (`docs/01-kien-truc/`, `docs/02-crates/`, `docs/03-phat-trien-van-hanh/`).

---

## 2. Cấu trúc Các Thư mục Con Lưu trữ

| Thư mục / Tệp | Nội dung lưu trữ | Ghi chú |
|---|---|---|
| `bao-cao-lich-su/` | Toàn bộ các báo cáo kiểm toán hệ thống, khảo sát nghiên cứu SOTA 2026, đánh giá nợ kỹ thuật và benchmark âm thanh/LLM. | Chứa các file `BAO_CAO_AUDIT_*`, `LIVA_SOTA_*`, `LIVA_COMPREHENSIVE_*`, `TEST_INFRA.md`, `TEST_READY.md`. |
| `ke-hoach-da-hoan-thanh/` | Kế hoạch di trú lõi Rust (`LIVA_NATIVE_MIGRATION_PLAN.md`), kế hoạch Avatar 3D, kế hoạch xử lý lỗi 18/08, các mốc U1-U33 và M1-M4. | Các kế hoạch này đã thực thi xong $100\%$. |
| `kien-truc-nodejs-v29/` | 16 tệp tài liệu kiến trúc, hướng dẫn khởi động và ngữ cảnh AI thời kỳ Node.js (`liva-gateway`) và Python (`liva-ai-engine`). | Lưu trữ tham khảo lịch sử. Không chạy lại. |
| `ban-ve-va-khao-sat-2026-07/` | Toàn bộ 11 bản vẽ hệ thống thuộc `docs/01-ban-ve/` cũ và các tài liệu hệ thống con `docs/03-he-thong-con/` trước khi tách crate. | Chứa các tham chiếu lịch sử tới WebSocket 8002 đã bị khai tử. |
| `thiet-ke-goc/` | Bản thiết kế client-server và yêu cầu ban đầu của dự án. | Lưu trữ nền tảng. |
| `prompts-cu/` | Các mẫu prompt kỹ thuật từng dùng để sinh mã và audit tài liệu. | |
| `registries-2026-07/` | Dữ liệu kiểm kê tài liệu cũ và ma trận năng lực giai đoạn 2026-07. | |
| `meta-cu/` | Bản đồ code và hướng dẫn bảo trì metadata cũ. | |
| `ai-devkit-pointers/` | Các tệp con tương thích cũ của `ai-devkit@0.47.0`. | |
| `khao-sat-kiem-thu-va-ci-2026-07-22.md` | Báo cáo khảo sát hạ tầng kiểm thử và CI giai đoạn đầu di trú. | |

---

## 3. Quy tắc Bảo trì Kho Lưu trữ

- Các tệp trong thư mục này được đánh dấu trạng thái đóng băng (`status: frozen` hoặc nằm ngoài phạm vi quét).
- Bộ công cụ `scripts/docs-check.mjs` và `scripts/docs-citations.mjs` tự động bỏ qua toàn bộ thư mục `docs/99-luu-tru/` trong quy trình kiểm tra CI, cho phép lưu giữ nguyên vẹn tọa độ và trích dẫn mã nguồn lịch sử.
