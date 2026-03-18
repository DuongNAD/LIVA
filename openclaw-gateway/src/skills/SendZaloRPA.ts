import puppeteer from 'puppeteer';
import * as path from 'path';
import * as fs from 'fs/promises';

export const metadata = {
    name: "send_zalo_rpa",
    description: "Tự động hóa trình duyệt (Browser Automation) để gửi tin nhắn qua Zalo Web.",
    parameters: {
        type: "object",
        properties: {
            targetName: { type: "string", description: "Số điện thoại hoặc tên người nhận tin nhắn (Phone number or Name)" },
            message: { type: "string", description: "Nội dung tin nhắn cần gửi (Message payload)" }
        },
        required: ["targetName", "message"]
    }
};

export const execute = async (args: { targetName: string; message: string }): Promise<string> => {
    let browser;
    try {
        console.log(`[RPA Zalo] Khởi động trình duyệt (Launching Headless Browser)...`);

        const livaProfileDir = path.resolve(process.cwd(), 'data', 'liva_zalo_profile');
        await fs.mkdir(livaProfileDir, { recursive: true });

        browser = await puppeteer.launch({
            headless: false,
            userDataDir: livaProfileDir,
            defaultViewport: null,
            args: [
                '--start-maximized',
                '--disable-extensions'
            ]
        });

        // Lấy trang trống hiện tại
        const pages = await browser.pages();
        const page = pages.length > 0 ? pages[0] : await browser.newPage();
        
        console.log(`[RPA Zalo] Đang điều hướng đến Zalo Web...`);
        // Đi tới trang chủ Zalo Chat
        await page.goto('https://chat.zalo.me/', { waitUntil: 'domcontentloaded', timeout: 60000 });

        // Kiểm tra xem đã đăng nhập chưa (Nếu còn nút Đăng nhập với mã QR)
        const isLoginPage = await page.evaluate(() => {
            return document.body.innerText.includes('Quét mã QR');
        }).catch(() => false);

        if (isLoginPage || page.url().includes('login')) {
            console.log(`[RPA Zalo] ⚠️ Yêu cầu đăng nhập lần đầu (Authentication required).`);
            console.log(`[RPA Zalo] Anh có 2 phút để mở ứng dụng Zalo trên điện thoại và quét mã QR nhé!`);
            await new Promise(r => setTimeout(r, 120000));
        }

        console.log(`[RPA Zalo] Bắt đầu tìm người nhận: ${args.targetName}`);

        // Chờ thanh tìm kiếm xuất hiện (Thường là ô input có id='contact-search-input' hoặc placeholder Tìm kiếm)
        const searchBoxSelector = '#contact-search-input';
        
        // Đợi Zalo tải xong danh bạ (có thể mất vài giây)
        await page.waitForSelector(searchBoxSelector, { timeout: 30000 });
        
        // Click vào ô tìm kiếm và gõ tên/số điện thoại
        await page.click(searchBoxSelector);
        // Clear ô tìm kiếm trước (nhấn Ctrl A + Backspace)
        await page.keyboard.down('Control');
        await page.keyboard.press('A');
        await page.keyboard.up('Control');
        await page.keyboard.press('Backspace');
        
        // Gõ tên người gửi
        await page.keyboard.type(args.targetName, { delay: 100 });
        
        // Đợi kết quả tìm kiếm hiện ra
        await new Promise(r => setTimeout(r, 2000));
        
        // Zalo tự động focus vào người đầu tiên trong kết quả tìm kiếm, chỉ cần ấn mũi tên xuống và Enter
        await page.keyboard.press('ArrowDown');
        await page.keyboard.press('Enter');

        console.log(`[RPA Zalo] Đang soạn tin nhắn...`);
        // Đợi load khung chat
        await new Promise(r => setTimeout(r, 2000));

        // Zalo dùng div contenteditable thay vì input thông thường cho khung chat
        // Thường có id='richInput'
        const chatBoxSelector = '#richInput';
        await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
        await page.click(chatBoxSelector);
        
        // Gõ nội dung tin nhắn
        await page.keyboard.type(args.message, { delay: 50 });

        // Nhấn Enter để gửi
        await page.keyboard.press('Enter');
        console.log(`[RPA Zalo] Đã gửi tin nhắn (Message dispatched)!`);

        // Chờ một xíu để tin cập nhật lên server
        await new Promise(r => setTimeout(r, 3000));
        
        // Tự động đóng trình duyệt cho nhẹ máy (đã login thành công roi)
        await browser.close();

        return `Hoàn tất (Successfully sent): Đã gởi tin nhắn Zalo cho ${args.targetName}`;
    } catch (error: any) {
        if (browser) await browser.close();
        return `Lỗi hệ thống cửa sổ (Zalo RPA Error): ${error.message}`;
    }
};
