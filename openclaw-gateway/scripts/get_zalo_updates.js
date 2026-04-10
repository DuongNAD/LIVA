const axios = require('axios');

(async () => {
    try {
        const token = "3988463180558233875:yoFVlFuccjopSXErqRneTzGISddKfozONAeIveqWTkLrxLuXhoZLzpnHjZDWdBdx";
        console.log("Checking for pending updates on the new Zalo Bot Platform...");
        const res = await axios.post(`https://bot-api.zaloplatforms.com/bot${token}/getUpdates`, { timeout: 5 });
        console.log(JSON.stringify(res.data, null, 2));
    } catch (e) {
        console.log(e.response?.data || e.message);
    }
})();
