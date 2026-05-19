import { Page } from "playwright-core";
import { getOrCreateBrowser, getActivePage } from "@utils/PlaywrightBrowser";
import { logger } from "@utils/logger";
import { RPAGuardrails } from "@security/RPAGuardrails";
import { HITLGuard } from "@security/HITLGuard";
import * as path from "node:path";
import * as fs from 'node:fs/promises';

export const metadata = {
    name: "send_messenger_rpa",
  search_keywords: ["send_messenger_rpa","send messenger rpa","gửi","nhắn tin","send message","facebook","messenger","chat","message friend","tin nhắn","nhắn mess","gửi tin"],
    description: "[ASK_FIRST] Browser Automation (RPA) to send a message via Facebook Messenger Web (facebook.com/messages). Use this when the user says 'nhắn tin cho...' or 'message ...'.",
    parameters: {
        type: "object",
        properties: {
            targetName: { type: "string", description: "Recipient name / Tên người nhận (MUST strip honorifics / BẮT BUỘC bỏ các đại từ sở hữu. e.g., 'anh Vũ/mr. Vu' -> 'Vũ/Vu', 'bạn Hùng/friend Hung' -> 'Hùng/Hung')" },
            message: { type: "string", description: "[LOCALIZED] UNDERSTAND THE INTENT, DO NOT COPY THE USER'S COMMAND VERBATIM! ROLEPLAY as the user and REWRITE naturally. CRITICAL: DO NOT add your own personality — NO 'Dạ', 'ạ', 'em' — write as if the USER is typing to their friend/family. Use casual, peer-level language matching the user's language." }
        },
        required: ["targetName", "message"]
    }
};

const MESSENGER_BASE = 'https://www.facebook.com/messages';

function isMessengerPage(url: string): boolean {
    return url.includes('facebook.com/messages');
}

function isChatThread(url: string): boolean {
    return /facebook\.com\/messages\/(e2ee\/)?t\/\d+/.test(url);
}

