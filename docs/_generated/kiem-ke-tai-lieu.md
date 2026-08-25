---
title: "Kiểm kê và disposition tài liệu LIVA"
updated: 2026-08-25
commit: 7493f091
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
| KEEP | 33 |
| SPLIT | 7 |
| GENERATE | 5 |
| FREEZE | 39 |
| MERGE | 16 |
| **Tổng** | **100** |

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
| `docs/_generated/kiem-ke-tai-lieu.md` | **GENERATE** | B | 4 | — | Báo cáo được sinh từ registry kiểm kê và link Markdown thực tế. |
| `docs/_generated/ma-tran-nang-luc.md` | **GENERATE** | A | 8 | — | Ma trận được sinh từ capability registry. |
| `docs/_meta/ban-do-code-tai-lieu.md` | **GENERATE** | D | 2 | — | Bản đồ code và tài liệu phải được sinh từ frontmatter covers. |
| `docs/_meta/huong-dan-bao-tri.md` | **KEEP** | A | 2 | — | Runbook bảo trì metadata và kiểm tra tài liệu. |
| `docs/_meta/nguon-su-that.md` | **KEEP** | A | 3 | — | Registry quyền sở hữu nội dung đang được docs-check thực thi. |
| `docs/00-san-pham/tam-nhin-jarvis.md` | **KEEP** | A | 7 | — | Canonical owner cho tầm nhìn và nguyên tắc sản phẩm. |
| `docs/01-ban-ve/00-tong-quan-he-thong.md` | **MERGE** | C-foundation | 9 | `docs/03-he-thong-con/runtime-native.md` | Tổng quan as-built cần nhập vào subsystem runtime, tránh cạnh tranh với product vision. |
| `docs/01-ban-ve/01-kien-truc-tong-the.md` | **SPLIT** | C-foundation | 21 | `docs/01-kien-truc/as-built.md`<br>`docs/03-he-thong-con/runtime-native.md` | Đang trộn topology as-built, target design và runtime composition. |
| `docs/01-ban-ve/02-giao-thuc-ipc-va-websocket.md` | **SPLIT** | C-contracts | 21 | `docs/02-hop-dong/tauri-ipc.md`<br>`docs/02-hop-dong/websocket.md` | Hai contract có vòng đời và consumer khác nhau. |
| `docs/01-ban-ve/03-duong-ong-thoai.md` | **FREEZE** | C-voice-complete | 13 | `docs/03-he-thong-con/voice.md`<br>`docs/05-chat-luong/voice-slo.md` | Đã tách canonical voice runtime và SLO; giữ khảo sát chi tiết như snapshot lịch sử. |
| `docs/01-ban-ve/04-he-llm-va-prompt.md` | **SPLIT** | C-agent | 14 | `docs/03-he-thong-con/llm-routing.md`<br>`docs/02-hop-dong/prompt-context.md` | Model routing và prompt contract cần chủ sở hữu độc lập. |
| `docs/01-ban-ve/05-agent-bo-nho-va-tien-hoa.md` | **FREEZE** | C-agent-tools-complete | 7 | `docs/03-he-thong-con/agent-tools.md`<br>`docs/03-he-thong-con/memory.md`<br>`docs/05-chat-luong/action-policy.md` | Đã tách agent/tool runtime, memory và action policy; evolution còn lại là snapshot experimental. |
| `docs/01-ban-ve/06-thi-giac-passive-va-governor.md` | **FREEZE** | C-vision-complete | 13 | `docs/03-he-thong-con/vision.md`<br>`docs/03-he-thong-con/context-broker.md`<br>`docs/05-chat-luong/resource-governor.md` | Đã tách perception đang chạy, proactive experimental và resource policy sang ba nguồn chuẩn độc lập. |
| `docs/01-ban-ve/07-tang-du-lieu-va-bao-mat.md` | **FREEZE** | C-security-complete | 4 | `docs/03-he-thong-con/persistence.md`<br>`docs/05-chat-luong/threat-model.md` | Đã tách data contract và threat model; bản 07 giữ làm snapshot lịch sử. |
| `docs/01-ban-ve/08-frontend-va-vo-tauri.md` | **FREEZE** | C-desktop-complete | 7 | `docs/03-he-thong-con/frontend.md`<br>`docs/03-he-thong-con/desktop-tauri.md` | Đã tách Vue rendering/transport và Tauri security boundary thành hai nguồn chuẩn. |
| `docs/01-ban-ve/09-tich-hop-ngoai.md` | **SPLIT** | C-integrations | 11 | `docs/03-he-thong-con/messaging.md`<br>`docs/03-he-thong-con/os-control.md`<br>`docs/03-he-thong-con/external-integrations.md` | Các adapter có identity, consent và mức trưởng thành khác nhau. |
| `docs/01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md` | **GENERATE** | D | 18 | — | Dependency/file lookup phải sinh từ GitNexus và source tree. |
| `docs/01-kien-truc/adr-001-ma-hoa-du-lieu-ca-nhan-beta.md` | **KEEP** | C-security-complete | 4 | — | Quyết định accepted cho mã hóa transcript/checkpoint, dense recall và key-compatible backup. |
| `docs/01-kien-truc/cognitive-runtime.md` | **KEEP** | A | 7 | — | Canonical owner cho kiến trúc đích Cognitive Runtime. |
| `docs/01-kien-truc/inventory-he-thong.md` | **KEEP** | B | 3 | — | Bản đồ capability tới module, luồng và bằng chứng as-built. |
| `docs/02-van-hanh/01-cau-hinh-va-bien-moi-truong.md` | **KEEP** | C-operations | 21 | `docs/04-van-hanh/cau-hinh.md` | Nội dung vận hành còn cần dùng; chỉ đổi vị trí sau khi sửa citation. |
| `docs/02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md` | **GENERATE** | D | 19 | `docs/04-van-hanh/mo-hinh-ai.md` | Bảng model phải sinh từ models-manifest; phần policy tài nguyên chuyển sang runbook. |
| `docs/02-van-hanh/03-trien-khai-va-runtime.md` | **SPLIT** | C-operations | 15 | `docs/04-van-hanh/trien-khai.md`<br>`docs/04-van-hanh/preflight.md` | Tách quy trình phát hành khỏi chẩn đoán runtime/preflight. |
| `docs/02-van-hanh/04-kiem-thu-va-ci.md` | **KEEP** | C-quality | 18 | `docs/05-chat-luong/kiem-thu-va-ci.md` | Runbook còn dùng; di chuyển sau khi CI links được cập nhật. |
| `docs/02-van-hanh/05-cai-dat-cho-nguoi-dung.md` | **KEEP** | C-operations | 5 | `docs/04-van-hanh/cai-dat.md` | Hướng dẫn beta cần còn truy cập được trong toàn bộ đợt di trú. |
| `docs/02-van-hanh/06-backup-restore-sqlite.md` | **KEEP** | C-operations | 4 | — | Runbook canonical cho online backup, offline restore và rollback SQLite. |
| `docs/02-van-hanh/07-macos-dev.md` | **KEEP** | C-operations | 5 | — | Runbook canonical cho đường phát triển trên macOS (nhánh mac-v2); bản Windows nằm trong 03-trien-khai-va-runtime. |
| `docs/02-van-hanh/release-v1.0.0-smoke-test.md` | **KEEP** | C-operations | 4 | — | Bằng chứng dựng installer v1.0.0 và checklist nghiệm thu trên Windows sạch. |
| `docs/03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md` | **FREEZE** | B | 12 | — | Snapshot khảo sát đã ghi rõ ngày và trạng thái đóng băng. |
| `docs/03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md` | **FREEZE** | B | 22 | — | Đối chiếu tại một thời điểm; capability registry đã thay vai trò trạng thái sống. |
| `docs/03-danh-gia/02-no-ky-thuat-va-rui-ro.md` | **MERGE** | C-quality | 25 | `docs/06-ke-hoach/roadmap.md`<br>`docs/05-chat-luong/risk-register.md` | Việc mở nhập vào roadmap; risk bền nhập vào risk register. |
| `docs/03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md` | **FREEZE** | B | 20 | — | Roadmap cũ đã bị master roadmap thay thế; giữ làm lịch sử F/P. |
| `docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md` | **FREEZE** | B | 5 | — | Đề xuất point-in-time, không phải kiến trúc hay roadmap hiện hành. |
| `docs/03-danh-gia/05-nang-cap-toan-dien.md` | **SPLIT** | C-roadmap | 12 | `docs/06-ke-hoach/roadmap.md`<br>`docs/99-luu-tru/bao-cao-lich-su/nang-cap-toan-dien-2026-07.md` | Việc còn mở nhập master roadmap; bằng chứng U1–U20 đóng băng. |
| `docs/03-danh-gia/06-nhan-tin-ra-ngoai.md` | **SPLIT** | C-integrations | 5 | `docs/03-he-thong-con/messaging.md`<br>`docs/06-ke-hoach/epics/messaging-reliability.md` | Tách as-built messaging khỏi backlog outbox/Telegram. |
| `docs/03-danh-gia/07-wake-word-viec-con-lai.md` | **FREEZE** | C-voice-complete | 3 | `docs/03-he-thong-con/wake-word.md`<br>`docs/05-chat-luong/wake-benchmark.md` | Đã tách architecture và benchmark wake; giữ số đo 27/07/2026 như snapshot lịch sử. |
| `docs/03-he-thong-con/agent-tools.md` | **KEEP** | C-agent-tools-complete | 11 | — | Canonical as-built cho StateGraph, reflex lane, tool selector, MCP executor và skill runtime. |
| `docs/03-he-thong-con/context-broker.md` | **KEEP** | C-vision-complete | 4 | — | Canonical owner cho ranh giới proactive observation, consent và cảnh báo passive keylogger. |
| `docs/03-he-thong-con/desktop-tauri.md` | **KEEP** | C-desktop-complete | 7 | — | Canonical owner cho Tauri boot, window capability, native IPC và session boundary. |
| `docs/03-he-thong-con/frontend.md` | **KEEP** | C-desktop-complete | 6 | — | Canonical as-built cho Vue entries, widget/dashboard và dual transport. |
| `docs/03-he-thong-con/memory.md` | **KEEP** | C-memory-complete | 7 | — | Canonical as-built cho checkpoint, conversational RAG, facts, projection worker và kế hoạch semantic memory. |
| `docs/03-he-thong-con/persistence.md` | **KEEP** | C-security-complete | 18 | — | Canonical as-built cho data root, SQLite schema, migration, durability và vòng đời dữ liệu. |
| `docs/03-he-thong-con/vision.md` | **KEEP** | C-vision-complete | 5 | — | Canonical as-built cho capture, region diff, vision ask và UI screen watch. |
| `docs/03-he-thong-con/voice.md` | **KEEP** | C-voice-complete | 9 | — | Canonical as-built cho capture, transport, STT, TTS và cancellation. |
| `docs/03-he-thong-con/wake-word.md` | **KEEP** | C-voice-complete | 6 | — | Canonical as-built và giới hạn sản phẩm của wake word. |
| `docs/04-quy-trinh/KNOWLEDGE_BASE.md` | **MERGE** | C-contribution | 3 | `docs/07-dong-gop/tri-thuc-va-vault.md` | Con trỏ Vault nên nằm trong hướng dẫn đóng góp thay vì một knowledge base thứ hai. |
| `docs/04-quy-trinh/NEW_feature_template.md` | **KEEP** | C-contribution | 1 | `docs/07-dong-gop/mau-de-xuat-tinh-nang.md` | Template còn hữu ích; đổi vị trí sau khi cập nhật link. |
| `docs/04-quy-trinh/prompts/_meta/optimize-architecture-review.md` | **MERGE** | C-agent-workflow | 1 | `.agents/skills/dev-design/SKILL.md` | Workflow agent phải do skill quản lý, không duy trì prompt song song. |
| `docs/04-quy-trinh/prompts/_meta/optimize-code-review.md` | **MERGE** | C-agent-workflow | 1 | `.agents/skills/dev-review/SKILL.md` | Workflow review đã có skill canonical. |
| `docs/04-quy-trinh/prompts/_meta/optimize-readme.md` | **MERGE** | C-agent-workflow | 1 | `docs/07-dong-gop/quy-hoach-tai-lieu.md` | Quy tắc cập nhật README thuộc governance docs, không cần meta prompt riêng. |
| `docs/04-quy-trinh/prompts/_meta/optimize-spring-cleaning.md` | **MERGE** | C-agent-workflow | 1 | `.agents/skills/liva-technical-debt-triage/SKILL.md` | Cleanup phải dùng debt triage và impact analysis hiện hành. |
| `docs/04-quy-trinh/prompts/architecture-review.md` | **MERGE** | C-agent-workflow | 1 | `.agents/skills/dev-design/SKILL.md` | Nội dung còn giá trị nhập vào skill review kiến trúc. |
| `docs/04-quy-trinh/prompts/code-review-prompt.md` | **MERGE** | C-agent-workflow | 1 | `.agents/skills/dev-review/SKILL.md` | Code review được điều phối bằng skill và GitNexus. |
| `docs/04-quy-trinh/prompts/openspace-g0-mcp-client-prompt.md` | **FREEZE** | B | 1 | — | Prompt triển khai point-in-time cho đề xuất OpenSpace. |
| `docs/04-quy-trinh/prompts/readme-generation-prompt.md` | **MERGE** | C-agent-workflow | 2 | `docs/07-dong-gop/quy-hoach-tai-lieu.md` | Quy tắc README phải nằm cùng governance và source ownership. |
| `docs/04-quy-trinh/prompts/spring-cleaning-prompt.md` | **MERGE** | C-agent-workflow | 1 | `.agents/skills/liva-technical-debt-triage/SKILL.md` | Prompt dọn dẹp cũ được thay bằng quy trình debt triage có acceptance test. |
| `docs/05-chat-luong/action-policy.md` | **KEEP** | C-agent-tools-complete | 8 | — | Canonical owner cho guardrail hiện hành và acceptance contract của side effect. |
| `docs/05-chat-luong/beta-thuc-dia.md` | **KEEP** | C-quality | 1 | — | Kênh thu nhận thủ công và bằng chứng thực địa thay cho telemetry trong beta. |
| `docs/05-chat-luong/resource-governor.md` | **KEEP** | C-vision-complete | 6 | — | Canonical owner cho ngưỡng CPU/GPU/fullscreen và chính sách nhường tài nguyên. |
| `docs/05-chat-luong/threat-model.md` | **KEEP** | C-security-complete | 23 | — | Canonical owner cho trust boundaries, crypto/keystore coverage và kế hoạch hardening. |
| `docs/05-chat-luong/voice-slo.md` | **KEEP** | C-voice-complete | 6 | — | Canonical owner cho runtime thresholds, SLO và acceptance voice. |
| `docs/05-chat-luong/wake-benchmark.md` | **KEEP** | C-voice-complete | 5 | — | Canonical owner cho corpus và acceptance benchmark wake. |
| `docs/05-chat-luong/wer-fleurs-vi.md` | **KEEP** | C-voice-complete | 3 | — | Bằng chứng WER tái lập qua đúng đường ống STT sản xuất trên FLEURS vi_vn. |
| `docs/06-ke-hoach/roadmap.md` | **KEEP** | A | 18 | — | Canonical owner duy nhất cho milestone và việc còn làm. |
| `docs/07-dong-gop/quy-hoach-tai-lieu.md` | **KEEP** | A | 5 | — | Canonical owner cho chiến lược di trú tài liệu. |
| `docs/99-luu-tru/bao-cao-lich-su/architecture-review/architecture-review-report-2026-05-31-20-12.md` | **FREEZE** | archive | 0 | — | Báo cáo review lịch sử. |
| `docs/99-luu-tru/bao-cao-lich-su/architecture-review/architecture-review-report-2026-05-31.md` | **FREEZE** | archive | 0 | — | Báo cáo review lịch sử. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_Acceptance_Report_2026.md` | **FREEZE** | archive | 1 | — | Báo cáo acceptance lịch sử. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_Architecture_Audit_2026.md` | **FREEZE** | archive | 0 | — | Audit kiến trúc lịch sử. |
| `docs/99-luu-tru/bao-cao-lich-su/LIVA_OSS_Research_2026-07.md` | **FREEZE** | archive | 1 | — | Nghiên cứu OSS point-in-time. |
| `docs/99-luu-tru/bao-cao-lich-su/liva_test_report.md` | **FREEZE** | archive | 0 | — | Kết quả test point-in-time. |
| `docs/99-luu-tru/bao-cao-lich-su/spring-cleaning/spring-cleaning-report-2026-05-31.md` | **FREEZE** | archive | 0 | — | Báo cáo cleanup lịch sử. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/LIVA_NATIVE_MIGRATION_PLAN.md` | **FREEZE** | archive | 0 | — | Kế hoạch migration Rust đã hoàn thành. |
| `docs/99-luu-tru/ke-hoach-da-hoan-thanh/parakeet_vi_integration_plan.md` | **FREEZE** | archive | 0 | — | Kế hoạch tích hợp model đã hoàn thành hoặc bị thay thế. |
| `docs/99-luu-tru/khao-sat-kiem-thu-va-ci-2026-07-22.md` | **FREEZE** | A | 1 | `docs/02-van-hanh/04-kiem-thu-va-ci.md` | Snapshot khảo sát cũ được tách khỏi tài liệu CI living sau khi nhiều nhận định đã được giải quyết. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/01_System_Overview.md` | **FREEZE** | archive | 0 | — | Kiến trúc Node.js đã nghỉ hưu. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/02_Memory_Subsystem.md` | **FREEZE** | archive | 0 | — | Thiết kế memory Node.js lịch sử. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/03_Agent_Control_Flow.md` | **FREEZE** | archive | 0 | — | Agent control flow Node.js lịch sử. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/04_Evolution_Singularity.md` | **FREEZE** | archive | 0 | — | Thiết kế evolution Node.js lịch sử. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/05_Security_Guardrails.md` | **FREEZE** | archive | 0 | — | Guardrail thuộc kiến trúc đã nghỉ hưu; chỉ dùng làm tham khảo lịch sử. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/06_Hardware_UX_Optimization.md` | **FREEZE** | archive | 0 | — | Tối ưu phần cứng/UX của stack cũ. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/07_Hybrid_Cloud_Architecture.md` | **FREEZE** | archive | 0 | — | Kiến trúc hybrid cloud của stack cũ. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/AI_CONTEXT.md` | **FREEZE** | archive | 0 | — | Agent context cũ đã được AGENTS.md thay thế. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/Architectural_Teardown_Proposal.md` | **FREEZE** | archive | 0 | — | Đề xuất teardown point-in-time. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/codebase_architecture.md` | **FREEZE** | archive | 0 | — | Codebase architecture Node.js lịch sử. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/personality_architecture_report.md` | **FREEZE** | archive | 0 | — | Báo cáo personality point-in-time. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/PROJECT.md` | **FREEZE** | archive | 0 | — | Mô tả dự án thuộc stack Node.js đã nghỉ hưu. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/skills_development_guide.md` | **FREEZE** | archive | 0 | — | Skill guide cũ đã được skill governance hiện hành thay thế. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/STARTUP_GUIDE.md` | **FREEZE** | archive | 1 | — | Hướng dẫn khởi động stack cũ, không được dùng vận hành. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/streaming_optimization.md` | **FREEZE** | archive | 0 | — | Tối ưu streaming của stack cũ. |
| `docs/99-luu-tru/kien-truc-nodejs-v29/TEST_READY.md` | **FREEZE** | archive | 0 | — | Checklist test của stack cũ. |
| `docs/99-luu-tru/README.md` | **KEEP** | archive | 3 | — | Mục lục và cảnh báo cho khu vực lịch sử. |
| `docs/99-luu-tru/thiet-ke-goc/LIVA_CLIENT_SERVER_DESIGN.md` | **FREEZE** | archive | 1 | — | Thiết kế client/server gốc. |
| `docs/99-luu-tru/thiet-ke-goc/ORIGINAL_REQUEST.md` | **FREEZE** | archive | 0 | — | Yêu cầu gốc được bảo toàn nguyên trạng. |
| `docs/ai/design/README.md` | **MERGE** | A | 0 | `docs/01-kien-truc/cognitive-runtime.md` | Pointer tương thích cho ai-devkit; nội dung thiết kế vẫn do tài liệu kiến trúc chuẩn sở hữu. |
| `docs/ai/implementation/README.md` | **MERGE** | A | 0 | `docs/_generated/ma-tran-nang-luc.md` | Pointer tương thích, không phải tài liệu triển khai song song. |
| `docs/ai/planning/README.md` | **MERGE** | A | 0 | `docs/06-ke-hoach/roadmap.md` | Pointer tương thích cho ai-devkit; roadmap là canonical owner. |
| `docs/ai/requirements/README.md` | **MERGE** | A | 0 | `docs/00-san-pham/tam-nhin-jarvis.md` | Pointer tương thích cho ai-devkit; yêu cầu sản phẩm không được chép lại. |
| `docs/ai/testing/README.md` | **MERGE** | A | 0 | `docs/02-van-hanh/04-kiem-thu-va-ci.md` | Pointer tương thích cho ai-devkit; test contract nằm ở tài liệu CI chuẩn. |
| `docs/README.md` | **KEEP** | A | 49 | — | Mục lục chuyển tiếp và điểm vào duy nhất của bộ tài liệu. |

