import { CDPClient } from "./CDPClient";
import { parseAxTree, formatAxSnapshot, type AxElement } from "./AxTreeParser";
// import { } from "./";

/**
 * CDPHelpers — Seed Helper Functions for CDP Browser Automation
 * ==============================================================
 * Equivalent to browser-harness's helpers.py — pre-built utility functions
 * that E4B can invoke through the BrowserHarness skill.
 * 
 * Level 1 Design (Read-only helpers):
 *   E4B can call these functions but cannot modify them.
 *   Architecture is prepared for Level 2 (dynamic helpers via vm module).
 * 
 * All helpers follow the pattern:
 *   Input: CDPClient instance + action-specific params
 *   Output: string result for LLM context
 */

// ============================================================
// Navigation Helpers
// ============================================================

/**
 * Navigate to URL and return page info.
 */
export async function navigateAndGetInfo(
    cdp: CDPClient,
    url: string
): Promise<string> {
    let targetUrl = url;
    if (!targetUrl.startsWith("http")) {
        targetUrl = `https://${targetUrl}`;
    }

    await cdp.navigateTo(targetUrl);

    const title = await cdp.getPageTitle();
    const currentUrl = await cdp.getCurrentUrl();

    return `[Đã mở trang] "${title}"\n[URL]: ${currentUrl}\n(Gọi ax_snapshot để xem cấu trúc trang)`;
}

/**
 * Wait for navigation to complete after an action.
 */
export async function waitForNavigation(
    cdp: CDPClient,
    timeoutMs = 10_000
): Promise<void> {
    return new Promise<void>((resolve, _reject) => {
        const timer = setTimeout(() => {
            cdp.off("Page.loadEventFired", handler);
            resolve(); // Don't reject — soft timeout
        }, timeoutMs);

        const handler = () => {
            clearTimeout(timer);
            cdp.off("Page.loadEventFired", handler);
            resolve();
        };
        cdp.on("Page.loadEventFired", handler);
    });
}

// ============================================================
// Accessibility Tree Helpers
// ============================================================

/** Cache for the most recent AxTree parse results */
let cachedAxElements: AxElement[] = [];

/**
 * Get a compact Accessibility Tree snapshot of the current page.
 * This is the PRIMARY way E4B "sees" the page (replaces raw DOM).
 */
export async function getAxSnapshot(
    cdp: CDPClient,
    tokenBudget = 2000
): Promise<string> {
    const rawTree = await cdp.getAccessibilityTree();
    const nodes = rawTree.nodes ?? [];

    // Parse and cache
    cachedAxElements = parseAxTree(nodes, {
        includeSemanticNodes: true,
        maxElements: 200,
    });

    return formatAxSnapshot(cachedAxElements, tokenBudget);
}

/**
 * Get only interactive elements (buttons, links, inputs).
 */
export async function getInteractiveSnapshot(
    cdp: CDPClient
): Promise<string> {
    const rawTree = await cdp.getAccessibilityTree();
    const nodes = rawTree.nodes ?? [];

    cachedAxElements = parseAxTree(nodes, {
        includeSemanticNodes: false,
        maxElements: 100,
    });

    return formatAxSnapshot(cachedAxElements, 1500);
}

// ============================================================
// Interaction Helpers
// ============================================================

/**
 * Click an element by its AxTree ID.
 * Uses CDP to find the element's bounding box and dispatch a click event.
 */
export async function clickByAxId(
    cdp: CDPClient,
    axId: number
): Promise<string> {
    const element = cachedAxElements.find(el => el.id === axId);
    if (!element) {
        return `[Lỗi] Không tìm thấy phần tử với id=${axId}. Hãy gọi ax_snapshot lại.`;
    }

    // Strategy: Use DOM.focus + Runtime.evaluate to find and click the element
    // We need to use JavaScript evaluation since AxTree IDs don't directly map to DOM nodes
    const clickScript = `
        (function() {
            // Find elements matching the role and name
            const role = ${JSON.stringify(element.role)};
            const name = ${JSON.stringify(element.name)};
            
            // Try aria role + accessible name matching
            const candidates = document.querySelectorAll('[role="' + role + '"], ' + role);
            for (const el of candidates) {
                const elName = el.getAttribute('aria-label') || el.textContent?.trim() || '';
                if (elName.includes(name) || name.includes(elName.substring(0, 20))) {
                    el.scrollIntoView({ block: 'center' });
                    el.click();
                    return 'clicked: ' + role + ' "' + elName.substring(0, 50) + '"';
                }
            }
            
            // Fallback: search by text content across all interactive elements
            const allInteractive = document.querySelectorAll('a, button, input, select, [role], [tabindex]');
            for (const el of allInteractive) {
                const elText = el.getAttribute('aria-label') || el.textContent?.trim() || '';
                if (elText.includes(name) || name.includes(elText.substring(0, 20))) {
                    el.scrollIntoView({ block: 'center' });
                    el.click();
                    return 'clicked (fallback): "' + elText.substring(0, 50) + '"';
                }
            }
            
            return 'not_found';
        })()
    `;

    try {
        const result = await cdp.evaluate(clickScript);

        if (result === "not_found") {
            return `[Lỗi] Không thể click phần tử id=${axId} (${element.role}: "${element.name}"). Phần tử có thể bị ẩn hoặc bị overlay che.`;
        }

        // Wait briefly for any navigation/UI update
        await new Promise(r => setTimeout(r, 500));

        return `[Đã click] ${element.role}: "${element.name}" (id=${axId})`;
    } catch (err: any) {
        return `[Lỗi click] ${err.message}`;
    }
}

