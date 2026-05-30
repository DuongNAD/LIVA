import { chromium } from "playwright-core";
import { getSystemChromePath } from "../src/utils/PlaywrightBrowser.js";
import * as path from "node:path";
import * as fs from "node:fs";

async function run() {
  console.log("Starting Playwright verification...");
  const chromePath = getSystemChromePath();
  console.log(`Detected system chrome: ${chromePath}`);

  // Launch in headless mode
  const browser = await chromium.launch({
    executablePath: chromePath,
    headless: true,
  });

  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
  });

  const page = await context.newPage();

  // 1. Dashboard
  console.log("Navigating to dashboard: http://localhost:5173/dashboard.html ...");
  try {
    await page.goto("http://localhost:5173/dashboard.html", { waitUntil: "networkidle", timeout: 10000 });
  } catch (err) {
    console.warn("Network idle timed out or failed, proceeding anyway...", err.message);
  }

  // Wait a bit extra for dynamic content / WebSockets to connect
  console.log("Waiting 5 seconds for WebSockets and VRM components to initialize...");
  await page.waitForTimeout(5000);

  const artifactDir = "C:\\Users\\Admin\\.gemini\\antigravity-ide\\brain\\7fe53414-068f-496f-aca0-82d05bb2c5e1";
  if (!fs.existsSync(artifactDir)) {
    fs.mkdirSync(artifactDir, { recursive: true });
  }

  const dashboardPath = path.join(artifactDir, "dashboard_ui.png");
  await page.screenshot({ path: dashboardPath });
  console.log(`✅ Dashboard screenshot saved to: ${dashboardPath}`);

  // 2. Widget
  console.log("Navigating to widget: http://localhost:5173/widget.html ...");
  try {
    await page.goto("http://localhost:5173/widget.html", { waitUntil: "networkidle", timeout: 10000 });
  } catch (err) {
    console.warn("Network idle timed out or failed, proceeding anyway...", err.message);
  }

  console.log("Waiting 3 seconds for widget...");
  await page.waitForTimeout(3000);

  const widgetPath = path.join(artifactDir, "widget_ui.png");
  await page.screenshot({ path: widgetPath });
  console.log(`✅ Widget screenshot saved to: ${widgetPath}`);

  await browser.close();
  console.log("Playwright verification completed successfully!");
}

run().catch((err) => {
  console.error("❌ Playwright verification failed:", err);
  process.exit(1);
});
