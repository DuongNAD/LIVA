import { logger } from "../utils/logger";
export const metadata = {
  name: "read_recent_emails",
  search_keywords: ["read_recent_emails","read recent emails"],
  description:
    "Truy xuất (Fetch) các email mới nhất từ hộp thư của người dùng. Kỹ năng này cung cấp nội dung email thô để AI đọc và tóm tắt.",
  parameters: {
    type: "object",
    properties: {},
    required: [],
  },
};

export const execute = async (): Promise<string> => {
  try {
    logger.info(
      `[Skill: read_recent_emails] Đang kết nối tới máy chủ Mail (Connecting to Mail Server)...`,
    );

    // Mô phỏng độ trễ của mạng (Simulate network latency)
    await new Promise((resolve) => setTimeout(resolve, 1500));

    // Đây là dữ liệu giả lập (Mock data) để AI thực hành tóm tắt.
    // Sau này ta sẽ thay bằng API thật của Gmail/Outlook.
    const mockEmails = `
Danh sách Email mới nhất (Recent Emails):
1. Từ: github-noreply@github.com - Tiêu đề: Cảnh báo lỗ hổng bảo mật (Vulnerability Alert) trong thư viện dự án CyberSentinel.
2. Từ: hr@pathtech.vn - Tiêu đề: Thư mời phỏng vấn vị trí Web Backend Intern - Vòng kỹ thuật.
3. Từ: daotao@fpt.edu.vn - Tiêu đề: Thông báo lịch bảo vệ dự án học kỳ này.
        `;

    return mockEmails.trim();
  } catch (error: any) {
    return `Lỗi khi lấy email (Fetch error): ${error.message}`;
  }
};
