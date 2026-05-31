import { getOrCreateBrowser, getActivePage, type Page } from "@utils/PlaywrightBrowser";
import { logger } from "@utils/logger";
import * as path from "node:path";
import * as fs from 'node:fs/promises';
import { RPAGuardrails } from "@security/RPAGuardrails";
import { HITLGuard } from "@security/HITLGuard";
import { z } from "zod";
import { memoryEvents } from "../../memory/MemoryEventBus";

const ReplyMessengerSchema = z.object({
  targetName: z.string().optional(),
  message: z.string().min(1, "message is required"),
  bypassHITL: z.boolean().optional().default(false),
});

export const metadata = {
  name: "reply_messenger_rpa",
  search_keywords: ["reply_messenger_rpa", "reply messenger rpa", "rep messenger", "rep mess", "trả lời messenger"],
  description: "[ASK_FIRST] Reply to a Facebook Messenger conversation. If targetName is provided, searches and replies to that contact. If targetName is omitted, replies to the currently active or most recent conversation in the Messenger list.",
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

const MESSENGER_BASE = 'https://www.facebook.com/messages';

function isMessengerPage(url: string): boolean {
  return url.includes('facebook.com/messages');
}

function isChatThread(url: string): boolean {
  return /facebook\.com\/messages\/(e2ee\/)?t\/\d+/.test(url);
}

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = ReplyMessengerSchema.safeParse(rawArgs);
  if (!parsed.success) {
    throw new Error(`[ValidationError] Invalid input: ${parsed.error.issues.map(i => i.message).join("; ")}`);
  }
  const args = parsed.data;

  let page: Page | null = null;
  try {
    const targetName = args.targetName || "active conversation";
    
    // RPAGuardrails Pre-Action Check
    const guardCheck = RPAGuardrails.preActionCheck(
      "reply_messenger_rpa", "send_message", targetName, args.message
    );
    if (!guardCheck.proceed) {
      return `[BẢO MẬT / SECURITY] Hành động bị chặn: ${guardCheck.warnings.join(", ")}`;
    }
    
    let safeMessage = guardCheck.filteredContent
        .replace(/[,\s]*(Dạ|dạ|Em|em|Ạ|ạ)[,\s]*$/gi, '')
        .replace(/^(Dạ|dạ|Em|em|Ạ|ạ)[,\s]+/gi, '')
        .trim();

    if (!safeMessage.includes("#Liva")) {
        safeMessage = `${safeMessage} • #Liva`;
    }

    // Open headless browser first to inspect state
    let browserContextInfo = await getOrCreateBrowser("messenger", true);
    let context = browserContextInfo.context;
    
    const page_list = context.pages();
    page = page_list.find((p: Page) => isMessengerPage(p.url())) || page_list[page_list.length - 1] || await context.newPage();
    
    if (!isMessengerPage(page.url())) {
      logger.info(`[reply_messenger_rpa] Đang điều hướng đến Facebook Messenger...`);
      await page.goto(MESSENGER_BASE, { waitUntil: 'domcontentloaded', timeout: 60000 });
    } else {
      await page.bringToFront().catch(() => {});
    }

    await new Promise(r => setTimeout(r, 3000));

    // Check login status
    const currentUrl = page.url();
    const isLoginRedirect = currentUrl.includes('/login') || 
                            currentUrl.includes('checkpoint') ||
                            currentUrl.includes('recover') ||
                            !currentUrl.includes('facebook.com/messages');

    if (isLoginRedirect) {
      logger.info(`[reply_messenger_rpa] Yêu cầu đăng nhập, chuyển sang chế độ headful...`);
      memoryEvents.emit("rpa_auth_required", {
        channel: "messenger",
        message: "Facebook Messenger chưa đăng nhập. Vui lòng đăng nhập lại."
      });
      browserContextInfo = await getOrCreateBrowser("messenger", false);
      context = browserContextInfo.context;
      page = await getActivePage(context, "facebook.com/messages");
      await page?.goto(MESSENGER_BASE, { waitUntil: 'domcontentloaded', timeout: 60000 }).catch(() => {});
      await page?.bringToFront().catch(() => {});
      return `[AuthRequired]: Facebook Messenger chưa đăng nhập. Vui lòng đăng nhập trên trình duyệt vừa hiện lên!`;
    }

    let detectedName = args.targetName || "";

    if (!args.targetName) {
      // Auto-detect currently active or top chat
      logger.info(`[reply_messenger_rpa] Đang tự động dò tìm hội thoại đang hoạt động...`);
      await new Promise(r => setTimeout(r, 2000));

      const chatInfo = await page.evaluate(() => {
        // Try to get active chat name in Messenger list
        // Typically has selected state or aria-selected
        const activeItem = document.querySelector('[role="navigation"] [class*="selected"], [role="navigation"] [aria-selected="true"], [role="navigation"] [class*="active"]');
        if (activeItem) {
          const nameEl = activeItem.querySelector('span[dir="auto"], div[dir="auto"]');
          const nameText = nameEl ? (nameEl as HTMLElement).innerText.trim() : "";
          if (nameText) return { name: nameText, alreadyActive: true };
        }
        
        // Fallback: Click first row in the grid
        const firstRow = document.querySelector('[role="grid"] [role="row"], a[href*="/messages/t/"], [role="option"]');
        if (firstRow) {
          const nameEl = firstRow.querySelector('span[dir="auto"], div[dir="auto"]');
          const nameText = nameEl ? (nameEl as HTMLElement).innerText.trim() : "";
          if (nameText) {
            (firstRow as HTMLElement).click();
            return { name: nameText, alreadyActive: false };
          }
        }
        return { name: "", alreadyActive: false };
      });

      if (chatInfo.name) {
        detectedName = chatInfo.name;
        logger.info(`[reply_messenger_rpa] Dò thấy hội thoại: "${detectedName}" (Active: ${chatInfo.alreadyActive})`);
        if (!chatInfo.alreadyActive) {
          await new Promise(r => setTimeout(r, 3000));
        }
      } else {
        throw new Error("Không phát hiện cuộc hội thoại hoạt động nào trong danh sách Messenger.");
      }
    } else {
      // Search for specific contact (similar to SendMessengerRPA)
      logger.info(`[reply_messenger_rpa] Đang tìm kiếm người nhận: ${args.targetName}`);
      
      const searchClicked = await page.evaluate(() => {
        const allInputs = Array.from(document.querySelectorAll('input'));
        for (const input of allInputs) {
          const placeholder = (input.placeholder || '').toLowerCase();
          const ariaLabel = (input.getAttribute('aria-label') || '').toLowerCase();
          const combined = placeholder + ' ' + ariaLabel;
          
          if (combined.includes('messenger') && !combined.includes('facebook')) {
            input.focus();
            input.click();
            return true;
          }
        }
        return false;
      });

      if (!searchClicked) {
        throw new Error(`Không tìm thấy thanh "Tìm kiếm trên Messenger".`);
      }
      
      await new Promise(r => setTimeout(r, 500));
      await page.keyboard.type(args.targetName, { delay: 100 });
      await new Promise(r => setTimeout(r, 2500));

      const searchResult = await page.evaluate((target: string) => {
        const els = Array.from(document.querySelectorAll('span[dir="auto"], div[dir="auto"]'));
        for (const el of els) {
          const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : (el.textContent ? el.textContent.trim() : "");
          if (text.toLowerCase() === target.toLowerCase()) {
            const clickable = el.closest('[role="option"], [role="link"], [role="row"], [role="button"], [role="listitem"], a');
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
        await page.keyboard.press('Escape');
        throw new Error(`Không tìm thấy người nhận "${args.targetName}" trong Messenger.`);
      }

      await page.keyboard.press('Escape');
      await new Promise(r => setTimeout(r, 3000));
    }

    // Verify chat thread is open
    let currentChatUrl = page.url();
    if (!isChatThread(currentChatUrl) && !currentChatUrl.includes('/messages/')) {
      await page.keyboard.press('Enter');
      await new Promise(r => setTimeout(r, 2000));
      currentChatUrl = page.url();
    }

    // Take screenshot for HITL confirmation
    const screenshotDir = path.resolve(process.cwd(), "..", "liva-ui", "public", "screenshots");
    await fs.mkdir(screenshotDir, { recursive: true });
    const screenshotFullPath = path.join(screenshotDir, "messenger_reply.png");
    await page.screenshot({ path: screenshotFullPath });

    if (!args.bypassHITL) {
      // Request HITL Approval
      const approved = await HITLGuard.requestApproval({
        toolName: "reply_messenger_rpa",
        args: { targetName: detectedName, message: safeMessage },
        reason: `Trả lời tin nhắn Messenger gửi đến "${detectedName}" với nội dung: "${safeMessage}"`,
        image: `/screenshots/messenger_reply.png?t=${Date.now()}`
      });

      if (!approved) {
        return "Lỗi: Người dùng từ chối gửi tin nhắn Messenger này.";
      }
    }

    // Selector Guard: check if chat box is present before typing
    const chatBoxSelector = 'div[role="textbox"][contenteditable="true"]';
    const chatBoxPresent = await page.waitForSelector(chatBoxSelector, { timeout: 5000 }).then(() => true).catch(() => false);
    
    if (!chatBoxPresent) {
      const currentUrl = page.url();
      const isStillLogin = currentUrl.includes('/login') || 
                            currentUrl.includes('checkpoint') ||
                            currentUrl.includes('recover') ||
                            !currentUrl.includes('facebook.com/messages');
      if (isStillLogin) {
        logger.info(`[reply_messenger_rpa] Yêu cầu đăng nhập phát hiện bởi Selector Guard.`);
        memoryEvents.emit("rpa_auth_required", {
          channel: "messenger",
          message: "Facebook Messenger bị mất phiên hoặc chưa đăng nhập. Vui lòng đăng nhập lại."
        });
        browserContextInfo = await getOrCreateBrowser("messenger", false);
        context = browserContextInfo.context;
        page = await getActivePage(context, "facebook.com/messages");
        await page?.goto(MESSENGER_BASE, { waitUntil: 'domcontentloaded', timeout: 60000 }).catch(() => {});
        await page?.bringToFront().catch(() => {});
        return `[AuthRequired]: Facebook Messenger chưa đăng nhập. Vui lòng đăng nhập trên trình duyệt vừa hiện lên!`;
      }
      throw new Error("Không tìm thấy hộp thoại soạn tin nhắn Messenger.");
    }
    
    const chatBoxFound = await page.evaluate((sel: string) => {
      const allBoxes = Array.from(document.querySelectorAll(sel));
      for (const box of allBoxes) {
        const el = box as HTMLElement;
        const ariaLabel = (el.getAttribute('aria-label') || '').toLowerCase();
        const isCommentBox = ariaLabel.includes('comment') || ariaLabel.includes('bình luận');
        if (!isCommentBox) {
          el.focus();
          el.click();
          return true;
        }
      }
      return false;
    }, chatBoxSelector);

    if (!chatBoxFound) {
      throw new Error("Không tìm thấy hộp thoại soạn tin nhắn.");
    }

    await page.keyboard.type(safeMessage, { delay: 50 });
    await page.keyboard.press('Enter');
    await new Promise(r => setTimeout(r, 2000));

    // Minimize browser
    try {
      const cdp = await page.context().newCDPSession(page);
      const { windowId } = await cdp.send('Browser.getWindowForTarget') as { windowId: number };
      await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
    } catch {}

    RPAGuardrails.logAction("reply_messenger_rpa", "message_sent", detectedName, safeMessage.substring(0, 50), false, "allowed");
    return `Thành công: Đã trả lời Messenger cho ${detectedName}. Trình duyệt đã được thu nhỏ.`;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
      return `[HỆ THỐNG]: Yêu cầu gửi tin nhắn bị từ chối hoặc quá thời gian phê duyệt.`;
    }
    return `Lỗi Reply Messenger RPA: ${errMsg}`;
  }
};
