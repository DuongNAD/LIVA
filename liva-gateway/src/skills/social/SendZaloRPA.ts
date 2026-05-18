import { getOrCreateBrowser, getActivePage, type Page, type BrowserContext } from "@utils/PlaywrightBrowser";
import { logger } from "@utils/logger";
import * as path from "node:path";
import * as fs from 'node:fs/promises';
import { RPAGuardrails } from "@security/RPAGuardrails";
import { HITLGuard } from "@security/HITLGuard";

export const metadata = {
  name: "send_zalo_rpa",
  search_keywords: ["send_zalo_rpa","send zalo rpa","gửi","nhắn tin","zalo","nhắn zalo","zalo web","chat zalo","nhắn bạn","nhắn mẹ","nhắn cho"],
  description:
    "[ASK_FIRST] DEFAULT Zalo messaging tool. Browser Automation (RPA) to send a personal message to FRIENDS, FAMILY, or CONTACTS via Zalo Web. Use this when user says 'nhắn zalo cho...', 'nhắn tin cho bạn/mẹ/anh...'. NEVER use send_zalo_bot for this — that tool is ONLY for self-notifications.",
  parameters: {
    type: "object",
    properties: {
      targetName: {
        type: "string",
        description:
          "Tên người nhận (BẮT BUỘC bỏ các đại từ sở hữu như 'tôi', 'của tôi'. Ví dụ: 'mẹ tôi' => 'Mẹ', 'vợ anh' => 'Vợ', 'bạn Hùng' => 'Hùng').",
      },
      message: {
        type: "string",
        description: "HIỂU LÝ DO, TUYỆT ĐỐI KHÔNG COPY NGUYÊN VĂN CÂU LỆNH CỦA NGƯỜI DÙNG! Bạn phải ĐÓNG VAI người dùng và VIẾT LẠI tin nhắn một cách tự nhiên, giống y như người dùng đang chat with bạn bè/người thân. Ví dụ: Nếu user bảo 'Nhắc em Khánh 6h30 hỗ trợ thi', bạn phải gửi: 'Khánh ơi chiều 6h30 hỗ trợ anh thi nhé!'. Dùng ngôn ngữ thân thiện, đời thường.",
      },
    },
    required: ["targetName", "message"],
  },
};

// Singleton background browser to prevent popping up new window every time
let globalContext: BrowserContext | null = null;

