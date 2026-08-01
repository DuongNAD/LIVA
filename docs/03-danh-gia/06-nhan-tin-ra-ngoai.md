---
title: "Nhắn tin ra ngoài — hiện trạng, việc còn lại, và các bẫy đã trả giá"
updated: 2026-08-01
commit: 3688b5f
stale-ok: 98efc55
status: living
owns:
  - nhan-tin-ra-ngoai-telegram-messenger
covers:
  - liva-native-core/src/messaging/mod.rs
  - liva-native-core/src/messaging/contacts.rs
  - liva-native-core/src/messaging/outbox.rs
  - liva-native-core/src/integrations/messenger.rs
  - liva-native-core/src/commands/messaging.rs
  - scripts/messenger-chrome.ps1
---
# Nhắn tin ra ngoài

Tài liệu này viết cho **phiên làm việc sau**. Nó ghi ba thứ: cái gì đã chạy thật, việc gì còn lại, và những cái bẫy đã ngốn nhiều giờ để tìm ra — mục cuối là phần đáng đọc nhất, vì mỗi mục trong đó từng làm chẩn đoán đi sai hướng.

Hình dạng của tính năng: **danh bạ → soạn bản nháp → người dùng xác nhận → gửi**. Chặng thứ ba không bỏ qua được, và đó là chủ ý — xem [§3.3](#33-vì-sao-cổng-xác-nhận-không-được-bỏ).

---

## 1. Hiện trạng

| Mảnh | Trạng thái | Ghi chú |
|---|---|---|
| Sổ danh bạ (`contacts`, schema v5; DB hiện ở schema v7) | **[OK]** | Bỏ dấu để tra, `(lookup_key, platform)` là khoá duy nhất |
| Hộp chờ xác nhận | **[OK]** | SQLite mã hóa; sống qua restart; dùng-một-lần, hết hạn 300 s, trần 32 |
| `route_intent` hiểu "nhắn cho X bảo Y" | **[OK]** | Đặt trước mọi nhánh khác trong `agent/graph.rs` |
| Thẻ xác nhận trong widget | **[OK]** | Hiện cả tên lẫn địa chỉ đích |
| Gửi **Messenger** | **[OK]** | Đã gửi thật 27/07/2026 lúc 01:53, người nhận trả lời |
| Gửi **Telegram** | **[MỘT PHẦN]** | Code xong, **chưa chạy lần nào** — máy chưa có `TELEGRAM_BOT_TOKEN` |
| Zalo và các web khác | **[THIẾU]** | Nút thắt chung đã gỡ; còn thiếu lớp hồ sơ trang |

### 1.1 Bằng chứng cho "Messenger đã gửi thật"

Không lấy từ cờ `sent: true` của chính mã nguồn — cờ đó chỉ nói "ô soạn đã rỗng". Bằng chứng đọc từ DOM của trang Messenger sau khi gửi:

```
01:53 | Test tin nhắn tự động từ trợ lý LIVA, bạn bỏ qua giúp mình nhé.
     | Nhập, Tin nhắn do Bạn gửi lúc 01:53: …
     | Đã gửi tin nhắn
```

Nhãn trợ năng do chính Messenger sinh ra ghi "Tin nhắn do **Bạn gửi**", và người nhận đã trả lời bằng sticker. Toàn chặng xác nhận → gửi mất **1 486 ms**.

---

## 2. Việc còn lại, theo thứ tự

### M1. Chặn danh bạ biến mất theo thư mục chạy — **đã xong**

`data_dir()` nay neo vào `LIVA_HOME` hoặc `%LOCALAPPDATA%\com.liva.cognitive-os`,
không còn theo cwd; `LIVA_DB_PATH` vẫn là override cao nhất. Boot còn dò và cảnh báo
các database rơi rớt từ layout cũ thay vì âm thầm chọn một bản.

### M2. Bản nháp phải sống sót qua khởi động lại lõi — **đã xong 31/07/2026**

Schema v6 thêm `message_outbox`; nội dung tin nằm ở `text_ciphertext` dưới khóa dữ
liệu hiện hành. `stage/take/cancel/pending` dùng transaction `IMMEDIATE`; `seq`
AUTOINCREMENT giữ đúng thứ tự qua restart. `take` phân loại `Expired`, `Missing`,
`Locked` và chỉ xóa hàng sau khi giải mã được. Test disk fixture đóng pool, mở lại,
đọc ciphertext, consume một lần và từ chối lần hai.

### M3. Tách lớp hồ sơ trang, rồi thêm Zalo Web

Nút thắt vừa gỡ (xem [§3.1](#31-inputdispatchkeyevent-bị-vứt-khi-cửa-sổ-không-phải-foreground)) **không thuộc về Messenger** — nó thuộc về cách bơm phím vào trình duyệt, nên dùng lại được cho mọi trang. Phần khác nhau giữa các trang chỉ có ba thứ:

1. mẫu URL hội thoại (`messenger.com/t/{handle}`),
2. bộ chọn ô soạn,
3. cách gửi (Enter, hay bấm nút nào).

Đó là một bảng dữ liệu, không phải viết lại. `contacts.platform` đã có sẵn khái niệm nền từ đầu nên **không phải migrate schema**; thêm một biến thể vào `contacts::Platform` là đủ.

### M4. Chạy thử Telegram một lần

Code đã xong và **chờ kết quả thật** thay vì fire-and-forget như `telegram:send_text` cũ. Cần: `TELEGRAM_BOT_TOKEN` trong `.env`, và người nhận phải `/start` bot một lần — Telegram không cho bot nhắn trước cho người lạ.

Ghi chú: `commands/integrations.rs::telegram:send_text` cũ **vẫn còn và vẫn fire-and-forget**. Hai đường cùng tồn tại là có chủ ý và tạm thời; đừng tưởng đó là trùng lặp vô tình.

### M5. Dọn nợ nhỏ

- `scripts/generate_hey_liva_model.py` giờ là rác: bộ wake word đã chuyển sang phân đoạn VAD + xác minh bằng STT ở lõi.
- Sổ danh bạ chưa có màn hình quản lý trong Dashboard; hiện phải thêm qua lệnh WebSocket `contacts:upsert`.

---

## 3. Bẫy đã trả giá — đọc trước khi sửa

### 3.1 `Input.dispatchKeyEvent` bị vứt khi cửa sổ không phải foreground

**Đây là nút thắt lớn nhất, và nó tốn nhiều giờ.**

Chrome **nhận** lệnh, trả về không lỗi, rồi **im lặng không chuyển tới trang** nếu cửa sổ trình duyệt không phải foreground ở tầng Windows. `Page.bringToFront` **không đủ** — nó kích hoạt tab bên trong cửa sổ, không đụng thứ tự cửa sổ của hệ điều hành. Cửa sổ vẫn hiện, `IsIconic` = false, mà phím vẫn bị bỏ.

Cái làm lỗi này khó đọc: **`Input.insertText` KHÔNG dính**, vì nó đi đường commit của IME chứ không phải đường phím. Nên chữ vào ô soạn ngon lành, mọi thứ trông như đang chạy, chỉ mỗi Enter im lặng không làm gì.

Cách đo dứt điểm — cài một bộ ghi vào trang rồi hỏi thẳng "phím có tới không":

```js
window.__ghi = []
document.addEventListener('keydown', e => window.__ghi.push(e.key + (e.isTrusted ? '(trusted)' : '(js)')), true)
// rồi bắn Input.dispatchKeyEvent và đọc lại window.__ghi
```

Trước khi ép foreground: mảng rỗng. Sau khi ép: có `F13(trusted)`.

Cách sửa nằm ở `integrations/messenger.rs`: hỏi PID trình duyệt qua `SystemInfo.getProcessInfo` (**phải nối vào WebSocket tầng browser lấy từ `/json/version`** — session của một tab không gọi được lệnh này), duyệt cửa sổ theo PID, rồi `ShowWindow` + `SetForegroundWindow`.

Đánh đổi phải nói trước với người dùng: **cửa sổ Chrome nhảy lên foreground trong khoảnh khắc gửi**. Không tránh được khi lái một trình duyệt thật.

### 3.2 Mỗi lần gửi hỏng để lại chữ trong ô soạn

Trước khi có bản vá, mỗi lần Enter không ăn thì câu **nằm lại** trong ô soạn, và lần sau `insertText` **nối vào đuôi**. Đã quan sát ô soạn tích tụ ba bản của cùng một câu.

Đây là kiểu hỏng tệ nhất trong cả hệ thống này: **nội dung gửi đi khác nội dung người dùng đã duyệt trên thẻ xác nhận**. Tệ hơn cả không gửi được.

Nay `send()` tự kiểm khả năng bấm phím **trước khi gõ**, và nếu không gửi được thì **không chạm vào ô soạn**. Dọn chữ sót dùng `execCommand('selectAll')` + `execCommand('delete')` qua `Runtime.evaluate` — không dùng Ctrl+A, vì đường phím chính là thứ không chạy được.

### 3.3 Vì sao cổng xác nhận không được bỏ

Gửi tin là **không hoàn tác được**, và cái sai không nằm ở máy mà nằm ở người khác đã đọc nó. Đầu vào thì thường là STT tiếng Việt qua model 2B: nghe "Hiến" thành "Hiền", "ngủ đi" thành "ngu đi" — cả hai đều là câu hợp lệ, máy không có tín hiệu nào để tự biết mình sai. Người đọc lại một dòng chữ thì biết ngay.

Bất biến được **kiểu dữ liệu** giữ chứ không trông vào kỷ luật người viết: `messaging::send` chỉ nhận `outbox::Draft`, mà `Draft` chỉ lấy được từ `outbox::take` — thứ chỉ thành công một lần và chỉ khi chưa hết hạn. Có test khoá cả ba tính chất; `commands/messaging.rs` còn có một test khẳng định **không tồn tại lệnh gửi một nhịp**.

Thẻ xác nhận hiện **cả tên lẫn địa chỉ đích**, vì tên đúng mà số sai vẫn là gửi nhầm người — và người dùng là lớp duy nhất bắt được chuyện đó.

### 3.4 Chrome 136+ từ chối debug port trên profile mặc định

Không "gắn vào Chrome bạn đang dùng" được nữa; bắt buộc `--user-data-dir` riêng, và người dùng **tự đăng nhập một lần** trong profile đó. Dùng `scripts/messenger-chrome.ps1`.

LIVA **không bao giờ chạm vào mật khẩu** — ngoài lý do an toàn (WebSocket 8002 chưa có xác thực), nó còn đúng hơn về kỹ thuật: đăng nhập tự động gần như chắc chắn kích hoạt checkpoint 2FA, còn cookie sẵn thì không.

Hệ quả cần biết: **không tự nhắn cho chính mình bằng id tài khoản của mình** — `messenger.com/t/<id-của-bạn>` chỉ hiện danh sách chat, không mở hội thoại nào.

### 3.5 Ba lỗi ĐO của chính người viết tài liệu này

Ghi lại vì chúng đều dẫn tới kết luận sai, và đều là loại rất dễ lặp lại:

1. **Tìm kết nối tới vite theo PID của `liva-desktop`** → kết luận nhầm "app không nạp từ dev server". Webview chạy ở tiến trình `msedgewebview2.exe` **riêng**, nên lọc theo PID của app là bỏ sót.
2. **So kích thước hai binary mà cả hai đều build sau khi `dist` đổi** → "chênh 0 byte" rồi kết luận "asset không được nhúng". Không có mốc trước thì không có phép so.
3. **Lọc nút gửi theo nhãn chứa chữ "gửi" rồi cắt 8 kết quả đầu** → kết luận "DOM không có nút gửi". Nhãn của hàng chục dòng tin nhắn cũ cũng chứa chữ đó ("Tin nhắn do Bạn gửi lúc…"), nên nút thật bị đẩy ra ngoài 8 kết quả. Nhãn thật của nó là **"Nhấn Enter để gửi"**.

Mẫu chung: khi một nửa đường đi hoạt động còn nửa kia im lặng, **đừng suy đoán về nửa im lặng — đo nó**.

### 3.6 `created_at` tính bằng GIÂY — mọi thứ xếp theo nó đều hoà, và tie-break ngẫu nhiên thì im lặng

`Draft.created_at` có độ phân giải **giây**. Bản nháp tạo trong cùng một giây vì thế **bằng nhau ở khoá sắp xếp chính** — và cả hai chỗ dựa vào thứ tự đều từng sai vì lý do đó, cách nhau vài ngày:

| Chỗ | Hỏng thế nào | Vá bằng |
|---|---|---|
| `stage` — chọn bản bị đuổi khi hộp đầy | `min_by_key` trên `created_at` từng đuổi **bản vừa tạo** thay vì bản cũ nhất | SQLite `seq INTEGER PRIMARY KEY AUTOINCREMENT` |
| `pending` — danh sách trả cho UI và cho LLM | tie-break bằng `draft_id` từng làm thứ tự ngẫu nhiên | `ORDER BY created_at DESC, seq DESC` |

Vế thứ hai đáng vá chứ không phải chuyện thẩm mỹ: `message:pending` trả **đúng danh sách này** làm thẻ xác nhận gửi tin, và cả module tồn tại để chặn gửi nhầm người ([§3.3](#33-vì-sao-cổng-xác-nhận-không-được-bỏ)). Một danh sách tự nhận là mới-nhất-trước mà thật ra xếp ngẫu nhiên là đúng cách để người dùng bấm xác nhận nhầm bản nháp — *"nhắn cho Hiến, và nhắn cho Nam luôn"* là đủ để dính.

**Cách tất định hoá một test cho lỗi ngẫu nhiên** (dùng ở cả hai test hồi quy trong `messaging/outbox.rs`): tạo `MAX_PENDING` bản nháp trong cùng một giây rồi assert thứ tự nghịch đảo đúng khít. Với tie-break ngẫu nhiên, xác suất cả loạt tình cờ ra đúng là 1/32! ≈ 0 — nên test không cần seed, không cần lặp, và **không nhấp nháy**.

Bài học chung, áp được ngoài module này: **một mốc thời gian có độ phân giải thô là một khoá sắp xếp hoà thường xuyên.** Khi nó hoà, thứ tự do khoá phụ quyết định — và nếu khoá phụ ngẫu nhiên thì lỗi không bao giờ nổ, chỉ âm thầm sai một phần thời gian.

### 3.7 Bẫy môi trường

- **`npm run dev` bật watcher `tauri dev`**, và nó build lại + khởi động lại lõi liên tục. Trong lúc thử tính năng cần trạng thái ổn định, hãy giết watcher rồi chạy thẳng `target/debug/liva-desktop.exe`.
- **File `.ps1` chứa tiếng Việt phải có BOM UTF-8.** PowerShell 5.1 đọc UTF-8-không-BOM theo codepage 1252, và lỗi báo ra là "missing terminator" ở một dòng cách xa nguyên nhân. Thêm BOM bằng byte, đừng đọc-ghi lại nội dung qua PowerShell — sẽ double-encode.

---

## 4. Cách tái lập

```powershell
# 1. Trình duyệt cho LIVA (đăng nhập một lần bằng tay)
powershell -ExecutionPolicy Bypass -File scripts\messenger-chrome.ps1

# 2. Lõi ổn định, KHÔNG watcher
$env:LIVA_DB_PATH = "E:\Project\LIVA\data\liva.db"
.\target\debug\liva-desktop.exe
```

Tiền kiểm trước khi gửi — lệnh `messenger:status` nói rõ hỏng ở chặng nào thay vì thất bại mù:

| `state` | Nghĩa |
|---|---|
| `chua_dang_nhap` | Profile chưa đăng nhập; tự đăng nhập trong cửa sổ đó |
| `khong_mo_duoc_hoi_thoai` | `handle` sai — lấy lại phần sau `/t/` trong URL |
| `khong_thay_o_soan` | Hội thoại mở nhưng giao diện đã đổi; cập nhật bộ chọn |
| `san_sang` | Gửi được |

Lấy `handle`: mở cuộc trò chuyện bằng tay, chép phần sau `/t/` trong URL.

---

## 5. Ranh giới cần giữ

Meta **cấm tự động hoá** trong điều khoản; rủi ro khoá tài khoản là thật. Module này tồn tại vì người dùng đã được báo và vẫn chọn làm. Facebook **không có API cho tin nhắn cá nhân** — Messenger Platform API chỉ cho Page trả lời người đã nhắn Page trước — nên lái giao diện là đường duy nhất, không phải lựa chọn kiến trúc.

Tài khoản mới lập **không** phải đường tắt: tin từ người lạ rơi vào hộp thư lọc, và tài khoản mới cộng tự động hoá là đúng chữ ký mà hệ thống liêm chính của Meta săn. Dùng chính tài khoản của người dùng là đường thực tế hơn.
