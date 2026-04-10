const axios = require('axios');

(async () => {
    try {
        const token = "3988463180558233875:yoFVlFuocjopSXErqRneTzGISddKfozONAeIveqWTkLrxLuXhoZLzpnHjZDWdBdx";
        
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