export const execute = async (args: {
  targetName: string;
  message: string;
}): Promise<string> => {
  let page: Page | null = null;
  try {
    // ====== RPAGuardrails Pre-Action Check ======
    const guardCheck = RPAGuardrails.preActionCheck(
      "send_zalo_rpa", "send_message", args.targetName, args.message
    );
    if (!guardCheck.proceed) {
      return `[BẢO MẬT / SECURITY] Hành động bị chặn: ${guardCheck.warnings.join(", ")} / Action blocked: ${guardCheck.warnings.join(", ")}`;
    }
    
    const safeMessage = guardCheck.filteredContent.includes("#Liva")
        ? guardCheck.filteredContent
        : `${guardCheck.filteredContent} • #Liva`;
    if (guardCheck.warnings.length > 0) {
      logger.warn(`[RPA Zalo/Guard] Cảnh báo: ${guardCheck.warnings.join(" | ")}`);
    }

    // ====== BƯỚC 1: Khởi tạo trình duyệt & thực hiện tìm kiếm trước để chụp ảnh minh chứng ======
    const livaProfileDir = path.resolve(
      process.cwd(),
      "data",
      "liva_zalo_profile",
    );
    await fs.mkdir(livaProfileDir, { recursive: true });

    if (!globalContext) {
      logger.info(
        `[RPA Zalo] Khởi động trình duyệt Zalo nền (First time launch)...`,
      );
      const { context } = await getOrCreateBrowser("zalo");
      globalContext = context;
    } else {
      logger.info(`[RPA Zalo] Dùng lại trình duyệt đang mở (Reusing browser)...`);
    }

    // Lấy tất cả trang hiện có, xem có trang zalo nào không
    page = await getActivePage(globalContext, "zalo.me");
    
    if (!page.url().includes("zalo.me")) {
      logger.info(`[RPA Zalo] Đang điều hướng đến Zalo Web...`);
      await page.goto("https://chat.zalo.me/", {
        waitUntil: "domcontentloaded",
        timeout: 60000,
      });
    } else {
      await page.bringToFront();
    }

    // Kiểm tra xem đã đăng nhập chưa
    const isLoginPage = await page
      .evaluate(() => {
        const text = document.body.innerText.toLowerCase();
        return text.includes("mã qr") || text.includes("với số điện thoại") || text.includes("đăng nhập");
      })
      .catch(() => false);

    if (isLoginPage || page.url().includes("login")) {
      logger.info(
        `[RPA Zalo] ⚠️ Yêu cầu đăng nhập (Scan QR / Login required).`,
      );
      return `[Yêu Cầu Từ Hệ Thống / System Request]: Zalo Web chưa được đăng nhập. Bạn hãy mở Cửa sổ Trình duyệt Zalo đang được LIVA bật lên và quét mã QR để kích hoạt RPA nhé! (Không đóng trình duyệt sau khi quét) / Zalo Web is not logged in. Please open the Zalo browser window spawned by LIVA and scan the QR code to activate RPA! (Do not close the browser after scanning)`;
    }

    logger.info(`[RPA Zalo] Bắt đầu tìm người nhận: ${args.targetName}`);

    // Chờ thanh tìm kiếm xuất hiện
    const searchBoxSelector = "#contact-search-input, input[placeholder*='Tìm kiếm'], input[placeholder*='Search']";
    await page.waitForSelector(searchBoxSelector, { timeout: 30000 });

    // Click vào ô tìm kiếm và gõ tên/số điện thoại
    await page.click(searchBoxSelector);
    
    // Clear ô tìm kiếm trước (nhấn Ctrl A + Backspace)
    await page.keyboard.down("Control");
    await page.keyboard.press("A");
    await page.keyboard.up("Control");
    await page.keyboard.press("Backspace");

    // Gõ tên người nhận
    await page.keyboard.type(args.targetName, { delay: 100 });

    // Đợi kết quả tìm kiếm hiện ra
    await new Promise((r) => setTimeout(r, 2500));

    // ====== BƯỚC 2: Chụp ảnh danh sách tìm được và lưu vào thư mục public của UI ======
    const screenshotDir = path.resolve(process.cwd(), "..", "liva-ui", "public", "screenshots");
    await fs.mkdir(screenshotDir, { recursive: true });
    const screenshotFullPath = path.join(screenshotDir, "zalo_search.png");
    
    // Chụp ảnh tập trung (focus) vào vùng gợi ý tìm kiếm để người dùng dễ chọn
    let clip: { x: number; y: number; width: number; height: number } | undefined = undefined;
    try {
      const searchBox = await page.$(searchBoxSelector);
      if (searchBox) {
        const box = await searchBox.boundingBox();
        if (box) {
          clip = {
            x: Math.max(0, box.x - 20),
            y: Math.max(0, box.y - 15),
            width: 380,
            height: 550
          };
          logger.info(`[RPA Zalo] 📸 Clipping screenshot at x:${clip.x}, y:${clip.y}, w:${clip.width}, h:${clip.height}`);
        }
      }
    } catch (e) {
      logger.warn(`[RPA Zalo] Không lấy được bounding box của ô tìm kiếm, chụp toàn màn hình làm dự phòng: ${e}`);
    }

    if (clip) {
      await page.screenshot({ path: screenshotFullPath, clip });
    } else {
      await page.screenshot({ path: screenshotFullPath });
    }
    logger.info(`[RPA Zalo] 📸 Đã chụp ảnh kết quả tìm kiếm danh bạ tại: ${screenshotFullPath}`);

    // ====== BƯỚC 3: Xác nhận thông tin với ảnh chụp trực quan từ Zalo ======
    const approved = await HITLGuard.requestApproval({
      toolName: "send_zalo_rpa",
      args: { targetName: args.targetName, message: safeMessage },
      reason: `Gửi tin Zalo đến / Send Zalo to "${args.targetName}" với nội dung / with content: "${safeMessage}"`,
      image: `/screenshots/zalo_search.png?t=${Date.now()}`
    });

    if (!approved) {
      return "Lỗi / Error: Người dùng đã từ chối gửi tin nhắn Zalo này. / User declined to send this Zalo message.";
    }

    // ====== BƯỚC 4: Tiến hành Click chọn và gửi tin nhắn (Sau khi được duyệt) ======
    // ==========================================
    // SAFETY CHECK 1: Kiểm tra có tìm thấy danh bạ hay không
    // ==========================================
    const isNotFound = await page.evaluate(() => {
       const text = document.body.innerText;
       return text.includes("Không tìm thấy kết quả") || text.includes("Không tìm thấy liên hệ");
    });
    
    if (isNotFound) {
       throw new Error(`[Lỗi / Error]: Không tìm thấy người nhận "${args.targetName}" trong danh bạ Zalo. Vui lòng đọc đúng tên bạn bè! / Recipient "${args.targetName}" not found in Zalo contacts. Please make sure the contact name is correct!`);
    }

    // ==========================================
    // SAFETY CHECK 2: Cào tên trong DOM và Click chính xác, hoặc trả về danh sách gợi ý SẠCH
    // ==========================================
    const searchResult = await page.evaluate((target) => {
        const els = Array.from(document.querySelectorAll('[class*="name" i], [class*="title" i], [class*="friend" i], .truncate'));
        const suggestions = new Set<string>();
        const targetWords = target.toLowerCase().split(" ").filter((w: string) => w.length >= 2);
        
        for (const el of els) {
            const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : "";
            
            if (text && text.length >= 2 && text.length <= 40 && !text.includes('\n')) {
                const textLower = text.toLowerCase();
                
                if (textLower === target.toLowerCase()) {
                    const clickable = el.closest('div[role="button"], [class*="msg-item"], [class*="conv-item"], [class*="list-item"]');
                    if (clickable) {
                        (clickable as HTMLElement).click();
                        return { clicked: true, suggestions: [] };
                    } else {
                        (el as HTMLElement).click();
                        return { clicked: true, suggestions: [] };
                    }
                }
                
                let isRelated = targetWords.some((w: string) => textLower.includes(w));
                if (textLower.includes(target.toLowerCase()) || target.toLowerCase().includes(textLower)) {
                    isRelated = true;
                }
                
                if (isRelated) {
                    suggestions.add(text);
                }
            }
        }
        return { clicked: false, suggestions: Array.from(suggestions).slice(0, 5) };
    }, args.targetName.trim());

    if (!searchResult.clicked) {
        throw new Error(`[Lỗi Xác Nhận / Matching Error]: Không tìm thấy tên khớp 100% với "${args.targetName}". Các gợi ý gần giống: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "Không có"}]. / No exact contact name matching "${args.targetName}" was found. Suggested names: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "None"}].`);
    }

    logger.info(`[RPA Zalo] Đang soạn tin nhắn...`);
    await new Promise((r) => setTimeout(r, 2000));

    const chatBoxSelector = "#richInput";
    await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
    await page.click(chatBoxSelector);

    // Gõ nội dung tin nhắn
    await page.keyboard.type(safeMessage, { delay: 50 });

    // Nhấn Enter để gửi
    await page.keyboard.press("Enter");
    logger.info(`[RPA Zalo] Đã gửi tin nhắn (Message dispatched)!`);

    // Chờ một xíu để tin cập nhật
    await new Promise((r) => setTimeout(r, 2000));
    
    // Thu nhỏ cửa sổ trình duyệt sau khi làm xong
    try {
      const cdp = await page.context().newCDPSession(page);
      const { windowId } = await cdp.send('Browser.getWindowForTarget') as any;
      await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
    } catch (e) { void e; }

    RPAGuardrails.logAction("send_zalo_rpa", "message_sent", args.targetName, safeMessage.substring(0, 50), false, "allowed");
    return `Hoàn tất / Success: Đã gửi tin nhắn Zalo cho / Successfully sent Zalo message to ${args.targetName}. Trình duyệt đã được thu nhỏ ngầm. / The background browser has been minimized.`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
      return `[HỆ THỐNG BẢO MẬT TỪ CHỐI / SECURITY REFUSED]: Yêu cầu gửi tin nhắn bị từ chối hoặc quá thời gian phê duyệt (300s). / Message request was rejected or approval timed out (300s).`;
    }
    return `Lỗi Zalo RPA / Zalo RPA Error: ${errMsg}`;
  }
};