## File chưa được phép di chuyển ngay

Các file sau có disposition `SPLIT/MERGE` và đang có link Markdown trỏ vào. Phải cập nhật
inbound link trước, sau đó giữ redirect ít nhất một chu kỳ release.

- `docs/03-danh-gia/02-no-ky-thuat-va-rui-ro.md`: 25 file trỏ vào.
- `docs/01-ban-ve/01-kien-truc-tong-the.md`: 21 file trỏ vào.
- `docs/01-ban-ve/02-giao-thuc-ipc-va-websocket.md`: 21 file trỏ vào.
- `docs/02-van-hanh/03-trien-khai-va-runtime.md`: 15 file trỏ vào.
- `docs/01-ban-ve/04-he-llm-va-prompt.md`: 14 file trỏ vào.
- `docs/03-danh-gia/05-nang-cap-toan-dien.md`: 12 file trỏ vào.
- `docs/01-ban-ve/09-tich-hop-ngoai.md`: 11 file trỏ vào.
- `docs/01-ban-ve/00-tong-quan-he-thong.md`: 9 file trỏ vào.
- `docs/03-danh-gia/06-nhan-tin-ra-ngoai.md`: 5 file trỏ vào.
- `docs/04-quy-trinh/KNOWLEDGE_BASE.md`: 3 file trỏ vào.
- `docs/04-quy-trinh/prompts/readme-generation-prompt.md`: 2 file trỏ vào.
- `docs/04-quy-trinh/prompts/_meta/optimize-architecture-review.md`: 1 file trỏ vào.
- `docs/04-quy-trinh/prompts/_meta/optimize-code-review.md`: 1 file trỏ vào.
- `docs/04-quy-trinh/prompts/_meta/optimize-readme.md`: 1 file trỏ vào.
- `docs/04-quy-trinh/prompts/_meta/optimize-spring-cleaning.md`: 1 file trỏ vào.
- `docs/04-quy-trinh/prompts/architecture-review.md`: 1 file trỏ vào.
- `docs/04-quy-trinh/prompts/code-review-prompt.md`: 1 file trỏ vào.
- `docs/04-quy-trinh/prompts/spring-cleaning-prompt.md`: 1 file trỏ vào.

## Gate

- Mọi `docs/**/*.md` phải có đúng một disposition.
- `SPLIT` và `MERGE` phải có ít nhất một target.
- Registry không được trỏ tới file nguồn đã biến mất.
- Chạy `npm run docs:inventory:check` để phát hiện thiếu file hoặc generated drift.
