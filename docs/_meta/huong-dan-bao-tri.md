---
title: "Hướng dẫn bảo trì bộ tài liệu"
updated: 2026-07-21
commit: 5d69c3c
status: index
owns:
  - luoc-do-front-matter
  - quy-trinh-bao-tri-tai-lieu
covers: []
---
# Hướng dẫn bảo trì bộ tài liệu LIVA

[⬆ Mục lục](../README.md) · [Sổ đăng ký nguồn sự thật](nguon-su-that.md) · [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md)

---

File này dành cho **người (hoặc agent) sắp sửa code hoặc sửa tài liệu**. Nó trả lời đúng ba câu hỏi:
code vừa đổi thì phải sửa tài liệu nào, sửa xong phải khai báo lại cái gì, và làm sao biết mình đã
làm sạch.

Bộ tài liệu LIVA được thiết kế quanh **một nguyên tắc vàng duy nhất**:

> **Một sự thật chỉ được viết đầy đủ ở đúng một nơi.**

Mọi bảng, mọi con số, mọi sơ đồ đều có **một tài liệu sở hữu**. Tài liệu khác muốn nhắc tới thì chỉ
được tóm tắt 1–3 dòng rồi trỏ về nguồn bằng một dòng trích dẫn `📌 Nguồn đầy đủ`. Nhờ vậy khi code
đổi, bạn chỉ phải sửa **một chỗ** thay vì đi tìm bảy bản sao của cùng một bảng.

Công cụ ép nguyên tắc này là `scripts/docs-check.mjs`. Nó là trọng tài — không phải trí nhớ của bạn.

---

## 1. Lược đồ front-matter

Mỗi file `.md` trong `docs/` (trừ `99-luu-tru/`, `assets/`, `04-quy-trinh/prompts/`) **bắt buộc** mở
đầu bằng một khối YAML. Đây là phần máy đọc được — checker phân tích chính khối này.

```yaml
---
title: "Đường ống thoại"
updated: 2026-07-21
commit: 5d69c3c
status: living
owns:
  - chuoi-xu-ly-thoai
  - bang-nguong-vad-aec-denoise
covers:
  - liva-native-core/src/stt/*
  - liva-native-core/src/webrtc/*
  - liva-ui/src/composables/useVoicePipeline.ts
---
```

| Trường | Bắt buộc | Ý nghĩa | Giá trị hợp lệ |
|---|---|---|---|
| `title` | ✅ | Tên hiển thị của tài liệu. Dùng trong mục lục và khi tài liệu khác trỏ tới nó. Nên trùng với tiêu đề `#` ở dòng đầu thân bài | Chuỗi trong nháy kép |
| `updated` | ✅ | Ngày sửa nội dung gần nhất. **Chỉ đổi khi nội dung thật sự đổi**, không đổi khi chỉ sửa chính tả | `YYYY-MM-DD` |
| `commit` | ✅ | Hash commit mà nội dung tài liệu mô tả. Đây là **mốc đối chiếu lỗi thời** — checker chạy `git log <commit>..HEAD -- <covers>` để xem code đã đi trước tài liệu chưa | Hash ngắn (7 ký tự), hoặc `auto` cho file sinh tự động |
| `status` | ✅ | Vòng đời của tài liệu | `living` \| `frozen` \| `index` |
| `owns` | ✅ | Danh sách **khoá sự thật** mà tài liệu này là nguồn duy nhất. Checker báo lỗi nếu hai tài liệu cùng nhận một khoá | Danh sách kebab-case không dấu, hoặc `[]` |
| `covers` | ✅ | Danh sách **file/thư mục mã nguồn** mà tài liệu này mô tả. Có thể dùng hậu tố `/*` cho cả thư mục | Đường dẫn tương đối gốc repo, hoặc `[]` |

### Ba giá trị của `status`

| Giá trị | Nghĩa | Checker làm gì | Bạn được sửa không? |
|---|---|---|---|
| `living` | Tài liệu **sống**, mô tả code hiện tại, phải theo kịp code | Đối chiếu `commit` với lịch sử git của `covers` → báo lỗi thời | ✅ Sửa bất cứ khi nào code đổi |
| `frozen` | **Ảnh chụp lịch sử**, cố tình đóng băng ở một mốc thời gian | Bỏ qua kiểm tra lỗi thời (nhưng vẫn kiểm liên kết + front-matter) | ❌ **Không bao giờ sửa nội dung** |
| `index` | Mục lục, bản đồ, hướng dẫn quy trình — không mô tả code | Bỏ qua kiểm tra lỗi thời | ✅ Sửa khi cấu trúc bộ tài liệu đổi |

