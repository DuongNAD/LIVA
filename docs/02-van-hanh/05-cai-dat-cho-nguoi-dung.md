---
title: "Cài đặt và sử dụng LIVA (cho người dùng)"
updated: 2026-07-31
commit: 3688b5f
status: living
owns:
  - cai-dat-windows
  - go-va-nang-cap
  - khac-phuc-su-co-nguoi-dung
covers:
  - liva-desktop/src-tauri/tauri.conf.json
  - liva-desktop/src-tauri/capabilities/widget.json
  - liva-desktop/src-tauri/capabilities/dashboard.json
  - liva-desktop/src-tauri/capabilities/setup.json
  - liva-native-core/src/setup/mod.rs
  - liva-native-core/src/commands/setup.rs
  - liva-native-core/src/setup_cli.rs
  - liva-ui/public/setup.html
  - data/models-manifest.json
  - scripts/check-installer-config.mjs
---
# Cài đặt và sử dụng LIVA

[⬆ Mục lục](../README.md) · [◀ Triển khai và runtime](03-trien-khai-va-runtime.md)

Tài liệu này viết cho **người dùng cuối** — bạn không cần biết lập trình, không
cần cài Node, Python, Rust hay Git. Nếu bạn muốn build LIVA từ mã nguồn, xem
[Triển khai và runtime](03-trien-khai-va-runtime.md).

---

## 1. Máy của bạn cần gì

| Yêu cầu | Mức | Ghi chú |
|---|---|---|
| Windows | 10 hoặc 11, 64-bit | Chưa có bản macOS/Linux |
| Ổ trống | **~4 GB tối thiểu; ~7 GB nếu tải full** | Ứng dụng ~250 MB; model minimal 2,28 GiB, full 5,95 GiB |
| RAM | 8 GB tối thiểu, 16 GB thì dễ thở | Model chạy trên RAM, không phải trên mây |
| Mạng | Chỉ cần **lúc tải model lần đầu** | Sau đó rút mạng LIVA vẫn chạy |
| Card đồ hoạ | Không bắt buộc | Xem mục 6 về tính năng nhìn màn hình |

Bạn **không** cần cài .NET, Java, Python hay WebView2 — bộ cài đã gồm sẵn.

---

## 2. Cài đặt

1. Tải file `LIVA_<phiên bản>_x64-setup.exe` (**~224 MB** — lớn vì đã gồm sẵn
   WebView2 offline, để máy không có mạng lúc cài vẫn cài được).
2. Bấm đúp để chạy. Windows sẽ hiện cảnh báo **"Windows protected your PC"** —
   bấm **More info** → **Run anyway**.

   > Cảnh báo này xuất hiện vì bộ cài **chưa được ký số**. Chứng chỉ ký số là
   > thứ phải mua hàng năm và LIVA hiện chưa có. Nói thẳng ra thì: cảnh báo đó
   > đúng — Windows không có cách nào xác minh ai làm ra file này. Chỉ cài nếu
   > bạn tin nguồn tải.

3. Chọn ngôn ngữ (Tiếng Việt hoặc English), rồi bấm Next tới hết.

LIVA cài vào thư mục cá nhân của bạn, **không cần quyền Administrator**:

```
C:\Users\<tên bạn>\AppData\Local\LIVA
```

---

## 3. Lần chạy đầu — tải model

Mở LIVA. Nếu máy chưa có model, một cửa sổ **"Chuẩn bị LIVA"** sẽ tự hiện ra.

Cửa sổ này liệt kê từng khả năng và tình trạng của nó:

| Khả năng | Thiếu thì sao |
|---|---|
| Chat (LLM router) | LIVA mở được nhưng mọi câu hỏi trả lỗi |
| Nghe (STT) | Không nhận được giọng nói của bạn |
| Nói tiếng Việt (TTS) | LIVA không phát ra tiếng |
| Bộ nhớ dài hạn | **LIVA không nhớ gì** giữa các lần trò chuyện |
| Cắt lượt nói (VAD) | Không biết bạn nói xong lúc nào |

Bấm **Tải model**. Bộ tối thiểu là **13 file, 2,28 GB** — với đường truyền
20 Mbps thì chừng 15–20 phút. Bộ đầy đủ là **29 file, 5,95 GiB**; 26 file có
nguồn tự tải và 3 file tuỳ chọn cần chuẩn bị thủ công.

Bốn điều nên biết:

- **Mỗi file đều được kiểm SHA-256 trước khi dùng.** LIVA băm file trong lúc tải
  và chỉ đưa vào thư mục model khi mã băm khớp đúng thứ dự án công bố. Lệch một
  byte là file bị xoá và báo lỗi, kể cả khi dung lượng khớp. Bạn không phải làm
  gì — nhưng nếu thấy báo "SHA-256 KHÔNG khớp" lặp lại thì đừng bỏ qua: mạng của
  bạn đang trả về một file khác.

