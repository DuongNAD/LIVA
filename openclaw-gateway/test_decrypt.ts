import { EncryptionEngine } from "./src/memory/EncryptionEngine.js";
import * as dotenv from "dotenv";
import * as fs from "node:fs";

dotenv.config();

console.log("LIVA_ENCRYPTION_KEY:", process.env.LIVA_ENCRYPTION_KEY);

const vaultPath = "../data/liva_vault.json";
const vaultData = JSON.parse(fs.readFileSync(vaultPath, "utf-8"));

for (const [key, value] of Object.entries(vaultData)) {
    console.log(key, "=>", EncryptionEngine.decrypt(value as string));
}
