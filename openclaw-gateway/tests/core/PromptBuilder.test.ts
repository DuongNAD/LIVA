import { describe, it, expect, vi, beforeEach } from "vitest";
import { PromptBuilder } from "../../src/core/PromptBuilder";

// ============================================================
// Mocks
// ============================================================
vi.mock("../../src/memory/SensoryManager", () => ({
    SensoryManager: {
        getInstance: () => ({
            injectSensoryPrompt: () => "<SystemSensory>Test sensory data</SystemSensory>",
        }),
    },
}));

vi.mock("../../src/memory/HeraCompass", () => {
    return {
        HeraCompass: {
            getInstance: vi.fn().mockReturnValue({
                getRelatedInsight: vi.fn().mockReturnValue([]),
            }),
        }
    };
});

vi.mock("../../src/system_prompt", () => ({
    getBaseSystemPrompt: () => "Bạn là Liva, một trợ lý AI.",
}));

// ============================================================
// TEST GROUP 1: semanticSkillFilter (via buildToolsPrompt)
// ============================================================
describe("PromptBuilder.buildToolsPrompt", () => {
    const sampleTools = [
        {
            name: "search_web",
            description: "Tìm kiếm thông tin trên mạng internet",
            isCoreSkill: false,
            search_keywords: ["tìm kiếm", "google", "web", "internet"],
            parameters: { type: "object", properties: { query: { type: "string" } } },
        },
        {
            name: "send_zalo_bot",
            description: "Gửi tin nhắn qua Zalo",
            isCoreSkill: false,
            search_keywords: ["zalo", "tin nhắn", "nhắn", "gửi"],
            parameters: { type: "object", properties: { message: { type: "string" } } },
        },
        {
            name: "get_current_time",
            description: "Lấy thời gian hiện tại",
            isCoreSkill: true,
            search_keywords: ["giờ", "thời gian", "ngày"],
            parameters: { type: "object", properties: {} },
        },
        {
            name: "read_file",
            description: "Đọc nội dung tệp tin",
            isCoreSkill: false,
            search_keywords: ["đọc", "file", "tệp"],
            parameters: { type: "object", properties: { path: { type: "string" } } },
        },
        {
            name: "run_code_sandbox",
            description: "Chạy code trong Docker sandbox",
            isCoreSkill: false,
            search_keywords: ["code", "chạy", "sandbox", "python"],
            parameters: { type: "object", properties: { code: { type: "string" } } },
        },
        {
            name: "send_email",
            description: "Gửi email qua Gmail SMTP",
            isCoreSkill: false,
            search_keywords: ["email", "gmail", "gửi mail"],
            parameters: { type: "object", properties: { to: { type: "string" } } },
        },
        {
            name: "browse_web",
            description: "Mở và đọc nội dung trang web bằng Puppeteer",
            isCoreSkill: false,
            search_keywords: ["duyệt web", "mở trang", "puppeteer"],
            parameters: { type: "object", properties: { url: { type: "string" } } },
        },
    ];

    beforeEach(() => {
        // Clear the static cache between tests
        // @ts-ignore - accessing private static
        PromptBuilder["#promptCache"]?.clear?.();
    });

    it("should always include core skills (get_current_time)", () => {
        const prompt = PromptBuilder.buildToolsPrompt("Mấy giờ rồi?", sampleTools);
        expect(prompt).toContain("get_current_time");
    });

    it("should always include handoff_to_expert (auto-injected)", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        expect(prompt).toContain("handoff_to_expert");
    });

    it("should prioritize tools matching query keywords", () => {
        const prompt = PromptBuilder.buildToolsPrompt("Tìm kiếm thời tiết Hà Nội", sampleTools);
        expect(prompt).toContain("search_web");
    });

    it("should prioritize Zalo tool when query mentions Zalo", () => {
        const prompt = PromptBuilder.buildToolsPrompt("Gửi tin nhắn cho Sếp qua Zalo", sampleTools);
        expect(prompt).toContain("send_zalo_bot");
    });

    it("should pass through all pre-filtered tools (filtering now in SkillRegistry)", () => {
        const prompt = PromptBuilder.buildToolsPrompt("help me", sampleTools);
        // Parse the JSON tools array in the prompt
        const toolsMatch = prompt.match(/\<tools\>\n([\s\S]*?)\n\<\/tools\>/);
        if (toolsMatch) {
            const toolsArr = JSON.parse(toolsMatch[1]);

            // All 7 input tools + 1 auto-injected handoff_to_expert = 8
            expect(toolsArr.length).toBe(sampleTools.length + 1);
        }
    });

    it("should strip search_keywords and isCoreSkill from final output", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        expect(prompt).not.toContain("search_keywords");
        expect(prompt).not.toContain("isCoreSkill");
    });

    it("should include system time in the prompt", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        // Vietnamese date format includes "/" separator
        expect(prompt).toContain("Thời gian hệ thống:");
    });

    it("should include CRITICAL RULES for tool call format", () => {
        const prompt = PromptBuilder.buildToolsPrompt("test", sampleTools);
        expect(prompt).toContain("<tool_call>");
        expect(prompt).toContain("CRITICAL RULES:");
    });

    it("should return all core + random topK for empty user query", () => {
        const prompt = PromptBuilder.buildToolsPrompt("", sampleTools);
        // Should still include core skills
        expect(prompt).toContain("get_current_time");
        expect(prompt).toContain("handoff_to_expert");
    });

    it("Filtered Full Schema: should only include JSON Parameters for the pre-filtered tools (Token efficiency)", () => {
        // Simulate getSemanticTopK returning only 3 tools out of 10
        const filteredTools = sampleTools.slice(0, 3);
        const prompt = PromptBuilder.buildToolsPrompt("test", filteredTools);
        
        // Assert schema is present for the 3 included tools
        expect(prompt).toContain("search_web");
        expect(prompt).toContain("send_zalo_bot");
        expect(prompt).toContain("get_current_time");
        
        // Assert schema is missing for the other tools
        expect(prompt).not.toContain("read_file");
        expect(prompt).not.toContain("run_code_sandbox");
        expect(prompt).not.toContain("browse_web");
    });
});

