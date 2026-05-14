import { BrowserContext, Page } from "playwright-core";
import { getOrCreateBrowser } from "@utils/PlaywrightBrowser";
import { logger } from "@utils/logger";
import { RPAGuardrails } from "@security/RPAGuardrails";

export const metadata = {
    name: "send_messenger_rpa",
  search_keywords: ["send_messenger_rpa","send messenger rpa","gửi","nhắn tin","send message","facebook","messenger","chat","message friend","tin nhắn","nhắn mess","gửi tin"],
    description: "[ASK_FIRST] Browser Automation (RPA) to send a message via Facebook Messenger Web (facebook.com/messages).",
    parameters: {
        type: "object",
        properties: {
            targetName: { type: "string", description: "Recipient name (MUST strip honorifics. e.g., 'Mr. Vu' -> 'Vu', 'friend Hung' -> 'Hung')" },
            message: { type: "string", description: "UNDERSTAND THE INTENT, DO NOT COPY THE USER'S COMMAND VERBATIM! ROLEPLAY as the user and REWRITE naturally. CRITICAL: DO NOT add your own personality — NO 'Dạ', 'ạ', 'em' — write as if the USER is typing to their friend/family. Use casual, peer-level language." }
        },
        required: ["targetName", "message"]
    }
};

let globalContext: BrowserContext | null = null;

// [P5] Updated: messenger.com has been discontinued (April 2026).
// Facebook Messenger is now at facebook.com/messages with E2EE default.
const MESSENGER_BASE = 'https://www.facebook.com/messages';
const MESSENGER_NEW_MSG = 'https://www.facebook.com/messages/new';

/**
 * Check if the current page URL is a Messenger page (covers all variants).
 */
function isMessengerPage(url: string): boolean {
    return url.includes('facebook.com/messages');
}

/**
 * Check if the URL is a valid chat thread (not just the inbox).
 */
