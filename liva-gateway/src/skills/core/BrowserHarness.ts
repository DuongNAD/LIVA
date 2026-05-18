import { CDPClient } from "@utils/CDPClient";
import { ChromeLauncher } from "@utils/ChromeLauncher";
import {
    navigateAndGetInfo,
    getAxSnapshot,
    getInteractiveSnapshot,
    clickByAxId,
    typeIntoElement,
    scrollPage,
    extractPageText,
    pressKey,
    takeScreenshot,
} from "@utils/CDPHelpers";
import { RPAGuardrails } from "@security/RPAGuardrails";
import { logger } from "@utils/logger";
import * as path from "node:path";

/**
 * BrowserHarness — CDP-Native Browser Automation Skill for LIVA
 * ==============================================================
 * The agentic browser skill that replaces ComputerUse for intelligent web tasks.
 * 
 * Key Differences from ComputerUse.ts:
 *   - Uses CDP WebSocket directly (no Playwright middleware)
 *   - Uses Accessibility Tree instead of CSS selectors (95% fewer tokens)
 *   - Semantic element targeting via AxTree IDs (more robust than CSS)
 *   - Self-healing architecture ready (Level 2 via vm module, not active yet)
 * 
 * Workflow for E4B:
 *   1. navigate → open a URL
 *   2. ax_snapshot → get compact semantic view of the page
 *   3. click_element(id=X) → click element from AxTree
 *   4. type_text(id=X, text=...) → type into input
 *   5. Repeat 2-4 as needed
 * 
 * Security:
 *   - All actions audited via RPAGuardrails
 *   - Sensitive domain detection
 *   - CDP command blocking (Security.*, Fetch.*)
 *   - Rate limiting: max 100 actions/minute
 */

// ============================================================
// Singleton CDP Client
// ============================================================

let cdpClient: CDPClient | null = null;
let actionCountThisMinute = 0;
let rateLimitResetTimer: ReturnType<typeof setInterval> | null = null;

const MAX_ACTIONS_PER_MINUTE = 100;

function startRateLimitTimer(): void {
    if (rateLimitResetTimer) return;
    rateLimitResetTimer = setInterval(() => {
        actionCountThisMinute = 0;
    }, 60_000);
}

/**
 * Get or create a connected CDP client.
 */
async function getOrCreateCDP(): Promise<CDPClient> {
    if (cdpClient?.isConnected) {
        return cdpClient;
    }

    // Clean up zombie Chrome processes from previous crashes
    await ChromeLauncher.cleanupZombies();

    // Launch Chrome with dynamic port
    const wsUrl = await ChromeLauncher.launchOrConnect();

    // Connect CDP client
    cdpClient = new CDPClient();
    
    // Connect to the browser-level WebSocket first
    await cdpClient.connect(wsUrl);

    // Get or create a tab
    const tabWsUrl = await ChromeLauncher.getFirstTabWsUrl();
    
    if (tabWsUrl) {
        // Disconnect from browser-level and reconnect to tab-level
        cdpClient.dispose();
        cdpClient = new CDPClient();
        await cdpClient.connect(tabWsUrl);
    }

    // Enable required CDP domains
    await cdpClient.enableDomains();

    startRateLimitTimer();

    return cdpClient;
}

// ============================================================
// Skill Metadata (OpenClaw Format)
// ============================================================

export const metadata = {
    name: "browser_harness",
    search_keywords: [
        "browser_harness", "browser", "web", "CDP", "trình duyệt",
        "duyệt web", "mở web", "trang web", "click", "navigate",
        "tìm kiếm", "google", "đọc web", "accessibility",
    ],
    description: 
        "[ASK_FIRST] Control Chrome browser directly via CDP protocol. " +
        "Uses Accessibility Tree to identify elements (NO CSS selectors). " +
        "Supports: navigate, ax_snapshot, " +
        "click_element, type_text, scroll, press_key, " +
        "read_page, screenshot. " +
        "ALWAYS call ax_snapshot before click/type.",
    isCoreSkill: false,
    parameters: {
        type: "object",
        properties: {
            action: {
                type: "string",
                enum: [
                    "navigate", "ax_snapshot", "interactive_snapshot",
                    "click_element", "type_text", "scroll",
                    "read_page", "press_key", "screenshot", "close",
                ],
                description:
                    "Action to perform: navigate, ax_snapshot (full tree), " +
                    "interactive_snapshot (interactive elements only), " +
                    "click_element (by id from AxTree), " +
                    "type_text (by id), " +
                    "scroll, read_page (raw text), " +
                    "press_key (Enter/Tab/Escape), " +
                    "screenshot, close",
            },
            url: {
                type: "string",
                description: "Website URL (for action=navigate)",
            },
            element_id: {
                type: "number",
                description: "Element ID from AxTree snapshot (for action=click_element/type_text)",
            },
            text: {
                type: "string",
                description: "Content to type (for action=type_text) or key to press (for action=press_key)",
            },
            direction: {
                type: "string",
                enum: ["up", "down"],
                description: "Scroll direction (for action=scroll)",
            },
        },
        required: ["action"],
    },
};

// ============================================================
// Skill Execution
// ============================================================

