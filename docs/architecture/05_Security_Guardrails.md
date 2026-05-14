# 05. Hệ thống Cảnh giới & Bảo mật (Security & Guardrails)

> Phiên bản: v20 (2026-05-11) — LIVA-UHM v2

Dự án LIVA áp dụng triết lý **Zero-Trust** và **Shift-Left** trong bảo mật, đảm bảo AI không vô tình thực hiện các tác vụ nguy hiểm trên máy cá nhân.

---

## 1. ZMAS Guard (Zero-Trust Model Application Security)

`ZMAS_Guard.ts` là tấm khiên đa lớp chặn trước khi kết quả của LLM được chuyển về hệ thống:
1. **Lọc PII (Personally Identifiable Information)**: Xóa thông tin thẻ tín dụng, SSN.
2. **Ngăn chặn Injection**: Phát hiện và chặn LLM trả về các payload thực thi shell độc hại (`rm -rf`, mã độc Bash/PowerShell).
3. **Quét URL**: Từ chối các liên kết đến IP lạ không nằm trong danh sách an toàn.
4. **Credential Filter**: Tự động che giấu (mask) các chuỗi có vẻ là API Key hay Token.

---

## 2. HITL Guard (Human-in-the-Loop)

Áp dụng cho các Skill mang tính phá hủy (vd: DeleteLocalFile, ExecuteCommand, SendEmail).
- LLM không được phép chạy trực tiếp. Thay vào đó, hệ thống tạm dừng FSM (`AgentLoop`).
- Gửi thông báo đến UI (Tauri) hoặc thiết bị di động (Telegram/Zalo Bot).
- Người dùng có 60 giây để Bấm "Duyệt" (Approve) hoặc "Từ chối" (Reject).
- Sau 60 giây nếu không phản hồi, tác vụ tự động bị Hủy (Timeout).

---

## 3. VRAM Guard (Dual-Layer Memory Protection)

Ngăn chặn OOM crash khi LLM đang stream và consolidation cố chạy song song:

1. **isRunning flag**: `ConsolidationCron` tự khóa chính nó, không cho 2 consolidation chạy đồng thời.
2. **AgentLoop State Gate**: `agentLoopStateGetter() === 'IDLE'` — consolidation CHỈ fire khi LLM hoàn toàn rảnh. Nếu AgentLoop đang `THINKING` hoặc `ACTING`, mọi trigger bị defer.
3. **15s Debounce**: Tránh event loop flooding từ các passive signals liên tục.

---

## 4. Quản lý Mật mã Hệ thống (DevSecOps Vault)

LIVA bảo vệ các File môi trường (`.env`):
- `openclaw-gateway/.env` liên tục được giám sát bởi tiến trình Host Tauri.
- Bất kỳ API Key nào nhạy cảm (`ZALO_OA_ACCESS_TOKEN`, `AI_API_KEY`) sẽ tự động bị rút khỏi `.env`.
- Mã hóa bằng chuẩn AES-256-GCM qua thư viện `node:crypto` và lưu vào `liva_vault.json`.
- Điều này đảm bảo ngay cả khi một script độc hại đánh cắp file `.env`, hacker cũng không lấy được mật khẩu thật.

---

## 5. Bảo vệ Giác quan (Sensory Anti-Injection)

LLM nhận đầu vào từ Clipboard hoặc thông tin Cửa sổ ứng dụng đang chạy (`SensoryManager`).
- **Nguy cơ**: Kẻ tấn công có thể chép một đoạn văn bản có chứa lệnh thao túng Prompt AI vào Clipboard. Khi AI đọc clipboard, AI sẽ bị hack (Prompt Injection).
- **Phòng vệ**: `sanitizeSensoryData()` cắt input tối đa 2000 ký tự, strip toàn bộ HTML tags, escape các ký tự điều khiển (Control characters) trước khi lắp vào prompt, ngăn ngừa hoàn toàn các chuỗi thoát bối cảnh (Context Escape).

---

## 6. File System Guardrails (RPAGuardrails)

Chặn AI không được thao tác trên các vùng nhạy cảm của Hệ điều hành.
- **Banned Directories**: `C:\Windows`, `C:\Program Files`, `C:\ProgramData`, `/etc`, `/var`.
- **Boot Files Protection**: Chặn sửa, xóa `ntldr`, `bootmgr`, `pagefile.sys`.
- Nếu AI cố tình sửa các tệp này, Gateway lập tức báo lỗi "Permission Denied by RPAGuardrails".
