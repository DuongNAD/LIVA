import dotenv from "dotenv";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load env
dotenv.config({ path: path.join(__dirname, "..", ".env") });

import { execute } from "../src/skills/core/GetWeather.ts";

async function run() {
  console.log("Starting weather test...");
  console.log("API Key loaded from env:", process.env.WEATHER_API_KEY ? `${process.env.WEATHER_API_KEY.substring(0, 10)}...` : "None");
  try {
    const result = await execute({ location: "Hanoi", days: 1 });
    console.log("Result:\n", result);
  } catch (error) {
    console.error("Execution error:", error);
  }
}

run();
