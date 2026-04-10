# LIVA - Desktop AI Assistant

Hệ thống Trợ lý ảo Desktop (Live2D) với năng lực suy luận linh hoạt nhờ kiến trúc hệ thống 3 phân lớp.

## Tổng quan dự án

Dự án được chia thành 3 modules chính để đóng gói tính năng và dễ dàng mở rộng:

1. **`liva-ui`** `(Vue 3 / Electron)`: 
   - Ứng dụng Desktop cho phép khởi chạy nhân vật Live2D hiển thị trong suốt (tàng hình nền) dưới góc màn hình.
   - Chịu trách nhiệm hiển thị giao diện, animation, và tương tác trực tiếp với người dùng.

2. **`openclaw-gateway`** `(Node.js / TypeScript)`:
   - Đóng vai trò là hệ thống Não bộ Trung gian (Gateway) quản lý kỹ năng (Skills), quản lý trí nhớ (Memory) và luồng hội thoại.
   - Giao tiếp với các dịch vụ bên thứ 3 như Zalo, Google Drive, Email...

3. **`liva-ai-engine`** `(Python)`: 
   - Động cơ AI Core xử lý suy luận (Inference), chạy các mô hình LLM lớn, hoặc tích hợp API bên ngoài tùy cấu hình.

## Yêu cầu hệ thống
- **Node.js** (Khuyến nghị bản v18+)
- **Python** (Bản v3.10+) 
- Npm / Yarn (để quản lý package cho Node)
- Các cấu hình API Key lưu tại `.env` trong thư mục `openclaw-gateway`.

## Hướng dẫn cài đặt và chạy

1. **Cài đặt các dependencies**:
   - Vào `liva-ui`: chạy `npm install`
   - Vào `openclaw-gateway`: chạy `npm install`
   - Vào `liva-ai-engine`: Tạo thư mục virtual environment nếu chưa có `python -m venv venv`, sau đó cài đặt bằng terminal `pip install -r requirements.txt`.

2. **Khởi chạy Hệ thống tự động**:
   - Ở thư mục gốc (`e:\Project\LIVA`), nhấn đúp chuột vào file `start_all.bat`.
   - File này sẽ tự động khởi động:
     - Khởi động **Python Engine** (AI).
     - Khởi động **Gateway Gateway** (Node.js).
     - Khởi chạy Vite Dev Server và mở App Electron cho **LIVA UI**.

## Code Formatting

Tính nhất quán của mã nguồn đã được chuẩn hoá với:
- Dùng `Prettier` định dạng cho các phần UI và Gateway (chạy lệnh `npx prettier --write .` trong thư mục tương ứng).
- Dùng `Black` để định dạng cho Python AI Engine (chạy lệnh `black .` trong môi trường ảo của thư mục engine).

## Ghi chú Debug

- Nếu có lỗi treo cổng mạng, system sẽ tự diệt các tiến trình NodeJS, Python cũ khi chạy lại bằng `start_all.bat`.
- Các file log rác và script test đã được dời vào thư mục `scripts` và `tests` bên trong luồng `openclaw-gateway`.
