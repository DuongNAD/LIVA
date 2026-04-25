import { describe, it, expect, vi, beforeEach } from "vitest";
import { execute, metadata } from "../../src/skills/BrowserHarness";
import { RPAGuardrails } from "../../src/security/RPAGuardrails";

// Mock CDPClient and Launcher
vi.mock("../../src/utils/ChromeLauncher", () => ({
    ChromeLauncher: {
        launchOrConnect: vi.fn().mockResolvedValue("ws://mock"),
        getFirstTabWsUrl: vi.fn().mockResolvedValue("ws://mock-tab"),
        cleanupZombies: vi.fn().mockResolvedValue(0),
        shutdown: vi.fn()
    }
}));

vi.mock("../../src/utils/CDPClient", () => {
    return {
        CDPClient: class {
            isConnected = true;
            connect = vi.fn().mockResolvedValue(undefined);
            enableDomains = vi.fn().mockResolvedValue(undefined);
            dispose = vi.fn();
            navigateTo = vi.fn().mockResolvedValue(undefined);
            getPageTitle = vi.fn().mockResolvedValue("Mock Title");
            getCurrentUrl = vi.fn().mockResolvedValue("http://mock.com");
            getAccessibilityTree = vi.fn().mockResolvedValue({ nodes: [] });
            evaluate = vi.fn().mockResolvedValue("not_found");
        }
    };
});

describe("BrowserHarness Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it("should reject action if missing required parameters", async () => {
        const result = await execute({ action: "navigate" }); // Missing URL
        expect(result).toContain("Thiếu tham số 'url'");
    });

    it("should log actions to RPAGuardrails", async () => {
        const logSpy = vi.spyOn(RPAGuardrails, "logAction");
        
        await execute({ action: "ax_snapshot" });
        
        expect(logSpy).toHaveBeenCalledWith(
            "browser_harness",
            "ax_snapshot",
            "element_id=",
            "",
            false,
            "allowed"
        );
    });

    it("should detect sensitive domains during navigation", async () => {
        const logSpy = vi.spyOn(RPAGuardrails, "logAction");
        
        await execute({ action: "navigate", url: "https://vietcombank.com.vn" });
        
        // Should log a warning for sensitive domain
        expect(logSpy).toHaveBeenCalledWith(
            "browser_harness",
            "navigate_sensitive",
            "https://vietcombank.com.vn",
            "",
            false,
            "warned"
        );
    });

    it("should return empty snapshot for empty ax tree", async () => {
        const result = await execute({ action: "ax_snapshot" });
        expect(result).toContain("[AxTree] Trang trống");
    });
});