Hiện có đúng một tài liệu `frozen`: [Báo cáo khảo sát gốc 2026-07](../03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md).
Nó là nguồn mà toàn bộ bộ tài liệu được biên tập ra; giữ nguyên để đối chiếu khi nghi ngờ một chi
tiết đã bị biên tập sai. Nếu bạn thấy nó nói sai so với code hôm nay — **đó là chuyện bình thường và
đúng thiết kế**: sửa ở bản vẽ tương ứng, không sửa nó.

### Vì sao `covers` là trường quan trọng nhất

`covers` là thứ duy nhất biến "tài liệu lỗi thời" từ **cảm giác** thành **tín hiệu tự động**.

Cơ chế: checker lấy `commit` trong front-matter, rồi hỏi git *"kể từ commit đó tới HEAD, có file nào
trong `covers` bị đổi không?"*. Nếu có, tài liệu bị đưa vào danh sách **CÓ THỂ ĐÃ LỖI THỜI** kèm đúng
danh sách file đã đổi — bạn biết ngay phải mở mục nào.

Hệ quả thực tế:

- **`covers` thiếu → tài liệu âm thầm mục ruỗng.** Code đổi, không ai được báo, tài liệu nói dối
  hàng tháng trời mà checker vẫn xanh.
- **`covers` thừa → nhiễu.** Khai cả `liva-native-core/src/*` cho một tài liệu chỉ nói về TTS thì
  mọi commit chạm lõi Rust đều báo động giả, và người ta bắt đầu bỏ qua cảnh báo.
- **`covers` trỏ sai đường dẫn → lỗi cứng.** Checker báo `covers trỏ tới đường dẫn không tồn tại`.
  Đây thường là dấu hiệu ai đó đổi tên file nguồn mà quên vá tài liệu.

`covers` cũng là dữ liệu sinh ra [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md) — bảng tra ngược
"sửa file này thì đọc tài liệu nào". Bản đồ đó **sinh tự động**, đừng sửa tay.

---

## 2. Quy trình khi sửa code

Làm theo đúng thứ tự. Bước 1 và bước 6 là cùng một lệnh — đó là chủ ý.

1. **Chạy checker để biết mình đang nợ gì.**

   ```powershell
   node scripts/docs-check.mjs
   ```

   Đọc mục `⚠️ TÀI LIỆU CÓ THỂ ĐÃ LỖI THỜI`. Mỗi dòng cho biết tài liệu nào ghi commit nào, và file
   mã nguồn nào đã đổi sau đó. Đây là **danh sách việc phải làm**, không phải gợi ý.

2. **Sửa đúng những tài liệu được báo tên.** Mở tài liệu, tìm mục nói về file vừa đổi (mỗi tài liệu
   sống đều có mục *"Khi sửa code sau đây thì phải cập nhật tài liệu này"* ở cuối, ánh xạ file →
   số mục). Sửa nội dung, sửa toạ độ `file:dòng` đã lệch, đổi nhãn nếu trạng thái nối dây đổi.

   Thứ tự sửa: `01-ban-ve/` → `02-van-hanh/` → `03-danh-gia/`. Bản đánh giá tham chiếu bản vẽ, sửa
   ngược lại sẽ sinh mâu thuẫn.

3. **Cập nhật `updated` và `commit` trong front-matter** của mọi tài liệu vừa sửa.

   ```yaml
   updated: 2026-08-03      # ngày hôm nay
   commit: a1b2c3d          # git rev-parse --short HEAD
   ```

   Đây là bước hay bị quên nhất. Quên nó thì checker vẫn báo tài liệu lỗi thời ở lần chạy sau, và
   bạn sẽ sửa lại chính chỗ vừa sửa.

4. **Nếu thêm / xoá / đổi tên file mã nguồn → cập nhật `covers`.**
   - Thêm module mới (ví dụ `liva-native-core/src/tts/vocoder.rs`): thêm vào `covers` của tài liệu
     mô tả nó. Nếu chưa tài liệu nào mô tả, checker sẽ liệt kê nó ở mục *"Chưa được tài liệu nào
     mô tả"* trong bản đồ.
   - Đổi tên file: sửa **mọi** `covers` đang trỏ tới tên cũ. Tìm nhanh bằng bản đồ code ↔ tài liệu.
   - Xoá file: gỡ khỏi `covers`, và gỡ luôn mục mô tả nó trong thân bài.

