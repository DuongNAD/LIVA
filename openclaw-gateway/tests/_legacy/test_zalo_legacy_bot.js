const axios = require('axios');

// ⚠️  Token được đọc từ biến môi trường — KHÔNG BAO GIỜ hardcode token vào source code
const token = process.env.ZALO_OA_ACCESS_TOKEN;
const userId = process.env.ZALO_TEST_USER_ID || "21b8b8c785936ccd3582";

if (!token) {
    console.error('[test_zalo_legacy_bot] ZALO_OA_ACCESS_TOKEN is not set. Aborting.');
    process.exit(1);
}

(async () => {
    try {
        console.log(`[Test Script] Đang gửi tin thử nghiệm tới ID ${userId} bằng token ENV...`);
        const endpoint = `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`;
        const payload = {
            chat_id: userId,
            text: "Xin chào! Đây là Liva AI. TÔI ĐÃ SỐNG LẠI TỪ CÕI CHẾT!! 🎉 (Gửi qua hệ Bot Creator)"
        };

        const response = await axios.post(endpoint, payload);
        if (response.data && response.data.ok) {
            console.log(`[Test Script] HOÀN TẤT! Tin nhắn đã được gửi thành công!`);
        } else {
            console.error(`Zalo Bot API Error:`, response.data);
        }
        
    } catch (error) {
        console.error(`Lỗi ngoại lệ:`, error.response?.data || error.message);
    }
})();