- **Tải dở không mất.** Đóng cửa sổ hay mất mạng giữa chừng đều được; lần sau mở
  lại, LIVA tải tiếp từ chỗ dừng chứ không tải lại từ đầu.
- **Có thể bỏ qua.** Bấm *Bỏ qua, dùng luôn* nếu bạn chỉ muốn xem giao diện. Các
  tính năng thiếu model sẽ không hoạt động, và cửa sổ này sẽ hiện lại lần sau.
- **Một vài file phải tự chuẩn bị.** Vài model tuỳ chọn (giọng nói tiếng Việt
  chất lượng cao, wake-word tự huấn luyện) không có nguồn tải công khai. Cửa sổ
  sẽ nói rõ file nào và vì sao, thay vì âm thầm bỏ qua.

---

## 4. Dùng LIVA

Sau khi tải xong model, đóng cửa sổ chuẩn bị. LIVA mở hai cửa sổ:

- **Widget** — hình đại diện trong suốt nổi trên màn hình. Bạn bấm xuyên qua
  được những chỗ trống của nó, nên nó không cản việc bạn đang làm.
- **Dashboard** — bảng điều khiển: chọn model, chỉnh giọng, xem trạng thái máy,
  nhập khoá API nếu bạn muốn dùng thêm dịch vụ ngoài.

Việc thường làm nhất:

| Bạn muốn | Làm gì |
|---|---|
| Trò chuyện bằng chữ | Gõ vào ô chat trong Dashboard |
| Nói chuyện bằng giọng | Bật micro trong Dashboard; LIVA tự biết khi bạn nói xong |
| Đổi model | Dashboard → phần AI → chọn file `.gguf` |
| Tải thêm model tuỳ chọn | Mở lại cửa sổ chuẩn bị (Dashboard → Thiết lập model) |

---

## 5. Dữ liệu của bạn nằm ở đâu

Đây là phần đáng đọc kỹ nhất, vì nó quyết định bạn mất gì khi gỡ cài đặt.