5. **Nếu thêm một bảng / sơ đồ / con số mới → khai `owns` và ghi vào sổ nguồn sự thật.**
   - Đặt một khoá kebab-case không dấu, mô tả đúng thứ được sở hữu: `bang-backend-tts`,
     `nguong-governor`, `erd-sqlite`.
   - Thêm khoá vào `owns` của **đúng một** tài liệu.
   - Thêm một dòng vào [sổ đăng ký nguồn sự thật](nguon-su-that.md), đúng mục con tương ứng.
   - Nếu tài liệu khác cần nhắc tới bảng đó: viết 1–3 dòng tóm tắt rồi thêm ngay dòng

     ```markdown
     > 📌 Nguồn đầy đủ: [Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md)
     ```

     **Không chép nguyên bảng.** Checker kiểm dòng này trỏ tới file có thật, và cảnh báo nếu file
     đích không khai sở hữu sự thật nào.

6. **Chạy lại checker cho tới khi sạch.**

   ```powershell
   node scripts/docs-check.mjs           # phải in "✅ Tài liệu sạch."
   node scripts/docs-check.mjs --map     # sinh lại bản đồ code ↔ tài liệu
   ```

   Checker thoát mã 1 nếu còn **lỗi**. Cảnh báo và danh sách lỗi thời không làm nó thoát 1 — nhưng
   để lại là nợ, không phải là xong.

> **Không commit tự động.** Theo `AGENTS.md`, chỉ chạy `git commit` / `git push` khi người dùng yêu
> cầu rõ ràng.

---

## 3. Quy trình khi thêm tài liệu mới

1. **Đặt tên file.** Tiền tố số hai chữ số + kebab-case **không dấu**, đuôi `.md`:
   `05-agent-bo-nho-va-tien-hoa.md`. Số quyết định vị trí trong chuỗi đọc, nên đừng nhét số trùng —
   nếu phải chèn giữa, đánh số lại cả nhánh và vá liên kết.

2. **Chọn thư mục theo bản chất nội dung.**

   | Thư mục | Dành cho |
   |---|---|
   | `01-ban-ve/` | Hệ thống được **lắp ráp thế nào** — bản vẽ kỹ thuật |
   | `02-van-hanh/` | Làm cho hệ thống **chạy được trên máy thật** — cấu hình, model, deploy, test |
   | `03-danh-gia/` | Hệ thống **đang ở đâu so với tuyên bố** — đối chiếu, rủi ro, lộ trình |
   | `04-quy-trinh/` | Công cụ làm việc — prompt, template. Không mô tả code |
   | `_meta/` | Tài liệu về **chính bộ tài liệu** — bản đồ, hướng dẫn bảo trì |

3. **Viết front-matter đầy đủ 6 trường** theo lược đồ ở §1. `status: living` nếu nó mô tả code.
   `covers` phải liệt kê thật — đừng để `[]` cho một tài liệu sống.

4. **Chèn vào chuỗi prev/next.** Mỗi tài liệu có **hai** dải điều hướng phải khớp nhau: một ngay
   dưới tiêu đề `#`, một ở cuối bài sau nhãn `**Đọc tiếp theo mạch:**`.

   ```markdown
   [⬆ Mục lục](../README.md) · [◀ Tên tài liệu trước]({03-ten-truoc}.md) · [Tên tài liệu sau ▶]({05-ten-sau}.md)
   ```

   Chèn một file vào giữa nghĩa là sửa **ba** file: file mới, file trước nó (đổi liên kết `▶`), file
   sau nó (đổi liên kết `◀`). File đầu chuỗi không có `◀`, file cuối chuỗi không có `▶`.

5. **Thêm vào [mục lục `docs/README.md`](../README.md):** một dòng trong bảng của thư mục tương ứng
   (cột Tài liệu · Nội dung · Sơ đồ), **và** nếu tài liệu thuộc luồng đọc chính thì thêm vào một
   trong ba chuỗi đọc ở mục *"Bắt đầu từ đâu"*.

6. **Khai `owns` + ghi vào [sổ đăng ký nguồn sự thật](nguon-su-that.md).** Nếu tài liệu mới lấy quyền
   sở hữu một bảng đang thuộc tài liệu cũ, phải **rút khoá đó khỏi `owns` của tài liệu cũ** và thay
   bảng ở đó bằng tóm tắt + dòng `📌 Nguồn đầy đủ`. Checker báo lỗi nếu hai nơi cùng nhận một khoá.

7. **Chạy `node scripts/docs-check.mjs --map`** để đưa `covers` mới vào bản đồ.

---

## 4. Quy trình khi xoá hoặc lưu trữ tài liệu

