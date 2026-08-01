---
title: "Tầm nhìn LIVA — trợ lý cá nhân kiểu JARVIS"
updated: 2026-07-30
commit: 3688b5f
status: living
owns:
  - tam-nhin-jarvis
  - nguyen-tac-san-pham-jarvis
covers:
  - README.md
  - docs/_data/capabilities.json
---
# Tầm nhìn LIVA — trợ lý cá nhân kiểu JARVIS

[⬆ Mục lục](../README.md) · [Ma trận năng lực](../_generated/ma-tran-nang-luc.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

## 1. Tuyên bố sản phẩm

LIVA là một trợ lý cá nhân **local-first** cho Windows: nghe, nói, nhìn, ghi nhớ và
thực hiện hành động thay người dùng trong các ranh giới do chính người dùng kiểm
soát.

“Kiểu JARVIS” trong tài liệu này không có nghĩa là tuyên bố đạt AGI. Nó được chuyển
thành tám năng lực có thể xây dựng và nghiệm thu:

1. **Hiện diện hội thoại** — đánh thức, đối thoại tự nhiên, barge-in và phản hồi nhanh.
2. **Nhận thức đa phương thức** — giọng nói, màn hình, trạng thái ứng dụng và thiết bị.
3. **Bộ nhớ cá nhân** — nhớ qua nhiều phiên, biết nguồn, biết quên và biết xử lý mâu thuẫn.
4. **Hành động có kiểm soát** — công cụ, ứng dụng, nhắn tin và thiết bị vật lý.
5. **Chủ động đúng lúc** — tự gợi ý khi có tín hiệu đủ mạnh, giữ im lặng khi không cần.
6. **Cá nhân hóa** — ngôn ngữ, giọng, thói quen và ưu tiên của từng người dùng.
7. **Hiện thân** — avatar và trạng thái trực quan phản ánh LIVA đang nghe, nghĩ hay hành động.
8. **Liên tục giữa thiết bị** — một danh tính, một bộ nhớ có phạm vi và quyền rõ ràng.

Trạng thái thực tế của từng năng lực không được viết lặp trong file này.

> 📌 Nguồn đầy đủ: [Ma trận năng lực LIVA → JARVIS](../_generated/ma-tran-nang-luc.md)

## 2. Nguyên tắc sản phẩm

### 2.1 Local-first, cloud-optional

Thoại, vision, memory và hành động lõi phải tiếp tục hoạt động khi mất mạng. Dịch vụ
cloud chỉ là adapter tùy chọn và không được trở thành điều kiện boot của cognitive
runtime.

### 2.2 Planner không có quyền thi hành

LLM được phép đề xuất hành động; `PolicyEngine` mới quyết định tự chạy, hỏi xác nhận,
từ chối hoặc mô phỏng. Mọi hành động không hoàn tác được phải qua cổng xác nhận hoặc
một automation rule đã được người dùng cấp quyền trước.

### 2.3 Chủ động không đồng nghĩa với giám sát bí mật

Không thu raw keystroke mặc định. Mọi sensor chủ động phải có:

- consent riêng;
- chỉ báo trực quan khi hoạt động;
- retention rõ ràng;
- nút tắt tức thì;
- khả năng xem và xóa dữ liệu đã sinh.

### 2.4 Trung thực hơn là “trông thông minh”

Không đo được thì trả `unknown`. Tool không nối thì báo chưa nối. Hành động thất bại
không được biến thành câu trả lời thành công. Tính năng `experimental`, `partial`,
`missing` hoặc `blocked` không được quảng cáo như hành vi mặc định.

### 2.5 Đường phản xạ tách khỏi đường suy luận

Wake word, dừng TTS, barge-in, âm lượng và media không được trả chi phí một lượt LLM.
LLM chỉ tham gia khi yêu cầu cần suy luận hoặc chọn giữa các công cụ không thể phân
biệt bằng luật tất định.

### 2.6 Mọi hành động đều giải thích và kiểm toán được

Một hành động phải trả lời được:

- ai hoặc tín hiệu nào khởi tạo;
- dữ liệu nào được dùng;
- policy nào cho phép;
- điều gì đã thực sự xảy ra;
- có thể hoàn tác bằng cách nào.

## 3. Tiêu chí “đủ giống JARVIS”

LIVA đạt mốc trợ lý cá nhân v1 khi một phiên duy nhất có thể:

1. đánh thức đáng tin cậy;
2. hiểu câu hỏi bằng giọng;
3. nhìn đúng vùng màn hình khi cần;
4. nhớ dữ kiện đã nói ở phiên trước;
5. đề xuất một hành động có giải thích;
6. xin xác nhận nếu hành động tạo side effect;
7. thực hiện và đọc lại kết quả thật;
8. ghi lại ký ức/audit đúng phạm vi;
9. dừng ngay khi người dùng chen lời hoặc tắt quyền.

Mỗi mục phải có acceptance test hoặc số đo tái lập được. Video demo không thay thế
bằng chứng kiểm thử.

## 4. Điều không ưu tiên trước mốc v1

- agent swarm;
- tự sửa mã nguồn không có review;
- fine-tune liên tục trên dữ liệu chưa quản trị;
- WebRTC cho đường desktop cùng máy;
- phục hồi Node.js/Python backend đã nghỉ hưu;
- avatar phức tạp trong khi tool, memory và consent chưa ổn định.

Các mục này có thể có giá trị về sau, nhưng không được chen trước độ tin cậy của vòng
nhận thức chính.
