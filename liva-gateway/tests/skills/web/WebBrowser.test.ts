import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { execute, metadata } from "../../../src/skills/web/WebBrowser";
import { getOrCreateBrowser } from "../../../src/utils/PlaywrightBrowser";

vi.mock("../../../src/utils/PlaywrightBrowser", () => ({
    getOrCreateBrowser: vi.fn()
}));

vi.mock("../../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn()
    }
}));

describe("Skill - WebBrowser", () => {
    let mockPage: any;
    let mockContext: any;

    beforeEach(() => {
        vi.clearAllMocks();

        mockPage = {
            goto: vi.fn().mockResolvedValue(undefined),
            title: vi.fn().mockResolvedValue("Test Title"),
            url: vi.fn().mockReturnValue("http://test.com"),
            evaluate: vi.fn().mockResolvedValue("Test body content"),
            locator: vi.fn().mockReturnValue({
                click: vi.fn().mockResolvedValue(undefined),
                fill: vi.fn().mockResolvedValue(undefined),
                innerText: vi.fn().mockResolvedValue("Test inner text")
            }),
            waitForLoadState: vi.fn().mockResolvedValue(undefined),
            on: vi.fn(),
        };

        mockContext = {
            close: vi.fn().mockResolvedValue(undefined),
            pages: vi.fn().mockReturnValue([mockPage]),
            newPage: vi.fn().mockResolvedValue(mockPage),
            on: vi.fn(),
        };

        vi.mocked(getOrCreateBrowser).mockResolvedValue({
            browser: {} as any,
            context: mockContext
        });
    });

    afterEach(async () => {
        // Reset the module state so each test gets the fresh mockPage
        await execute({ action: "close" });
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("web_browser");
        expect(metadata.parameters.required).toContain("action");
    });

    it("should handle close action when browser is not open", async () => {
        const result = await execute({ action: "close" });
        expect(result).toBe("Trình duyệt đang không mở.");
    });

    it("should initialize browser and navigate to url", async () => {
        const result = await execute({ action: "navigate", url: "test.com" });
        
        expect(getOrCreateBrowser).toHaveBeenCalledWith("web_browser");
        expect(mockPage.goto).toHaveBeenCalledWith("https://test.com", expect.any(Object));
        expect(result).toContain("Đã tải xong trang: \"Test Title\"");
        expect(result).toContain("Test body content");
    });

    it("should fail navigate if url is missing", async () => {
        const result = await execute({ action: "navigate" });
        expect(result).toBe("Lỗi: Thiếu tham số bắt buộc `url` cho hành động navigate.");
    });

    it("should handle click action", async () => {
        const result = await execute({ action: "click", selector: "#btn" });
        
        expect(mockPage.locator).toHaveBeenCalledWith("#btn");
        expect(mockPage.waitForLoadState).toHaveBeenCalledWith("domcontentloaded");
        expect(result).toContain("Đã Click thành công vào: \"#btn\".");
    });

    it("should fail click if selector is missing", async () => {
        const result = await execute({ action: "click" });
        expect(result).toBe("Lỗi: Thiếu tham số `selector` cho hành động click.");
    });

    it("should handle type action", async () => {
        const result = await execute({ action: "type", selector: "#input", text: "hello" });
        
        expect(mockPage.locator).toHaveBeenCalledWith("#input");
        expect(result).toContain("Đã hoàn tất việc gõ chữ \"hello\" vào vị trí \"#input\".");
    });

    it("should fail type if selector or text is missing", async () => {
        const result1 = await execute({ action: "type", text: "hello" });
        expect(result1).toBe("Lỗi: Thiếu tham số `selector` hoặc `text` cho hành động type.");
        
        const result2 = await execute({ action: "type", selector: "#input" });
        expect(result2).toBe("Lỗi: Thiếu tham số `selector` hoặc `text` cho hành động type.");
    });

    it("should handle extract action with selector", async () => {
        const result = await execute({ action: "extract", selector: ".data" });
        
        expect(mockPage.locator).toHaveBeenCalledWith(".data");
        expect(result).toContain("[TIÊU ĐỀ TRANG]: Test Title");
        expect(result).toContain("Test inner text");
    });

    it("should handle extract action without selector", async () => {
        const result = await execute({ action: "extract" });
        
        expect(mockPage.evaluate).toHaveBeenCalled();
        expect(result).toContain("[TIÊU ĐỀ TRANG]: Test Title");
        expect(result).toContain("Test body content");
    });

    it("should handle extract action when locator innerText fails", async () => {
        mockPage.locator.mockReturnValueOnce({
            innerText: vi.fn().mockRejectedValue(new Error("Timeout"))
        });
        const result = await execute({ action: "extract", selector: ".bad-data" });
        expect(result).toContain("Không tìm thấy CSS selector này.");
    });

    it("should handle invalid action", async () => {
        const result = await execute({ action: "invalid" as any });
        expect(result).toBe("Lỗi: Hành động 'invalid' không được hệ thống hỗ trợ.");
    });

    it("should handle close action when browser is open", async () => {
        // Ensure browser is open first
        await execute({ action: "navigate", url: "test.com" });
        
        const result = await execute({ action: "close" });
        
        expect(mockContext.close).toHaveBeenCalled();
        expect(result).toBe("Đã đóng trình duyệt thành công và giải phóng bộ nhớ.");
    });

    it("should create new page if context has no pages", async () => {
        // Force browser to be closed first
        await execute({ action: "close" });
        
        mockContext.pages.mockReturnValueOnce([]);
        await execute({ action: "navigate", url: "test.com" });
        
        expect(mockContext.newPage).toHaveBeenCalled();
    });

    it("should catch and return errors gracefully", async () => {
        // Force browser to be closed first
        await execute({ action: "close" });
        
        vi.mocked(getOrCreateBrowser).mockRejectedValueOnce(new Error("Browser launch failed"));
        
        const result = await execute({ action: "navigate", url: "test.com" });
        expect(result).toBe("[LỖI TRÌNH DUYỆT CỤC BỘ]: Yêu cầu thao tác thất bại với mô tả: Browser launch failed");
    });
});
