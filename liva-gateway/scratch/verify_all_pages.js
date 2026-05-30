import { chromium } from "playwright-core";
import { getSystemChromePath } from "../src/utils/PlaywrightBrowser.js";
import * as path from "node:path";
import * as fs from "node:fs";

async function run() {
  console.log("Starting full page verification...");
  const chromePath = getSystemChromePath();
  console.log(`Detected system chrome: ${chromePath}`);

  const browser = await chromium.launch({
    executablePath: chromePath,
    headless: true,
  });

  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
  });

  const page = await context.newPage();

  console.log("Navigating to http://localhost:5173/dashboard.html ...");
  await page.goto("http://localhost:5173/dashboard.html", { waitUntil: "networkidle", timeout: 15000 }).catch(e => {
    console.log("Network idle timeout, proceeding...");
  });

  console.log("Waiting 5s for dashboard to load...");
  await page.waitForTimeout(5000);

  const artifactDir = "C:\\Users\\Admin\\.gemini\\antigravity-ide\\brain\\7fe53414-068f-496f-aca0-82d05bb2c5e1";
  if (!fs.existsSync(artifactDir)) {
    fs.mkdirSync(artifactDir, { recursive: true });
  }

  // List of selectors and page IDs in order of mainNavItems + settingsItem
  const pagesToTest = [
    { id: "1_avatar", selector: ".sidebar-nav button:nth-child(1)" },
    { id: "2_ai", selector: ".sidebar-nav button:nth-child(2)" },
    { id: "3_api", selector: ".sidebar-nav button:nth-child(3)" },
    { id: "4_voice", selector: ".sidebar-nav button:nth-child(4)" },
    { id: "5_tasks", selector: ".sidebar-nav button:nth-child(5)" },
    { id: "6_memory", selector: ".sidebar-nav button:nth-child(6)" },
    { id: "7_skills", selector: ".sidebar-nav button:nth-child(7)" },
    { id: "8_system", selector: ".sidebar-nav button:nth-child(8)" },
    { id: "9_profile", selector: ".sidebar-nav button:nth-child(9)" },
    { id: "10_settings", selector: ".sidebar-footer button" },
  ];

  for (const pageInfo of pagesToTest) {
    console.log(`Testing page: ${pageInfo.id} using selector: ${pageInfo.selector}`);
    try {
      // Click the navigation item
      await page.click(pageInfo.selector);
      
      // Wait for page transition and content loading
      await page.waitForTimeout(2000);

      // Check if there is a global error on the page
      const hasError = await page.evaluate(() => {
        const errEl = document.querySelector(".dashboard-content");
        return errEl ? errEl.textContent.includes("Dashboard Error Captured") : false;
      });

      if (hasError) {
        console.error(`❌ Page ${pageInfo.id} has a rendering error!`);
      } else {
        console.log(`✅ Page ${pageInfo.id} loaded without errors.`);
      }

      // Take screenshot
      const imgPath = path.join(artifactDir, `page_${pageInfo.id}.png`);
      await page.screenshot({ path: imgPath });
      console.log(`Saved screenshot to: ${imgPath}`);
    } catch (err) {
      console.error(`❌ Error testing page ${pageInfo.id}:`, err.message);
    }
  }

  await browser.close();
  console.log("Full page verification completed!");
}

run().catch((err) => {
  console.error("❌ Full page verification failed:", err);
  process.exit(1);
});
