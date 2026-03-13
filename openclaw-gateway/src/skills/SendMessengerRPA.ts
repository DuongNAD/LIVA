import puppeteer from 'puppeteer';
import * as path from 'path';
import * as fs from 'fs/promises';

export const metadata = {
    name: "send_messenger_rpa",
    description: "Tự động hóa trình duyệt (Browser Automation) để gửi tin nhắn qua Facebook Messenger.",
    parameters: {
        type: "object",
        properties: {
            targetName: { type: "string", description: "Tên người nhận tin nhắn (Target recipient)" },
            message: { type: "string", description: "Nội dung tin nhắn cần gửi (Message payload)" }
        },
        required: ["targetName", "message"]
    }
};

export const execute = async (args: { targetName: string; message: string }): Promise<string> => {
    let browser;
    try {
        console.log(`[RPA] Khởi động robot trình duyệt (Launching Headless Browser)...`);

        // 1. Quản lý Phiên đăng nhập (Session Management)
        // Lưu lại dữ liệu (Cookies/Cache) để Anh chỉ phải đăng nhập bằng tay đúng 1 lần duy nhất
        const userDataDir = path.resolve(process.cwd(), 'data', 'browser_session');
        await fs.mkdir(userDataDir, { recursive: true });

        // 2. Mở trình duyệt có giao diện (headless: false) để Anh dễ quan sát robot làm việc
        browser = await puppeteer.launch({
            headless: false,
            userDataDir: userDataDir,
            defaultViewport: null,
            args: ['--start-maximized']
        });

        const page = await browser.newPage();
        await page.goto('https://www.messenger.com/', { waitUntil: 'networkidle2' });

        // 3. Kiểm tra xác thực (Authentication Check)
        if (page.url().includes('login')) {
            console.log(`[RPA] ⚠️ Yêu cầu đăng nhập lần đầu (First-time Authentication required).`);
            console.log(`[RPA] Anh Dương có 60 giây để tự gõ tài khoản và mật khẩu trên cửa sổ trình duyệt nhé!`);
            // Tạm dừng robot 60 giây để Anh đăng nhập
            await new Promise(r => setTimeout(r, 60000));
        }

        console.log(`[RPA] Bắt đầu tìm kiếm (Searching for) người nhận: ${args.targetName}`);

        // 4. Tìm thanh tìm kiếm và gõ tên (UI Interaction)
        const searchBoxSelector = '[aria-label="Tìm kiếm trên Messenger"]';
        await page.waitForSelector(searchBoxSelector, { timeout: 10000 });
        await page.click(searchBoxSelector);
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        // Nhấn Enter và mũi tên xuống để chọn người đầu tiên trong danh sách kết quả
        await new Promise(r => setTimeout(r, 2000));
        await page.keyboard.press('ArrowDown');
        await page.keyboard.press('Enter');

        // 5. Chuyển sang ô nhập tin nhắn và gõ nội dung (Drafting message)
        console.log(`[RPA] Đang soạn tin nhắn...`);
        await new Promise(r => setTimeout(r, 2000));

        // Thường ô chat của Messenger là một thẻ có role="textbox"
        const chatBoxSelector = '[role="textbox"]';
        await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
        await page.click(chatBoxSelector);
        await page.keyboard.type(args.message, { delay: 50 }); // delay 50ms giữa mỗi phím để giống người thật gõ

        // 6. Gửi tin nhắn (Dispatching message)
        await page.keyboard.press('Enter');
        console.log(`[RPA] Đã gửi tin nhắn thành công (Message dispatched)!`);

        // Chờ 3 giây để tin nhắn kịp bay đi trước khi đóng phần mềm
        await new Promise(r => setTimeout(r, 3000));
        await browser.close();

        return `Hoàn tất (Successfully sent): Đã tự động gửi tin nhắn tới ${args.targetName}`;
    } catch (error: any) {
        if (browser) await browser.close();
        return `Lỗi quá trình tự động hóa (RPA Error): ${error.message}`;
    }
};