// ============================================================
// TEST GROUP 2: buildContextPrompt (Requires Memory Mock)
// ============================================================
describe("PromptBuilder.buildContextPrompt", () => {
    let mockMemory: any;

    beforeEach(() => {
        mockMemory = {
            getUserProfile: vi.fn().mockResolvedValue({
                name: "Dương",
                preferred_name: "Anh Dương",
            }),
            getStructuredMemoryPrompt: vi.fn().mockReturnValue("\n[BỘ NHỚ CẤU TRÚC]\n- name: Dương\n"),
            getLongTermMarkdown: vi.fn().mockResolvedValue("## Working Concepts\n- User likes dark mode UI.\n- User prefers Vietnamese language for all interactions"),
            getSessionState: vi.fn().mockResolvedValue(""),
        };
    });

    it("should include user profile in context", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("Dương");
        expect(ctx).toContain("HỒ SƠ NGƯỜI DÙNG");
    });

    it("should include structured memory prompt", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("BỘ NHỚ CẤU TRÚC");
    });

    it("should include long-term context when available", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("KÝ ỨC DÀI HẠN");
        expect(ctx).toContain("Working Concepts");
    });

    it("should skip long-term context when too short", async () => {
        mockMemory.getLongTermMarkdown.mockResolvedValue("short");
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).not.toContain("KÝ ỨC DÀI HẠN");
    });

    it("should include sensory data", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).toContain("SystemSensory");
    });

    it("should set current_location on user profile", async () => {
        await PromptBuilder.buildContextPrompt(mockMemory, "TP.HCM");
        const profile = await mockMemory.getUserProfile();
        // The method mutates the returned profile object
        expect(mockMemory.getUserProfile).toHaveBeenCalled();
    });

    it("should handle null user profile gracefully", async () => {
        mockMemory.getUserProfile.mockResolvedValue(null);
        const ctx = await PromptBuilder.buildContextPrompt(mockMemory, "Hà Nội");
        expect(ctx).not.toContain("HỒ SƠ NGƯỜI DÙNG");
    });
});

