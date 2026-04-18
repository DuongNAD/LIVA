import puppeteer, { type Browser, type Page } from 'puppeteer';
import * as path from 'path';
import * as fs from 'fs/promises';

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

let globalBrowser: Browser | null = null;

export const execute = async (args: { targetName: string; message: string }): Promise<string> => {
    let page: Page | null = null;
    try {
        const livaProfileDir = path.resolve(process.cwd(), 'data', 'liva_rpa_profile_messenger');
        await fs.mkdir(livaProfileDir, { recursive: true });

        if (!globalBrowser || !globalBrowser.isConnected()) {
            console.log(`[RPA FB] Khởi động robot trình duyệt Messenger (First time launch)...`);
            globalBrowser = await puppeteer.launch({
                headless: false, // Để người dùng có thể đăng nhập thủ công nếu cần
                userDataDir: livaProfileDir,
                defaultViewport: null,
                args: ['--start-maximized', '--disable-extensions', '--disable-blink-features=AutomationControlled'],
                ignoreDefaultArgs: ['--enable-automation']
            });
        } else {
            console.log(`[RPA FB] Dùng lại trình duyệt đang mở (Reusing browser)...`);
        }

        const pages = await globalBrowser.pages();
        page = pages.find((p: Page) => p.url().includes('messenger.com')) || pages[0];

        if (!page.url().includes('messenger.com')) {
            await page.setUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36");
            await page.evaluateOnNewDocument(() => { Object.defineProperty(navigator, 'webdriver', { get: () => undefined }); });
            
            console.log(`[RPA FB] Đang điều hướng đến Messenger Web sạch...`);
            await page.goto('https://www.messenger.com/', { waitUntil: 'domcontentloaded', timeout: 60000 });
        } else {
            await page.bringToFront();
        }

        // Chờ tải trang
        await new Promise(r => setTimeout(r, 3000));

        if (page.url().includes('login') || (await page.$('button[name="login"]'))) {
            console.log(`[RPA FB] ⚠️ Yêu cầu đăng nhập lần đầu (Authentication required).`);
            return `[Yêu Cầu Từ Hệ Thống]: Messenger Web chưa được đăng nhập. Bạn hãy mở Cửa sổ Trình duyệt đang được LIVA bật lên và đăng nhập thủ công tài khoản Facebook để kích hoạt Messenger RPA nhé! (Không đóng trình duyệt sau khi đăng nhập xong)`;
        }

        console.log(`[RPA FB] Bắt đầu tìm kiếm người nhận: ${args.targetName}`);
        const searchBoxSelector = 'input[type="search"], input[aria-label="Tìm kiếm trên Messenger"], input[placeholder="Tìm kiếm trên Messenger"], input[aria-label="Search Messenger"]';
        
        await page.waitForSelector(searchBoxSelector, { timeout: 10000 });
        
        await page.evaluate((sel) => {
            const el = document.querySelector(sel);
            if(el) (el as HTMLElement).focus();
        }, searchBoxSelector);

        // Type tên người nhận
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        console.log(`[RPA FB] Đang chọn đoạn chat...`);
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

        console.log(`[RPA FB] Đang soạn tin nhắn gửi đi...`);
        const chatBoxSelectorChat = 'div[role="textbox"]';
        await page.waitForSelector(chatBoxSelectorChat, { timeout: 10000 });
        
        await page.evaluate((sel) => {
            const els = document.querySelectorAll(sel);
            if (els && els.length > 0) {
                (els[els.length - 1] as HTMLElement).focus();
            }
        }, chatBoxSelectorChat);

        // Gõ nội dung tin
        await page.keyboard.type(args.message, { delay: 50 });

        // Gửi và báo cáo
        await page.keyboard.press('Enter');
        console.log(`[RPA FB] Đã gửi tin nhắn (Message dispatched)!`);

        // Đợi 2s để tin nhắn đẩy đi trước khi minimize
        await new Promise(r => setTimeout(r, 2000));

        // Thu nhỏ cửa sổ trình duyệt sau khi làm xong
        try {
            const session = await page.target().createCDPSession();
            const { windowId } = await session.send('Browser.getWindowForTarget');
            await session.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'minimized' } });
        } catch(e) {}

        return `Hoàn tất: Đã gởi tin nhắn Messenger cho ${args.targetName}. Cửa sổ ngầm đã được đóng cất.`;
    } catch (error: any) {
        return `Lỗi Messenger RPA: ${error.message}`;
    }
};