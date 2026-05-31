import { getOrCreateBrowser, getActivePage, type Page } from "@utils/PlaywrightBrowser";
import { logger } from "@utils/logger";
import * as path from "node:path";
import * as fs from 'node:fs/promises';
import { RPAGuardrails } from "@security/RPAGuardrails";
import { HITLGuard } from "@security/HITLGuard";
import { z } from "zod";
import { memoryEvents } from "../../memory/MemoryEventBus";

const ReplyZaloSchema = z.object({
  targetName: z.string().optional(),
  message: z.string().min(1, "message is required"),
  bypassHITL: z.boolean().optional().default(false),
});

export const metadata = {
  name: "reply_zalo_rpa",
  search_keywords: ["reply_zalo_rpa", "reply zalo rpa", "rep zalo", "trả lời zalo", "nhắn tin zalo"],
  description: "[ASK_FIRST] Reply to a Zalo conversation. If targetName is provided, searches and replies to that contact. If targetName is omitted, replies to the currently active or most recent conversation in the Zalo Web list.",
  kit: "SOCIAL_KIT",
  requires_hitl: true,
  parameters: {
    type: "object",
    properties: {
      targetName: {
        type: "string",
        description: "Optional recipient name. If omitted, LIVA will reply to the currently open chat or the most recent chat in the sidebar."
      },
      message: {
        type: "string",
        description: "[LOCALIZED] Reply message content. Roleplay as the user and write naturally."
      },
      bypassHITL: {
        type: "boolean",
        description: "Whether to bypass HITL approval. Default is false."
      }
    },
    required: ["message"]
  }
};

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = ReplyZaloSchema.safeParse(rawArgs);
  if (!parsed.success) {
    throw new Error(`[ValidationError] Invalid input: ${parsed.error.issues.map(i => i.message).join("; ")}`);
  }
  const args = parsed.data;

  let page: Page | null = null;
  try {
    const targetName = args.targetName || "active conversation";
    
    // RPAGuardrails Pre-Action Check
    const guardCheck = RPAGuardrails.preActionCheck(
      "reply_zalo_rpa", "send_message", targetName, args.message
    );
    if (!guardCheck.proceed) {
      return `[BẢO MẬT / SECURITY] Hành động bị chặn: ${guardCheck.warnings.join(", ")}`;
    }
    
    const safeMessage = guardCheck.filteredContent.includes("#Liva")
        ? guardCheck.filteredContent
        : `${guardCheck.filteredContent} • #Liva`;

    const livaProfileDir = path.resolve(process.cwd(), "data", "liva_zalo_profile");
    await fs.mkdir(livaProfileDir, { recursive: true });

    // Open headless browser first to inspect state
    let browserContextInfo = await getOrCreateBrowser("zalo", true);
    let context = browserContextInfo.context;
    page = await getActivePage(context, "zalo.me");
    
    if (!page.url().includes("zalo.me")) {
      logger.info(`[reply_zalo_rpa] Đang điều hướng đến Zalo Web...`);
      await page.goto("https://chat.zalo.me/", { waitUntil: "domcontentloaded", timeout: 60000 });
    } else {
      await page.bringToFront().catch(() => {});
    }

    // Check login status
    const isLoginPage = await page.evaluate(() => {
      const text = document.body.innerText.toLowerCase();
      return text.includes("mã qr") || text.includes("với số điện thoại") || text.includes("đăng nhập");
    }).catch(() => false);

    if (isLoginPage || page.url().includes("login")) {
      logger.info(`[reply_zalo_rpa] Yêu cầu đăng nhập, chuyển sang chế độ headful...`);
      memoryEvents.emit("rpa_auth_required", {
        channel: "zalo",
        message: "Zalo Web chưa đăng nhập. Vui lòng quét mã QR."
      });
      browserContextInfo = await getOrCreateBrowser("zalo", false);
      context = browserContextInfo.context;
      page = await getActivePage(context, "zalo.me");
      await page.goto("https://chat.zalo.me/", { waitUntil: "domcontentloaded", timeout: 60000 }).catch(() => {});
      await page.bringToFront().catch(() => {});
      return `[AuthRequired]: Zalo Web chưa đăng nhập. Vui lòng đăng nhập bằng mã QR trong cửa sổ trình duyệt vừa hiện lên!`;
    }

    let detectedName = args.targetName || "";

    if (!args.targetName) {
      // Auto-detect currently active or top chat
      logger.info(`[reply_zalo_rpa] Đang tự động dò tìm hội thoại đang hoạt động...`);
      await new Promise(r => setTimeout(r, 2000));
      
      const chatInfo = await page.evaluate(() => {
        // Try to find selected or active conversation item
        const activeItem = document.querySelector(
          '[class*="msg-item"][class*="active"], [class*="conv-item"][class*="active"], [class*="active-item"], [class*="msg-item"].selected, [class*="conv-item"].selected'
        );
        if (activeItem) {
          const nameEl = activeItem.querySelector('[class*="name" i], [class*="title" i], [class*="friend" i], .truncate');
          const nameText = nameEl ? (nameEl as HTMLElement).innerText.trim() : "";
          if (nameText) return { name: nameText, alreadyActive: true };
        }
        
        // Fallback: Click first conversation in the sidebar list
        const firstItem = document.querySelector('[class*="msg-item"], [class*="conv-item"], [class*="list-item"]');
        if (firstItem) {
          const nameEl = firstItem.querySelector('[class*="name" i], [class*="title" i], [class*="friend" i], .truncate');
          const nameText = nameEl ? (nameEl as HTMLElement).innerText.trim() : "";
          if (nameText) {
            (firstItem as HTMLElement).click();
            return { name: nameText, alreadyActive: false };
          }
        }
        return { name: "", alreadyActive: false };
      });

      if (chatInfo.name) {
        detectedName = chatInfo.name;
        logger.info(`[reply_zalo_rpa] Dò thấy hội thoại: "${detectedName}" (Active: ${chatInfo.alreadyActive})`);
        if (!chatInfo.alreadyActive) {
          await new Promise(r => setTimeout(r, 1500));
        }
      } else {
        throw new Error("Không phát hiện cuộc hội thoại hoạt động nào trong danh sách.");
      }
    } else {
      // Search for specific contact
      logger.info(`[reply_zalo_rpa] Đang tìm kiếm người nhận: ${args.targetName}`);
      const searchBoxSelector = "#contact-search-input, input[placeholder*='Tìm kiếm'], input[placeholder*='Search']";
      await page.waitForSelector(searchBoxSelector, { timeout: 15000 });
      await page.click(searchBoxSelector);
      
      // Ctrl+A -> Backspace to clear
      await page.keyboard.down("Control");
      await page.keyboard.press("A");
      await page.keyboard.up("Control");
      await page.keyboard.press("Backspace");
      await page.keyboard.type(args.targetName, { delay: 50 });
      await new Promise((r) => setTimeout(r, 2000));

      const isNotFound = await page.evaluate(() => {
        const text = document.body.innerText;
        return text.includes("Không tìm thấy kết quả") || text.includes("Không tìm thấy liên hệ");
      });
      if (isNotFound) {
        throw new Error(`Không tìm thấy người nhận "${args.targetName}" trong Zalo.`);
      }

      // Select contact matching 100%
      const searchResult = await page.evaluate((target) => {
        const els = Array.from(document.querySelectorAll('[class*="name" i], [class*="title" i], [class*="friend" i], .truncate'));
        for (const el of els) {
          const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : "";
          if (text.toLowerCase() === target.toLowerCase()) {
            const clickable = el.closest('div[role="button"], [class*="msg-item"], [class*="conv-item"], [class*="list-item"]');
            if (clickable) {
              (clickable as HTMLElement).click();
            } else {
              (el as HTMLElement).click();
            }
            return { clicked: true };
          }
        }
        return { clicked: false };
      }, args.targetName.trim());

      if (!searchResult.clicked) {
        throw new Error(`Không tìm thấy tên khớp chính xác với "${args.targetName}" trong kết quả tìm kiếm.`);
      }
      await new Promise((r) => setTimeout(r, 2000));
    }

    // Take screenshot for HITL confirmation
    const screenshotDir = path.resolve(process.cwd(), "..", "liva-ui", "public", "screenshots");
    await fs.mkdir(screenshotDir, { recursive: true });
    const screenshotFullPath = path.join(screenshotDir, "zalo_reply.png");
    await page.screenshot({ path: screenshotFullPath });

    if (!args.bypassHITL) {
      // Request HITL Approval
      const approved = await HITLGuard.requestApproval({
        toolName: "reply_zalo_rpa",
        args: { targetName: detectedName, message: safeMessage },
        reason: `Trả lời tin nhắn Zalo gửi đến "${detectedName}" với nội dung: "${safeMessage}"`,
        image: `/screenshots/zalo_reply.png?t=${Date.now()}`
      });

      if (!approved) {
        return "Lỗi: Người dùng từ chối gửi tin nhắn Zalo này.";
      }
    }

    // Selector Guard: check if chat box is present before typing
    const chatBoxSelector = "#richInput";
    const chatBoxPresent = await page.waitForSelector(chatBoxSelector, { timeout: 5000 }).then(() => true).catch(() => false);
    
    if (!chatBoxPresent) {
      const isStillLoginPage = await page.evaluate(() => {
        const text = document.body.innerText.toLowerCase();
        return text.includes("mã qr") || text.includes("với số điện thoại") || text.includes("đăng nhập");
      }).catch(() => false);
      
      if (isStillLoginPage || page.url().includes("login")) {
        logger.info(`[reply_zalo_rpa] Yêu cầu đăng nhập phát hiện bởi Selector Guard.`);
        memoryEvents.emit("rpa_auth_required", {
          channel: "zalo",
          message: "Zalo Web bị mất phiên hoặc chưa đăng nhập. Vui lòng quét mã QR."
        });
        browserContextInfo = await getOrCreateBrowser("zalo", false);
        context = browserContextInfo.context;
        page = await getActivePage(context, "zalo.me");
        await page.goto("https://chat.zalo.me/", { waitUntil: "domcontentloaded", timeout: 60000 }).catch(() => {});
        await page.bringToFront().catch(() => {});
        return `[AuthRequired]: Zalo Web chưa đăng nhập. Vui lòng đăng nhập bằng mã QR trong cửa sổ trình duyệt vừa hiện lên!`;
      }
      throw new Error("Không tìm thấy hộp thoại soạn tin nhắn (#richInput) trên Zalo.");
    }
    
    await page.click(chatBoxSelector);
    await page.keyboard.type(safeMessage, { delay: 50 });
    await page.keyboard.press("Enter");
    await new Promise((r) => setTimeout(r, 2000));

    // Minimize browser
    try {
      const cdp = await page.context().newCDPSession(page);
      const { windowId } = await cdp.send('Browser.getWindowForTarget') as { windowId: number };
      await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
    } catch {}

    RPAGuardrails.logAction("reply_zalo_rpa", "message_sent", detectedName, safeMessage.substring(0, 50), false, "allowed");
    return `Thành công: Đã trả lời Zalo cho ${detectedName}. Trình duyệt đã được thu nhỏ.`;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
      return `[HỆ THỐNG]: Yêu cầu gửi tin nhắn bị từ chối hoặc quá thời gian phê duyệt.`;
    }
    return `Lỗi Reply Zalo RPA: ${errMsg}`;
  }
};
