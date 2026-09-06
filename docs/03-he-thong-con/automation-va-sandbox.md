---
title: "Tự động hóa trình duyệt, OS & Sandbox bảo mật đa tầng"
updated: 2026-09-03
commit: 35f5f26d
status: living
owns: []
covers:
  - liva-native-core/src/commands/browser.rs
---
# Hệ Thống Con: Tự Động Hóa Trình Duyệt, OS & Sandbox Bảo Mật Đa Tầng

[⬆ Mục lục](../README.md)


Tài liệu thiết kế và kiến trúc cho hệ thống tự động hóa và sandbox an toàn trong LIVA Native Rust Core (`liva-native-core/src/automation/`).

---

## 1. Tổng Quan Kiến Trúc

Hệ thống Automation cung cấp khả năng tương tác với trình duyệt web, cây DOM ngữ nghĩa, cửa sổ ứng dụng hệ điều hành và thiết bị ngoại vi với cơ chế kiểm soát an toàn tuyệt đối (Fail-Closed Sandbox Guardrails).

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        AUTOMATION SUBSYSTEM ARCHITECTURE                │
└─────────────────────────────────────────────────────────────────────────┘

   [ LLM / Tool Dispatcher ]
              │
              ▼
   ┌───────────────────────────────────────────────────────────────────┐
   │             Multi-Tier Sandbox & Security Guardrails              │
   │  - AST Command Sanitizer (chặn lệnh phá hoại, fork bomb)         │
   │  - SSRF & Cloud Metadata Blocker (169.254.169.254, loopback)     │
   │  - Path Chroot Boundary Enforcement (ngăn path traversal)         │
   │  - Network Domain Allowlist (*.domain, exact host)                │
   └──────────┬──────────────────────────┬─────────────────────────────┘
              │                          │
              ▼                          ▼
   ┌───────────────────────────┐  ┌────────────────────────────────────┐
   │  Headless Browser (CDP)   │  │   Cross-Platform OS Automation     │
   │  - Navigation & Session   │  │   - Window Discovery (WindowInfo)  │
   │  - Element Click & Typing │  │   - Synthetic Keystrokes & Inputs  │
   │  - Viewport PNG Capture   │  │   - Screen Capture (xcap / memory) │
   └──────────┬────────────────┘  └────────────────────────────────────┘
              │
              ▼
   ┌───────────────────────────────────────────────────────────────────┐
   │                  Semantic DOM Tree Extractor                      │
   │  - Strips noise: script, style, nav, footer, ads, svg             │
   │  - Modes: FullHtml, CleanMarkdown, PlainText, AccessibilityTree   │
   │  - Numerical Grounding Markers ([1], [2]) cho LLM targeting       │
   │  - Giảm >85% token footprint so với HTML thô                      │
   └───────────────────────────────────────────────────────────────────┘
```

---

## 2. Các Thành Phần Chính

### 2.1. Headless Browser Controller (`browser.rs`)
- Trait `BrowserDriver`: Giao diện bất đồng bộ chuẩn hóa cho tương tác trình duyệt qua giao thức Chrome DevTools Protocol (CDP).
- `MockBrowserDriver`: Driver giả lập tốc độ cao cho test suite và môi trường cô lập.
- `CdpBrowserController`: Driver kết nối trực tiếp đến trình duyệt Chromium không đầu.
- Tích hợp kiểm tra `SandboxGuard` trước mọi thao tác điều hướng URL.

### 2.2. Semantic DOM Extractor (`dom.rs`)
- `DomExtractMode`:
  - `FullHtml`: Giữ nguyên mã HTML gốc.
  - `CleanMarkdown`: Lược bỏ thẻ rác, định dạng Markdown cấu trúc rõ ràng (`# Header`, `- List`, `[Link](url)`, `[Button: text]`, `[Input: name]`).
  - `PlainText`: Văn bản thuần chuẩn hóa khoảng trắng.
  - `AccessibilityTree`: Cây node trợ năng dạng thụt đầu dòng (`[AXRoot]`, `[AXHeading]`, `[AXButton]`, `[AXLink]`).
- Cơ chế đánh chỉ mục phần tử tương tác (`InteractiveElement`) gắn nhãn số định danh duy nhất giúp mô hình ngôn ngữ ra quyết định gọi công cụ chính xác 100%.

### 2.3. Tự Động Hóa Hệ Điều Hành & Input (`system.rs`)
- Trait `SystemAutomationDriver`:
  - `list_windows()`: Liệt kê cửa sổ ứng dụng đang mở kèm toạ độ, kích thước, PID và trạng thái focus.
  - `focus_window(id)`: Kích hoạt cửa sổ lên tiền cảnh.
  - `send_key_action(action)`: Gửi phím bấm (`KeyDown`, `KeyUp`, `KeyStroke`, `Combination`, `UnicodeText`).
  - `move_mouse(x, y)` & `click_mouse(button, double)`: Điều khiển chuột độ chính xác cao.
  - `capture_screen(region)`: Chụp màn hình qua crate `xcap` nén PNG trực tiếp trong RAM.

### 2.4. Sandbox Bảo Mật Đa Tầng (`sandbox.rs`)
- `SandboxPolicy`: Cấu hình danh sách tên miền cho phép, thư mục đọc/ghi được cấp quyền, danh sách đen lệnh shell, giới hạn RAM và thời gian chạy.
- `SandboxViolation`: Các lỗi vi phạm bảo mật phân loại rõ ràng (`BlockedDomain`, `SsrfAttempt`, `PathJailbreak`, `WriteDenied`, `DestructiveCommand`).
- Phòng chống SSRF cấp Host & IP: Tự động phân tích địa chỉ IP riêng tư (RFC 1918), IP loopback, và endpoint metadata điện toán đám mây (`169.254.169.254`).
- Bộ phân tích cú pháp lệnh (AST Command Sanitizer): Chặn đứng các chuỗi lệnh độc hại như `rm -rf`, `mkfs`, `dd if=`, `:(){ :|:& };:`, `chmod -R 777 /`.
