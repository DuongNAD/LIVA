const axios = require('axios');

// ⚠️  Token được đọc từ biến môi trường — KHÔNG BAO GIỜ hardcode token vào source code
// Tạo file .env ở gốc project với: ZALO_OA_ACCESS_TOKEN=your_token
const token = process.env.ZALO_OA_ACCESS_TOKEN;

if (!token) {
    console.error('[get_zalo_id] ZALO_OA_ACCESS_TOKEN is not set. Aborting.');
    process.exit(1);
}

(async () => {
    try {
        console.log("Fetching recent chats...");
        let res = await axios.get('https://openapi.zalo.me/v2.0/oa/listrecentchat?data={"offset":0,"count":5}', {
            headers: { access_token: token }
        });
        console.log(JSON.stringify(res.data, null, 2));

        console.log("\nFetching conversations...");
        res = await axios.get('https://openapi.zalo.me/v2.0/oa/conversation?data={"offset":0,"count":5}', {
            headers: { access_token: token }
        });
        console.log(JSON.stringify(res.data, null, 2));
    } catch (e) {
        console.log(e.response?.data || e.message);
    }
})();