function isChatThread(url: string): boolean {
    // Matches: /messages/t/{id}, /messages/e2ee/t/{id}
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
            return `[BẢO MẬT] Hành động bị chặn: ${guardCheck.warnings.join(", ")}`;
        }
        
        // Strip trailing AI personality fragments that might have leaked into the tool parameter
        let safeMessage = guardCheck.filteredContent
            .replace(/[,\s]*(Dạ|dạ|Em|em|Ạ|ạ)[,\s]*$/gi, '')
            .replace(/^(Dạ|dạ|Em|em|Ạ|ạ)[,\s]+/gi, '')
            .trim();

        // One more pass for combinations like "Dạ, em" at the beginning or end
        safeMessage = safeMessage
            .replace(/^(Dạ[,\s]+em|Dạ)[,\s]+/gi, '')
            .replace(/[,\s]+(Dạ[,\s]+em|Dạ|ạ|em|nhé|nha|ạ)[,\s]*$/gi, '')
            .trim();

        if (guardCheck.warnings.length > 0) {
            logger.warn(`[RPA Messenger/Guard] Cảnh báo: ${guardCheck.warnings.join(" | ")}`);
        }
        // [AUTO-TAG] Append #Liva so recipients know this is AI-generated
        // ⚠️ MUST NOT use \n — Messenger treats Enter as "send message"
        // which would split #Liva into a separate bubble
        if (!safeMessage.includes("#Liva")) {
            safeMessage = `${safeMessage} • #Liva`;
        }
        // ============================================
        // Profile dir is auto-managed by getOrCreateBrowser("messenger")
        // → data/liva_messenger_profile/

        if (!globalContext) {
            logger.info(`[RPA FB] Khởi động robot trình duyệt Messenger (First time launch)...`);
            const { context } = await getOrCreateBrowser("messenger");
            globalContext = context;
        } else {
            logger.info(`[RPA FB] Dùng lại trình duyệt đang mở (Reusing browser)...`);
        }

        const page_list = globalContext.pages();
        page = page_list.find((p: Page) => isMessengerPage(p.url())) || page_list[page_list.length - 1] || await globalContext.newPage();

        if (!isMessengerPage(page.url())) {
            logger.info(`[RPA FB] Đang điều hướng đến Facebook Messenger...`);
            await page.goto(MESSENGER_BASE, { waitUntil: 'domcontentloaded', timeout: 60000 });
        } else {
            await page.bringToFront();
        }

        // Chờ tải trang
        await new Promise(r => setTimeout(r, 4000));

        // [P5] Login detection: Check URL for login/checkpoint redirect
        // Only trust URL-based detection (element-based is unreliable on Facebook)
        const currentUrl = page.url();
        const isLoginRedirect = currentUrl.includes('/login') || 
                                currentUrl.includes('checkpoint') ||
                                currentUrl.includes('recover') ||
                                !currentUrl.includes('facebook.com/messages');
        
        if (isLoginRedirect) {
            logger.info(`[RPA FB] ⚠️ Yêu cầu đăng nhập (URL: ${currentUrl}).`);
            return `[Yêu Cầu Từ Hệ Thống]: Facebook Messenger chưa được đăng nhập. Bạn hãy mở Cửa sổ Trình duyệt đang được LIVA bật lên và đăng nhập thủ công tài khoản Facebook để kích hoạt Messenger RPA nhé! Sau khi đăng nhập xong, hãy quay lại ra lệnh cho LIVA lần nữa. (Không đóng trình duyệt)`;
        }

        logger.info(`[RPA FB] Bắt đầu tìm kiếm người nhận: ${args.targetName}`);
        
        // [P5] CRITICAL: Must click the Messenger sidebar search ("Tìm kiếm trên Messenger")
        // NOT the global Facebook search bar ("Tìm kiếm trên Facebook")
        // Strategy: find input whose placeholder/aria-label contains "Messenger", NOT "Facebook"
        const searchClicked = await page.evaluate(() => {
            const allInputs = Array.from(document.querySelectorAll('input'));
            for (const input of allInputs) {
                const placeholder = (input.placeholder || '').toLowerCase();
                const ariaLabel = (input.getAttribute('aria-label') || '').toLowerCase();
                const combined = placeholder + ' ' + ariaLabel;
                
                // Match: "Tìm kiếm trên Messenger" or "Search Messenger"
                // Exclude: "Tìm kiếm trên Facebook" or "Search Facebook"
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

        // Type tên người nhận
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        logger.info(`[RPA FB] Đang chọn đoạn chat...`);
        await new Promise(r => setTimeout(r, 2500));
        
        const oldUrl = page.url();
        
        // [P5] Updated: search result matching for new Facebook Messages UI
        const searchResult = await page.evaluate((target: string) => {
            const els = Array.from(document.querySelectorAll('span[dir="auto"], div[dir="auto"]'));
            const suggestions = new Set<string>();
            const targetWords = target.toLowerCase().split(" ").filter((w: string) => w.length >= 2);
            
            for (const el of els) {
                const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : (el.textContent ? el.textContent.trim() : "");
                if (text && text.length >= 2 && text.length <= 40 && !text.includes('\n')) {
                    const textLower = text.toLowerCase();
                    
                    if (textLower === target.toLowerCase()) {
                        // Facebook Messages UI: clickable containers
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
            // Clear search text before throwing — prevent stale text on retry
            await page.keyboard.press('Escape');
            throw new Error(`[Lỗi Xác Nhận] Không tìm thấy danh bạ Messenger trùng với tên "${args.targetName}". Gợi ý người có vẻ giống: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "Trống"}]. HÃY YÊU CẦU NGƯỜI DÙNG CUNG CẤP LẠI TÊN!`);
        }

        // [FIX] Clear search text after successful selection — prevents stale text on next search
        // Escape closes the search overlay, leaving a clean Messenger sidebar
        await page.keyboard.press('Escape');

        // Đợi URL chuyển đến chat thread (e2ee/t/{id} hoặc t/{id})
        await new Promise(r => setTimeout(r, 3000));
        let currentChatUrl = page.url();
        logger.info(`[RPA FB] URL sau khi click kết quả: ${currentChatUrl}`);

        // ═══ PROFILE PAGE GUARD ═══
        // Nếu click kết quả nhảy vào profile page thay vì chat thread,
        // tìm nút "Nhắn tin" / "Message" trên profile để mở chat.
        const isProfilePage = currentChatUrl.includes('facebook.com/') && 
                              !currentChatUrl.includes('/messages/') &&
                              !currentChatUrl.includes('/login');
        
        if (isProfilePage) {
            logger.warn(`[RPA FB] ⚠️ Đang ở Profile page, tìm nút "Nhắn tin"...`);
            
            // Click nút "Nhắn tin" / "Message" trên profile
            const msgBtnClicked = await page.evaluate(() => {
                // Tìm nút có text "Nhắn tin" hoặc "Message"
                const allBtns = Array.from(document.querySelectorAll('div[role="button"], a[role="button"], span'));
                for (const btn of allBtns) {
                    const text = (btn as HTMLElement).innerText?.trim() || '';
                    if (text === 'Nhắn tin' || text === 'Message') {
                        (btn as HTMLElement).click();
                        return true;
                    }
                }
                // Fallback: tìm link có href chứa /messages/t/
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

            // Đợi chuyển hướng đến chat thread
            await new Promise(r => setTimeout(r, 3000));
            currentChatUrl = page.url();
            logger.info(`[RPA FB] URL sau khi click "Nhắn tin": ${currentChatUrl}`);
        }

        // ═══ FINAL URL GUARD — MUST be in a chat thread ═══
        if (!isChatThread(currentChatUrl) && !currentChatUrl.includes('/messages/')) {
            // Thử fallback: Enter
            logger.warn(`[RPA FB] URL không phải chat thread. Thử nhấn Enter...`);
            await page.keyboard.press('Enter');
            await new Promise(r => setTimeout(r, 2000));
            
            currentChatUrl = page.url();
            if (!isChatThread(currentChatUrl) && !currentChatUrl.includes('/messages/')) {
                throw new Error(`[Lỗi Nặng] Không thể mở chat thread! URL hiện tại: ${currentChatUrl}. RPA đã dừng để tránh comment/tương tác sai.`);
            }
        }

        logger.info(`[RPA FB] ✅ Đã vào chat thread. Đang soạn tin nhắn gửi đi...`);
        
        // ═══ ANTI-COMMENT GUARD ═══
        // CHỈ gõ vào chat message textbox, TUYỆT ĐỐI KHÔNG gõ vào comment box
        // Chat textbox: div[role="textbox"] nằm trong footer/form, KHÔNG nằm trong feed/post
        const chatBoxSelector = 'div[role="textbox"][contenteditable="true"]';
        await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
        
        // Tìm đúng chat textbox (loại trừ comment box)
        const chatBoxFound = await page.evaluate((sel: string) => {
            const allBoxes = Array.from(document.querySelectorAll(sel));
            for (const box of allBoxes) {
                const el = box as HTMLElement;
                // Comment box thường nằm trong feed, post, hoặc có aria-label chứa "comment"/"bình luận"
                const ariaLabel = (el.getAttribute('aria-label') || '').toLowerCase();
                const isCommentBox = ariaLabel.includes('comment') || 
                                     ariaLabel.includes('bình luận') ||
                                     ariaLabel.includes('viết bình luận') ||
                                     ariaLabel.includes('write a comment');
                
                // Chat message box: aria-label chứa "message"/"tin nhắn" hoặc KHÔNG phải comment box
                const isChatBox = ariaLabel.includes('message') || 
                                  ariaLabel.includes('tin nhắn') ||
                                  ariaLabel.includes('nhập') ||
                                  ariaLabel === '' || // Facebook chat textbox sometimes has no label
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

        // Đợi 2s để tin nhắn đẩy đi trước khi minimize
        await new Promise(r => setTimeout(r, 2000));

        // Thu nhỏ cửa sổ trình duyệt sau khi làm xong
        try {
            const cdp = await page.context().newCDPSession(page);
            const { windowId } = await cdp.send('Browser.getWindowForTarget') as any;
            await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
        } catch (e) { void e; }

        RPAGuardrails.logAction("send_messenger_rpa", "message_sent", args.targetName, safeMessage.substring(0, 50), false, "allowed");
        return `Hoàn tất: Đã gởi tin nhắn Messenger cho ${args.targetName}. Cửa sổ ngầm đã được đóng cất.`;
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        return `Lỗi Messenger RPA: ${errMsg}`;
    }
};