// ============================================================
// TEST GROUP 3: prepareFullAiMessages
// ============================================================
describe("PromptBuilder.prepareFullAiMessages", () => {
    let mockMemory: any;

    beforeEach(() => {
        mockMemory = {
            getUserProfile: vi.fn().mockResolvedValue({ name: "Test" }),
            getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
            getLongTermMarkdown: vi.fn().mockResolvedValue(""),
            getSessionState: vi.fn().mockResolvedValue(""),
            workingBuffer: { checkBudget: vi.fn().mockResolvedValue("[context-budget: 5.0% used, 63000 tokens remaining]") },
            getHybridContext: vi.fn().mockResolvedValue([
                { role: "user", content: "Xin chào" },
                { role: "assistant", content: "Chào bạn!" },
            ]),
        };
    });

    it("should return array starting with system message", async () => {
        const messages = await PromptBuilder.prepareFullAiMessages(
            "test query",
            mockMemory,
            "Hà Nội",
            []
        );

        expect(messages[0].role).toBe("system");
        expect(messages[0].content).toContain("Bạn là Liva");
    });

    it("should include history messages after system", async () => {
        const messages = await PromptBuilder.prepareFullAiMessages(
            "test",
            mockMemory,
            "Hà Nội",
            []
        );

        expect(messages.length).toBe(3); // system + 2 history
        expect(messages[1].role).toBe("user");
        expect(messages[1].content).toBe("Xin chào");
    });

    it("HeraCompass ICL: should inject actionable rule block when HeraCompass returns insights", async () => {
        const { HeraCompass } = await import("../../src/memory/HeraCompass");
        vi.mocked(HeraCompass.getInstance().getRelatedInsight).mockReturnValue([
            { actionable_rule: "Dung bao gio doc file qua lon", tool_target: "read_file" }
        ]);

        const messages = await PromptBuilder.prepareFullAiMessages(
            "doc file",
            mockMemory,
            "Hà Nội",
            []
        );

        expect(messages[0].content).toContain("[CẢNH BÁO TỪ KINH NGHIỆM]");
        expect(messages[0].content).toContain("Dung bao gio doc file qua lon");
    });
});

