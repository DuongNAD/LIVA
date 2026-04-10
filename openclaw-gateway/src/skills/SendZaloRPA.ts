import puppeteer from "puppeteer";
import * as path from "path";
import * as fs from "fs/promises";

export const metadata = {
  name: "send_zalo_rpa",
  description:
    "CHỈ DÙNG để thực hiện yêu cầu NHẮN TIN CHO NGƯỜI KHÁC (như Mẹ, Bạn bè, Đối tác). Mở giao diện để nhắn tin trực tiếp từ nick cá nhân. QUAN TRỌNG: TUYỆT ĐỐI KHÔNG dùng kỹ năng này để gửi báo cáo, tóm tắt công việc cho chính người dùng (hãy dùng send_zalo_bot thay thế).",
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
        description: "Nội dung tin nhắn cần gửi (Message payload)",
      },
    },
    required: ["targetName", "message"],
  },
};

export const execute = async (args: {
  targetName: string;
  message: string;
}): Promise<string> => {
  let browser;
  try {
    console.log(
      `[RPA Zalo] Khởi động trình duyệt (Launching Headless Browser)...`,
    );

    const livaProfileDir = path.resolve(
      process.cwd(),
      "data",
      "liva_zalo_profile",
    );
    await fs.mkdir(livaProfileDir, { recursive: true });

    browser = await puppeteer.launch({
      headless: false,
      userDataDir: livaProfileDir,
      defaultViewport: null,
      args: ["--start-maximized", "--disable-extensions"],
    });

    // Lấy trang trống hiện tại
    const pages = await browser.pages();
    const page = pages.length > 0 ? pages[0] : await browser.newPage();

    console.log(`[RPA Zalo] Đang điều hướng đến Zalo Web...`);
    // Đi tới trang chủ Zalo Chat
    await page.goto("https://chat.zalo.me/", {
      waitUntil: "domcontentloaded",
      timeout: 60000,
    });

    // Kiểm tra xem đã đăng nhập chưa (Nếu còn nút Đăng nhập với mã QR)
    const isLoginPage = await page
      .evaluate(() => {
        return document.body.innerText.includes("Quét mã QR");
      })
      .catch(() => false);

    if (isLoginPage || page.url().includes("login")) {
      console.log(
        `[RPA Zalo] ⚠️ Yêu cầu đăng nhập lần đầu (Authentication required).`,
      );
      console.log(
        `[RPA Zalo] Anh có 2 phút để mở ứng dụng Zalo trên điện thoại và quét mã QR nhé!`,
      );
      await new Promise((r) => setTimeout(r, 120000));
    }

    console.log(`[RPA Zalo] Bắt đầu tìm người nhận: ${args.targetName}`);

    // Chờ thanh tìm kiếm xuất hiện (Thường là ô input có id='contact-search-input' hoặc placeholder Tìm kiếm)
    const searchBoxSelector = "#contact-search-input";

    // Đợi Zalo tải xong danh bạ (có thể mất vài giây)
    await page.waitForSelector(searchBoxSelector, { timeout: 30000 });

    // Click vào ô tìm kiếm và gõ tên/số điện thoại
    await page.click(searchBoxSelector);
    // Clear ô tìm kiếm trước (nhấn Ctrl A + Backspace)
    await page.keyboard.down("Control");
    await page.keyboard.press("A");
    await page.keyboard.up("Control");
    await page.keyboard.press("Backspace");

    // Gõ tên người gửi
    await page.keyboard.type(args.targetName, { delay: 100 });

    // Đợi kết quả tìm kiếm hiện ra
    await new Promise((r) => setTimeout(r, 2000));

    // ==========================================
    // SAFETY CHECK 1: Kiểm tra có tìm thấy danh bạ hay không
    // ==========================================
    const isNotFound = await page.evaluate(() => {
       const text = document.body.innerText;
       return text.includes("Không tìm thấy kết quả") || text.includes("Không tìm thấy liên hệ");
    });
    
    // Nếu có dòng chữ không tìm thấy kết quả thì throw
    if (isNotFound) {
       throw new Error(`[Lỗi Nặng] Không tìm thấy người nhận "${args.targetName}" trong danh bạ Zalo. Vui lòng đọc đúng tên bạn bè! BÁO LẠI CHO NGƯỜI DÙNG.`);
    }

    // ==========================================
    // SAFETY CHECK 2: Cào tên trong DOM và Click chính xác, hoặc trả về danh sách gợi ý SẠCH
    // ==========================================
    const searchResult = await page.evaluate((target) => {
        // Lấy các thẻ đóng vai trò là tên người dùng để tránh bị cắt vụn chữ (DOM của Zalo)
        const els = Array.from(document.querySelectorAll('[class*="name" i], [class*="title" i], [class*="friend" i], .truncate'));
        let suggestions = new Set<string>();
        
        // Tách từ khóa tìm kiếm thành mảng (vd: "Khánh Vũ" -> ["khánh", "vũ"])
        const targetWords = target.toLowerCase().split(" ").filter((w: string) => w.length >= 2);
        
        for (let el of els) {
            // Dùng innerText để lấy text toàn vẹn không bị cắt bởi các thẻ span highlight
            const text = (el as HTMLElement).innerText ? (el as HTMLElement).innerText.trim() : "";
            
            // Lọc rác: Tên không quá dài, không rỗng
            if (text && text.length >= 2 && text.length <= 40 && !text.includes('\n')) {
                const textLower = text.toLowerCase();
                
                // 1. So khớp tuyệt đối -> Nhấn chọn
                if (textLower === target.toLowerCase()) {
                    let clickable = el.closest('div[role="button"], [class*="msg-item"], [class*="conv-item"], [class*="list-item"]');
                    if (clickable) {
                        (clickable as HTMLElement).click();
                        return { clicked: true, suggestions: [] };
                    } else {
                        (el as HTMLElement).click();
                        return { clicked: true, suggestions: [] };
                    }
                }
                
                // 2. Gom gợi ý thông minh (Lọc rác): Chỉ lấy những tên CHỨA ít nhất 1 từ trong tên mục tiêu
                // Ví dụ tìm "Khánh Vũ", nó sẽ nhặt những người tên "Khánh" hoặc "Khánh Nguyễn" chứ không nhặt chữ "Danh bạ" hay "Tải Zalo"
                let isRelated = targetWords.some((w: string) => textLower.includes(w));
                
                // Hoặc nếu tên quá ngắn không chia từ được thì so ngược lại
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
        throw new Error(`[Lỗi Xác Nhận] Không tìm thấy tên khớp MỘT TRĂM PHẦN TRĂM với "${args.targetName}". Các tên liên quan (nghi ngờ) đang hiện trên Zalo là: [${searchResult.suggestions.length > 0 ? searchResult.suggestions.join(", ") : "Không có tên nào giống"}]. HÃY LIỆT KÊ CHO NGƯỜI DÙNG CHỌN.`);
    }

    console.log(`[RPA Zalo] Đang soạn tin nhắn...`);
    // Đợi load khung chat
    await new Promise((r) => setTimeout(r, 2000));

    // Zalo dùng div contenteditable thay vì input thông thường cho khung chat
    // Thường có id='richInput'
    const chatBoxSelector = "#richInput";
    await page.waitForSelector(chatBoxSelector, { timeout: 10000 });
    await page.click(chatBoxSelector);

    // Gõ nội dung tin nhắn
    await page.keyboard.type(args.message, { delay: 50 });

    // Nhấn Enter để gửi
    await page.keyboard.press("Enter");
    console.log(`[RPA Zalo] Đã gửi tin nhắn (Message dispatched)!`);

    // Chờ một xíu để tin cập nhật lên server
    await new Promise((r) => setTimeout(r, 3000));

    // Tự động đóng trình duyệt cho nhẹ máy (đã login thành công roi)
    await browser.close();

    return `Hoàn tất (Successfully sent): Đã gởi tin nhắn Zalo cho ${args.targetName}`;
  } catch (error: any) {
    if (browser) await browser.close();
    return `Lỗi hệ thống cửa sổ (Zalo RPA Error): ${error.message}`;
  }
};
