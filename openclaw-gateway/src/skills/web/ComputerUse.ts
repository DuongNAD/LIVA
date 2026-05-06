import { RPAGuardrails } from "@security/RPAGuardrails";

/**
 * ComputerUse — General-Purpose Browser Automation Skill
 * ======================================================
 * Allows LIVA to interact with any website on the user's real computer.
 * Supports: navigate, click, type, scroll, read page content, screenshot.
 * 
 * Security:
 *   - Sensitive domain detection (banking, payment) → requires awareness
 *   - All actions logged via RPAGuardrails audit trail
 *   - PII detection on typed content
 *   - Singleton browser with user data persistence
 * 
 * Powered by playwright-core — uses system Chrome/Edge (no bundled browser).
 */

export const metadata = {
    name: "computer_use",
    search_keywords: ["computer_use", "browse", "google", "tìm kiếm", "mở web", "trình duyệt", "trang web", "open browser"],
    description: "Mở trình duyệt Chrome và thao tác trên các trang web thật của người dùng. Có thể tìm Google, đọc trang web, điền form, click nút, cuộn trang. Dùng khi người dùng yêu cầu tương tác với bất kỳ trang web nào.",
    isCoreSkill: false,
    parameters: {
        type: "object",
        properties: {
            action: {
                type: "string",
                enum: ["navigate", "click", "type", "scroll", "read_page", "screenshot", "google_search"],
                description: "Hành động cần thực hiện: navigate (mở URL), click (bấm phần tử), type (gõ chữ), scroll (cuộn trang), read_page (đọc nội dung), screenshot (chụp màn hình), google_search (tìm Google nhanh)"
            },
            url: {
                type: "string",
                description: "URL trang web cần mở (dùng cho action=navigate)"
            },
            selector: {
                type: "string",
                description: "CSS selector của phần tử cần tương tác (dùng cho action=click/type)"
            },
            text: {
                type: "string",
                description: "Nội dung cần gõ (dùng cho action=type) hoặc từ khóa tìm kiếm (dùng cho action=google_search)"
            },
            direction: {
                type: "string",
                enum: ["up", "down"],
                description: "Hướng cuộn trang (dùng cho action=scroll)"
            }
        },
        required: ["action"]
    }
};

/**
 * Extract readable text content from the current page
 */
async function extractPageContent(page: Page, maxLength: number = 3000): Promise<string> {
    try {
        const content = await page.evaluate(() => {
            // Remove script, style, nav elements
            const elementsToRemove = document.querySelectorAll("script, style, nav, footer, header, aside, iframe");
            elementsToRemove.forEach(el => el.remove());

            const body = document.body;
            if (!body) return "";

            // Get text content, clean up whitespace
            let text = body.innerText || body.textContent || "";
            text = text.replaceAll(/\n{3,}/g, "\n\n").trim();
            return text;
        });

        const title = await page.title();
        const url = page.url();

        let result = `[Trang web: ${title}]\n[URL: ${url}]\n\n`;
        result += content.substring(0, maxLength);
        if (content.length > maxLength) {
            result += "\n\n... (Nội dung đã được cắt ngắn để tiết kiệm token)";
        }
        return result;
    } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
        return `[Lỗi đọc trang]: ${errMsg}`;
    }
}