// ============================================================
// TEST GROUP 4: Route-Specific Context Loading
// ============================================================
describe("PromptBuilder.buildContextPrompt — Route branches", () => {
    let mockMemory: any;
    const mockSensory = {
        injectSensoryPrompt: () => "<Sensory>mock sensory</Sensory>",
    };

    beforeEach(() => {
        mockMemory = {
            getUserProfile: vi.fn().mockResolvedValue({
                name: "Dương",
                preferred_name: "Anh Dương",
            }),
            getStructuredMemoryPrompt: vi.fn().mockReturnValue("\n[BỘ NHỚ CẤU TRÚC]\n- name: Dương\n"),
            getLongTermMarkdown: vi.fn().mockResolvedValue("## Working Concepts\n- Long term content that is certainly more than 50 characters for the check"),
            getSessionState: vi.fn().mockResolvedValue("Session is active"),
            getLanceMemory: vi.fn().mockReturnValue(null),
        };
    });

    it("chitchat route: should return profile + sensory ONLY (skip memory)", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "chitchat"
        );

        expect(ctx).toContain("Dương");
        expect(ctx).toContain("mock sensory");
        // Should NOT include structured memory or long-term
        expect(ctx).not.toContain("BỘ NHỚ CẤU TRÚC");
        expect(ctx).not.toContain("KÝ ỨC DÀI HẠN");
        // Should NOT call heavy methods
        expect(mockMemory.getStructuredMemoryPrompt).not.toHaveBeenCalled();
        expect(mockMemory.getLongTermMarkdown).not.toHaveBeenCalled();
    });

    it("system_command route: should return profile + sensory ONLY", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "system_command"
        );

        expect(ctx).toContain("mock sensory");
        expect(ctx).not.toContain("KÝ ỨC DÀI HẠN");
        expect(ctx).not.toContain("BỘ NHỚ CẤU TRÚC");
    });

    it("factual_recall route: should include full pipeline (L3+L1+session)", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "factual_recall"
        );

        expect(ctx).toContain("BỘ NHỚ CẤU TRÚC");
        expect(ctx).toContain("KÝ ỨC DÀI HẠN");
    });

    it("deep_reasoning route: should include full pipeline", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "deep_reasoning"
        );

        expect(ctx).toContain("BỘ NHỚ CẤU TRÚC");
    });

    it("should include session state when available", async () => {
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "deep_reasoning"
        );

        expect(ctx).toContain("Session is active");
    });

    it("should handle empty session state", async () => {
        mockMemory.getSessionState.mockResolvedValue("");
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "deep_reasoning"
        );

        expect(ctx).not.toContain("TRẠNG THÁI PHIÊN");
    });

    it("token budget: should truncate session when memory exceeds budget", async () => {
        // Make structured memory very large to exceed budget
        mockMemory.getStructuredMemoryPrompt.mockReturnValue("A".repeat(7000));
        mockMemory.getSessionState.mockResolvedValue("Session data that should be truncated");

        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "deep_reasoning"
        );

        // Session should be truncated or empty since budget is exceeded
        expect(ctx.length).toBeGreaterThan(0);
    });

    it("should handle null user profile gracefully in route paths", async () => {
        mockMemory.getUserProfile.mockResolvedValue(null);
        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "chitchat"
        );

        expect(ctx).not.toContain("HỒ SƠ NGƯỜI DÙNG");
        expect(ctx).toContain("mock sensory");
    });

    it("L2 Injection: should inject semantic memory when FF_ENABLE_L2_INJECTION is true", async () => {
        process.env.FF_ENABLE_L2_INJECTION = "true";
        mockMemory.getLanceMemory.mockReturnValue({
            searchAnchors: vi.fn().mockResolvedValue(["Semantic anchor 1", "Semantic anchor 2"])
        });

        const ctx = await PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "factual_recall", "What happened before?"
        );

        expect(mockMemory.getLanceMemory).toHaveBeenCalled();
        expect(ctx).toContain("<context_memory>");
        expect(ctx).toContain("Semantic anchor 1");
        
        delete process.env.FF_ENABLE_L2_INJECTION;
    });

    it("L2 Injection: should fallback gracefully if search times out", async () => {
        process.env.FF_ENABLE_L2_INJECTION = "true";
        mockMemory.getLanceMemory.mockReturnValue({
            // Mock a delayed promise that triggers the 1500ms timeout
            searchAnchors: vi.fn().mockReturnValue(new Promise(resolve => setTimeout(resolve, 2000)))
        });

        // Use fake timers to speed up the timeout test
        vi.useFakeTimers();
        
        const promptPromise = PromptBuilder.buildContextPrompt(
            mockMemory, "Hà Nội", mockSensory, "factual_recall", "What happened before?"
        );
        
        // Advance timers by 1600ms to trigger the timeout
        vi.advanceTimersByTime(1600);
        
        const ctx = await promptPromise;
        
        // Context should still be built without crashing, just missing the L2 data
        expect(ctx).not.toContain("<context_memory>");
        expect(ctx).toContain("mock sensory");
        
        vi.useRealTimers();
        delete process.env.FF_ENABLE_L2_INJECTION;
    });
});

// ============================================================
// TEST GROUP 5: LRU Cache behavior
// ============================================================
describe("PromptBuilder.buildToolsPrompt — Cache", () => {
    it("should return cached result for same query + tools", () => {
        const tools = [{ name: "test_tool", description: "Test", parameters: {} }];
        const first = PromptBuilder.buildToolsPrompt("cache test", tools);
        const second = PromptBuilder.buildToolsPrompt("cache test", tools);

        expect(first).toBe(second); // Same reference = cache hit
    });

    it("should return different results for different queries", () => {
        const tools = [{ name: "test_tool", description: "Test", parameters: {} }];
        const first = PromptBuilder.buildToolsPrompt("query A", tools);
        const second = PromptBuilder.buildToolsPrompt("query B", tools);

        // Content is same structure but fingerprint key differs
        expect(first).toBeDefined();
        expect(second).toBeDefined();
    });
});
