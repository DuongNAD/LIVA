const axios = require('axios');

// ⚠️  Token được đọc từ biến môi trường — KHÔNG BAO GIỜ hardcode token vào source code
const token = process.env.ZALO_OA_ACCESS_TOKEN;

if (!token) {
    console.error('[get_zalo_updates] ZALO_OA_ACCESS_TOKEN is not set. Aborting.');
    process.exit(1);
}

(async () => {
    try {
        console.log("Checking for pending updates on the new Zalo Bot Platform...");
        const res = await axios.post(`https://bot-api.zaloplatforms.com/bot${token}/getUpdates`, { timeout: 5 });
        console.log(JSON.stringify(res.data, null, 2));
    } catch (e) {
        console.log(e.response?.data || e.message);
    }
})();
