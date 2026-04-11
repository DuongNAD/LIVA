import puppeteer from 'puppeteer';
import * as path from 'path';
import * as fs from 'fs/promises';

export const metadata = {
    name: "send_messenger_rpa",
  search_keywords: ["send_messenger_rpa","send messenger rpa","gửi","nhắn tin"],
    description: "Tự động hóa trình duyệt (Browser Automation) để gửi tin nhắn qua Facebook Messenger.",
    parameters: {
        type: "object",
        properties: {
            targetName: { type: "string", description: "Tên người nhận tin nhắn (BẮT BUỘC lọc đại từ xưng hô, VD: 'anh Vũ' -> 'Vũ', 'bạn Hùng' -> 'Hùng')" },
            message: { type: "string", description: "Nội dung tin nhắn cần gửi (Message payload)" }
        },
        required: ["targetName", "message"]
    }
};

export const execute = async (args: { targetName: string; message: string }): Promise<string> => {
    let browser;
    try {
        console.log(`[RPA FB] Khởi động robot trình duyệt (Launching Headless Browser)...`);

        const livaProfileDir = path.resolve(process.cwd(), 'data', 'liva_rpa_profile');
        await fs.mkdir(livaProfileDir, { recursive: true });

        browser = await puppeteer.launch({
            headless: false,
            userDataDir: livaProfileDir,
            defaultViewport: null,
            args: ['--start-maximized', '--disable-extensions']
        });

        // Chờ thêm 2 giây để Chrome nạp xong các Extension
        await new Promise(r => setTimeout(r, 2000));

        const pages = await browser.pages();
        const page = pages.length > 0 ? pages[0] : await browser.newPage();
        
        console.log(`[RPA FB] Đang điều hướng đến Messenger / Facebook Message...`);
        
        // Thay url trang web sang facebook/messages để tránh lỗi popup Feed
        await page.goto('https://www.facebook.com/messages/t/', { waitUntil: 'domcontentloaded', timeout: 60000 });

        if (page.url().includes('login') || (await page.$('button[name="login"]'))) {
            console.log(`[RPA FB] ⚠️ Yêu cầu đăng nhập lần đầu (First-time Authentication required).`);
            console.log(`[RPA FB] Anh có 2 phút để tự gõ tài khoản và mật khẩu trên cửa sổ trình duyệt nhé!`);
            await new Promise(r => setTimeout(r, 120000));
        }

        // Đợi giao diện DOM của Facebook nạp đầy đủ (chậm hơn bình thường)
        await new Promise(r => setTimeout(r, 5000));

        console.log(`[RPA FB] Bắt đầu tìm kiếm người nhận: ${args.targetName}`);

        // THẬT SỰ QUAN TRỌNG: Chỉ tìm input riêng biệt của thẻ Messenger. Sẽ KHÔNG dùng input[type="search"] chung vì nó sẽ dính thanh tìm kiếm toàn cầu của FB ở góc trên bên trái!
        const searchBoxSelector = 'input[aria-label="Tìm kiếm trên Messenger"], input[placeholder="Tìm kiếm trên Messenger"], input[aria-label="Search Messenger"]';
        
        // Nếu không tìm thấy ô Messenger chuẩn trong 10s -> Halt luôn.
        await page.waitForSelector(searchBoxSelector, { timeout: 10000 });
        
        await page.evaluate((sel) => {
            const el = document.querySelector(sel);
            if(el) (el as HTMLElement).focus();
        }, searchBoxSelector);

        // Type tên người nhận
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        console.log(`[RPA FB] Đang chọn đoạn chat...`);
        // Chờ 2s để dropdown hiển thị ra kết quả search
        await new Promise(r => setTimeout(r, 2000));
        
        // Lưu URL trước khi Click
        const oldUrl = page.url();
        
        // THUẬT TOÁN MỚI: Cào tên trong DOM và Click chính xác, hoặc trả về danh sách gợi ý
        const searchResult = await page.evaluate((target) => {
            // FB Messenger thường dùng span[dir="auto"] để gói tên người dùng trong danh sách tìm kiếm
            const els = Array.from(document.querySelectorAll('span[dir="auto"], div[dir="auto"]'));
            let suggestions = new Set<string>();
            const targetWords = target.toLowerCase().split(" ").filter((w: string) => w.length >= 2);
            
            for (let el of els) {
                const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : (el.textContent ? el.textContent.trim() : "");
                // Chỉ lấy các text ngắn giống tên người (độ dài hợp lý, không phải đoạn văn)
                if (text && text.length >= 2 && text.length <= 40 && !text.includes('\n')) {
                    const textLower = text.toLowerCase();
                    
                    // 1. So khớp tuyệt đối -> Nhấn chọn
                    if (textLower === target.toLowerCase()) {
                        let clickable = el.closest('[role="link"], [role="button"], a');
                        if (clickable) {
                            (clickable as HTMLElement).click();
                            return { clicked: true, suggestions: [] };
                        }
                    }
                    
                    // 2. Gom nhặt các gợi ý thông minh (Lọc rác): Chỉ lấy những tên CHỨA ít nhất 1 từ trong tên mục tiêu
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

        // Bỏ qua phương án phím bấm hên xui. Nếu click trượt, trả thẳng Gợi Ý SẠCH cho AI xử lý.
        if (!searchResult.clicked) {
            throw new Error(`[Lỗi Nặng] Không tìm thấy danh bạ nào trùng MỘT TRĂM PHẦN TRĂM với tên "${args.targetName}". Các kết quả liên quan hiển thị trên màn hình là: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "Không có kết quả nào giống"}]. HÃY CHỦ ĐỘNG BÁO LẠI cho người dùng và hỏi họ muốn chọn ai trong danh sách này!`);
        }

        // Đợi URL nẩy ID
        await new Promise(r => setTimeout(r, 3000));
        const newUrl = page.url();

        // STRICT URL CHECK TIN CẬY HƠN: URL phải chứa /messages/ và /t/
        const isSafeMessageRoute = newUrl.includes('/messages/') && newUrl.includes('/t/');
        
        // Nếu URL vẫn là trang gốc chưa vào kênh nào, hoặc ra ngoài Feed thì CẮT
        if (!isSafeMessageRoute || newUrl === 'https://www.facebook.com/messages/t/' || newUrl === 'https://www.facebook.com/messages/') {
            throw new Error(`[Lỗi Nặng] Không thể truy cập đoạn chat an toàn! Đường dẫn bị lệch thành: ${newUrl}. Ngưng RPA an toàn để tránh nhắn nhầm vào bài công khai.`);
        }

        console.log(`[RPA FB] Đang soạn tin nhắn gửi đi an toàn...`);
        // Lấy đúng thẻ role=textbox của CHATBOX (thường đi kèm aria-label='Nhập tin nhắn' hoặc 'Message')
        const chatBoxSelector = 'div[role="textbox"]';
        await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
        
        // Lấy box textbox cuối cùng vì FB có hai textbox (1 search, 1 chat) nếu render sai
        await page.evaluate((sel) => {
            const els = document.querySelectorAll(sel);
            if (els && els.length > 0) {
                (els[els.length - 1] as HTMLElement).focus();
            }
        }, chatBoxSelector);

        // Gõ nội dung tin
        await page.keyboard.type(args.message, { delay: 50 });

        // Gửi và báo cáo
        await page.keyboard.press('Enter');
        console.log(`[RPA FB] Đã gửi tin nhắn (Message dispatched)!`);

        // Đợi 3s trước khi đóng tab để data sync
        await new Promise(r => setTimeout(r, 3000));
        await browser.close();

        return `Hoàn tất: Đã tự động gửi tin nhắn FB tới ${args.targetName}`;
    } catch (error: any) {
        if (browser) await browser.close();
        return `Lỗi Facebook RPA: ${error.message}`;
    }
};