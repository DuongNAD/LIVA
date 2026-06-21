# Original User Request

## Initial Request — 2026-06-21T08:16:14Z

# Teamwork Project Prompt — Draft

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Xây dựng một hệ thống Wiki dành cho LLM dựa trên Obsidian để lưu trữ, quản lý các kỹ năng (skills) và kiến thức cho LIVA. Mục tiêu là tạo ra một cơ sở dữ liệu tri thức nhất quán, kết nối qua MCP Server, giúp LIVA tra cứu thông tin chính xác và tránh tình trạng "ảo giác".

Working directory: E:\Project\LIVA\teamwork_projects\obsidian_llm_wiki
Integrity mode: benchmark

## Requirements

### R1. Obsidian Vault Setup
Thiết lập một Vault Obsidian với cấu trúc thư mục chuẩn (vd: Skills, Knowledge, Rules) và các file template mẫu chứa metadata (Frontmatter) để đảm bảo định dạng dữ liệu nhất quán khi con người hoặc LIVA tạo bài viết mới.

### R2. LIVA-Obsidian MCP Server
Xây dựng một MCP Server cho phép LIVA tương tác với Obsidian Vault. Server phải cung cấp các tool cơ bản: đọc file markdown, tìm kiếm (semantic/full-text search) trong vault, và tạo/cập nhật file. 

### R3. Infrastructure Constraints
MCP Server chỉ được phép đọc và ghi dữ liệu bên trong thư mục giới hạn của Obsidian Vault, tuyệt đối không được phép truy cập hoặc thay đổi các file nằm ngoài thư mục Vault này.

## Acceptance Criteria

### Obsidian Vault Setup
- [ ] Tồn tại các thư mục cơ bản (Skills, Knowledge, Rules) và file template markdown có chứa Frontmatter chuẩn (ví dụ: `title`, `tags`, `author`, `last_update`).
- [ ] Có script tự động kiểm tra (validate) cấu trúc Vault và sự hiện diện của các template này.

### LIVA-Obsidian MCP Server
- [ ] Có một automated test script (hoặc test suite) có thể khởi động MCP server, kết nối thành công và gọi một thao tác tìm kiếm (search) trả về kết quả chính xác từ dữ liệu mẫu trong Vault.
- [ ] Test script có thể gọi thao tác tạo mới một file markdown thông qua MCP Server và file đó được ghi thành công vào đúng thư mục trong Vault.
- [ ] Test script xác nhận MCP Server sẽ báo lỗi (hoặc từ chối) nếu có yêu cầu đọc/ghi một file nằm ngoài đường dẫn của Vault.
