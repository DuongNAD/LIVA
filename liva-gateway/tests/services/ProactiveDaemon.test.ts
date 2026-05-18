import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ProactiveDaemon, type ProactiveDaemonDeps } from "../../src/services/ProactiveDaemon";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock safeFetch for Tavily + Cloud LLM
vi.stubGlobal("fetch", vi.fn());
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

function createMockDeps(overrides?: Partial<ProactiveDaemonDeps>): ProactiveDaemonDeps {
    return {
        getTopics: vi.fn().mockResolvedValue({ interests: ["AI Agents"], focus: ["Rust", "TypeScript"] }),
        isAgentBusy: vi.fn().mockReturnValue(false),
        saveBriefing: vi.fn(),
        getUnreadCount: vi.fn().mockReturnValue(3),
        cleanExpired: vi.fn().mockReturnValue(0),
        pushNotification: vi.fn(),
        pushEgress: vi.fn(),
        isUserOnline: vi.fn().mockReturnValue(true),
        ...overrides,
    };
}

describe("ProactiveDaemon", () => {
    let daemon: ProactiveDaemon;
    let deps: ProactiveDaemonDeps;
    const savedEnv: Record<string, string | undefined> = {};

    beforeEach(async () => {
        vi.useFakeTimers();
        deps = createMockDeps();
        // Save and clean env
        for (const key of ["TAVILY_API_KEY", "AI_BASE_URL", "AI_API_KEY", "AI_MODEL"]) {
            savedEnv[key] = process.env[key];
            delete process.env[key];
        }
        // Clear mock counts between tests
        const { safeFetch } = await import("../../src/utils/HttpClient");
        vi.mocked(safeFetch).mockReset();
    });

    afterEach(() => {
        daemon?.dispose();
        vi.useRealTimers();
        // Restore env
        for (const [key, val] of Object.entries(savedEnv)) {
            if (val === undefined) delete process.env[key];
            else process.env[key] = val;
        }
    });

    it("should start and stop without errors", () => {
        daemon = new ProactiveDaemon(deps);
        daemon.start();
        daemon.dispose();
    });

    it("should not start twice", () => {
        daemon = new ProactiveDaemon(deps);
        daemon.start();
        daemon.start(); // idempotent
        daemon.dispose();
    });

    it("should skip if no topics found (memory_strength all < 0.2)", async () => {
        deps = createMockDeps({
            getTopics: vi.fn().mockResolvedValue({ interests: [], focus: [] }),
        });
        daemon = new ProactiveDaemon(deps);
        await daemon.forceDigest();
        expect(deps.saveBriefing).not.toHaveBeenCalled();
    });

    it("should skip if TAVILY_API_KEY is not set", async () => {
        delete process.env.TAVILY_API_KEY;
        daemon = new ProactiveDaemon(deps);
        await daemon.forceDigest();
        expect(deps.saveBriefing).not.toHaveBeenCalled();
    });

    it("should fetch news and store raw briefing when no cloud API configured", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        delete process.env.AI_BASE_URL;
        delete process.env.AI_API_KEY;

        const { safeFetch } = await import("../../src/utils/HttpClient");
        const mockFetch = vi.mocked(safeFetch);
        mockFetch.mockResolvedValue({
            json: vi.fn().mockResolvedValue({
                results: [
                    { title: "AI News 1", url: "https://example.com/1", content: "AI content 1" },
                    { title: "AI News 2", url: "https://example.com/2", content: "AI content 2" },
                ],
            }),
        } as never);

        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        expect(mockFetch).toHaveBeenCalledTimes(2); // One for interests, one for focus
        expect(deps.saveBriefing).toHaveBeenCalledTimes(2); // One per category group
        const savedBriefing = vi.mocked(deps.saveBriefing).mock.calls[0][0];
        expect(savedBriefing.source).toBe("raw_articles");
        expect(savedBriefing.content).toContain("AI News 1");
        expect(savedBriefing.content).toContain("AI News 2");
    });

    it("should synthesize via Cloud API when available", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        process.env.AI_BASE_URL = "https://api.test.com/v1/";
        process.env.AI_API_KEY = "cloud-key";
        process.env.AI_MODEL = "test-model";

        const { safeFetch } = await import("../../src/utils/HttpClient");
        const mockFetch = vi.mocked(safeFetch);
        
        // Mock: only provide interests topics (focus is empty)
        deps = createMockDeps({
            getTopics: vi.fn().mockResolvedValue({ interests: ["AI Agents"], focus: [] }),
        });

        // Mock Tavily (1 call — interests only, focus empty = no Tavily call)
        mockFetch.mockResolvedValueOnce({
            json: vi.fn().mockResolvedValue({
                results: [{ title: "Interest News", url: "https://example.com/1", content: "..." }],
            }),
        } as never);

        // Mock Cloud LLM (1 call — for interests group)
        mockFetch.mockResolvedValueOnce({
            json: vi.fn().mockResolvedValue({
                choices: [{ message: { content: "Cloud synthesized summary" } }],
            }),
        } as never);

        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        expect(mockFetch).toHaveBeenCalledTimes(2); // 1 Tavily + 1 Cloud LLM
        expect(deps.saveBriefing).toHaveBeenCalledTimes(1);
        const savedBriefing = vi.mocked(deps.saveBriefing).mock.calls[0][0];
        expect(savedBriefing.source).toBe("cloud_synthesis");
        expect(savedBriefing.content).toContain("Cloud synthesized summary");
    });

    it("should defer synthesis when VRAM is busy", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        delete process.env.AI_BASE_URL;

        const { safeFetch } = await import("../../src/utils/HttpClient");
        const mockFetch = vi.mocked(safeFetch);
        mockFetch.mockResolvedValueOnce({
            json: vi.fn().mockResolvedValue({
                results: [{ title: "News", url: "https://x.com", content: "C" }],
            }),
        } as never);

        deps = createMockDeps({
            isAgentBusy: vi.fn().mockReturnValue(true),
        });

        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        // Should NOT have saved yet — deferred
        expect(deps.saveBriefing).not.toHaveBeenCalled();
    });

    it("should fallback to raw briefing after max VRAM retries", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        delete process.env.AI_BASE_URL;

        const { safeFetch } = await import("../../src/utils/HttpClient");
        const mockFetch = vi.mocked(safeFetch);
        // Only interests topics, no focus — simpler single-group test
        deps = createMockDeps({
            getTopics: vi.fn().mockResolvedValue({ interests: ["AI"], focus: [] }),
            isAgentBusy: vi.fn().mockReturnValue(true), // Always busy
        });

        mockFetch.mockResolvedValue({
            json: vi.fn().mockResolvedValue({
                results: [{ title: "News", url: "https://x.com", content: "C" }],
            }),
        } as never);

        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        // First digest: fetches news, then attempts synthesize (deferred because busy)
        await daemon.forceDigest();
        // Subsequent digests: articles present, synthesize called again (still busy → retries)
        await daemon.forceDigest();
        await daemon.forceDigest();
        await daemon.forceDigest(); // 4th attempt > MAX_VRAM_RETRIES(3) → raw fallback

        // By now, retry count > 3, should have stored raw
        expect(deps.saveBriefing).toHaveBeenCalled();
    });

    it("should push notification when user is online", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        delete process.env.AI_BASE_URL;

        const { safeFetch } = await import("../../src/utils/HttpClient");
        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: vi.fn().mockResolvedValue({
                results: [{ title: "News", url: "https://x.com", content: "C" }],
            }),
        } as never);

        deps = createMockDeps({ isUserOnline: vi.fn().mockReturnValue(true) });
        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        expect(deps.pushNotification).toHaveBeenCalled();
        expect(deps.pushEgress).not.toHaveBeenCalled();
    });

    it("should send egress when user is offline", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        delete process.env.AI_BASE_URL;

        const { safeFetch } = await import("../../src/utils/HttpClient");
        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: vi.fn().mockResolvedValue({
                results: [{ title: "News", url: "https://x.com", content: "C" }],
            }),
        } as never);

        deps = createMockDeps({ isUserOnline: vi.fn().mockReturnValue(false) });
        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        expect(deps.pushEgress).toHaveBeenCalled();
        expect(deps.pushNotification).not.toHaveBeenCalled();
    });

    it("should clean expired briefings on tick", async () => {
        const cleanFn = vi.fn().mockReturnValue(5);
        deps = createMockDeps({ cleanExpired: cleanFn });
        daemon = new ProactiveDaemon(deps);
        await daemon.forceDigest();
        expect(cleanFn).toHaveBeenCalled();
    });

    it("should handle Tavily API failure gracefully", async () => {
        process.env.TAVILY_API_KEY = "test-key";

        const { safeFetch } = await import("../../src/utils/HttpClient");
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Network error"));

        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        // Should not crash, should not save
        expect(deps.saveBriefing).not.toHaveBeenCalled();
    });

    it("should handle Cloud LLM failure and fallback to raw", async () => {
        process.env.TAVILY_API_KEY = "test-key";
        process.env.AI_BASE_URL = "https://api.test.com/v1/";
        process.env.AI_API_KEY = "cloud-key";

        const { safeFetch } = await import("../../src/utils/HttpClient");
        const mockFetch = vi.mocked(safeFetch);

        // Single group to simplify: only interests
        deps = createMockDeps({
            getTopics: vi.fn().mockResolvedValue({ interests: ["AI"], focus: [] }),
        });

        // Mock Tavily success
        mockFetch.mockResolvedValueOnce({
            json: vi.fn().mockResolvedValue({
                results: [{ title: "News", url: "https://x.com", content: "Some content" }],
            }),
        } as never);

        // Mock Cloud LLM failure
        mockFetch.mockRejectedValueOnce(new Error("Cloud API timeout"));

        daemon = new ProactiveDaemon(deps, { scheduleHour: new Date().getHours() });
        await daemon.forceDigest();

        // Should fallback to raw storage after Cloud failure
        expect(deps.saveBriefing).toHaveBeenCalled();
    });
});
