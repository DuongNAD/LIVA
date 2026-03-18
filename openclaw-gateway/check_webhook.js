const axios = require('axios');
require('dotenv').config();

(async () => {
    try {
        const token = process.env.ZALO_OA_ACCESS_TOKEN;
        console.log("Token:", token.substring(0, 20) + "...");
        
        console.log("Checking Webhook Info...");
        const res = await axios.post(`https://bot-api.zaloplatforms.com/bot${token}/getWebhookInfo`);
        console.log(JSON.stringify(res.data, null, 2));

        console.log("Checking getUpdates again just in case...");
        const updateRes = await axios.post(`https://bot-api.zaloplatforms.com/bot${token}/getUpdates`);
        console.log(JSON.stringify(updateRes.data, null, 2));
    } catch (e) {
        console.log(e.response?.data || e.message);
    }
})();