Nguyên tắc: **không xoá thẳng.** Tài liệu lỗi thời vẫn là tư liệu lịch sử; xoá đi là mất bối cảnh
"tại sao ngày đó lại thiết kế như vậy".

1. **Chuyển file vào [`docs/99-luu-tru/`](../99-luu-tru/README.md)**, đặt vào thư mục con hợp nghĩa
   (`kien-truc-nodejs-v29/`, `bao-cao-lich-su/`, `ke-hoach-da-hoan-thanh/`, `thiet-ke-goc/`) hoặc
   tạo thư mục con mới. Checker **bỏ qua toàn bộ `99-luu-tru/`**, nên front-matter ở đó không còn bị
   ràng buộc — nhưng giữ lại vẫn tốt cho việc truy vết.

2. **Ghi lý do vào README lưu trữ.** Mở [`docs/99-luu-tru/README.md`](../99-luu-tru/README.md), thêm
   một dòng: tài liệu nào, chuyển ngày nào, **vì sao lỗi thời**, và **đọc gì thay thế**. Không có
   dòng này thì người sau sẽ mở nhầm và tin nhầm.

3. **Vá mọi liên kết đang trỏ tới nó.** Đây là bước dễ sót nhất. Tìm bằng:

   ```powershell
   node scripts/docs-check.mjs        # sẽ báo "liên kết hỏng → ..."
   ```

   Với mỗi liên kết hỏng: hoặc trỏ sang tài liệu thay thế, hoặc gỡ hẳn câu chứa nó. **Đừng** trỏ vào
   `99-luu-tru/` như một nguồn tham chiếu — chỉ được trỏ tới đó với ngữ cảnh "tư liệu lịch sử".

4. **Nối lại chuỗi prev/next.** File trước và file sau tài liệu vừa gỡ phải trỏ thẳng vào nhau, ở
   **cả hai** dải điều hướng (đầu bài và cuối bài).

5. **Chuyển quyền sở hữu `owns` nếu có.** Nếu tài liệu bị lưu trữ đang sở hữu khoá sự thật nào, khoá
   đó phải được chuyển sang tài liệu kế thừa, cùng với nội dung bảng. Bỏ trống nghĩa là sự thật đó
   không còn nguồn nào — tệ hơn cả trùng lặp.

6. **Gỡ khỏi mục lục** `docs/README.md`: xoá dòng trong bảng, xoá khỏi các chuỗi đọc, xoá khỏi mục
   `## Liên quan` ở cuối.

7. **Chạy lại `node scripts/docs-check.mjs --map`** — bản đồ code ↔ tài liệu phải không còn nhắc tên
   file đã lưu trữ.

---

## 5. Bảng ánh xạ nhanh: sửa vùng code này → đọc/sửa tài liệu nào

Dữ liệu lấy từ trường `covers` của 18 tài liệu sống. Đây là bảng **rút gọn theo thư mục** để tra
bằng mắt; bảng đầy đủ tới từng file sinh tự động ở [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md).

**Ký hiệu tài liệu:** `BV` = `01-ban-ve/` · `VH` = `02-van-hanh/` · `ĐG` = `03-danh-gia/`.