/**
 * Type text into an element identified by AxTree ID.
 */
export async function typeIntoElement(
    cdp: CDPClient,
    axId: number,
    text: string
): Promise<string> {
    const element = cachedAxElements.find(el => el.id === axId);
    if (!element) {
        return `[Lỗi] Không tìm thấy phần tử với id=${axId}. Hãy gọi ax_snapshot lại.`;
    }

    // Focus the element first
    const focusScript = `
        (function() {
            const name = ${JSON.stringify(element.name)};
            const inputs = document.querySelectorAll('input, textarea, [contenteditable="true"], [role="textbox"], [role="searchbox"], [role="combobox"]');
            for (const el of inputs) {
                const elName = el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.getAttribute('name') || '';
                if (elName.includes(name) || name.includes(elName.substring(0, 20))) {
                    el.scrollIntoView({ block: 'center' });
                    el.focus();
                    if (el.select) el.select();
                    return 'focused';
                }
            }
            return 'not_found';
        })()
    `;

    const focusResult = await cdp.evaluate(focusScript);
    if (focusResult === "not_found") {
        return `[Lỗi] Không thể focus vào phần tử id=${axId} (${element.role}: "${element.name}")`;
    }

    // Clear existing content and type new text
    await cdp.insertText(text);

    return `[Đã nhập] "${text.substring(0, 30)}${text.length > 30 ? "..." : ""}" vào ${element.role}: "${element.name}" (id=${axId})`;
}

/**
 * Scroll the page up or down.
 */
export async function scrollPage(
    cdp: CDPClient,
    direction: "up" | "down",
    amount = 500
): Promise<string> {
    const deltaY = direction === "up" ? -amount : amount;
    await cdp.scrollPage(deltaY);

    // Wait for any lazy-loaded content
    await new Promise(r => setTimeout(r, 300));

    return `[Đã cuộn trang ${direction === "up" ? "lên" : "xuống"}]`;
}

/**
 * Extract visible text content from the current page.
 */
export async function extractPageText(
    cdp: CDPClient,
    maxLength = 3000
): Promise<string> {
    const text = await cdp.evaluate(`
        (function() {
            const remove = document.querySelectorAll('script, style, nav, footer, header, aside, iframe, noscript');
            remove.forEach(el => el.remove());
            let text = document.body?.innerText || '';
            text = text.replaceAll(/\\n{3,}/g, '\\n\\n').trim();
            return text;
        })()
    `);

    const title = await cdp.getPageTitle();
    const url = await cdp.getCurrentUrl();

    let result = `[Trang: ${title}]\n[URL: ${url}]\n\n`;
    result += (text || "").substring(0, maxLength);
    if ((text || "").length > maxLength) {
        result += "\n\n... (cắt ngắn)";
    }
    return result;
}

/**
 * Press a keyboard key (Enter, Tab, Escape, etc.)
 */
export async function pressKey(
    cdp: CDPClient,
    key: string
): Promise<string> {
    // Map common key names to CDP key identifiers
    const keyMap: Record<string, { key: string; code: string; keyCode: number }> = {
        "enter":  { key: "Enter",  code: "Enter",  keyCode: 13 },
        "tab":    { key: "Tab",    code: "Tab",    keyCode: 9 },
        "escape": { key: "Escape", code: "Escape", keyCode: 27 },
        "space":  { key: " ",      code: "Space",  keyCode: 32 },
        "backspace": { key: "Backspace", code: "Backspace", keyCode: 8 },
        "delete": { key: "Delete", code: "Delete", keyCode: 46 },
        "arrowup":    { key: "ArrowUp",    code: "ArrowUp",    keyCode: 38 },
        "arrowdown":  { key: "ArrowDown",  code: "ArrowDown",  keyCode: 40 },
        "arrowleft":  { key: "ArrowLeft",  code: "ArrowLeft",  keyCode: 37 },
        "arrowright": { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
    };

    const mapped = keyMap[key.toLowerCase()];
    if (!mapped) {
        return `[Lỗi] Phím "${key}" không được hỗ trợ. Các phím: Enter, Tab, Escape, Space, Backspace, Delete, ArrowUp/Down/Left/Right`;
    }

    await cdp.send("Input.dispatchKeyEvent", {
        type: "keyDown",
        key: mapped.key,
        code: mapped.code,
        windowsVirtualKeyCode: mapped.keyCode,
        nativeVirtualKeyCode: mapped.keyCode,
    });
    await cdp.send("Input.dispatchKeyEvent", {
        type: "keyUp",
        key: mapped.key,
        code: mapped.code,
        windowsVirtualKeyCode: mapped.keyCode,
        nativeVirtualKeyCode: mapped.keyCode,
    });

    return `[Đã nhấn phím] ${key}`;
}

/**
 * Take a screenshot and save to disk.
 */
export async function takeScreenshot(
    cdp: CDPClient,
    savePath: string
): Promise<string> {
    const { promises: fsp } = await import("fs");
    const pathModule = await import("path");

    const dir = pathModule.dirname(savePath);
    await fsp.mkdir(dir, { recursive: true });

    const base64Data = await cdp.screenshot("png");
    const buffer = Buffer.from(base64Data, "base64");
    await fsp.writeFile(savePath, buffer);

    return `[Screenshot] Đã lưu tại: ${savePath}`;
}
