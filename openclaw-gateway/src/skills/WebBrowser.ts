import { logger } from "../utils/logger";

// Biến toàn cục để giữ trạng thái trình duyệt ở chế độ nền (Stateful)
let browserContext: BrowserContext | null = null;
let pageInstance: Page | null = null;

export const metadata = {
  name: "web_browser",
  search_keywords: ["web_browser","web browser"],
  description:
    "Công cụ điều khiển trình duyệt web ngầm định theo cơ chế Tự trị (Agentic). Gồm các hành động: navigate, click, type, extract, close. Giúp LIVA đọc HTML, trích xuất dữ liệu, và thao tác trên mọi website giống như con người.",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["navigate", "click", "type", "extract", "close"],
        description: "Hành động cần thực hiện trên trình duyệt.",
      },
      url: {
        type: "string",
        description:
          "Dùng cho hành động 'navigate': Đường dẫn trang web cần mở.",
      },
      selector: {
        type: "string",
        description:
          "Dùng cho hành động 'click', 'type', hoặc 'extract': query selectors của phần tử (VD: 'button.submit', '#search-input', '.article').",
      },
      text: {
        type: "string",
        description:
          "Dùng cho hành động 'type': Nội dung chữ/text cần gõ vào ô input (VD: Từ khóa cần tìm).",
      },
    },
    required: ["action"],
  },
};

export const execute = async (args: {
  action: string;
  url?: string;
  selector?: string;
  text?: string;
}): Promise<string> => {
  try {
    logger.info(`[Skill: web_browser] Nhận lệnh: ${args.action}`);

    // 1. Đóng trình duyệt
    if (args.action === "close") {
      if (browserContext) {
        await browserContext.close().catch(() => {});
        browserContext = null;
        pageInstance = null;
        return "Đã đóng trình duyệt thành công và giải phóng bộ nhớ.";
      }
      return "Trình duyệt đang không mở.";
    }

    // 2. Khởi tạo trình duyệt nếu chưa mở
    if (!browserContext || !pageInstance) {
      logger.info(`[Skill: web_browser] Đang khởi động Playwright Browser...`);
      const { context } = await getOrCreateBrowser("web_browser");
      browserContext = context;
      const pages = browserContext.pages();
      pageInstance = pages.length > 0 ? pages[0] : await browserContext.newPage();
    }

    // 3. Xử lý logic từng Action
    switch (args.action) {
      case "navigate": {
        if (!args.url)
          return "Lỗi: Thiếu tham số bắt buộc `url` cho hành động navigate.";

        let targetUrl = args.url;
        if (!targetUrl.startsWith("http")) {
          targetUrl = `https://${targetUrl}`; // Tự động fix URL lỗi
        }

        await pageInstance.goto(targetUrl, {
          waitUntil: "domcontentloaded",
          timeout: 30000,
        });

        const pageTitle = await pageInstance.title();
        const content = await pageInstance.evaluate(() => {
          return document.body.innerText.substring(0, 5000);
        });

        return `[TRẠNG THÁI] Đã tải xong trang: "${pageTitle}"\n[URL HIỆN TẠI]: ${pageInstance.url()}\n=====================\n[NỘI DUNG VĂN BẢN (Preview)]:\n${content}\n=====================\n(Gợi ý cho LIVA: Nếu LIVA muốn tìm nút bấm, hãy suy luận theo văn bản hiển thị và gọi hàm click, hoặc gọi extract với selector).`;
      }

      case "click": {
        if (!args.selector)
          return "Lỗi: Thiếu tham số `selector` cho hành động click.";
        // Playwright auto-waits for element to be actionable
        await pageInstance.locator(args.selector).click({ timeout: 10000 });

        // Chờ navigation nếu có (non-blocking)
        await pageInstance.waitForLoadState("domcontentloaded").catch(() => {});

        return `[TRẠNG THÁI] Đã Click thành công vào: "${args.selector}".\nLưu ý: Trang có thể đã chuyển hướng. Bạn có thể dùng hành động 'extract' (không cần selector) để lấy lại trạng thái trang mới nhất.`;
      }

      case "type": {
        if (!args.selector || !args.text)
          return "Lỗi: Thiếu tham số `selector` hoặc `text` cho hành động type.";
        
        // Clear existing content and type new text
        await pageInstance.locator(args.selector).fill(args.text);
        return `[TRẠNG THÁI] Đã hoàn tất việc gõ chữ "${args.text}" vào vị trí "${args.selector}".`;
      }

      case "extract": {
        const pageTitle = await pageInstance.title();
        let extractedText = "";

        if (args.selector) {
          extractedText = await pageInstance.locator(args.selector).innerText({ timeout: 10000 }).catch(() => "Không tìm thấy CSS selector này.");
        } else {
          extractedText = await pageInstance.evaluate(() =>
            document.body.innerText.substring(0, 5000),
          );
        }

        return `[TIÊU ĐỀ TRANG]: ${pageTitle}\n[DỮ LIỆU CHẾT XUẤT]: \n${extractedText}`;
      }

      default:
        return `Lỗi: Hành động '${args.action}' không được hệ thống hỗ trợ.`;
    }
  } catch (error: any) {
    return `[LỖI TRÌNH DUYỆT CỤC BỘ]: Yêu cầu thao tác thất bại với mô tả: ${error.message}`;
  }
};