export const execute = async (args: {
    action: string;
    url?: string;
    element_id?: number;
    text?: string;
    direction?: string;
}): Promise<string> => {
    try {
        // Rate limiting check
        if (actionCountThisMinute >= MAX_ACTIONS_PER_MINUTE) {
            return "[Rate Limit] Đã vượt quá 100 hành động/phút. Vui lòng đợi.";
        }
        actionCountThisMinute++;

        // Audit log
        RPAGuardrails.logAction(
            "browser_harness",
            args.action,
            args.url || `element_id=${args.element_id ?? ""}`,
            args.text || "",
            false,
            "allowed"
        );

        // Handle close action (no CDP connection needed)
        if (args.action === "close") {
            return await handleClose();
        }

        // Get or create CDP connection
        const cdp = await getOrCreateCDP();

        switch (args.action) {
            // ==========================================
            // NAVIGATE — Mở URL qua CDP
            // ==========================================
            case "navigate": {
                if (!args.url) {
                    return "Lỗi: Thiếu tham số 'url'. Hãy cung cấp URL cần mở.";
                }

                // Sensitive domain check
                if (RPAGuardrails.isSensitiveDomain(args.url)) {
                    logger.warn(`[BrowserHarness/Guard] ⚠️ Domain nhạy cảm: ${args.url}`);
                    RPAGuardrails.logAction(
                        "browser_harness", "navigate_sensitive",
                        args.url, "", false, "warned"
                    );
                }

                return await navigateAndGetInfo(cdp, args.url);
            }

            // ==========================================
            // AX_SNAPSHOT — Chụp Accessibility Tree
            // ==========================================
            case "ax_snapshot": {
                logger.info("[BrowserHarness] Generating AxTree snapshot...");
                return await getAxSnapshot(cdp, 2000);
            }

            // ==========================================
            // INTERACTIVE_SNAPSHOT — Chỉ phần tử tương tác
            // ==========================================
            case "interactive_snapshot": {
                logger.info("[BrowserHarness] Generating interactive-only snapshot...");
                return await getInteractiveSnapshot(cdp);
            }

            // ==========================================
            // CLICK_ELEMENT — Bấm theo AxTree ID
            // ==========================================
            case "click_element": {
                if (args.element_id === undefined) {
                    return "Lỗi: Thiếu tham số 'element_id'. Hãy gọi ax_snapshot trước để lấy ID.";
                }

                logger.info(`[BrowserHarness] Clicking element id=${args.element_id}`);
                return await clickByAxId(cdp, args.element_id);
            }

            // ==========================================
            // TYPE_TEXT — Nhập liệu theo AxTree ID
            // ==========================================
            case "type_text": {
                if (!args.text) {
                    return "Lỗi: Thiếu tham số 'text'. Hãy cung cấp nội dung cần nhập.";
                }

                // PII check
                const piiCheck = RPAGuardrails.scanForPII(args.text);
                if (piiCheck.hasPII) {
                    logger.warn(`[BrowserHarness/Guard] ⚠️ PII in typed content: ${piiCheck.detectedTypes.join(", ")}`);
                }

                if (args.element_id !== undefined) {
                    return await typeIntoElement(cdp, args.element_id, args.text);
                }

                // No element_id: type at current focus
                await cdp.insertText(args.text);
                return `[Đã nhập] "${args.text.substring(0, 30)}..." tại vị trí hiện tại`;
            }

            // ==========================================
            // SCROLL — Cuộn trang
            // ==========================================
            case "scroll": {
                const dir = (args.direction as "up" | "down") || "down";
                return await scrollPage(cdp, dir);
            }

            // ==========================================
            // READ_PAGE — Đọc nội dung text
            // ==========================================
            case "read_page": {
                logger.info("[BrowserHarness] Reading page content...");
                return await extractPageText(cdp);
            }

            // ==========================================
            // PRESS_KEY — Nhấn phím đặc biệt
            // ==========================================
            case "press_key": {
                if (!args.text) {
                    return "Lỗi: Thiếu tham số 'text'. Hãy cung cấp tên phím (Enter, Tab, Escape, ...)";
                }
                return await pressKey(cdp, args.text);
            }

            // ==========================================
            // SCREENSHOT — Chụp màn hình
            // ==========================================
            case "screenshot": {
                const screenshotDir = path.join(process.cwd(), "data", "screenshots");
                const filename = `cdp_screenshot_${Date.now()}.png`;
                const filepath = path.join(screenshotDir, filename);

                return await takeScreenshot(cdp, filepath);
            }

            default:
                return `Hành động không hợp lệ: "${args.action}". Các hành động: navigate, ax_snapshot, interactive_snapshot, click_element, type_text, scroll, read_page, press_key, screenshot, close`;
        }
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        RPAGuardrails.logAction(
            "browser_harness", args.action,
            args.url || "", errMsg, false, "blocked"
        );
        logger.error(`[BrowserHarness] Error: ${errMsg}`);
        return `[Lỗi Browser Harness] ${errMsg}`;
    }
};

// ============================================================
// Cleanup
// ============================================================

async function handleClose(): Promise<string> {
    if (cdpClient) {
        cdpClient.dispose();
        cdpClient = null;
    }
    await ChromeLauncher.shutdown();

    if (rateLimitResetTimer) {
        clearInterval(rateLimitResetTimer);
        rateLimitResetTimer = null;
    }

    return "Đã đóng trình duyệt CDP và giải phóng tài nguyên.";
}

/**
 * Shutdown hook — called by CoreKernel during system shutdown.
 */
export async function shutdownBrowserHarness(): Promise<void> {
    await handleClose();
    logger.info("[BrowserHarness] Shutdown complete");
}
