const axios = require('axios');
require('dotenv').config();

(async () => {
    try {
        console.log(`[Test Script] Đang kiểm tra tích hợp Zalo Bot API...`);

        const accessToken = process.env.ZALO_OA_ACCESS_TOKEN;
        let userId = process.env.ZALO_USER_ID;

        if (!accessToken || accessToken.includes("NHẬP_TOKEN")) {
            console.error(`Lỗi: Chưa có ZALO_OA_ACCESS_TOKEN`);
            return;
        }

        const isBotCreatorToken = accessToken.includes(':');

        if (isBotCreatorToken) {
            console.log(`[Test Script] Đã phát hiện Bot Mới (Bot Creator Token)`);
            if (!userId || userId.includes("NHẬP_USER_ID") || userId === "21b8b8c785936ccd3582") { 
                console.log(`[Test Script] Đang quét API dò tìm ID tự động...`);
                try {
                    const updateRes = await axios.post(`https://bot-api.zaloplatforms.com/bot${accessToken}/getUpdates`, { timeout: "5" });
                    
                    if (updateRes.data && updateRes.data.ok && updateRes.data.result && updateRes.data.result.message) {
                        userId = updateRes.data.result.message.chat.id;
                        console.log(`[Test Script] Magic! Đã tự động bắt được User ID mới: ${userId}`);
                    } else {
                        console.error(`Lỗi hệ thống: Không tìm thấy tin nhắn nào trong inbox của Bot để bắt User ID. Tin nhắn API trả về:`, JSON.stringify(updateRes.data));
                        return;
                    }
                } catch(e) {
                    console.error(`Lỗi tự động dò User ID:`, e.response?.data || e.message);
                    return;
                }
            }

            console.log(`[Test Script] Đang gửi tin nhắn thử nghiệm tới ID: ${userId}`);
            const endpoint = `https://bot-api.zaloplatforms.com/bot${accessToken}/sendMessage`;
            const payload = {
                chat_id: userId,
                text: "Xin chào! Đây là tin nhắn test từ hệ thống Auto-Discovery của Liva AI! 🎉"
            };

            const response = await axios.post(endpoint, payload);
            if (response.data && response.data.ok) {
                console.log(`[Test Script] HOÀN TẤT! Đã gửi tin nhắn rọt rẹt qua zalo!`);
            } else {
                console.error(`Zalo Bot API Error:`, response.data.description);
            }

        } else {
            console.log(`[Test Script] Đây là Zalo OA Token (Hệ cũ). Bỏ qua test...`);
        }
        
    } catch (error) {
        console.error(`Lỗi ngoại lệ:`, error.response?.data || error.message);
    }
})();
