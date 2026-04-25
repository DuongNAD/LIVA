import * as path from "node:path";
import * as fs from 'fs/promises';
import { RPAGuardrails } from '../security/RPAGuardrails';

export const metadata = {
    name: "send_messenger_rpa",
  search_keywords: ["send_messenger_rpa","send messenger rpa","gửi","nhắn tin"],
    description: "Tự động hóa trình duyệt (Browser Automation) để gửi tin nhắn qua Messenger Web.",
    parameters: {
        type: "object",
        properties: {
            targetName: { type: "string", description: "Tên người nhận tin nhắn (BẮT BUỘC lọc đại từ xưng hô, VD: 'anh Vũ' -> 'Vũ', 'bạn Hùng' -> 'Hùng')" },
            message: { type: "string", description: "HIỂU LÝ DO, TUYỆT ĐỐI KHÔNG COPY NGUYÊN VĂN CÂU LỆNH CỦA NGƯỜI DÙNG! Bạn phải ĐÓNG VAI người dùng và VIẾT LẠI tin nhắn một cách tự nhiên, giống y như người đang chat với bạn bè/người thân. Dùng ngôn ngữ thân thiện, đời thường." }
        },
        required: ["targetName", "message"]
    }
};

let globalContext: BrowserContext | null = null;

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
        const safeMessage = guardCheck.filteredContent;
        if (guardCheck.warnings.length > 0) {
            logger.warn(`[RPA Messenger/Guard] Cảnh báo: ${guardCheck.warnings.join(" | ")}`);
        }
        // ============================================
        const livaProfileDir = path.resolve(process.cwd(), 'data', 'liva_rpa_profile_messenger');
        await fs.mkdir(livaProfileDir, { recursive: true });

        if (!globalContext) {
            logger.info(`[RPA FB] Khởi động robot trình duyệt Messenger (First time launch)...`);
            const { context } = await getOrCreateBrowser("messenger");
            globalContext = context;
        } else {
            logger.info(`[RPA FB] Dùng lại trình duyệt đang mở (Reusing browser)...`);
        }

        const page_list = globalContext.pages();
        page = page_list.find((p: Page) => p.url().includes('messenger.com')) || page_list[page_list.length - 1] || await globalContext.newPage();

        if (!page.url().includes('messenger.com')) {
            // Anti-bot stealth đã được inject tự động qua PlaywrightBrowser
            logger.info(`[RPA FB] Đang điều hướng đến Messenger Web sạch...`);
            await page.goto('https://www.messenger.com/', { waitUntil: 'domcontentloaded', timeout: 60000 });
        } else {
            await page.bringToFront();
        }

        // Chờ tải trang
        await new Promise(r => setTimeout(r, 3000));

        if (page.url().includes('login') || (await page.locator('button[name="login"]').count() > 0)) {
            logger.info(`[RPA FB] ⚠️ Yêu cầu đăng nhập lần đầu (Authentication required).`);
            return `[Yêu Cầu Từ Hệ Thống]: Messenger Web chưa được đăng nhập. Bạn hãy mở Cửa sổ Trình duyệt đang được LIVA bật lên và đăng nhập thủ công tài khoản Facebook để kích hoạt Messenger RPA nhé! (Không đóng trình duyệt sau khi đăng nhập xong)`;
        }

        logger.info(`[RPA FB] Bắt đầu tìm kiếm người nhận: ${args.targetName}`);
        const searchBoxSelector = 'input[type="search"], input[aria-label="Tìm kiếm trên Messenger"], input[placeholder="Tìm kiếm trên Messenger"], input[aria-label="Search Messenger"]';
        
        await page.waitForSelector(searchBoxSelector, { timeout: 10000 });
        
        await page.evaluate((sel) => {
            const el = document.querySelector(sel);
            if(el) (el as HTMLElement).focus();
        }, searchBoxSelector);

        // Type tên người nhận
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        logger.info(`[RPA FB] Đang chọn đoạn chat...`);
        await new Promise(r => setTimeout(r, 2000));
        
        const oldUrl = page.url();
        
        const searchResult = await page.evaluate((target) => {
            const els = Array.from(document.querySelectorAll('span[dir="auto"], div[dir="auto"]'));
            let suggestions = new Set<string>();
            const targetWords = target.toLowerCase().split(" ").filter((w: string) => w.length >= 2);
            
            for (let el of els) {
                const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : (el.textContent ? el.textContent.trim() : "");
                if (text && text.length >= 2 && text.length <= 40 && !text.includes('\n')) {
                    const textLower = text.toLowerCase();
                    
                    if (textLower === target.toLowerCase()) {
                        // Messenger.com uses li[role="option"], div[role="row"], or a tags natively.
                        let clickable = el.closest('[role="option"], [role="link"], [role="row"], [role="button"], a');
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
            throw new Error(`[Lỗi Xác Nhận] Không tìm thấy danh bạ Zalo/Messenger trùng với tên "${args.targetName}". Gợi ý người có vẻ giống: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "Trống"}]. HÃY YÊU CẦU NGƯỜI DÙNG CUNG CẤP LẠI TÊN!`);
        }

        // Đợi URL nẩy ID chat (Messenger dùng /t/)
        await new Promise(r => setTimeout(r, 3000));
        const newUrl = page.url();

        if (newUrl === oldUrl || newUrl === 'https://www.messenger.com/' || newUrl === 'https://www.messenger.com/t/') {
            throw new Error(`[Lỗi Nặng] Không thể ấn định đoạn chat an toàn! Đường dẫn không chuyển hướng. Ngưng RPA an toàn.`);
        }

        logger.info(`[RPA FB] Đang soạn tin nhắn gửi đi...`);
        const chatBoxSelectorChat = 'div[role="textbox"]';
        await page.waitForSelector(chatBoxSelectorChat, { timeout: 10000 });
        
        await page.evaluate((sel) => {
            const els = document.querySelectorAll(sel);
            if (els && els.length > 0) {
                (els[els.length - 1] as HTMLElement).focus();
            }
        }, chatBoxSelectorChat);

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
    } catch (error: any) {
        return `Lỗi Messenger RPA: ${error.message}`;
    }
};