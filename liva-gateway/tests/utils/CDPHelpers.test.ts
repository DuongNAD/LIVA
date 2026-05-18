import { describe, it, expect, vi, beforeEach } from "vitest";
import { CDPClient } from "../../src/utils/CDPClient";
import * as CDPHelpers from "../../src/utils/CDPHelpers";

// Mock the internal parser dependencies
vi.mock("../../src/utils/AxTreeParser", () => ({
    parseAxTree: vi.fn().mockReturnValue([{ id: 123, role: "button", name: "Submit" }]),
    formatAxSnapshot: vi.fn().mockReturnValue("Formatted Snapshot")
}));

import * as fs from "node:fs";

vi.mock("node:path", () => ({
    dirname: vi.fn().mockReturnValue("/some/dir")
}));

describe("CDPHelpers", () => {
    let mockCdp: any;

    beforeEach(() => {
        vi.clearAllMocks();
        mockCdp = {
            navigateTo: vi.fn().mockResolvedValue(undefined),
            getPageTitle: vi.fn().mockResolvedValue("Page Title"),
            getCurrentUrl: vi.fn().mockResolvedValue("https://example.com"),
            on: vi.fn(),
            off: vi.fn(),
            getAccessibilityTree: vi.fn().mockResolvedValue({ nodes: [] }),
            evaluate: vi.fn().mockResolvedValue("clicked: button \"Submit\""),
            insertText: vi.fn().mockResolvedValue(undefined),
            scrollPage: vi.fn().mockResolvedValue(undefined),
            send: vi.fn().mockResolvedValue(undefined),
            screenshot: vi.fn().mockResolvedValue("base64data")
        };
        vi.spyOn(fs.promises, "mkdir").mockResolvedValue(undefined);
        vi.spyOn(fs.promises, "writeFile").mockResolvedValue(undefined);
    });

    it("navigateAndGetInfo should navigate and return formatted info", async () => {
        const result = await CDPHelpers.navigateAndGetInfo(mockCdp, "example.com");
        expect(mockCdp.navigateTo).toHaveBeenCalledWith("https://example.com");
        expect(result).toContain("Page Title");
        expect(result).toContain("https://example.com");
    });

    it("waitForNavigation should resolve on Page.loadEventFired", async () => {
        let loadHandler: any;
        mockCdp.on.mockImplementation((event: string, handler: any) => {
            if (event === "Page.loadEventFired") loadHandler = handler;
        });

        const p = CDPHelpers.waitForNavigation(mockCdp, 1000);
        
        // Wait a tick for handlers to attach
        await new Promise(r => setTimeout(r, 0));
        if (loadHandler) loadHandler();
        
        await expect(p).resolves.toBeUndefined();
    });

    it("waitForNavigation should resolve on timeout", async () => {
        vi.useFakeTimers();
        const p = CDPHelpers.waitForNavigation(mockCdp, 1000);
        vi.advanceTimersByTime(1050);
        await expect(p).resolves.toBeUndefined();
        vi.useRealTimers();
    });

    it("getAxSnapshot should return formatted snapshot", async () => {
        const result = await CDPHelpers.getAxSnapshot(mockCdp);
        expect(mockCdp.getAccessibilityTree).toHaveBeenCalled();
        expect(result).toBe("Formatted Snapshot");
    });

    it("getInteractiveSnapshot should return formatted interactive snapshot", async () => {
        const result = await CDPHelpers.getInteractiveSnapshot(mockCdp);
        expect(mockCdp.getAccessibilityTree).toHaveBeenCalled();
        expect(result).toBe("Formatted Snapshot");
    });

    it("clickByAxId should evaluate script and return success", async () => {
        // Need to run getAxSnapshot first to populate cachedAxElements
        await CDPHelpers.getAxSnapshot(mockCdp);
        const result = await CDPHelpers.clickByAxId(mockCdp, 123);
        expect(mockCdp.evaluate).toHaveBeenCalled();
        expect(result).toContain("Đã click");
    });

    it("clickByAxId should return error if axId not found", async () => {
        const result = await CDPHelpers.clickByAxId(mockCdp, 999);
        expect(result).toContain("Không tìm thấy phần tử");
    });

    it("clickByAxId should handle not_found from evaluate", async () => {
        await CDPHelpers.getAxSnapshot(mockCdp);
        mockCdp.evaluate.mockResolvedValue("not_found");
        const result = await CDPHelpers.clickByAxId(mockCdp, 123);
        expect(result).toContain("Không thể click");
    });

    it("clickByAxId should handle evaluate error", async () => {
        await CDPHelpers.getAxSnapshot(mockCdp);
        mockCdp.evaluate.mockRejectedValue(new Error("JS Error"));
        const result = await CDPHelpers.clickByAxId(mockCdp, 123);
        expect(result).toContain("Lỗi click");
    });

    it("typeIntoElement should focus, insert text and return success", async () => {
        await CDPHelpers.getAxSnapshot(mockCdp);
        mockCdp.evaluate.mockResolvedValue("focused");
        const result = await CDPHelpers.typeIntoElement(mockCdp, 123, "hello");
        expect(mockCdp.evaluate).toHaveBeenCalled();
        expect(mockCdp.insertText).toHaveBeenCalledWith("hello");
        expect(result).toContain("Đã nhập");
    });

    it("typeIntoElement should return error if axId not found", async () => {
        const result = await CDPHelpers.typeIntoElement(mockCdp, 999, "hello");
        expect(result).toContain("Không tìm thấy phần tử");
    });

    it("typeIntoElement should handle focus not_found", async () => {
        await CDPHelpers.getAxSnapshot(mockCdp);
        mockCdp.evaluate.mockResolvedValue("not_found");
        const result = await CDPHelpers.typeIntoElement(mockCdp, 123, "hello");
        expect(result).toContain("Không thể focus");
    });

    it("scrollPage should dispatch mouseWheel up and down", async () => {
        await CDPHelpers.scrollPage(mockCdp, "down", 500);
        expect(mockCdp.scrollPage).toHaveBeenCalledWith(500);
        await CDPHelpers.scrollPage(mockCdp, "up", 500);
        expect(mockCdp.scrollPage).toHaveBeenCalledWith(-500);
    });

    it("extractPageText should evaluate JS to clean up and return text", async () => {
        mockCdp.evaluate.mockResolvedValue("Some page text");
        const result = await CDPHelpers.extractPageText(mockCdp, 10);
        expect(result).toContain("Page Title");
        expect(result).toContain("Some page");
        expect(result).toContain("cắt ngắn");
    });

    it("pressKey should dispatch key down and up", async () => {
        const result = await CDPHelpers.pressKey(mockCdp, "enter");
        expect(mockCdp.send).toHaveBeenCalledWith("Input.dispatchKeyEvent", expect.objectContaining({ type: "keyDown", key: "Enter" }));
        expect(mockCdp.send).toHaveBeenCalledWith("Input.dispatchKeyEvent", expect.objectContaining({ type: "keyUp", key: "Enter" }));
        expect(result).toContain("Đã nhấn phím");
    });

    it("pressKey should return error for unsupported key", async () => {
        const result = await CDPHelpers.pressKey(mockCdp, "unsupported");
        expect(result).toContain("không được hỗ trợ");
    });

    it("takeScreenshot should write file and return success", async () => {
        const result = await CDPHelpers.takeScreenshot(mockCdp, "/tmp/test.png");
        expect(mockCdp.screenshot).toHaveBeenCalledWith("png");
        expect(result).toContain("Đã lưu tại: /tmp/test.png");
    });
});