export const execute = async (args: { targetName: string; message: string }): Promise<string> => {
    let page: Page | null = null;
    try {
        // ====== RPAGuardrails Pre-Action Check ======
        const guardCheck = RPAGuardrails.preActionCheck(
            "send_messenger_rpa", "send_message", args.targetName, args.message
        );
        if (!guardCheck.proceed) {
            return `[BẢO MẬT / SECURITY] Hành động bị chặn: ${guardCheck.warnings.join(", ")} / Action blocked: ${guardCheck.warnings.join(", ")}`;
        }
        
        let safeMessage = guardCheck.filteredContent
            .replace(/[,\s]*(Dạ|dạ|Em|em|Ạ|ạ)[,\s]*$/gi, '')
            .replace(/^(Dạ|dạ|Em|em|Ạ|ạ)[,\s]+/gi, '')
            .trim();

        safeMessage = safeMessage
            .replace(/^(Dạ[,\s]+em|Dạ)[,\s]+/gi, '')
            .replace(/[,\s]+(Dạ[,\s]+em|Dạ|ạ|em|nhé|nha|ạ)[,\s]*$/gi, '')
            .trim();

        if (guardCheck.warnings.length > 0) {
            logger.warn(`[RPA Messenger/Guard] Cảnh báo: ${guardCheck.warnings.join(" | ")}`);
        }
        
        if (!safeMessage.includes("#Liva")) {
            safeMessage = `${safeMessage} • #Liva`;
        }

        // ====== BƯỚC 1: Khởi động robot trình duyệt Messenger & thực hiện tìm kiếm danh bạ trước ======
        // Thử chạy Headless (vô hình) trước để không gián đoạn người dùng
        let browserContextInfo = await getOrCreateBrowser("messenger", true);
        let context = browserContextInfo.context;

        const page_list = context.pages();
        page = page_list.find((p: Page) => isMessengerPage(p.url())) || page_list[page_list.length - 1] || await context.newPage();

        if (!isMessengerPage(page.url())) {
            logger.info(`[RPA FB] Đang điều hướng đến Facebook Messenger (Headless)...`);
            await page.goto(MESSENGER_BASE, { waitUntil: 'domcontentloaded', timeout: 60000 });
        } else {
            await page.bringToFront().catch(() => {});
        }

        // Chờ tải trang
        await new Promise(r => setTimeout(r, 4000));

        // Kiểm tra đăng nhập
        const currentUrl = page.url();
        const isLoginRedirect = currentUrl.includes('/login') || 
                                currentUrl.includes('checkpoint') ||
                                currentUrl.includes('recover') ||
                                !currentUrl.includes('facebook.com/messages');
        
        if (isLoginRedirect) {
            logger.info(`[RPA FB] ⚠️ Yêu cầu đăng nhập (URL: ${currentUrl}). Tự động chuyển sang chế độ có UI (Headful Fallback)...`);
            browserContextInfo = await getOrCreateBrowser("messenger", false);
            context = browserContextInfo.context;
            page = await getActivePage(context, "facebook.com/messages");
            await page?.goto(MESSENGER_BASE, { waitUntil: 'domcontentloaded', timeout: 60000 }).catch(() => {});
            await page?.bringToFront().catch(() => {});
            return `[Yêu Cầu Từ Hệ Thống / System Request]: Facebook Messenger chưa được đăng nhập. Bạn hãy mở Cửa sổ Trình duyệt đang được LIVA bật lên và đăng nhập thủ công tài khoản Facebook để kích hoạt Messenger RPA nhé! Sau khi đăng nhập xong, hãy quay lại ra lệnh cho LIVA lần nữa. (Không đóng trình duyệt) / Facebook Messenger is not logged in. Please open the browser window spawned by LIVA and log in to Facebook manually to activate RPA! (Do not close the browser after logging in)`;
        }

        logger.info(`[RPA FB] Bắt đầu tìm kiếm người nhận: ${args.targetName}`);
        
        // Nhấn nút Tìm kiếm trên Messenger sidebar
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
            throw new Error(`[Lỗi] Không tìm thấy thanh "Tìm kiếm trên Messenger" trong sidebar. Trang có thể chưa tải xong.`);
        }
        
        await new Promise(r => setTimeout(r, 500));

        // Nhập tên
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        logger.info(`[RPA FB] Đang lấy kết quả gợi ý danh bạ...`);
        await new Promise(r => setTimeout(r, 2500));

        // ====== BƯỚC 2: Chụp ảnh danh bạ gợi ý để lưu vào folder public của UI ======
        const screenshotDir = path.resolve(process.cwd(), "..", "liva-ui", "public", "screenshots");
        await fs.mkdir(screenshotDir, { recursive: true });
        const screenshotFullPath = path.join(screenshotDir, "messenger_search.png");
        
        let clip: { x: number; y: number; width: number; height: number } | undefined = undefined;
        try {
          const searchInputHandle = await page.evaluateHandle(() => {
            const el = document.activeElement;
            return el && el.tagName === "INPUT" ? el : document.querySelector('input[placeholder*="Messenger"]');
          });
          const searchBox = searchInputHandle.asElement();
          if (searchBox) {
            const box = await searchBox.boundingBox();
            if (box) {
              clip = {
                x: Math.max(0, box.x - 20),
                y: Math.max(0, box.y - 15),
                width: 380,
                height: 550
              };
              logger.info(`[RPA FB] 📸 Clipping screenshot at x:${clip.x}, y:${clip.y}, w:${clip.width}, h:${clip.height}`);
            }
          }
        } catch (e) {
          logger.warn(`[RPA FB] Không tìm thấy bounding box của ô tìm kiếm, chụp toàn màn hình làm dự phòng: ${e}`);
        }

        if (clip) {
          await page.screenshot({ path: screenshotFullPath, clip });
        } else {
          await page.screenshot({ path: screenshotFullPath });
        }
        logger.info(`[RPA FB] 📸 Đã chụp ảnh kết quả gợi ý danh bạ Messenger tại: ${screenshotFullPath}`);

        // ====== BƯỚC 3: Xác nhận thông tin với ảnh chụp trực quan từ Messenger ======
        const approved = await HITLGuard.requestApproval({
          toolName: "send_messenger_rpa",
          args: { targetName: args.targetName, message: safeMessage },
          reason: `Gửi tin Messenger đến / Send Messenger message to "${args.targetName}" với nội dung / with content: "${safeMessage}"`,
          image: `/screenshots/messenger_search.png?t=${Date.now()}`
        });

        if (!approved) {
          // Trả lại trạng thái sạch cho thanh tìm kiếm trước khi thoát
          await page.keyboard.press('Escape');
          return "Lỗi / Error: Người dùng đã từ chối gửi tin nhắn Messenger này. / User declined to send this Messenger message.";
        }

        // ====== BƯỚC 4: Click chọn và thực hiện gửi tin nhắn (Sau khi được duyệt) ======

        const searchResult = await page.evaluate((target: string) => {
            const els = Array.from(document.querySelectorAll('span[dir="auto"], div[dir="auto"]'));
            const suggestions = new Set<string>();
            const targetWords = target.toLowerCase().split(" ").filter((w: string) => w.length >= 2);
            
            for (const el of els) {
                const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : (el.textContent ? el.textContent.trim() : "");
                if (text && text.length >= 2 && text.length <= 40 && !text.includes('\n')) {
                    const textLower = text.toLowerCase();
                    
                    if (textLower === target.toLowerCase()) {
                        const clickable = el.closest('[role="option"], [role="link"], [role="row"], [role="button"], [role="listitem"], a');
                        if (clickable) {
                            (clickable as HTMLElement).click();
                        } else {
                            (el as HTMLElement).click();
                        }
                        return { clicked: true, suggestions: [] };
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
            await page.keyboard.press('Escape');
            throw new Error(`[Lỗi Xác Nhận / Matching Error]: Không tìm thấy danh bạ Messenger trùng với tên "${args.targetName}". Gợi ý: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "Trống"}]. / No exact Messenger contact matching "${args.targetName}" was found. Suggested names: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "None"}].`);
        }

        // Đóng overlay tìm kiếm
        await page.keyboard.press('Escape');

        // Đợi chuyển hướng
        await new Promise(r => setTimeout(r, 3000));
        let currentChatUrl = page.url();
        logger.info(`[RPA FB] URL sau khi click kết quả: ${currentChatUrl}`);

        const isProfilePage = currentChatUrl.includes('facebook.com/') && 
                              !currentChatUrl.includes('/messages/') &&
                              !currentChatUrl.includes('/login');
        
        if (isProfilePage) {
            logger.warn(`[RPA FB] ⚠️ Đang ở Profile page, tìm nút "Nhắn tin"...`);
            
            const msgBtnClicked = await page.evaluate(() => {
                const allBtns = Array.from(document.querySelectorAll('div[role="button"], a[role="button"], span'));
                for (const btn of allBtns) {
                    const text = (btn as HTMLElement).innerText?.trim() || '';
                    if (text === 'Nhắn tin' || text === 'Message') {
                        (btn as HTMLElement).click();
                        return true;
                    }
                }
                const msgLinks = Array.from(document.querySelectorAll('a[href*="/messages/t/"], a[href*="/messages/e2ee/t/"]'));
                if (msgLinks.length > 0) {
                    (msgLinks[0] as HTMLElement).click();
                    return true;
                }
                return false;
            });

            if (!msgBtnClicked) {
                throw new Error(`[Lỗi] Đang ở profile "${args.targetName}" nhưng không tìm được nút "Nhắn tin". Ngưng RPA an toàn.`);
            }

            await new Promise(r => setTimeout(r, 3000));
            currentChatUrl = page.url();
            logger.info(`[RPA FB] URL sau khi click "Nhắn tin": ${currentChatUrl}`);
        }

        if (!isChatThread(currentChatUrl) && !currentChatUrl.includes('/messages/')) {
            logger.warn(`[RPA FB] URL không phải chat thread. Thử nhấn Enter...`);
            await page.keyboard.press('Enter');
            await new Promise(r => setTimeout(r, 2000));
            
            currentChatUrl = page.url();
            if (!isChatThread(currentChatUrl) && !currentChatUrl.includes('/messages/')) {
                throw new Error(`[Lỗi Nặng] Không thể mở chat thread! URL hiện tại: ${currentChatUrl}. RPA đã dừng để tránh comment/tương tác sai.`);
            }
        }

        logger.info(`[RPA FB] ✅ Đã vào chat thread. Đang soạn tin nhắn gửi đi...`);
        
        const chatBoxSelector = 'div[role="textbox"][contenteditable="true"]';
        await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
        
        const chatBoxFound = await page.evaluate((sel: string) => {
            const allBoxes = Array.from(document.querySelectorAll(sel));
            for (const box of allBoxes) {
                const el = box as HTMLElement;
                const ariaLabel = (el.getAttribute('aria-label') || '').toLowerCase();
                const isCommentBox = ariaLabel.includes('comment') || 
                                     ariaLabel.includes('bình luận') ||
                                     ariaLabel.includes('viết bình luận') ||
                                     ariaLabel.includes('write a comment');
                
                const isChatBox = ariaLabel.includes('message') || 
                                  ariaLabel.includes('tin nhắn') ||
                                  ariaLabel.includes('nhập') ||
                                  ariaLabel === '' || 
                                  !isCommentBox;
                
                if (isChatBox && !isCommentBox) {
                    el.focus();
                    el.click();
                    return { found: true, label: ariaLabel };
                }
            }
            return { found: false, label: '' };
        }, chatBoxSelector);

        if (!chatBoxFound.found) {
            throw new Error(`[Lỗi] Không tìm thấy chat textbox (chỉ thấy comment box). Ngưng RPA để tránh comment nhầm!`);
        }
        
        logger.info(`[RPA FB] Textbox đã chọn: "${chatBoxFound.label || 'default'}"`);
        await new Promise(r => setTimeout(r, 300));

        // Gõ nội dung tin
        await page.keyboard.type(safeMessage, { delay: 50 });

        // Gửi và báo cáo
        await page.keyboard.press('Enter');
        logger.info(`[RPA FB] Đã gửi tin nhắn (Message dispatched)!`);

        await new Promise(r => setTimeout(r, 2000));

        // Thu nhỏ cửa sổ trình duyệt sau khi làm xong
        try {
            const cdp = await page.context().newCDPSession(page);
            const { windowId } = await cdp.send('Browser.getWindowForTarget') as { windowId: number };
            await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
        } catch (e) { void e; }

        RPAGuardrails.logAction("send_messenger_rpa", "message_sent", args.targetName, safeMessage.substring(0, 50), false, "allowed");
        return `Hoàn tất / Success: Đã gửi tin nhắn Messenger cho / Successfully sent Messenger message to ${args.targetName}. Trình duyệt đã được thu nhỏ ngầm. / The background browser has been minimized.`;
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
            return `[HỆ THỐNG BẢO MẬT TỪ CHỐI / SECURITY REFUSED]: Yêu cầu gửi tin nhắn bị từ chối hoặc quá thời gian phê duyệt (300s). / Message request was rejected or approval timed out (300s).`;
        }
        return `Lỗi Messenger RPA / Messenger RPA Error: ${errMsg}`;
    }
};