| Vùng mã nguồn | Tài liệu chính (sửa trước) | Tài liệu phụ (rà lại) |
|---|---|---|
| `liva-native-core/src/main.rs` | [BV01 Kiến trúc tổng thể](../01-ban-ve/01-kien-truc-tong-the.md) | [BV08 Frontend & Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) · [BV09 Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) |
| `liva-native-core/src/webrtc/*` | [BV03 Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) · [BV02 Giao thức IPC & WS](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) | [BV10 Phụ thuộc module](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) · [VH01 Cấu hình & env](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH04 Kiểm thử & CI](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-native-core/src/stt/*` | [BV03 Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) | [VH01 Cấu hình & env](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02 Mô hình AI & tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) *(`parakeet.rs`)* · [VH04 Kiểm thử & CI](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `liva-native-core/src/tts/*` (kể cả `vieneu/`) | [BV03 Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) | [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [VH03 Triển khai & runtime](../02-van-hanh/03-trien-khai-va-runtime.md) *(`espeak.rs`)* · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) *(`style_vector.rs`)* |
| `liva-native-core/src/llm/*` | [BV04 Hệ LLM & prompt](../01-ban-ve/04-he-llm-va-prompt.md) | [BV06 Thị giác & governor](../01-ban-ve/06-thi-giac-passive-va-governor.md) *(`engine.rs`)* · [BV07 Dữ liệu & bảo mật](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) *(`embed.rs`)* · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-native-core/src/agent/*` | [BV05 Agent, bộ nhớ & tiến hoá](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) | [BV03](../01-ban-ve/03-duong-ong-thoai.md) · [BV04](../01-ban-ve/04-he-llm-va-prompt.md) · [BV06](../01-ban-ve/06-thi-giac-passive-va-governor.md) · [BV07](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) *(`memory.rs`)* · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) *(`dispatcher.rs`)* · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-native-core/src/evolution/*` | [BV05 Agent, bộ nhớ & tiến hoá](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) | [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `liva-native-core/src/mcp/*` | [BV09 Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) | [BV01](../01-ban-ve/01-kien-truc-tong-the.md) *(`server.rs`)* · [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) · [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) *(`protocol.rs`)* · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-native-core/src/vision/*` | [BV06 Thị giác & governor](../01-ban-ve/06-thi-giac-passive-va-governor.md) | [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) *(`capture.rs`)* · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `liva-native-core/src/passive/*` | [BV06 Thị giác & governor](../01-ban-ve/06-thi-giac-passive-va-governor.md) | [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) *(`hook.rs`)* · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `liva-native-core/src/integrations/*` | [BV09 Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) *(`smart_home.rs`)* | [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `liva-native-core/src/db.rs` · `crypto.rs` · `prng.rs` | [BV07 Dữ liệu & bảo mật](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) | [BV00 Tổng quan](../01-ban-ve/00-tong-quan-he-thong.md) · [BV04](../01-ban-ve/04-he-llm-va-prompt.md) · [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) |
| `liva-native-core/src/telegram.rs` · `wake_model.rs` | [BV09 Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) | [VH02 Mô hình AI & tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) |
| `liva-native-core/src/bin/*` (binary kiểm chứng) | [VH04 Kiểm thử & CI](../02-van-hanh/04-kiem-thu-va-ci.md) | [BV02](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) *(`verify_duplex`)* · [BV03](../01-ban-ve/03-duong-ong-thoai.md) · [BV04](../01-ban-ve/04-he-llm-va-prompt.md) *(`router_stress`, `qwen3vl_probe`)* · [BV06](../01-ban-ve/06-thi-giac-passive-va-governor.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) *(`verify_integrations`)* · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `liva-native-core/tests/*` | [VH04 Kiểm thử & CI](../02-van-hanh/04-kiem-thu-va-ci.md) | [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) · [BV07](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) *(`verify_commands.rs`)* · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `Cargo.toml` · `liva-native-core/Cargo.toml` (deps, feature flag) | [VH02 Mô hình AI & tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) | [BV04](../01-ban-ve/04-he-llm-va-prompt.md) · [BV08](../01-ban-ve/08-frontend-va-vo-tauri.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH03](../02-van-hanh/03-trien-khai-va-runtime.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-desktop/src-tauri/src/lib.rs` (lệnh Tauri) | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | Gần như **mọi** tài liệu sống đều `covers` file này — chạy checker rồi sửa theo danh sách nó in ra |
| `liva-desktop/src-tauri/tauri.conf.json` (CSP, cửa sổ) | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV00](../01-ban-ve/00-tong-quan-he-thong.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH03](../02-van-hanh/03-trien-khai-va-runtime.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [Mục lục](../README.md) |
| `liva-ui/src/App.vue` · `WidgetApp.vue` | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV00](../01-ban-ve/00-tong-quan-he-thong.md) · [BV01](../01-ban-ve/01-kien-truc-tong-the.md) · [BV02](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) · [BV03](../01-ban-ve/03-duong-ong-thoai.md) · [BV04](../01-ban-ve/04-he-llm-va-prompt.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-ui/src/composables/*` | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV01](../01-ban-ve/01-kien-truc-tong-the.md) · [BV02](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) · [BV03](../01-ban-ve/03-duong-ong-thoai.md) *(`useVoicePipeline`, `useSpeakerPlayback`)* · [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) · [BV06](../01-ban-ve/06-thi-giac-passive-va-governor.md) · [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) *(`useVRM`)* · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) |
| `liva-ui/src/components/dashboard/*` | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) *(`SkillsView`, `TaskManager`)* · [BV06](../01-ban-ve/06-thi-giac-passive-va-governor.md) *(`VisionView`)* · [BV07](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) *(`ApiManagementView`, `SystemView`)* · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) *(`SettingsView`)* · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) |
| `liva-ui/src/workers/*` (wake word, audio) | [BV03 Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) | [BV08](../01-ban-ve/08-frontend-va-vo-tauri.md) · [BV10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) *(`audio-worker`)* · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) |
| `liva-ui/src/platform/*` (adapter Tauri / mock web) | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV07](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) · [VH03](../02-van-hanh/03-trien-khai-va-runtime.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) |
| `liva-ui/src/utils/*` (`speakerFrame`, `fetch`, `avatarSync`) | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV01](../01-ban-ve/01-kien-truc-tong-the.md) · [BV02](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) · [BV03](../01-ban-ve/03-duong-ong-thoai.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `liva-ui/vite.config.ts` · `liva-ui/package.json` · `package.json` | [BV08 Frontend & vỏ Tauri](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV00](../01-ban-ve/00-tong-quan-he-thong.md) · [BV01](../01-ban-ve/01-kien-truc-tong-the.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `packages/liva-common/src/types/*` (hợp đồng WS & config) | [BV02 Giao thức IPC & WS](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) · [BV08](../01-ban-ve/08-frontend-va-vo-tauri.md) | [BV00](../01-ban-ve/00-tong-quan-he-thong.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) |
| `liva-voice/*` (dịch vụ Python cổng 8765) | [BV09 Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) | [BV00](../01-ban-ve/00-tong-quan-he-thong.md) · [BV03](../01-ban-ve/03-duong-ong-thoai.md) *(`vietnamese_normalizer.py`)* · [VH03](../02-van-hanh/03-trien-khai-va-runtime.md) · [VH04](../02-van-hanh/04-kiem-thu-va-ci.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `data/liva-config.json` · `models.config.json` · `skill_whitelist.json` | [VH01 Cấu hình & env](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) | [BV01](../01-ban-ve/01-kien-truc-tong-the.md) · [BV02](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) · [BV04](../01-ban-ve/04-he-llm-va-prompt.md) · [BV05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) *(`skill_whitelist`)* · [BV07](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [VH03](../02-van-hanh/03-trien-khai-va-runtime.md) · [ĐG01](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |
| `scripts/start_all.ps1` (khởi động dev) | [VH03 Triển khai & runtime](../02-van-hanh/03-trien-khai-va-runtime.md) | [BV01](../01-ban-ve/01-kien-truc-tong-the.md) · [BV02](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) · [BV08](../01-ban-ve/08-frontend-va-vo-tauri.md) · [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `scripts/ai-pre-commit.cjs` · `eslint.config.js` (hook & luật lint) | [VH04 Kiểm thử & CI](../02-van-hanh/04-kiem-thu-va-ci.md) | [BV09](../01-ban-ve/09-tich-hop-ngoai.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) |
| `.github/workflows/test.yml` (CI) | [VH04 Kiểm thử & CI](../02-van-hanh/04-kiem-thu-va-ci.md) | [BV00](../01-ban-ve/00-tong-quan-he-thong.md) · [VH01](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) · [VH02](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) · [ĐG02](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) · [ĐG03](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) |

> Cột *"Tài liệu chính"* là nơi chứa **bảng/sơ đồ sở hữu** cho vùng code đó — sửa ở đây trước. Cột
> *"Tài liệu phụ"* chỉ nhắc lại vài dòng và trỏ về nguồn; thường chỉ cần sửa nếu con số tóm tắt đổi.

---

## 6. Sổ nguồn sự thật

Ai sở hữu bảng nào — cùng mô tả ngắn từng khoá và danh sách tài liệu đang tham chiếu tới nó — được
giữ trong **một sổ đăng ký riêng**, không chép lại ở đây. Sổ đó cũng là nơi liệt kê các sự thật
**chưa có chủ** (đang bị viết ở ≥2 nơi mà chưa khoá nào bảo vệ) và quy tắc chuyển quyền sở hữu.

> 📌 Nguồn đầy đủ: [Sổ đăng ký nguồn sự thật](nguon-su-that.md)

Ba thứ trong `_meta/` chia việc như sau — nhớ đúng ba dòng này là đủ để không sửa nhầm chỗ:

| Hỏi | Tra ở đâu |
|---|---|
| "Sự thật X được viết đầy đủ ở tài liệu nào?" | [Sổ đăng ký nguồn sự thật](nguon-su-that.md) |
| "Tôi vừa sửa file mã nguồn này, tài liệu nào lỗi thời?" | [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md) + §5 ở trên |
| "Quy trình sửa/thêm/xoá tài liệu ra sao?" | Chính file này (§2, §3, §4) |

Khi thêm hoặc chuyển một khoá `owns`, phải sửa **cả hai** chỗ: front-matter của tài liệu chủ **và**
bảng đăng ký trong sổ. Checker chỉ bắt được lỗi trùng khoá giữa các front-matter — nó **không** biết
sổ đã lạc hậu, nên đây là phần vẫn phải làm bằng tay.

---

## 7. Chống trôi dạt

"Trôi dạt" là khi tài liệu vẫn trông chỉn chu nhưng đã không còn đúng với code. Bốn quy ước dưới đây
tồn tại để chống chính điều đó.

### 7.1 Không bịa số

- **Số nào không có nguồn thì không viết.** Mọi con số — LOC, số bảng SQLite, số lệnh
  `handle_command`, timing, ngưỡng VAD — đều phải đếm lại được từ code hoặc đọc được từ một hằng số
  trong code.
- **Tách bạch "đã kiểm chứng" và "tiềm năng".** Benchmark chưa chạy thì ghi rõ là ước tính, kèm cách
  tính. Không quy đổi ý định thành thành tựu.
- **Code thắng tài liệu cũ.** Nếu `99-luu-tru/` nói một đằng và code nói một nẻo, viết theo code.
- **Ghi rõ chỗ nghi ngờ** thay vì làm tròn cho đẹp. "Chưa xác minh", "không tìm thấy call-site",
  "cần đo lại" đều là câu trả lời hợp lệ và tốt hơn một con số bịa.

### 7.2 Luôn kèm `file:dòng`

Mọi khẳng định về hành vi hệ thống phải kèm toạ độ dạng `` `db.rs:188-354` `` hoặc `` `main.rs:42` ``.
Đường dẫn rút gọn tương đối theo module — `webrtc/vad.rs` nghĩa là
`liva-native-core/src/webrtc/vad.rs`.

Mục đích là **bất kỳ ai cũng mở đúng chỗ và tự kiểm chứng được**. Nếu một dòng đã dịch chuyển do sửa
code, hãy tìm theo **tên symbol** chứ đừng tin số dòng tuyệt đối — và nhân tiện sửa luôn toạ độ
trong tài liệu.

### 7.3 Ba nhãn trạng thái

Dùng thống nhất toàn bộ tài liệu. Chúng nói về **mức độ nối dây**, không nói về chất lượng code.

| Nhãn | Ý nghĩa | Cách kiểm chứng |
|---|---|---|
| **[OK]** | Đang chạy thật trên đường chạy chính, đã nối dây đầu-cuối | Có call-site trong đường đi mặc định, không cần bật env |
| **[MỘT PHẦN]** | Có code chạy được nhưng tắt mặc định / chỉ bật opt-in bằng env / chỉ sống ở một trong hai profile chạy / mới nối dây một nửa | Đọc điều kiện bật trong code — thường là một `std::env::var(...)` hoặc một nhánh `if` |
| **[THIẾU]** | Chưa có, là stub trả literal, hoặc là code mồ côi (0 call-site trong `src/`) | Grep tên symbol trong `src/` — không nơi nào gọi |

Khi đổi trạng thái nối dây, **phải đổi nhãn ở mọi nơi**: bật một tính năng opt-in thành mặc định thì
nâng **[MỘT PHẦN]** → **[OK]**; xoá code mồ côi thì gỡ hẳn mục **[THIẾU]** tương ứng khỏi
[ĐG02 Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md).

Với mục mang nhãn **[MỘT PHẦN]** hoặc **[THIẾU]**, luôn nêu rõ **thiếu chính xác cái gì** để người
sửa biết phải nối dây ở đâu.

### 7.4 Tài liệu `frozen` thì không bao giờ sửa

Một tài liệu `frozen` là **ảnh chụp tại một mốc thời gian**, giá trị của nó nằm ở chỗ nó *không* đổi.
Nếu nó nói sai so với code hôm nay, đó không phải lỗi cần vá — đó là bằng chứng hệ thống đã đi được
bao xa. Sửa ở bản vẽ tương ứng.

Muốn thay đổi một tài liệu `frozen` thì chỉ có hai lựa chọn hợp lệ: (a) tạo một ảnh chụp mới với tên
và ngày mới, hoặc (b) chuyển nó vào `99-luu-tru/` theo quy trình §4. Không có lựa chọn "sửa nhẹ".

---

## 8. Sai lầm thường gặp

| # | Sai lầm | Vì sao hại | Làm đúng |
|---|---|---|---|
| 1 | **Chép nguyên bảng sang tài liệu khác** cho "tiện đọc" | Lần sau code đổi, chỉ một bản được sửa; hai bảng mâu thuẫn và không ai biết bảng nào đúng | Tóm tắt 1–3 dòng rồi thêm một dòng `📌 Nguồn đầy đủ` trỏ về tài liệu sở hữu. Bảng đầy đủ chỉ nằm ở tài liệu khai `owns` khoá đó |
| 2 | **Sửa tài liệu `frozen`** vì thấy nó "nói sai" | Phá mất mốc đối chiếu — thứ duy nhất cho biết bộ tài liệu đã bị biên tập lệch chỗ nào | Sửa ở bản vẽ sống tương ứng. Ảnh chụp gốc giữ nguyên (§7.4) |
| 3 | **Đổi tên file mã nguồn mà quên cập nhật `covers`** | Checker báo `covers trỏ tới đường dẫn không tồn tại`, và tệ hơn: tài liệu đó **mất luôn tín hiệu lỗi thời** cho file mới | Sau khi đổi tên, tra [bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md) tìm mọi tài liệu đang `covers` tên cũ, sửa hết, rồi chạy `--map` |
| 4 | **Dùng đường dẫn tuyệt đối** (`E:\Project\LIVA\docs\...`) hoặc đường dẫn kiểu `docs/01-ban-ve/...` trong liên kết | Vỡ trên máy người khác, vỡ trên GitHub, vỡ trong mọi trình xem markdown | Luôn tương đối so với **vị trí file đang viết**. Từ `docs/01-ban-ve/X.md` sang `docs/02-van-hanh/Y.md` là `../02-van-hanh/Y.md`; từ `docs/_meta/` về mục lục là `../README.md` |
| 5 | **Sửa nội dung mà quên `updated` + `commit`** | Checker tiếp tục báo tài liệu lỗi thời; sau vài lần người ta bắt đầu bỏ qua cảnh báo và cơ chế chết | Coi hai trường này là một phần của việc sửa, không phải bước dọn dẹp cuối |
| 6 | **Sửa tay `_meta/ban-do-code-tai-lieu.md`** | File sinh tự động — lần chạy `--map` kế tiếp xoá sạch công sức | Sửa `covers` trong front-matter của tài liệu tương ứng, rồi chạy `node scripts/docs-check.mjs --map` |
| 7 | **Thêm tài liệu mới mà quên chuỗi prev/next và mục lục** | Tài liệu tồn tại nhưng không ai đi tới được — hiệu quả bằng không | Sửa đủ ba chỗ: file trước (`▶`), file sau (`◀`), và bảng trong [mục lục](../README.md). Nhớ **hai** dải điều hướng mỗi file: đầu bài và cuối bài |
| 8 | **Ghi số liệu "khoảng", "ước chừng" không kèm cách tính** | Vi phạm nguyên tắc không bịa số; con số đó sẽ được trích dẫn lại như sự thật đã kiểm chứng | Ghi rõ "ước tính, tính bằng …" hoặc ghi thẳng "chưa đo" (§7.1) |

---

## 9. Tra cứu nhanh lệnh

```powershell
node scripts/docs-check.mjs           # kiểm tra, thoát 1 nếu có lỗi
node scripts/docs-check.mjs --map     # kiểm tra + sinh lại _meta/ban-do-code-tai-lieu.md
node scripts/docs-check.mjs --quiet   # chỉ in lỗi (dùng trong CI/hook)
git rev-parse --short HEAD            # lấy hash điền vào trường `commit`
```

Checker kiểm 7 thứ: front-matter hợp lệ · tài liệu lỗi thời theo `covers` · liên kết tương đối
không hỏng · `covers` trỏ tới đường dẫn có thật · `owns` không trùng · con trỏ `📌 Nguồn đầy đủ` hợp
lệ · fence ``` cân bằng. Cộng thêm một cảnh báo liệt kê file mã nguồn chưa tài liệu nào mô tả.

---

## Liên quan

- [Mục lục bộ tài liệu](../README.md) — điểm vào, ba lối đọc theo vai trò
- [Sổ đăng ký nguồn sự thật](nguon-su-that.md) — ai sở hữu bảng nào, ai đang tham chiếu, sự thật nào chưa có chủ
- [Bản đồ code ↔ tài liệu](ban-do-code-tai-lieu.md) — bảng tra ngược đầy đủ tới từng file, sinh tự động
- [BV10 Phụ thuộc module và tra cứu](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) — "tôi cần sửa X thì mở file nào" ở tầng mã nguồn
- [ĐG03 Lộ trình sửa lỗi và nâng cấp](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) — việc cần làm tiếp, ảnh hưởng tới tài liệu nào
- [99-luu-tru/README.md](../99-luu-tru/README.md) — nơi tài liệu lỗi thời đi về, và cảnh báo kèm theo
