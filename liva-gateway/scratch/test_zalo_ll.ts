import { safeFetch } from "../src/utils/HttpClient.js";

const token = "461340000852775250:PLMhrADNhmbbLFYluJtwqmPqLHXBcKRFpbWTUfvdXUlozWKnOTTQUSmYvtYYPjWM";

async function verify() {
  console.log("Verifying token with zero timeout...");
  try {
    const updateRes = await safeFetch(
      `https://bot-api.zaloplatforms.com/bot${token}/getUpdates`,
      {
         method: "POST",
         headers: { "Content-Type": "application/json" },
         body: JSON.stringify({ timeout: "10" })
      },
      30000
    );
    const data = await updateRes.json() as any;
    console.log("Response:", JSON.stringify(data, null, 2));
  } catch (err) {
    console.error("Error:", err);
  }
}

verify();
