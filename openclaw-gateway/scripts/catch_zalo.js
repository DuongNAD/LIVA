const axios = require('axios');
const fs = require('fs');
require('dotenv').config();

const token = process.env.ZALO_OA_ACCESS_TOKEN;

if (!token || !token.includes(':')) {
    console.error("Lỗi: Không tìm thấy Token hợp lệ trong file .env!");
    process.exit(1);
}

console.log("=========================================");
console.log("📡 ĐANG QUÉT TÌM ZALO CHAT ID CỦA ANH...");
console.log("👉 BÂY GIỜ anh hãy mở điện thoại, nhắn chữ 'ping' (hoặc bất cứ chữ gì) cho con Bot Liva learning đi!");
console.log("=========================================\n");

const poll = async () => {
    try {
        process.stdout.write("Đang nghe (Listening)... ");
        const res = await axios.post(`https://bot-api.zaloplatforms.com/bot${token}/getUpdates`, { timeout: "10" });
        
        if (res.data && res.data.ok && res.data.result && res.data.result.message) {
            const userId = res.data.result.message.chat.id;
            console.log(`\n🎉 THÀNH CÔNG! ĐÃ BẮT ĐƯỢC ZALO USER ID MỚI: ${userId}`);
            
            // Tự động ghi vào file env
            let envData = fs.readFileSync('.env', 'utf8');
            envData = envData.replace(/ZALO_USER_ID=.*/g, `ZALO_USER_ID=${userId}`);
            fs.writeFileSync('.env', envData);
            
            console.log("✅ Đã tự động lưu Chat ID vào cấu hình Liva (.env)!");
            console.log("\n🚀 XONG! Bây giờ anh có thể tắt bảng này và chạy hệ thống Liva bình thường.");
            process.exit(0);
        } else {
            console.log("Chưa có tin nhắn nào tới. (Timeout) Tiếp tục chờ...");
            setTimeout(poll, 1000);
        }
    } catch (error) {
        if (error.response && error.response.status === 408) {
            console.log("Chưa có tin... Tiếp tục đợi (408).");
            setTimeout(poll, 1000);
        } else {
            console.error("\nLỗi API:", error.response?.data || error.message);
            setTimeout(poll, 2000);
        }
    }
};

poll();
