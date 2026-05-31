/* eslint-disable no-console */
import { chromium } from "playwright-core";
import { getSystemChromePath } from "./utils/PlaywrightBrowser.js";

async function run() {
  console.log("Launching browser...");
  const browser = await chromium.launch({
    executablePath: getSystemChromePath(),
    headless: true
  });
  const page = await browser.newPage();
  
  page.on("console", (msg) => {
    console.log(`[BROWSER CONSOLE] [${msg.type()}] ${msg.text()}`);
    if (msg.type() === "error") {
      const location = msg.location();
      console.log(`  at ${location.url}:${location.lineNumber}:${location.columnNumber}`);
    }
  });

  page.on("pageerror", (err) => {
    console.log("[BROWSER ERROR]", err);
  });

  console.log("Navigating to http://localhost:5173/dashboard.html...");
  await page.goto("http://localhost:5173/dashboard.html");
  await page.waitForTimeout(3000); // wait for 3D and data load

  console.log("Clicking L0 RAM Cache tab...");
  await page.click(".l0-stat");
  await page.waitForTimeout(2000);

  console.log("Clicking L3 Facts tab...");
  await page.click(".facts-stat");
  await page.waitForTimeout(2000);

  console.log("Closing browser...");
  await browser.close();
}

run().catch(console.error);
