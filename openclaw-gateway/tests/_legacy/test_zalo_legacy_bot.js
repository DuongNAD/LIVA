const axios = require('axios');

(async () => {
    try {
        const token = "1295422184960370610:yLlEndsCtRvFBaNiTWGTsBlvIeOBhUqAulcYuiUQEihkZrLgXPUBvlcdlSZEDxiR";
        const userId = "21b8b8c785936ccd3582";
        
        console.log(`[Test Script] Đang gửi tin thử nghiệm tới ID ${userId} bằng token CŨ...`);
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
