import dotenv from "dotenv";
import path from "path";
import * as fs from "fs/promises";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load env
dotenv.config({ path: path.join(__dirname, "..", ".env") });

import { EncryptionEngine } from "../src/memory/EncryptionEngine.js";

async function run() {
  const token = "461340000852775250:PLMhrADNhmbbLFYluJtwqmPqLHXBcKRFpbWTUfvdXUlozWKnOTTQUSmYvtYYPjWM";
  const vaultPath = path.join(__dirname, "..", "..", "data", "liva_vault.json");
  
  console.log("Vault path:", vaultPath);
  try {
    const rawVault = await fs.readFile(vaultPath, "utf8");
    const vaultData = JSON.parse(rawVault);
    
    // Encrypt new token
    console.log("Encrypting new token...");
    const encryptedToken = EncryptionEngine.encrypt(token);
    vaultData["ZALO_OA_ACCESS_TOKEN"] = encryptedToken;
    
    // Save vault back
    await fs.writeFile(vaultPath, JSON.stringify(vaultData, null, 2), "utf8");
    console.log("✅ Successfully updated ZALO_OA_ACCESS_TOKEN in liva_vault.json!");
  } catch (err) {
    console.error("Error:", err);
  }
}

run();
