import { chromium } from "playwright-core";
import { getSystemChromePath } from "../src/utils/PlaywrightBrowser.js";
import * as path from "node:path";
import * as fs from "node:fs";

async function run() {
  console.log("Starting deep tab verification...");
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

  // 1. Walk through all primary pages in the sidebar
  const primaryPages = [
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

  for (const pageInfo of primaryPages) {
    console.log(`Navigating to sidebar item: ${pageInfo.id}`);
    try {
      await page.click(pageInfo.selector);
      await page.waitForTimeout(2000);

      // Verify page content loaded without crash
      const hasError = await page.evaluate(() => {
        const errEl = document.querySelector(".dashboard-content");
        return errEl ? errEl.textContent.includes("Dashboard Error Captured") : false;
      });
      if (hasError) {
        console.error(`❌ Page ${pageInfo.id} has a crash!`);
      }

      // Take standard screenshot
      const imgPath = path.join(artifactDir, `page_${pageInfo.id}.png`);
      await page.screenshot({ path: imgPath });
      console.log(`Saved screenshot: ${imgPath}`);

      // 2. If this is the memory viewer page, click through all sub-tabs
      if (pageInfo.id === "6_memory") {
        console.log("Memory tab detected. Checking sub-tabs...");
        const memoryTabs = [
          { name: "L0 RAM Cache", selector: ".l0-stat" },
          { name: "L0.5 Session", selector: ".l0-5-stat" },
          { name: "L3 Facts", selector: ".facts-stat" },
          { name: "L2 Events", selector: ".events-stat" },
          { name: "L1 Vectors", selector: ".vectors-stat" },
        ];

        for (const tab of memoryTabs) {
          console.log(`Clicking memory sub-tab: ${tab.name}`);
          await page.click(tab.selector);
          await page.waitForTimeout(1500);

          const subTabImgPath = path.join(artifactDir, `memory_tab_${tab.name.toLowerCase().replace(/[\s.]/g, "_")}.png`);
          await page.screenshot({ path: subTabImgPath });
          console.log(`Saved sub-tab screenshot: ${subTabImgPath}`);
        }
      }
    } catch (err) {
      console.error(`❌ Error on sidebar item ${pageInfo.id}:`, err.message);
    }
  }

  await browser.close();
  console.log("Deep tab verification completed!");
}

run().catch((err) => {
  console.error("❌ Deep tab verification failed:", err);
  process.exit(1);
});
