import { safeFetch } from "../src/utils/HttpClient.js";

const token = "461340000852775250:PLMhrADNhmbbLFYluJtwqmPqLHXBcKRFpbWTUfvdXUlozWKnOTTQUSmYvtYYPjWM";

async function verifySend() {
  console.log("Testing sendMessage with dummy chat_id...");
  try {
    const res = await fetch(
      `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`,
      {
         method: "POST",
         headers: { "Content-Type": "application/json" },
         body: JSON.stringify({
           chat_id: "123456789",
           text: "Test"
         })
      }
    );
    const status = res.status;
    const data = await res.json() as any;
    console.log(`HTTP Status: ${status}`);
    console.log("Response:", JSON.stringify(data, null, 2));
    
    if (status === 200 && data.ok) {
      console.log("Token is valid and message sent!");
    } else if (status === 401 || (data && data.error_code === 401)) {
      console.log("Token is INVALID (Unauthorized 401)");
    } else {
      console.log("Token is VALID! (Server returned error for dummy chat_id as expected)");
    }
  } catch (err) {
    console.error("Error:", err);
  }
}

verifySend();