export const execute = async (args: {
    action: string;
    url?: string;
    selector?: string;
    text?: string;
    direction?: string;
}): Promise<string> => {
    try {
        const { context } = await getOrCreateBrowser("computer_use");
        const page = await getActivePage(context);

        RPAGuardrails.logAction("computer_use", args.action, args.url || args.selector || "", args.text || "", false, "allowed");

        switch (args.action) {
            // ==========================================
            // NAVIGATE — Mở URL
            // ==========================================
            case "navigate": {
                if (!args.url) return "Lỗi: Thiếu tham số 'url'. Hãy cung cấp URL cần mở.";
                
                // Sensitive domain check
                if (RPAGuardrails.isSensitiveDomain(args.url)) {
                    logger.warn(`[ComputerUse/Guard] ⚠️ Đang truy cập domain nhạy cảm: ${args.url}`);
                    RPAGuardrails.logAction("computer_use", "navigate_sensitive", args.url, "", false, "warned");
                }

                logger.info(`[ComputerUse] Đang mở: ${args.url}`);
                await page.goto(args.url, { waitUntil: "domcontentloaded", timeout: 30000 });
                // Playwright auto-waits — no manual setTimeout needed
                
                const title = await page.title();
                return `Đã mở trang: "${title}" (${args.url})`;
            }

            // ==========================================
            // GOOGLE SEARCH — Tìm Google nhanh
            // ==========================================
            case "google_search": {
                if (!args.text) return "Lỗi: Thiếu tham số 'text'. Hãy cung cấp từ khóa tìm kiếm.";

                logger.info(`[ComputerUse] Đang tìm Google: "${args.text}"`);
                const searchUrl = `https://www.google.com/search?q=${encodeURIComponent(args.text)}`;
                await page.goto(searchUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
                // Wait for search results to render
                await page.waitForSelector("div.g, div[data-hveid]", { timeout: 5000 }).catch(() => {});

                // Extract search results
                const results = await page.evaluate(() => {
                    const items: string[] = [];
                    const searchResults = document.querySelectorAll("div.g, div[data-hveid]");
                    searchResults.forEach((el, i) => {
                        if (i >= 5) return; // Top 5 results
                        const titleEl = el.querySelector("h3");
                        const snippetEl = el.querySelector("div[data-sncf], span[class]");
                        const linkEl = el.querySelector("a[href]");
                        if (titleEl) {
                            const title = titleEl.textContent || "";
                            const snippet = snippetEl?.textContent || "";
                            const link = linkEl?.getAttribute("href") || "";
                            items.push(`${i + 1}. ${title}\n   ${snippet.substring(0, 150)}\n   ${link}`);
                        }
                    });
                    return items.join("\n\n");
                });

                return `Kết quả Google cho "${args.text}":\n\n${results || "(Không trích xuất được kết quả, hãy thử read_page)"}`;
            }

            // ==========================================
            // CLICK — Bấm phần tử (Playwright auto-waits)
            // ==========================================
            case "click": {
                if (!args.selector) return "Lỗi: Thiếu tham số 'selector'. Hãy cung cấp CSS selector của phần tử cần click.";

                logger.info(`[ComputerUse] Đang click: ${args.selector}`);
                // Playwright locator auto-waits for element to be actionable
                await page.locator(args.selector).click({ timeout: 10000 });
                
                return `Đã click vào phần tử: ${args.selector}`;
            }

            // ==========================================
            // TYPE — Gõ chữ vào phần tử
            // ==========================================
            case "type": {
                if (!args.text) return "Lỗi: Thiếu tham số 'text'. Hãy cung cấp nội dung cần gõ.";

                // PII check on typed content
                const piiCheck = RPAGuardrails.scanForPII(args.text);
                if (piiCheck.hasPII) {
                    logger.warn(`[ComputerUse/Guard] ⚠️ PII detected in typed content: ${piiCheck.detectedTypes.join(", ")}`);
                }

                if (args.selector) {
                    await page.locator(args.selector).click({ timeout: 10000 });
                }
                await page.keyboard.type(args.text, { delay: 50 });
                
                return `Đã gõ "${args.text.substring(0, 30)}..." vào ${args.selector || "vị trí hiện tại"}`;
            }

            // ==========================================
            // SCROLL — Cuộn trang
            // ==========================================
            case "scroll": {
                const distance = args.direction === "up" ? -500 : 500;
                await page.evaluate((d) => window.scrollBy(0, d), distance);
                return `Đã cuộn trang ${args.direction === "up" ? "lên" : "xuống"}`;
            }

            // ==========================================
            // READ PAGE — Đọc nội dung trang
            // ==========================================
            case "read_page": {
                logger.info(`[ComputerUse] Đang đọc nội dung trang...`);
                const content = await extractPageContent(page);
                return content;
            }

            // ==========================================
            // SCREENSHOT — Chụp màn hình
            // ==========================================
            case "screenshot": {
                logger.info(`[ComputerUse] Đang chụp màn hình...`);
                const screenshotDir = path.join(process.cwd(), "data", "screenshots");
                await fs.mkdir(screenshotDir, { recursive: true });
                
                const filename = `screenshot_${Date.now()}.png`;
                const filepath = path.join(screenshotDir, filename);
                await page.screenshot({ path: filepath, fullPage: false });
                
                const title = await page.title();
                return `Đã chụp màn hình trang "${title}" và lưu tại: ${filepath}`;
            }

            default:
                return `Hành động không hợp lệ: "${args.action}". Các hành động hỗ trợ: navigate, google_search, click, type, scroll, read_page, screenshot`;
        }
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        RPAGuardrails.logAction("computer_use", args.action, args.url || "", errMsg, false, "blocked");
        return `Lỗi ComputerUse: ${errMsg}`;
    }
};
