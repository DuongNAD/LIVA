import { safeFetch } from "../src/utils/HttpClient.js";

const baseBotId = "461340000852775250";

// Variations for the two ambiguous positions (represented as [l, I, 1])
const pos1Options = ["l", "I", "1"];
const pos2Options = ["l", "I", "1"];

async function bruteForce() {
  console.log("Starting Zalo Bot Token verification...");
  
  for (const p1 of pos1Options) {
    for (const p2 of pos2Options) {
      // Construct candidate secret
      // PLMhrADNhmbbLFY [p1] uJtwqmPqLHX BcKRFpbWTUfvdXU [p2] ozWKnOTTQUSmYvtYYPjWM
      const secret = `PLMhrADNhmbbLFY${p1}uJtwqmPqLHXBcKRFpbWTUfvdXU${p2}ozWKnOTTQUSmYvtYYPjWM`;
      const token = `${baseBotId}:${secret}`;
      
      try {
        const updateRes = await safeFetch(
          `https://bot-api.zaloplatforms.com/bot${token}/getUpdates`,
          {
             method: "POST",
             headers: { "Content-Type": "application/json" },
             body: JSON.stringify({ timeout: "1" })
          },
          3000
        );
        const data = await updateRes.json() as any;
        
        if (data && data.ok) {
          console.log(`\n🎉 SUCCESS! Found valid token:`);
          console.log(`Token: ${token}`);
          console.log(`Response:`, JSON.stringify(data, null, 2));
          return token;
        } else {
          console.log(`Failed variation (pos1: ${p1}, pos2: ${p2}) -> ${data.description || "Error"}`);
        }
      } catch (e: any) {
        console.log(`Error variation (pos1: ${p1}, pos2: ${p2}) -> ${e.message}`);
      }
    }
  }
  
  console.log("\n❌ All combinations failed. Please check the token image again.");
  return null;
}

bruteForce();