| Thứ | Đường dẫn | Gỡ cài đặt có mất? |
|---|---|---|
| **Ứng dụng** (thư mục cài) | `%LOCALAPPDATA%\LIVA\` | Có — đây chính là thứ bị gỡ |
| Model AI | `%LOCALAPPDATA%\com.liva.cognitive-os\models\` | **Không** |
| Ký ức, lịch sử trò chuyện | `%LOCALAPPDATA%\com.liva.cognitive-os\data\` | **Không** |
| Cấu hình | `%LOCALAPPDATA%\com.liva.cognitive-os\data\liva-config.json` | **Không** |
| Khoá API đã lưu | `%LOCALAPPDATA%\com.liva.cognitive-os\liva_vault.app` | **Không** |

Hai thư mục khác nhau, và khác nhau là có chủ đích: `LIVA\` là **thư mục cài**
(trình gỡ dọn sạch nó), còn `com.liva.cognitive-os\` là **dữ liệu của bạn**
(trình gỡ không đụng tới). Tên thứ hai lấy theo mã định danh ứng dụng, cùng chỗ
két API key vẫn nằm từ trước.

> **Nâng cấp từ bản trước 28/07/2026?** Các bản cũ để dữ liệu ngay trong thư mục
> cài. LIVA tự nhận ra điều đó: thấy `liva-config.json`, database hoặc thư mục
> `models` ở chỗ cũ thì nó **tiếp tục dùng chỗ cũ** thay vì bỏ lại. Bạn không
> phải làm gì. Muốn dọn về chỗ mới thì chép hai thư mục `data` và `models` sang
> `%LOCALAPPDATA%\com.liva.cognitive-os\` rồi mở lại LIVA.

Dán `%LOCALAPPDATA%\com.liva.cognitive-os` vào thanh địa chỉ của File Explorer
để mở nhanh.

**Muốn để model sang ổ khác** (model chiếm vài GB): đặt biến môi trường
`LIVA_HOME` trỏ tới thư mục bạn chọn, trước khi mở LIVA. Ví dụ trong PowerShell:

```powershell
[Environment]::SetEnvironmentVariable('LIVA_HOME', 'D:\LIVA', 'User')
```

Mở lại LIVA sau khi đặt. Dữ liệu cũ **không tự chuyển** — chép tay thư mục
`models` và `data` sang chỗ mới nếu bạn muốn giữ.

---

## 6. Nâng cấp và gỡ cài đặt

**Nâng cấp:** tải bản `-setup.exe` mới và chạy đè. Không cần gỡ bản cũ. Model,
ký ức và cấu hình giữ nguyên vì chúng nằm ngoài thư mục cài (bảng ở mục 5).

**Gỡ:** Settings → Apps → Installed apps → LIVA → Uninstall. Hoặc chạy
`%LOCALAPPDATA%\LIVA\uninstall.exe`.

Sau khi gỡ, nếu muốn xoá sạch cả model và ký ức thì xoá tay thư mục
`%LOCALAPPDATA%\com.liva.cognitive-os` (thư mục `%LOCALAPPDATA%\LIVA` đã được
trình gỡ dọn).

---

## 7. Khắc phục sự cố

| Hiện tượng | Nguyên nhân thường gặp | Cách xử lý |
|---|---|---|
| Windows chặn khi cài | Bộ cài chưa ký số | More info → Run anyway (mục 2) |
| LIVA mở nhưng không trả lời | Chưa tải model chat | Mở cửa sổ chuẩn bị, tải nhóm "Chat" |
| LIVA không nghe thấy gì | Chưa tải model STT, hoặc Windows chưa cho phép micro | Tải nhóm "Nghe"; kiểm Settings → Privacy → Microphone |
| LIVA không phát ra tiếng | Thiếu giọng đọc, hoặc thiếu `espeak-ng` | Tải nhóm "Nói tiếng Việt". Vẫn câm thì cài [espeak-ng](https://github.com/espeak-ng/espeak-ng/releases) |
| LIVA không nhớ gì | Thiếu model bộ nhớ dài hạn | Tải nhóm "Bộ nhớ dài hạn" |
| Cửa sổ trắng trơn | WebView2 hỏng | Cài lại [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| Tải model hỏng giữa chừng | Mạng đứt | Bấm Tải lại — phần đã tải được giữ |
| Nhìn màn hình rất chậm (~80 giây) | Đang chạy trên CPU | Cần bản dựng có CUDA + card NVIDIA; xem mục 8 |
| Hộp thoại “LIVA không thể khởi động” | Lỗi DB, quyền thư mục dữ liệu hoặc khoá mã hoá | Làm đúng hướng dẫn trong hộp thoại; **không xoá** DB/vault |
| Báo database `malformed` | File SQLite hỏng | Đóng LIVA, sao lưu toàn bộ `%LOCALAPPDATA%\com.liva.cognitive-os`, rồi phục hồi từ backup; cài lại app không sửa được dữ liệu hỏng |
| Báo `readonly` / `permission denied` | Thư mục dữ liệu không ghi được | Cấp quyền ghi cho thư mục dữ liệu hoặc sửa `LIVA_HOME`; không chuyển DB vào `Program Files` |
| Báo ổ đĩa đầy | Không còn chỗ cho DB/WAL/model | Giải phóng dung lượng; **không xoá** file `-wal`/`-shm` khi LIVA đang chạy |

`espeak-ng` **không** đi kèm bộ cài. Nó là phần mềm ngoài với giấy phép riêng
và LIVA vẫn nói được tiếng Việt không cần nó trong phần lớn trường hợp — chỉ
ngữ điệu một số câu bị ảnh hưởng. Cài thêm nếu bạn thấy giọng đọc sai.

---

## 8. Những gì bản cài này **không** làm

Nói trước để bạn không mất thời gian tìm:

- **Không tự cập nhật.** Muốn bản mới thì tải và chạy đè.
- **Không ký số** — Windows sẽ cảnh báo mỗi lần cài.
- **Không kèm model** (2,28 GB bộ tối thiểu) — tải riêng ở lần chạy đầu.
- **Không kèm CUDA.** Bản phát hành chạy CPU. Tính năng *nhìn màn hình*
  (`vision:ask`) vì thế mất khoảng 80 giây mỗi lượt thay vì ~1,2 giây. Muốn
  nhanh thì phải tự build với `--features cuda` — xem
  [Triển khai và runtime](03-trien-khai-va-runtime.md).
- **Không có bản MSI**, không triển khai qua GPO.
- **Không gửi gì ra Internet** sau khi tải model xong, trừ khi chính bạn bật
  tích hợp Telegram hoặc nhập khoá API dịch vụ ngoài.

---

## Liên quan

**Đọc tiếp theo mạch:** [⬆ Mục lục](../README.md) · [Triển khai và runtime](03-trien-khai-va-runtime.md)

**Tài liệu này dựa vào:**
- [Triển khai và runtime](03-trien-khai-va-runtime.md) — tiến trình, cổng, cách chạy từ mã nguồn
- [Mô hình AI và tài nguyên](02-mo-hinh-ai-va-tai-nguyen.md) — model nào để làm gì, tốn bao nhiêu RAM
- [Cấu hình và biến môi trường](01-cau-hinh-va-bien-moi-truong.md) — danh sách `LIVA_*`

**Khi sửa code sau đây thì phải cập nhật tài liệu này:**
- `liva-desktop/src-tauri/tauri.conf.json` — thư mục cài, ngôn ngữ, WebView2 (mục 1, 2)
- `data/models-manifest.json` — danh sách model và dung lượng (mục 3)
- `liva-native-core/src/setup/mod.rs` · `liva-ui/public/setup.html` — cửa sổ chuẩn bị (mục 3)
- `liva-native-core/src/paths.rs#data_dir` / `liva-native-core/src/paths.rs#user_home_dir` — bảng đường dẫn dữ liệu (mục 5)
