import puppeteer, { Browser, Page } from "puppeteer";

// Biến toàn cục để giữ trạng thái trình duyệt ở chế độ nền (Stateful)
let browserInstance: Browser | null = null;
let pageInstance: Page | null = null;

export const metadata = {
  name: "web_browser",
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
    console.log(`[Skill: web_browser] Nhận lệnh: ${args.action}`);

    // 1. Đóng trình duyệt
    if (args.action === "close") {
      if (browserInstance) {
        await browserInstance.close();
        browserInstance = null;
        pageInstance = null;
        return "Đã đóng trình duyệt thành công và giải phóng bộ nhớ.";
      }
      return "Trình duyệt đang không mở.";
    }

    // 2. Khởi tạo trình duyệt nếu chưa mở
    if (!browserInstance || !pageInstance) {
      console.log(`[Skill: web_browser] Đang khởi động Puppeteer Browser...`);
      browserInstance = await puppeteer.launch({
        headless: false, // Để false để bạn (User) THẤY TẬN MẮT việc LIVA tự thao web. Cảm giác rất "WOW"! Set lại thành 'true' nếu muốn ẩn hoàn toàn.
        defaultViewport: null,
        args: ["--start-maximized"],
      });
      const pages = await browserInstance.pages();
      pageInstance =
        pages.length > 0 ? pages[0] : await browserInstance.newPage();

      // Bypass một số cơ chế bot detection cơ bản
      await pageInstance.setUserAgent(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
      );
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
          // Lấy toàn bộ chữ hiển thị trên body, cắt ở 5000 ký tự để LIVA không bị quá tải token context.
          return document.body.innerText.substring(0, 5000);
        });

        return `[TRẠNG THÁI] Đã tải xong trang: "${pageTitle}"\n[URL HIỆN TẠI]: ${pageInstance.url()}\n=====================\n[NỘI DUNG VĂN BẢN (Preview)]:\n${content}\n=====================\n(Gợi ý cho LIVA: Nếu LIVA muốn tìm nút bấm, hãy suy luận theo văn bản hiển thị và gọi hàm click, hoặc gọi extract với selector).`;
      }

      case "click": {
        if (!args.selector)
          return "Lỗi: Thiếu tham số `selector` cho hành động click.";
        await pageInstance.waitForSelector(args.selector, { timeout: 10000 });

        await pageInstance.click(args.selector);

        // Mẹo: Click có thể dẫn tới chuyển trang, chúng ta chờ hệ thống mạng idle một lát (nhưng không bắt buộc để tránh treo)
        await pageInstance
          .waitForNavigation({ waitUntil: "networkidle2", timeout: 3000 })
          .catch(() => {});

        return `[TRẠNG THÁI] Đã Click thành công vào: "${args.selector}".\nLưu ý: Trang có thể đã chuyển hướng. Bạn có thể dùng hành động 'extract' (không cần selector) để lấy lại trạng thái trang mới nhất.`;
      }

      case "type": {
        if (!args.selector || !args.text)
          return "Lỗi: Thiếu tham số `selector` hoặc `text` cho hành động type.";
        await pageInstance.waitForSelector(args.selector, { timeout: 10000 });

        // Xóa nội dung điền sẵn nếu có
        await pageInstance.click(args.selector, { clickCount: 3 });
        await pageInstance.keyboard.press("Backspace");

        // Gõ chậm để qua mặt các script chống bot
        await pageInstance.type(args.selector, args.text, { delay: 50 });
        return `[TRẠNG THÁI] Đã hoàn tất việc gõ chữ "${args.text}" vào vị trí "${args.selector}".`;
      }

      case "extract": {
        const pageTitle = await pageInstance.title();
        let extractedText = "";

        if (args.selector) {
          extractedText = (await pageInstance.evaluate((sel: string) => {
            const el = document.querySelector(sel) as HTMLElement;
            return el ? el.innerText : "Không tìm thấy CSS selector này.";
          }, args.selector)) as string;
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
